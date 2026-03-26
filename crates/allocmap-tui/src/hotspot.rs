use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};
use allocmap_core::StackFrame;
use crate::app::App;
use crate::theme::Theme;
use crate::timeline::format_bytes;

// ── stdlib filter (same logic as before, prevents false-positives) ──────────

const SKIP_CONTAINS: &[&str] = &[
    "nanosleep", "clock_nanosleep", "clock_gettime",
    "futex", "epoll_wait", "poll", "select",
    "pthread_cond", "pthread_mutex",
    "__GI_", "__kernel_", "__libc_",
    "libc::", "std::sys", "std::thread::sleep",
    "tokio::", "mio::",
    "malloc", "free", "calloc", "realloc",
    "clone", "sigreturn",
    "std::rt::",
    "black_box",       // core::hint::black_box
];

const SKIP_PREFIX: &[&str] = &[
    "alloc::",
    "core::",
    "std::",
];

fn is_stdlib(name: &str) -> bool {
    SKIP_CONTAINS.iter().any(|p| name.contains(p))
        || SKIP_PREFIX.iter().any(|p| name.starts_with(p))
        || (name.starts_with('<') && name.ends_with('>') && name.contains(".so"))
}

/// Best user-visible function name from a call stack
fn best_user_frame(frames: &[StackFrame]) -> Option<&StackFrame> {
    for f in frames {
        if let Some(ref n) = f.function {
            if !is_stdlib(n) { return Some(f); }
        }
    }
    for f in frames {
        if f.function.is_some() { return Some(f); }
    }
    frames.first()
}

fn best_user_name(frames: &[StackFrame]) -> String {
    best_user_frame(frames)
        .and_then(|f| f.function.clone())
        .unwrap_or_else(|| "<unknown>".to_string())
}

/// Truncate a string to max_chars, adding "…" if truncated
fn truncate(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        format!("{:<width$}", s, width = max_chars)
    } else {
        let mut out: String = chars[..max_chars - 1].iter().collect();
        out.push('…');
        out
    }
}

/// Scan the recent frame history to find the peak live_bytes for a site
/// identified by its best-user-frame function name.
fn peak_for_site(name: &str, app: &App) -> u64 {
    app.frames
        .iter()
        .flat_map(|f| f.top_sites.iter())
        .filter(|s| best_user_name(&s.frames) == name)
        .map(|s| s.live_bytes)
        .max()
        .unwrap_or(0)
        .max(app.frames.back()
            .and_then(|f| f.top_sites.iter()
                .find(|s| best_user_name(&s.frames) == name)
                .map(|s| s.peak_bytes))
            .unwrap_or(0))
}

/// Return Trend: +1 = rising, -1 = falling, 0 = stable
fn trend_for_site(name: &str, current_live: u64, app: &App) -> i8 {
    let lookback = app.frames.len().min(250);
    let old_frame = app.frames.get(app.frames.len().saturating_sub(lookback));
    let old_live = old_frame
        .and_then(|f| f.top_sites.iter()
            .find(|s| best_user_name(&s.frames) == name)
            .map(|s| s.live_bytes))
        .unwrap_or(0);

    let threshold = (current_live / 20).max(1); // 5% change threshold
    if current_live > old_live + threshold {
        1
    } else if old_live > current_live + threshold {
        -1
    } else {
        0
    }
}

/// Build a mini bar for the Live% column: 8 █/░ chars + " XX.X%"
fn live_pct_bar(pct: f64) -> String {
    let filled = ((pct / 100.0) * 8.0).round() as usize;
    let filled = filled.min(8);
    let empty  = 8 - filled;
    format!(
        "{}{} {:4.1}%",
        "█".repeat(filled),
        "░".repeat(empty),
        pct,
    )
}

/// Color for the Live% bar
fn pct_color(pct: f64) -> Color {
    if pct > 60.0 { Color::Red }
    else if pct > 30.0 { Color::Yellow }
    else { Color::Green }
}

/// Render the hotspot table showing top allocation sites
pub fn render_hotspot(f: &mut Frame, app: &App, area: Rect) {
    let latest = match app.latest_frame() {
        Some(fr) => fr,
        None => {
            let block = Block::default()
                .title(" Top Allocators ")
                .borders(Borders::ALL)
                .border_style(Theme::border());
            let p = ratatui::widgets::Paragraph::new("Waiting for samples...")
                .block(block).style(Theme::dimmed());
            f.render_widget(p, area);
            return;
        }
    };

    let sites = &latest.top_sites;
    let total_live: u64 = sites.iter().map(|s| s.live_bytes).sum::<u64>().max(1);
    let peak_heap = app.peak_heap_bytes.max(app.current_heap_bytes());

    // Block title with status line
    let title = format!(
        " Top Allocators — Live: {} / Peak: {} · {} sites ",
        format_bytes(app.current_heap_bytes()),
        format_bytes(peak_heap),
        sites.len(),
    );
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Theme::border());

    // ── Header row ────────────────────────────────────────────────────────────
    // Cols: #(3) func(36) live(9) live%(14) peak(9) alloc(6) avgSz(9) trend(2)
    let header_text = format!(
        "{:>3} {:<36} {:>9} {:<14} {:>9} {:>6} {:>9} {:>2}",
        "#", "Function", "Live", "Live%", "Peak", "Alloc", "AvgSz", "T",
    );
    let header_line = Line::from(vec![
        Span::styled(header_text, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    ]);

    let mut items: Vec<ListItem> = vec![ListItem::new(header_line)];

    // ── Data rows ─────────────────────────────────────────────────────────────
    // display_idx tracks position in the rendered list (for scroll_offset).
    // site_idx is the original rank index (for coloring and expansion state).
    for (display_idx, (site_idx, site)) in sites.iter().enumerate().take(app.top_n).enumerate() {
        let name = best_user_name(&site.frames);

        let live_mb  = site.live_bytes as f64 / 1_048_576.0;
        let pct      = site.live_bytes as f64 / total_live as f64 * 100.0;
        let peak     = peak_for_site(&name, app).max(site.peak_bytes);
        let alloc    = site.alloc_count;
        let avg_sz   = if alloc > 0 { site.live_bytes / alloc } else { site.live_bytes };
        let trend    = trend_for_site(&name, site.live_bytes, app);

        let func_str = truncate(&name, 36);
        let live_str = format!("{:>9}", format!("{:.1}MB", live_mb));
        let bar_str  = live_pct_bar(pct);
        let peak_str = format!("{:>9}", format_bytes(peak));
        let alloc_str = format!("{:>6}", alloc);
        let avg_sz_str = format!("{:>9}", format_bytes(avg_sz));
        let (trend_ch, trend_color) = match trend {
            1  => ("↑", Color::Red),
            -1 => ("↓", Color::Green),
            _  => ("→", Color::White),
        };

        let is_selected = display_idx == app.scroll_offset;
        let row_style = if is_selected {
            Style::default().add_modifier(Modifier::BOLD).fg(Color::White)
        } else if site_idx == 0 {
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        } else if site_idx < 3 {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::Green)
        };

        let bar_color = pct_color(pct);
        let line = Line::from(vec![
            Span::styled(format!("{:>3} ", site_idx + 1), Theme::dimmed()),
            Span::styled(func_str, row_style),
            Span::styled(" ", Style::default()),
            Span::styled(live_str, row_style),
            Span::styled(" ", Style::default()),
            Span::styled(bar_str, Style::default().fg(bar_color)),
            Span::styled(" ", Style::default()),
            Span::styled(peak_str, Theme::dimmed()),
            Span::styled(" ", Style::default()),
            Span::styled(alloc_str, Theme::dimmed()),
            Span::styled(" ", Style::default()),
            Span::styled(avg_sz_str, Theme::dimmed()),
            Span::styled(" ", Style::default()),
            Span::styled(format!("{:>2}", trend_ch), Style::default().fg(trend_color)),
        ]);
        items.push(ListItem::new(line));

        // If this site is expanded, show call stack
        let expanded = app.hotspot_expanded.get(site_idx).copied().unwrap_or(false);
        if expanded {
            // Show up to 6 frames
            let mut first = true;
            for frame in site.frames.iter().take(6) {
                let fname = frame.function.as_deref().unwrap_or("0x???");
                let loc = match (&frame.file, frame.line) {
                    (Some(file), Some(line)) => {
                        let short = std::path::Path::new(file)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(file);
                        format!(" ({short}:{line})")
                    }
                    _ => String::new(),
                };
                let prefix = if first { "  └─ " } else { "     └─ " };
                let sub_line = Line::from(vec![
                    Span::styled(prefix.to_string(), Theme::dimmed()),
                    Span::styled(fname.to_string(), Theme::dimmed()),
                    Span::styled(loc, Theme::dimmed()),
                ]);
                items.push(ListItem::new(sub_line));
                first = false;
            }
        }
    }

    if items.len() <= 1 {
        items.push(ListItem::new("No allocation sites recorded yet"));
    }

    let list = List::new(items).block(block);
    f.render_widget(list, area);
}
