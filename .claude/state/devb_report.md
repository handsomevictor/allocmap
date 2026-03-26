# Developer B Report — Phase 1, Iter 01

**Crate**: `allocmap-ptrace`
**Status**: COMPLETE
**Date**: 2026-03-26

---

## Summary

Implemented and completed the ptrace-based memory sampler in `crates/allocmap-ptrace/src/`. All four source files have been reviewed, fixed, and augmented with unit tests.

---

## Files Modified

### `crates/allocmap-ptrace/src/attach.rs` (rewritten)

**Changes**:
- Added standalone free functions: `attach(pid: Pid)`, `detach(pid: Pid)`, `get_heap_bytes(pid: u32)`, `process_exists(pid: u32)` as specified.
- Fixed the Errno-to-i32 conversion bug: the original code used `e as i32` (invalid for a struct type) and tried to use `e` after it was consumed by the match arm. Fixed by using `if/else` with equality comparison (`e == nix::errno::Errno::EPERM`) and `libc::EPERM` for the numeric errno value.
- Added `libc` as a Linux-only dependency in `Cargo.toml` (was missing).
- Kept the `PtraceAttach` RAII struct that `sampler.rs` depends on, with `PtraceAttach::attach()` calling the free `attach()` function.
- Added 4 unit tests: `process_exists` for current PID, `process_exists` for non-existent PID, `get_heap_bytes` success, `get_heap_bytes` failure.

### `crates/allocmap-ptrace/src/backtrace.rs` (rewritten)

**Changes**:
- Added standalone `collect_backtrace(pid: Pid, max_frames: usize) -> Result<Vec<u64>, PtraceError>` function for frame-pointer unwinding via `ptrace::getregs` + `ptrace::read`.
- Added `read_u64_remote` helper that uses `ptrace::read` (returns `nix::Result<i64>`, cast to `u64`).
- `BacktraceCapture` struct now delegates to `collect_backtrace` and uses `SymbolResolver` for IP-to-frame translation.
- Added `/proc/<pid>/mem` based `read_u64_from_proc` as an alternative read method.
- Added 3 unit tests: compile-only construction test, error on non-existent PID, error path for `read_u64_remote`.

### `crates/allocmap-ptrace/src/symbols.rs` (augmented)

**Changes**:
- Added 3 unit tests: construction with empty cache, result caching verification, fallback for unknown addresses.
- No functional changes (existing implementation was correct).

### `crates/allocmap-ptrace/src/sampler.rs` (augmented)

**Changes**:
- Fixed `get_heap_bytes(self.pid).map(|b| b).unwrap_or(0)` → `get_heap_bytes(self.pid).unwrap_or(0)` (removes clippy warning about identity map).
- Added 3 unit tests: `SamplerConfig::default()` validation, sample interval calculation, attach failure on non-existent PID.

### `crates/allocmap-ptrace/Cargo.toml` (modified)

**Changes**:
- Added `libc = { workspace = true }` under `[target.'cfg(target_os = "linux")'.dependencies]` to support `libc::EPERM` constant in attach.rs.

---

## Key Design Decisions

1. **Errno handling**: Used if/else equality comparison instead of match arms to avoid Errno-to-i32 cast issues.
2. **ptrace vs /proc/mem**: `collect_backtrace` uses `ptrace::read` for frame-pointer unwinding (process must be stopped first). The `BacktraceCapture::read_u64_from_proc` method offers `/proc/mem` as an alternative.
3. **Symbol resolution**: `symbols.rs` uses `addr2line::object::File::parse` + `addr2line::Context::new` for full debug-info resolution with a per-instance cache. Falls back to binary basename for stripped binaries.
4. **x86_64 only**: `collect_backtrace` is gated on `#[cfg(target_arch = "x86_64")]` and returns a clear error on other architectures.

---

## Unit Test Coverage

| File | Tests Added |
|------|-------------|
| `attach.rs` | 4 tests (process_exists × 2, get_heap_bytes × 2) |
| `backtrace.rs` | 3 tests (construction, invalid PID error × 2) |
| `symbols.rs` | 3 tests (construction, caching, fallback) |
| `sampler.rs` | 3 tests (config default, interval calc, attach failure) |

---

## Known Limitations (for iter01)

- Frame-pointer unwinding requires the target binary to have been compiled with frame pointers (`-fno-omit-frame-pointer` or debug mode). Release builds without frame pointers will yield shallow stacks.
- `ptrace::read` may fail on hardened kernels with strict ptrace_scope settings (user documentation covers this).
- Symbol resolution uses `addr2line` via the library API; no shell-out fallback is needed as the crate-based approach works for binaries with DWARF debug info.
