# AllocMap 经验教训记录

> 本文档记录开发过程中遇到的问题、解决方案和经验教训。
> 每次迭代后由 Doc Agent 追加。

---

## 项目初始化阶段

### 经验：开发环境使用 Docker 隔离

**背景**：在 EC2 上开发，需要避免污染宿主机环境，同时需要测试 ptrace 功能。EC2 宿主机未安装 Rust 工具链，所有构建必须在容器内执行。

**解决方案**：
- 开发和编译在 Docker 容器内进行（`rust:latest` 镜像）
- 集成测试使用独立的 `ubuntu:24.04` 镜像模拟用户环境
- 容器启动时加入 `--cap-add=SYS_PTRACE` 和 `security_opt: seccomp:unconfined`
- GitHub Token 通过环境变量注入容器，不写入任何文件

**教训**：Docker 容器内默认是 root 用户，ptrace 权限问题需要在容器级别（`--cap-add=SYS_PTRACE`）配置，而不是在 Linux 用户级别配置。`seccomp:unconfined` 同样必要，因为默认的 Docker seccomp 白名单会拦截某些 ptrace 操作。

---

### 经验：LD_PRELOAD .so 内部不能使用标准 allocator

**背景**：allocmap-preload 需要在 `malloc`/`free` 函数被调用时执行我们的代码。

**问题**：如果在 hook 函数内部使用标准的 `Vec`、`HashMap`、`String` 等类型，它们会调用系统 malloc，触发我们的 hook，产生无限递归，最终导致栈溢出（SIGSEGV）。

**解决方案**：
1. 在 allocmap-preload 中使用自定义的 `BumpAllocator`，只使用 `mmap` 直接分配内存，绕过 malloc 调用链
2. 使用每线程 `Cell<bool>` 作为重入保护标志，在 hook 开头检查并在结尾清除
3. 所有内部数据结构（`AllocEvent`）使用 `repr(C)` 的栈上分配结构体，不依赖 heap

**教训**：编写 malloc hook 时必须极其谨慎。任何触发内存分配的代码（包括 `format!`、`println!`、`Vec::push` 等）都会产生递归。推荐使用 `static` + `OnceLock`/`Mutex` + 栈变量的组合，完全避免堆分配。

**补充**：原版使用 `static mut` 函数指针（REAL_MALLOC 等），虽然通过 `Once` 保证了初始化安全，但 `static mut` 本质上是 unsafe anti-pattern。更好的方式是使用 `OnceLock<MallocFn>`，在 iter02 中计划改进。

---

## Phase 1 — Iter 01（2026-03-26）

### 经验（CRITICAL）：Linux ptrace 是线程绑定的

**背景**：`PtraceSampler` 需要在 async Tokio 运行时中运行，因为 CLI 是全 async 的。

**问题**：Linux 的 `ptrace(2)` 有一个不在文档显眼位置的关键约束：**所有 ptrace 操作（attach、waitpid、GETREGS、CONT 等）必须由同一个 OS 线程发出**。具体来说，调用 `ptrace::attach` 的那个线程，之后必须也调用 `waitpid` 和所有后续 ptrace 请求。

在 iter01 的初始实现中，`PtraceSampler::attach()` 在 Tokio 异步线程上调用，但 `sample()` 被放在 `spawn_blocking` 的线程池线程中运行。这两个线程是不同的 OS 线程，导致：
- `SIGSTOP` 信号发送成功（`kill` 不受 ptrace 线程限制）
- `waitpid` 调用失败，返回 `ESRCH`（"No such process"）
- 采样循环遇到错误立即 `break`
- 最终 `snapshot` 命令输出 `sample_count: 0`，frames 为空

这个 bug 在单元测试中不可见，因为测试只测试"非法 PID 报错"路径，从不对活跃进程跨线程采样。

**修复**：将 `PtraceSampler::attach()` 和整个采样循环（`loop { sample(); sleep() }`）全部移入 `spawn_blocking` 闭包内部，确保 attach 与所有后续 ptrace 操作在同一 OS 线程执行。

**教训**：
1. 在使用 ptrace 的 async Rust 代码中，必须将整个 ptrace 生命周期（从 attach 到 detach）都放在同一个 `spawn_blocking` 任务中
2. Linux man page 对此约束的描述不够显眼，容易遗漏。应在代码注释中明确标注 `// IMPORTANT: must run on the same OS thread as attach`
3. 类似约束也存在于其他 Linux 特性（如某些 io_uring 操作、特定 socket 类型等），编写系统级代码时需格外留意线程亲和性要求

---

### 经验：Multi-Agent 并行写文件可能产生冲突

**背景**：iter01 派发了 5 个 Developer Agent 和 1 个 DevOps Agent 并行工作，部分目标文件重叠。

**问题**：DevOps Agent 负责创建"缺失的模块存根文件"，而 Developer Agent 也被指派实现同一批文件。由于两者并行执行，最终文件内容取决于写入顺序，可能导致一方的实现覆盖另一方。

**实际结果**：DevOps Agent 提供的模块实现足够完整（包含编译通过的代码和基本测试），加上各 Developer Agent 对自己负责的文件进行了正确扩充，最终版本通过了编译和测试。

**教训**：
1. 文件级别的任务分配应尽量避免重叠；若必须重叠，应明确规定"谁的版本优先"
2. DevOps Agent 在创建存根时应优先创建"能编译通过的最小实现"，而不是完整实现，以降低与 Developer Agent 的冲突风险
3. 可以通过在任务描述中加入"读取文件后再写入"约束，确保 Agent 基于最新状态工作，而非覆盖

---

### 经验：realloc hook 的 bump-arena 路径存在 UB

**背景**：`realloc` 需要处理指针来自 bump-arena 的情况（hook 初始化期间的临时分配）。

**问题**：最初实现中，将旧数据拷贝到新位置时使用了 `new_size` 作为 `copy_nonoverlapping` 的字节数。当 `new_size > old_size` 时，这会读取源缓冲区边界之外的内存，触发 undefined behavior，可能导致数据损坏或崩溃。

**修复**：将拷贝字节数改为 `min(old_size, new_size)`。由于 bump-arena 分配仅用于 hook 初始化阶段的少量内部数据，还加了 4096 字节上限作为额外安全保护。

**教训**：在不安全代码（unsafe Rust）中，任何 `copy_nonoverlapping` 调用都必须仔细审查源/目标大小边界。特别是在 realloc 语义下，"新旧大小取小"是正确的拷贝原则。

---

### 经验：Docker 镜像未预装 clippy

**问题**：`cargo clippy` 在 Docker 容器内失败，提示 "component 'clippy' is not installed"。Dockerfile 使用 `rust:latest` 基础镜像，但 clippy 不在默认安装列表中。

**修复方案**：在使用 clippy 前先运行 `rustup component add clippy`。长期解决方案：在 `docker/Dockerfile` 中加入 `RUN rustup component add clippy` 以预装到镜像层，iter02 计划实施。

**教训**：`rust:latest` 镜像不包含所有 rustup 组件（clippy、rustfmt 等）。生产 Dockerfile 应显式安装所需组件，以确保 CI 环境的确定性。

---

## Phase 1 — Iter 02（2026-03-26）

### 经验：spawn_blocking JoinHandle 处理

**问题**：`tokio::task::spawn_blocking` 返回 `JoinHandle`，如果不保存该 handle，其中发生的 panic 会被静默丢弃，调用方无法感知错误。

**修复**：
- 对于需要等待结果的场景（如 snapshot），用 `let handle = spawn_blocking(...); handle.await` 显式等待。
- 对于不需要等待的场景（如 attach 的 TUI 模式），用 `let _handle = spawn_blocking(...)` 命名变量，明确表达"有意不等待"的意图。

**教训**：在 Tokio 异步代码中，任何 `spawn_blocking` 调用都应保存 handle 并处理，即使只是为了记录 panic。

---

### 经验：测试覆盖率从第一个迭代开始

**问题**：Iter 01 中 allocmap-tui 和 allocmap-cli 测试覆盖率为 0，导致 Reviewer 给出 "FAILED WITH CONDITIONS" 的判决。

**修复**：Iter 02 补充了 28 个新测试，将总数从 27 提升至 55。

**教训**：每个 crate 的测试覆盖率应在实现的同一迭代中完成，而不是推迟到下一个迭代。CLAUDE.md 规范要求每个功能最少 3 个测试（成功、无效输入、边界情况）。

---

## Phase 2 — Iter 01（2026-03-26）

### 经验：Rust cfg 条件编译是跨平台隔离的正确方式

**背景**：Phase 2 需要同时支持 Linux（ptrace）和 macOS（task_for_pid），两个平台的采样 API 完全不同。

**问题**：若维护两套独立代码分支，代码库体积增大，同步维护成本高，且容易出现"只修了一个平台"的 bug。

**解决方案**：
- 在同一函数/模块内使用 `#[cfg(target_os = "linux")]` 和 `#[cfg(target_os = "macos")]` 隔离平台差异
- 不使用 Cargo feature flags 切换平台（feature flags 更适合功能开关，平台差异用 cfg 更自然）
- 对于较大的平台特定实现，新建独立文件（如 `macos_sampler.rs`），通过 `#[cfg(target_os = "macos")] mod macos_sampler;` 条件包含

**教训**：
1. `#[cfg(target_os)]` 比 `#[cfg(feature = "macos")]` 更符合"平台支持"的语义
2. 条件编译块应尽量小，公共接口保持统一，只在实现层面分叉
3. 即使 macOS 实现是存根（stub），也应保证在 macOS 上 `cargo build` 通过，以便 CI 提前发现问题

---

### 经验：replay 帧定时应基于录制时间戳差值，而非固定间隔

**背景**：`allocmap replay` 需要以接近真实节奏播放录制文件中的帧序列。

**问题**：如果固定使用 `1000ms / sample_rate` 作为帧间隔，当录制期间采样丢帧（如目标进程短暂不响应）时，回放会"压缩"那段时间，丢失了真实的时间感。

**解决方案**：计算相邻帧的 `timestamp_ms` 差值作为等待时间，再除以 `--speed` 倍数。代码逻辑：
```
sleep_ms = (frame[i+1].timestamp_ms - frame[i].timestamp_ms) / speed
```

**教训**：
1. 录制文件中保存绝对时间戳（而非间隔）是正确选择，使回放逻辑大幅简化
2. 变速播放只需除法，无需特殊处理
3. Phase 1 `.amr` 格式中 `SampleFrame.timestamp_ms` 的设计决策在 Phase 2 得到了验证

---

### 经验：状态标志的传播需要贯穿整个数据流

**背景**：`replay_paused` 标志添加到了 TUI `App` 结构体，用于 Space 键切换暂停状态。

**问题**：TUI `App` 中的 `replay_paused` 字段更新后，负责向 TUI 推送帧的 feeder 任务（运行在独立 Tokio 任务中）并不读取这个字段，导致暂停按键只改变显示状态，不实际暂停帧的推送。

**根因**：feeder 任务和 TUI 任务通过 `mpsc::channel` 通信，但缺少反向的控制信号通路。

**修复方向（iter02）**：新增一个 `tokio::sync::watch` 或 `Mutex<bool>` 共享标志，feeder 任务在每帧推送前检查该标志，若暂停则 `sleep` 等待。

**教训**：
1. UI 状态和后台任务状态是两个不同的层，需要显式的跨任务通信机制来同步
2. 设计异步系统时，应在架构层面明确"谁控制谁、如何传递信号"，而不是在实现后发现遗漏
3. 这类问题在 review 阶段容易被发现（Reviewer 可以静态分析控制流），应列入 Reviewer checklist

---

### 经验：/proc/PID/task/ 枚举线程需要注意竞态

**背景**：`list_threads(pid)` 通过读取 `/proc/{pid}/task/` 目录来枚举目标进程的所有线程。

**问题**：目录读取（`fs::read_dir`）和 ptrace 操作之间存在竞态——某个线程可能在我们读完目录但还未 attach 时退出，导致后续 ptrace 调用返回 `ESRCH`。

**当前处理**：`list_threads()` 返回线程 ID 列表，调用方负责处理 `ESRCH` 错误（忽略已退出的线程）。每次采样前重新调用 `list_threads()` 以获取最新线程列表，而非缓存。

**教训**：
1. 多线程进程的线程集合是动态变化的，不能假设"枚举时获取的列表"在操作时依然有效
2. 系统级代码中，所有"先读后操作"的模式都应考虑 TOCTOU 竞态
3. `ESRCH` 应被视为正常情况，而非错误，在 ptrace 代码的循环中应 continue 而非 break

---

## Phase 2 — Iter 02（2026-03-26）

### 经验：跨任务状态共享应使用 Arc<Atomic*>，而非结构体内部字段

**背景**：Iter 01 中 `App.replay_paused` 被设计为普通 `bool`，Space 键切换时只更新 `App` 内部值，而 feeder 任务独立运行于另一个 `tokio::spawn` 任务，无法感知该字段的变化。

**问题**：Tokio 任务之间不共享内存，两个任务只通过 `mpsc::channel` 单向通信。`App` 结构体存活于 TUI 任务中，feeder 任务持有的是独立的数据副本或无引用，导致暂停标志更新对 feeder 不可见。

**修复**：将 `replay_paused` 和 `seek_target` 改为 `Arc<AtomicBool>` 和 `Arc<AtomicU64>`，在创建 feeder 任务时传入 `Arc::clone()`。feeder 在每帧推送前原子读取标志，若暂停则以短间隔轮询等待，完全绕开 channel 机制。

**教训**：
1. 在 Tokio 多任务架构中，"控制信号"（暂停、跳转、取消）与"数据流"（帧推送）是两个正交的通信方向，前者适合 `Arc<Atomic*>` 或 `watch::channel`，后者适合 `mpsc::channel`
2. 在设计异步系统时，应在架构图上明确标注每个任务的输入和输出，防止遗漏反向控制通路
3. `Arc<AtomicBool>` 是最轻量的跨任务布尔标志，无锁，无 await，适合高频读取的暂停/恢复场景

---

### 经验：PTRACE_O_TRACECLONE 应设为 best-effort

**背景**：`PTRACE_O_TRACECLONE` 可让内核在目标进程调用 `clone()` 时自动通知 tracer，实现新线程的自动追踪。

**问题**：并非所有内核配置和权限级别都支持 `PTRACE_O_TRACECLONE`。若将其设为必要步骤（失败则中止），会导致在某些 EC2 实例或受限容器环境中 `allocmap attach` 无法工作。

**处理方式**：在 `ptrace::setoptions()` 调用后检查返回值，若失败仅记录 `warn!` 日志，继续执行后续采样逻辑。功能降级为"无自动线程追踪"，而非完全失败。

**教训**：
1. 任何"增强功能"（非核心功能）的系统调用都应设为 best-effort，失败时降级而非中止
2. `warn!` 而非 `error!` 传达"功能受限但程序可继续"的语义，有助于用户诊断而不引起恐慌
3. 在 Reviewer checklist 中加入"非核心系统调用是否有 best-effort 处理"一项

---

## Phase 2 — Iter 03（2026-03-26）

### 经验：ratatui Table 组件适合展示结构化列表数据（如线程 TID 表格）

**背景**：Phase 2 Iter 03 需要在 TUI 中新增一个线程视图，展示当前采样帧的所有活跃线程（TID + 角色）。

**问题**：最初考虑用 `Paragraph` 组件手动拼接文本行来渲染线程列表，但这样无法对齐列宽，也难以后续扩展为可选中、可滚动的多列表格。

**解决方案**：使用 ratatui 的 `Table` 组件：
- `Table::new(rows, widths)` 接受 `Vec<Row>` 和列宽约束（`Constraint::Length` / `Constraint::Min`）
- 每个 `Row` 由若干 `Cell` 组成，支持独立样式（如用 `Style::default().fg(Color::Cyan)` 高亮主线程）
- `Table` 自带列对齐，无需手动 `format!("{:>10}", ...)`
- 切换到 `DisplayMode::Threads` 后，渲染函数 `render_threads_panel()` 从最新帧的 `thread_ids` 字段构建 `Vec<Row>`

**关键代码模式**：
```rust
let rows: Vec<Row> = thread_ids.iter().map(|&tid| {
    let role = if tid == min_tid { "main" } else { "worker" };
    Row::new(vec![
        Cell::from(tid.to_string()),
        Cell::from(role),
    ])
}).collect();

let table = Table::new(rows, [Constraint::Length(8), Constraint::Min(10)])
    .header(Row::new(vec!["TID", "Role"]).style(Style::default().bold()));
```

**教训**：
1. ratatui `Table` 是渲染二维列表（如线程列表、热点列表）的首选组件，比手动字符串拼接可维护性高得多
2. `Constraint::Length(N)` 适合固定宽度列（如 TID 数字），`Constraint::Min(N)` 适合弹性宽度列（如函数名、角色名）
3. 表格的"最小 TID 为主线程"启发式规则简单有效，避免了需要额外 API 来区分主线程的复杂度

---

### 经验：`#[serde(default)]` 是向后兼容结构体字段扩展的标准做法

**背景**：`SampleFrame` 新增 `thread_ids: Vec<u32>` 字段，需要确保旧版本录制的 `.amr` 文件仍可正常反序列化。

**问题**：如果直接添加字段而不设默认值，使用 `serde` 反序列化旧文件时会因字段缺失而报错。

**解决方案**：对新字段添加 `#[serde(default)]` 注解。`Vec<u32>` 的 `Default` 实现返回空 `Vec`，因此旧文件在反序列化时 `thread_ids` 自动填充为 `[]`，完全无感知。

**教训**：
1. 任何对已存在序列化格式（`.amr` 文件、JSON 输出）的结构体字段扩展，都必须加 `#[serde(default)]`
2. `bincode` 和 `serde_json` 对此注解的处理行为一致：字段缺失时使用 `Default::default()`
3. 这个模式应作为 AllocMap 核心数据结构扩展的标准规范，记入代码注释

---

---

## Phase 2 — Iter 04（2026-03-27）

### 经验：sleep-phase 帧会污染热点帧列表

**背景**：`PtraceSampler` 以 50Hz 频率对目标进程采样。当目标进程在 `std::thread::sleep(3s)` 中，50Hz 意味着 150 个样本都抓到进程处于 `nanosleep` 系统调用内部的帧，这些帧只有 libc 函数，没有源文件信息。

**问题**：`AccumSite.frames` 每次采样都无条件用最新帧覆盖，导致分配阶段捕获的含 `file` 信息的帧（如 `spike_alloc::function_large_alloc` → `src/main.rs:53`）被 150 个 sleep-phase 帧覆盖，最终 `best_user_frame()` 找不到用户代码，`File:Line` 栏显示 `<system>`。

**修复**：只有当新帧集合中存在 `file.is_some()` 的帧时，才覆盖 `AccumSite.frames`：
```rust
let new_has_file = frames.iter().any(|f| f.file.is_some());
let old_has_file = entry.frames.iter().any(|f| f.file.is_some());
if new_has_file || !old_has_file {
    entry.frames = frames;
}
```

**教训**：
1. ptrace 采样是统计性的，分配事件和休眠事件都会被采样，两者比例取决于持有时长与采样频率的比值
2. 帧质量不等于帧新鲜度，需要保留"最有信息量"的帧而非"最新"的帧
3. `file.is_some()` 是判断帧质量的快速代理指标：有源文件路径的帧来自调试信息充分的用户代码

---

### 经验：PIE 可执行文件的 addr2line 地址需要减去 load_base

**背景**：Linux PIE（Position Independent Executable）在运行时被 mmap 到随机地址（ASLR）。ptrace 捕获的 RIP 是运行时虚拟地址，但 DWARF 调试信息中的地址是相对 ELF 文件起始的虚拟地址。

**问题**：将运行时地址直接传给 `addr2line` 会得到错误的（或空的）符号信息，因为地址空间不匹配。

**修复**：从 `/proc/PID/maps` 找到包含目标 IP 的映射段，并找到同一路径中 `file_offset == 0` 的段起始地址（即 `load_base`）：
```rust
let load_base = entries.iter()
    .filter(|e| e.path == hit.path && e.file_offset == 0)
    .map(|e| e.start)
    .next()
    .unwrap_or(hit.start);
let elf_vaddr = ip.saturating_sub(load_base);
```

**教训**：
1. 对 PIE 二进制（几乎所有现代 Linux 可执行文件），`addr2line` 需要的是 ELF 虚拟地址，不是进程虚拟地址
2. `file_offset == 0` 的 mmap 段是第一个 PT_LOAD 段的开始，代表整个 ELF 在内存中的起始位置
3. 调试符号解析问题加 `ALLOCMAP_DEBUG_SYMBOLS=1` 环境变量即可看到完整的地址变换过程

---

### 经验：braille 字符可将柱状图密度提高一倍

**背景**：每个终端字符（monospace）占一个字符宽，若每秒采样一列则每个字符宽对应 1 秒。终端宽度约 200 字符时，最多显示 200 秒历史。

**问题**：要显示 400 秒历史而不减少时间分辨率，需要每个字符宽对应 2 秒数据，但普通字符（▁▂▃▄▅▆▇█）只能表达一列。

**方案**：使用 Unicode Braille 字符（`U+2800` - `U+28FF`）。每个 braille 字符包含 4×2=8 个点位，分左右两列。左列点位映射 bit 6,2,1,0，右列点位映射 bit 7,5,4,3。每列有 0-4 个点可填充，表达 5 个高度级别（0%-100% 以 25% 为步长）。

**实现**：
```rust
const LEFT_BITS:  &[u8] = &[0x00, 0x40, 0x44, 0x46, 0x47]; // 从底到顶填充左列
const RIGHT_BITS: &[u8] = &[0x00, 0x80, 0xA0, 0xB0, 0xB8]; // 从底到顶填充右列
let braille = char::from_u32(0x2800 + (left_bits | right_bits) as u32).unwrap();
```

**教训**：
1. Braille 字符是终端 UI 中实现"半宽"柱状图的标准技术（btop、bpytop 等工具均采用）
2. 峰值点可通过在 avg 柱顶部上方的行加单个最高点位（`0x01` 左顶，`0x08` 右顶）表达
3. 颜色仍按字符整体着色，因为终端不支持同一字符内的分色

---

---

## Phase 2 — Iter 05（2026-03-27）

### 经验：动态计算 Y 轴宽度而非硬编码常量

**背景**：Timeline 图表的 Y 轴标签显示三行：y_max、y_max/2、0。原始实现硬编码 `Y_LABEL_WIDTH = 9`，基于 `{:>6}` 格式对齐。

**问题**：当 y_max = 1.2GB 时，`format_bytes(1.2GB) = "1.1GB"` = 5 chars，`format_bytes(0.6GB) = "572.2MB"` = 7 chars。`{:>6}` 格式将 5 char 值填充到 6，将 7 char 值保持为 7，导致三行标签宽度不同（8 vs 9），`┤` 列偏移。

**修复**：在渲染时动态计算宽度：
```rust
let max_val_len = [y_max, y_max/2, 0].iter().map(|&v| format_bytes(v).len()).max().unwrap_or(2);
let y_label_width = max_val_len + 2; // +space +corner
```

**教训**：
1. TUI 中任何"固定"列宽都应在运行时验证，尤其当内容是格式化数字（字节、时间等）时
2. 当值跨越单位边界（KB→MB→GB）时，格式化字符串的长度会变化，必须动态适应
3. `compute_y_label_width` 设为 `pub fn` 使得后续的自动化测试可以验证边界值

---

### 经验：从调用栈聚合火焰图无需完整的树结构

**背景**：传统火焰图工具（如 FlameGraph.pl）从 perf 记录中读取大量采样，每帧包含完整调用栈，通过计数每个 (depth, function) 对出现的频率来确定块宽度。

**AllocMap 的简化**：我们已经有预聚合的 `top_sites`（每个条目含一个代表性调用栈 + `live_bytes`），不需要原始采样数据。直接对每个 site 的调用栈按深度聚合即可：

```rust
for (depth, frame) in site.frames.iter().rev().enumerate() {
    depth_data[depth].entry(frame.function.clone()).or_default().0 += site.live_bytes;
}
```

**结果**：每个深度层（row）独立归一化（块宽度 = 该层内的占比），图表在有限的采样数量下也能提供有用的视角。

**局限**：这不是严格意义上的火焰图（块位置不反映父子关系的位置对应），而是"按深度分层直方图"。对于展示"哪些函数在哪个调用深度最耗内存"，这已经足够实用。

**教训**：
1. 在有限数据源上实现工具时，应先设计简化版并验证其有用性，而非追求理论完整性
2. 每层独立归一化的"扁平火焰图"实现简单（O(N×D) 复杂度），视觉效果与真实火焰图相近
3. ratatui 中无法高效做"柱状块背景色"，但彩色前景字符（配合 Modifier::BOLD）已足够区分语言

---

*最后更新：Phase 2 Iter 05（2026-03-27）*
