use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use crate::app::App;
use crate::theme::Theme;

const Y_LABEL_WIDTH: usize = 9; // "  202MB ┤"
/// Sub-cell block characters (1/8 row increments, index 0 = space)
const BLOCKS: &[char] = &[' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
/// Peak marker character shown above a bar when peak > avg within that bucket
const PEAK_MARKER: char = '▔';

/// Format a byte count into a compact human-readable string
pub fn format_bytes(bytes: u64) -> String {
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

/// Format elapsed milliseconds as a compact time label (used for X-axis)
fn format_ms(ms: u64) -> String {
    if ms >= 3_600_000 {
        let h = ms / 3_600_000;
        let m = (ms % 3_600_000) / 60_000;
        if m == 0 { format!("{}h", h) } else { format!("{}h{}m", h, m) }
    } else if ms >= 60_000 {
        let m = ms / 60_000;
        let s = (ms % 60_000) / 1_000;
        if s == 0 { format!("{}m", m) } else { format!("{}m{}s", m, s) }
    } else {
        format!("{}s", ms / 1_000)
    }
}

/// Choose a bar color based on the heap value's proportion of the locked Y-axis max.
///
/// Thresholds are percentage-based so they apply consistently to programs of
/// any memory scale (100 MB or 100 GB).
///
/// * `is_leaking` — true when the last 30 columns show monotonic growth;
///   colours all bars with LightRed as a leak-warning signal.
fn bar_color(heap: u64, y_max: u64, is_leaking: bool) -> Color {
    if is_leaking {
        return Color::LightRed;
    }
    if y_max == 0 {
        return Color::Green;
    }
    let pct = heap * 100 / y_max;
    if pct > 90 {
        Color::Red
    } else if pct > 70 {
        Color::Rgb(255, 165, 0) // orange
    } else if pct > 40 {
        Color::Yellow
    } else {
        Color::Green
    }
}

/// Compute the Y-axis label for a given chart row.
/// Returns a fixed 9-character string.
fn y_label(row: usize, chart_height: usize, y_max: u64) -> String {
    let is_top = row == 0;
    let is_mid = row == chart_height / 2;
    let is_bot = row == chart_height.saturating_sub(1);

    let (val, corner) = if is_top {
        (y_max, '┤')
    } else if is_mid && chart_height > 2 {
        (y_max / 2, '┤')
    } else if is_bot {
        (0, '┴')
    } else {
        return "        │".to_string();
    };

    let s = format_bytes(val);
    format!("{:>6} {}", s, corner)
}

/// Detect whether the last 30 committed columns show monotonically non-decreasing
/// heap usage (i.e. memory has grown for 30 consecutive seconds without any release).
fn detect_leak(app: &App) -> bool {
    let cols = &app.timeline_columns;
    if cols.len() < 30 {
        return false;
    }
    // Iterator is newest-first; w[0]=newer, w[1]=older → newer >= older means growth
    cols.iter().rev().take(30).map(|c| c.heap_bytes)
        .collect::<Vec<_>>()
        .windows(2)
        .all(|w| w[0] >= w[1])
}

/// Render the timeline view showing heap memory over time.
pub fn render_timeline(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Timeline — Heap Memory Over Time ")
        .borders(Borders::ALL)
        .border_style(Theme::border());

    let inner = block.inner(area);
    let inner_h = inner.height as usize;
    let inner_w = inner.width as usize;

    if inner_h < 4 || inner_w <= Y_LABEL_WIDTH {
        let p = Paragraph::new("Terminal too small").block(block).style(Theme::dimmed());
        f.render_widget(p, area);
        return;
    }

    // chart_height rows for bars, 1 for X-axis ruler, 1 for stats row
    let chart_height = inner_h.saturating_sub(2).max(1);
    let chart_cols  = inner_w - Y_LABEL_WIDTH;

    // ── Collect column data ──────────────────────────────────────────────────
    let mut all_avgs:   Vec<u64> = app.timeline_columns.iter().map(|c| c.heap_bytes).collect();
    let mut all_peaks:  Vec<u64> = app.timeline_columns.iter().map(|c| c.peak_bytes).collect();
    let mut all_end_ms: Vec<u64> = app.timeline_columns.iter().map(|c| c.end_ms).collect();

    // Add the current partial bucket as the rightmost live column
    if app.bucket_count > 0 {
        let partial_avg  = app.bucket_sum / app.bucket_count;
        let partial_peak = app.bucket_peak;
        let partial_ms   = app.frames.back().map(|fr| fr.timestamp_ms).unwrap_or(0);
        all_avgs.push(partial_avg);
        all_peaks.push(partial_peak);
        all_end_ms.push(partial_ms);
    }

    if all_avgs.is_empty() {
        let p = Paragraph::new("Waiting for 1s of samples to build the first column...")
            .block(block)
            .style(Theme::dimmed());
        f.render_widget(p, area);
        return;
    }

    // Show only the most recent chart_cols columns
    let skip = all_avgs.len().saturating_sub(chart_cols);
    let col_avgs:  Vec<u64> = all_avgs[skip..].to_vec();
    let col_peaks: Vec<u64> = all_peaks[skip..].to_vec();
    let col_ms:    Vec<u64> = all_end_ms[skip..].to_vec();

    // Locked Y-axis maximum — only ever grows, so historical bars never rescale downward
    let y_max = app.y_axis_max.max(1);

    // Leak detection: all 30 most-recent committed columns show non-decreasing heap
    let is_leaking = detect_leak(app);

    // ── Build chart rows ─────────────────────────────────────────────────────
    let n_data = col_avgs.len();

    // Pre-zip into Option<(avg, peak)> padded to chart_cols width
    let padded: Vec<Option<(u64, u64)>> = col_avgs.iter().copied()
        .zip(col_peaks.iter().copied())
        .map(|(a, p)| Some((a, p)))
        .chain(std::iter::repeat_n(None, chart_cols.saturating_sub(n_data)))
        .take(chart_cols)
        .collect();

    let mut lines: Vec<Line<'static>> = Vec::with_capacity(chart_height + 2);

    for row in 0..chart_height {
        // row 0 = top of chart; row_from_bottom 0 = bottom of chart
        let row_from_bottom = chart_height.saturating_sub(1) - row;

        let ylabel = y_label(row, chart_height, y_max);
        let mut spans: Vec<Span<'static>> = vec![Span::raw(ylabel)];

        let mut group_str = String::with_capacity(chart_cols);
        let mut group_col = Color::White;
        let mut first     = true;

        for maybe in &padded {
            let (ch, color) = match maybe {
                // Padding slot — no data yet
                None => (' ', Color::DarkGray),

                Some((avg, peak)) => {
                    // Average bar height in 1/8-row sub-units
                    let avg_h      = (*avg as f64 / y_max as f64 * chart_height as f64 * 8.0) as usize;
                    let avg_rows   = avg_h / 8;
                    let avg_frac   = (avg_h % 8).min(BLOCKS.len() - 1);

                    // Peak marker height
                    let peak_clamped = (*peak).min(y_max);
                    let peak_h       = (peak_clamped as f64 / y_max as f64 * chart_height as f64 * 8.0) as usize;
                    let peak_rows    = peak_h / 8;

                    let bcolor = bar_color(*avg, y_max, is_leaking);

                    // Peak marker: only shown when peak occupies a strictly higher row than avg
                    if peak_rows > avg_rows && row_from_bottom == peak_rows {
                        (PEAK_MARKER, Color::White)
                    } else if row_from_bottom < avg_rows {
                        // Inside the filled bar body
                        ('█', bcolor)
                    } else if row_from_bottom == avg_rows {
                        // Top fractional block of the bar
                        (BLOCKS[avg_frac], bcolor)
                    } else {
                        // Above bar (and above or below peak marker if any)
                        (' ', bcolor) // color used only for span-grouping; space is invisible
                    }
                }
            };

            // Group consecutive same-color chars into a single Span
            if first || color != group_col {
                if !first {
                    spans.push(Span::styled(
                        group_str.clone(),
                        Style::default().fg(group_col),
                    ));
                }
                group_str = String::from(ch);
                group_col = color;
                first     = false;
            } else {
                group_str.push(ch);
            }
        }
        if !group_str.is_empty() {
            spans.push(Span::styled(group_str, Style::default().fg(group_col)));
        }

        lines.push(Line::from(spans));
    }

    // ── X-axis ruler + time labels ───────────────────────────────────────────
    let ruler_prefix  = format!("{:width$}└", "", width = Y_LABEL_WIDTH - 1);
    let mut ruler_chars: Vec<char> = "─".repeat(chart_cols).chars().collect();

    // One label every 10 columns = 10 seconds.
    // Labels use the actual end_ms of that column so the timestamp is correct.
    const LABEL_STEP: usize = 10;
    let mut last_label_end = 0usize;
    for i in (0..n_data).step_by(LABEL_STEP) {
        if i < last_label_end { continue; }
        let ms    = col_ms.get(i).copied().unwrap_or(0);
        let label = format_ms(ms);
        for (j, ch) in label.chars().enumerate() {
            let idx = i + j;
            if idx < ruler_chars.len() {
                ruler_chars[idx] = ch;
            }
        }
        last_label_end = i + label.len() + 1; // +1 gap to avoid run-together labels
    }

    let ruler_str: String = ruler_chars.iter().collect();
    lines.push(Line::from(vec![
        Span::styled(ruler_prefix, Style::default().fg(Color::DarkGray)),
        Span::styled(ruler_str,    Style::default().fg(Color::DarkGray)),
    ]));

    // ── Stats row ────────────────────────────────────────────────────────────
    let latest_heap = app.current_heap_bytes();
    let growth      = app.growth_rate_bytes_per_sec();
    let sign        = if growth >= 0.0 { "+" } else { "-" };
    let n_cols_shown = n_data.min(chart_cols);
    let leak_tag    = if is_leaking { "  ⚠ LEAK?" } else { "" };
    let label = format!(
        "{:width$}max={}  current={}  growth={}{}/s  samples={}  cols={}×1s{}",
        "",
        format_bytes(y_max),
        format_bytes(latest_heap),
        sign,
        format_bytes(growth.abs() as u64),
        app.total_samples,
        n_cols_shown,
        leak_tag,
        width = Y_LABEL_WIDTH,
    );
    let stats_style = if is_leaking {
        Theme::for_growth_rate(f64::MAX) // always red when leaking
    } else {
        Theme::for_growth_rate(growth)
    };
    lines.push(Line::styled(label, stats_style));

    let p = Paragraph::new(lines).block(block);
    f.render_widget(p, area);
}
