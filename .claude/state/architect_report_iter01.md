# Architect Report - Phase 1 Iter 01

Generated: 2026-03-26

---

## Current State Assessment

### What's Implemented

**allocmap-core** (COMPLETE — no changes needed for iter01)
- `SampleFrame`, `AllocationSite`, `StackFrame` structs with correct fields and serde derives
- `AllocMapRecording`, `RecordingHeader`, `RecordingFooter` structs
- `AMR_MAGIC` and `AMR_VERSION` constants
- `CoreError` with `thiserror` (InvalidRecording, UnsupportedVersion, Io, Serialization)
- `StackFrame::display_name()` helper
- NOTE: The `recording.rs` file defines the data structs but has NO read/write logic for the `.amr` binary format. The actual serialization (magic bytes, version u32, JSON header, bincode frames, JSON footer) is absent.

**allocmap-ptrace** (SKELETON ONLY — critical modules missing)
- `lib.rs`: declares 5 modules under `#[cfg(target_os = "linux")]`, non-Linux compile_error
- `error.rs`: `PtraceError` enum with good, user-friendly error messages
- MISSING FILES (declared in lib.rs but no corresponding .rs file):
  - `src/attach.rs` — ptrace attach/detach logic
  - `src/sampler.rs` — `PtraceSampler` struct and sampling loop
  - `src/backtrace.rs` — stack unwinding via ptrace
  - `src/symbols.rs` — addr2line/gimli symbol resolution

**allocmap-preload** (SKELETON ONLY — all modules missing)
- `lib.rs`: declares 3 modules, `allocmap_init()` stub with TODO
- MISSING FILES (declared in lib.rs but no corresponding .rs file):
  - `src/hooks.rs` — `malloc`/`free`/`realloc`/`calloc` interception via `LD_PRELOAD`
  - `src/ipc.rs` — Unix socket IPC to send data back to allocmap-cli
  - `src/bump_alloc.rs` — internal allocator (must NOT use standard allocator to prevent recursion)

**allocmap-tui** (SKELETON ONLY — most modules missing)
- `lib.rs`: declares 5 modules
- `theme.rs`: `Theme` struct with all color helpers — COMPLETE
- MISSING FILES (declared in lib.rs but no corresponding .rs file):
  - `src/app.rs` — `App` struct (main TUI state machine)
  - `src/timeline.rs` — timeline widget (braille blocks line chart)
  - `src/hotspot.rs` — hotspot list widget
  - `src/events.rs` — keyboard event handling (q/f/t/s/↑↓/Enter)

**allocmap-cli** (SKELETON — all commands are stubs)
- `main.rs`: `#[tokio::main]` async entry, parses CLI and dispatches — COMPLETE
- `cli.rs`: `Cli` struct + `Commands` enum for Attach/Run/Snapshot — COMPLETE
- `error.rs`: `print_error_and_exit()` helper — COMPLETE
- `cmd/attach.rs`: `AttachArgs` struct with all required fields, `execute()` is a TODO stub
- `cmd/run.rs`: `RunArgs` struct with all required fields, `execute()` is a TODO stub
- `cmd/snapshot.rs`: `SnapshotArgs` struct with all required fields, `execute()` is a TODO stub

**tests/target_programs** (COMPLETE)
- All 4 required programs exist and are well-written:
  - `spike_alloc`: function_a(100MB) → function_b(200MB) spike pattern
  - `leak_linear`: 10MB/sec linear leak up to 1GB cap
  - `multithreaded`: 4 threads with distinct allocation patterns
  - `steady_state`: 50MB baseline + 1MB/s alloc-free cycles

**Workspace / DevOps** (MOSTLY COMPLETE)
- Root `Cargo.toml`: workspace with all 8 members (4 crates + 4 test programs), all workspace deps correct
- `.cargo/config.toml`: jobs=4, dev debug=true, release strip, profiling profile — COMPLETE
- `docker/` directory exists with Dockerfile, Dockerfile.test, docker-compose.yml
- No integration test `.rs` files exist yet (`tests/integration/` directory is absent)

### What's Missing (Critical for iter01)

1. **allocmap-ptrace** — 4 missing source files (`attach.rs`, `sampler.rs`, `backtrace.rs`, `symbols.rs`). The crate cannot compile without them.
2. **allocmap-preload** — 3 missing source files (`hooks.rs`, `ipc.rs`, `bump_alloc.rs`). The crate cannot compile without them.
3. **allocmap-tui** — 4 missing source files (`app.rs`, `timeline.rs`, `hotspot.rs`, `events.rs`). The crate cannot compile without them.
4. **allocmap-core `.amr` I/O** — `recording.rs` has the struct definitions but no `write_to()` / `read_from()` functions implementing the binary format (magic, version, JSON header, bincode frames, JSON footer).
5. **allocmap-cli command implementations** — all three `execute()` functions are stubs; no real ptrace sampling, TUI rendering, or output is performed.
6. **Integration tests** — `tests/integration/` directory does not exist; no `.rs` test files.
7. **Duration parsing** — `--duration` accepts a `String` but there is no parser for human-friendly formats like `30s`, `5m`. Needs a helper.

### What's Missing (Nice-to-Have for iter01)

- Flamegraph view in TUI (the `--mode flamegraph` flag is wired in CLI args but the render widget is not needed for initial acceptance if timeline + hotspot work)
- `tests/integration/` with the required 3-tests-per-feature structure
- Per-thread memory view in TUI (Phase 2 requirement, not blocking for Phase 1)

---

## Issues Found

### Per-Crate Issues

#### allocmap-core
- `recording.rs`: Structs are defined but there is no `impl AllocMapRecording` with `write()` and `read()` methods. The spec requires a precise binary layout (4-byte magic `AMR\0`, 4-byte u32 version, length-prefixed JSON header, length-prefixed bincode frames, JSON footer). This must be added.
- `CoreError::Serialization(String)` uses a plain `String` rather than wrapping `bincode::Error` or `serde_json::Error`. This is acceptable but means callers must call `.to_string()` manually. Low priority, can stay.
- No unit tests in any `allocmap-core` source file.

#### allocmap-ptrace
- `lib.rs` declares `pub mod attach; pub mod sampler; pub mod backtrace; pub mod symbols;` but none of these files exist. **The crate will fail to compile.**
- `PtraceSampler` is re-exported from `sampler` but `sampler.rs` does not exist.
- The ptrace sampling strategy needs careful design:
  - Attach with `nix::sys::ptrace::attach(Pid)`
  - `waitpid` for the process to stop
  - Read registers with `PTRACE_GETREGS` to get instruction pointer (IP/RIP on x86_64)
  - Unwind stack by reading frame pointers or using DWARF unwind info
  - Resume with `PTRACE_CONT`
  - Read heap size from `/proc/PID/status` (`VmRSS`, `VmHeap`) or `/proc/PID/smaps` for more accuracy
  - Symbol resolution: parse `/proc/PID/maps` to find loaded `.so` and binary, then use `addr2line` crate

#### allocmap-preload
- All 3 declared module files (`hooks.rs`, `ipc.rs`, `bump_alloc.rs`) are missing. **The crate will fail to compile.**
- The `bump_alloc.rs` is architecturally critical: hooks must not call the standard allocator (infinite recursion). A simple `static mut` bump allocator backed by `mmap` is needed.
- The `hooks.rs` must use `libc::dlsym(RTLD_NEXT, ...)` to get original `malloc`/`free`/`realloc`/`calloc` and then intercept them with `#[no_mangle] pub unsafe extern "C" fn malloc(size: usize) -> *mut c_void`.
- IPC design: Unix domain socket at path from `ALLOCMAP_SOCKET_PATH` env var. The preload library writes `SampleFrame`-equivalent data; the CLI process reads it. Protocol should be length-prefixed bincode messages. **Important**: IPC writes must also use the bump allocator or be lock-free to avoid deadlock inside malloc hooks.
- The crate-type is `cdylib` — correct.

#### allocmap-tui
- `app.rs`, `timeline.rs`, `hotspot.rs`, `events.rs` are all missing. **The crate will fail to compile.**
- `App` struct needs to hold: current `Vec<SampleFrame>` (ring buffer of last N), target pid, program name, selected mode, scroll position, start time.
- The timeline widget should use `ratatui::widgets::Chart` or `Sparkline`. The spec says "Unicode braille blocks" — ratatui's `Chart` with `Dataset` supports this natively via `Marker::Braille`.
- The hotspot widget should use `ratatui::widgets::Table` or a custom `List` with collapsible rows.
- Events: `crossterm::event::read()` in a non-blocking loop. Keys: `q` quit, `f` flamegraph mode, `t` timeline mode, `s` snapshot, `↑↓` scroll, `Enter` expand/collapse.

#### allocmap-cli
- `allocmap-ptrace` is in the CLI's dependencies only under `[target.'cfg(target_os = "linux")'.dependencies]` in `Cargo.toml`. However, the `execute()` functions in `cmd/attach.rs` and `cmd/snapshot.rs` will need `#[cfg(target_os = "linux")]` guards when they call into `allocmap-ptrace`. Otherwise the code will not compile on non-Linux (though Phase 1 is Linux-only, it's better practice).
- No duration parsing utility exists. The `--duration` args take `String`; need a `parse_duration(s: &str) -> Result<Duration>` function that handles `30s`, `5m`, `1h`.
- The `cmd/run.rs` `execute()` function needs to resolve the path to `liballocmap_preload.so`. In a dev environment this will be in `target/debug/` or `target/release/`. In an installed scenario it could be next to the binary. This path discovery logic is non-trivial and should look in common places.

#### Workspace / Root Cargo.toml
- `nix` features include `"mman"` which is good (needed for `mmap` in bump allocator).
- `rustc-demangle` is declared in workspace deps and used in `allocmap-ptrace` — good for demangling Rust symbol names.
- No `colored` usage in `allocmap-core`; it's declared in workspace and cli Cargo.toml but `allocmap-core/Cargo.toml` does not list it. That's fine.
- The test target programs (`spike_alloc`, etc.) are workspace members but their individual `Cargo.toml` files use `version = "0.1.0"` directly instead of `version.workspace = true`. This is a minor inconsistency but not a blocker.

---

## Implementation Plan for Iter 01

### allocmap-core changes needed

**File: `crates/allocmap-core/src/recording.rs`** — add I/O methods.

Add `impl AllocMapRecording` with:
```rust
pub fn write_to<W: Write>(&self, writer: &mut W) -> Result<(), CoreError>
pub fn read_from<R: Read>(&mut reader: &mut R) -> Result<Self, CoreError>
```
Binary layout (as specified):
1. Write `AMR_MAGIC` (4 bytes)
2. Write `AMR_VERSION` as little-endian u32 (4 bytes)
3. Serialize `header` as JSON, write 4-byte length prefix then JSON bytes
4. For each frame: serialize with `bincode::serialize`, write 4-byte length prefix then bytes
5. Serialize `footer` as JSON, write 4-byte length prefix then JSON bytes

Add unit tests in `recording.rs` covering write+read roundtrip and version mismatch error.

**File: `crates/allocmap-core/src/lib.rs`** — optionally add a `duration` module for shared duration parsing, or put it in `allocmap-cli`.

### allocmap-ptrace changes needed

Create these files:

**`crates/allocmap-ptrace/src/sampler.rs`**
```rust
pub struct PtraceSampler {
    pid: Pid,
    sample_rate_hz: u32,
    // channel to send SampleFrame to consumer
}
impl PtraceSampler {
    pub fn attach(pid: u32, sample_rate_hz: u32) -> Result<Self, PtraceError>
    pub async fn run_sampling_loop(
        &mut self,
        tx: tokio::sync::mpsc::Sender<SampleFrame>,
        duration: Option<Duration>,
    ) -> Result<(), PtraceError>
    pub fn detach(self) -> Result<(), PtraceError>
}
```
Sampling loop logic:
1. Every `1/sample_rate_hz` seconds:
   a. `ptrace::interrupt(pid)` (or use `kill(pid, SIGSTOP)` + `waitpid`)
   b. Collect stack frames via `backtrace` module
   c. Read heap size from `/proc/pid/status` (parse `VmRSS` line)
   d. Build `SampleFrame` with timestamp, heap size, rates, top sites
   e. Send frame over `tx` channel
   f. `ptrace::cont(pid, None)` to resume

**`crates/allocmap-ptrace/src/attach.rs`**
```rust
pub fn attach(pid: Pid) -> Result<(), PtraceError>  // PTRACE_ATTACH + waitpid
pub fn detach(pid: Pid) -> Result<(), PtraceError>  // PTRACE_DETACH
pub fn get_heap_bytes(pid: u32) -> Result<u64, PtraceError>  // parse /proc/pid/status
```

**`crates/allocmap-ptrace/src/backtrace.rs`**
```rust
pub fn collect_backtrace(pid: Pid) -> Result<Vec<u64>, PtraceError>
// Reads registers (PTRACE_GETREGS on x86_64 → UserRegs → rip, rsp, rbp)
// Frame pointer unwinding: follow rbp chain reading remote memory via
// ptrace::read() to get saved RIP values
// Returns Vec of instruction pointer addresses
```
Note: Frame-pointer unwinding requires target to be compiled without `-fomit-frame-pointer`. For better reliability on release builds, consider reading `/proc/pid/maps` and using DWARF unwind via `addr2line`. For iter01, frame-pointer unwinding is sufficient with a depth cap of ~32 frames.

**`crates/allocmap-ptrace/src/symbols.rs`**
```rust
pub struct SymbolResolver {
    // Caches parsed object files keyed by path
}
impl SymbolResolver {
    pub fn new(pid: u32) -> Result<Self, PtraceError>
    // Parses /proc/pid/maps to find all loaded segments
    pub fn resolve(&mut self, ip: u64) -> StackFrame
    // Uses addr2line crate to look up function name, file, line
}
```

### allocmap-preload changes needed

Create these files:

**`crates/allocmap-preload/src/bump_alloc.rs`**
- A simple fixed-size bump allocator backed by `mmap(MAP_ANONYMOUS)`.
- Must be safe to call from within malloc hooks (no libc allocation).
- Provide `pub fn alloc(size: usize) -> *mut u8` and `pub fn dealloc(ptr: *mut u8, size: usize)` — or just a bump-only allocator (no free, memory is reused in fixed-size slots).
- Use a `static AtomicUsize` as the bump pointer. Pre-allocate e.g. 4MB via mmap at init time.

**`crates/allocmap-preload/src/hooks.rs`**
- `malloc`, `free`, `realloc`, `calloc` replacements via `#[no_mangle] pub unsafe extern "C" fn`.
- Must use `dlsym(RTLD_NEXT, "malloc")` to get original functions.
- Use a thread-local `bool` guard to prevent re-entrancy during hook execution.
- On allocation: record `(address, size, thread_id, timestamp)` in a lock-free ring buffer (using the bump allocator for the ring buffer itself).
- On free: record `(address)` in ring buffer.
- Periodically (or when ring buffer is full) flush to IPC socket.

**`crates/allocmap-preload/src/ipc.rs`**
- Connect to Unix socket path from `ALLOCMAP_SOCKET_PATH` env var.
- Serialize alloc/free events and send as length-prefixed bincode messages.
- Must use `write()` syscall directly (not `std::io::Write` which may allocate) or carefully use a `ManuallyDrop<UnixStream>` with a static fd.

**`crates/allocmap-preload/src/lib.rs`** — update `allocmap_init()`:
- Register as a `__attribute__((constructor))` equivalent: use `#[link_section = ".init_array"]` or `ctor` crate, OR rely on `dlopen` + `allocmap_init` being called explicitly. The `#[no_mangle] pub extern "C" fn allocmap_init()` already handles this if the caller invokes it; for automatic invocation, use `#[ctor::ctor]` or the `.init_array` trick.
- Actually for LD_PRELOAD, the simplest approach: use `std::sync::Once` in each hook to initialize on first call, avoiding a separate constructor function entirely.

### allocmap-tui changes needed

Create these files:

**`crates/allocmap-tui/src/app.rs`**
```rust
pub struct App {
    pub pid: u32,
    pub program_name: String,
    pub mode: DisplayMode,  // Timeline | Hotspot | Flamegraph
    pub frames: VecDeque<SampleFrame>,  // ring buffer, cap ~500 frames
    pub scroll_offset: usize,
    pub expanded_sites: HashSet<usize>,
    pub should_quit: bool,
    pub start_time: std::time::Instant,
}
pub enum DisplayMode { Timeline, Hotspot, Flamegraph }
impl App {
    pub fn new(pid: u32, program_name: String) -> Self
    pub fn push_frame(&mut self, frame: SampleFrame)
    pub fn current_heap_bytes(&self) -> u64  // latest frame's live_heap_bytes
    pub fn growth_rate(&self) -> f64  // bytes/sec from recent frames
    pub fn render<B: Backend>(&mut self, f: &mut Frame<B>)
    pub fn on_key(&mut self, key: KeyEvent)
}
```

**`crates/allocmap-tui/src/timeline.rs`**
```rust
pub fn render_timeline<B: Backend>(f: &mut Frame<B>, area: Rect, frames: &VecDeque<SampleFrame>)
// Uses ratatui::widgets::Chart with Marker::Braille
// Color based on Theme::for_growth_rate()
// Shows last N frames proportional to area width
```

**`crates/allocmap-tui/src/hotspot.rs`**
```rust
pub fn render_hotspot<B: Backend>(
    f: &mut Frame<B>, area: Rect,
    sites: &[AllocationSite],
    scroll: usize,
    expanded: &HashSet<usize>,
    top_n: usize,
)
// Uses ratatui::widgets::Table or custom List
// Colors: hotspot_top (top 3), hotspot_mid (4-10), hotspot_low (rest)
```

**`crates/allocmap-tui/src/events.rs`**
```rust
pub enum AppEvent {
    Key(KeyEvent),
    NewFrame(SampleFrame),
    Tick,
    Quit,
}
pub fn poll_event(timeout: Duration) -> Result<Option<AppEvent>>
// Wraps crossterm::event::poll + read
```

The main TUI render loop lives in `App::render()` and produces:
```
╭─ allocmap · pid=XXXX (name) · HH:MM:SS · N samples ─╮
│ LIVE HEAP: XXX MB  △ +X.XMB/s  ALLOCS: X/s  FREES: X/s │
├──────────────────────────────────────────────────────────┤
│  [Timeline or Hotspot widget here]                       │
╰──────────────────────────────────────────────────────────╯
[q]quit [f]flamegraph [t]timeline [s]snapshot [↑↓]scroll
```

### allocmap-cli changes needed

**`crates/allocmap-cli/src/cmd/attach.rs`** — implement `execute()`:
1. Validate PID exists (`/proc/{pid}` readable)
2. Parse duration string with a `parse_duration()` helper
3. Spawn `PtraceSampler::attach(pid)` in a background `tokio::task`
4. Create `mpsc::channel::<SampleFrame>()` between sampler and TUI
5. If `--output` is given: collect frames, serialize to JSON, no TUI
6. Else: initialize `App`, enter raw mode, run event loop:
   - Poll for keyboard events with `crossterm`
   - Receive frames from channel
   - Call `app.render()` then `terminal.draw()`
   - Handle duration timeout via `tokio::time::sleep`
7. On exit: detach sampler, restore terminal, optionally save `.amr` if `--record` given

**`crates/allocmap-cli/src/cmd/snapshot.rs`** — implement `execute()`:
1. Parse duration
2. `PtraceSampler::attach(pid)`
3. Collect frames for `duration`
4. Build summary JSON (peak heap, avg heap, top sites)
5. Output to stdout or `--output` file
6. Detach

**`crates/allocmap-cli/src/cmd/run.rs`** — implement `execute()`:
1. Find `liballocmap_preload.so` (look next to binary, then in `target/debug/`, then `target/release/`)
2. Create Unix socket, bind to a temp path
3. Set `LD_PRELOAD` + `ALLOCMAP_SOCKET_PATH` env vars
4. Spawn child process with `std::process::Command`
5. Accept connection from preload library
6. Receive `SampleFrame` data over socket
7. Feed into same TUI loop as `attach`

**Add shared `parse_duration()` utility** — either in `allocmap-cli/src/util.rs` or inline:
```rust
fn parse_duration(s: &str) -> Result<std::time::Duration> {
    if let Some(v) = s.strip_suffix('s') { return Ok(Duration::from_secs(v.parse()?)); }
    if let Some(v) = s.strip_suffix('m') { return Ok(Duration::from_secs(v.parse::<u64>()? * 60)); }
    if let Some(v) = s.strip_suffix('h') { return Ok(Duration::from_secs(v.parse::<u64>()? * 3600)); }
    Err(anyhow!("Invalid duration '{}': use format like 30s, 5m, 1h", s))
}
```

---

## Dependency Issues

### Cargo.toml Fixes Needed

1. **`crossterm` version**: Workspace has `crossterm = "0.28"`. As of early 2026, the latest stable is `0.28.x` — this is fine. Verify compatibility with `ratatui = "0.28"` (they must be version-aligned; ratatui 0.28 requires crossterm 0.28 — CONFIRMED OK).

2. **`libc` crate is missing from workspace deps** — `allocmap-preload/src/hooks.rs` will need `libc` for `dlsym`, `RTLD_NEXT`, `c_void`, etc. Add to workspace:
   ```toml
   libc = "0.2"
   ```
   And add to `allocmap-preload/Cargo.toml`:
   ```toml
   libc = { workspace = true }
   ```

3. **`nix` features need `"user"` for uid checks** in permission denied handling — current features are `["ptrace", "process", "signal", "mman"]`. Optionally add `"user"` but not strictly required.

4. **`allocmap-preload` needs a `#[no_std]` consideration** — currently uses `anyhow` which requires `std`. Since preload hooks must avoid standard allocator, `anyhow` should be removed from `allocmap-preload` dependencies. It is currently listed but will cause issues if any code path in the hooks exercises it. Replace with manual error handling or a `no_std`-compatible approach for the hot paths.

5. **`addr2line` crate** needs the `object` feature enabled for loading debug info from binary files. Current workspace dependency is `addr2line = "0.22"` with no explicit features. Should be:
   ```toml
   addr2line = { version = "0.22", features = ["object"] }
   ```

6. **`object` crate** needs features for reading ELF files. Current is `object = "0.36"` with no features. Should be:
   ```toml
   object = { version = "0.36", features = ["read"] }
   ```

7. **Test target programs Cargo.toml** use hardcoded `version = "0.1.0"` instead of `version.workspace = true` — minor style inconsistency, not a build blocker but worth fixing.

---

## Risk Assessment

### High Risk

1. **ptrace stack unwinding reliability** — Frame-pointer unwinding requires targets compiled with frame pointers (`-C force-frame-pointers=yes` or without `-fomit-frame-pointer`). The test programs are Rust crates compiled in dev mode, which preserves frame pointers by default. However, system libraries (libc, etc.) are compiled without frame pointers. The backtrace will be truncated at the first frame without frame pointer. For iter01 this is acceptable but must be documented. DWARF-based unwinding via `addr2line` + `gimli` is more robust but significantly more complex to implement correctly.

2. **LD_PRELOAD + standard allocator re-entrancy** — If any code inside the malloc hook calls anything that triggers malloc (e.g., `eprintln!`, `String::new()`, `Vec::new()`), the result is infinite recursion and stack overflow. The `bump_alloc` module is safety-critical. Developer must be extremely careful. Use a thread-local `bool` guard (using `pthread_key_create` or `#[thread_local] static IN_HOOK: Cell<bool>`).

3. **TUI terminal restoration on panic** — If the TUI crashes or panics while in raw mode, the terminal is left in an unusable state. Must install a panic hook that calls `disable_raw_mode()` and `execute!(LeaveAlternateScreen)` before printing the panic message.

4. **`/proc/PID/maps` parsing complexity** — Maps lines have varying format. The parser must handle lines with no filename (anonymous), `[heap]`, `[stack]`, and named shared libraries. Use a simple line-by-line parser; avoid regex to keep deps minimal.

### Medium Risk

5. **IPC protocol between preload and CLI** — If the CLI is not listening when the preload library tries to connect, the target process may hang or crash. The IPC must be non-blocking with a timeout. If the socket connection fails, the preload library should silently continue without instrumentation (fail-open) rather than crashing the target.

6. **`allocmap run` .so path discovery** — When installed via `cargo install`, the `.so` is NOT installed alongside the binary. The `run` command requires the shared library to be findable. For Phase 1 this can be documented as a dev-mode-only feature (use `target/debug/liballocmap_preload.so`).

7. **`ratatui` 0.28 API** — ratatui has changed its API significantly between versions. Ensure the `Chart` widget and `Braille` marker API matches 0.28 specifically. Check that `Frame<B>` generic is still present in 0.28 (in later versions it was simplified to non-generic `Frame`).

### Low Risk

8. **Duration parsing** — Simple string parsing, well-contained, easy to test.

9. **JSON output format** — `serde_json::to_string_pretty` on a `Vec<SampleFrame>` is straightforward; no custom serialization needed.

10. **`cargo clippy -D warnings`** — The current skeleton code is clean. New implementations must be written to clippy standards from the start. Key areas to watch: unused variables, unnecessary `mut`, missing `#[allow]` attributes for intentional unsafe code.

---

## Recommended Developer Task Split for Iter 01

Given the above, here is the recommended parallel task split:

| Developer | Crate | Files |
|-----------|-------|-------|
| Dev-A | allocmap-core | `recording.rs` I/O methods + unit tests |
| Dev-B | allocmap-ptrace | `attach.rs`, `sampler.rs`, `backtrace.rs`, `symbols.rs` |
| Dev-C | allocmap-preload | `bump_alloc.rs`, `hooks.rs`, `ipc.rs` |
| Dev-D | allocmap-tui | `app.rs`, `timeline.rs`, `hotspot.rs`, `events.rs` |
| Dev-E | allocmap-cli | Wire up all three `execute()` functions + `parse_duration()` |

Dev-A and Dev-D can work fully in parallel (no shared dependency on new code).
Dev-B must complete before Dev-E can fully integrate.
Dev-C depends on Dev-A's IPC format decisions.
Dev-E depends on Dev-B (ptrace), Dev-C (preload path), and Dev-D (TUI App).

The critical path is: **Dev-B → Dev-E** (ptrace → CLI integration).

Integration tests should be written by Dev-E or a dedicated Tester agent after all five developer tasks complete.
