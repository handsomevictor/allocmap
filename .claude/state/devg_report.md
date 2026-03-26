# Developer G Report — Phase 1 Iter 02

**Status**: ALL TASKS COMPLETE

---

## Task 1: Dockerfile — Pre-install clippy and rustfmt

**File**: `/home/admin/allocmap/docker/Dockerfile`

Added `RUN rustup component add clippy rustfmt` after the system deps block and before `cargo install cargo-watch`. This ensures the tools are baked into the image so subsequent `cargo clippy` and `cargo fmt` calls work without network access.

---

## Task 2: Fix spawn_blocking JoinHandle

### attach.rs (`/home/admin/allocmap/crates/allocmap-cli/src/cmd/attach.rs`)
Changed the `tokio::task::spawn_blocking(...)` call to assign the returned handle to `_sampling_handle`. The handle is intentionally not awaited — when the TUI exits the handle is dropped, signaling the blocking thread to stop. This suppresses the unused-value warning and documents the intent.

### snapshot.rs (`/home/admin/allocmap/crates/allocmap-cli/src/cmd/snapshot.rs`)
Changed the `tokio::task::spawn_blocking(...)` call to store the handle as `sampling_handle`, then added `let _ = sampling_handle.await;` after the frame drain loop. This ensures any panic from the blocking thread surfaces cleanly and the thread is properly joined before building the JSON summary.

### run.rs (`/home/admin/allocmap/crates/allocmap-cli/src/cmd/run.rs`)
Changed the `tokio::task::spawn_blocking(...)` call inside the `#[cfg(target_os = "linux")]` block to assign the returned handle to `_sampling_handle`. Same rationale as attach.rs — the handle is dropped when the TUI exits, which is the correct signal mechanism.

---

## Task 3: Add `--mode` option to `run` command

**File**: `/home/admin/allocmap/crates/allocmap-cli/src/cmd/run.rs`

Added to `RunArgs`:
```rust
/// Display mode: timeline, hotspot, or flamegraph
#[arg(long, default_value = "timeline")]
pub mode: String,
```

Updated `execute()` to parse `args.mode` via `DisplayMode::parse` and pass it to `App::new_with_mode(...)` instead of the plain `App::new(...)`. This brings `run` into parity with `attach` for display mode selection.

---

## Task 4: Build Verification

- `cargo build`: **PASSED** — `Finished dev profile` with no errors
- `cargo clippy -- -D warnings`: **PASSED** — 0 warnings, 0 errors

---

**Completed**: 2026-03-26
