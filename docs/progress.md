# AllocMap 开发进度记录

> 本文档记录每次迭代的详细修改内容，供后续参考。
> 每次迭代结束后由 Doc Agent 自动更新。

---

## 项目初始化（2026-03-26）

### 完成内容
- 创建项目基础结构（Cargo workspace，5 个 crate）
- 编写 CLAUDE.md（项目开发规范 / "宪法"）
- 配置 Multi-Agent 工作流（`.claude/` 目录）
- 编写初始文档框架（README、progress、structure、tutorial、lesson_learned）
- 配置 Docker 开发环境（`docker/Dockerfile`、`docker/Dockerfile.test`、`docker-compose.yml`）
- 配置 GitHub Actions CI（`.github/workflows/ci.yml`）

### 技术决策记录
- 采样机制：双模式（ptrace + LD_PRELOAD），不实现 eBPF
- TUI 框架：ratatui 0.28
- 符号解析：addr2line + gimli + object + rustc-demangle
- 错误处理：anyhow（CLI 层）
- 颜色输出：owo-colors

---

## Phase 1 — Iter 01（2026-03-26）

### 迭代目标

实现 Phase 1 全部核心功能：`allocmap attach`、`allocmap run`、`allocmap snapshot`，以及 ratatui TUI 界面和 LD_PRELOAD 注入库。

### 构建与测试结果

| 检查项 | 结果 |
|--------|------|
| `cargo build` | PASSED |
| `cargo build --release` | PASSED（约 1.5 分钟） |
| `cargo clippy --workspace -- -D warnings` | PASSED（0 warnings） |
| `cargo test --workspace` | PASSED（27 tests，0 failures） |
| `allocmap snapshot` 功能测试 | PASSED（修复后返回真实堆数据） |

### 新增 / 修改的文件

#### allocmap-core（Developer A）

- **`crates/allocmap-core/src/recording.rs`**：完整实现 `.amr` 文件格式的读写
  - `write_to<W: Write>`：写入魔数、版本、JSON header、bincode 帧流、JSON footer
  - `read_from<R: Read>`：读取并校验魔数和版本，按 `frame_count` 读取帧
  - 新增 `RecordingHeader.frame_count: u64` 字段
  - 单元测试：roundtrip、invalid magic、version mismatch（共 3 个测试）

- **`Cargo.toml`（workspace 根）**：
  - 新增 `libc = "0.2"` 到 workspace dependencies
  - `addr2line` 添加 `features = ["object"]`
  - `object` 添加 `features = ["read"]`

#### allocmap-ptrace（DevOps + Developer B）

- **`crates/allocmap-ptrace/src/attach.rs`**：
  - `attach(pid: Pid)`、`detach(pid: Pid)` 独立函数
  - `get_heap_bytes(pid: u32)`：读取 `/proc/PID/status` VmRSS
  - `process_exists(pid: u32)`：检查 `/proc/PID` 是否存在
  - RAII `PtraceAttach` 结构体，Drop 时自动 detach
  - 修复 Errno 到 i32 转换 bug（使用 if/else 代替 match）
  - 4 个单元测试

- **`crates/allocmap-ptrace/src/sampler.rs`**：
  - `PtraceSampler::attach(pid: u32)`（单参数）
  - `sample()` 方法：SIGSTOP → waitpid → 读 backtrace → PTRACE_CONT
  - `sample_interval()` 返回采样间隔
  - 3 个单元测试（config default、interval、attach failure）

- **`crates/allocmap-ptrace/src/backtrace.rs`**：
  - `collect_backtrace(pid, max_frames)`：frame-pointer stack unwinding（x86_64）
  - `BacktraceCapture` 结构体，委托给 `collect_backtrace` + `SymbolResolver`
  - 3 个单元测试

- **`crates/allocmap-ptrace/src/symbols.rs`**：
  - `SymbolResolver`：addr2line + rustc-demangle，含结果缓存
  - 3 个单元测试（construction、caching、fallback）

#### allocmap-preload（DevOps + Developer C）

- **`crates/allocmap-preload/src/bump_alloc.rs`**：
  - mmap 分配的 8MB 内存池，原子 `fetch_add` bump 分配，8 字节对齐
  - `init()`（Once guard）、`alloc(size)`、`contains(ptr)`、`used_bytes()`

- **`crates/allocmap-preload/src/ipc.rs`**：
  - `AllocEvent`（`repr(C)` wire 格式）：type + address + size + timestamp_ms
  - `send_event()`：非阻塞 `try_lock`，socket 设非阻塞模式，事件丢失优于阻塞目标进程
  - `IPC_INIT: Once` 保证幂等初始化

- **`crates/allocmap-preload/src/hooks.rs`**：
  - `malloc`/`free`/`calloc`/`realloc` 的 `#[no_mangle] pub unsafe extern "C"` 钩子
  - 每线程 `Cell<bool>` 重入保护（非进程全局）
  - `dlsym(RTLD_NEXT, ...)` 解析原始函数指针
  - free 钩子递减 `LIVE_BYTES`（修复 H1）
  - realloc bump-arena fallback 使用 `min(old_size, new_size)` 拷贝（修复 H2 UB）

- **`crates/allocmap-preload/src/lib.rs`**：4 个单元测试

#### allocmap-tui（DevOps + Developer D）

- **`crates/allocmap-tui/src/events.rs`**：`AppEvent` enum，`poll_event()` 带 100ms 超时
- **`crates/allocmap-tui/src/app.rs`**：
  - `App` 状态结构体（VecDeque<SampleFrame> ring buffer，容量 500）
  - `DisplayMode::from_str()`，`new_with_mode()`
  - `current_heap_bytes()`、`growth_rate_bytes_per_sec()` 等辅助方法
- **`crates/allocmap-tui/src/timeline.rs`**：Unicode block-character 柱状图，颜色随增速变化
- **`crates/allocmap-tui/src/hotspot.rs`**：top-N 分配热点列表，支持折叠展开
- **`crates/allocmap-tui/src/lib.rs`**：
  - `init_terminal()`、`restore_terminal()`、`install_panic_hook()`
  - `async fn run_tui_loop(app, terminal, rx, duration)`：主事件循环，60fps 渲染

#### allocmap-cli（Developer E）

- **`crates/allocmap-cli/src/util.rs`**：`parse_duration(s)` 工具函数，支持 `30s`/`5m`/`1h`，7 个单元测试

- **`crates/allocmap-cli/src/cmd/attach.rs`**：
  - 读取 `/proc/{pid}/comm` 获取程序名
  - 将整个 ptrace 采样循环（attach + sample + sleep）放在 `spawn_blocking` 内（解决跨线程 ptrace 问题）
  - 支持 `--output`（非交互 JSON）和 `--record`（.amr 录制）模式

- **`crates/allocmap-cli/src/cmd/snapshot.rs`**：
  - 采样循环完全在 `spawn_blocking` 内（修复跨线程 ptrace bug H3）
  - 输出 JSON：pid、sample_count、duration_ms、peak_heap_bytes、avg_heap_bytes、top_sites、frames

- **`crates/allocmap-cli/src/cmd/run.rs`**：
  - 通过 `find_preload_so()` 查找 `liballocmap_preload.so`
  - 创建 Unix socket `/tmp/allocmap-{pid}.sock`
  - 设置 `LD_PRELOAD` 和 `ALLOCMAP_SOCKET_PATH` 环境变量，spawn 子进程
  - 回退到 ptrace 采样（LD_PRELOAD IPC 端对端集成留在 iter02）

- **`crates/allocmap-cli/src/cli.rs`**：CLI help 文本改为英文（修复 L3）

#### 测试目标程序

- **`tests/target_programs/spike_alloc/`**：函数 A 大量分配 → 释放 → 函数 B 大量分配
- **`tests/target_programs/leak_linear/`**：每秒 10MB 线性内存泄漏
- **`tests/target_programs/steady_state/`**：稳定分配释放，验证无误报
- **`tests/target_programs/multithreaded/`**：8 线程并发分配场景

#### Docker 基础设施（DevOps）

- **`docker/Dockerfile`**：`rust:latest`（Debian），含 cargo-watch，构建时约 3 分钟
- 安装 Docker 26.1.5，构建镜像 `allocmap-dev:latest`（1.92 GB）

### 关键 Bug 修复

#### H3（CRITICAL）：跨线程 ptrace 问题

**现象**：`allocmap snapshot` 返回 `sample_count: 0`，frames 为空。

**根因**：Linux ptrace 是线程绑定的，`PtraceSampler::attach()` 在 Tokio 异步线程调用，而 `sample()` 在 `spawn_blocking` 的线程池线程运行，两者为不同 OS 线程，导致 `waitpid` 等 ptrace 调用全部返回 `ESRCH`。

**修复**：将 `PtraceSampler::attach()` 移至 `spawn_blocking` 闭包内部，确保 attach 与所有后续 ptrace 操作在同一 OS 线程执行。

#### H2（HIGH）：realloc bump-arena UB

**现象**：`realloc` bump-arena fallback 中 `copy_nonoverlapping` 使用 new_size 作为拷贝字节数，当 new_size > old_size 时读越界，触发 UB。

**修复**：拷贝字节数改为 `min(old_size, new_size)` 并加 4096 字节上限。

#### H1（HIGH）：free 未递减 LIVE_BYTES

**现象**：`LIVE_BYTES` 计数器只增不减，LD_PRELOAD 模式下 live_bytes 读数持续增大。

**修复**：在 `free()` 钩子中调用 `LIVE_BYTES.fetch_sub(...)`。注：ptrace 模式的 live_heap_bytes 来自 `/proc/PID/status VmRSS`，不受此影响。

### 已知限制（待 iter02 改进）

- allocmap-tui 和 allocmap-cli 测试覆盖率为 0（无单元测试）
- `tests/integration/` 集成测试目录不存在
- `run` 命令缺少 `--mode` 选项（与 attach 不一致）
- LD_PRELOAD IPC 端对端集成未完成（run 命令回退到 ptrace 采样）
- Docker 镜像未预装 clippy（需在容器内 `rustup component add clippy`）
- 火焰图视图为占位符（显示 "flamegraph not yet implemented"）

---

## Phase 1 — Iter 02（2026-03-26）

### 目标
修复 Iter 01 遗留问题，达到 Phase 1 完整验收标准。

### 本次修改

#### 新增测试
- `allocmap-tui/src/app.rs`：新增 13 个单元测试（App 状态、push_frame、ring buffer、按键事件、DisplayMode）
- `allocmap-cli/src/cmd/attach.rs`：新增 3 个测试（PID 验证、DisplayMode 解析）
- `allocmap-cli/src/cmd/snapshot.rs`：新增 3 个测试（PID 验证、duration 解析集成）
- `allocmap-cli/tests/integration_tests.rs`：新增 5 个集成测试（snapshot 非存在PID、无效duration、--help英文）
- `allocmap-cli/src/util.rs`：扩展至 9 个 duration 解析测试

#### Bug 修复
- `cmd/attach.rs`：`spawn_blocking` 返回的 `JoinHandle` 命名为 `_sampling_handle`（避免 panic 被丢弃）
- `cmd/snapshot.rs`：`sampling_handle.await` 正确等待采样线程完成
- `cmd/run.rs`：新增 `--mode` 选项（与 attach 命令保持一致），修复 JoinHandle

#### 功能增强
- `cmd/run.rs`：新增 `--mode timeline|hotspot|flamegraph` 选项

#### DevOps
- `docker/Dockerfile`：新增 `rustup component add clippy rustfmt`，确保容器内 clippy 预装

### 测试结果
- 总测试数：55（较 iter01 增加 28 个）
- allocmap-tui：14 tests（原 0）
- allocmap-cli：21 tests（原 0）
- 所有测试通过

### 验收状态
- **Reviewer**: PASSED（0 clippy warnings，所有修改已验证）
- **Tester**: PASSED（55 tests, snapshot 146 frames, peak 2.1MB）
- **Phase 1 状态：COMPLETED ✅**

---

## Phase 2 — Iter 01（2026-03-26）

### 迭代目标

实现 Phase 2 核心功能：`allocmap replay`、`allocmap diff`、macOS 平台支持基础结构，以及多线程追踪的初步框架。

### 构建与测试结果

| 检查项 | 结果 |
|--------|------|
| `cargo build --release` | PASSED |
| `cargo clippy --workspace -- -D warnings` | PASSED（0 warnings） |
| `cargo test --workspace` | PASSED（64 tests，0 failures） |
| Reviewer | PASSED |
| Tester | PASSED |

### 新增 / 修改的文件

#### allocmap-cli（新增子命令）

- **`crates/allocmap-cli/src/cmd/replay.rs`**（新文件）：
  - `allocmap replay <file.amr>` 命令实现
  - 支持选项：`--from <offset_ms>`、`--to <offset_ms>`、`--speed <倍速>`
  - 读取 `.amr` 文件，按时间偏移过滤帧，以正确节奏向 TUI 送帧
  - 键盘支持：`Space` = 暂停/继续，`+`/`-` = 加速/减速

- **`crates/allocmap-cli/src/cmd/diff.rs`**（新文件）：
  - `allocmap diff <baseline.amr> <current.amr>` 命令实现
  - 对比两个录制文件，按绝对字节差降序排列
  - `--min-change-pct <pct>` 过滤选项（只显示变化超过阈值的热点）
  - 彩色输出：变化 ≥ 10% 用黄色标注，≥ 50% 用红色标注

- **`crates/allocmap-cli/src/cmd/run.rs`**（修改）：
  - 新增 macOS 平台支持：`#[cfg(target_os = "macos")]` 下使用 `DYLD_INSERT_LIBRARIES` 替代 `LD_PRELOAD`
  - `#[cfg(target_os = "linux")]` 和 `#[cfg(target_os = "macos")]` 平台条件编译隔离

#### allocmap-ptrace（新增 macOS 支持 + 多线程框架）

- **`crates/allocmap-ptrace/src/macos_sampler.rs`**（新文件）：
  - macOS 平台采样器存根（`#[cfg(target_os = "macos")]`）
  - 当前实现：使用 `ps -o rss=` 读取进程内存大小
  - `task_for_pid` 完整实现推迟至 iter02
  - `top_sites` 在 macOS 下当前返回空列表

- **`crates/allocmap-ptrace/src/attach.rs`**（修改）：
  - 新增 `list_threads(pid: u32) -> Vec<u32>` 函数
  - 读取 `/proc/{pid}/task/` 目录枚举所有线程 ID
  - 注：当前 `list_threads` 结果仅收集，尚未在 TUI 中分线程显示

#### allocmap-tui（回放状态支持）

- **`crates/allocmap-tui/src/app.rs`**（修改）：
  - `App` 结构体新增字段：`is_replay: bool`、`replay_speed: f64`、`replay_paused: bool`
  - 支持在回放模式下在 TUI 标题栏显示回放速度和暂停状态

### 关键设计决策

#### macOS 平台隔离策略

采用 Rust `#[cfg(target_os)]` 条件编译在单一代码库中支持双平台，而非维护两套代码：
- Linux 路径：`LD_PRELOAD` + `ptrace(2)` + `/proc/PID/status`
- macOS 路径：`DYLD_INSERT_LIBRARIES` + `ps -o rss=` 存根（`task_for_pid` 待 iter02）

这确保 `cargo build` 在两个平台上均可通过，且无需 feature flags。

#### replay 帧定时设计

replay 命令读取相邻帧的 `timestamp_ms` 差值来计算等待时间，并除以 `--speed` 倍数。这确保：
1. 录制的真实时间节奏得以还原
2. 变速播放的计算简单且精确
3. 采样间隔不均匀（如采样丢帧）时依然正确

### 已知限制（待 iter02 改进）

- `replay_paused` 标志已更新 App 状态，但暂停逻辑尚未传播到 feeder 任务（Space 键显示暂停但不中断帧推送）
- macOS `top_sites` 永远为空（`ps` 方式仅能获取 RSS 总量，无法区分分配热点）
- `list_threads()` 结果当前被丢弃，未在 TUI 中展示每线程独立数据
- `PTRACE_O_TRACECLONE` 自动追踪新线程尚未实现

### 验收状态

- **Reviewer**: PASSED（0 clippy warnings）
- **Tester**: PASSED（64 tests，0 failures）
- **Phase 2 Iter 01 状态：COMPLETED ✅，进入 Iter 02**

---

<!-- 后续迭代记录由 Doc Agent 在每次迭代后追加 -->
