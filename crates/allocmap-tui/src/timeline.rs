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

    // Build a simple bar/braille chart
    let chart_height = height.saturating_sub(1);
    let mut rows: Vec<String> = vec![String::new(); chart_height];

    for &val in &data {
        let bar_h = ((val as f64 / max_bytes as f64) * chart_height as f64) as usize;
        let bar_h = bar_h.min(chart_height);
        for (row, row_buf) in rows.iter_mut().enumerate() {
            let ch = if chart_height - row <= bar_h { '█' } else { ' ' };
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
