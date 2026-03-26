use anyhow::Result;
use nix::sys::ptrace;
use nix::sys::signal::Signal;
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::Pid;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use allocmap_core::{SampleFrame, AllocationSite, StackFrame};
use crate::attach::{PtraceAttach, get_heap_bytes};
use crate::backtrace::BacktraceCapture;

/// One accumulated call-site entry: keeps track of how many times this unique
/// call stack was observed and the heap state during those observations.
#[derive(Debug, Clone)]
struct AccumSite {
    frames: Vec<StackFrame>,
    /// Heap bytes at the latest sample where this stack was active
    live_bytes: u64,
    /// Sum of live_heap_bytes across all samples for this stack
    live_bytes_sum: u64,
    /// How many samples captured this call stack
    sample_count: u64,
}

impl AccumSite {
    /// Average live heap bytes across all samples for this site.
    /// This is more stable than the instantaneous `live_bytes` because it
    /// represents the memory "associated with" this call stack over its lifetime.
    fn avg_live_bytes(&self) -> u64 {
        if self.sample_count == 0 { 0 } else { self.live_bytes_sum / self.sample_count }
    }
}

/// Compute a 64-bit fingerprint for the top-N frame IPs in a call stack.
/// We use the top 6 IPs so that the same function (even at different heap sizes)
/// maps to the same bucket.
fn stack_fingerprint(frames: &[StackFrame]) -> u64 {
    // FNV-1a 64-bit hash
    let mut h: u64 = 0xcbf29ce484222325;
    for f in frames.iter().take(6) {
        let bytes = f.ip.to_le_bytes();
        for b in bytes {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    h
}

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
    /// Accumulated call-site observations across samples.
    /// Key = fingerprint of the top-6 frame IPs.
    site_map: HashMap<u64, AccumSite>,
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
            site_map: HashMap::new(),
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
        // list_threads() provides per-thread TID list for the TUI threads view.
        let thread_ids = crate::attach::list_threads(self.pid);
        let thread_count = thread_ids.len() as u32;
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

        // Accumulate this sample into the per-call-stack site map.
        // Each unique call stack (fingerprinted by its top-6 IPs) becomes one
        // "allocation site", allowing the hotspot view to show which functions
        // were active while the heap held the most memory.
        // Accumulate this sample into the per-call-stack site map.
        if !frames.is_empty() {
            let key = stack_fingerprint(&frames);
            let entry = self.site_map.entry(key).or_insert(AccumSite {
                frames: frames.clone(),
                live_bytes: 0,
                live_bytes_sum: 0,
                sample_count: 0,
            });
            entry.live_bytes = heap_bytes;
            entry.live_bytes_sum += heap_bytes;
            entry.sample_count += 1;
            entry.frames = frames; // keep freshest resolved names
        }

        // Build top_sites sorted by avg_live_bytes (highest first).
        // Using the average means both function_a (peak 100MB) and function_b
        // (peak 200MB) stay visible simultaneously even after one has freed its
        // allocation, because their averages reflect the whole observation window.
        let mut sorted: Vec<&AccumSite> = self.site_map.values().collect();
        sorted.sort_by(|a, b| b.avg_live_bytes().cmp(&a.avg_live_bytes())
            .then(b.sample_count.cmp(&a.sample_count)));

        let top_sites: Vec<AllocationSite> = sorted
            .into_iter()
            .take(self.config.top_n)
            .map(|s| AllocationSite {
                live_bytes: s.avg_live_bytes(),
                alloc_count: s.sample_count,
                frames: s.frames.clone(),
            })
            .collect();

        Ok(SampleFrame {
            timestamp_ms: elapsed_ms,
            live_heap_bytes: heap_bytes,
            alloc_rate,
            free_rate,
            top_sites,
            thread_count,
            thread_ids,
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
