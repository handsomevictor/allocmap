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
    "black_box",
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

pub fn best_user_name(frames: &[StackFrame]) -> String {
    best_user_frame(frames)
        .and_then(|f| f.function.clone())
        .unwrap_or_else(|| "<unknown>".to_string())
}

/// Truncate to max_chars, appending '…' if truncated; pad with spaces otherwise.
fn truncate(s: &str, max_chars: usize) -> String {
    if max_chars == 0 { return String::new(); }
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        format!("{:<width$}", s, width = max_chars)
    } else {
        let mut out: String = chars[..max_chars - 1].iter().collect();
        out.push('\u{2026}');
        out
    }
}

/// Short file:line string from a frame.
fn format_file_line(frame: &StackFrame) -> String {
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
    if let Some(file) = &frame.file {
        if let Some(name) = std::path::Path::new(file.as_str()).file_name().and_then(|n| n.to_str()) {
            return name.to_string();
        }
    }
    if let Some(func) = &frame.function {
        if func.starts_with('<') && func.ends_with('>') {
            let inner = &func[1..func.len() - 1];
            return inner.to_string();
        }
    }
    "<system>".to_string()
}

fn detect_lang(frame: &StackFrame) -> (&'static str, Color) {
    if let Some(ref file) = frame.file {
        if file.ends_with(".rs") { return ("Rust", Color::Rgb(255, 165, 0)); }
        if file.ends_with(".cpp") || file.ends_with(".cc") || file.ends_with(".cxx") {
            return ("C++", Color::Blue);
        }
        if file.ends_with(".py") { return ("Py", Color::Yellow); }
        if file.contains("libpython") || file.contains("python") { return ("Py", Color::Yellow); }
        if file.contains("libjvm") { return ("Java", Color::Rgb(100, 149, 237)); }
    }
    if let Some(ref func) = frame.function {
        if func.starts_with("_Z") { return ("C++", Color::Blue); }
        if func.contains("::") && !func.starts_with('<') { return ("Rust", Color::Rgb(255, 165, 0)); }
        if func.contains("PyEval") || func.contains("Py_") { return ("Py", Color::Yellow); }
    }
    ("C", Color::White)
}

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

fn delta_for_site(name: &str, current_live: u64, app: &App) -> i64 {
    let n = app.frames.len();
    if n < 2 { return 0; }
    let prev_frame = &app.frames[n - 2];
    let prev_live = prev_frame
        .top_sites.iter()
        .find(|s| best_user_name(&s.frames) == name)
        .map(|s| s.live_bytes)
        .unwrap_or(0);
    current_live as i64 - prev_live as i64
}

fn live_pct_bar(pct: f64) -> String {
    let filled = ((pct / 100.0) * 8.0).round() as usize;
    let filled = filled.min(8);
    let empty  = 8 - filled;
    format!("{}{} {:4.1}%", "\u{2588}".repeat(filled), "\u{2591}".repeat(empty), pct)
}

fn pct_color(pct: f64) -> Color {
    if pct > 60.0 { Color::Red }
    else if pct > 30.0 { Color::Yellow }
    else { Color::Green }
}

// ── Column layout ─────────────────────────────────────────────────────────────

struct ColLayout {
    fn_w: usize,
    file_w: usize,
    show_delta: bool,
    show_avgsz: bool,
}

impl ColLayout {
    fn from_width(term_w: usize) -> Self {
        let show_delta = term_w >= 100;
        let show_avgsz = term_w >= 120;
        // Fixed columns (chars): # (4) + Lang (5) + Live (9) + LivePct (14) + Peak (8)
        // Optional:               Delta (10) + AvgSz (8)
        let fixed = 4 + 5 + 9 + 14 + 8
            + if show_delta { 10 } else { 0 }
            + if show_avgsz { 8 }  else { 0 };
        let avail = term_w.saturating_sub(fixed + 2);
        // Function gets 60% of available, File:Line gets 40%
        let fn_w   = ((avail * 60) / 100).max(35);
        let file_w = ((avail * 40) / 100).max(25);
        ColLayout { fn_w, file_w, show_delta, show_avgsz }
    }

    fn header_line(&self) -> String {
        let mut h = format!(
            "{:>3} {:<fn_w$} {:<file_w$} {:>4} {:>8} {:<13} {:>7}",
            "#", "Function", "File:Line", "Lang", "Live", "Live%", "Peak",
            fn_w = self.fn_w, file_w = self.file_w,
        );
        if self.show_delta { h.push_str(&format!(" {:>9}", "Delta")); }
        if self.show_avgsz { h.push_str(&format!(" {:>7}", "AvgSz")); }
        h
    }
}

// ── Row style helper ──────────────────────────────────────────────────────────

fn site_row_style(site_idx: usize, is_selected: bool) -> Style {
    if is_selected {
        Style::default().add_modifier(Modifier::BOLD).fg(Color::White)
    } else if site_idx == 0 {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else if site_idx < 3 {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Green)
    }
}

/// Build a single collapsed list item.
#[allow(clippy::too_many_arguments)]
fn collapsed_item(
    site_idx: usize,
    name: &str,
    file_line_str: &str,
    lang_label: &'static str,
    lang_color: Color,
    live_mb: f64,
    pct: f64,
    peak: u64,
    delta: i64,
    avg_sz: u64,
    is_selected: bool,
    col: &ColLayout,
) -> ListItem<'static> {
    let row_style   = site_row_style(site_idx, is_selected);
    let bar_color   = pct_color(pct);
    let delta_color = if delta > 0 { Color::Red } else if delta < 0 { Color::Green } else { Color::White };

    let func_str      = truncate(name, col.fn_w);
    let file_line_col = truncate(file_line_str, col.file_w);
    let live_str      = format!("{:>8}", format!("{:.1}MB", live_mb));
    let bar_str       = live_pct_bar(pct);
    let peak_str      = format!("{:>7}", format_bytes(peak));

    let mut spans: Vec<Span<'static>> = vec![
        Span::styled(format!("{:>3} ", site_idx + 1), Theme::dimmed()),
        Span::styled(func_str,      row_style),
        Span::raw(" "),
        Span::styled(file_line_col, Theme::dimmed()),
        Span::raw(" "),
        Span::styled(format!("{:>4}", lang_label), Style::default().fg(lang_color)),
        Span::raw(" "),
        Span::styled(live_str,      row_style),
        Span::raw(" "),
        Span::styled(bar_str,       Style::default().fg(bar_color)),
        Span::raw(" "),
        Span::styled(peak_str,      Theme::dimmed()),
    ];

    if col.show_delta {
        let delta_str = if delta > 0 {
            format!("{:>+9}", format!("+{}", format_bytes(delta as u64)))
        } else if delta < 0 {
            format!("{:>9}", format!("-{}", format_bytes((-delta) as u64)))
        } else {
            format!("{:>9}", "0")
        };
        spans.push(Span::raw(" "));
        spans.push(Span::styled(delta_str, Style::default().fg(delta_color)));
    }
    if col.show_avgsz {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(format!("{:>7}", format_bytes(avg_sz)), Theme::dimmed()));
    }

    ListItem::new(Line::from(spans))
}

/// Build expanded multi-line list items for one site (returns multiple ListItems).
#[allow(clippy::too_many_arguments)]
fn expanded_items(
    site_idx: usize,
    site: &allocmap_core::AllocationSite,
    name: &str,
    file_line_str: &str,
    lang_label: &'static str,
    lang_color: Color,
    live_mb: f64,
    pct: f64,
    peak: u64,
    delta: i64,
    avg_sz: u64,
    is_selected: bool,
) -> Vec<ListItem<'static>> {
    let row_style   = site_row_style(site_idx, is_selected);
    let delta_color = if delta > 0 { Color::Red } else if delta < 0 { Color::Green } else { Color::White };
    let delta_str   = if delta > 0 {
        format!("+{}", format_bytes(delta as u64))
    } else if delta < 0 {
        format!("-{}", format_bytes((-delta) as u64))
    } else {
        "\u{b1}0".to_string()
    };

    let mut items: Vec<ListItem<'static>> = Vec::new();

    // Line 1: index + full function name (bold)
    items.push(ListItem::new(Line::from(vec![
        Span::styled(format!("{:>3} ", site_idx + 1), Theme::dimmed()),
        Span::styled(name.to_string(), row_style.add_modifier(Modifier::BOLD)),
    ])));

    // Line 2: file:line · lang · stats
    items.push(ListItem::new(Line::from(vec![
        Span::raw("    "),
        Span::styled(file_line_str.to_string(), Theme::dimmed()),
        Span::styled(" \u{b7} ", Theme::dimmed()),
        Span::styled(lang_label.to_string(), Style::default().fg(lang_color)),
        Span::styled(
            format!(" \u{b7} Live: {:.1}MB ({:.1}%) \u{b7} Peak: {} \u{b7} Delta: {} \u{b7} Avg: {}",
                live_mb, pct,
                format_bytes(peak),
                delta_str,
                format_bytes(avg_sz),
            ),
            Style::default().fg(delta_color),
        ),
    ])));

    // Lines 3-5: call stack frames (up to 3, filtering stdlib when possible)
    let user_frames: Vec<&StackFrame> = site.frames.iter()
        .filter(|f| f.function.as_deref().map(|n| !is_stdlib(n)).unwrap_or(true))
        .take(3)
        .collect();
    let display_frames: Vec<&StackFrame> = if user_frames.is_empty() {
        site.frames.iter().take(3).collect()
    } else {
        user_frames
    };

    for frame in display_frames {
        let fname = frame.function.as_deref().unwrap_or("???");
        let loc = match (&frame.file, frame.line) {
            (Some(file), Some(line)) => {
                let short = std::path::Path::new(file.as_str())
                    .file_name().and_then(|n| n.to_str()).unwrap_or(file.as_str());
                format!(" ({short}:{line})")
            }
            (Some(file), None) => {
                let short = std::path::Path::new(file.as_str())
                    .file_name().and_then(|n| n.to_str()).unwrap_or(file.as_str());
                format!(" ({short})")
            }
            _ => String::new(),
        };
        items.push(ListItem::new(Line::from(vec![
            Span::styled("    \u{2514}\u{2500} ".to_string(), Theme::dimmed()),
            Span::styled(fname.to_string(), Theme::dimmed()),
            Span::styled(loc, Theme::dimmed()),
        ])));
    }

    // Blank separator
    items.push(ListItem::new(Line::raw("")));
    items
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Render the hotspot table showing top allocation sites.
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

    let sites        = &latest.top_sites;
    let total_live   = sites.iter().map(|s| s.live_bytes).sum::<u64>().max(1);
    let peak_heap    = app.peak_heap_bytes.max(app.current_heap_bytes());

    let title = format!(
        " Top Allocators \u{2014} Live: {} / Peak: {} \u{b7} {} sites ",
        format_bytes(app.current_heap_bytes()),
        format_bytes(peak_heap),
        sites.len(),
    );
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Theme::border());

    let col = ColLayout::from_width(area.width as usize);

    // Header
    let mut items: Vec<ListItem> = vec![
        ListItem::new(Line::from(Span::styled(
            col.header_line(),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ))),
    ];

    // Binary-deleted warning
    if app.binary_deleted {
        items.push(ListItem::new(Line::from(Span::styled(
            " \u{26a0} Binary was replaced. Restart the target process for accurate symbols.".to_string(),
            Style::default().fg(Color::Yellow),
        ))));
    }

    // Data rows
    for (site_idx, site) in sites.iter().enumerate().take(app.top_n) {
        let name     = best_user_name(&site.frames);
        let live_mb  = site.live_bytes as f64 / 1_048_576.0;
        let pct      = site.live_bytes as f64 / total_live as f64 * 100.0;
        let peak     = peak_for_site(&name, app).max(site.peak_bytes);
        let avg_sz   = site.live_bytes / site.alloc_count.max(1);
        let delta    = delta_for_site(&name, site.live_bytes, app);

        let (file_line_str, lang_label, lang_color) =
            if let Some(frame) = best_user_frame(&site.frames) {
                let fl   = format_file_line(frame);
                let (lang, lc) = detect_lang(frame);
                (fl, lang, lc)
            } else {
                ("<unknown>".to_string(), "C", Color::White)
            };

        let is_selected = site_idx == app.scroll_offset;
        let is_expanded = app.hotspot_all_expanded
            || app.hotspot_expanded.get(site_idx).copied().unwrap_or(false);

        if is_expanded {
            items.extend(expanded_items(
                site_idx, site, &name, &file_line_str,
                lang_label, lang_color,
                live_mb, pct, peak, delta, avg_sz,
                is_selected,
            ));
        } else {
            items.push(collapsed_item(
                site_idx, &name, &file_line_str,
                lang_label, lang_color,
                live_mb, pct, peak, delta, avg_sz,
                is_selected, &col,
            ));
        }
    }

    if items.len() <= 1 {
        items.push(ListItem::new("No allocation sites recorded yet"));
    }

    let list = List::new(items).block(block);
    f.render_widget(list, area);
}
