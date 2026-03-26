# Developer P2A Report — Phase 2 Iter 01

## Status: COMPLETE ✓

## Tasks Completed

### Task 1: CLI — Replay and Diff Commands Added
- **`crates/allocmap-cli/src/cli.rs`**: Added `Replay(replay::ReplayArgs)` and `Diff(diff::DiffArgs)` variants to `Commands` enum; updated `Cli::run()` to dispatch them.
- **`crates/allocmap-cli/src/cmd/mod.rs`**: Added `pub mod diff;` and `pub mod replay;`.

### Task 2: `crates/allocmap-cli/src/cmd/replay.rs` — Created
- `ReplayArgs`: `file`, `--from`, `--to`, `--speed` (default 1.0)
- Reads `.amr` file via `AllocMapRecording::read_from()`
- Filters frames by `from`/`to` time window (using `crate::util::parse_duration`)
- Spawns async frame feeder that replays frames at original timing ÷ speed
- Runs full TUI loop via `allocmap_tui::run_tui_loop()`
- 3 unit tests: nonexistent file, speed clamping, from/to filter logic

### Task 3: `crates/allocmap-cli/src/cmd/diff.rs` — Created
- `DiffArgs`: `baseline`, `current`, `--min-change-pct`
- Reads both `.amr` files, aggregates peak `live_bytes` per function name
- Builds diff table sorted by absolute delta
- Color output: red ≥50% change, yellow ≥10%, white otherwise
- 4 unit tests: format_bytes, nonexistent file, empty recording, peak aggregation
- Correctly uses `live_bytes` (not `bytes`) and `function: Option<String>` (not `name`)

### Task 4: `crates/allocmap-tui/src/app.rs` — Updated App struct
- Added fields: `is_replay: bool`, `replay_speed: f64`, `replay_paused: bool`
- Initialized in `App::new()` as `false`, `1.0`, `false`
- Added key handlers in `on_key()`: `Space` (pause/resume), `+`/`=` (2× speed), `-` (½ speed)

### Task 5: `crates/allocmap-tui/src/lib.rs` — Updated TUI
- Header now shows `(REPLAY ×{speed})` tag when `app.is_replay` is true
- Keybindings hint switches to replay-specific hints when in replay mode

## Pre-existing Fix
- **`crates/allocmap-ptrace/src/lib.rs`**: Removed blank line between doc comment and `#[cfg]` block to fix `clippy::empty_line_after_doc_comments` warning (pre-existing issue, not introduced by this dev).

## Build Results
- `cargo build`: **SUCCESS** (Finished dev profile)
- `cargo clippy -- -D warnings`: **0 warnings, 0 errors**
- `cargo test`: **All tests pass** (64+ tests across all crates, 0 failed)

## Files Created/Modified
- Created: `crates/allocmap-cli/src/cmd/replay.rs`
- Created: `crates/allocmap-cli/src/cmd/diff.rs`
- Modified: `crates/allocmap-cli/src/cli.rs`
- Modified: `crates/allocmap-cli/src/cmd/mod.rs`
- Modified: `crates/allocmap-tui/src/app.rs`
- Modified: `crates/allocmap-tui/src/lib.rs`
- Modified: `crates/allocmap-ptrace/src/lib.rs` (pre-existing clippy fix)
