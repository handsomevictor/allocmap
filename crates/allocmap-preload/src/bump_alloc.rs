/// A minimal bump allocator for use inside the LD_PRELOAD .so.
///
/// SAFETY: This module provides memory allocation that MUST NOT call the system
/// malloc/free (to avoid infinite recursion in our hooks). It uses a static
/// arena backed by mmap.
///
/// This is intentionally simple and does NOT support deallocation.
/// The arena is per-process and reset only on process exit.
use std::sync::atomic::{AtomicUsize, Ordering};

/// Size of the bump arena: 8MB via mmap
const BUMP_SIZE: usize = 8 * 1024 * 1024;

/// Base address of the mmap'd arena (0 = not initialized)
static BUMP_BASE: AtomicUsize = AtomicUsize::new(0);

/// Current allocation offset within the arena
static BUMP_OFFSET: AtomicUsize = AtomicUsize::new(0);

/// One-time initialization guard
static BUMP_INIT: std::sync::Once = std::sync::Once::new();

/// Initialize the bump allocator by mmap'ing the backing arena.
/// Safe to call multiple times; subsequent calls are no-ops.
pub fn init() {
    BUMP_INIT.call_once(|| {
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                BUMP_SIZE,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            )
        };
        if ptr != libc::MAP_FAILED {
            BUMP_BASE.store(ptr as usize, Ordering::SeqCst);
        }
        // If mmap fails we leave BUMP_BASE at 0; alloc() will return null.
    });
}

/// Allocate `size` bytes from the bump allocator (8-byte aligned).
/// Returns null if the arena is exhausted or not initialized.
/// Thread-safe via atomic fetch_add.
///
/// The returned memory lives for the process lifetime (no free).
pub fn alloc(size: usize) -> *mut u8 {
    // Align to 8 bytes
    let size = (size + 7) & !7;
    let base = BUMP_BASE.load(Ordering::SeqCst);
    if base == 0 {
        return std::ptr::null_mut();
    }

    let offset = BUMP_OFFSET.fetch_add(size, Ordering::SeqCst);
    if offset + size > BUMP_SIZE {
        // Best-effort rollback; we just waste some arena space on concurrent OOM
        BUMP_OFFSET.fetch_sub(size, Ordering::SeqCst);
        return std::ptr::null_mut();
    }

    (base + offset) as *mut u8
}

/// Returns true if `ptr` lives within the bump allocator arena.
/// Used in `free()` / `realloc()` to avoid passing bump-arena pointers
/// to the real allocator.
pub fn contains(ptr: *mut u8) -> bool {
    let base = BUMP_BASE.load(Ordering::Relaxed);
    if base == 0 {
        return false;
    }
    let used = BUMP_OFFSET.load(Ordering::Relaxed);
    let addr = ptr as usize;
    addr >= base && addr < base + used
}

/// Returns the number of bytes currently used in the bump arena.
pub fn used_bytes() -> usize {
    BUMP_OFFSET.load(Ordering::Relaxed)
}
