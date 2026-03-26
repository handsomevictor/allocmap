//! macOS memory sampler using sysctl and ps.
//!
//! Note: Full task_for_pid + mach_vm_read support requires entitlements or
//! root access on modern macOS. This stub provides basic RSS sampling via
//! `ps` as a starting point for Phase 2.

use std::time::{Duration, Instant};
use allocmap_core::{SampleFrame, AllocationSite};

/// Errors that can occur in the macOS sampler.
#[derive(Debug)]
pub enum MacosSamplerError {
    /// The target process was not found (does not exist or already exited).
    ProcessNotFound { pid: u32 },
    /// An OS-level operation failed.
    OsError { detail: String },
}

impl std::fmt::Display for MacosSamplerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MacosSamplerError::ProcessNotFound { pid } => write!(
                f,
                "Failed to attach to PID {}: process not found. \
                 Make sure the process is running: ps aux | grep {}",
                pid, pid
            ),
            MacosSamplerError::OsError { detail } => write!(f, "OS error: {}", detail),
        }
    }
}

impl std::error::Error for MacosSamplerError {}

/// macOS sampler — provides the same public interface as the Linux PtraceSampler.
///
/// Uses `ps -o rss= -p <pid>` to read resident set size because direct mach
/// port access requires special entitlements on modern macOS.
pub struct PtraceSampler {
    pid: u32,
    sample_interval: Duration,
    start_time: Instant,
    prev_rss: u64,
    prev_time: Instant,
    sample_count: u64,
}

impl PtraceSampler {
    /// Verify the process exists and create a sampler attached to it.
    pub fn attach(pid: u32) -> Result<Self, MacosSamplerError> {
        // Use kill(pid, 0) to check process existence without sending a signal.
        let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
        if result != 0 {
            return Err(MacosSamplerError::ProcessNotFound { pid });
        }
        let now = Instant::now();
        Ok(Self {
            pid,
            sample_interval: Duration::from_millis(20),
            start_time: now,
            prev_rss: 0,
            prev_time: now,
            sample_count: 0,
        })
    }

    /// Return the sampling interval.
    pub fn sample_interval(&self) -> Duration {
        self.sample_interval
    }

    /// Return the PID being sampled.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Return the number of samples collected so far.
    pub fn sample_count(&self) -> u64 {
        self.sample_count
    }

    /// Collect one sample frame from the target process.
    pub fn sample(&mut self) -> Result<SampleFrame, MacosSamplerError> {
        let rss = self.get_rss()?;
        let now = Instant::now();
        let elapsed = now.duration_since(self.prev_time).as_secs_f64().max(0.001);

        let alloc_rate = if rss > self.prev_rss {
            (rss - self.prev_rss) as f64 / elapsed
        } else {
            0.0
        };
        let free_rate = if rss < self.prev_rss {
            (self.prev_rss - rss) as f64 / elapsed
        } else {
            0.0
        };

        self.prev_rss = rss;
        self.prev_time = now;
        self.sample_count += 1;

        Ok(SampleFrame {
            timestamp_ms: self.start_time.elapsed().as_millis() as u64,
            live_heap_bytes: rss,
            alloc_rate,
            free_rate,
            top_sites: vec![],
            thread_count: 1,
            thread_ids: vec![],
        })
    }

    /// Read resident set size in bytes for the target process using `ps`.
    fn get_rss(&self) -> Result<u64, MacosSamplerError> {
        let output = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &self.pid.to_string()])
            .output()
            .map_err(|e| MacosSamplerError::OsError {
                detail: format!("ps command failed: {}", e),
            })?;

        if !output.status.success() && output.stdout.is_empty() {
            // Process no longer exists.
            return Err(MacosSamplerError::ProcessNotFound { pid: self.pid });
        }

        let rss_kb: u64 = String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse()
            .unwrap_or(0);

        // ps reports RSS in KB; convert to bytes.
        Ok(rss_kb * 1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attach_self() {
        let pid = std::process::id();
        let result = PtraceSampler::attach(pid);
        assert!(result.is_ok(), "attach to current process should succeed");
    }

    #[test]
    fn test_attach_nonexistent_fails() {
        let result = PtraceSampler::attach(99_999_999);
        assert!(result.is_err(), "attach to non-existent PID should fail");
    }

    #[test]
    fn test_sample_interval_default() {
        let pid = std::process::id();
        if let Ok(sampler) = PtraceSampler::attach(pid) {
            assert_eq!(sampler.sample_interval(), Duration::from_millis(20));
        }
    }
}
