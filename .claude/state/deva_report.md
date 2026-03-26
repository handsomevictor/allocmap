# Developer A Report — iter01

## Status: COMPLETE

## Tasks Completed

### Task 1: Root Cargo.toml dependency fixes
- Added `libc = "0.2"` to `[workspace.dependencies]`
- Fixed `addr2line` to `{ version = "0.22", features = ["object"] }`
- Fixed `object` to `{ version = "0.36", features = ["read"] }`

### Task 2: .amr format read/write implementation
File: `crates/allocmap-core/src/recording.rs`

- Added `frame_count: u64` field to `RecordingHeader` to track number of frames
- Implemented `AllocMapRecording::write_to<W: Write>` with format:
  - 4-byte magic `AMR\0`
  - 4-byte LE version (u32)
  - 4-byte LE header JSON length + JSON bytes (with frame_count baked in)
  - For each frame: 4-byte LE bincode length + bincode bytes
  - 4-byte LE footer JSON length + JSON bytes
- Implemented `AllocMapRecording::read_from<R: Read>` which:
  - Verifies magic and version
  - Reads header (extracts frame_count)
  - Reads exactly frame_count frames
  - Reads footer

### Task 3: Unit tests added (3 tests)
1. `test_roundtrip_write_read` — Writes a recording with 2 frames and reads back, asserting all fields match
2. `test_invalid_magic_returns_error` — Bad magic bytes produce `CoreError::InvalidRecording`
3. `test_version_mismatch_returns_error` — Wrong version produces `CoreError::UnsupportedVersion { expected: 1, got: 99 }`

### Task 4: allocmap-preload Cargo.toml fix
- Removed `anyhow` dependency (dangerous in malloc hooks)
- Added `libc = { workspace = true }`

## Files Modified
- `/home/admin/allocmap/Cargo.toml`
- `/home/admin/allocmap/crates/allocmap-core/src/recording.rs`
- `/home/admin/allocmap/crates/allocmap-preload/Cargo.toml`

## Notes
- No other crates construct `RecordingHeader` directly, so adding `frame_count` is non-breaking
- All error messages are in English
- Error mapping follows the specified pattern: `CoreError::Io(e)` via `#[from]` and `CoreError::Serialization(e.to_string())`
- cargo was not available in this environment to run a live build check, but the code has been reviewed for correctness
