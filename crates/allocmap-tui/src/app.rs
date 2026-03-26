use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use allocmap_core::SampleFrame;
use crate::events::AppEvent;

/// Display mode for the TUI
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMode {
    Timeline,
    Hotspot,
    Flamegraph,
    Threads,
}

impl DisplayMode {
    /// Parse from a string (case-insensitive)
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "hotspot" => DisplayMode::Hotspot,
            "flamegraph" => DisplayMode::Flamegraph,
            "threads" => DisplayMode::Threads,
            _ => DisplayMode::Timeline,
        }
    }
}

const MAX_FRAMES: usize = 500;

/// Main application state for the TUI
pub struct App {
    pub pid: u32,
    pub program_name: String,
    pub mode: DisplayMode,
    pub top_n: usize,
    pub frames: VecDeque<SampleFrame>,
    pub should_quit: bool,
    pub scroll_offset: usize,
    /// Which hotspot indices have their call stack expanded
    pub hotspot_expanded: Vec<bool>,
    pub total_samples: u64,
    pub start_time: Instant,
    /// True when replaying an .amr file rather than live sampling
    pub is_replay: bool,
    /// Playback speed multiplier (1.0 = real-time)
    pub replay_speed: f64,
    /// Whether replay is currently paused
    pub replay_paused: bool,
    /// Shared pause flag synchronized with the feeder task
    pub pause_flag: Option<Arc<AtomicBool>>,
    /// Shared seek target synchronized with the feeder task (u64::MAX = no seek pending)
    pub seek_target: Option<Arc<AtomicU64>>,
    /// Total duration of the recording in milliseconds (for jump-to-end)
    pub replay_total_ms: u64,
}

impl App {
    pub fn new(pid: u32, program_name: String, top_n: usize) -> Self {
        Self {
            pid,
            program_name,
            mode: DisplayMode::Timeline,
            top_n,
            frames: VecDeque::with_capacity(MAX_FRAMES),
            should_quit: false,
            scroll_offset: 0,
            hotspot_expanded: Vec::new(),
            total_samples: 0,
            start_time: Instant::now(),
            is_replay: false,
            replay_speed: 1.0,
            replay_paused: false,
            pause_flag: None,
            seek_target: None,
            replay_total_ms: 0,
        }
    }

    /// Create an App with a specific display mode
    pub fn new_with_mode(pid: u32, program_name: String, top_n: usize, mode: DisplayMode) -> Self {
        let mut app = Self::new(pid, program_name, top_n);
        app.mode = mode;
        app
    }

    /// Add a new sample frame to the app state
    pub fn push_frame(&mut self, frame: SampleFrame) {
        if self.frames.len() >= MAX_FRAMES {
            self.frames.pop_front();
        }
        self.frames.push_back(frame);
        self.total_samples += 1;
        // Grow expanded list to match hotspot count
        if let Some(latest) = self.frames.back() {
            let n = latest.top_sites.len();
            if self.hotspot_expanded.len() < n {
                self.hotspot_expanded.resize(n, false);
            }
        }
    }

    /// Get the latest frame if available
    pub fn latest_frame(&self) -> Option<&SampleFrame> {
        self.frames.back()
    }

    /// Get current live heap bytes
    pub fn current_heap_bytes(&self) -> u64 {
        self.frames.back().map(|f| f.live_heap_bytes).unwrap_or(0)
    }

    /// Get current alloc rate in bytes/sec
    pub fn current_alloc_rate(&self) -> f64 {
        self.frames.back().map(|f| f.alloc_rate).unwrap_or(0.0)
    }

    /// Get current free rate in bytes/sec
    pub fn current_free_rate(&self) -> f64 {
        self.frames.back().map(|f| f.free_rate).unwrap_or(0.0)
    }

    /// Compute bytes/sec growth rate from oldest to newest frame
    pub fn growth_rate_bytes_per_sec(&self) -> f64 {
        if self.frames.len() < 2 {
            return 0.0;
        }
        let newest = self.frames.back().unwrap();
        let oldest = self.frames.front().unwrap();
        let dt_ms = newest.timestamp_ms.saturating_sub(oldest.timestamp_ms) as f64;
        if dt_ms == 0.0 {
            return 0.0;
        }
        let byte_diff = newest.live_heap_bytes as f64 - oldest.live_heap_bytes as f64;
        byte_diff / (dt_ms / 1000.0)
    }

    /// Get elapsed time in whole seconds since the TUI started
    pub fn elapsed_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Handle an AppEvent
    pub fn on_event(&mut self, event: AppEvent) {
        if let AppEvent::Key(key) = event {
            self.on_key(key);
        }
    }

    /// Handle a key event, returning true if the app should quit.
    pub fn on_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                self.should_quit = true;
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            KeyCode::Char('t') => {
                self.mode = DisplayMode::Timeline;
            }
            KeyCode::Char('h') => {
                self.mode = DisplayMode::Hotspot;
            }
            KeyCode::Char('f') => {
                self.mode = DisplayMode::Flamegraph;
            }
            KeyCode::Char('T') => {
                self.mode = DisplayMode::Threads;
            }
            KeyCode::Down => {
                self.scroll_offset = self.scroll_offset.saturating_add(1);
            }
            KeyCode::Up => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
            }
            KeyCode::Enter => {
                // Toggle expansion of the currently selected hotspot
                let idx = self.scroll_offset;
                if idx < self.hotspot_expanded.len() {
                    self.hotspot_expanded[idx] = !self.hotspot_expanded[idx];
                }
            }
            KeyCode::Char(' ') if self.is_replay => {
                self.replay_paused = !self.replay_paused;
                if let Some(flag) = &self.pause_flag {
                    flag.store(self.replay_paused, Ordering::Relaxed);
                }
            }
            KeyCode::Char('g') if self.is_replay => {
                // Jump to beginning
                if let Some(target) = &self.seek_target {
                    target.store(0, Ordering::Release);
                }
                self.frames.clear();
                self.total_samples = 0;
            }
            KeyCode::Char('G') if self.is_replay => {
                // Jump to end
                if let Some(target) = &self.seek_target {
                    target.store(self.replay_total_ms, Ordering::Release);
                }
                self.frames.clear();
                self.total_samples = 0;
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                self.replay_speed = (self.replay_speed * 2.0).min(32.0);
            }
            KeyCode::Char('-') => {
                self.replay_speed = (self.replay_speed / 2.0).max(0.125);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use allocmap_core::SampleFrame;

    fn make_frame(heap_bytes: u64, timestamp_ms: u64) -> SampleFrame {
        SampleFrame {
            timestamp_ms,
            live_heap_bytes: heap_bytes,
            alloc_rate: 0.0,
            free_rate: 0.0,
            top_sites: vec![],
            thread_count: 0,
            thread_ids: vec![],
        }
    }

    // ── Success tests ────────────────────────────────────────────────────────

    #[test]
    fn test_app_new_initial_state() {
        let app = App::new(1234, "test_prog".to_string(), 20);
        assert_eq!(app.pid, 1234);
        assert_eq!(app.program_name, "test_prog");
        assert_eq!(app.top_n, 20);
        assert_eq!(app.current_heap_bytes(), 0);
        assert!(!app.should_quit);
        assert_eq!(app.mode, DisplayMode::Timeline);
        assert_eq!(app.total_samples, 0);
        assert!(app.frames.is_empty());
    }

    #[test]
    fn test_push_frame_updates_heap_and_sample_count() {
        let mut app = App::new(1, "test".to_string(), 20);
        app.push_frame(make_frame(1024 * 1024, 0));
        assert_eq!(app.current_heap_bytes(), 1024 * 1024);
        assert_eq!(app.total_samples, 1);

        app.push_frame(make_frame(2 * 1024 * 1024, 1000));
        assert_eq!(app.current_heap_bytes(), 2 * 1024 * 1024);
        assert_eq!(app.total_samples, 2);
    }

    #[test]
    fn test_new_with_mode_sets_mode() {
        let app = App::new_with_mode(42, "prog".to_string(), 10, DisplayMode::Hotspot);
        assert_eq!(app.mode, DisplayMode::Hotspot);
        assert_eq!(app.pid, 42);
    }

    #[test]
    fn test_growth_rate_two_frames() {
        let mut app = App::new(1, "test".to_string(), 20);
        // 1 MB at t=0, 2 MB at t=1000ms → growth = 1MB/s
        app.push_frame(make_frame(1_048_576, 0));
        app.push_frame(make_frame(2_097_152, 1000));
        let rate = app.growth_rate_bytes_per_sec();
        assert!((rate - 1_048_576.0).abs() < 1.0, "Expected ~1MB/s growth, got {rate}");
    }

    // ── Failure / boundary tests ─────────────────────────────────────────────

    #[test]
    fn test_ring_buffer_capped_at_max_frames() {
        let mut app = App::new(1, "test".to_string(), 20);
        // Push more than MAX_FRAMES (500) frames
        for i in 0..600u64 {
            app.push_frame(make_frame(i * 1000, i * 100));
        }
        assert!(
            app.frames.len() <= 500,
            "Ring buffer should be capped at MAX_FRAMES=500, got {}",
            app.frames.len()
        );
        // The latest frame should be from the last push
        assert_eq!(app.current_heap_bytes(), 599 * 1000);
        // total_samples keeps counting beyond MAX_FRAMES
        assert_eq!(app.total_samples, 600);
    }

    #[test]
    fn test_current_heap_bytes_empty() {
        let app = App::new(1, "test".to_string(), 20);
        assert_eq!(app.current_heap_bytes(), 0, "Empty app should return 0 heap bytes");
    }

    #[test]
    fn test_growth_rate_single_frame_is_zero() {
        let mut app = App::new(1, "test".to_string(), 20);
        app.push_frame(make_frame(1_000_000, 0));
        assert_eq!(app.growth_rate_bytes_per_sec(), 0.0, "Single frame should yield zero growth rate");
    }

    // ── Key-event tests ──────────────────────────────────────────────────────

    #[test]
    fn test_on_key_q_sets_should_quit() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = App::new(1, "test".to_string(), 20);
        assert!(!app.should_quit);
        app.on_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(app.should_quit);
    }

    #[test]
    fn test_on_key_ctrl_c_sets_should_quit() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = App::new(1, "test".to_string(), 20);
        app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.should_quit);
    }

    #[test]
    fn test_on_key_mode_switching() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = App::new(1, "test".to_string(), 20);
        assert_eq!(app.mode, DisplayMode::Timeline);

        app.on_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
        assert_eq!(app.mode, DisplayMode::Hotspot);

        app.on_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE));
        assert_eq!(app.mode, DisplayMode::Flamegraph);

        app.on_key(KeyEvent::new(KeyCode::Char('T'), KeyModifiers::NONE));
        assert_eq!(app.mode, DisplayMode::Threads);

        app.on_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::NONE));
        assert_eq!(app.mode, DisplayMode::Timeline);
    }

    #[test]
    fn test_on_key_scroll() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = App::new(1, "test".to_string(), 20);
        assert_eq!(app.scroll_offset, 0);

        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.scroll_offset, 1);

        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.scroll_offset, 2);

        // Scrolling up from 2 → 1
        app.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(app.scroll_offset, 1);

        // Saturating: scrolling up from 0 stays at 0
        app.scroll_offset = 0;
        app.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(app.scroll_offset, 0);
    }

    // ── DisplayMode tests ────────────────────────────────────────────────────

    #[test]
    fn test_display_mode_parse_valid() {
        assert_eq!(DisplayMode::parse("timeline"), DisplayMode::Timeline);
        assert_eq!(DisplayMode::parse("hotspot"), DisplayMode::Hotspot);
        assert_eq!(DisplayMode::parse("flamegraph"), DisplayMode::Flamegraph);
        assert_eq!(DisplayMode::parse("threads"), DisplayMode::Threads);
    }

    #[test]
    fn test_display_mode_parse_case_insensitive() {
        assert_eq!(DisplayMode::parse("TIMELINE"), DisplayMode::Timeline);
        assert_eq!(DisplayMode::parse("HoTsPoT"), DisplayMode::Hotspot);
        assert_eq!(DisplayMode::parse("FLAMEGRAPH"), DisplayMode::Flamegraph);
        assert_eq!(DisplayMode::parse("THREADS"), DisplayMode::Threads);
    }

    #[test]
    fn test_display_mode_parse_threads() {
        assert_eq!(DisplayMode::parse("threads"), DisplayMode::Threads);
        assert_eq!(DisplayMode::parse("THREADS"), DisplayMode::Threads);
        assert_eq!(DisplayMode::parse("Threads"), DisplayMode::Threads);
    }

    #[test]
    fn test_display_mode_parse_unknown_defaults_to_timeline() {
        assert_eq!(DisplayMode::parse(""), DisplayMode::Timeline);
        assert_eq!(DisplayMode::parse("unknown"), DisplayMode::Timeline);
        assert_eq!(DisplayMode::parse("graph"), DisplayMode::Timeline);
    }

    #[test]
    fn test_replay_pause_flag_synced_on_space() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = App::new(1, "test".to_string(), 20);
        app.is_replay = true;
        let flag = Arc::new(AtomicBool::new(false));
        app.pause_flag = Some(Arc::clone(&flag));
        // Press Space → should set flag to true
        app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(app.replay_paused);
        assert!(flag.load(Ordering::Relaxed));
        // Press Space again → should clear flag
        app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(!app.replay_paused);
        assert!(!flag.load(Ordering::Relaxed));
    }

    #[test]
    fn test_replay_seek_g_sets_target_zero() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicU64, Ordering};
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = App::new(1, "test".to_string(), 20);
        app.is_replay = true;
        app.replay_total_ms = 60_000;
        let target = Arc::new(AtomicU64::new(u64::MAX));
        app.seek_target = Some(Arc::clone(&target));
        // Press g → seek to beginning (0)
        app.on_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
        assert_eq!(target.load(Ordering::Relaxed), 0);
        // Press G → seek to end (total_duration)
        app.on_key(KeyEvent::new(KeyCode::Char('G'), KeyModifiers::NONE));
        assert_eq!(target.load(Ordering::Relaxed), 60_000);
    }
}
