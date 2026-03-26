# AllocMap 项目架构

> 本文档描述项目的完整目录结构和每个文件的职责。
> 每次迭代后由 Doc Agent 更新。

---

## 目录结构

```
allocmap/
├── Cargo.toml                          # Workspace 根配置，声明所有 crate
├── Cargo.lock                          # 依赖锁定文件（binary 项目提交）
├── CLAUDE.md                           # Claude Code 开发指南（项目宪法）
├── README.md                           # 项目主页文档
├── .gitignore                          # Git 忽略规则
├── .cargo/
│   └── config.toml                     # Cargo 编译配置（并行数、profile 等）
│
├── crates/
│   ├── allocmap-core/                  # ⭐ 核心数据结构（无平台依赖）
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  # crate 入口，re-export 主要类型
│   │       ├── sample.rs               # SampleFrame、AllocationSite 等数据结构
│   │       └── recording.rs            # .amr 文件格式的读写实现
│   │
│   ├── allocmap-ptrace/                # ⭐ ptrace 采样实现（Linux only）
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  # crate 入口
│   │       ├── attach.rs               # PTRACE_ATTACH / PTRACE_DETACH 逻辑
│   │       ├── sampler.rs              # 定频采样循环（默认 50Hz）
│   │       ├── backtrace.rs            # 从 ptrace 读取调用栈
│   │       └── symbols.rs              # DWARF 符号解析（addr2line + rustc-demangle）
│   │
│   ├── allocmap-preload/               # ⭐ LD_PRELOAD .so 实现
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  # .so 入口，malloc/free hook
│   │       ├── hooks.rs                # malloc/free/realloc 的替换实现
│   │       ├── ipc.rs                  # 与 allocmap-cli 的进程间通信（Unix socket）
│   │       └── bump_alloc.rs           # .so 内部专用分配器（避免递归）
│   │
│   ├── allocmap-tui/                   # ⭐ Ratatui TUI 渲染
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  # crate 入口
│   │       ├── app.rs                  # TUI 应用状态管理
│   │       ├── timeline.rs             # 内存时序折线图组件
│   │       ├── hotspot.rs              # 分配热点列表组件
│   │       ├── theme.rs                # 颜色主题定义
│   │       └── events.rs               # 键盘事件处理
│   │
│   └── allocmap-cli/                   # ⭐ CLI 入口
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs                 # 程序入口
│           ├── cli.rs                  # clap 命令定义
│           ├── cmd/
│           │   ├── attach.rs           # attach 命令实现
│           │   ├── run.rs              # run 命令实现
│           │   └── snapshot.rs         # snapshot 命令实现
│           └── error.rs                # 统一错误处理和格式化
│
├── tests/
│   ├── target_programs/                # 集成测试用的目标程序
│   │   ├── spike_alloc/                # 模拟函数级内存 surge 的程序
│   │   │   └── src/main.rs
│   │   ├── leak_linear/                # 模拟线性内存泄漏的程序
│   │   │   └── src/main.rs
│   │   ├── steady_state/               # 稳定状态程序（验证无误报）
│   │   │   └── src/main.rs
│   │   └── multithreaded/              # 多线程分配场景
│   │       └── src/main.rs
│   └── integration/                    # 集成测试
│       ├── test_attach.rs              # attach 命令的集成测试
│       ├── test_run.rs                 # run 命令的集成测试
│       └── test_snapshot.rs            # snapshot 命令的集成测试
│
├── docs/
│   ├── progress.md                     # 迭代进度记录（本文件）
│   ├── structure.md                    # 项目架构说明
│   ├── lesson_learned.md               # 经验教训
│   ├── tutorial.md                     # 功能使用教程
│   └── review_reports/                 # 验收报告（每次迭代生成）
│       └── review_report_phase1_iter01.md
│
├── docker/
│   ├── Dockerfile                      # 开发镜像（rust:latest）
│   ├── Dockerfile.test                 # 集成测试镜像（ubuntu:24.04）
│   └── docker-compose.yml              # Docker Compose 配置
│
└── .claude/
    ├── agents/                         # Multi-Agent 定义
    │   ├── orchestrator.md
    │   ├── architect.md
    │   ├── developer.md
    │   ├── devops.md
    │   ├── doc.md
    │   ├── reviewer.md
    │   └── tester.md
    ├── commands/                       # Slash 命令
    │   ├── iterate.md
    │   ├── review.md
    │   └── push.md
    ├── hooks/                          # 自动化钩子
    │   └── post-iteration.sh
    └── state/                          # 迭代状态追踪
        └── iteration_state.json
```

---

## 核心模块详解

### allocmap-core（核心，无平台依赖）

所有其他 crate 都依赖此 crate。定义了项目中所有核心数据类型：
- `SampleFrame`：一次采样的快照数据
- `AllocationSite`：一个分配热点（函数名 + 字节数 + 次数 + 调用栈）
- `AllocMapRecording`：.amr 文件的完整内容

### allocmap-ptrace（采样引擎，Linux only）

实现 ptrace 采样循环。关键技术点：
- 使用 `nix` crate 的 ptrace API
- 采样时 `PTRACE_ATTACH` → 读 backtrace → `PTRACE_CONT`
- 符号解析使用 `addr2line` crate，支持 DWARF debug info

### allocmap-preload（注入库）

编译为 `.so` 动态库，通过 `LD_PRELOAD` 注入目标进程。**关键约束**：
- 内部不能使用 Rust 标准分配器（会导致 malloc hook 无限递归）
- 使用自定义 `BumpAllocator` 分配内部数据结构
- 通过 Unix Domain Socket 将数据发送给 allocmap-cli 进程

### allocmap-tui（用户界面）

基于 `ratatui` 的终端 UI。颜色约定：
- 🟢 绿色：正常状态，内存稳定
- 🟡 黄色：内存增长中（增速 > 1MB/s）
- 🔴 红色：快速增长（增速 > 10MB/s）或可能泄漏

---

*最后更新：项目初始化*
