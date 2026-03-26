/// 测试目标程序：spike_alloc
///
/// 模拟场景：函数A分配100MB并持有3秒 → 释放 → 函数B分配200MB并持有3秒 → 释放 → 循环
/// 用于验证 allocmap 能否在时序图中清晰看到两个不同的内存 surge
///
/// 使用方式：
///   ./spike_alloc                  # 默认无限循环
///   ./spike_alloc --cycles 3       # 循环3次后退出
///   ./spike_alloc --no-loop        # 只运行一次
use std::time::Duration;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cycles = parse_cycles(&args);

    println!(
        "[spike_alloc] started, pid={}, cycles={}",
        std::process::id(),
        if cycles == 0 { "infinite".to_string() } else { cycles.to_string() }
    );
    println!("[spike_alloc] pattern: function_a(100MB,3s) -> function_b(200MB,3s) -> repeat");

    let mut count = 0;
    loop {
        function_a_heavy_alloc();
        function_b_heavier_alloc();

        count += 1;
        if cycles > 0 && count >= cycles {
            println!("[spike_alloc] completed {} cycles, exiting", count);
            break;
        }
    }
}

/// 函数A：分配100MB，持有3秒后释放
#[inline(never)]
fn function_a_heavy_alloc() {
    println!("[spike_alloc] function_a: allocating 100MB...");
    let data: Vec<u8> = vec![1u8; 100 * 1024 * 1024];
    // 防止编译器优化掉这块内存
    std::hint::black_box(&data);
    std::thread::sleep(Duration::from_secs(3));
    println!("[spike_alloc] function_a: releasing 100MB");
    drop(data);
}

/// 函数B：分配200MB，持有3秒后释放
#[inline(never)]
fn function_b_heavier_alloc() {
    println!("[spike_alloc] function_b: allocating 200MB...");
    let data: Vec<u8> = vec![2u8; 200 * 1024 * 1024];
    std::hint::black_box(&data);
    std::thread::sleep(Duration::from_secs(3));
    println!("[spike_alloc] function_b: releasing 200MB");
    drop(data);
}

fn parse_cycles(args: &[String]) -> u64 {
    for i in 0..args.len() {
        if args[i] == "--cycles" {
            if let Some(val) = args.get(i + 1) {
                return val.parse().unwrap_or(0);
            }
        }
        if args[i] == "--no-loop" {
            return 1;
        }
    }
    0 // 0 = 无限循环
}
