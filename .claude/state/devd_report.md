# Developer D Report — allocmap-tui TUI Implementation

**Date**: 2026-03-26
**Task**: Implement TUI crates/allocmap-tui/ using ratatui 0.28

## Status: COMPLETE

## Files Modified

### 1. `crates/allocmap-tui/Cargo.toml`
- Added `tokio = { workspace = true }` dependency (required for `run_tui_loop` async fn)

### 2. `crates/allocmap-tui/src/events.rs`
- Rewrote to define `AppEvent` enum (`Key`, `Resize`, `Tick`)
- `poll_event(timeout: Duration) -> Result<Option<AppEvent>>` using anyhow
- Filters to press-only key events using `KeyEventKind::Press`

### 3. `crates/allocmap-tui/src/app.rs`
- Rewrote with correct field names matching allocmap-core types:
  - `frames: VecDeque<SampleFrame>` (ring buffer, max 500 frames)
  - `total_samples: u64` (was `sample_count` in old version)
  - `hotspot_expanded: Vec<bool>` (auto-grows with push_frame)
  - `start_time: Instant`
- Added `DisplayMode::from_str()` for CLI integration
- Added `new_with_mode()` constructor
- Added methods: `current_heap_bytes()`, `current_alloc_rate()`, `current_free_rate()`, `growth_rate_bytes_per_sec()`
- `on_event()` / `on_key()` with Ctrl+C quit support

### 4. `crates/allocmap-tui/src/timeline.rs`
- Added public `format_bytes(u64) -> String` function (used by lib.rs)
- Updated `render_timeline` to work with `VecDeque<SampleFrame>` using `.iter().skip()`
- Uses `app.total_samples` (not the removed `sample_count`)
- Uses `app.growth_rate_bytes_per_sec()` for color selection
- Output: block-character bar chart + stats label row

### 5. `crates/allocmap-tui/src/hotspot.rs`
- No changes needed — already correctly uses `site.live_bytes`, `site.alloc_count`, `site.frames`, `frame.display_name()`, `app.hotspot_expanded`, `app.top_n`, `app.latest_frame()`

### 6. `crates/allocmap-tui/src/lib.rs`
- Updated module declarations and re-exports
- Added: `init_terminal()`, `restore_terminal()`, `install_panic_hook()`
- Added: `async fn run_tui_loop(app, terminal, rx, duration)` — the main event loop:
  - Drains `mpsc::Receiver<SampleFrame>` each tick (non-blocking)
  - Renders 4-zone layout: header block / stats bar / main content / keybindings hint
  - Main content switches between Timeline, Hotspot, Flamegraph views
  - Polls events with 100ms timeout; yields 16ms per iteration (~60fps target)
  - Exits on `app.should_quit` or when `duration` elapses

## API Notes for CLI Integration

```rust
use allocmap_tui::{App, DisplayMode, init_terminal, restore_terminal, install_panic_hook, run_tui_loop};
use tokio::sync::mpsc;
use allocmap_core::SampleFrame;

// In async fn execute():
install_panic_hook();
let mut terminal = init_terminal()?;
let (tx, mut rx) = mpsc::channel::<SampleFrame>(256);
let mut app = App::new(pid, program_name, top_n);
app.mode = DisplayMode::from_str(&args.mode);
// spawn sampler sending to tx ...
run_tui_loop(&mut app, &mut terminal, &mut rx, duration).await?;
restore_terminal(&mut terminal)?;
```

## Known Constraints
- Cargo is not available on the EC2 host — compilation must be done inside Docker
- Build verification must be performed by the DevOps agent in the Docker container
- `theme.rs` uses associated functions (not instance methods) — all calls use `Theme::xxx()` syntax
