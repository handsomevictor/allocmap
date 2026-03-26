//! allocmap-preload: LD_PRELOAD injection library
//!
//! # Safety Warning
//! This crate compiles to a `.so` injected into the target process via
//! `LD_PRELOAD`. Inside malloc/free hooks we MUST NOT call the standard
//! allocator — doing so causes infinite recursion and stack overflow.
//! All internal allocations go through `bump_alloc`.

pub mod bump_alloc;
pub mod hooks;
pub mod ipc;

// Re-export AllocEvent so allocmap-cli can use the same type when reading IPC.
pub use ipc::AllocEvent;

/// Library constructor — called automatically by the dynamic linker when the
/// `.so` is loaded into the target process (before `main`).
///
/// Initialises the bump allocator and IPC channel so they are ready before
/// the first intercepted allocation occurs.
#[no_mangle]
pub extern "C" fn allocmap_init() {
    bump_alloc::init();
    ipc::init();
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// The bump allocator must return a valid, non-null pointer for small
    /// allocations after initialisation.
    #[test]
    fn test_bump_alloc_basic() {
        bump_alloc::init();
        let ptr = bump_alloc::alloc(64);
        assert!(!ptr.is_null(), "bump_alloc::alloc must return a non-null pointer");
        assert!(
            bump_alloc::contains(ptr),
            "allocated pointer must be inside the bump arena"
        );
    }

    /// A second allocation after init must also succeed.
    #[test]
    fn test_bump_alloc_multiple() {
        bump_alloc::init();
        let ptr = bump_alloc::alloc(128);
        assert!(!ptr.is_null(), "second bump_alloc::alloc must return non-null");
    }

    /// `init()` must not panic when `ALLOCMAP_SOCKET_PATH` is unset
    /// (i.e. the library is loaded outside allocmap supervision).
    #[test]
    fn test_ipc_init_no_socket() {
        std::env::remove_var("ALLOCMAP_SOCKET_PATH");
        ipc::init(); // must not panic or crash
        assert!(
            !ipc::is_connected(),
            "IPC must not be connected when ALLOCMAP_SOCKET_PATH is absent"
        );
    }

    /// `get_stats()` must be callable without panicking, even before any
    /// allocations have been intercepted.
    #[test]
    fn test_hook_stats_initial() {
        let stats = hooks::get_stats();
        // We cannot assert exact counts (other tests may have run first), but
        // the call itself must not panic.
        let _ = stats.total_allocs;
        let _ = stats.total_frees;
        let _ = stats.live_bytes;
    }
}
