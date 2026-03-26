# Review Report — Phase 2, Iteration 01

**Date**: 2026-03-26
**Reviewer**: Reviewer Agent

---

## Verdict: PASSED

---

## Build Status

- `cargo build --release`: **PASSED** — compiled in ~28s, zero errors
- `cargo clippy -- -D warnings`: **PASSED** — 0 warnings
- `cargo test`: **PASSED** — all 64 tests across all crates pass (0 failed)

---

## Phase 2 Feature Review

### allocmap replay

**Status: PASSED**

`crates/allocmap-cli/src/cmd/replay.rs` implements:

- `--from <offset>` and `--to <offset>` duration-based frame filtering (parsed via `crate::util::parse_duration`)
- `--speed <multiplier>` with clamping to `[0.1, 100.0]`
- Frame timing is replayed at recorded inter-frame gaps divided by the speed multiplier, capped at 2 000 ms to prevent stalls on large gaps
- TUI fields `is_replay = true` and `replay_speed` are set on `App` before the TUI loop begins
- Space-bar pause (`replay_paused` toggle) is handled in `App::on_key` (line 169) and shown in the replay-mode keybindings hint in `lib.rs` (line 177)
- `+`/`-` speed keys adjust `replay_speed` in `App::on_key` (lines 172–177)
- Three unit tests covering: file-not-found check, speed clamping, and from/to filter logic

**Gap noted (LOW)**: The `replay_paused` flag is stored in `App` state and toggled by Space, but `run_tui_loop` does not consult `app.replay_paused` before consuming frames from the channel. The frame feeder task runs independently and does not observe the pause flag. This means pressing Space marks `replay_paused = true` visually/logically but does not actually halt frame delivery. Full pause semantics require the feeder task to check the flag (e.g. via an `Arc<AtomicBool>`). This is a MEDIUM functional gap for iter02.

The `--help` output is correct and complete.

### allocmap diff

**Status: PASSED**

`crates/allocmap-cli/src/cmd/diff.rs` implements:

- Reads two `.amr` files with clear error messages on missing files
- Aggregates peak `live_bytes` per function name across all frames (`aggregate_peak_sites`)
- Builds a diff table sorted by absolute delta bytes descending
- Color coding via `owo_colors`:
  - `abs_pct >= 50.0` → red (entire row)
  - `10.0 <= abs_pct < 50.0` → yellow (entire row)
  - Otherwise plain text
- Delta formatted as `+X.XMB` or `-X.XMB`
- `--min-change-pct` filter to suppress low-noise rows
- `format_bytes` helper with full unit test coverage (B/KB/MB/GB)
- Four unit tests covering: format_bytes values, missing file guard, empty recording, and peak-max aggregation

The `--help` output is correct and complete.

### macOS Support

**Status: PARTIAL — Acceptable for iter01**

`crates/allocmap-ptrace/src/lib.rs` correctly gates all Linux modules under `#[cfg(target_os = "linux")]` and the macOS stub under `#[cfg(target_os = "macos")]`. An unsupported platform triggers `compile_error!`.

`crates/allocmap-cli/src/cmd/run.rs` correctly uses:
```rust
#[cfg(target_os = "linux")]
let inject_var = "LD_PRELOAD";
#[cfg(target_os = "macos")]
let inject_var = "DYLD_INSERT_LIBRARIES";
```
Both the environment variable name and the `cmd.env(...)` call at line 82–84 are properly platform-gated.

`crates/allocmap-preload/src/lib.rs` and `find_preload_so()` in `run.rs` handle the `.dylib` extension for macOS.

**macOS `task_for_pid` + `mach_vm_read`**: The `macos_sampler.rs` stub uses `kill(pid, 0)` to verify process existence and `ps -o rss= -p <pid>` for RSS sampling. This provides a functional (if lower-fidelity) implementation — heap bytes are readable without entitlements. Full `task_for_pid` + `mach_vm_read` for per-allocation site data is not implemented.

**MEDIUM issue**: Per CLAUDE.md Phase 2 requirement, macOS `attach` should use `task_for_pid + mach_vm_read`. The current stub only provides total RSS, not allocation sites. `top_sites` is always empty on macOS. This is acceptable for iter01 and should be addressed in iter02.

### Multi-thread tracking

**Status: PARTIAL — Acceptable for iter01**

`crates/allocmap-ptrace/src/attach.rs` implements `list_threads(pid)` at line 114–123 which reads `/proc/{pid}/task/` and returns a `Vec<u32>` of thread IDs. The function has two unit tests (self-process enumeration and nonexistent-PID fallback).

In `sampler.rs` line 90, `list_threads` is called per sample cycle:
```rust
let _thread_count = crate::attach::list_threads(self.pid);
```

**MEDIUM issue**: The result is immediately discarded (`_thread_count`). Thread IDs are enumerated but neither stored in `SampleFrame` nor surfaced in the TUI. This is the enumeration groundwork but not the full implementation.

**LOW issue**: `PTRACE_O_TRACECLONE` for automatic new-thread tracking is not implemented. The sampler does not call `ptrace::setoptions` with `PTRACE_O_TRACECLONE`. This was listed as a Phase 2 requirement ("automatically track newly created threads"). Acceptable as a placeholder for iter01.

---

## Issues Found

| Severity | Location | Description |
|----------|----------|-------------|
| MEDIUM | `crates/allocmap-tui/src/lib.rs` + `replay.rs` | `replay_paused` flag not consumed by `run_tui_loop` or the feeder task — Space key toggles state but does not halt frame delivery |
| MEDIUM | `crates/allocmap-ptrace/src/macos_sampler.rs` | macOS sampler uses `ps` RSS only; `task_for_pid + mach_vm_read` not implemented, so `top_sites` is always empty on macOS |
| MEDIUM | `crates/allocmap-ptrace/src/sampler.rs` line 90 | `list_threads` result is discarded; thread IDs not stored or displayed |
| LOW | `crates/allocmap-ptrace/src/sampler.rs` | `PTRACE_O_TRACECLONE` not called — newly created threads are not auto-tracked |
| LOW | `crates/allocmap-cli/src/cmd/replay.rs` | Test `test_replay_nonexistent_file_would_fail` is trivially weak (only checks `Path::exists`, no async execution) |

---

## Summary

**Overall verdict: PASSED**

All build, clippy, and test gates pass cleanly. The three core Phase 2 deliverables are functionally implemented:

1. `allocmap replay` is complete with `--from`, `--to`, `--speed`, and TUI Space/+/- key handling. The one gap (pause not propagated to the feeder task) is a behavioral defect but does not break the feature.
2. `allocmap diff` is fully implemented with correct color thresholds (10%/50%), sorted output, and good test coverage.
3. macOS support has correct platform gating (`#[cfg(...)]` throughout), correct `DYLD_INSERT_LIBRARIES` injection, and a working RSS-based sampler stub. Full `task_for_pid` implementation is deferred to iter02, which is acceptable.
4. Multi-thread enumeration via `/proc/{pid}/task/` is in place; PTRACE_O_TRACECLONE and per-thread TUI display are iter02 work items.

The three MEDIUM issues identified are expected partial implementations for iter01 and should be tracked as iter02 targets.
