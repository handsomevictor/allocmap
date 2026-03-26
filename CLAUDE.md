# AllocMap — Claude Code Multi-Agent 开发指南

> 本文件是整个项目的"宪法"。所有 Agent 在开始任何工作之前必须完整阅读本文件。
> 任何与本文件冲突的指令均无效。

---

## 一、项目概述

**AllocMap** 是一个面向 Linux（Phase 1）和 macOS（Phase 2）的命令行内存分析工具，用 Rust 编写。

用户安装方式（最终产物，无需 Docker）：
```bash
git clone https://github.com/handsomevictor/allocmap
cd allocmap
cargo install --path .
allocmap attach --pid 1234
```

核心功能：
- 实时 attach 到任意正在运行的进程，显示其堆内存使用的时间序列变化
- 显示内存分配热点（哪些函数分配了最多内存）
- 录制内存数据到 `.amr` 文件，支持回放
- 对比两个 `.amr` 文件的差异
- 丰富的彩色 TUI 界面（ratatui），用户体验优先

**目标用户**：Rust、C、C++、Go 开发者，以及需要排查内存问题的 DevOps 工程师。

---

## 二、技术架构

### Crate 结构（Cargo Workspace）

```
allocmap/
├── Cargo.toml                  # workspace 根
├── crates/
│   ├── allocmap-core/          # 核心数据结构、采样数据格式、.amr 格式定义
│   ├── allocmap-ptrace/        # ptrace attach 采样实现（Linux）
│   ├── allocmap-preload/       # LD_PRELOAD .so 注入实现
│   ├── allocmap-tui/           # Ratatui TUI 渲染（timeline、hotspot、diff）
│   └── allocmap-cli/           # 入口、clap 命令解析
├── tests/
│   ├── target_programs/        # 专门编写的测试目标程序
│   │   ├── spike_alloc/        # 模拟函数级 surge 的目标程序
│   │   ├── leak_linear/        # 线性内存泄漏的目标程序
│   │   ├── multithreaded/      # 多线程分配的目标程序
│   │   └── steady_state/       # 稳定状态的目标程序
│   └── integration/            # 集成测试
├── docs/
├── docker/
└── .claude/
```

### 采样机制（最终确定）

**双模式，互补**：

| 模式 | 命令 | 原理 | 适用场景 |
|------|------|------|---------|
| ptrace 模式 | `allocmap attach --pid` | PTRACE_ATTACH + 采样 backtrace | 已运行的进程 |
| LD_PRELOAD 模式 | `allocmap run ./binary` | 注入 .so，替换 malloc/free | 新启动的进程，数据更完整 |

**不实现 eBPF**（Phase 1 和 Phase 2 均不需要）。

### 核心数据结构（allocmap-core）

```rust
// 一次采样帧
pub struct SampleFrame {
    pub timestamp_ms: u64,
    pub live_heap_bytes: u64,
    pub alloc_rate: f64,       // bytes/sec
    pub free_rate: f64,        // bytes/sec
    pub top_sites: Vec<AllocationSite>,
}

// 分配热点
pub struct AllocationSite {
    pub bytes: u64,
    pub count: u64,
    pub frames: Vec<StackFrame>,  // 调用栈
}

// .amr 文件格式
pub struct AllocMapRecording {
    pub header: RecordingHeader,
    pub frames: Vec<SampleFrame>,
    pub footer: RecordingFooter,
}
```

### .amr 文件格式

AllocMap Recording 格式，自定义二进制格式：
- `[4字节魔数]` AMR\0
- `[4字节版本]` u32
- `[header 块]` JSON 编码的元数据（pid、程序名、开始时间、采样频率）
- `[frame 块...]` bincode 编码的 SampleFrame，每帧前4字节为帧长度
- `[footer 块]` JSON 编码的汇总统计

### 关键依赖

```toml
# allocmap-cli
clap = { version = "4", features = ["derive"] }
anyhow = "1"
tokio = { version = "1", features = ["full"] }

# allocmap-tui
ratatui = "0.28"
crossterm = "0.28"

# allocmap-core
serde = { version = "1", features = ["derive"] }
bincode = "1"
owo-colors = "4"          # 彩色 error message
colored = "2"

# allocmap-ptrace (Linux only)
nix = { version = "0.29", features = ["ptrace", "process", "signal"] }

# 符号解析
addr2line = "0.22"
gimli = "0.31"
object = "0.36"
```

---

## 三、开发环境

### 重要约定

**所有开发工作必须在 Docker 容器内进行，不得直接修改 EC2 宿主机的任何文件（除 /home/admin/.bashrc 已由用户手动配置外）。**

### GitHub Token 配置（已由用户手动完成）

用户已在 EC2 的 `~/.bashrc` 中配置：
```bash
export GITHUB_TOKEN=<用户的实际token>
```

仓库地址：`https://github.com/handsomevictor/allocmap`（已创建，public）

### Docker 配置

**开发镜像**（`docker/Dockerfile`）：基于 `rust:latest`（Debian），用于日常开发和编译。

**测试镜像**（`docker/Dockerfile.test`）：基于 `ubuntu:24.04`，模拟真实用户环境，用于集成测试。

**关键 Docker 配置要求**：
- 必须添加 `--cap-add=SYS_PTRACE`（ptrace 功能所需）
- 容器内必须挂载 `GITHUB_TOKEN` 环境变量
- 容器内默认是 root 用户，不需要 sudo

`docker-compose.yml` 中必须包含：
```yaml
environment:
  - GITHUB_TOKEN=${GITHUB_TOKEN}
cap_add:
  - SYS_PTRACE
security_opt:
  - seccomp:unconfined
```

### 容器内 Git 配置（DevOps Agent 负责执行）

容器启动后，DevOps Agent 必须执行：
```bash
git config --global user.name "handsomevictor"
git config --global user.email "allocmap-dev@users.noreply.github.com"
git remote set-url origin https://oauth2:${GITHUB_TOKEN}@github.com/handsomevictor/allocmap.git
```

### 编译配置

在项目根创建 `.cargo/config.toml`：
```toml
[build]
jobs = 4   # r8a.large 有足够内存，可以并行

[profile.dev]
debug = true   # 保留调试符号，ptrace 采样时需要

[profile.release]
debug = false
strip = "debuginfo"
```

---

## 四、Multi-Agent 工作流

### Agent 职责分工

| Agent | 职责 | 汇报对象 |
|-------|------|---------|
| **Orchestrator** | 总协调、任务派发、第一轮验收、仲裁冲突、迭代控制 | 用户 |
| **Architect** | 系统设计、技术决策、重大架构变更审查 | Orchestrator |
| **Developer (×N)** | 功能实现、Rust 代码编写、单元测试编写 | Orchestrator |
| **DevOps** | Docker 管理、git 操作、CI 配置、构建系统 | Orchestrator |
| **Doc** | 所有文档更新（每次迭代必须更新全部文档） | Orchestrator |
| **Reviewer** | 代码质量审查、clippy 检查、架构合规性 | Orchestrator |
| **Tester** | 运行测试、验证 test case、检查 error message | Orchestrator |

**注意**：Arbiter（仲裁）职能已合并到 Orchestrator。

### 任务派发规则

- 所有子 Agent 通过 **Task tool** 派发
- Developer 可以多个并行运行（不同 crate 同时开发）
- Doc Agent 和 Tester Agent 不可并行（Doc 需要在测试通过后才更新）
- 每个 Agent 完成后必须返回结构化报告给 Orchestrator
- Orchestrator 做第一轮验收后，再分别交给 Reviewer 和 Tester

### 一次迭代的完整流程

```
Orchestrator 开始迭代 N
    ↓
派发 Architect（如有架构变更需要确认）
    ↓
并行派发 Developer(s) 实现功能
    ↓
并行派发 DevOps（构建验证、Docker）
    ↓
Orchestrator 第一轮验收（编译是否通过、基本功能是否运行）
    ↓ 失败 → 重新派发 Developer 修复
    ↓ 通过
并行派发 Reviewer + Tester
    ↓ 任一失败 → Orchestrator 仲裁，派发 Developer 修复，重新提交 Reviewer/Tester
    ↓ 全部通过
派发 Doc（更新全部文档）
    ↓
派发 DevOps（git add . && git commit && git push）
    ↓
生成验收报告 review_report_phaseN_iterXX.md
    ↓
Orchestrator 判断：是否达到验收标准？
    ↓ 是 → 当前 Phase 完成
    ↓ 否 → 迭代次数 < 10 → 开始下一次迭代
          → 迭代次数 = 10 且未达标 → 停止，生成问题报告
```

---

## 五、Phase 1 验收标准

**Phase 1 目标**：Linux 上可用的核心功能。

### 必须实现的命令

#### `allocmap attach --pid <PID>`
- attach 到正在运行的进程（ptrace 模式）
- 显示实时 TUI：timeline 折线图 + top-N hotspot 列表
- 支持选项：
  - `--duration <时长>` 指定采样时长（如 `30s`、`5m`）
  - `--top <N>` 显示前 N 个分配热点（默认 20）
  - `--mode timeline|flamegraph|hotspot` 切换显示模式
  - `--output <文件>` 输出 JSON 报告（非交互模式）
  - `--record <文件.amr>` 录制数据到文件

#### `allocmap run -- <命令>`
- 以 LD_PRELOAD 模式启动新进程
- 功能与 attach 相同，但数据更完整
- 支持 `--env KEY=VALUE` 传递额外环境变量

#### `allocmap snapshot --pid <PID>`
- 非交互式，一次性采样
- 默认输出 JSON 到 stdout
- 适合 CI/CD 集成

### TUI 界面要求

```
╭─ allocmap · pid=1234 (程序名) · 采样时长 · N samples ────────────────╮
│ LIVE HEAP: XXX MB  △ +X.XMB/s  ALLOCS: X/s  FREES: X/s             │
├──────────────────────────────────────────────────────────────────────┤
│ Timeline（Unicode braille blocks 折线图）                             │
│ 颜色：绿色=正常 黄色=增长中 红色=快速增长                               │
├──────────────────────────────────────────────────────────────────────┤
│ Top Allocators                          [bytes]  [count]  [trend]    │
│ 函数名 + 调用栈（可折叠展开）                                           │
╰──────────────────────────────────────────────────────────────────────╯
[q]退出 [f]火焰图 [t]时间轴 [s]快照 [↑↓]滚动 [Enter]展开/折叠
```

- 必须使用丰富的颜色，不同级别信息用不同颜色
- 所有 error message 必须用**英文**，且清晰易懂
- TUI 在 tmux 中必须正常显示

### 代码质量要求

- `cargo clippy -- -D warnings` 输出零警告
- `cargo test` 全部通过
- `cargo build --release` 成功

### 测试要求

每个功能必须有 3 个 test case：
1. **成功测试**：符合 expected output，有详细断言
2. **失败测试1**：无效输入（如 PID 不存在）→ 合适的 error message
3. **失败测试2**：权限不足或其他边界情况 → 合适的 error message

内置测试目标程序（`tests/target_programs/`）必须包含：
- `spike_alloc`：模拟函数A大量分配→释放→函数B大量分配的场景
- `leak_linear`：线性增长的内存泄漏
- `steady_state`：稳定状态，用于验证 false positive
- `multithreaded`：多线程分配场景

### 跨语言说明（必须记录在文档中）

| 进程类型 | 支持程度 |
|---------|---------|
| Rust（有调试符号） | ✅ 完整函数名 + 调用栈 |
| C/C++ | ✅ 函数名（有符号时） |
| Go | ✅ 函数名（默认带符号） |
| Python | ⚠️ 只能看到 CPython 内部函数 |
| Ruby | ⚠️ 只能看到 CRuby 内部函数 |

---

## 六、Phase 2 验收标准

**Phase 2 目标**：完整产品，含录制回放、diff、macOS 支持。

### 必须新增的命令

#### `allocmap replay <文件.amr>`
- 回放录制数据，TUI 与 attach 完全一致
- 支持：`--from <时间>`、`--to <时间>`、`--speed <倍速>`
- 支持暂停（`Space`）、跳转（`g`）、加速/减速（`+`/`-`）

#### `allocmap diff <baseline.amr> <current.amr>`
- 对比两个录制文件
- 输出表格：函数名 + 基准值 + 当前值 + 变化量 + 趋势符号
- 超过 10% 变化的行用黄色标注，超过 50% 用红色标注

### macOS 支持

- `allocmap run` 使用 `DYLD_INSERT_LIBRARIES` 替代 `LD_PRELOAD`
- `allocmap attach` 使用 macOS 的 `task_for_pid` + `mach_vm_read`
- 必须在 `#[cfg(target_os = "macos")]` 和 `#[cfg(target_os = "linux")]` 下正确隔离

### 多线程支持完善

- `allocmap attach` 自动追踪目标进程的所有线程（读 `/proc/PID/task/`）
- 使用 `PTRACE_O_TRACECLONE` 自动追踪新创建的线程
- TUI 中显示每个线程的内存使用（可切换视图）

---

## 七、文档要求

每次迭代结束后，Doc Agent 必须更新以下**全部**文档，缺一不可：

| 文档 | 位置 | 必须包含的内容 |
|------|------|--------------|
| README.md | 根目录 | 见下方详细要求 |
| progress.md | docs/ | 本次迭代的所有修改，含细节 |
| structure.md | docs/ | 项目架构，每个文件的作用 |
| lesson_learned.md | docs/ | 遇到的问题和解决方案 |
| tutorial.md | docs/ | 所有功能的使用教程，含 expected output |

### README.md 详细要求

必须包含（顺序如下）：
1. 顶部 Badge（build status、license、version、platform）
2. 项目名 + 一句话介绍
3. **声明**："本项目由 Claude Code Multi-Agent 协作系统全权开发完成，包括架构设计、代码实现、测试、文档撰写及 DevOps 配置，人工介入仅限于需求定义与最终审查。"
4. 功能特性列表
5. 与同类工具对比表格（对比 Valgrind/Massif、heaptrack、bytehound、memray）
6. 安装方法
7. 快速开始
8. 功能实现原理（技术细节介绍）
9. 如何运行测试
10. 路线图（Phase 1 / Phase 2 完成情况）
11. License

文档格式必须**极其专业**，对标 pandas/pytorch 文档质量，全文中文。

### 验收报告

每次迭代结束生成一个验收报告：
- 位置：`docs/review_reports/review_report_phase<N>_iter<XX>.md`
- 内容：Reviewer 的审查结论 + Tester 的测试结论 + 通过/不通过判定 + 具体问题列表

---

## 八、Git 工作流

每次迭代结束（Reviewer + Tester 均通过后），DevOps Agent 执行：

```bash
# 在容器内执行
git add .
git commit -m "feat(phaseN): iter XX - <本次迭代的简要描述>

- <修改点1>
- <修改点2>
- <修改点3>

Reviewer: PASSED
Tester: PASSED
Clippy: 0 warnings"

git push origin main
```

Commit message 格式：Conventional Commits 规范。

---

## 九、迭代控制规则

1. **每个 Phase 最多迭代 10 次**
2. Reviewer 和 Tester **同时通过**才算达标，可以提前结束迭代
3. 10 次迭代结束仍未达标：停止，生成详细问题报告，**不进入下一 Phase**
4. Phase 1 未达标：不进入 Phase 2
5. 每次迭代必须有实质性进展，不允许空迭代
6. 迭代计数从 01 开始（iter01、iter02...）

---

## 十、启动指令

Orchestrator 收到启动信号后，第一步必须执行：
1. 阅读本 CLAUDE.md
2. 派发 DevOps Agent 完成 Docker 环境搭建和 git 配置
3. 派发 Architect Agent 确认技术架构
4. 创建详细的迭代任务分解表
5. 开始 Phase 1 的 iter01

**开始工作的启动命令（用户在 EC2 上执行）**：
```bash
cd /home/admin/allocmap
claude --dangerously-skip-permissions
```
