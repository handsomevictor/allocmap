# Tester Report — Phase 2 Iter 02

## Verdict: PASSED

## Test Counts
- Total: 67 (67 passed, 0 failed)
  - allocmap-cli unit tests: 24
  - allocmap-cli integration tests: 5
  - allocmap-core unit tests: 3
  - allocmap-preload unit tests: 4
  - allocmap-ptrace unit tests: 15
  - allocmap-tui unit tests: 16

## Command Tests

### 1. `cargo test --workspace`
- Result: PASSED
- All 67 tests passed, 0 failed, 0 ignored
- Includes `test_replay_pause_flag_synced_on_space` (pause Arc<AtomicBool>)
- Includes `test_replay_seek_g_sets_target_zero` (g/G seek keys)

### 2. `allocmap replay --help`
- Result: PASSED (with note)
- `--help` shows correct CLI options (--from, --to, --speed)
- The `[g]start [G]end` keys appear in the TUI keybinding hint bar
  (`crates/allocmap-tui/src/lib.rs:177`) rather than in `--help` output,
  which is the correct design — keybindings are TUI runtime hints, not CLI flags.

### 3. `allocmap snapshot --pid <multithreaded PID> --duration 3s`
- Result: PASSED
- `thread_count` field present in every frame (value: 5 — main thread + 4 workers, correct)
- `live_heap_bytes` is non-zero (75,829,248 bytes ~ 72 MB, consistent with
  thread_large_slow holding 50 MB + thread_steady_hold holding 20 MB)
- `peak_heap_bytes` reported correctly in summary

### 4. `allocmap diff --help`
- Result: PASSED
- Help shows correct usage with `<BASELINE>` and `<CURRENT>` positional args
- `--min-change-pct` option present

## Issues Found
None. All implemented features work correctly:
- Replay pause via Arc<AtomicBool> confirmed by unit test + compile
- g/G seek keys implemented and tested in TUI app unit tests
- thread_count added to SampleFrame and visible in snapshot JSON output
- PTRACE_O_TRACECLONE implemented (thread_count reflects live thread count = 5)

## Summary
Phase 2 Iter 02 passes all tests. The 67-test suite is green. All four
new features (replay pause, seek keys, thread_count field, PTRACE_O_TRACECLONE)
are implemented, tested, and functioning correctly.
