/// Flamegraph renderer for allocmap-tui.
///
/// Renders a horizontal flame chart from the ptrace-sampled allocation sites.
/// Each row represents one depth in the call stack (outermost at the bottom,
/// innermost at the top).  Block widths are proportional to the bytes
/// attributed to each function at that depth.
///
/// Color key:
///   Orange  — Rust  (.rs source / "::" name mangling)
///   Blue    — C++   (.cpp / _Z prefix)
///   Yellow  — Python
///   White   — C
///   Gray    — system / libc / unknown
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use allocmap_core::AllocationSite;
use crate::app::App;
use crate::theme::Theme;
use crate::timeline::format_bytes;

/// Minimum samples before we show data (avoids meaningless single-frame charts).
const MIN_SAMPLES: u64 = 10;

// ── Language detection ───────────────────────────────────────────────────────

fn detect_lang(function: Option<&str>, file: Option<&str>) -> (&'static str, Color) {
    if let Some(f) = file {
        if f.ends_with(".rs")  { return ("Rust", Color::Rgb(255, 165, 0)); }
        if f.ends_with(".cpp") || f.ends_with(".cc") || f.ends_with(".cxx") {
            return ("C++", Color::Blue);
        }
        if f.ends_with(".py") { return ("Py", Color::Yellow); }
        if f.contains("libjvm") { return ("Java", Color::Rgb(100, 149, 237)); }
    }
    if let Some(func) = function {
        if func.starts_with("_Z")                      { return ("C++", Color::Blue); }
        if func.contains("::") && !func.starts_with('<') { return ("Rust", Color::Rgb(255, 165, 0)); }
        if func.contains("PyEval") || func.contains("Py_") { return ("Py", Color::Yellow); }
        // libc / system library frames: wrapped as <libname.so.N>
        if func.starts_with('<') && func.ends_with('>') { return ("sys", Color::DarkGray); }
    }
    ("C", Color::White)
}

// ── Tree construction ────────────────────────────────────────────────────────

struct FlameBlock {
    name: String,
    bytes: u64,
    file: Option<String>,
    line: Option<u32>,
    lang: &'static str,
    lang_color: Color,
}

struct FlameLevel {
    blocks: Vec<FlameBlock>,
    /// Sum of bytes across all blocks at this depth level.
    total_bytes: u64,
}

/// Build flamegraph levels from allocation sites.
///
/// `levels[0]` = outermost callers (displayed at the bottom of the chart).
/// `levels[N]` = innermost frames (displayed at the top).
///
/// Each AllocationSite contributes its `live_bytes` to every frame in its
/// call stack, at the corresponding depth.  Frames with the same function
/// name at the same depth are merged and their bytes are summed.
fn build_levels(sites: &[AllocationSite]) -> Vec<FlameLevel> {
    use std::collections::BTreeMap;

    // depth_data[depth]: BTreeMap<function_name, depth_entry>
    // BTreeMap gives stable (alphabetical) ordering within each level.
    type DepthEntry = (u64, Option<String>, Option<u32>);
    let mut depth_data: Vec<BTreeMap<String, DepthEntry>> = Vec::new();

    for site in sites {
        // site.frames[0] = innermost (leaf); last = outermost (root-ish)
        // Reversed: index 0 = outermost = depth 0 of the flamegraph
        for (depth, frame) in site.frames.iter().rev().enumerate() {
            if depth >= depth_data.len() {
                depth_data.push(BTreeMap::new());
            }
            let name = frame.function.clone()
                .unwrap_or_else(|| format!("0x{:x}", frame.ip));
            let entry = depth_data[depth]
                .entry(name)
                .or_insert((0, frame.file.clone(), frame.line));
            entry.0 += site.live_bytes;
        }
    }

    depth_data
        .into_iter()
        .map(|map| {
            let mut blocks: Vec<FlameBlock> = map
                .into_iter()
                .map(|(name, (bytes, file, line))| {
                    let (lang, lang_color) =
                        detect_lang(Some(name.as_str()), file.as_deref());
                    FlameBlock { name, bytes, file, line, lang, lang_color }
                })
                .collect();
            // Sort largest-first so widest block is always on the left.
            blocks.sort_by(|a, b| b.bytes.cmp(&a.bytes));
            let total_bytes = blocks.iter().map(|b| b.bytes).sum();
            FlameLevel { blocks, total_bytes }
        })
        .collect()
}

// ── Block label formatting ───────────────────────────────────────────────────

/// Build the text label for a single flamegraph block.
///
/// Tries to fit `name + " XX%"`.  Falls back to truncated name, then just
/// the percentage, then spaces as the width shrinks.
fn block_label(name: &str, pct: f64, width: usize) -> String {
    if width == 0 { return String::new(); }
    if width <= 2 { return " ".repeat(width); }

    let pct_str = format!("{:.0}%", pct);

    // Attempt 1: name + " " + pct_str
    let full = format!("{} {}", name, pct_str);
    if full.chars().count() <= width {
        return format!("{:<width$}", full, width = width);
    }

    // Attempt 2: truncated name + "… " + pct_str
    let needed = pct_str.len() + 2; // "… XX%"
    if width > needed {
        let avail = width - needed;
        let truncated: String = name.chars().take(avail).collect();
        let s = format!("{}… {}", truncated, pct_str);
        return format!("{:<width$}", s, width = width);
    }

    // Attempt 3: just the percentage
    if width >= pct_str.len() {
        return format!("{:>width$}", pct_str, width = width);
    }

    // Fallback: fill with spaces
    " ".repeat(width)
}

// ── Row rendering ────────────────────────────────────────────────────────────

/// Render one horizontal row of the flamegraph.
///
/// Blocks are laid out left-to-right, widths proportional to bytes, separated
/// by a single `│` character.  The selected row gets bold text.
fn render_row(
    blocks: &[FlameBlock],
    total_bytes: u64,
    width: usize,
    is_selected: bool,
) -> Vec<Span<'static>> {
    if width == 0 {
        return vec![];
    }
    if blocks.is_empty() || total_bytes == 0 {
        return vec![Span::styled(
            " ".repeat(width),
            Style::default().fg(Color::DarkGray),
        )];
    }

    let modifier = if is_selected {
        Modifier::BOLD
    } else {
        Modifier::empty()
    };

    let n = blocks.len();
    // Account for separator chars between blocks (one │ per gap)
    let separators = n.saturating_sub(1);
    let available = width.saturating_sub(separators);

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut used_avail = 0usize;
    let mut used_total = 0usize;

    for (i, block) in blocks.iter().enumerate() {
        if used_avail >= available || used_total >= width {
            break;
        }
        let remaining_avail = available - used_avail;

        // Proportional block width; last block fills remaining space.
        let block_w = if i == n - 1 {
            remaining_avail
        } else {
            let w = ((block.bytes as f64 / total_bytes as f64) * available as f64)
                .round() as usize;
            w.min(remaining_avail)
        };

        if block_w == 0 {
            // Block too small to show; add separator if not the last block.
            if i < n - 1 && used_total < width {
                spans.push(Span::styled(
                    "│".to_string(),
                    Style::default().fg(Color::DarkGray),
                ));
                used_total += 1;
            }
            continue;
        }

        let pct = block.bytes as f64 / total_bytes as f64 * 100.0;
        let label = block_label(&block.name, pct, block_w);

        spans.push(Span::styled(
            label,
            Style::default().fg(block.lang_color).add_modifier(modifier),
        ));
        used_avail += block_w;
        used_total += block_w;

        // Add separator between blocks (not after the last one).
        if i < n - 1 && used_total < width {
            spans.push(Span::styled(
                "│".to_string(),
                Style::default().fg(Color::DarkGray),
            ));
            used_total += 1;
        }
    }

    // Pad any remaining columns (can occur when levels have fewer bytes than
    // the overall total, or when all blocks have been placed already).
    if used_total < width {
        spans.push(Span::styled(
            " ".repeat(width - used_total),
            Style::default().fg(Color::DarkGray),
        ));
    }

    spans
}

// ── Public entry point ───────────────────────────────────────────────────────

/// Render the flamegraph view.
///
/// Layout (bottom of panel = outermost callers):
/// ```text
/// ┌─ Flamegraph ─────────────────────────────────────────────────────────────┐
/// │ (empty rows if fewer levels than chart height)                           │
/// │ [nanosleep 89%                              ][…]  ← depth N (innermost)  │
/// │ [std::thread::sleep 89%                     ][…]  ← …                   │
/// │ [function_large 70%][function_small 19%][…]       ← depth 2             │
/// │ [spike_alloc::main 100%                         ]  ← depth 1            │
/// │ [start_thread 100%                              ]  ← depth 0 (outermost)│
/// │ [depth 2] spike_alloc::function_large_alloc — 300.0MB …     ← status   │
/// │ [↑↓] navigate depths   total: 430.0MB   5 sites                ← legend │
/// └──────────────────────────────────────────────────────────────────────────┘
/// ```
pub fn render_flamegraph(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Flamegraph — Heap by Call Stack Depth ")
        .borders(Borders::ALL)
        .border_style(Theme::border());

    let inner = block.inner(area);
    let inner_h = inner.height as usize;
    let inner_w = inner.width as usize;

    // ── Not enough data yet ──────────────────────────────────────────────────
    if app.total_samples < MIN_SAMPLES {
        let p = Paragraph::new(format!(
            " Collecting data... ({} samples, need {}+)",
            app.total_samples, MIN_SAMPLES,
        ))
        .block(block)
        .style(Theme::dimmed());
        f.render_widget(p, area);
        return;
    }

    let latest = match app.latest_frame() {
        Some(fr) => fr,
        None => {
            let p = Paragraph::new(" Waiting for samples... ")
                .block(block)
                .style(Theme::dimmed());
            f.render_widget(p, area);
            return;
        }
    };

    let sites = &latest.top_sites;
    if sites.is_empty() {
        let p = Paragraph::new(" No allocation sites recorded yet ")
            .block(block)
            .style(Theme::dimmed());
        f.render_widget(p, area);
        return;
    }

    // If no sites have call stack data, show a helpful diagnostic.
    let has_stack_data = sites.iter().any(|s| !s.frames.is_empty());
    if !has_stack_data {
        let msg = format!(
            " No call stack data. Ensure target binary has debug symbols (-g or debug=true)\n\n Samples with stack: {} / Total samples: {}",
            app.samples_with_stack, app.total_samples,
        );
        let p = Paragraph::new(msg)
            .block(block)
            .style(Style::default().fg(Color::Yellow));
        f.render_widget(p, area);
        return;
    }

    let session_total: u64 = sites.iter().map(|s| s.live_bytes).sum::<u64>().max(1);

    // ── Layout ───────────────────────────────────────────────────────────────
    // Reserve 2 rows at the bottom: 1 status + 1 legend.
    let flame_rows = inner_h.saturating_sub(2).max(1);

    // Build levels (level 0 = outermost callers, displayed at chart bottom).
    let levels = build_levels(sites);
    let n_levels = levels.len().min(flame_rows);

    // scroll_offset selects the highlighted depth level.
    let selected = app.scroll_offset.min(n_levels.saturating_sub(1));

    let mut lines: Vec<Line<'static>> = Vec::with_capacity(inner_h);

    // ── Empty rows at the top when chart has fewer levels than rows ──────────
    for _ in 0..flame_rows.saturating_sub(n_levels) {
        lines.push(Line::from(Span::raw(" ".repeat(inner_w))));
    }

    // ── Flame rows: deepest (top of chart) → shallowest (bottom of chart) ───
    // We render level n_levels-1 first (appears at top), level 0 last (appears
    // at the bottom, just above the status row).
    for level_idx in (0..n_levels).rev() {
        let level = &levels[level_idx];
        let is_selected_row = level_idx == selected;
        let spans = render_row(
            &level.blocks,
            level.total_bytes.max(1),
            inner_w,
            is_selected_row,
        );
        let mut line_spans = spans;

        // Add a left-side depth indicator for the selected row.
        if is_selected_row && inner_w >= 4 {
            // Replace first char with a '▶' marker (bold cyan).
            // We insert a 1-char span at the front and shorten the first
            // content span so the total width stays at inner_w.
            line_spans.insert(
                0,
                Span::styled(
                    "▶".to_string(),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
            );
            // Trim 1 char from the last text span to keep total width.
            if let Some(last) = line_spans.last_mut() {
                let s: String = last.content.chars().rev().skip(1).collect::<String>()
                    .chars().rev().collect();
                *last = Span::styled(s, last.style);
            }
        }

        lines.push(Line::from(line_spans));
    }

    // ── Status bar ───────────────────────────────────────────────────────────
    let status = if n_levels > 0 {
        let level = &levels[selected];
        if let Some(top) = level.blocks.first() {
            let loc = match (&top.file, top.line) {
                (Some(f), Some(l)) => {
                    let fname = std::path::Path::new(f.as_str())
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(f.as_str());
                    format!("  {}:{}", fname, l)
                }
                (Some(f), None) => {
                    let fname = std::path::Path::new(f.as_str())
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(f.as_str());
                    format!("  {}", fname)
                }
                _ => String::new(),
            };
            format!(
                " depth={} │ {} — {} ({:.1}%) [{}]{}",
                selected,
                top.name,
                format_bytes(top.bytes),
                top.bytes as f64 / session_total as f64 * 100.0,
                top.lang,
                loc,
            )
        } else {
            format!(" depth {} — no data", selected)
        }
    } else {
        " No call stack data available ".to_string()
    };
    lines.push(Line::styled(status, Style::default().fg(Color::Cyan)));

    // ── Legend / debug info ───────────────────────────────────────────────────
    let legend = format!(
        " [↑↓] depths  [t]timeline [h]hotspot  total: {}  {} sites  Samples with stack: {}/{} ",
        format_bytes(session_total),
        sites.len(),
        app.samples_with_stack,
        app.total_samples,
    );
    lines.push(Line::styled(legend, Theme::dimmed()));

    let p = Paragraph::new(lines).block(block);
    f.render_widget(p, area);
}
