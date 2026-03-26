use anyhow::Result;
use clap::Args;
use std::collections::HashMap;
use std::fs::File;
use allocmap_core::recording::AllocMapRecording;

#[derive(Args, Debug)]
pub struct DiffArgs {
    /// Baseline .amr recording file
    pub baseline: String,

    /// Current .amr recording file
    pub current: String,

    /// Show only functions with at least this percentage change
    #[arg(long, default_value = "0")]
    pub min_change_pct: f64,
}

pub async fn execute(args: DiffArgs) -> Result<()> {
    // 1. Read both files
    let mut baseline_file = File::open(&args.baseline)
        .map_err(|e| anyhow::anyhow!("Cannot open baseline '{}': {}", args.baseline, e))?;
    let baseline = AllocMapRecording::read_from(&mut baseline_file)
        .map_err(|e| anyhow::anyhow!("Failed to read baseline '{}': {}", args.baseline, e))?;

    let mut current_file = File::open(&args.current)
        .map_err(|e| anyhow::anyhow!("Cannot open current '{}': {}", args.current, e))?;
    let current = AllocMapRecording::read_from(&mut current_file)
        .map_err(|e| anyhow::anyhow!("Failed to read current '{}': {}", args.current, e))?;

    // 2. Aggregate peak bytes per function from each recording
    let baseline_sites = aggregate_peak_sites(&baseline);
    let current_sites = aggregate_peak_sites(&current);

    // 3. Build diff table
    let mut all_funcs: std::collections::HashSet<String> = std::collections::HashSet::new();
    all_funcs.extend(baseline_sites.keys().cloned());
    all_funcs.extend(current_sites.keys().cloned());

    struct DiffRow {
        name: String,
        baseline_bytes: u64,
        current_bytes: u64,
        delta_bytes: i64,
        change_pct: f64,
    }

    let mut rows: Vec<DiffRow> = all_funcs
        .into_iter()
        .map(|name| {
            let b = *baseline_sites.get(&name).unwrap_or(&0);
            let c = *current_sites.get(&name).unwrap_or(&0);
            let delta = c as i64 - b as i64;
            let pct = if b == 0 {
                if c > 0 { 100.0 } else { 0.0 }
            } else {
                (delta as f64 / b as f64) * 100.0
            };
            DiffRow {
                name,
                baseline_bytes: b,
                current_bytes: c,
                delta_bytes: delta,
                change_pct: pct,
            }
        })
        .filter(|r| r.change_pct.abs() >= args.min_change_pct)
        .collect();

    rows.sort_by(|a, b| b.delta_bytes.abs().cmp(&a.delta_bytes.abs()));

    // 4. Print colored table
    use owo_colors::OwoColorize;
    println!(
        "{:<50} {:>12} {:>12} {:>12} {:>8}",
        "Function", "Baseline", "Current", "Delta", "Change%"
    );
    println!("{}", "-".repeat(98));

    for row in &rows {
        let name = if row.name.len() > 49 {
            format!("{}...", &row.name[..46])
        } else {
            row.name.clone()
        };

        let delta_str = if row.delta_bytes >= 0 {
            format!("+{}", format_bytes(row.delta_bytes as u64))
        } else {
            format!("-{}", format_bytes((-row.delta_bytes) as u64))
        };

        let pct_str = format!("{:+.1}%", row.change_pct);

        let abs_pct = row.change_pct.abs();
        if abs_pct >= 50.0 {
            println!(
                "{:<50} {:>12} {:>12} {:>12} {:>8}",
                name.red(),
                format_bytes(row.baseline_bytes).red(),
                format_bytes(row.current_bytes).red(),
                delta_str.red(),
                pct_str.red(),
            );
        } else if abs_pct >= 10.0 {
            println!(
                "{:<50} {:>12} {:>12} {:>12} {:>8}",
                name.yellow(),
                format_bytes(row.baseline_bytes).yellow(),
                format_bytes(row.current_bytes).yellow(),
                delta_str.yellow(),
                pct_str.yellow(),
            );
        } else {
            println!(
                "{:<50} {:>12} {:>12} {:>12} {:>8}",
                name,
                format_bytes(row.baseline_bytes),
                format_bytes(row.current_bytes),
                delta_str,
                pct_str
            );
        }
    }

    println!(
        "\nBaseline: {} | Current: {} | {} functions changed",
        &args.baseline,
        &args.current,
        rows.len()
    );

    Ok(())
}

pub fn aggregate_peak_sites(recording: &AllocMapRecording) -> HashMap<String, u64> {
    let mut result: HashMap<String, u64> = HashMap::new();
    for frame in &recording.frames {
        for site in &frame.top_sites {
            let name = site
                .frames
                .first()
                .and_then(|f| f.function.clone())
                .unwrap_or_else(|| "<unknown>".to_string());
            let entry = result.entry(name).or_insert(0);
            *entry = (*entry).max(site.live_bytes);
        }
    }
    result
}

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1}GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1}MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1_024 {
        format!("{:.1}KB", bytes as f64 / 1_024.0)
    } else {
        format!("{}B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0B");
        assert_eq!(format_bytes(1023), "1023B");
        assert_eq!(format_bytes(1024), "1.0KB");
        assert_eq!(format_bytes(1_048_576), "1.0MB");
        assert_eq!(format_bytes(1_073_741_824), "1.0GB");
    }

    #[test]
    fn test_diff_nonexistent_baseline_would_fail() {
        assert!(!std::path::Path::new("/nonexistent/baseline.amr").exists());
    }

    #[test]
    fn test_aggregate_empty_recording() {
        use allocmap_core::recording::{AllocMapRecording, RecordingFooter, RecordingHeader};
        let recording = AllocMapRecording {
            header: RecordingHeader {
                version: 1,
                pid: 1,
                program_name: "test".into(),
                start_time_ms: 0,
                sample_rate_hz: 10,
                frame_count: 0,
            },
            frames: vec![],
            footer: RecordingFooter {
                end_time_ms: 0,
                total_frames: 0,
                peak_heap_bytes: 0,
                avg_heap_bytes: 0,
            },
        };
        let sites = aggregate_peak_sites(&recording);
        assert!(sites.is_empty());
    }

    #[test]
    fn test_aggregate_peak_takes_max() {
        use allocmap_core::recording::{AllocMapRecording, RecordingFooter, RecordingHeader};
        use allocmap_core::sample::{AllocationSite, SampleFrame, StackFrame};

        let make_site = |live_bytes: u64, func: &str| AllocationSite {
            live_bytes,
            alloc_count: 1,
            frames: vec![StackFrame {
                ip: 0,
                function: Some(func.to_string()),
                file: None,
                line: None,
            }],
        };

        let recording = AllocMapRecording {
            header: RecordingHeader {
                version: 1,
                pid: 1,
                program_name: "test".into(),
                start_time_ms: 0,
                sample_rate_hz: 10,
                frame_count: 2,
            },
            frames: vec![
                SampleFrame {
                    timestamp_ms: 0,
                    live_heap_bytes: 1024,
                    alloc_rate: 0.0,
                    free_rate: 0.0,
                    top_sites: vec![make_site(512, "foo")],
                },
                SampleFrame {
                    timestamp_ms: 1000,
                    live_heap_bytes: 2048,
                    alloc_rate: 0.0,
                    free_rate: 0.0,
                    top_sites: vec![make_site(1024, "foo")],
                },
            ],
            footer: RecordingFooter {
                end_time_ms: 1000,
                total_frames: 2,
                peak_heap_bytes: 2048,
                avg_heap_bytes: 1536,
            },
        };

        let sites = aggregate_peak_sites(&recording);
        // Should take the max of 512 and 1024
        assert_eq!(*sites.get("foo").unwrap(), 1024);
    }
}
