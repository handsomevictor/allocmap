use std::time::Duration;
use rand::Rng;

fn main() {
    println!("[spike_alloc] started pid={}", std::process::id());
    println!("[spike_alloc] random allocs 50MB-1GB, hold 2-8s each");
    let mut rng = rand::thread_rng();
    loop {
        match rng.gen::<u8>() % 4 {
            0 => function_small_alloc(),
            1 => function_medium_alloc(),
            2 => function_large_alloc(),
            _ => function_burst_alloc(),
        }
    }
}

/// Small allocation: 50-100 MB, hold 2-5 s
#[inline(never)]
fn function_small_alloc() {
    let mut rng = rand::thread_rng();
    let mb = rng.gen_range(50..=100) as usize;
    let secs = rng.gen_range(2..=5);
    println!("[spike_alloc] small_alloc: {}MB for {}s", mb, secs);
    let data: Vec<u8> = vec![1u8; mb * 1024 * 1024];
    std::hint::black_box(&data);
    std::thread::sleep(Duration::from_secs(secs));
    drop(data);
}

/// Medium allocation: 100-300 MB, hold 2-6 s
#[inline(never)]
fn function_medium_alloc() {
    let mut rng = rand::thread_rng();
    let mb = rng.gen_range(100..=300) as usize;
    let secs = rng.gen_range(2..=6);
    println!("[spike_alloc] medium_alloc: {}MB for {}s", mb, secs);
    let data: Vec<u8> = vec![2u8; mb * 1024 * 1024];
    std::hint::black_box(&data);
    std::thread::sleep(Duration::from_secs(secs));
    drop(data);
}

/// Large allocation: 300 MB-1 GB, hold 3-8 s
#[inline(never)]
fn function_large_alloc() {
    let mut rng = rand::thread_rng();
    let mb = rng.gen_range(300..=1024) as usize;
    let secs = rng.gen_range(3..=8);
    println!("[spike_alloc] large_alloc: {}MB for {}s", mb, secs);
    let data: Vec<u8> = vec![3u8; mb * 1024 * 1024];
    std::hint::black_box(&data);
    std::thread::sleep(Duration::from_secs(secs));
    drop(data);
}

/// Burst: many small allocations accumulating, hold 2-4 s
#[inline(never)]
fn function_burst_alloc() {
    let mut rng = rand::thread_rng();
    let count = rng.gen_range(5..=20);
    let secs = rng.gen_range(2..=4);
    println!("[spike_alloc] burst_alloc: {} x ~10MB for {}s", count, secs);
    let mut chunks: Vec<Vec<u8>> = Vec::with_capacity(count);
    for _ in 0..count {
        let mb = rng.gen_range(5..=20) as usize;
        chunks.push(vec![4u8; mb * 1024 * 1024]);
    }
    std::hint::black_box(&chunks);
    std::thread::sleep(Duration::from_secs(secs));
    drop(chunks);
}
