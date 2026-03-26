# AllocMap 使用教程

> 本文档涵盖 AllocMap 的所有已实现功能，逐一介绍使用方法、参数说明、预期输出和常见问题。
> 每次迭代后由 Doc Agent 更新，确保与实际功能完全一致。
>
> **当前版本**：Phase 2 Iter 03（2026-03-26）— Phase 2 COMPLETED ✅

---

## 目录

1. [安装](#安装)
2. [权限配置](#权限配置linux-必读)
3. [allocmap attach](#allocmap-attach)
4. [allocmap snapshot](#allocmap-snapshot)
5. [allocmap run](#allocmap-run)
6. [allocmap replay](#allocmap-replay)
7. [allocmap diff](#allocmap-diff)
8. [使用测试目标程序](#使用测试目标程序)
9. [录制到 .amr 文件](#录制到-amr-文件)
10. [TUI 界面详解](#tui-界面详解)
11. [常见问题](#常见问题)

---

## 安装

AllocMap 需要在 Docker 容器内构建（EC2 宿主机未安装 Rust 工具链）。

```bash
git clone https://github.com/handsomevictor/allocmap
cd allocmap

# 构建 Docker 开发镜像（首次约 3 分钟，需要下载 rust:latest 基础镜像）
docker build -f docker/Dockerfile -t allocmap-dev .

# 在容器内编译 release 二进制（约 1.5 分钟）
docker run --rm --cap-add=SYS_PTRACE --security-opt seccomp:unconfined \
  -v $(pwd):/workspace -w /workspace \
  allocmap-dev cargo build --release

# 安装到系统路径
sudo cp target/release/allocmap /usr/local/bin/

# 验证安装
allocmap --version
# 预期输出：allocmap 0.1.0
```

如果本地已安装 Rust（1.75+），也可以直接构建：

```bash
cd allocmap
cargo build --release
./target/release/allocmap --version
```

---

## 权限配置（Linux 必读）

AllocMap 使用 `ptrace` 系统调用，普通用户可能遇到权限问题：

```bash
# 检查当前 ptrace 权限级别
cat /proc/sys/kernel/yama/ptrace_scope
# 0 = 允许所有用户 attach 任意进程
# 1 = 只能 attach 到子进程（需 sudo 才能 attach 任意进程）
# 2/3 = 更严格的限制

# 临时允许（重启后恢复默认）
echo 0 | sudo tee /proc/sys/kernel/yama/ptrace_scope

# 永久生效
echo 'kernel.yama.ptrace_scope = 0' | sudo tee /etc/sysctl.d/10-ptrace.conf
sudo sysctl -p /etc/sysctl.d/10-ptrace.conf
```

**Docker 容器内**无需额外配置，容器以 root 运行，`--cap-add=SYS_PTRACE` 已包含在标准启动命令中。

---

## allocmap attach

**功能**：attach 到正在运行的进程，打开交互式 TUI 界面实时显示内存使用情况。

### 基本用法

```bash
# 监控 PID 1234，按 q 退出
allocmap attach --pid 1234

# 指定采样时长，超时后自动退出（支持 s/m/h 后缀）
allocmap attach --pid 1234 --duration 30s
allocmap attach --pid 1234 --duration 5m

# 显示前 30 个分配热点（默认 20）
allocmap attach --pid 1234 --top 30

# 切换显示模式（timeline=时序图，hotspot=热点列表，flamegraph=火焰图占位符）
allocmap attach --pid 1234 --mode hotspot

# 非交互式模式：采样后输出 JSON 报告（不打开 TUI）
allocmap attach --pid 1234 --duration 10s --output report.json

# 输出到 stdout
allocmap attach --pid 1234 --duration 5s --output -

# 同时录制 .amr 文件（用于未来回放，Phase 2 功能）
allocmap attach --pid 1234 --duration 30s --record session.amr

# 调整采样频率（默认 50Hz）
allocmap attach --pid 1234 --sample-rate 100
```

### 完整参数列表

```
allocmap attach --pid <PID> [OPTIONS]

Options:
  --pid <PID>              Target process PID [required]
  --duration <DURATION>    Sampling duration (e.g. 30s, 5m, 1h) [default: unlimited]
  --top <N>                Number of top allocation sites to display [default: 20]
  --mode <MODE>            Display mode: timeline | hotspot | flamegraph [default: timeline]
  --output <FILE>          Output JSON report to file (use - for stdout, disables TUI)
  --record <FILE>          Record sampling data to .amr file
  --sample-rate <HZ>       Sampling rate in Hz [default: 50]
```

### 预期 TUI 输出

```
╭─ allocmap · pid=1234 (leak_linear) · 10s · 500 samples ─────────────╮
│ LIVE HEAP: 105.4 MB  △ +10.5MB/s  ALLOCS: 2048/s  FREES: 0/s       │
├──────────────────────────────────────────────────────────────────────┤
│  ▁▂▃▄▅▆▇█████████████████████████████████████████████████████████   │
│  0s                             5s                              10s  │
├──────────────────────────────────────────────────────────────────────┤
│ Top Allocators                          [bytes]  [count]  [trend]    │
│ > leak_linear::main                     105.4MB       1    ↑↑       │
╰──────────────────────────────────────────────────────────────────────╯
[q]退出  [t]时间轴  [h]热点  [f]火焰图  [↑↓]滚动  [Enter]展开/折叠
```

### 非交互模式 JSON 输出格式

```bash
allocmap attach --pid 1234 --duration 5s --output -
```

```json
{
  "pid": 1234,
  "program": "my_service",
  "duration_ms": 5000,
  "sample_count": 250,
  "peak_heap_bytes": 134217728,
  "avg_heap_bytes": 98304000,
  "top_sites": [
    {
      "function": "engine::allocate_buffer",
      "bytes": 67108864,
      "count": 1024,
      "frames": ["engine::allocate_buffer", "engine::run", "main"]
    }
  ],
  "frames": [...]
}
```

### 错误情况

```bash
# 进程不存在
allocmap attach --pid 99999
# Error: Process 99999 not found. Make sure the PID is correct and the process is running.

# 权限不足
allocmap attach --pid 1
# Error: Permission denied attaching to PID 1. Try running as root or set /proc/sys/kernel/yama/ptrace_scope=0.

# 无效时长格式
allocmap attach --pid 1234 --duration invalid
# Error: Invalid duration 'invalid': expected format like 30s, 5m, 1h
```

---

## allocmap snapshot

**功能**：非交互式快照，一次性采样指定时长后输出 JSON 报告，天然适合 CI/CD 集成。

### 基本用法

```bash
# 采样 5 秒（默认），输出到 stdout
allocmap snapshot --pid 1234

# 指定采样时长
allocmap snapshot --pid 1234 --duration 10s

# 保存到文件
allocmap snapshot --pid 1234 --duration 10s --output snapshot.json

# 显示前 10 个热点（默认 20）
allocmap snapshot --pid 1234 --top 10
```

### 完整参数列表

```
allocmap snapshot --pid <PID> [OPTIONS]

Options:
  --pid <PID>              Target process PID [required]
  --duration <DURATION>    Sampling duration [default: 5s]
  --output <FILE>          Output file path (default: stdout)
  --top <N>                Number of top allocation sites [default: 20]
```

### 预期 JSON 输出

```bash
allocmap snapshot --pid $(pidof leak_linear) --duration 5s
```

```json
{
  "pid": 42,
  "duration_ms": 5000,
  "sample_count": 248,
  "peak_heap_bytes": 52428800,
  "avg_heap_bytes": 26214400,
  "top_sites": [
    {
      "function": "leak_linear::main",
      "bytes": 52428800,
      "count": 1,
      "trend": "increasing"
    }
  ],
  "frames": [
    {
      "timestamp_ms": 1000,
      "live_heap_bytes": 10485760,
      "alloc_rate": 10485760.0,
      "free_rate": 0.0,
      "top_sites": [...]
    }
  ]
}
```

### 在 CI/CD 中使用

```bash
#!/bin/bash
# CI 内存回归检测示例

# 启动待测服务
./my_service &
SERVICE_PID=$!
sleep 2  # 等待服务启动

# 采样 30 秒
allocmap snapshot --pid $SERVICE_PID --duration 30s --output /tmp/memory_snapshot.json

# 检查峰值内存是否超过阈值（使用 jq）
PEAK_MB=$(jq '.peak_heap_bytes / 1048576' /tmp/memory_snapshot.json)
if (( $(echo "$PEAK_MB > 512" | bc -l) )); then
  echo "FAIL: Peak memory ${PEAK_MB}MB exceeds 512MB threshold"
  exit 1
fi
echo "PASS: Peak memory ${PEAK_MB}MB is within threshold"
```

---

## allocmap run

**功能**：以 LD_PRELOAD 模式启动新进程，注入 `liballocmap_preload.so`，在进程启动时即开始监控。

> **注意**：Phase 1 Iter 01 中，LD_PRELOAD IPC 端对端集成尚未完成，`run` 命令会回退到 ptrace 采样作为 TUI 数据源。LD_PRELOAD 注入模块的钩子已正确实现，完整集成计划在 iter02 完成。

### 基本用法

```bash
# 以 LD_PRELOAD 模式启动程序
allocmap run -- ./my_binary

# 传递命令行参数
allocmap run -- ./my_binary --config config.yaml input.txt

# 注入额外环境变量
allocmap run --env LOG_LEVEL=debug --env DEBUG=1 -- ./my_binary

# 指定运行时长
allocmap run --duration 60s -- ./my_binary

# 保存 JSON 报告
allocmap run --output report.json -- ./my_binary

# 录制 .amr 文件
allocmap run --record session.amr -- ./my_binary
```

### 完整参数列表

```
allocmap run [OPTIONS] -- <COMMAND> [ARGS]...

Options:
  --env <KEY=VALUE>        Inject environment variable (repeatable)
  --top <N>                Number of top allocation sites [default: 20]
  --duration <DURATION>    Maximum run duration [default: unlimited]
  --output <FILE>          Output JSON report (use - for stdout, disables TUI)
  --record <FILE>          Record sampling data to .amr file
```

### 工作原理

1. AllocMap 在 `/tmp/allocmap-{pid}.sock` 创建 Unix Domain Socket
2. 以 `LD_PRELOAD=/path/to/liballocmap_preload.so` 和 `ALLOCMAP_SOCKET_PATH=/tmp/allocmap-{pid}.sock` 启动目标程序
3. 注入库的 `malloc`/`free` 钩子将 `AllocEvent` 通过 socket 发送给 AllocMap
4. AllocMap 同时使用 ptrace 读取堆大小用于 TUI 显示（iter01 回退路径）
5. 目标程序退出时清理 socket 文件

### 查找 .so 文件

`run` 命令通过 `find_preload_so()` 按以下顺序查找 `liballocmap_preload.so`：

1. 与 `allocmap` 可执行文件同目录
2. `../lib/` 相对路径
3. `target/debug/liballocmap_preload.so`（开发模式）
4. `target/release/liballocmap_preload.so`（release 模式）

如果找不到 .so 文件，命令会输出错误信息并退出。

---

## allocmap replay

**功能**：回放录制的 `.amr` 文件，还原当时的 TUI 界面，支持变速播放和时间范围裁剪。

> **注意**：回放功能为 Phase 2 新增。需先通过 `allocmap attach --record` 或 `allocmap run --record` 录制 `.amr` 文件。

### 基本用法

```bash
# 以正常速度回放录制文件
allocmap replay session.amr

# 以 2 倍速回放
allocmap replay session.amr --speed 2.0

# 以 0.5 倍速慢放（方便观察细节）
allocmap replay session.amr --speed 0.5

# 只回放录制的前 30 秒
allocmap replay session.amr --to 30000

# 跳过前 10 秒，从第 10 秒开始回放
allocmap replay session.amr --from 10000

# 只回放第 10 秒到第 40 秒的片段
allocmap replay session.amr --from 10000 --to 40000
```

### 完整参数列表

```
allocmap replay <FILE> [OPTIONS]

Arguments:
  <FILE>               .amr 录制文件路径 [required]

Options:
  --from <MS>          从指定时间偏移（毫秒）开始回放 [default: 0]
  --to <MS>            在指定时间偏移（毫秒）停止回放 [default: 文件末尾]
  --speed <MULTIPLIER> 回放倍速（1.0=原速，2.0=2倍速，0.5=半速）[default: 1.0]
```

### 回放时的键盘快捷键

| 键 | 功能 |
|----|------|
| `Space` | 暂停 / 继续回放（真正中断帧流，Phase 2 Iter 02 修复） |
| `g` | 跳转到录制开头（第一帧） |
| `G` | 跳转到录制结尾（最后一帧） |
| `+` | 加速（倍速 +0.5） |
| `-` | 减速（倍速 -0.5，最低 0.1） |
| `q` / `Ctrl+C` | 退出回放 |
| `t` | 切换到时序图视图 |
| `h` | 切换到热点列表视图 |
| `↑` / `↓` | 在热点列表中滚动 |
| `Enter` | 展开 / 折叠热点调用栈 |

### TUI 标题栏回放状态示例

```
╭─ allocmap · REPLAY · session.amr · 1.0x · 45s / 120s ──────────────╮
│ LIVE HEAP: 85.2 MB  △ +8.3MB/s  ALLOCS: 1502/s  FREES: 210/s       │
...
```

- 标题栏显示 `REPLAY` 标识，以及当前回放速度（如 `1.0x`）
- 暂停时显示 `[PAUSED]` 标识

### 完整工作流示例

```bash
# 第一步：对目标程序进行 60 秒录制
./tests/target_programs/leak_linear/target/debug/leak_linear &
LEAK_PID=$!
allocmap attach --pid $LEAK_PID --duration 60s --record leak_session.amr
kill $LEAK_PID

# 第二步：回放录制，以 2 倍速快速浏览
allocmap replay leak_session.amr --speed 2.0

# 第三步：聚焦分析第 20-40 秒的内存激增片段，用半速仔细观察
allocmap replay leak_session.amr --from 20000 --to 40000 --speed 0.5
```

### 已知限制

- macOS `task_for_pid` 完整实现尚未完成（当前 `top_sites` 在 macOS 返回空列表，基础 RSS 监控可用）
- 火焰图视图当前为占位符（显示提示文字，待后续版本实现）

---

## allocmap diff

**功能**：对比两个 `.amr` 录制文件，逐函数展示内存分配的变化量，快速定位性能回归或内存使用变化。

> **注意**：diff 功能为 Phase 2 新增，适合对比"优化前"与"优化后"的录制结果，也适合 CI/CD 集成用于检测回归。

### 基本用法

```bash
# 对比 baseline 和 current 两次录制
allocmap diff baseline.amr current.amr

# 只显示变化超过 20% 的函数
allocmap diff baseline.amr current.amr --min-change-pct 20

# 将结果保存为文本文件
allocmap diff baseline.amr current.amr > diff_report.txt
```

### 完整参数列表

```
allocmap diff <BASELINE> <CURRENT> [OPTIONS]

Arguments:
  <BASELINE>           基准录制文件（旧版本）[required]
  <CURRENT>            当前录制文件（新版本）[required]

Options:
  --min-change-pct <PCT>   只显示变化幅度 ≥ PCT% 的函数 [default: 0]
```

### 预期输出格式

```bash
allocmap diff v1_baseline.amr v2_current.amr
```

```
AllocMap Diff Report
====================
Baseline : v1_baseline.amr
Current  : v2_current.amr

Function                          Baseline     Current      Delta       Change
──────────────────────────────────────────────────────────────────────────────
engine::process_batch             32.0 MB      89.4 MB      +57.4 MB    +179%   ← 红色
parser::parse_header              12.1 MB      18.6 MB      +6.5 MB     +54%    ← 红色
alloc::vec::Vec::reserve          44.2 MB      48.8 MB      +4.6 MB     +10%    ← 黄色
std::collections::HashMap::new    8.3 MB       8.1 MB       -0.2 MB     -2%
engine::cleanup_buffers           15.0 MB      9.2 MB       -5.8 MB     -39%

5 functions changed (2 new, 0 removed, 3 modified)
Total delta: +62.5 MB  (+49%)
```

颜色规则：
- **红色**：变化幅度 ≥ 50%（绝对增加或减少）
- **黄色**：变化幅度 ≥ 10% 且 < 50%
- **无色**：变化幅度 < 10%
- 输出按绝对字节差降序排列（变化最大的在最前面）

### 在 CI/CD 中使用

```bash
#!/bin/bash
# 内存回归检测示例

# 构建两个版本
git checkout main && cargo build --release
./target/release/my_service &; sleep 3
allocmap attach --pid $! --duration 30s --record /tmp/baseline.amr
kill $!

git checkout feature-branch && cargo build --release
./target/release/my_service &; sleep 3
allocmap attach --pid $! --duration 30s --record /tmp/current.amr
kill $!

# 比较，任何函数变化超过 30% 即报警
if allocmap diff /tmp/baseline.amr /tmp/current.amr --min-change-pct 30 | grep -q "^Total delta.*+"; then
  echo "WARN: Memory regression detected"
  allocmap diff /tmp/baseline.amr /tmp/current.amr
  exit 1
fi
echo "PASS: No significant memory regression"
```

### 错误情况

```bash
# 文件不存在
allocmap diff nonexistent.amr current.amr
# Error: Cannot open baseline file 'nonexistent.amr': No such file or directory

# 文件格式无效
allocmap diff corrupted.bin current.amr
# Error: Failed to parse 'corrupted.bin' as AllocMap recording: invalid magic number
```

---

## 使用测试目标程序

AllocMap 内置了 4 个测试目标程序，位于 `tests/target_programs/`，用于演示和集成测试。

### 构建测试目标程序

```bash
# 在 Docker 容器内构建所有测试目标程序
docker run --rm --cap-add=SYS_PTRACE --security-opt seccomp:unconfined \
  -v $(pwd):/workspace -w /workspace \
  allocmap-dev bash -c "
    cargo build --manifest-path tests/target_programs/leak_linear/Cargo.toml &&
    cargo build --manifest-path tests/target_programs/spike_alloc/Cargo.toml &&
    cargo build --manifest-path tests/target_programs/steady_state/Cargo.toml &&
    cargo build --manifest-path tests/target_programs/multithreaded/Cargo.toml
  "
```

### leak_linear — 线性内存泄漏

模拟每秒约 10MB 的线性内存泄漏，用于验证 AllocMap 能否检测到持续增长的内存。

```bash
# 启动泄漏程序（后台运行）
./tests/target_programs/leak_linear/target/debug/leak_linear &
LEAK_PID=$!

# 采样 10 秒，观察内存持续增长
allocmap snapshot --pid $LEAK_PID --duration 10s

# 预期输出：peak_heap_bytes 应约为 100MB（10s × 10MB/s）
# avg_heap_bytes 应约为 50MB

# 结束测试进程
kill $LEAK_PID
```

### spike_alloc — 函数级内存 Surge

模拟函数 A 大量分配内存、释放、然后函数 B 大量分配的场景，用于验证热点定位功能。

```bash
./tests/target_programs/spike_alloc/target/debug/spike_alloc &
SPIKE_PID=$!

# TUI 模式观察，可以看到内存的周期性 surge 和 drop
allocmap attach --pid $SPIKE_PID --duration 30s

kill $SPIKE_PID
```

### steady_state — 稳定状态

模拟正常工作负载：持续分配然后立即释放，内存使用保持稳定。用于验证 AllocMap 不会误报正常程序为泄漏。

```bash
./tests/target_programs/steady_state/target/debug/steady_state &
STEADY_PID=$!

# 快照应显示 alloc_rate ≈ free_rate，live_heap 稳定
allocmap snapshot --pid $STEADY_PID --duration 5s

kill $STEADY_PID
```

### multithreaded — 多线程分配

8 个线程同时进行内存分配，用于测试多线程场景下的采样准确性。

```bash
./tests/target_programs/multithreaded/target/debug/multithreaded &
MT_PID=$!

allocmap attach --pid $MT_PID --duration 20s
# TUI 中按 T 键可切换到线程列表视图，观察活跃线程数（约 5 个：主线程 + 4 工作线程）

kill $MT_PID
```

---

## 录制到 .amr 文件

> **注意**：`.amr` 文件的读写格式已在 Phase 1 实现，`allocmap replay` 回放命令已在 Phase 2 Iter 01 实现。

### 录制

```bash
# 使用 attach 命令录制
allocmap attach --pid 1234 --duration 60s --record session.amr

# 使用 run 命令录制
allocmap run --record session.amr -- ./my_binary
```

### .amr 文件格式

录制文件为自定义二进制格式，包含以下内容：

| 字段 | 大小 | 说明 |
|------|------|------|
| 魔数 | 4 字节 | `AMR\0` |
| 版本 | 4 字节 | u32 LE，当前为 1 |
| header 长度 | 4 字节 | u32 LE |
| header JSON | 变长 | pid、程序名、采样频率、帧数 |
| 帧数据 × N | 变长 | 每帧：4 字节长度 + bincode 编码 |
| footer 长度 | 4 字节 | u32 LE |
| footer JSON | 变长 | peak_bytes、total_frames 等汇总统计 |

---

## TUI 界面详解

### 布局

```
┌─ allocmap · pid=<PID> (<PROGRAM>) · <ELAPSED> · <N> samples ─────────┐
│ LIVE HEAP: <SIZE>  △ <RATE>/s  ALLOCS: <N>/s  FREES: <N>/s  THREADS: <N>  │  ← Stats Bar
├───────────────────────────────────────────────────────────────────────┤
│                                                                       │
│  [时序图 / 热点列表 / 火焰图（占位符）/ 线程列表]                         │  ← Main Content
│                                                                       │
├───────────────────────────────────────────────────────────────────────┤
│ [q]退出  [t]时间轴  [h]热点  [f]火焰图  [T]线程  [↑↓]滚动  [Enter]展开/折叠  │  ← Keybindings
└───────────────────────────────────────────────────────────────────────┘
```

Stats bar 中的 `THREADS: N` 始终显示当前采样帧的活跃线程数，无论当前处于哪种视图。

### 键盘快捷键

| 键 | 功能 |
|----|------|
| `q` / `Ctrl+C` | 退出 AllocMap |
| `t` | 切换到时序图视图（Timeline） |
| `h` | 切换到热点列表视图（Hotspot） |
| `f` | 切换到火焰图视图（占位符） |
| `T` | 切换到线程列表视图（Threads，Phase 2 Iter 03 新增） |
| `↑` / `↓` | 在热点列表中滚动 |
| `Enter` | 展开/折叠选中热点的调用栈 |

### 颜色含义

| 颜色 | 含义 | 触发条件 |
|------|------|---------|
| 绿色 | 内存稳定 | 增速 < 1 MB/s |
| 黄色 | 内存增长中 | 增速 1–10 MB/s |
| 红色 | 内存快速增长/疑似泄漏 | 增速 > 10 MB/s |

### 时序图视图

- 纵轴：堆内存大小
- 横轴：时间（最近 500 个采样点）
- 使用 Unicode block 字符绘制柱状图
- 顶部显示当前值和增速统计

### 热点列表视图

每行显示一个分配热点：
```
> function_name                    [bytes]   [count]   [trend]
  └─ caller_function
     └─ caller_caller
```

- `>` 表示当前选中行
- 按 `Enter` 展开/折叠调用栈
- 如无调试符号，函数名显示为 ELF 文件名或内存地址

### 线程列表视图（Phase 2 Iter 03 新增，`T` 键切换）

展示当前采样帧的所有活跃线程：

```
┌─ Threads ───────────────────────────────────────────────────┐
│  TID     Role                                               │
│  ───────────────────────────────────────────────────────    │
│  7       main                                               │
│  8       worker                                             │
│  9       worker                                             │
│  11      worker                                             │
│  12      worker                                             │
└─────────────────────────────────────────────────────────────┘
```

- `TID` 列显示 Linux 线程 ID（来自 `/proc/PID/task/`）
- `Role` 列：最小 TID 的线程标记为 `main`，其余标记为 `worker`
- 数据来源：`SampleFrame.thread_ids`（每次采样时由 sampler 填充）
- 多线程程序示例：`multithreaded` 测试目标程序运行时可观察到 5 个线程（1 个主线程 + 4 个工作线程）

```bash
# 验证多线程追踪
./tests/target_programs/multithreaded/target/debug/multithreaded &
MT_PID=$!
allocmap attach --pid $MT_PID  # 按 T 键切换到线程视图
kill $MT_PID
```

---

## 常见问题

### Q：运行时报 "Operation not permitted"？

确认 ptrace 权限配置：

```bash
cat /proc/sys/kernel/yama/ptrace_scope  # 应为 0
echo 0 | sudo tee /proc/sys/kernel/yama/ptrace_scope
```

或使用 `sudo` 运行 AllocMap：

```bash
sudo allocmap attach --pid 1234
```

### Q：只能看到内存地址，看不到函数名？

目标程序编译时没有保留调试符号。解决方法：

```bash
# Rust 程序（在 Cargo.toml 中）
[profile.release]
debug = true

# 或设置环境变量
RUSTFLAGS="-g" cargo build --release

# C/C++ 程序
gcc -g -O2 your_program.c -o your_program
cc -g -O2 your_program.cpp -o your_program
```

### Q：Python/Ruby 进程只能看到 CPython/CRuby 内部函数？

这是预期行为。解释型语言的用户代码函数名存储在解释器的运行时数据结构中，不在 native 调用栈上。
- Python 建议使用专门工具：[memray](https://github.com/bloomberg/memray)
- Ruby 建议使用：[ruby-prof](https://github.com/ruby-prof/ruby-prof)

### Q：TUI 在 tmux 中显示异常？

确认 tmux 的颜色配置：

```bash
# 在 tmux 中启用 256 色
echo "set -g default-terminal tmux-256color" >> ~/.tmux.conf
tmux source-file ~/.tmux.conf

# 或在启动 allocmap 时指定 TERM
TERM=xterm-256color allocmap attach --pid 1234
```

### Q：snapshot 输出的 sample_count 为 0？

可能的原因：
1. ptrace 权限不足（见上方权限配置）
2. 目标进程在采样期间退出
3. 在非 Linux 平台运行（Phase 1 仅支持 Linux）

检查方法：

```bash
# 确认进程仍在运行
ps aux | grep $PID

# 确认权限
cat /proc/sys/kernel/yama/ptrace_scope  # 应为 0

# 尝试手动 attach（需 strace）
strace -e ptrace allocmap snapshot --pid $PID 2>&1 | head -20
```

### Q：`run` 命令找不到 liballocmap_preload.so？

`run` 命令按以下路径查找 .so 文件：
1. `allocmap` 可执行文件的同级目录
2. `../lib/`
3. `target/debug/liballocmap_preload.so`
4. `target/release/liballocmap_preload.so`

确保已编译 preload 库：

```bash
docker run --rm -v $(pwd):/workspace -w /workspace \
  allocmap-dev cargo build -p allocmap-preload

ls target/debug/liballocmap_preload.so  # 应存在
```

### Q：火焰图（flamegraph）视图显示占位符？

这是已知限制（Phase 1 Iter 01）。火焰图视图将在后续迭代实现，目前显示 "flamegraph view not yet implemented"。可以使用 `[t]` 时序图或 `[h]` 热点列表视图。
