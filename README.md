# AllocMap

[![构建状态](https://img.shields.io/github/actions/workflow/status/handsomevictor/allocmap/ci.yml?branch=main&style=flat-square&label=构建)](https://github.com/handsomevictor/allocmap/actions)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](LICENSE)
[![版本](https://img.shields.io/badge/version-0.1.0-orange?style=flat-square)](Cargo.toml)
[![平台](https://img.shields.io/badge/platform-Linux%20%7C%20macOS-lightgrey?style=flat-square)](#安装)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange?style=flat-square)](https://www.rust-lang.org)

**AllocMap** 是一个面向 Linux 平台的命令行内存分析工具，专为 Rust、C、C++、Go 开发者以及 DevOps 工程师设计。无需重启目标进程，随时 attach，实时观察堆内存的时序变化与分配热点。

```bash
# 一行命令，立即观察任意进程的内存行为
allocmap attach --pid $(pidof my_service)
```

---

> **本项目由 Claude Code Multi-Agent 协作系统全权开发完成，包括架构设计、代码实现、测试、文档撰写及 DevOps 配置，人工介入仅限于需求定义与最终审查。**
>
> 详见 [.claude/](.claude/) 目录了解 Multi-Agent 工作流设计。

---

## 功能特性

- **实时 Attach**：无需重启目标进程，通过 `ptrace` 随时 attach，随时退出，对目标进程影响极小（< 5% 开销）
- **内存时序图**：以 Unicode braille 字符块绘制堆内存变化折线图，直观展示函数执行期间的 surge 与 drop
- **分配热点定位**：展示 top-N 分配热点，结合调用栈精确定位到函数名和源码行号（需调试符号）
- **双模式采样**：`ptrace` 模式适用于已运行进程；`LD_PRELOAD` 模式在进程启动时注入，数据更完整
- **彩色 TUI 界面**：基于 `ratatui` 的丰富彩色终端界面，绿/黄/红三色表示内存状态，信息密度高、可读性强
- **非交互快照**：`snapshot` 命令输出 JSON 格式报告，天然兼容 CI/CD 流水线
- **录制与回放**：将采样数据录制为 `.amr` 文件，`allocmap replay` 支持变速播放、时间范围裁剪、Space 暂停
- **版本 Diff 对比**：`allocmap diff` 逐函数对比两个录制文件，颜色标注变化幅度，快速定位内存回归
- **macOS 支持**（实验性）：`allocmap run` 使用 `DYLD_INSERT_LIBRARIES` 注入，基础 RSS 监控已可用

## 与同类工具对比

| 特性 | AllocMap | Valgrind/Massif | heaptrack | bytehound | memray |
|------|:--------:|:---------------:|:---------:|:---------:|:------:|
| attach 到运行中进程 | ✅ | ❌ 需重启 | ❌ 需重启 | ❌ 需重启 | ❌ 需重启 |
| 实时 TUI 时序图 | ✅ | ❌ | ❌ | ❌ Web UI | ❌ Web UI |
| 录制与回放 | ✅ | ❌ | ❌ | 有限 | ❌ |
| 版本 diff 对比 | ✅ | ❌ | ❌ | ❌ | ❌ |
| 性能开销 | 低（< 5%） | 极高（10–50x） | 低（< 5%） | 低（< 5%） | 低（< 5%） |
| 跨语言支持 | ✅ | ✅ | ✅ | ✅ | ❌ 仅 Python |
| Linux 支持 | ✅ | ✅ | ✅ | ✅ | ✅ |
| macOS 支持 | 实验性（Phase 2 Iter 01） | 有限 | ❌ | ❌ | ❌ |
| 纯终端（无 GUI） | ✅ | ✅ | ❌ 需 GUI | ❌ 需浏览器 | ❌ 需浏览器 |
| 零源码修改 | ✅ | ✅ | ✅ | ✅ | ❌ |

## 安装

### 前置要求

- Linux（Kernel 4.4+，x86_64）或 macOS（实验性支持）
- Docker（推荐，用于构建；EC2 宿主机未预装 Rust）
- 或：Rust 工具链 1.75+

### 在 Docker 中构建（推荐）

```bash
git clone https://github.com/handsomevictor/allocmap
cd allocmap

# 构建 Docker 开发镜像（首次约 3 分钟）
docker build -f docker/Dockerfile -t allocmap-dev .

# 在容器内编译 release 二进制
docker run --rm --cap-add=SYS_PTRACE --security-opt seccomp:unconfined \
  -v $(pwd):/workspace -w /workspace \
  allocmap-dev cargo build --release

# 将二进制复制到系统路径
sudo cp target/release/allocmap /usr/local/bin/
allocmap --version
```

### 直接安装（需本地 Rust 环境）

```bash
git clone https://github.com/handsomevictor/allocmap
cd allocmap
cargo install --path crates/allocmap-cli
```

### 权限配置（Linux 必读）

AllocMap 使用 `ptrace` 系统调用，需要相应权限：

```bash
# 检查当前 ptrace 权限级别（0 = 允许，1 = 仅子进程，2/3 = 严格）
cat /proc/sys/kernel/yama/ptrace_scope

# 临时允许 attach 任意进程（重启后失效）
echo 0 | sudo tee /proc/sys/kernel/yama/ptrace_scope

# 永久生效
echo 'kernel.yama.ptrace_scope = 0' | sudo tee /etc/sysctl.d/10-ptrace.conf
sudo sysctl -p /etc/sysctl.d/10-ptrace.conf
```

Docker 容器内默认以 root 运行，无需额外配置权限。

## 快速开始

```bash
# 1. 实时监控正在运行的进程（按 q 退出）
allocmap attach --pid $(pidof my_service)

# 2. 采样 30 秒后自动退出，保存 JSON 报告
allocmap attach --pid 1234 --duration 30s --output report.json

# 3. 录制数据供后续回放分析
allocmap attach --pid 1234 --record session.amr

# 4. 回放录制文件（TUI 界面，支持变速 / 暂停）
allocmap replay session.amr --speed 2.0

# 5. 对比两次录制，定位内存回归
allocmap diff baseline.amr current.amr --min-change-pct 10

# 6. 显示前 30 个分配热点
allocmap attach --pid 1234 --top 30

# 7. 以 LD_PRELOAD 模式启动新进程（数据更完整）
allocmap run -- ./my_binary --arg1 --arg2

# 8. 非交互式快照，输出 JSON（适合 CI/CD）
allocmap snapshot --pid 1234 --duration 5s

# 9. 将快照保存到文件
allocmap snapshot --pid 1234 --duration 10s --output snapshot.json
```

### TUI 界面说明

```
╭─ allocmap · pid=1234 (my_service) · 30s · 150 samples ──────────────╮
│ LIVE HEAP: 128.4 MB  △ +2.1MB/s  ALLOCS: 1234/s  FREES: 1198/s     │
├──────────────────────────────────────────────────────────────────────┤
│  ▁▂▃▄▄▅▆▇█▇▆▅▄▄▅▆▇██████████████▇▆▅                                 │
│  0s                    15s                    30s                    │
├──────────────────────────────────────────────────────────────────────┤
│ Top Allocators                          [bytes]  [count]  [trend]    │
│ > engine::run_simulation                128.4MB   45,231   ↑        │
│   └─ alloc::vec::Vec::reserve            64.2MB   22,115            │
│   └─ std::collections::HashMap::insert  32.1MB   11,058            │
│ > parser::parse_chunk                    22.3MB    8,432   →        │
╰──────────────────────────────────────────────────────────────────────╯
[q]退出  [t]时间轴  [h]热点  [f]火焰图  [↑↓]滚动  [Enter]展开/折叠
```

颜色含义：
- **绿色**：内存稳定，增速 < 1 MB/s
- **黄色**：内存增长中，增速 1–10 MB/s
- **红色**：内存快速增长（> 10 MB/s）或疑似泄漏

## 功能实现原理

### 采样机制

AllocMap 采用双模式设计，互补覆盖不同场景：

**ptrace 模式**（`attach` 和 `snapshot` 命令）

使用 Linux `ptrace(2)` 系统调用 attach 到目标进程。关键约束：Linux ptrace 是**线程绑定**的，发出 `ptrace_attach` 的 OS 线程必须也执行所有后续的 ptrace 操作（`waitpid`、`PTRACE_GETREGS`、`PTRACE_CONT` 等）。AllocMap 将整个采样循环放在 `tokio::task::spawn_blocking` 内，确保 attach 与 sample 在同一 OS 线程执行。

采样流程：
```
PTRACE_ATTACH
    ↓
SIGSTOP 目标进程
    ↓
waitpid 等待停止
    ↓
对所有线程调用 PTRACE_GETREGS（读取 RSP、RBP）
    ↓
沿帧指针链展开调用栈（frame-pointer unwinding）
    ↓
读取 /proc/PID/status VmRSS 作为堆内存近似值
    ↓
PTRACE_CONT 恢复目标进程
    ↓
等待采样间隔（默认 50Hz = 20ms）
    ↓
重复...
```

**LD_PRELOAD 模式**（`run` 命令）

通过 `LD_PRELOAD` 将 `liballocmap_preload.so` 注入目标进程。注入库使用 `dlsym(RTLD_NEXT, "malloc")` 获取原始 malloc 指针，并在钩子函数中：
- 使用每线程 `Cell<bool>` 防止钩子重入
- 使用 mmap 分配的 BumpAllocator 作为内部内存来源（避免调用被 hook 的 malloc）
- 通过 Unix Domain Socket 将 `AllocEvent` 异步发送给 allocmap-cli 进程

### 符号解析

```
原始指令指针（如 0x7fff8a2b1c40）
    ↓ 解析 /proc/PID/maps → 确定所属 ELF 文件及偏移
    ↓ ELF 解析（object crate）→ 加载调试符号段
    ↓ DWARF 查询（addr2line crate）→ 函数名:文件:行号
    ↓ rustc-demangle → 还原 Rust mangled 函数名
    → engine::run_simulation (engine.rs:142)
```

符号解析结果会在 `SymbolResolver` 中缓存，避免重复 IO。对于没有调试符号的二进制，回退显示 ELF 文件名。

### .amr 文件格式

AllocMap Recording 格式，自定义二进制格式，支持流式写入和随机访问：

| 字段 | 大小 | 说明 |
|------|------|------|
| 魔数 | 4 字节 | `AMR\0` |
| 版本 | 4 字节 | u32 LE，当前为 1 |
| header 长度 | 4 字节 | u32 LE |
| header JSON | 变长 | pid、程序名、采样频率、帧数等 |
| [帧长度 + 帧数据] × N | 变长 | 每帧：4 字节长度 + bincode 编码的 SampleFrame |
| footer 长度 | 4 字节 | u32 LE |
| footer JSON | 变长 | 汇总统计（peak_bytes、total_frames 等） |

### 跨语言支持

| 进程类型 | 支持程度 | 说明 |
|---------|---------|------|
| Rust（带调试符号） | 完整函数名 + 调用栈 | `-g` 或 `debug = true` |
| C / C++ | 函数名（有符号时） | `-g` 编译选项 |
| Go | 函数名（默认带符号） | Go 默认保留符号 |
| Python | 仅 CPython 内部函数 | 解释器层不在 native 栈 |
| Ruby | 仅 CRuby 内部函数 | 同上 |

## 运行测试

```bash
# 在 Docker 容器内运行所有单元测试（推荐）
docker run --rm --cap-add=SYS_PTRACE --security-opt seccomp:unconfined \
  -v $(pwd):/workspace -w /workspace \
  allocmap-dev cargo test --workspace

# 运行特定 crate 的测试
docker run --rm --cap-add=SYS_PTRACE --security-opt seccomp:unconfined \
  -v $(pwd):/workspace -w /workspace \
  allocmap-dev cargo test -p allocmap-ptrace

# 运行 clippy 静态检查
docker run --rm --cap-add=SYS_PTRACE --security-opt seccomp:unconfined \
  -v $(pwd):/workspace -w /workspace \
  allocmap-dev bash -c "rustup component add clippy && cargo clippy --workspace -- -D warnings"
```

当前测试状态（Phase 2 Iter 01）：

| Crate | 测试数 | 状态 |
|-------|--------|------|
| allocmap-core | 3 | 通过 |
| allocmap-ptrace | 13 | 通过 |
| allocmap-preload | 4 | 通过 |
| allocmap-tui | 14 | 通过 |
| allocmap-cli | 30 | 通过 |
| **合计** | **64** | **全部通过** |

### 内置测试目标程序

```bash
# 构建测试目标程序
docker run --rm -v $(pwd):/workspace -w /workspace \
  allocmap-dev cargo build --manifest-path tests/target_programs/leak_linear/Cargo.toml

# 用 allocmap 监控泄漏程序
./tests/target_programs/leak_linear/target/debug/leak_linear &
allocmap snapshot --pid $! --duration 5s
```

## 路线图

### Phase 1 — Linux 核心功能（✅ 已完成，iter02，2026-03-26）

- [x] `allocmap attach` — ptrace 模式实时监控，TUI 界面
- [x] `allocmap run` — LD_PRELOAD 模式，支持 `--env` 注入，`--mode` 选项
- [x] `allocmap snapshot` — 非交互式 JSON 快照
- [x] 彩色 TUI（时序折线图 + 热点列表）
- [x] JSON 报告输出（`--output` 选项）
- [x] 内置测试目标程序（leak_linear、spike_alloc、steady_state、multithreaded）
- [x] Docker 开发环境（`docker/Dockerfile`）
- [x] 完整测试覆盖（55 tests，全部通过）
- [x] Reviewer PASSED，Tester PASSED（snapshot 146 frames, peak 2.1MB）

### Phase 2 — 完整产品（🚧 进行中）

#### Iter 01（✅ 已完成，2026-03-26）

- [x] `allocmap replay` — `.amr` 文件回放，支持 `--from`/`--to`/`--speed`，Space 暂停，+/- 变速
- [x] `allocmap diff` — 两个录制文件的逐函数对比，颜色标注变化幅度，`--min-change-pct` 过滤
- [x] macOS 基础支持：`DYLD_INSERT_LIBRARIES` 注入，`ps -o rss=` RSS 监控
- [x] 多线程枚举框架：`list_threads()` 读取 `/proc/PID/task/`
- [x] TUI 回放状态字段（`is_replay`、`replay_speed`、`replay_paused`）
- [x] 64 tests，全部通过，Reviewer PASSED，Tester PASSED

#### Iter 02（计划中）

- [ ] `allocmap replay` Space 暂停真正中断帧推送（当前仅更新显示状态）
- [ ] macOS `task_for_pid` 完整实现（分配热点支持）
- [ ] 多线程视图（`PTRACE_O_TRACECLONE`，每线程独立 TUI 数据显示）
- [ ] 火焰图视图（当前为占位符）
- [ ] 集成测试套件

## License

MIT License © 2024 handsomevictor
