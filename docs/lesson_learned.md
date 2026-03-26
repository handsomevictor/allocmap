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

*最后更新：Phase 1 Iter 01（2026-03-26）*
