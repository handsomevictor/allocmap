# AllocMap 使用教程

> 本文档涵盖 AllocMap 的所有功能，逐一介绍使用方法、参数说明、预期输出和常见问题。
> 每次迭代后由 Doc Agent 更新，确保与实际功能完全一致。

---

## 安装

```bash
git clone https://github.com/handsomevictor/allocmap
cd allocmap
cargo install --path .

# 验证安装
allocmap --version
# 输出：allocmap 0.1.0
```

---

## 权限配置（Linux 必读）

AllocMap 使用 ptrace 系统调用，需要相应权限：

```bash
# 检查当前 ptrace 权限级别
cat /proc/sys/kernel/yama/ptrace_scope
# 0 = 允许所有用户 attach
# 1 = 只能 attach 到子进程（需要 sudo 才能 attach 到任意进程）
# 2/3 = 更严格的限制

# 临时设置为允许（重启失效）
echo 0 | sudo tee /proc/sys/kernel/yama/ptrace_scope
```

---

## 命令详解

<!-- 以下章节在功能实现后由 Doc Agent 填充 -->

### allocmap attach

**功能**：attach 到正在运行的进程，实时显示内存使用情况。

> 📌 此功能在 Phase 1 实现，文档将在对应迭代后更新。

---

### allocmap run

**功能**：以 LD_PRELOAD 模式启动新进程，数据采集更完整。

> 📌 此功能在 Phase 1 实现，文档将在对应迭代后更新。

---

### allocmap snapshot

**功能**：非交互式快照，适合 CI/CD 集成。

> 📌 此功能在 Phase 1 实现，文档将在对应迭代后更新。

---

### allocmap replay

**功能**：回放 `.amr` 录制文件。

> 📌 此功能在 Phase 2 实现，文档将在对应迭代后更新。

---

### allocmap diff

**功能**：对比两个 `.amr` 文件的差异。

> 📌 此功能在 Phase 2 实现，文档将在对应迭代后更新。

---

## TUI 界面说明

> 📌 TUI 界面截图和说明将在 Phase 1 实现后更新。

---

## 常见问题

### Q：attach 时报 "Operation not permitted"？

**A**：需要配置 ptrace 权限，见上方[权限配置](#权限配置)章节。

### Q：看不到函数名，只看到内存地址？

**A**：目标程序编译时没有保留调试符号。解决方法：

```bash
# Rust 程序
RUSTFLAGS="-g" cargo build --release
# 或在 Cargo.toml 中：
# [profile.release]
# debug = true

# C/C++ 程序
gcc -g -O2 your_program.c -o your_program
```

### Q：Python/Ruby 进程只能看到 CPython/CRuby 内部函数？

**A**：这是预期行为。解释型语言的函数名存在解释器的运行时结构中，不在 native 调用栈里。Python 请使用专门的 `memray` 工具；Ruby 请使用 `ruby-prof`。
