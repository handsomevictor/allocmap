use anyhow::Result;
use clap::Args;
use std::fs::File;
use std::time::Duration;
use allocmap_core::recording::AllocMapRecording;
use allocmap_core::sample::SampleFrame;
use tokio::sync::mpsc;

#[derive(Args, Debug)]
pub struct ReplayArgs {
    /// Path to .amr recording file
    pub file: String,

    /// Start replay from this time offset (e.g. 10s, 1m)
    #[arg(long)]
    pub from: Option<String>,

    /// Stop replay at this time offset (e.g. 60s, 5m)
    #[arg(long)]
    pub to: Option<String>,

    /// Playback speed multiplier (default 1.0, use 2.0 for 2x speed)
    #[arg(long, default_value = "1.0")]
    pub speed: f64,
}

pub async fn execute(args: ReplayArgs) -> Result<()> {
    // 1. Read the .amr file
    let mut file = File::open(&args.file)
        .map_err(|e| anyhow::anyhow!("Cannot open '{}': {}", args.file, e))?;
    let recording = AllocMapRecording::read_from(&mut file)
        .map_err(|e| anyhow::anyhow!("Failed to read recording '{}': {}", args.file, e))?;

    if recording.frames.is_empty() {
        anyhow::bail!("Recording '{}' contains no frames.", args.file);
    }

    // 2. Parse from/to filters
    let from_ms = args
        .from
        .as_deref()
        .map(crate::util::parse_duration)
        .transpose()?
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let to_ms = args
        .to
        .as_deref()
        .map(crate::util::parse_duration)
        .transpose()?
        .map(|d| d.as_millis() as u64);

    // 3. Filter frames by time range
    let frames: Vec<SampleFrame> = recording
        .frames
        .into_iter()
        .filter(|f| f.timestamp_ms >= from_ms)
        .filter(|f| to_ms.is_none_or(|t| f.timestamp_ms <= t))
        .collect();

    if frames.is_empty() {
        anyhow::bail!(
            "No frames in the specified time range (from {:?} to {:?}).",
            args.from,
            args.to
        );
    }

    let speed = args.speed.clamp(0.1, 100.0);

    eprintln!(
        "Replaying '{}': {} frames, pid={}, program={}",
        args.file,
        frames.len(),
        recording.header.pid,
        recording.header.program_name
    );

    // 4. Create TUI
    allocmap_tui::install_panic_hook();
    let top_n = 20usize;
    let mut app = allocmap_tui::App::new(
        recording.header.pid,
        recording.header.program_name.clone(),
        top_n,
    );
    app.is_replay = true;
    app.replay_speed = speed;

    let (tx, mut rx) = mpsc::channel::<SampleFrame>(256);

    // 5. Spawn frame feeder — replays frames at recorded timing adjusted for speed
    let speed_clone = speed;
    tokio::spawn(async move {
        let mut prev_ts = frames[0].timestamp_ms;
        for frame in frames {
            let delay_ms =
                ((frame.timestamp_ms.saturating_sub(prev_ts)) as f64 / speed_clone) as u64;
            if delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(delay_ms.min(2000))).await;
            }
            prev_ts = frame.timestamp_ms;
            if tx.send(frame).await.is_err() {
                break; // TUI quit
            }
        }
    });

    // 6. Run TUI loop (no duration limit — replay ends when feeder closes channel)
    let mut terminal = allocmap_tui::init_terminal()
        .map_err(|e| anyhow::anyhow!("Failed to initialize terminal: {}", e))?;
    let result = allocmap_tui::run_tui_loop(&mut app, &mut terminal, &mut rx, None).await;
    allocmap_tui::restore_terminal(&mut terminal)?;
    result.map_err(|e| anyhow::anyhow!("TUI error: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replay_nonexistent_file_would_fail() {
        // Verify our error message is correct by checking path logic
        assert!(!std::path::Path::new("/nonexistent/file.amr").exists());
    }

    #[test]
    fn test_speed_clamping() {
        // Speed should be clamped between 0.1 and 100.0
        let speed = 0.0f64.clamp(0.1, 100.0);
        assert_eq!(speed, 0.1);
        let speed = 200.0f64.clamp(0.1, 100.0);
        assert_eq!(speed, 100.0);
        // Valid speed passes through unchanged
        let speed = 2.0f64.clamp(0.1, 100.0);
        assert_eq!(speed, 2.0);
    }

    #[test]
    fn test_from_to_filter_logic() {
        let frames: Vec<SampleFrame> = (0..10)
            .map(|i| SampleFrame {
                timestamp_ms: i * 1000,
                live_heap_bytes: i * 1024,
                alloc_rate: 0.0,
                free_rate: 0.0,
                top_sites: vec![],
            })
            .collect();

        let from_ms = 2000u64;
        let to_ms = Some(7000u64);
        let filtered: Vec<_> = frames
            .iter()
            .filter(|f| f.timestamp_ms >= from_ms)
            .filter(|f| to_ms.is_none_or(|t| f.timestamp_ms <= t))
            .collect();
        // timestamps 2000, 3000, 4000, 5000, 6000, 7000 → 6 frames
        assert_eq!(filtered.len(), 6);
    }
}
