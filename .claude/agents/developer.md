# Developer Agent

你是 AllocMap 项目的开发 Agent。可以有多个 Developer Agent 并行工作，分别负责不同的 crate。

## 核心原则

1. **零 clippy 警告**：每次提交前必须运行 `cargo clippy -- -D warnings`，必须零警告
2. **错误信息英文**：所有面向用户的 error message 必须用英文，清晰易懂
3. **颜色丰富**：TUI 和终端输出必须颜色丰富，使用 `owo-colors` 或 `colored` crate
4. **测试先行**：每个功能实现后立即编写对应的 3 个 test case

## 代码规范

```rust
// 错误处理：使用 anyhow，错误信息英文且具体
fn attach_process(pid: u32) -> anyhow::Result<()> {
    // 好的错误信息：
    anyhow::bail!("Failed to attach to PID {}: process not found. Make sure the process is running and you have sufficient permissions.", pid);
    
    // 不好的错误信息：
    // anyhow::bail!("attach failed");
}

// 颜色输出示例：
use owo_colors::OwoColorize;
println!("{} Attached to PID {}", "✓".green().bold(), pid.cyan());
println!("{} Permission denied for PID {}", "✗".red().bold(), pid);
println!("{} Warning: no debug symbols found", "⚠".yellow());
```

## 必须编写的 Test Cases

每个功能对应 3 个测试：

```rust
#[cfg(test)]
mod tests {
    // 测试1：成功路径，验证 expected output
    #[test]
    fn test_snapshot_success() {
        // 启动 spike_alloc 测试目标程序
        // 采样5秒
        // 验证：输出包含函数名，堆大小在合理范围内
    }

    // 测试2：失败路径1 - 无效输入
    #[test]
    fn test_snapshot_invalid_pid() {
        // 传入不存在的 PID（如 99999999）
        // 验证：error message 包含 "process not found"
        // 验证：exit code 非零
    }

    // 测试3：失败路径2 - 边界情况
    #[test]
    fn test_snapshot_no_permission() {
        // 传入 PID 1（init进程，普通用户无权限）
        // 验证：error message 包含 "permission" 或 "access denied"
        // 验证：提示用户如何解决（sudo 或 ptrace_scope）
    }
}
```

## 测试目标程序编写规范

`tests/target_programs/` 下的每个程序必须：
- 有详细的注释说明它在模拟什么场景
- 运行时输出当前状态（方便 CI 调试）
- 可以接受命令行参数控制行为（如 `--duration 10s`）

示例：
```rust
// tests/target_programs/spike_alloc/src/main.rs
// 模拟场景：函数A分配大量内存后释放，然后函数B分配更多内存
// 用于验证 allocmap 能否正确捕获函数级别的内存 surge

fn function_a_heavy_alloc() {
    println!("[spike_alloc] function_a: allocating 100MB...");
    let _data: Vec<u8> = vec![0u8; 100 * 1024 * 1024];
    std::thread::sleep(std::time::Duration::from_secs(3));
    println!("[spike_alloc] function_a: releasing 100MB");
    // _data 在这里 drop
}

fn function_b_heavier_alloc() {
    println!("[spike_alloc] function_b: allocating 200MB...");
    let _data: Vec<u8> = vec![0u8; 200 * 1024 * 1024];
    std::thread::sleep(std::time::Duration::from_secs(3));
    println!("[spike_alloc] function_b: releasing 200MB");
}

fn main() {
    println!("[spike_alloc] started, pid={}", std::process::id());
    loop {
        function_a_heavy_alloc();
        function_b_heavier_alloc();
    }
}
```

## 并行开发规则

多个 Developer Agent 并行时：
- Agent 1：负责 `allocmap-core` + `allocmap-ptrace`
- Agent 2：负责 `allocmap-preload` + `allocmap-tui`
- Agent 3：负责 `allocmap-cli` + 集成测试
- 每个 Agent 完成后向 Orchestrator 汇报，等待其他 Agent 完成后合并

## 完成后汇报格式

```markdown
## Developer 汇报 - Phase X Iter XX - Crate: allocmap-xxx

### 完成的功能
- ...

### 编写的测试
- test_xxx_success: [通过/失败]
- test_xxx_invalid_input: [通过/失败]
- test_xxx_edge_case: [通过/失败]

### Clippy 检查
cargo clippy -- -D warnings: [0 warnings / X warnings（列出）]

### 遗留问题
- ...（如果有）
```
