# Developer Report — Phase 2 Iter 03: Per-Thread TUI Display

## Summary

Implemented per-thread TUI display as required by CLAUDE.md Phase 2: "TUI 中显示每个线程的内存使用（可切换视图）".

## Files Modified

### Core data model
- `crates/allocmap-core/src/sample.rs`
  - Added `thread_ids: Vec<u32>` field to `SampleFrame` (with `#[serde(default)]` for backwards compatibility)

### Sampler
- `crates/allocmap-ptrace/src/sampler.rs`
  - Populated `thread_ids` from `crate::attach::list_threads(self.pid)` in `sample()`
  - `thread_count` is now derived from `thread_ids.len()`

- `crates/allocmap-ptrace/src/macos_sampler.rs`
  - Added `thread_ids: vec![]` to `SampleFrame` construction

### TUI
- `crates/allocmap-tui/src/app.rs`
  - Added `Threads` variant to `DisplayMode` enum
  - Added `"threads"` case to `DisplayMode::parse()`
  - Added `KeyCode::Char('T')` handler in `on_key()` to switch to Threads mode
  - Updated test `make_frame()` helper to include `thread_ids: vec![]`
  - Added `test_display_mode_parse_threads` test
  - Updated `test_on_key_mode_switching` to cover `T` key → `DisplayMode::Threads`
  - Updated `test_display_mode_parse_valid` and `test_display_mode_parse_case_insensitive` to cover Threads

- `crates/allocmap-tui/src/lib.rs`
  - Stats bar now shows `THREADS: N` count from `frame.thread_count`
  - Added `DisplayMode::Threads` arm in the main content match → calls `render_threads_panel()`
  - Updated keybindings hint (both replay and live) to include `[T]threads`
  - Added `render_threads_panel()` function: shows a ratatui `Table` with TID and Role columns; marks the main thread (TID == app.pid) as "main", others as "worker"; falls back to a yellow warning if `thread_ids` is empty (old recordings)

### Test fixtures / other construction sites
- `crates/allocmap-core/src/recording.rs` — added `thread_ids: vec![]` to 2 test frames
- `crates/allocmap-cli/src/cmd/diff.rs` — added `thread_ids: vec![]` to 2 test frames
- `crates/allocmap-cli/src/cmd/replay.rs` — added `thread_ids: vec![]` to 1 test frame factory

## Build

PASSED — `cargo build` succeeded with no errors.

## Clippy

PASSED — `cargo clippy -- -D warnings` produced 0 warnings.

## Tests

PASSED — 68 tests passing, 0 failing across all workspace crates.

- allocmap-tui: 17 tests (including 3 new Threads-related tests)
- allocmap-ptrace: 15 tests
- allocmap-core: 5 tests
- allocmap-cli: 24 + 4 + 3 tests
