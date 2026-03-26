use anyhow::Result;
use clap::Args;
use owo_colors::OwoColorize;

#[derive(Args, Debug)]
pub struct RunArgs {
    /// Command to run (pass after --)
    #[arg(trailing_var_arg = true, required = true)]
    pub command: Vec<String>,

    /// Extra environment variables to pass to the target (format: KEY=VALUE)
    #[arg(long = "env", short = 'e')]
    pub env_vars: Vec<String>,

    /// Display the top N allocation hotspots
    #[arg(long, default_value = "20")]
    pub top: usize,

    /// Sampling duration (e.g. 30s, 5m). If omitted, runs until the process exits or you press q.
    #[arg(long)]
    pub duration: Option<String>,

    /// Write results as JSON to this file path (non-interactive mode)
    #[arg(long)]
    pub output: Option<String>,

    /// Record session data to an .amr file
    #[arg(long)]
    pub record: Option<String>,

    /// Display mode: timeline, hotspot, or flamegraph
    #[arg(long, default_value = "timeline")]
    pub mode: String,
}

pub async fn execute(args: RunArgs) -> Result<()> {
    let program = args
        .command
        .first()
        .ok_or_else(|| anyhow::anyhow!("No command specified."))?;

    #[cfg(target_os = "linux")]
    let inject_var = "LD_PRELOAD";
    #[cfg(target_os = "macos")]
    let inject_var = "DYLD_INSERT_LIBRARIES";
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    anyhow::bail!("The 'run' command is only supported on Linux and macOS.");

    println!(
        "{} Launching {} with {} injection...",
        "→".cyan().bold(),
        program.yellow(),
        inject_var
    );

    // Validate --env format early
    for env_var in &args.env_vars {
        if !env_var.contains('=') {
            anyhow::bail!(
                "Invalid --env format '{}': expected KEY=VALUE",
                env_var
            );
        }
    }

    let duration = args
        .duration
        .as_deref()
        .map(crate::util::parse_duration)
        .transpose()?;

    // Locate the preload shared library
    let so_path = find_preload_so()?;

    // Create a temporary Unix socket path for IPC
    let socket_path = format!("/tmp/allocmap-{}.sock", std::process::id());

    // Build and spawn the child process
    let mut cmd = std::process::Command::new(&args.command[0]);
    cmd.args(&args.command[1..]);
    #[cfg(target_os = "linux")]
    cmd.env("LD_PRELOAD", &so_path);
    #[cfg(target_os = "macos")]
    cmd.env("DYLD_INSERT_LIBRARIES", &so_path);
    cmd.env("ALLOCMAP_SOCKET_PATH", &socket_path);

    for env_var in &args.env_vars {
        if let Some((key, val)) = env_var.split_once('=') {
            cmd.env(key, val);
        }
    }

    let mut child = cmd.spawn().map_err(|e| {
        anyhow::anyhow!("Failed to start '{}': {}", args.command[0], e)
    })?;

    let child_pid = child.id();
    eprintln!(
        "Started '{}' with PID {} ({} mode)",
        args.command[0], child_pid, inject_var
    );

    let program_name = std::path::Path::new(&args.command[0])
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&args.command[0])
        .to_string();

    let (tx, mut rx) = tokio::sync::mpsc::channel::<allocmap_core::SampleFrame>(256);
    let top_n = args.top;

    // Use ptrace-based sampling as a fallback data source so the TUI
    // shows live heap data even before the LD_PRELOAD IPC is plumbed end-to-end.
    #[cfg(target_os = "linux")]
    {
        use allocmap_ptrace::PtraceSampler;

        // Brief pause to let the child process start and be ready to ptrace.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        if std::path::Path::new(&format!("/proc/{}", child_pid)).exists() {
            match PtraceSampler::attach(child_pid) {
                Ok(mut sampler) => {
                    let sample_interval = sampler.sample_interval();
                    let dur_clone = duration;
                    let tx_clone = tx.clone();
                    // The handle is intentionally not awaited — we drop it when TUI exits,
                    // which signals the blocking thread to stop (tx_clone drops, blocking_send fails).
                    let _sampling_handle = tokio::task::spawn_blocking(move || {
                        let start = std::time::Instant::now();
                        loop {
                            if let Some(d) = dur_clone {
                                if start.elapsed() >= d {
                                    break;
                                }
                            }
                            match sampler.sample() {
                                Ok(frame) => {
                                    if tx_clone.blocking_send(frame).is_err() {
                                        break;
                                    }
                                }
                                Err(_) => break,
                            }
                            std::thread::sleep(sample_interval);
                        }
                    });
                }
                Err(e) => {
                    eprintln!(
                        "Warning: could not attach ptrace to child PID {}: {}. \
                         TUI will show empty data until LD_PRELOAD IPC is connected.",
                        child_pid, e
                    );
                }
            }
        }
    }

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
        let _ = std::fs::remove_file(&socket_path);
        let _ = child.try_wait();
        return Ok(());
    }

    // ----- Interactive TUI mode -----
    allocmap_tui::install_panic_hook();

    let mode = allocmap_tui::DisplayMode::parse(&args.mode);
    let mut app = allocmap_tui::App::new_with_mode(child_pid, program_name.clone(), top_n, mode);
    let mut terminal = allocmap_tui::init_terminal()
        .map_err(|e| anyhow::anyhow!("Failed to initialize terminal: {}", e))?;

    let result =
        allocmap_tui::run_tui_loop(&mut app, &mut terminal, &mut rx, duration).await;

    allocmap_tui::restore_terminal(&mut terminal)?;

    // Clean up IPC socket
    let _ = std::fs::remove_file(&socket_path);
    // Best-effort wait for child
    let _ = child.try_wait();

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
                pid: child_pid,
                program_name,
                start_time_ms: now_ms.saturating_sub(elapsed_ms),
                sample_rate_hz: 50,
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

    Ok(())
}

/// Locate the preload shared library relative to the current executable or project root.
/// On Linux the library is `liballocmap_preload.so`; on macOS it is `liballocmap_preload.dylib`.
fn find_preload_so() -> Result<std::path::PathBuf> {
    #[cfg(target_os = "linux")]
    let so_name = "liballocmap_preload.so";
    #[cfg(target_os = "macos")]
    let so_name = "liballocmap_preload.dylib";
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let so_name = "liballocmap_preload.so"; // fallback for unsupported platforms

    let exe_path = std::env::current_exe()?;
    let exe_dir = exe_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));

    let candidates = vec![
        exe_dir.join(so_name),
        exe_dir.join(format!("../lib/{}", so_name)),
        std::path::PathBuf::from(format!("target/debug/{}", so_name)),
        std::path::PathBuf::from(format!("target/release/{}", so_name)),
    ];

    for path in &candidates {
        if path.exists() {
            return Ok(path.clone());
        }
    }

    anyhow::bail!(
        "Cannot find {}. Looked in: {:?}. \
         Build the project first with 'cargo build', then run from the project root.",
        so_name,
        candidates
    )
}
