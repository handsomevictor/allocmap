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

// Left column dots fill from bottom (bits 6,2,1,0 for dots 7,3,2,1)
const LEFT_BITS: &[u8] = &[0x00, 0x40, 0x44, 0x46, 0x47];
// Right column dots fill from bottom (bits 7,5,4,3 for dots 8,6,5,4)
const RIGHT_BITS: &[u8] = &[0x00, 0x80, 0xA0, 0xB0, 0xB8];

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
    // Iterator is newest-first; w[0]=newer, w[1]=older -> newer >= older means growth
    cols.iter().rev().take(30).map(|c| c.heap_bytes)
        .collect::<Vec<_>>()
        .windows(2)
        .all(|w| w[0] >= w[1])
}

/// Compute the fill level (0..=4) for a braille sub-column at a given chart row.
/// `avg` is in bytes, `y_max` is in bytes, `chart_height` is number of terminal rows.
/// `row_from_bottom` is 0 at the bottom row.
fn col_level(avg: u64, y_max: u64, chart_height: usize, row_from_bottom: usize) -> usize {
    let h = (avg as f64 / y_max as f64 * chart_height as f64 * 4.0) as usize;
    let full_rows = h / 4;
    let frac = h % 4;
    if row_from_bottom < full_rows { 4 }
    else if row_from_bottom == full_rows { frac }
    else { 0 }
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

    // Each char displays TWO data columns (left + right braille sub-columns)
    let max_data_cols = chart_cols * 2;
    let skip = all_avgs.len().saturating_sub(max_data_cols);
    let col_avgs:  Vec<u64> = all_avgs[skip..].to_vec();
    let col_peaks: Vec<u64> = all_peaks[skip..].to_vec();
    let col_ms:    Vec<u64> = all_end_ms[skip..].to_vec();

    // Y-axis max with 15% headroom above actual peak
    let y_max = ((app.y_axis_max as f64 * 1.15) as u64).max(1);

    // Leak detection: all 30 most-recent committed columns show non-decreasing heap
    let is_leaking = detect_leak(app);

    // ── Build chart rows ─────────────────────────────────────────────────────
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(chart_height + 2);

    for row in 0..chart_height {
        // row 0 = top of chart; row_from_bottom 0 = bottom of chart
        let row_from_bottom = chart_height.saturating_sub(1) - row;

        let ylabel = y_label(row, chart_height, y_max);
        let mut spans: Vec<Span<'static>> = vec![Span::raw(ylabel)];

        let mut group_str = String::with_capacity(chart_cols);
        let mut group_col = Color::White;
        let mut first     = true;

        for char_pos in 0..chart_cols {
            let left_i  = char_pos * 2;
            let right_i = char_pos * 2 + 1;

            let left  = col_avgs.get(left_i).copied();
            let right = col_avgs.get(right_i).copied();
            let left_peak  = col_peaks.get(left_i).copied();
            let right_peak = col_peaks.get(right_i).copied();

            let (ch, color) = if left.is_none() && right.is_none() {
                (' ', Color::DarkGray)
            } else {
                let la = left.unwrap_or(0);
                let ra = right.unwrap_or(0);
                let lp = left_peak.unwrap_or(la);
                let rp = right_peak.unwrap_or(ra);

                let ll = col_level(la, y_max, chart_height, row_from_bottom);
                let rl = if right.is_some() { col_level(ra, y_max, chart_height, row_from_bottom) } else { 0 };

                // Peak indicators: if peak is in a higher row, show top dot
                let lp_rows = (lp as f64 / y_max as f64 * chart_height as f64 * 4.0) as usize / 4;
                let rp_rows = (rp as f64 / y_max as f64 * chart_height as f64 * 4.0) as usize / 4;
                let la_rows = (la as f64 / y_max as f64 * chart_height as f64 * 4.0) as usize / 4;
                let ra_rows = (ra as f64 / y_max as f64 * chart_height as f64 * 4.0) as usize / 4;

                let left_has_peak  = lp_rows > la_rows && row_from_bottom == lp_rows;
                let right_has_peak = right.is_some() && rp_rows > ra_rows && row_from_bottom == rp_rows;

                let mut left_bits_val  = LEFT_BITS[ll.min(4)];
                let mut right_bits_val = RIGHT_BITS[if right.is_some() { rl.min(4) } else { 0 }];

                // Add peak dot at top of sub-column (dot 1 for left, dot 4 for right)
                if left_has_peak  { left_bits_val  |= 0x01; }  // dot 1 (top-left)
                if right_has_peak { right_bits_val |= 0x08; }  // dot 4 (top-right)

                let braille = char::from_u32(0x2800 + (left_bits_val | right_bits_val) as u32)
                    .unwrap_or('\u{2588}');

                let is_peak_only = (left_has_peak && ll == 0) || (right_has_peak && rl == 0);
                let dominant = la.max(ra);
                let color = if is_peak_only { Color::White } else { bar_color(dominant, y_max, is_leaking) };

                (braille, color)
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

    // X-axis labels: every 10 chars = every 20 data columns = every 20 seconds
    let mut last_label_end = 0usize;
    for char_pos in (0..chart_cols).step_by(10) {
        if char_pos < last_label_end { continue; }
        let data_idx = char_pos * 2; // corresponding data column
        if data_idx >= col_ms.len() { continue; }
        let ms = col_ms[data_idx];
        let label = format_ms(ms);
        for (j, ch) in label.chars().enumerate() {
            let idx = char_pos + j;
            if idx < ruler_chars.len() { ruler_chars[idx] = ch; }
        }
        last_label_end = char_pos + label.len() + 1;
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
    let n_cols_shown = col_avgs.len();  // actual data columns
    let leak_tag    = if is_leaking { "  \u{26a0} LEAK?" } else { "" };
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
