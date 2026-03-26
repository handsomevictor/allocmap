# /review 命令

手动触发 Reviewer + Tester 对当前代码进行审查。

## 使用方式

```
/review
```

## 执行逻辑

1. 并行派发 Reviewer Agent 和 Tester Agent
2. 等待两者都返回结果
3. 汇总结果，输出综合报告
4. 如果两者都通过：提示可以执行 /push
5. 如果任一不通过：列出所有问题
