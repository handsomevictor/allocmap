use ratatui::{
    layout::Rect,
    text::Line,
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use crate::app::App;
use crate::theme::Theme;

/// Format a byte count into a human-readable string
pub fn format_bytes(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.2} GB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.2} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1_024 {
        format!("{:.2} KB", bytes as f64 / 1_024.0)
    } else {
        format!("{} B", bytes)
    }
}

/// Render the timeline view showing heap memory over time
pub fn render_timeline(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Timeline — Heap Memory Over Time ")
        .borders(Borders::ALL)
        .border_style(Theme::border());

    if app.frames.is_empty() {
        let p = Paragraph::new("Waiting for samples...")
            .block(block)
            .style(Theme::dimmed());
        f.render_widget(p, area);
        return;
    }

    // Build a simple ASCII/Unicode braille-like representation
    let inner = block.inner(area);
    let width = inner.width as usize;
    let height = inner.height as usize;

    // Get heap data points (last `width` frames) — VecDeque supports iter()
    let frames = &app.frames;
    let n = frames.len().min(width);
    let skip = frames.len().saturating_sub(n);
    let data: Vec<u64> = frames.iter().skip(skip).map(|fr| fr.live_heap_bytes).collect();

    let max_bytes = data.iter().copied().max().unwrap_or(1).max(1);
    let min_bytes = data.iter().copied().min().unwrap_or(0);
    let range = max_bytes.saturating_sub(min_bytes);

    // Build a sparkline-style chart using 8-level Unicode block characters.
    // Each data column is encoded as: full rows of '█' + a partial tip char (▁▂▃▄▅▆▇).
    //
    // Y-axis uses min-max normalisation so the variance between phases is always
    // visible on screen:
    //   • range > 0  →  min_bytes maps to ~0, max_bytes maps to full height
    //   • range == 0 (all values identical, e.g. holding 100 MB for 3 s)
    //               →  bars are shown at 50% height so the chart is never
    //                  a solid wall of █
    const BLOCKS: &[char] = &[' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

    let chart_height = height.saturating_sub(1).max(1);
    let mut rows: Vec<String> = vec![String::new(); chart_height];

    for &val in &data {
        // Normalise to [0.0, 1.0]; flat signal → 0.5
        let norm = if range == 0 {
            0.5_f64
        } else {
            val.saturating_sub(min_bytes) as f64 / range as f64
        };

        // height in 1/8-row sub-units
        let full_h = (norm * (chart_height as f64 * 8.0)) as usize;
        let full_rows = full_h / 8;   // rows completely filled with '█'
        let frac     = (full_h % 8).min(BLOCKS.len() - 1);  // fractional tip

        for (row, row_buf) in rows.iter_mut().enumerate() {
            let row_from_bottom = chart_height - 1 - row;
            let ch = if row_from_bottom < full_rows {
                '█'
            } else if row_from_bottom == full_rows {
                BLOCKS[frac]
            } else {
                ' '
            };
            row_buf.push(ch);
        }
    }

    // Stats label row
    let latest_bytes = frames.back().map(|fr| fr.live_heap_bytes).unwrap_or(0);
    let growth = app.growth_rate_bytes_per_sec();
    let growth_sign = if growth >= 0.0 { "+" } else { "-" };
    let label = format!(
        "max={}  current={}  growth={}{}/s  samples={}",
        format_bytes(max_bytes),
        format_bytes(latest_bytes),
        growth_sign,
        format_bytes(growth.abs() as u64),
        app.total_samples,
    );

    let mut lines: Vec<Line> = rows.into_iter().map(Line::raw).collect();
    lines.push(Line::raw(label));

    // Determine color based on growth rate
    let style = Theme::for_growth_rate(growth);

    let p = Paragraph::new(lines)
        .block(block)
        .style(style);

    f.render_widget(p, area);
}
