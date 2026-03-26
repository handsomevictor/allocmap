# AllocMap 开发进度记录

> 本文档记录每次迭代的详细修改内容，供后续参考。
> 每次迭代结束后由 Doc Agent 自动更新。

---

## 项目初始化（2024-XX-XX）

### 完成内容
- 创建项目基础结构
- 编写 CLAUDE.md（项目开发规范）
- 配置 Multi-Agent 工作流（.claude/ 目录）
- 编写初始文档框架
- 配置 Docker 开发环境
- 配置 GitHub Actions CI

### 技术决策记录
- 采样机制：双模式（ptrace + LD_PRELOAD），不实现 eBPF
- TUI 框架：ratatui
- 符号解析：addr2line + gimli + object + rustc-demangle
- 错误处理：anyhow（CLI 层）
- 颜色输出：owo-colors

---

<!-- 后续迭代记录由 Doc Agent 在每次迭代后追加 -->
