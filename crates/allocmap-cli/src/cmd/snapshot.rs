use anyhow::Result;
use clap::Args;
use owo_colors::OwoColorize;

#[derive(Args, Debug)]
pub struct SnapshotArgs {
    /// Target process PID
    #[arg(long, short)]
    pub pid: u32,

    /// Sampling duration (default 5s)
    #[arg(long, default_value = "5s")]
    pub duration: String,

    /// Write JSON output to this file (default: stdout)
    #[arg(long)]
    pub output: Option<String>,

    /// Display the top N allocation hotspots in the summary
    #[arg(long, default_value = "20")]
    pub top: usize,
}

pub async fn execute(args: SnapshotArgs) -> Result<()> {
    let pid = args.pid;

    // Validate process exists
    if !std::path::Path::new(&format!("/proc/{}", pid)).exists() {
        anyhow::bail!(
            "Process {} not found. Make sure the PID is correct and the process is running.",
            pid
        );
    }

    let duration = crate::util::parse_duration(&args.duration)?;

    println!(
        "{} Taking snapshot of PID {} for {}...",
        "→".cyan().bold(),
        pid.yellow(),
        args.duration.green()
    );

    #[cfg(not(target_os = "linux"))]
    anyhow::bail!("The 'snapshot' command is only supported on Linux.");

    #[cfg(target_os = "linux")]
    {
        use allocmap_ptrace::PtraceSampler;
        use tokio::sync::mpsc;

        let (tx, mut rx) = mpsc::channel::<allocmap_core::SampleFrame>(256);

        let dur_clone = duration;

        // Run sampling in a blocking thread — attach AND sample must happen on the same OS thread
        // because Linux ptrace is per-thread: only the thread that called ptrace::attach may
        // issue subsequent ptrace operations.
        tokio::task::spawn_blocking(move || {
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
                if start.elapsed() >= dur_clone {
                    break;
                }
                match sampler.sample() {
                    Ok(frame) => {
                        if tx.blocking_send(frame).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
                std::thread::sleep(sample_interval);
            }
        });

        // Collect frames for the duration
        let deadline = std::time::Instant::now() + duration;
        let mut frames = Vec::new();

        while std::time::Instant::now() < deadline {
            while let Ok(frame) = rx.try_recv() {
                frames.push(frame);
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        // Drain any remaining frames
        while let Ok(frame) = rx.try_recv() {
            frames.push(frame);
        }

        // Build summary statistics
        let sample_count = frames.len();
        let peak_heap = frames.iter().map(|f| f.live_heap_bytes).max().unwrap_or(0);
        let avg_heap = if frames.is_empty() {
            0
        } else {
            frames.iter().map(|f| f.live_heap_bytes).sum::<u64>() / sample_count as u64
        };

        // Top allocation sites from the last frame
        let top_sites: Vec<_> = frames
            .last()
            .map(|f| {
                let mut sites = f.top_sites.clone();
                sites.truncate(args.top);
                sites
            })
            .unwrap_or_default();

        let summary = serde_json::json!({
            "pid": pid,
            "sample_count": sample_count,
            "duration_ms": duration.as_millis(),
            "peak_heap_bytes": peak_heap,
            "avg_heap_bytes": avg_heap,
            "top_sites": top_sites,
            "frames": frames,
        });

        let json = serde_json::to_string_pretty(&summary)?;

        if let Some(output_path) = &args.output {
            std::fs::write(output_path, &json)?;
            eprintln!("Snapshot written to {}", output_path);
        } else {
            println!("{}", json);
        }
    }

    Ok(())
}
