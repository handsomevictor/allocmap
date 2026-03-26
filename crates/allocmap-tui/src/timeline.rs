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
const BLOCKS: &[char] = &[' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

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

/// Color for a bar based on its absolute heap as % of the global max
fn bar_color(heap: u64, global_max: u64) -> Color {
    if global_max == 0 {
        return Color::Green;
    }
    let pct = heap * 100 / global_max;
    if pct > 80 {
        Color::Red
    } else if pct > 50 {
        Color::Yellow
    } else {
        Color::Green
    }
}

/// Compute the Y-axis label for a given chart row.
/// Returns a 9-character string (right-padded / right-aligned).
fn y_label(row: usize, chart_height: usize, global_max: u64) -> String {
    let is_top = row == 0;
    let is_mid = row == chart_height / 2;
    let is_bot = row == chart_height.saturating_sub(1);

    let (val, corner) = if is_top {
        (global_max, '┤')
    } else if is_mid && chart_height > 2 {
        (global_max / 2, '┤')
    } else if is_bot {
        (0, '┴')
    } else {
        return "        │".to_string();
    };

    let s = format_bytes(val);
    // Right-align the value in 6 chars, then " " + corner
    format!("{:>6} {}", s, corner)
}

/// Render the timeline view showing heap memory over time
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

    // chart_height rows for bars, 1 for X-axis+ruler, 1 for stats
    let chart_height = inner_h.saturating_sub(2).max(1);
    let chart_cols  = inner_w - Y_LABEL_WIDTH;

    // ── Collect column data ──────────────────────────────────────────────────
    // committed columns + current partial (if any samples in current bucket)
    let mut all_heaps: Vec<u64> = app.timeline_columns.iter().map(|c| c.heap_bytes).collect();
    let mut all_end_ms: Vec<u64> = app.timeline_columns.iter().map(|c| c.end_ms).collect();

    // Add current partial bucket as rightmost column
    if app.bucket_count > 0 {
        let partial = app.bucket_sum / app.bucket_count;
        let partial_ms = app.frames.back().map(|fr| fr.timestamp_ms).unwrap_or(0);
        all_heaps.push(partial);
        all_end_ms.push(partial_ms);
    }

    // If no data yet, show waiting message
    if all_heaps.is_empty() {
        let p = Paragraph::new("Waiting for 5s of samples to build the first column...")
            .block(block)
            .style(Theme::dimmed());
        f.render_widget(p, area);
        return;
    }

    // Take only the last chart_cols worth of data
    let skip = all_heaps.len().saturating_sub(chart_cols);
    let col_heaps: Vec<u64> = all_heaps[skip..].to_vec();
    let col_ms: Vec<u64>    = all_end_ms[skip..].to_vec();

    let global_max = col_heaps.iter().copied().max().unwrap_or(1).max(1);
    let global_min = col_heaps.iter().copied().min().unwrap_or(0);
    let range = global_max.saturating_sub(global_min);

    // ── Build chart rows ─────────────────────────────────────────────────────
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(chart_height + 2);

    for row in 0..chart_height {
        let row_from_bottom = chart_height.saturating_sub(1) - row; // 0=bottom, top=max

        let ylabel = y_label(row, chart_height, global_max);
        let mut spans: Vec<Span<'static>> = vec![Span::raw(ylabel)];

        // Group consecutive columns with the same color into one span
        let mut group_str  = String::with_capacity(chart_cols);
        let mut group_col  = Color::White;
        let mut first = true;

        let n_data = col_heaps.len();
        // Build a padded column slice: data columns followed by empty placeholder slots
        let padded: Vec<Option<u64>> = col_heaps.iter().copied().map(Some)
            .chain(std::iter::repeat_n(None, chart_cols.saturating_sub(n_data)))
            .take(chart_cols)
            .collect();

        for maybe_heap in &padded {
            let heap = maybe_heap.unwrap_or(0);

            // Normalise heap → bar height in 1/8-row sub-units
            let norm: f64 = if range == 0 {
                0.5
            } else {
                heap.saturating_sub(global_min) as f64 / range as f64
            };
            let full_h  = (norm * (chart_height as f64 * 8.0)) as usize;
            let full_rows = full_h / 8;
            let frac      = (full_h % 8).min(BLOCKS.len() - 1);

            let ch = if maybe_heap.is_none() {
                ' ' // no data yet for this column slot
            } else if row_from_bottom < full_rows {
                '█'
            } else if row_from_bottom == full_rows {
                BLOCKS[frac]
            } else {
                ' '
            };

            let color = if maybe_heap.is_some() { bar_color(heap, global_max) } else { Color::DarkGray };

            if first || color != group_col {
                if !first {
                    spans.push(Span::styled(
                        group_str.clone(),
                        Style::default().fg(group_col),
                    ));
                }
                group_str = String::from(ch);
                group_col = color;
                first = false;
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
    // Build a ruler string: "         └" + "─" * chart_cols
    // Then overlay time labels every 6 columns (30 s) measured from right
    let ruler_prefix = format!("{:width$}└", "", width = Y_LABEL_WIDTH - 1);
    let mut ruler_chars: Vec<char> = "─".repeat(chart_cols).chars().collect();

    // Time labels: position from right, every 6 columns = 30s
    let latest_ms = col_ms.last().copied().unwrap_or(0);
    let n_cols = col_heaps.len();
    let mut label_positions: Vec<(usize, String)> = Vec::new();
    let mut secs_back = 0u64;
    loop {
        let cols_back = (secs_back / 5) as usize;
        if cols_back >= chart_cols { break; }
        // Position from left in the ruler
        let pos_from_left = chart_cols.saturating_sub(1 + cols_back);
        let absolute_ms = latest_ms.saturating_sub(secs_back * 1000);
        let label = if absolute_ms < latest_ms {
            format!("{}s", latest_ms.saturating_sub(absolute_ms) / 1000)
        } else {
            format!("{}s", app.elapsed_secs())
        };
        label_positions.push((pos_from_left, label));
        if secs_back == 0 {
            secs_back = 30;
        } else {
            secs_back += 30;
        }
        if secs_back > 3600 { break; }
    }

    // Write labels into ruler_chars
    for (pos, label) in &label_positions {
        for (i, ch) in label.chars().enumerate() {
            let idx = pos + i;
            if idx < ruler_chars.len() {
                ruler_chars[idx] = ch;
            }
        }
    }

    let ruler_str: String = ruler_chars.iter().collect();
    lines.push(Line::from(vec![
        Span::styled(ruler_prefix, Style::default().fg(Color::DarkGray)),
        Span::styled(ruler_str,    Style::default().fg(Color::DarkGray)),
    ]));

    // ── Stats label row ───────────────────────────────────────────────────────
    let latest_heap = app.current_heap_bytes();
    let growth      = app.growth_rate_bytes_per_sec();
    let sign        = if growth >= 0.0 { "+" } else { "-" };
    let n_cols_shown = n_cols.min(chart_cols);
    let label = format!(
        "{:width$}max={}  current={}  growth={}{}/s  samples={}  cols={}×5s",
        "",
        format_bytes(global_max),
        format_bytes(latest_heap),
        sign,
        format_bytes(growth.abs() as u64),
        app.total_samples,
        n_cols_shown,
        width = Y_LABEL_WIDTH,
    );
    let stats_style = Theme::for_growth_rate(growth);
    lines.push(Line::styled(label, stats_style));

    let p = Paragraph::new(lines).block(block);
    f.render_widget(p, area);
}
