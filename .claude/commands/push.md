# /push 命令

手动触发 git commit + push。

## 使用方式

```
/push [commit message 描述]
```

## 执行逻辑

1. 验证 GITHUB_TOKEN 已设置
2. 验证 cargo build --release 通过
3. 验证 cargo clippy -- -D warnings 零警告
4. 执行 git add .
5. 执行 git commit（使用规范的 commit message 格式）
6. 执行 git push origin main
7. 输出 commit hash 和 push 结果

## 注意

只有在 Reviewer 和 Tester 都通过后才应该执行 /push。
正常迭代流程中 DevOps Agent 会自动执行 push，此命令用于手动补充执行。
