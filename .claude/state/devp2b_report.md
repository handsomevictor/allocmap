# Developer P2B Report — Phase 2 Iter 01

## Status: COMPLETE

## Tasks Completed

### Task 1: macOS preload support in allocmap-cli/src/cmd/run.rs

- Updated `find_preload_so()` to use `liballocmap_preload.so` on Linux and `liballocmap_preload.dylib` on macOS, with a fallback for other platforms.
- Updated the env injection: sets `LD_PRELOAD` on Linux, `DYLD_INSERT_LIBRARIES` on macOS.
- Removed the old `#[cfg(not(target_os = "linux"))]` bail that blocked macOS from using `run`.
- Updated log messages that hard-coded "LD_PRELOAD mode" to show the actual env var name.

**File modified:** `crates/allocmap-cli/src/cmd/run.rs`

### Task 2: macOS attach stub in allocmap-ptrace

- Replaced the `compile_error!` for non-Linux in `lib.rs` with a macOS stub module and an unsupported-platform `compile_error!`.
- Created `crates/allocmap-ptrace/src/macos_sampler.rs` with:
  - `MacosSamplerError` enum (ProcessNotFound, OsError) implementing `std::error::Error`.
  - `PtraceSampler` struct with the same public interface as the Linux sampler (attach, sample, sample_interval, pid, sample_count).
  - `get_rss()` using `ps -o rss= -p <pid>` as a Phase 2 starting point.
  - Three unit tests: attach_self, attach_nonexistent_fails, sample_interval_default.
- Added `libc` as a macOS-only dependency in `allocmap-ptrace/Cargo.toml`.
- Updated `lib.rs` to re-export `PtraceSampler` and `MacosSamplerError as PtraceError` under `#[cfg(target_os = "macos")]`.

**Files modified/created:**
- `crates/allocmap-ptrace/src/lib.rs`
- `crates/allocmap-ptrace/src/macos_sampler.rs` (new)
- `crates/allocmap-ptrace/Cargo.toml`

### Task 3: Multi-thread tracking in Linux sampler

- Added `list_threads(pid: u32) -> Vec<u32>` to `crates/allocmap-ptrace/src/attach.rs`. Reads `/proc/{pid}/task/` and returns all TIDs; falls back to `vec![pid]` on error.
- Added two new unit tests for `list_threads`: `test_list_threads_self` and `test_list_threads_nonexistent_falls_back_to_pid`.
- Updated `sampler.rs` to call `list_threads` as a placeholder comment for the future per-thread TUI view.

**Files modified:**
- `crates/allocmap-ptrace/src/attach.rs`
- `crates/allocmap-ptrace/src/sampler.rs`

### Task 4: Build Verification

- `cargo build`: **PASSED** (Finished dev profile, 0 errors)
- `cargo clippy -- -D warnings`: **PASSED** (0 warnings)
- `cargo test`: **PASSED** (all tests pass — 64 total across all crates)

## No regressions introduced. All macOS code is strictly cfg-gated.
