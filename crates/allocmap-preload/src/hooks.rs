/// malloc/free/calloc/realloc hook implementations for LD_PRELOAD injection.
///
/// # Critical Safety Rules
/// 1. These hooks intercept ALL allocator calls in the target process.
///    We MUST NOT call the standard allocator inside a hook — doing so causes
///    infinite recursion and stack overflow.
/// 2. No `String`, `Vec`, `Box`, `format!`, `println!`, or `eprintln!` inside
///    hook code paths. All internal memory comes from `bump_alloc`.
/// 3. Per-thread reentrancy guard (`IN_HOOK`) prevents recursive invocation.
/// 4. Real function pointers are resolved once via `dlsym(RTLD_NEXT, ...)`.
use std::cell::Cell;
use std::os::raw::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Once;

use libc::size_t;

use crate::bump_alloc;
use crate::ipc::{self, AllocEvent};

// ---------------------------------------------------------------------------
// Per-thread reentrancy guard
// ---------------------------------------------------------------------------

thread_local! {
    /// Set to `true` while we are inside a hook body.
    /// Prevents recursive hook invocations.
    static IN_HOOK: Cell<bool> = const { Cell::new(false) };
}

// ---------------------------------------------------------------------------
// Real function pointers (resolved once via dlsym)
// ---------------------------------------------------------------------------

type MallocFn = unsafe extern "C" fn(size_t) -> *mut c_void;
type FreeFn = unsafe extern "C" fn(*mut c_void);
type ReallocFn = unsafe extern "C" fn(*mut c_void, size_t) -> *mut c_void;
type CallocFn = unsafe extern "C" fn(size_t, size_t) -> *mut c_void;

static mut REAL_MALLOC: Option<MallocFn> = None;
static mut REAL_FREE: Option<FreeFn> = None;
static mut REAL_REALLOC: Option<ReallocFn> = None;
static mut REAL_CALLOC: Option<CallocFn> = None;

static INIT_ONCE: Once = Once::new();

/// Resolve the real allocator symbols exactly once.
///
/// # Safety
/// Writes to static muts, but protected by `Once`.
unsafe fn init_real_functions() {
    INIT_ONCE.call_once(|| {
        bump_alloc::init();
        ipc::init();

        // SAFETY: dlsym returns a function pointer or null; we validate before use.
        REAL_MALLOC = {
            let ptr = libc::dlsym(libc::RTLD_NEXT, c"malloc".as_ptr().cast());
            if ptr.is_null() { None } else { Some(std::mem::transmute::<*mut c_void, MallocFn>(ptr)) }
        };
        REAL_FREE = {
            let ptr = libc::dlsym(libc::RTLD_NEXT, c"free".as_ptr().cast());
            if ptr.is_null() { None } else { Some(std::mem::transmute::<*mut c_void, FreeFn>(ptr)) }
        };
        REAL_REALLOC = {
            let ptr = libc::dlsym(libc::RTLD_NEXT, c"realloc".as_ptr().cast());
            if ptr.is_null() { None } else { Some(std::mem::transmute::<*mut c_void, ReallocFn>(ptr)) }
        };
        REAL_CALLOC = {
            let ptr = libc::dlsym(libc::RTLD_NEXT, c"calloc".as_ptr().cast());
            if ptr.is_null() { None } else { Some(std::mem::transmute::<*mut c_void, CallocFn>(ptr)) }
        };
    });
}

// ---------------------------------------------------------------------------
// Statistics (atomic counters, no allocation)
// ---------------------------------------------------------------------------

static TOTAL_ALLOCS: AtomicU64 = AtomicU64::new(0);
static TOTAL_FREES: AtomicU64 = AtomicU64::new(0);
static LIVE_BYTES: AtomicU64 = AtomicU64::new(0);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Return a monotonic timestamp in milliseconds without allocating.
fn get_timestamp_ms() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    (ts.tv_sec as u64) * 1_000 + (ts.tv_nsec as u64) / 1_000_000
}

// ---------------------------------------------------------------------------
// Hook implementations
// ---------------------------------------------------------------------------

/// Hook for `malloc`.
///
/// # Safety
/// Replaces the system malloc; callers must follow standard malloc contract.
#[no_mangle]
pub unsafe extern "C" fn malloc(size: size_t) -> *mut c_void {
    init_real_functions();

    let in_hook = IN_HOOK.with(Cell::get);
    // SAFETY: REAL_MALLOC is only written inside INIT_ONCE, so once set it
    // is safe to read (happens-before by Once semantics).
    if in_hook || unsafe { REAL_MALLOC }.is_none() {
        return bump_alloc::alloc(size).cast::<c_void>();
    }

    IN_HOOK.with(|h| h.set(true));

    // SAFETY: checked above that REAL_MALLOC is Some.
    let ptr = unsafe { REAL_MALLOC.unwrap()(size) };

    if !ptr.is_null() {
        TOTAL_ALLOCS.fetch_add(1, Ordering::Relaxed);
        LIVE_BYTES.fetch_add(size as u64, Ordering::Relaxed);

        ipc::send_event(&AllocEvent {
            event_type: 1,
            address: ptr as u64,
            size: size as u64,
            timestamp_ms: get_timestamp_ms(),
        });
    }

    IN_HOOK.with(|h| h.set(false));
    ptr
}

/// Hook for `free`.
///
/// # Safety
/// Replaces the system free; callers must follow standard free contract.
#[no_mangle]
pub unsafe extern "C" fn free(ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }
    if bump_alloc::contains(ptr.cast::<u8>()) {
        return;
    }

    init_real_functions();

    let in_hook = IN_HOOK.with(Cell::get);
    if !in_hook {
        IN_HOOK.with(|h| h.set(true));

        TOTAL_FREES.fetch_add(1, Ordering::Relaxed);
        // Note: we cannot reliably decrement LIVE_BYTES here because the standard
        // allocator does not provide the allocation size via the free() API.
        // LIVE_BYTES is best-effort; use /proc/pid/status for accurate heap size.
        ipc::send_event(&AllocEvent {
            event_type: 2,
            address: ptr as u64,
            size: 0,
            timestamp_ms: get_timestamp_ms(),
        });

        IN_HOOK.with(|h| h.set(false));
    }

    // SAFETY: REAL_FREE is set during init; ptr is not null and not from bump arena.
    if let Some(real_free) = unsafe { REAL_FREE } {
        real_free(ptr);
    }
}

/// Hook for `calloc`.
///
/// # Safety
/// Replaces the system calloc; callers must follow standard calloc contract.
#[no_mangle]
pub unsafe extern "C" fn calloc(nmemb: size_t, size: size_t) -> *mut c_void {
    init_real_functions();

    let in_hook = IN_HOOK.with(Cell::get);
    // SAFETY: REAL_CALLOC written once by INIT_ONCE.
    if in_hook || unsafe { REAL_CALLOC }.is_none() {
        let total = nmemb.saturating_mul(size);
        let ptr = bump_alloc::alloc(total);
        if !ptr.is_null() {
            std::ptr::write_bytes(ptr, 0, total);
        }
        return ptr.cast::<c_void>();
    }

    IN_HOOK.with(|h| h.set(true));

    // SAFETY: checked above.
    let ptr = unsafe { REAL_CALLOC.unwrap()(nmemb, size) };

    if !ptr.is_null() {
        let total = nmemb as u64 * size as u64;
        TOTAL_ALLOCS.fetch_add(1, Ordering::Relaxed);
        LIVE_BYTES.fetch_add(total, Ordering::Relaxed);

        ipc::send_event(&AllocEvent {
            event_type: 1,
            address: ptr as u64,
            size: total,
            timestamp_ms: get_timestamp_ms(),
        });
    }

    IN_HOOK.with(|h| h.set(false));
    ptr
}

/// Hook for `realloc`.
///
/// # Safety
/// Replaces the system realloc; callers must follow standard realloc contract.
#[no_mangle]
pub unsafe extern "C" fn realloc(ptr: *mut c_void, size: size_t) -> *mut c_void {
    if ptr.is_null() {
        return unsafe { malloc(size) };
    }
    if size == 0 {
        unsafe { free(ptr) };
        return std::ptr::null_mut();
    }

    if bump_alloc::contains(ptr.cast::<u8>()) {
        let new_ptr = bump_alloc::alloc(size);
        if !new_ptr.is_null() {
            // SAFETY: Bump allocations are only used for internal hook metadata
            // (small structures). We cap the copy at BUMP_MAX_COPY_BYTES to avoid
            // reading past the original allocation whose size we do not track.
            const BUMP_MAX_COPY_BYTES: usize = 4096;
            let copy_len = size.min(BUMP_MAX_COPY_BYTES);
            std::ptr::copy_nonoverlapping(ptr.cast::<u8>(), new_ptr, copy_len);
        }
        return new_ptr.cast::<c_void>();
    }

    init_real_functions();

    let in_hook = IN_HOOK.with(Cell::get);
    // SAFETY: REAL_REALLOC written once by INIT_ONCE.
    if in_hook || unsafe { REAL_REALLOC }.is_none() {
        return ptr;
    }

    IN_HOOK.with(|h| h.set(true));

    // SAFETY: checked above.
    let new_ptr = unsafe { REAL_REALLOC.unwrap()(ptr, size) };

    if !new_ptr.is_null() {
        let ts = get_timestamp_ms();
        ipc::send_event(&AllocEvent {
            event_type: 2,
            address: ptr as u64,
            size: 0,
            timestamp_ms: ts,
        });

        TOTAL_ALLOCS.fetch_add(1, Ordering::Relaxed);
        LIVE_BYTES.fetch_add(size as u64, Ordering::Relaxed);

        ipc::send_event(&AllocEvent {
            event_type: 1,
            address: new_ptr as u64,
            size: size as u64,
            timestamp_ms: ts,
        });
    }

    IN_HOOK.with(|h| h.set(false));
    new_ptr
}

// ---------------------------------------------------------------------------
// Statistics helpers
// ---------------------------------------------------------------------------

/// Snapshot of hook counters.
#[derive(Debug, Clone)]
pub struct HookStats {
    pub total_allocs: u64,
    pub total_frees: u64,
    pub live_bytes: u64,
}

/// Return current hook counters without allocating.
pub fn get_stats() -> HookStats {
    HookStats {
        total_allocs: TOTAL_ALLOCS.load(Ordering::Relaxed),
        total_frees: TOTAL_FREES.load(Ordering::Relaxed),
        live_bytes: LIVE_BYTES.load(Ordering::Relaxed),
    }
}
