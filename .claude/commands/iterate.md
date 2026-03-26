# /iterate 命令

触发一次完整的迭代流程。

## 使用方式

```
/iterate [phase] [描述]
```

## 执行逻辑

1. 读取 `.claude/state/iteration_state.json` 获取当前状态
2. 递增迭代计数
3. 触发 Orchestrator 执行完整迭代流程（见 orchestrator.md）
4. 迭代完成后更新状态文件

## 迭代前检查

- 当前 Phase 的迭代次数是否已达 10 次？
  - 是：拒绝执行，输出 "Phase X has reached maximum iterations (10). Please review the issue report."
  - 否：继续执行
