# Tester Report — Phase 2 Iter 03

**Date**: 2026-03-26
**Tester**: Tester Agent

---

## 1. Cargo Test Suite

All workspace tests passed with zero failures.

| Crate | Tests | Result |
|-------|-------|--------|
| allocmap-cli (unit) | 24 | PASSED |
| allocmap-cli (integration) | 5 | PASSED |
| allocmap-core | 3 | PASSED |
| allocmap-preload | 4 | PASSED |
| allocmap-ptrace | 15 | PASSED |
| allocmap-tui | 17 | PASSED |
| **Total** | **68** | **ALL PASSED** |

Notable new TUI tests in iter03:
- `test_display_mode_parse_threads` — PASSED
- `test_on_key_mode_switching` — PASSED (covers T-key for Threads mode)

---

## 2. Snapshot with thread_ids Verification

**Test**: Ran `multithreaded` target program (4 worker threads + main = 5 threads), took snapshot with `--duration 3s`.

**Result**: PASSED

The JSON output contained `thread_ids` array in every SampleFrame with multiple distinct TIDs. Example from first frame:

```json
"thread_ids": [7, 8, 9, 11, 12],
"thread_count": 5,
```

- 5 threads were detected (1 main + 4 workers), matching the program's design.
- `thread_ids` was non-empty in all sampled frames.
- `thread_count` matched the length of `thread_ids`.

---

## 3. Command Help Tests

All commands returned correct help text with no errors:

| Command | Result |
|---------|--------|
| `allocmap --help` | PASSED — lists all 5 subcommands |
| `allocmap replay --help` | PASSED — shows --from, --to, --speed options |
| `allocmap diff --help` | PASSED — shows baseline/current args |
| `allocmap snapshot --help` | PASSED — shows --pid, --duration, --output, --top options |

---

## 4. Regression Check

- No previously passing tests regressed.
- `DisplayMode::Threads` parses correctly (case-insensitive).
- `T` key cycles to Threads mode in app event handler.

---

## Overall Verdict: PASSED

All 68 tests pass, `thread_ids` is present and populated in snapshot JSON output, and all CLI commands function correctly. Phase 2 Iter 03 meets its stated goals.
