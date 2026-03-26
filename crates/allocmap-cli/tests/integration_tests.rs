//! Integration tests for the allocmap CLI binary.
//!
//! These tests invoke the compiled allocmap binary and verify behavior from the
//! outside — exit codes, stderr messages, and stdout format.  They require that
//! the binary has been built beforehand (`cargo build`) and, for attach/snapshot
//! tests, ptrace permissions (`--cap-add=SYS_PTRACE` in Docker).

use std::path::PathBuf;
use std::process::Command;

/// Return the path to the allocmap debug binary.
fn allocmap_binary() -> PathBuf {
    // CARGO_MANIFEST_DIR is the allocmap-cli crate directory.
    // The binary lives at workspace_root/target/debug/allocmap.
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .parent() // crates/
        .and_then(|p| p.parent()) // workspace root
        .expect("Could not resolve workspace root");

    let path = workspace_root.join("target/debug/allocmap");
    if path.exists() {
        return path;
    }
    // Fallback: hope it is on PATH
    PathBuf::from("allocmap")
}

// ── Success tests ─────────────────────────────────────────────────────────────

/// `allocmap --help` must succeed and list the main sub-commands.
#[test]
fn test_help_lists_subcommands() {
    let output = match Command::new(allocmap_binary()).arg("--help").output() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Skipping test_help_lists_subcommands: binary not found ({})", e);
            return;
        }
    };

    assert!(
        output.status.success(),
        "allocmap --help should exit 0, got: {}",
        output.status
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("attach") || stdout.contains("Attach"),
        "Help output should mention 'attach', got:\n{stdout}"
    );
    assert!(
        stdout.contains("snapshot") || stdout.contains("Snapshot"),
        "Help output should mention 'snapshot', got:\n{stdout}"
    );
    assert!(
        stdout.contains("run") || stdout.contains("Run"),
        "Help output should mention 'run', got:\n{stdout}"
    );
}

/// `allocmap snapshot --help` should succeed and be in English.
#[test]
fn test_snapshot_help_is_english() {
    let output = match Command::new(allocmap_binary())
        .args(["snapshot", "--help"])
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Skipping test_snapshot_help_is_english: binary not found ({})", e);
            return;
        }
    };

    assert!(
        output.status.success(),
        "allocmap snapshot --help should exit 0"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    // English keywords expected in help text
    assert!(
        stdout.contains("pid") || stdout.contains("PID") || stdout.contains("process"),
        "Snapshot help should mention PID/process, got:\n{stdout}"
    );
}

// ── Failure tests — invalid input ─────────────────────────────────────────────

/// Passing a non-existent PID to `allocmap snapshot` must fail with a clear message.
#[test]
fn test_snapshot_nonexistent_pid_fails() {
    let output = match Command::new(allocmap_binary())
        .args(["snapshot", "--pid", "99999999"])
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            eprintln!(
                "Skipping test_snapshot_nonexistent_pid_fails: binary not found ({})",
                e
            );
            return;
        }
    };

    assert!(
        !output.status.success(),
        "allocmap snapshot with non-existent PID should exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not found")
            || stderr.contains("Process")
            || stderr.contains("99999999"),
        "Error message should reference the missing PID, got:\n{stderr}"
    );
}

/// Passing a negative-looking or zero value to `--pid` should fail (clap rejects it because
/// the field is u32; 0 is technically valid u32 but /proc/0 may not be a real user process).
#[test]
fn test_snapshot_invalid_pid_type_fails() {
    let output = match Command::new(allocmap_binary())
        .args(["snapshot", "--pid", "notanumber"])
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            eprintln!(
                "Skipping test_snapshot_invalid_pid_type_fails: binary not found ({})",
                e
            );
            return;
        }
    };

    assert!(
        !output.status.success(),
        "allocmap snapshot --pid notanumber should exit non-zero"
    );
}

// ── Failure tests — boundary / permissions ────────────────────────────────────

/// `allocmap snapshot` with an invalid duration format should fail with a useful message.
#[test]
fn test_snapshot_invalid_duration_fails() {
    // Use our own PID so the process-exists check passes;
    // the duration parse should fail first.
    let self_pid = std::process::id().to_string();
    let output = match Command::new(allocmap_binary())
        .args(["snapshot", "--pid", &self_pid, "--duration", "notaduration"])
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            eprintln!(
                "Skipping test_snapshot_invalid_duration_fails: binary not found ({})",
                e
            );
            return;
        }
    };

    assert!(
        !output.status.success(),
        "allocmap snapshot with invalid duration should exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("duration")
            || stderr.contains("notaduration")
            || stderr.contains("Invalid")
            || stderr.contains("invalid"),
        "Error message should describe the bad duration, got:\n{stderr}"
    );
}
