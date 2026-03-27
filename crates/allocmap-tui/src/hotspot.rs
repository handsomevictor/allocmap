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

// ── stdlib filter ────────────────────────────────────────────────────────────
// SKIP_CONTAINS: matched via name.contains()  — safe because these strings
//   do not appear as substrings of legitimate user-code identifiers.
// SKIP_PREFIX:  matched via name.starts_with() — must NOT use contains() for
//   these because e.g. "alloc::" appears inside "spike_alloc::..." as a suffix.

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

/// Format file:line from a StackFrame.
/// Shows the last two path components to keep it compact.
/// Returns "<system>" for frames with no file info.
fn format_file_line(frame: &StackFrame) -> String {
    // Best case: have both file and line from debug info
    if let (Some(file), Some(line)) = (&frame.file, frame.line) {
        let path = std::path::Path::new(file.as_str());
        let components: Vec<_> = path.components().collect();
        let short = if components.len() >= 2 {
            let parent = components[components.len() - 2]
                .as_os_str().to_str().unwrap_or("?");
            let name = components[components.len() - 1]
                .as_os_str().to_str().unwrap_or("?");
            format!("{}/{}", parent, name)
        } else {
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(file.as_str())
                .to_string()
        };
        return format!("{}:{}", short, line);
    }
    // File but no line number
    if let Some(file) = &frame.file {
        if let Some(name) = std::path::Path::new(file.as_str()).file_name().and_then(|n| n.to_str()) {
            return name.to_string();
        }
    }
    // No file: try to extract library/binary name from "<libname.so.N>" function strings
    if let Some(func) = &frame.function {
        if func.starts_with('<') && func.ends_with('>') {
            let inner = &func[1..func.len() - 1];
            // e.g. "libc.so.6" -> show as-is
            return inner.to_string();
        }
    }
    "<system>".to_string()
}

/// Detect the source language from a StackFrame.
/// Returns (short label, color).
fn detect_lang(frame: &StackFrame) -> (&'static str, Color) {
    // File extension is the most reliable indicator
    if let Some(ref file) = frame.file {
        if file.ends_with(".rs") {
            return ("Rust", Color::Rgb(255, 165, 0)); // orange
        }
        if file.ends_with(".cpp") || file.ends_with(".cc") || file.ends_with(".cxx") {
            return ("C++", Color::Blue);
        }
        if file.ends_with(".py") {
            return ("Py", Color::Yellow);
        }
        if file.contains("libpython") || file.contains("python") {
            return ("Py", Color::Yellow);
        }
        if file.contains("libjvm") {
            return ("Java", Color::Rgb(100, 149, 237)); // cornflower blue
        }
    }
    // Fall back to function name mangling heuristics
    if let Some(ref func) = frame.function {
        if func.starts_with("_Z") {
            return ("C++", Color::Blue);
        }
        // Rust names contain "::" and often "<T as Trait>" patterns
        if func.contains("::") && !func.starts_with('<') {
            return ("Rust", Color::Rgb(255, 165, 0));
        }
        if func.contains("PyEval") || func.contains("Py_") {
            return ("Py", Color::Yellow);
        }
    }
    ("C", Color::White)
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

/// Compute the live-bytes delta vs the previous sample frame for a given site.
/// Positive = grew, negative = shrank, 0 = unchanged / not seen before.
fn delta_for_site(name: &str, current_live: u64, app: &App) -> i64 {
    let n = app.frames.len();
    if n < 2 {
        return 0;
    }
    let prev_frame = &app.frames[n - 2];
    let prev_live = prev_frame
        .top_sites
        .iter()
        .find(|s| best_user_name(&s.frames) == name)
        .map(|s| s.live_bytes)
        .unwrap_or(0);
    current_live as i64 - prev_live as i64
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

    // ── Column layout ─────────────────────────────────────────────────────────
    // # (3)  Function (22)  File:Line (16)  Lang (4)
    // Live (8)  Live% (13)  Peak (7)  Delta (8)  AvgSz (7)
    // Total with separating spaces ≈ 100 chars (fits 110+ terminal width)
    let header_text = format!(
        "{:>3} {:<22} {:<16} {:>4} {:>8} {:<13} {:>7} {:>9} {:>7}",
        "#", "Function", "File:Line", "Lang", "Live", "Live%", "Peak", "Delta", "AvgSz",
    );
    let header_line = Line::from(vec![
        Span::styled(header_text, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    ]);

    let mut items: Vec<ListItem> = vec![ListItem::new(header_line)];

    // ── Data rows ─────────────────────────────────────────────────────────────
    for (display_idx, (site_idx, site)) in sites.iter().enumerate().take(app.top_n).enumerate() {
        let name = best_user_name(&site.frames);

        let live_mb  = site.live_bytes as f64 / 1_048_576.0;
        let pct      = site.live_bytes as f64 / total_live as f64 * 100.0;
        let peak     = peak_for_site(&name, app).max(site.peak_bytes);
        let avg_sz   = {
            let alloc = site.alloc_count.max(1);
            site.live_bytes / alloc
        };
        let delta    = delta_for_site(&name, site.live_bytes, app);

        // File:Line + Lang come from the best user frame
        let (file_line_str, lang_label, lang_color) =
            if let Some(frame) = best_user_frame(&site.frames) {
                let fl = format_file_line(frame);
                let (lang, lc) = detect_lang(frame);
                (fl, lang, lc)
            } else {
                ("<unknown>".to_string(), "C", Color::White)
            };

        let func_str      = truncate(&name, 22);
        let file_line_col = truncate(&file_line_str, 16);
        let live_str      = format!("{:>8}", format!("{:.1}MB", live_mb));
        let bar_str       = live_pct_bar(pct);
        let peak_str      = format!("{:>7}", format_bytes(peak));
        let delta_str     = if delta > 0 {
            format!("{:>+9}", format!("+{}", format_bytes(delta as u64)))
        } else if delta < 0 {
            format!("{:>9}", format!("-{}", format_bytes((-delta) as u64)))
        } else {
            format!("{:>9}", "0")
        };
        let avg_sz_str    = format!("{:>7}", format_bytes(avg_sz));
        let delta_color   = if delta > 0 { Color::Red }
            else if delta < 0 { Color::Green }
            else { Color::White };

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
            Span::styled(func_str,      row_style),
            Span::styled(" ",           Style::default()),
            Span::styled(file_line_col, Theme::dimmed()),
            Span::styled(" ",           Style::default()),
            Span::styled(format!("{:>4}", lang_label), Style::default().fg(lang_color)),
            Span::styled(" ",           Style::default()),
            Span::styled(live_str,      row_style),
            Span::styled(" ",           Style::default()),
            Span::styled(bar_str,       Style::default().fg(bar_color)),
            Span::styled(" ",           Style::default()),
            Span::styled(peak_str,      Theme::dimmed()),
            Span::styled(" ",           Style::default()),
            Span::styled(delta_str,     Style::default().fg(delta_color)),
            Span::styled(" ",           Style::default()),
            Span::styled(avg_sz_str,    Theme::dimmed()),
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
                        let short = std::path::Path::new(file.as_str())
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(file.as_str());
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
