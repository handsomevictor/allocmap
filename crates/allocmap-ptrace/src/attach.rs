use nix::sys::ptrace;
use nix::sys::signal::Signal;
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::Pid;
use crate::error::PtraceError;


/// Attach to a process using PTRACE_ATTACH and wait for it to stop.
pub fn attach(pid: Pid) -> Result<(), PtraceError> {
    let pid_u32 = pid.as_raw() as u32;

    // Check that the process exists first
    if !process_exists(pid_u32) {
        return Err(PtraceError::ProcessNotFound { pid: pid_u32 });
    }

    ptrace::attach(pid).map_err(|e| {
        if e == nix::errno::Errno::EPERM {
            PtraceError::PermissionDenied {
                pid: pid_u32,
                errno: libc::EPERM,
            }
        } else if e == nix::errno::Errno::ESRCH {
            PtraceError::ProcessNotFound { pid: pid_u32 }
        } else {
            PtraceError::PtraceOp {
                pid: pid_u32,
                detail: format!("PTRACE_ATTACH failed: {}", e),
            }
        }
    })?;

    // Wait for the process to stop
    match waitpid(pid, None) {
        Ok(WaitStatus::Stopped(_, Signal::SIGSTOP))
        | Ok(WaitStatus::Stopped(_, Signal::SIGTRAP)) => {}
        Ok(_) => {
            // Process stopped for some other reason; still acceptable
        }
        Err(e) => {
            return Err(PtraceError::PtraceOp {
                pid: pid_u32,
                detail: format!("waitpid failed after attach: {}", e),
            });
        }
    }

    Ok(())
}

/// Detach from a process, resuming its execution.
pub fn detach(pid: Pid) -> Result<(), PtraceError> {
    let pid_u32 = pid.as_raw() as u32;
    ptrace::detach(pid, None).map_err(|e| PtraceError::PtraceOp {
        pid: pid_u32,
        detail: format!("PTRACE_DETACH failed: {}", e),
    })
}

/// Read heap usage from /proc/PID/status (VmRSS line in kB, convert to bytes).
/// Falls back to VmSize if VmRSS not available.
pub fn get_heap_bytes(pid: u32) -> Result<u64, PtraceError> {
    let status_path = format!("/proc/{}/status", pid);
    let content = std::fs::read_to_string(&status_path).map_err(|e| PtraceError::PtraceOp {
        pid,
        detail: format!("Failed to read {}: {}", status_path, e),
    })?;

    // Try VmRSS first (resident set size — best proxy for live heap)
    let mut vmrss_kb: Option<u64> = None;
    let mut vmsize_kb: Option<u64> = None;

    for line in content.lines() {
        if line.starts_with("VmRSS:") {
            vmrss_kb = parse_proc_status_kb(line);
        } else if line.starts_with("VmSize:") {
            vmsize_kb = parse_proc_status_kb(line);
        }
    }

    if let Some(kb) = vmrss_kb {
        return Ok(kb * 1024);
    }
    if let Some(kb) = vmsize_kb {
        return Ok(kb * 1024);
    }

    Err(PtraceError::PtraceOp {
        pid,
        detail: "VmRSS and VmSize not found in /proc/status".to_string(),
    })
}

/// Parse a "Key:   N kB" line from /proc/pid/status and return the N value.
fn parse_proc_status_kb(line: &str) -> Option<u64> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 2 {
        parts[1].parse::<u64>().ok()
    } else {
        None
    }
}

/// Check if /proc/PID exists (process is alive).
pub fn process_exists(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{}", pid)).exists()
}

/// RAII wrapper: attaches on creation, detaches on drop.
pub struct PtraceAttach {
    pub pid: Pid,
}

impl PtraceAttach {
    /// Attach to the given PID via PTRACE_ATTACH.
    pub fn attach(pid: u32) -> anyhow::Result<Self> {
        let nix_pid = Pid::from_raw(pid as i32);
        attach(nix_pid)?;
        Ok(Self { pid: nix_pid })
    }

    /// Resume the process.
    pub fn cont(&self) -> anyhow::Result<()> {
        ptrace::cont(self.pid, None).map_err(|e| {
            anyhow::anyhow!(
                "PTRACE_CONT failed on PID {}: {}",
                self.pid,
                e
            )
        })
    }

    /// Detach from the process, allowing it to continue normally.
    pub fn detach(&self) -> anyhow::Result<()> {
        ptrace::detach(self.pid, None)
            .map_err(|e| anyhow::anyhow!("Failed to detach from PID {}: {}", self.pid, e))
    }
}

impl Drop for PtraceAttach {
    fn drop(&mut self) {
        // Best-effort detach on drop so the target process is not left frozen.
        let _ = ptrace::detach(self.pid, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_exists_current_process() {
        let my_pid = std::process::id();
        assert!(
            process_exists(my_pid),
            "Current process (PID {}) should exist in /proc",
            my_pid
        );
    }

    #[test]
    fn test_process_exists_nonexistent_pid() {
        // PID 99999999 is astronomically unlikely to exist.
        assert!(
            !process_exists(99_999_999),
            "PID 99999999 should not exist"
        );
    }

    #[test]
    fn test_get_heap_bytes_current_process() {
        let my_pid = std::process::id();
        let result = get_heap_bytes(my_pid);
        assert!(result.is_ok(), "get_heap_bytes should succeed for current process");
        let bytes = result.unwrap();
        assert!(bytes > 0, "Current process should have non-zero resident memory");
    }

    #[test]
    fn test_get_heap_bytes_nonexistent_process() {
        let result = get_heap_bytes(99_999_999);
        assert!(
            result.is_err(),
            "get_heap_bytes should fail for a non-existent PID"
        );
    }
}
