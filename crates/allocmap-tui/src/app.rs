use std::collections::VecDeque;
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
}

impl DisplayMode {
    /// Parse from a string (case-insensitive)
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "hotspot" => DisplayMode::Hotspot,
            "flamegraph" => DisplayMode::Flamegraph,
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

    /// Handle a key event
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
            _ => {}
        }
    }
}
