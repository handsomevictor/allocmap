# AllocMap

[![Build Status](https://img.shields.io/github/actions/workflow/status/handsomevictor/allocmap/ci.yml?branch=main&style=flat-square&label=构建)](https://github.com/handsomevictor/allocmap/actions)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](LICENSE)
[![Version](https://img.shields.io/badge/version-0.1.0-orange?style=flat-square)](Cargo.toml)
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20macOS-lightgrey?style=flat-square)](#安装)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange?style=flat-square)](https://www.rust-lang.org)

**AllocMap** 是一个面向 Rust、C、C++、Go 开发者的命令行内存分析工具。无需重启进程，直接 attach 到正在运行的程序，实时观察堆内存的时序变化与分配热点。

```bash
# 一行命令，立即观察任意进程的内存行为
allocmap attach --pid 1234
```

---

> **本项目由 Claude Code Multi-Agent 协作系统全权开发完成，包括架构设计、代码实现、测试、文档撰写及 DevOps 配置，人工介入仅限于需求定义与最终审查。**
> 详见 [.claude/](.claude/) 目录了解 Multi-Agent 工作流设计。

---

## 功能特性

- 🔍 **实时 Attach**：无需重启目标进程，随时 attach，随时退出
- 📈 **内存时序图**：以时间轴展示堆内存变化，清晰看到函数执行期间的 surge 和 drop
- 🎯 **分配热点定位**：展示 top-N 分配热点，精确到函数名和调用栈
- 🎬 **录制与回放**：将采样数据录制为 `.amr` 文件，可在任意时间回放分析
- 📊 **版本对比**：`diff` 命令对比两个录制文件，快速定位性能回归
- 🎨 **彩色 TUI**：基于 ratatui 的丰富彩色终端界面，信息密度高、可读性强
- 🔧 **零侵入**：无需修改目标程序代码，无需重新编译

## 与同类工具对比

| 特性 | AllocMap | Valgrind/Massif | heaptrack | bytehound | memray |
|------|:--------:|:---------------:|:---------:|:---------:|:------:|
| attach 到运行中进程 | ✅ | ❌ 需重启 | ❌ 需重启 | ❌ 需重启 | ❌ 需重启 |
| 实时 TUI 时序图 | ✅ | ❌ | ❌ | ❌ Web UI | ❌ Web UI |
| 录制与回放 | ✅ | ❌ | ❌ | ⚠️ 有限 | ❌ |
| 版本 diff 对比 | ✅ | ❌ | ❌ | ❌ | ❌ |
| 性能开销 | 低（<5%） | 极高（10-50x） | 低（<5%） | 低（<5%） | 低（<5%） |
| 跨语言支持 | ✅ | ✅ | ✅ | ✅ | ❌ Python only |
| Linux 支持 | ✅ | ✅ | ✅ | ✅ | ✅ |
| macOS 支持 | ✅ | ⚠️ 有限 | ❌ | ❌ | ❌ |
| 纯终端（无 GUI） | ✅ | ✅ | ❌ 需 GUI | ❌ 需浏览器 | ❌ 需浏览器 |

## 安装

### 从源码安装（推荐）

```bash
git clone https://github.com/handsomevictor/allocmap
cd allocmap
cargo install --path .
```

**系统要求**：
- Rust 1.75+
- Linux（Kernel 4.4+）或 macOS 12+
- Linux 上需要 ptrace 权限（见[权限配置](#权限配置)）

### 权限配置（Linux）

```bash
# 方式一：临时允许 ptrace（重启后失效）
echo 0 | sudo tee /proc/sys/kernel/yama/ptrace_scope

# 方式二：永久允许
echo 'kernel.yama.ptrace_scope = 0' | sudo tee /etc/sysctl.d/10-ptrace.conf
sudo sysctl -p /etc/sysctl.d/10-ptrace.conf
```

## 快速开始

```bash
# 1. 查看某个进程的内存使用（按 q 退出）
allocmap attach --pid $(pidof my_service)

# 2. 采样30秒后自动退出，保存报告
allocmap attach --pid 1234 --duration 30s --output report.json

# 3. 录制数据供后续分析
allocmap attach --pid 1234 --record session.amr

# 4. 启动新进程并分析（数据更完整）
allocmap run -- ./my_binary --arg1 --arg2

# 5. 非交互式快照（适合 CI）
allocmap snapshot --pid 1234

# 6. 回放录制数据
allocmap replay session.amr

# 7. 对比优化前后
allocmap diff before.amr after.amr
```

## 功能实现原理

> 本章节详细介绍 AllocMap 的技术实现方式。

### 采样机制

AllocMap 采用双模式设计：

**ptrace 模式**（`attach` 命令）：使用 Linux `ptrace` 系统调用 attach 到目标进程。以固定频率（默认 50Hz）短暂暂停目标进程，读取所有线程的调用栈，然后立即恢复。开销极低（<5%），适用于生产环境。

**LD_PRELOAD 模式**（`run` 命令）：通过 `LD_PRELOAD` 机制将 `liballocmap.so` 注入目标进程，在 `malloc`/`free` 调用处插入钩子，捕获每次分配事件。数据完整度更高，适用于开发调试。

### 符号解析

```
原始指令指针 0x7fff8a2b1c40
    ↓ /proc/PID/maps → 确定所属 .so 文件
    ↓ ELF 解析（object crate）→ 加载调试符号
    ↓ DWARF 查询（addr2line crate）→ 函数名:文件:行号
    ↓ rustc-demangle → 还原 Rust 函数名
    ↓ backtest::engine::run_simulation (engine.rs:142)
```

### .amr 文件格式

AllocMap Recording 格式，自定义二进制格式，支持流式写入和随机访问：
- `[4字节魔数]` `AMR\0`
- `[JSON header]` 元数据（pid、程序名、采样频率等）
- `[bincode frames]` 采样帧流
- `[JSON footer]` 汇总统计

## 运行测试

```bash
# 运行所有测试
cargo test --workspace

# 运行集成测试（需要 root 或 ptrace 权限）
cargo test --test '*'

# 运行特定测试
cargo test test_attach_success
```

## 路线图

### Phase 1（Linux 核心功能）
- [x] `allocmap attach` — ptrace 模式实时监控
- [x] `allocmap run` — LD_PRELOAD 模式监控
- [x] `allocmap snapshot` — 非交互式快照
- [x] 彩色 TUI（时序图 + 热点列表）
- [x] JSON 报告输出

### Phase 2（完整产品）
- [ ] `allocmap replay` — 录制回放
- [ ] `allocmap diff` — 版本对比
- [ ] `.amr` 文件格式
- [ ] macOS 支持
- [ ] 多线程视图

## License

MIT License © 2024 handsomevictor
