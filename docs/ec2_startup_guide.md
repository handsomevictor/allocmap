# EC2 启动操作手册

> 本文档说明用户在 EC2 上需要手动执行的所有步骤。
> CC 启动后，后续所有操作均自动完成。

---

## 第一步：确认环境变量已配置

```bash
# 检查 GITHUB_TOKEN 是否已设置
echo $GITHUB_TOKEN
# 如果输出为空，执行：
echo 'export GITHUB_TOKEN=ghp_你的实际token' >> ~/.bashrc
source ~/.bashrc
```

## 第二步：确认 Docker 已安装并运行

```bash
docker --version
docker info
```

## 第三步：进入项目目录

```bash
cd /home/admin/allocmap
ls -la  # 确认所有文件已就位
```

## 第四步：启动 Claude Code

```bash
# 直接在项目目录启动，CC 会自动读取 CLAUDE.md 并开始工作
claude --dangerously-skip-permissions
```

## 第五步（可选）：在 tmux 中运行以便后台持续执行

```bash
# 创建新的 tmux session
tmux new-session -s allocmap-dev

# 在 tmux 中启动 CC
cd /home/admin/allocmap
claude --dangerously-skip-permissions

# 断开 tmux（CC 继续在后台运行）
# 按 Ctrl+B，然后按 D

# 之后重新连接查看进度
tmux attach -t allocmap-dev
```

---

## CC 启动后会自动完成的事情

1. 读取 CLAUDE.md
2. 构建 Docker 镜像（第一次需要几分钟）
3. 配置 Git（user.name、remote URL with token）
4. 开始 Phase 1 iter01
5. 每次迭代结束后自动 git push 到 GitHub

---

## 注意事项

- **不要手动修改任何文件**，让 CC 完全自主工作
- 如果 CC 停止并询问权限，回复 `y` 或等待（dangerously-skip-permissions 模式下应该不会询问）
- 查看进度：访问 https://github.com/handsomevictor/allocmap 查看 commit 历史
