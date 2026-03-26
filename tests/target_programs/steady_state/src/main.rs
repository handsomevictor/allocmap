/// 测试目标程序：steady_state
///
/// 模拟场景：保持50MB固定内存占用，每秒分配并立即释放小块内存
/// 用于验证 allocmap 不会产生误报，稳定状态下时序图应为平线
use std::time::Duration;

fn main() {
    println!(
        "[steady_state] started, pid={}, holding 50MB, allocating/freeing 1MB/sec",
        std::process::id()
    );

    // 持有50MB固定内存
    let _baseline = vec![0u8; 50 * 1024 * 1024];
    std::hint::black_box(&_baseline);

    let mut tick = 0u64;
    loop {
        // 每秒分配1MB然后立即释放（正常的工作内存使用）
        {
            let temp = vec![0u8; 1024 * 1024];
            std::hint::black_box(&temp);
            // temp 在这里 drop
        }

        tick += 1;
        if tick.is_multiple_of(10) {
            println!("[steady_state] tick={}, memory stable at ~50MB", tick);
        }

        std::thread::sleep(Duration::from_secs(1));
    }
}
