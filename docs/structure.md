# AllocMap 项目架构

> 本文档描述项目的完整目录结构和每个文件的职责。
> 每次迭代后由 Doc Agent 更新。

---

## 目录结构

```
allocmap/
├── Cargo.toml                          # Workspace 根配置，声明所有成员 crate 及共享依赖
├── Cargo.lock                          # 依赖锁定文件（binary 项目提交到 git）
├── CLAUDE.md                           # Claude Code Multi-Agent 开发规范（项目宪法）
├── README.md                           # 项目主页文档（中文，对标 pandas/pytorch 质量）
├── .gitignore                          # Git 忽略规则（target/、*.amr 等）
│
├── .cargo/
│   └── config.toml                     # Cargo 编译配置：并行 jobs=4，dev 保留调试符号
│
├── crates/
│   ├── allocmap-core/                  # 核心数据结构（无平台依赖，其他 crate 均依赖此 crate）
│   │   ├── Cargo.toml                  # 依赖：serde + bincode + owo-colors
│   │   └── src/
│   │       ├── lib.rs                  # crate 入口，re-export SampleFrame、AllocationSite 等
│   │       ├── sample.rs               # SampleFrame、AllocationSite、StackFrame 数据结构定义
│   │       └── recording.rs            # AllocMapRecording：.amr 文件格式读写实现
│   │                                   #   - write_to<W: Write>：写魔数 + header + frames + footer
│   │                                   #   - read_from<R: Read>：读取并校验格式，按 frame_count 读帧
│   │                                   #   - 3 个单元测试（roundtrip、invalid magic、version mismatch）
│   │
│   ├── allocmap-ptrace/                # ptrace 采样实现（Linux primary，macOS stub）
│   │   ├── Cargo.toml                  # 依赖：nix（ptrace/process/signal）、addr2line、libc
│   │   └── src/
│   │       ├── lib.rs                  # crate 入口，平台特性 re-export
│   │       ├── attach.rs               # attach/detach/get_heap_bytes/process_exists 函数
│   │       │                           #   - PtraceAttach RAII 包装（Drop 时自动 detach）
│   │       │                           #   - get_heap_bytes 读 /proc/PID/status VmRSS
│   │       │                           #   - list_threads(pid) 读 /proc/{pid}/task/ 枚举线程（Phase 2 Iter 01 新增）
│   │       │                           #   - 4 个单元测试
│   │       ├── sampler.rs              # PtraceSampler：定频采样循环（Linux）
│   │       │                           #   - attach(pid: u32) 构建采样器，附加 PTRACE_O_TRACECLONE（best-effort）
│   │       │                           #   - sample() → SampleFrame（SIGSTOP→waitpid→读栈→PTRACE_CONT）
│   │       │                           #   - sample() 使用 list_threads().len() 填充 thread_count 字段（Iter 02 新增）
│   │       │                           #   - sample() 填充 thread_ids: Vec<u32>（Iter 03 新增）
│   │       │                           #   - 3 个单元测试
│   │       ├── macos_sampler.rs        # macOS 平台采样器存根（Phase 2 Iter 01 新增）
│   │       │                           #   - #[cfg(target_os = "macos")] 条件编译
│   │       │                           #   - 当前实现：ps -o rss= 读取 RSS 总量
│   │       │                           #   - task_for_pid 完整实现推迟至 iter02
│   │       │                           #   - top_sites 当前返回空列表
│   │       ├── backtrace.rs            # 栈回溯：frame-pointer unwinding（x86_64 only）
│   │       │                           #   - collect_backtrace(pid, max_frames) → Vec<u64>
│   │       │                           #   - read_u64_remote（ptrace::read 读远端内存）
│   │       │                           #   - 3 个单元测试
│   │       └── symbols.rs              # DWARF 符号解析
│   │                                   #   - SymbolResolver：addr2line + rustc-demangle
│   │                                   #   - 地址到函数名缓存（BTreeMap）
│   │                                   #   - 3 个单元测试（construction、caching、fallback）
│   │
│   ├── allocmap-preload/               # LD_PRELOAD 注入库，编译为 .so 动态库
│   │   ├── Cargo.toml                  # crate-type = ["cdylib"]，依赖 libc
│   │   └── src/
│   │       ├── lib.rs                  # .so 入口：allocmap_init()，re-export AllocEvent
│   │       │                           #   - 4 个单元测试
│   │       ├── hooks.rs                # malloc/free/calloc/realloc 的 #[no_mangle] extern "C" 钩子
│   │       │                           #   - 每线程 Cell<bool> 重入保护
│   │       │                           #   - dlsym(RTLD_NEXT, ...) 解析原始函数
│   │       │                           #   - LIVE_BYTES 原子计数（alloc+1，free-1）
│   │       │                           #   - realloc bump-arena fallback 使用 min(old,new) 字节拷贝
│   │       ├── ipc.rs                  # Unix socket IPC（.so → allocmap-cli）
│   │       │                           #   - AllocEvent（repr(C)）：type + addr + size + ts_ms
│   │       │                           #   - send_event()：非阻塞 try_lock + 非阻塞 socket
│   │       └── bump_alloc.rs           # mmap 分配的 8MB bump allocator（hook 内部专用）
│   │                                   #   - 原子 fetch_add，8 字节对齐
│   │                                   #   - contains(ptr) 用于 free/realloc 路径判断
│   │
│   ├── allocmap-tui/                   # Ratatui TUI 渲染（基于 ratatui 0.28 + crossterm 0.28）
│   │   ├── Cargo.toml                  # 依赖：ratatui、crossterm、tokio（async TUI loop）
│   │   └── src/
│   │       ├── lib.rs                  # crate 入口
│   │       │                           #   - init_terminal()、restore_terminal()、install_panic_hook()
│   │       │                           #   - async fn run_tui_loop(app, terminal, rx, duration)
│   │       │                           #     60fps 渲染循环，从 mpsc::Receiver<SampleFrame> 消费数据
│   │       ├── app.rs                  # App 状态管理
│   │       │                           #   - frames: VecDeque<SampleFrame>（最多 500 帧）
│   │       │                           #   - DisplayMode（Timeline/Hotspot/Flamegraph/Threads）
│   │       │                           #   - Threads 变体（Phase 2 Iter 03 新增）：T 键切换到线程列表视图
│   │       │                           #   - is_replay / replay_speed / replay_paused（Phase 2 Iter 01 新增）
│   │       │                           #   - pause_flag: Arc<AtomicBool>（Phase 2 Iter 02 新增）与 feeder 共享，真正中断帧流
│   │       │                           #   - seek_target: Arc<AtomicU64>（Phase 2 Iter 02 新增）g/G 跳转目标
│   │       │                           #   - replay_total_ms: u64（Phase 2 Iter 02 新增）回放总时长
│   │       │                           #   - on_key() 处理键盘事件（q/t/h/f/T/Space/g/G/+/-/↑↓/Enter）
│   │       │                           #   - 13 个单元测试（iter02 新增）
│   │       ├── timeline.rs             # 内存时序图组件
│   │       │                           #   - Unicode block-character 柱状图
│   │       │                           #   - 颜色：绿（< 1MB/s）黄（1–10MB/s）红（> 10MB/s）
│   │       │                           #   - format_bytes(u64) 工具函数（公开）
│   │       ├── hotspot.rs              # 分配热点列表组件
│   │       │                           #   - top-N AllocationSite 渲染
│   │       │                           #   - 支持折叠/展开调用栈（Enter 键）
│   │       ├── theme.rs                # 颜色主题常量（关联函数形式，Theme::xxx()）
│   │       │   （注：render_threads_panel() 线程面板渲染函数，Phase 2 Iter 03 新增）
│   │       │                           #   - render_threads_panel()：使用 ratatui Table 组件渲染 TID + Role 表格
│   │       │                           #   - Stats bar 新增 THREADS: N 字段
│   │       └── events.rs               # 键盘事件轮询
│   │                                   #   - AppEvent enum（Key/Resize/Tick）
│   │                                   #   - poll_event(timeout: Duration) → Option<AppEvent>
│   │
│   └── allocmap-cli/                   # CLI 入口（cargo install 安装此 crate）
│       ├── Cargo.toml                  # 依赖：clap（derive）、tokio（full）、anyhow
│       └── src/
│           ├── main.rs                 # 程序入口，tokio::main，路由到子命令
│           ├── cli.rs                  # clap 命令定义（CLI 结构体，help 文本为英文）
│           ├── util.rs                 # 工具函数
│           │                           #   - parse_duration("30s"/"5m"/"1h") → Duration
│           │                           #   - 9 个单元测试
│           ├── error.rs                # 统一错误处理和格式化（English error messages）
│           └── cmd/
│               ├── attach.rs           # attach 命令实现
│               │                       #   - 验证 /proc/{pid} 存在
│               │                       #   - spawn_blocking 内完成 attach + 采样循环（ptrace 线程约束）
│               │                       #   - 支持 --output（JSON）和 --record（.amr）模式
│               ├── run.rs              # run 命令实现
│               │                       #   - find_preload_so() 定位 liballocmap_preload.so
│               │                       #   - 创建 Unix socket，spawn 子进程（LD_PRELOAD 注入）
│               │                       #   - Linux: LD_PRELOAD 注入；macOS: DYLD_INSERT_LIBRARIES
│               │                       #   - #[cfg(target_os)] 平台条件编译隔离
│               ├── replay.rs           # replay 命令实现（Phase 2 Iter 01 新增）
│               │                       #   - 读取 .amr 文件，按 --from/--to 过滤时间范围
│               │                       #   - 以 timestamp_ms 差值 / speed 计算帧间延迟
│               │                       #   - 支持 Space=暂停/继续，+/-=加减速
│               ├── diff.rs             # diff 命令实现（Phase 2 Iter 01 新增）
│               │                       #   - 对比 baseline.amr 与 current.amr 两个录制文件
│               │                       #   - --min-change-pct 过滤阈值选项
│               │                       #   - 彩色输出：≥10% 黄色，≥50% 红色，按绝对字节差降序
│               └── snapshot.rs         # snapshot 命令实现
│                                       #   - spawn_blocking 内完成 attach + 采样（ptrace 线程约束）
│                                       #   - 输出 JSON：sample_count、peak_heap、avg_heap、top_sites
│       └── tests/
│           └── integration_tests.rs    # CLI 集成测试（iter02 新增，5 个测试）
│                                       #   - snapshot 非存在 PID → 正确 error message
│                                       #   - snapshot 无效 duration → 正确 error message
│                                       #   - --help 输出为英文
│
├── tests/
│   ├── target_programs/                # 集成测试专用目标程序（每个均为独立 Cargo 项目）
│   │   ├── spike_alloc/                # 模拟函数级内存 surge：函数A大量分配→释放→函数B大量分配
│   │   │   ├── Cargo.toml
│   │   │   └── src/main.rs
│   │   ├── leak_linear/                # 线性内存泄漏模拟（每秒约 10MB）
│   │   │   ├── Cargo.toml
│   │   │   └── src/main.rs
│   │   ├── steady_state/               # 稳定分配释放（用于验证无误报）
│   │   │   ├── Cargo.toml
│   │   │   └── src/main.rs
│   │   └── multithreaded/              # 8 线程并发分配场景
│   │       ├── Cargo.toml
│   │       └── src/main.rs
│   └── integration/                    # 集成测试（暂未使用，测试集中在各 crate 内）
│       └── (空目录)
│
├── docs/
│   ├── progress.md                     # 迭代进度记录（每次迭代追加）
│   ├── structure.md                    # 项目架构说明（本文件）
│   ├── lesson_learned.md               # 经验教训（每次迭代追加）
│   ├── tutorial.md                     # 功能使用教程（每次迭代更新）
│   └── review_reports/                 # 验收报告（每次迭代生成一份）
│       └── review_report_phase1_iter01.md
│
├── docker/
│   ├── Dockerfile                      # 开发镜像（rust:latest，Debian）
│   │                                   #   - 含 cargo-watch、基础 build 工具
│   │                                   #   - 用于日常开发和 CI 构建
│   ├── Dockerfile.test                 # 集成测试镜像（ubuntu:24.04）
│   │                                   #   - 模拟真实用户环境（不含 Rust 工具链）
│   └── docker-compose.yml              # Docker Compose 配置
│                                       #   - cap_add: [SYS_PTRACE]
│                                       #   - security_opt: [seccomp:unconfined]
│                                       #   - 挂载 GITHUB_TOKEN 环境变量
│
└── .claude/
    ├── agents/                         # Multi-Agent 角色定义文档
    │   ├── orchestrator.md             # 总协调、任务派发、仲裁
    │   ├── architect.md                # 系统设计、技术决策
    │   ├── developer.md                # Rust 代码实现
    │   ├── devops.md                   # Docker、git、CI
    │   ├── doc.md                      # 文档更新
    │   ├── reviewer.md                 # 代码质量审查
    │   └── tester.md                   # 测试执行
    ├── commands/                       # Slash 命令定义
    ├── hooks/                          # 自动化钩子
    └── state/                          # 迭代状态追踪
        ├── iteration_state.json        # 当前 phase/iter 编号和状态
        ├── checkpoint.md               # 当前步骤快照（用于断点恢复）
        ├── deva_report.md              # Developer A 报告（iter01）
        ├── devb_report.md              # Developer B 报告（iter01）
        ├── devc_report.md              # Developer C 报告（iter01）
        ├── devd_report.md              # Developer D 报告（iter01）
        ├── deve_report.md              # Developer E 报告（iter01）
        ├── devops_report_iter01.md     # DevOps 报告（iter01）
        └── doc_report_iter01.md        # Doc Agent 报告（iter01）
```

---

## 核心模块详解

### allocmap-core（核心，无平台依赖）

所有其他 crate 均依赖此 crate。定义了项目中所有核心数据类型：

- `SampleFrame`：一次采样的快照数据（timestamp_ms、live_heap_bytes、alloc_rate、free_rate、top_sites、thread_count、thread_ids）
- `AllocationSite`：一个分配热点（bytes、count、调用栈 Vec<StackFrame>）
- `StackFrame`：调用栈中的一帧（address、function_name、file、line）
- `AllocMapRecording`：完整的 .amr 录制文件（header + frames + footer）

### allocmap-ptrace（采样引擎，Linux only）

实现基于 ptrace 的采样循环。**关键约束**：Linux ptrace 是线程绑定的，attach 和所有后续 ptrace 操作必须在同一 OS 线程执行。AllocMap 通过将整个采样循环放在 `tokio::task::spawn_blocking` 内解决此问题。

采样精度：读取 `/proc/PID/status` 的 VmRSS 字段作为 live_heap_bytes 近似值。VmRSS 包含共享库和栈，稍高于纯堆，但对趋势分析足够准确。

### allocmap-preload（注入库）

编译为 `.so` 动态库，通过 `LD_PRELOAD` 注入目标进程。**关键约束**：
- 钩子内部不能调用标准 allocator（会导致 malloc hook 无限递归）
- 使用 mmap 分配的 `BumpAllocator` 作为内部数据结构的内存来源
- 通过 Unix Domain Socket 将 `AllocEvent` 异步发送给 allocmap-cli 进程
- 使用每线程 `Cell<bool>` 防止重入（非进程全局 AtomicBool，以避免跨线程误保护）

已知限制：`LIVE_BYTES` 计数器在 free 时减去估算值（free 钩子不知道实际分配大小），计数为 best-effort。ptrace 模式的 live_heap_bytes 来自 `/proc/PID/status`，更准确。

### allocmap-tui（用户界面）

基于 `ratatui 0.28` 的终端 UI，使用 `mpsc::Receiver<SampleFrame>` 从采样线程接收数据。

布局（4 区域）：
1. **Header block**：pid、程序名、采样时长、帧数
2. **Stats bar**：LIVE HEAP、增速、ALLOCS/s、FREES/s
3. **Main content**：Timeline / Hotspot / Flamegraph（按键切换）
4. **Keybindings hint**：底部快捷键提示

颜色约定：
- 绿色：正常状态，增速 < 1 MB/s
- 黄色：内存增长中，增速 1–10 MB/s
- 红色：快速增长（> 10 MB/s）或可能泄漏

### allocmap-cli（命令行入口）

基于 `clap 4`（derive 特性）的 CLI 入口。五个子命令各自实现独立的 `execute()` async 函数（Phase 2 Iter 01 新增 `replay` 和 `diff`）。所有 user-visible error messages 均为英文，格式清晰：
```
Error: Process 99999 not found. Make sure the PID is correct and the process is running.
Error: Invalid duration 'xyz': expected format like 30s, 5m, 1h
```

---

*最后更新：Phase 2 Iter 03（2026-03-26）*
