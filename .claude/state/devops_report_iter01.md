# DevOps Report — Phase 1 Iter 01

**Date**: 2026-03-26
**Agent**: DevOps
**Status**: SUCCESS

---

## Docker Installation

- Docker 26.1.5 installed via `apt-get install docker.io`
- Service started and enabled via systemd
- `hello-world` test passed
- User `admin` added to `docker` group

## Docker Image Build

- Image: `allocmap-dev:latest` built from `docker/Dockerfile`
- Base: `rust:latest` (Debian)
- Build time: ~3 minutes (includes cargo-watch install)
- Image size: 1.92 GB

## Missing Source Files Created

The existing skeleton references modules that did not exist. The following files were created as functional stubs:

### allocmap-ptrace
- `crates/allocmap-ptrace/src/attach.rs` — PTRACE_ATTACH/DETACH + PtraceAttach RAII wrapper
- `crates/allocmap-ptrace/src/sampler.rs` — PtraceSampler with SIGSTOP-based sampling
- `crates/allocmap-ptrace/src/backtrace.rs` — frame-pointer stack unwinding
- `crates/allocmap-ptrace/src/symbols.rs` — addr2line-based symbol resolution

### allocmap-tui
- `crates/allocmap-tui/src/app.rs` — App state struct, event handling
- `crates/allocmap-tui/src/timeline.rs` — Timeline chart renderer
- `crates/allocmap-tui/src/hotspot.rs` — Hotspot list renderer
- `crates/allocmap-tui/src/events.rs` — Terminal event polling

### allocmap-preload
- `crates/allocmap-preload/src/hooks.rs` — malloc/free/calloc/realloc hooks
- `crates/allocmap-preload/src/ipc.rs` — Unix socket IPC channel
- `crates/allocmap-preload/src/bump_alloc.rs` — mmap-backed bump allocator

## Dependency Fixes

- Added `libc = "0.2"` to workspace dependencies (required by allocmap-preload)
- Updated `addr2line` to include `features = ["object"]`
- Added `object` with `features = ["read"]`
- Used `addr2line::object::File` (version 0.35) to avoid version conflict with object 0.36

## Build Results

| Step | Result |
|------|--------|
| `cargo build` | PASSED |
| `cargo build --release` | PASSED (1m 24s) |
| `cargo clippy -- -D warnings` | PASSED (0 warnings) |
| `cargo test` | PASSED |

## Test Results

```
allocmap-core:   3 tests — all passed
allocmap-preload: 4 tests — all passed
allocmap-ptrace: 13 tests — all passed
Total: 20 tests, 0 failures
```

## Key clippy fixes applied

- Removed empty lines after doc comments (`///`) in multiple files
- Changed crate-level doc comments from `///` to `//!`
- Used `addr2line::object::File` to avoid version mismatch
- Fixed `transmute` calls to use typed aliases
- Used C-string literals (`c"malloc"`) instead of manual nul bytes
- Used `io::Error::other()` instead of `io::Error::new(Other, ...)`
- Changed `match` → `if let` where appropriate
- Removed identity `.map(|b| b)` calls
- Renamed `from_str` to `parse` to avoid confusion with `std::str::FromStr`

## Commands That Work Correctly

```bash
# Build the Docker image
sudo docker build -f docker/Dockerfile -t allocmap-dev /home/admin/allocmap

# Build the project
sudo docker run --rm --cap-add=SYS_PTRACE --security-opt seccomp:unconfined \
  -v /home/admin/allocmap:/workspace -w /workspace \
  allocmap-dev cargo build

# Run tests
sudo docker run --rm --cap-add=SYS_PTRACE --security-opt seccomp:unconfined \
  -v /home/admin/allocmap:/workspace -w /workspace \
  allocmap-dev cargo test

# Run clippy
sudo docker run --rm --cap-add=SYS_PTRACE --security-opt seccomp:unconfined \
  -v /home/admin/allocmap:/workspace -w /workspace \
  allocmap-dev bash -c "rustup component add clippy && cargo clippy -- -D warnings"
```

## Limitations / Known Issues

- The `allocmap attach` and `allocmap run` commands are functional stubs (they print a "not yet implemented" message). Full ptrace attach loop and TUI rendering will be implemented by Developer agents in the next iteration.
- The `cargo build --release` takes ~1.5 minutes in Docker due to the large number of dependencies.
