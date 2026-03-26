use anyhow::Result;
use nix::sys::ptrace;
use nix::sys::signal::Signal;
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::Pid;
use std::time::{Duration, Instant};
use allocmap_core::{SampleFrame, AllocationSite};
use crate::attach::{PtraceAttach, get_heap_bytes};
use crate::backtrace::BacktraceCapture;

/// Configuration for the ptrace sampler
#[derive(Debug, Clone)]
pub struct SamplerConfig {
    /// Sampling frequency in Hz
    pub sample_rate_hz: u32,
    /// Maximum number of top allocation sites to report
    pub top_n: usize,
}

impl Default for SamplerConfig {
    fn default() -> Self {
        Self {
            sample_rate_hz: 50,
            top_n: 20,
        }
    }
}

/// The main ptrace-based sampler.
/// Attaches to a running process and periodically samples its memory state.
pub struct PtraceSampler {
    pid: u32,
    config: SamplerConfig,
    _attach: PtraceAttach,
    backtrace: BacktraceCapture,
    sample_count: u64,
    start_time: Instant,
    /// Tracks previous measurement for rate calculation
    prev_heap_bytes: u64,
    prev_sample_time: Instant,
}

impl PtraceSampler {
    /// Attach to the given PID and create a sampler
    pub fn attach(pid: u32) -> Result<Self> {
        let attach = PtraceAttach::attach(pid)?;
        // Resume the process after initial attach stop
        attach.cont()?;

        Ok(Self {
            pid,
            config: SamplerConfig::default(),
            _attach: attach,
            backtrace: BacktraceCapture::new(),
            sample_count: 0,
            start_time: Instant::now(),
            prev_heap_bytes: 0,
            prev_sample_time: Instant::now(),
        })
    }

    /// Attach with custom configuration
    pub fn attach_with_config(pid: u32, config: SamplerConfig) -> Result<Self> {
        let mut s = Self::attach(pid)?;
        s.config = config;
        Ok(s)
    }

    /// Collect a single sample frame from the target process.
    /// Briefly stops the process, reads its state, then resumes.
    pub fn sample(&mut self) -> Result<SampleFrame> {
        let nix_pid = Pid::from_raw(self.pid as i32);

        // Stop the process briefly to read its state
        nix::sys::signal::kill(nix_pid, Signal::SIGSTOP)?;

        // Wait for it to actually stop
        match waitpid(nix_pid, None) {
            Ok(WaitStatus::Stopped(_, _)) => {}
            Ok(_) | Err(_) => {
                // Try to continue anyway
                let _ = ptrace::cont(nix_pid, None);
                return Err(anyhow::anyhow!("Process {} did not stop cleanly", self.pid));
            }
        }

        // Read heap information from /proc/<pid>/status.
        // VmRSS already aggregates all threads in the process.
        // thread_count is available via list_threads() for future per-thread view.
        let _thread_count = crate::attach::list_threads(self.pid);
        let heap_bytes = get_heap_bytes(self.pid).unwrap_or(0);

        // Capture a backtrace for the current execution point
        let frames = self.backtrace.capture(nix_pid).unwrap_or_default();

        // Resume the process
        ptrace::cont(nix_pid, None)?;

        // Calculate rates
        let now = Instant::now();
        let dt = now.duration_since(self.prev_sample_time).as_secs_f64().max(0.001);

        let heap_delta = heap_bytes as f64 - self.prev_heap_bytes as f64;
        let alloc_rate = heap_delta.max(0.0) / dt;
        let free_rate = (-heap_delta).max(0.0) / dt;

        self.prev_heap_bytes = heap_bytes;
        self.prev_sample_time = now;

        let elapsed_ms = self.start_time.elapsed().as_millis() as u64;
        self.sample_count += 1;

        // Build allocation sites from the captured backtrace
        let top_sites = if !frames.is_empty() {
            vec![AllocationSite {
                live_bytes: heap_bytes,
                alloc_count: self.sample_count,
                frames,
            }]
        } else {
            vec![]
        };

        Ok(SampleFrame {
            timestamp_ms: elapsed_ms,
            live_heap_bytes: heap_bytes,
            alloc_rate,
            free_rate,
            top_sites,
        })
    }

    /// Return the sampling interval as a Duration
    pub fn sample_interval(&self) -> Duration {
        Duration::from_micros(1_000_000 / self.config.sample_rate_hz as u64)
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn sample_count(&self) -> u64 {
        self.sample_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sampler_config_default() {
        let config = SamplerConfig::default();
        assert_eq!(config.sample_rate_hz, 50);
        assert_eq!(config.top_n, 20);
    }

    #[test]
    fn test_sample_interval_calculation() {
        // Build a sampler config with known rate and verify the interval.
        let config = SamplerConfig {
            sample_rate_hz: 10,
            top_n: 5,
        };
        // At 10 Hz the interval should be 100 ms = 100_000 µs.
        let interval = Duration::from_micros(1_000_000 / config.sample_rate_hz as u64);
        assert_eq!(interval, Duration::from_millis(100));
    }

    #[test]
    fn test_attach_nonexistent_pid_returns_error() {
        // Attaching to a non-existent PID must return an error, not panic.
        let result = PtraceSampler::attach(99_999_999);
        assert!(
            result.is_err(),
            "PtraceSampler::attach on a non-existent PID should fail"
        );
    }
}
