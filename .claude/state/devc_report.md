# Developer C Report — allocmap-preload LD_PRELOAD Library

**Status**: COMPLETE
**Date**: 2026-03-26
**Crate**: `crates/allocmap-preload/`

---

## Files Modified

### 1. `crates/allocmap-preload/src/bump_alloc.rs` (rewritten)

Replaced the static-array arena with an mmap-backed arena (8 MB). New public API:

- `init()` — mmap the arena once via `std::sync::Once`; idempotent
- `alloc(size: usize) -> *mut u8` — 8-byte-aligned bump allocation via atomic `fetch_add`; returns null on OOM
- `contains(ptr: *mut u8) -> bool` — checks whether a pointer lives inside the arena (used by `free` / `realloc` hooks)
- `used_bytes() -> usize` — diagnostic counter

Key design choice: mmap avoids BSS bloat; atomic `fetch_add` is lock-free.

### 2. `crates/allocmap-preload/src/ipc.rs` (extended)

Added:
- `AllocEvent` struct (`repr(C)`, wire-stable): `event_type: u8`, `address: u64`, `size: u64`, `timestamp_ms: u64`
- `send_event(event: &AllocEvent)` — zero-allocation send path using `try_lock` (drops event if mutex is contended, never blocks target process)
- `IPC_INIT: Once` guard so `init()` is idempotent even when called from both `allocmap_init` and `init_real_functions`
- Socket set to non-blocking mode to prevent target process stalls

Retained existing `send(data: &[u8])`, `is_connected()`.

### 3. `crates/allocmap-preload/src/hooks.rs` (rewritten)

Full malloc/free/calloc/realloc hooks with `#[no_mangle]` and `pub unsafe extern "C"` ABI:

- **Reentrancy guard**: per-thread `thread_local! { static IN_HOOK: Cell<bool> }` — correct for multi-threaded targets (atomic `AtomicBool` would have been process-global and caused false positives across threads)
- **dlsym resolution**: `init_real_functions()` via `Once`, resolves `REAL_MALLOC/FREE/CALLOC/REALLOC` via `libc::dlsym(RTLD_NEXT, ...)`
- **malloc**: falls back to bump allocator if reentrant or before init; otherwise calls real malloc + sends `AllocEvent(type=1)`
- **free**: skips bump-arena pointers; sends `AllocEvent(type=2, size=0)` (size unknown at free time)
- **calloc**: bump fallback with explicit `write_bytes` zero-fill; otherwise calls real calloc + sends alloc event
- **realloc**: handles null ptr (delegates to malloc), size=0 (delegates to free), bump-arena pointers (copy to new bump alloc), and normal case (models as free+alloc pair)
- `get_stats() -> HookStats` — lock-free counter snapshot

### 4. `crates/allocmap-preload/src/lib.rs` (updated)

- Updated `allocmap_init()` to call `bump_alloc::init()` and `ipc::init()`
- Added `pub use ipc::AllocEvent` re-export for `allocmap-cli` consumers
- Added 4 unit tests:
  - `test_bump_alloc_basic` — verifies non-null return and `contains()` membership
  - `test_bump_alloc_multiple` — verifies repeated allocation succeeds
  - `test_ipc_init_no_socket` — verifies graceful no-op when `ALLOCMAP_SOCKET_PATH` absent
  - `test_hook_stats_initial` — verifies `get_stats()` does not panic

---

## Safety Properties Maintained

| Constraint | How enforced |
|-----------|-------------|
| No malloc inside hooks | `IN_HOOK` cell checked first; bump allocator used as fallback |
| No String/Vec/format! in hooks | Only primitive types and `AllocEvent` (stack-allocated) used |
| No infinite recursion | Per-thread `Cell<bool>` guard (not process-global) |
| Target process never crashes | All errors silently ignored; null-check before every dereference |
| Thread safety | Atomic counters, `try_lock` for IPC, `Once` for init |

---

## Notes for Integration

- `AllocEvent` has 7 bytes of padding after `event_type` (u8 before u64). Receiver (allocmap-cli) must use the same `repr(C)` struct.
- Wire format: `[4-byte LE u32 length][AllocEvent bytes]` — matches the existing `ipc::send()` framing.
- `allocmap-core` remains in `Cargo.toml` but is not used by the new implementation (no harm; harmless dead dep).
