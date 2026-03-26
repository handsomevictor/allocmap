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

<!-- 后续迭代记录由 Doc Agent 在每次迭代后追加 -->
