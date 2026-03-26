use anyhow::Result;
use clap::Args;
use owo_colors::OwoColorize;

#[derive(Args, Debug)]
pub struct AttachArgs {
    /// Target process PID
    #[arg(long, short)]
    pub pid: u32,

    /// Sampling duration (e.g. 30s, 5m). If omitted, runs until you press q.
    #[arg(long)]
    pub duration: Option<String>,

    /// Display the top N allocation hotspots
    #[arg(long, default_value = "20")]
    pub top: usize,

    /// Display mode: timeline, hotspot, or flamegraph
    #[arg(long, default_value = "timeline")]
    pub mode: String,

    /// Write results as JSON to this file path (non-interactive mode). Use "-" for stdout.
    #[arg(long)]
    pub output: Option<String>,

    /// Record session data to an .amr file
    #[arg(long)]
    pub record: Option<String>,

    /// Sampling frequency in Hz (default 50)
    #[arg(long, default_value = "50")]
    pub sample_rate: u32,
}

pub async fn execute(args: AttachArgs) -> Result<()> {
    let pid = args.pid;

    // Validate process exists
    if !std::path::Path::new(&format!("/proc/{}", pid)).exists() {
        anyhow::bail!(
            "Process {} not found. Make sure the PID is correct and the process is running.",
            pid
        );
    }

    // Parse optional duration
    let duration = args
        .duration
        .as_deref()
        .map(crate::util::parse_duration)
        .transpose()?;

    // Read program name from /proc/PID/comm
    let program_name = std::fs::read_to_string(format!("/proc/{}/comm", pid))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| format!("pid-{}", pid));

    println!(
        "{} Attaching to PID {} ({})...",
        "→".cyan().bold(),
        pid.yellow(),
        program_name.green()
    );

    #[cfg(not(target_os = "linux"))]
    anyhow::bail!("The 'attach' command is only supported on Linux.");

    #[cfg(target_os = "linux")]
    {
        use allocmap_ptrace::PtraceSampler;
        use tokio::sync::mpsc;

        let (tx, mut rx) = mpsc::channel::<allocmap_core::SampleFrame>(256);

        let dur_clone = duration;
        let tx_clone = tx.clone();

        // Spawn background sampling task.
        // IMPORTANT: attach AND sample must happen on the same OS thread because Linux ptrace
        // is per-thread — only the thread that called ptrace::attach may issue subsequent
        // ptrace operations.
        // The handle is intentionally not awaited — we drop it when TUI exits,
        // which signals the blocking thread to stop (tx_clone drops, blocking_send fails).
        let _sampling_handle = tokio::task::spawn_blocking(move || {
            let mut sampler = match PtraceSampler::attach(pid) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to attach to process {}: {}", pid, e);
                    return;
                }
            };
            let sample_interval = sampler.sample_interval();
            let start = std::time::Instant::now();
            loop {
                // Check duration limit
                if let Some(d) = dur_clone {
                    if start.elapsed() >= d {
                        break;
                    }
                }

                match sampler.sample() {
                    Ok(frame) => {
                        if tx_clone.blocking_send(frame).is_err() {
                            break; // Receiver dropped — TUI exited
                        }
                    }
                    Err(_) => break, // Process exited or unattachable
                }

                std::thread::sleep(sample_interval);
            }
        });

        // ----- Non-interactive JSON output mode -----
        if let Some(output_path) = &args.output {
            let collect_dur = duration.unwrap_or(std::time::Duration::from_secs(10));
            let deadline = std::time::Instant::now() + collect_dur;
            let mut frames = Vec::new();

            while std::time::Instant::now() < deadline {
                while let Ok(frame) = rx.try_recv() {
                    frames.push(frame);
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }

            let json = serde_json::to_string_pretty(&frames)?;
            if output_path == "-" {
                println!("{}", json);
            } else {
                std::fs::write(output_path, &json)?;
                eprintln!("Output written to {}", output_path);
            }
            return Ok(());
        }

        // ----- Interactive TUI mode -----
        allocmap_tui::install_panic_hook();

        let top_n = args.top;
        let mode = allocmap_tui::DisplayMode::parse(&args.mode);

        let mut app = allocmap_tui::App::new_with_mode(pid, program_name.clone(), top_n, mode);

        let mut terminal = allocmap_tui::init_terminal()
            .map_err(|e| anyhow::anyhow!("Failed to initialize terminal: {}", e))?;

        let result =
            allocmap_tui::run_tui_loop(&mut app, &mut terminal, &mut rx, duration).await;

        allocmap_tui::restore_terminal(&mut terminal)?;

        result.map_err(|e| anyhow::anyhow!("TUI error: {}", e))?;

        // ----- Optional .amr recording -----
        if let Some(record_path) = &args.record {
            use allocmap_core::recording::{
                AllocMapRecording, RecordingFooter, RecordingHeader, AMR_VERSION,
            };
            use std::time::{SystemTime, UNIX_EPOCH};

            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            // Compute elapsed before consuming app.frames
            let elapsed_ms = app.elapsed_secs() * 1000;
            let frames: Vec<_> = app.frames.into_iter().collect();
            let total_frames = frames.len() as u64;
            let peak_heap = frames.iter().map(|f| f.live_heap_bytes).max().unwrap_or(0);
            let avg_heap = if frames.is_empty() {
                0
            } else {
                frames.iter().map(|f| f.live_heap_bytes).sum::<u64>() / total_frames
            };

            let recording = AllocMapRecording {
                header: RecordingHeader {
                    version: AMR_VERSION,
                    pid,
                    program_name,
                    start_time_ms: now_ms.saturating_sub(elapsed_ms),
                    sample_rate_hz: args.sample_rate,
                    frame_count: total_frames,
                },
                frames,
                footer: RecordingFooter {
                    end_time_ms: now_ms,
                    total_frames,
                    peak_heap_bytes: peak_heap,
                    avg_heap_bytes: avg_heap,
                },
            };

            let mut file = std::fs::File::create(record_path)?;
            recording.write_to(&mut file).map_err(|e| {
                anyhow::anyhow!("Failed to write recording to '{}': {}", record_path, e)
            })?;
            eprintln!("Recording saved to {}", record_path);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    /// Test that a clearly invalid PID has no /proc entry.
    #[test]
    fn test_pid_validation_nonexistent() {
        assert!(
            !std::path::Path::new("/proc/99999999").exists(),
            "/proc/99999999 should not exist"
        );
    }

    /// Test that the current process always has a valid /proc entry.
    #[test]
    fn test_pid_validation_self() {
        let pid = std::process::id();
        assert!(
            std::path::Path::new(&format!("/proc/{}", pid)).exists(),
            "/proc/{{pid}} should always exist for the running process"
        );
    }

    /// Test that DisplayMode::parse returns the correct variants and defaults for unknowns.
    #[test]
    fn test_mode_parse_all_variants() {
        use allocmap_tui::DisplayMode;

        assert_eq!(DisplayMode::parse("timeline"), DisplayMode::Timeline);
        assert_eq!(DisplayMode::parse("hotspot"), DisplayMode::Hotspot);
        assert_eq!(DisplayMode::parse("flamegraph"), DisplayMode::Flamegraph);
        // Case-insensitive
        assert_eq!(DisplayMode::parse("TIMELINE"), DisplayMode::Timeline);
        assert_eq!(DisplayMode::parse("HOTSPOT"), DisplayMode::Hotspot);
        assert_eq!(DisplayMode::parse("FLAMeGraph"), DisplayMode::Flamegraph);
        // Unknown value falls back to Timeline
        assert_eq!(DisplayMode::parse("unknown"), DisplayMode::Timeline);
        assert_eq!(DisplayMode::parse(""), DisplayMode::Timeline);
    }
}
