# Developer B — Phase 2 Iter 02 Report

## Files Modified

### Primary (owned files)
- `crates/allocmap-core/src/sample.rs` — Added `thread_count: u32` field to `SampleFrame`, `#[serde(default = "default_thread_count")]` for backward compat, `Default` derive
- `crates/allocmap-ptrace/src/sampler.rs` — Changed `_thread_count` to `thread_count = list_threads().len() as u32`, added `thread_count` to `SampleFrame` construction
- `crates/allocmap-ptrace/src/attach.rs` — Added `ptrace::setoptions(nix_pid, ptrace::Options::PTRACE_O_TRACECLONE)` in `PtraceAttach::attach` (best-effort, errors ignored)

### Additional (required to fix compilation)
- `crates/allocmap-ptrace/src/macos_sampler.rs` — Added `thread_count: 1` to `SampleFrame` construction
- `crates/allocmap-core/src/recording.rs` — Added `thread_count: 1` to two test `SampleFrame` constructions
- `crates/allocmap-cli/src/cmd/diff.rs` — Added `thread_count: 1` to two test `SampleFrame` constructions
- Note: `crates/allocmap-cli/src/cmd/replay.rs` and `crates/allocmap-tui/src/app.rs` were already updated by Developer A with `thread_count: 0`

## What Was Implemented

1. **`thread_count: u32` in `SampleFrame`**: New field with `#[serde(default)]` for backward-compatible deserialization of old `.amr` files. `Default` derive added so `..Default::default()` syntax works if needed.

2. **Ptrace sampler captures thread count**: `list_threads(pid)` result is now stored as `thread_count` and passed into each `SampleFrame`. This gives the TUI real-time thread count per sample.

3. **`PTRACE_O_TRACECLONE` in `PtraceAttach::attach`**: After successful attach and stop, calls `ptrace::setoptions` with `PTRACE_O_TRACECLONE` so the kernel automatically notifies the tracer when the target spawns new threads. Errors are silently ignored (best-effort).

4. **TUI stats bar**: The `thread_count` field is now present in every `SampleFrame` produced by the ptrace sampler. Developer A's TUI code in `app.rs` already reads this field (was pre-populated with `thread_count: 0` as a placeholder, now receives real values from the sampler).

## Build: PASSED

```
Compiling allocmap-core v0.1.0
Compiling allocmap-ptrace v0.1.0
Compiling allocmap-tui v0.1.0
Compiling allocmap-preload v0.1.0
Compiling allocmap v0.1.0 (allocmap-cli)
Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.20s
```

Zero warnings, zero errors.

## Tests: 67 passing / 0 failing

- allocmap-tui: 24 passed
- allocmap-cli: 5 passed
- allocmap-core: 3 passed
- allocmap-ptrace (attach): 4 passed
- allocmap-ptrace (all): 15 passed
- allocmap-cli (full): 16 passed
- All other crates: 0 tests (no regressions)
