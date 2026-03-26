/// 测试目标程序：leak_linear
///
/// 模拟场景：每秒分配10MB，永不释放（线性内存泄漏）
/// 用于验证 allocmap 能否检测到持续增长的内存趋势
/// 以及在时序图中显示线性增长曲线
use std::time::Duration;

fn main() {
    println!(
        "[leak_linear] started, pid={}, leaking 10MB/sec",
        std::process::id()
    );

    let mut leaked: Vec<Vec<u8>> = Vec::new();
    let mut total_mb = 0u64;

    loop {
        // 每次分配10MB，故意不释放（模拟泄漏）
        let chunk = vec![0u8; 10 * 1024 * 1024];
        std::hint::black_box(&chunk);
        leaked.push(chunk);
        total_mb += 10;

        println!(
            "[leak_linear] total leaked: {}MB ({} chunks)",
            total_mb,
            leaked.len()
        );

        // 泄漏到1GB后停止（防止 OOM）
        if total_mb >= 1024 {
            println!("[leak_linear] reached 1GB limit, holding...");
            loop {
                std::thread::sleep(Duration::from_secs(60));
            }
        }

        std::thread::sleep(Duration::from_secs(1));
    }
}
