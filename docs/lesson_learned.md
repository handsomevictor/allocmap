# AllocMap 经验教训记录

> 本文档记录开发过程中遇到的问题、解决方案和经验教训。
> 每次迭代后由 Doc Agent 更新。

---

## 项目初始化阶段

### 经验：开发环境使用 Docker 隔离

**背景**：在 EC2 上开发，需要避免污染宿主机环境，同时需要测试 ptrace 功能。

**解决方案**：
- 开发和编译在 Docker 容器内进行（`rust:latest` 镜像）
- 集成测试使用独立的 `ubuntu:24.04` 镜像模拟用户环境
- 容器启动时加入 `--cap-add=SYS_PTRACE` 和 `security_opt: seccomp:unconfined`
- GitHub Token 通过环境变量注入容器，不写入任何文件

**教训**：Docker 容器内默认是 root 用户，ptrace 权限问题需要在容器级别配置，而不是在 Linux 用户级别配置。

---

### 经验：LD_PRELOAD .so 内部不能使用标准 allocator

**背景**：allocmap-preload 需要在 malloc/free 函数被调用时执行我们的代码。

**问题**：如果在 hook 函数内部使用标准的 `Vec`、`HashMap` 等类型，它们会调用系统 malloc，触发我们的 hook，产生无限递归，导致栈溢出。

**解决方案**：在 allocmap-preload 中使用自定义的 bump allocator，只使用 `mmap` 直接分配内存，绕过 malloc 调用链。

**教训**：编写 malloc hook 时必须极其谨慎，所有内部数据结构都必须使用与被 hook 函数不同的内存分配路径。

---

<!-- 后续迭代的经验教训由 Doc Agent 追加 -->
