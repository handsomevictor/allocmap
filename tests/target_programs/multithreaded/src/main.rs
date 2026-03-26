/// 测试目标程序：multithreaded
///
/// 模拟场景：4个线程并发分配/释放内存，各自有不同的分配模式
/// 用于验证 allocmap 能否正确追踪多线程场景

use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

fn main() {
    println!(
        "[multithreaded] started, pid={}, launching 4 worker threads",
        std::process::id()
    );

    let barrier = Arc::new(Barrier::new(5)); // 4个worker + 主线程

    let mut handles = Vec::new();

    // 线程1：大块分配，慢速
    {
        let b = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            b.wait();
            thread_large_slow(1);
        }));
    }

    // 线程2：小块分配，高频
    {
        let b = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            b.wait();
            thread_small_fast(2);
        }));
    }

    // 线程3：周期性大量分配
    {
        let b = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            b.wait();
            thread_periodic_burst(3);
        }));
    }

    // 线程4：稳定持有
    {
        let b = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            b.wait();
            thread_steady_hold(4);
        }));
    }

    barrier.wait();
    println!("[multithreaded] all threads started");

    for h in handles {
        h.join().unwrap();
    }
}

#[inline(never)]
fn thread_large_slow(id: u32) {
    loop {
        println!("[thread-{}] allocating 50MB...", id);
        let data = vec![id as u8; 50 * 1024 * 1024];
        std::hint::black_box(&data);
        thread::sleep(Duration::from_secs(5));
        println!("[thread-{}] releasing 50MB", id);
        drop(data);
        thread::sleep(Duration::from_secs(2));
    }
}

#[inline(never)]
fn thread_small_fast(id: u32) {
    loop {
        let data = vec![id as u8; 64 * 1024]; // 64KB
        std::hint::black_box(&data);
        thread::sleep(Duration::from_millis(100));
        drop(data);
    }
}

#[inline(never)]
fn thread_periodic_burst(id: u32) {
    loop {
        thread::sleep(Duration::from_secs(8));
        println!("[thread-{}] burst: allocating 100MB...", id);
        let data = vec![id as u8; 100 * 1024 * 1024];
        std::hint::black_box(&data);
        thread::sleep(Duration::from_secs(2));
        println!("[thread-{}] burst: releasing 100MB", id);
        drop(data);
    }
}

#[inline(never)]
fn thread_steady_hold(id: u32) {
    println!("[thread-{}] holding 20MB permanently", id);
    let _data = vec![id as u8; 20 * 1024 * 1024];
    std::hint::black_box(&_data);
    loop {
        thread::sleep(Duration::from_secs(60));
    }
}
