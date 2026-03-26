use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};
use crate::app::App;
use crate::theme::Theme;

/// Render the hotspot list showing top allocation sites
pub fn render_hotspot(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Top Allocators ")
        .borders(Borders::ALL)
        .border_style(Theme::border());

    let latest = match app.latest_frame() {
        Some(f) => f,
        None => {
            let p = ratatui::widgets::Paragraph::new("Waiting for samples...")
                .block(block)
                .style(Theme::dimmed());
            f.render_widget(p, area);
            return;
        }
    };

    let sites = &latest.top_sites;
    let total_bytes: u64 = sites.iter().map(|s| s.live_bytes).sum::<u64>().max(1);

    let mut items: Vec<ListItem> = Vec::new();

    for (idx, site) in sites.iter().enumerate().take(app.top_n) {
        let pct = site.live_bytes as f64 / total_bytes as f64 * 100.0;
        let mb = site.live_bytes as f64 / (1024.0 * 1024.0);

        // Choose color based on rank
        let style = if idx == 0 {
            Theme::hotspot_top()
        } else if idx < 3 {
            Theme::hotspot_mid()
        } else {
            Theme::hotspot_low()
        };

        // Function name from top frame
        let func_name = site.frames.first()
            .map(|f| f.display_name())
            .unwrap_or_else(|| "<unknown>".to_string());

        let line = Line::from(vec![
            Span::styled(
                format!("{:>3}. ", idx + 1),
                Theme::dimmed(),
            ),
            Span::styled(
                format!("{:.1}MB ({:.1}%) ", mb, pct),
                style,
            ),
            Span::styled(
                format!("[{}x] ", site.alloc_count),
                Theme::info(),
            ),
            Span::styled(
                func_name,
                Theme::label(),
            ),
        ]);

        items.push(ListItem::new(line));

        // Show stack frames if expanded
        let expanded = app.hotspot_expanded.get(idx).copied().unwrap_or(false);
        if expanded {
            for (fi, frame) in site.frames.iter().enumerate().skip(1).take(5) {
                let frame_line = Line::from(vec![
                    Span::styled(
                        format!("       {:>2}: ", fi),
                        Theme::dimmed(),
                    ),
                    Span::styled(
                        frame.display_name(),
                        Theme::dimmed(),
                    ),
                ]);
                items.push(ListItem::new(frame_line));
            }
        }
    }

    if items.is_empty() {
        items.push(ListItem::new("No allocation sites recorded yet"));
    }

    let list = List::new(items).block(block);
    f.render_widget(list, area);
}
