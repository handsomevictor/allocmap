# Developer E Report — Phase 1 Iter 01

**Developer**: Dev-E (allocmap-cli command executors)
**Completed**: 2026-03-26

---

## Summary

Implemented all three CLI command executors and the duration parsing utility in `crates/allocmap-cli/`.

---

## Files Created / Modified

### New Files

1. **`crates/allocmap-cli/src/util.rs`**
   - `parse_duration(s: &str) -> Result<Duration>` utility
   - Supports suffixes: `s` (seconds), `m` (minutes), `h` (hours), and plain integers
   - Includes 7 unit tests (3 valid formats, plain int, 2 invalid cases, empty)

### Modified Files

2. **`crates/allocmap-cli/src/main.rs`**
   - Added `mod util;` declaration

3. **`crates/allocmap-cli/src/cmd/attach.rs`**
   - Full implementation of `AttachArgs` struct and `execute()` function
   - Validates PID exists via `/proc/{pid}`
   - Reads program name from `/proc/{pid}/comm`
   - Spawns `PtraceSampler` in a `spawn_blocking` task (sampling loop with `sample()` + `sleep`)
   - `--output` path: non-interactive JSON collection mode
   - Default: interactive TUI via `allocmap_tui::run_tui_loop`
   - `--record` path: saves `.amr` recording after TUI exits
   - All ptrace calls guarded by `#[cfg(target_os = "linux")]`

4. **`crates/allocmap-cli/src/cmd/snapshot.rs`**
   - Full implementation of `SnapshotArgs` struct and `execute()` function
   - Parses `duration` string (required, default `5s`)
   - Runs sampling loop in `spawn_blocking`, collects frames for the duration
   - Builds JSON summary: pid, sample_count, duration_ms, peak_heap_bytes, avg_heap_bytes, top_sites, frames
   - Outputs to stdout or `--output` file

5. **`crates/allocmap-cli/src/cmd/run.rs`**
   - Full implementation of `RunArgs` struct (added `duration` field) and `execute()` function
   - Validates `--env` format (`KEY=VALUE`)
   - Finds `liballocmap_preload.so` via `find_preload_so()` (checks exe dir, `../lib/`, `target/debug/`, `target/release/`)
   - Creates Unix socket at `/tmp/allocmap-{pid}.sock`
   - Spawns child process with `LD_PRELOAD` and `ALLOCMAP_SOCKET_PATH` set
   - Falls back to ptrace-based sampling for TUI data (with graceful error if ptrace unavailable)
   - Supports `--output` (JSON) and `--record` (`.amr`) modes
   - Cleans up socket file on exit

---

## API Adaptation Notes

The actual implementations deviated from the architect's pseudocode in several ways. The real APIs were used:

- `PtraceSampler::attach(pid: u32)` — one argument only (no `sample_rate_hz` parameter)
- `PtraceSampler` has `sample()` and `sample_interval()` but no `run_sampling_loop()` or `detach()`. Implemented a manual `loop { sample(); sleep(interval) }` in `spawn_blocking`.
- `App::new(pid, program_name, top_n)` — three args; `App::new_with_mode()` used for mode setting
- `RecordingFooter` uses `end_time_ms` and `total_frames` (not `duration_ms` and `total_samples`)
- `SnapshotArgs.duration` is `String` (not `Option<String>`) — no `.as_deref()` needed
- `RunArgs.env_vars` (not `.env`) — adapted field name

---

## Key Design Decisions

1. **Sampling in `spawn_blocking`**: `PtraceSampler` is not `Send + Sync` (contains `Instant`, raw nix types). Using `tokio::task::spawn_blocking` keeps the blocking ptrace calls off the async executor thread.

2. **`run.rs` ptrace fallback**: Since the LD_PRELOAD IPC is not yet fully integrated end-to-end (the preload lib writes to the socket but the CLI socket reader is not wired into the TUI channel yet), ptrace is used as a data source. This gives a working TUI for iter01.

3. **Non-interactive JSON mode**: Both `attach` and `run` support `--output -` for stdout, making them CI-friendly without needing `snapshot`.

---

## Status

- All code written, no compile errors expected based on reading actual API signatures
- Cannot verify via `cargo build` (cargo not installed on EC2 host; must run in Docker)
- All error messages are in English
- No `.unwrap()` in production code paths

---

## Tests Present

`util.rs` contains 7 unit tests covering `parse_duration`.
The three command executors do not have isolated unit tests in this PR (integration tests are a separate task).
