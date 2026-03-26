use nix::sys::ptrace;
use nix::unistd::Pid;
use allocmap_core::StackFrame;
use crate::symbols::SymbolResolver;
use crate::error::PtraceError;

/// Maximum number of stack frames to unwind.
const MAX_FRAMES: usize = 32;

/// Collect instruction pointer addresses via frame-pointer unwinding.
/// Returns up to `max_frames` instruction pointers.
/// The target process MUST be stopped before calling this.
pub fn collect_backtrace(pid: Pid, max_frames: usize) -> Result<Vec<u64>, PtraceError> {
    let pid_u32 = pid.as_raw() as u32;

    #[cfg(not(target_arch = "x86_64"))]
    return Err(PtraceError::PtraceOp {
        pid: pid_u32,
        detail: "Backtrace collection is only supported on x86_64".to_string(),
    });

    #[cfg(target_arch = "x86_64")]
    {
        let regs = ptrace::getregs(pid).map_err(|e| PtraceError::PtraceOp {
            pid: pid_u32,
            detail: format!("PTRACE_GETREGS failed: {}", e),
        })?;

        let mut ips: Vec<u64> = Vec::with_capacity(max_frames);

        // Always include the current instruction pointer.
        let rip = regs.rip;
        if rip != 0 {
            ips.push(rip);
        }

        // Walk the frame-pointer chain.
        let mut rbp = regs.rbp;
        while rbp != 0 && ips.len() < max_frames {
            // Read the saved return address at rbp+8.
            let ret_addr = match read_u64_remote(pid, rbp.wrapping_add(8)) {
                Ok(v) => v,
                Err(_) => break,
            };

            if ret_addr == 0 {
                break;
            }
            ips.push(ret_addr);

            // Read the previous frame pointer at rbp.
            let prev_rbp = match read_u64_remote(pid, rbp) {
                Ok(v) => v,
                Err(_) => break,
            };

            // Guard against infinite loops (frame pointer didn't change or went backwards).
            if prev_rbp <= rbp {
                break;
            }
            rbp = prev_rbp;
        }

        Ok(ips)
    }
}

/// Read 8 bytes from a remote process's address space via ptrace PEEKDATA.
/// ptrace::read returns nix::Result<i64>; we reinterpret the bits as u64.
fn read_u64_remote(pid: Pid, addr: u64) -> Result<u64, nix::errno::Errno> {
    // ptrace::read expects the address aligned to word size.
    // On x86_64, PTRACE_PEEKDATA reads one 8-byte word.
    let val = ptrace::read(pid, addr as *mut std::ffi::c_void)?;
    Ok(val as u64)
}

/// Captures a backtrace from a stopped process and resolves each frame.
pub struct BacktraceCapture {
    resolver: SymbolResolver,
}

impl BacktraceCapture {
    pub fn new() -> Self {
        Self {
            resolver: SymbolResolver::new(),
        }
    }

    /// Read registers and unwind the stack of a stopped process.
    /// The process MUST be stopped (e.g., via SIGSTOP) before calling this.
    pub fn capture(&mut self, pid: Pid) -> anyhow::Result<Vec<StackFrame>> {
        let pid_u32 = pid.as_raw() as u32;
        let ips = collect_backtrace(pid, MAX_FRAMES)?;

        let frames: Vec<StackFrame> = ips
            .iter()
            .map(|&ip| self.resolver.resolve(ip, pid_u32))
            .collect();

        Ok(frames)
    }

    /// Read 8 bytes from a remote process's address space via /proc/<pid>/mem.
    pub fn read_u64_from_proc(&self, pid: Pid, addr: u64) -> anyhow::Result<u64> {
        use std::io::{Read, Seek, SeekFrom};
        use std::fs::OpenOptions;

        let mem_path = format!("/proc/{}/mem", pid.as_raw());
        let mut f = OpenOptions::new().read(true).open(&mem_path)?;
        f.seek(SeekFrom::Start(addr))?;

        let mut buf = [0u8; 8];
        f.read_exact(&mut buf)?;
        Ok(u64::from_le_bytes(buf))
    }
}

impl Default for BacktraceCapture {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that BacktraceCapture can be created (compile-time check).
    /// We cannot exercise ptrace in unit tests without special permissions,
    /// so this test only verifies the type is constructable.
    #[test]
    fn test_backtrace_capture_new_compiles() {
        let _capture = BacktraceCapture::new();
        // If this compiles and runs, the struct is correctly defined.
    }

    /// Verify that collect_backtrace returns an error for a non-existent PID.
    /// This tests the error path without requiring ptrace permissions on a live process.
    #[test]
    fn test_collect_backtrace_invalid_pid_returns_error() {
        let fake_pid = Pid::from_raw(99_999_998_i32);
        let result = collect_backtrace(fake_pid, 32);
        // We expect an error because PID 99999998 almost certainly doesn't exist.
        assert!(
            result.is_err(),
            "collect_backtrace on a non-existent PID should fail"
        );
    }

    /// Verify read_u64_remote error handling doesn't panic on invalid address.
    #[test]
    fn test_read_u64_remote_invalid_pid() {
        let fake_pid = Pid::from_raw(99_999_997_i32);
        let result = read_u64_remote(fake_pid, 0x1000);
        assert!(result.is_err(), "read_u64_remote on non-existent PID should fail");
    }
}
