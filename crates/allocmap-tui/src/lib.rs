/// allocmap-tui: ratatui-based terminal UI for allocmap
///
/// Color conventions:
/// - Green:  normal state (stable memory)
/// - Yellow: memory growing (>1MB/s)
/// - Red:    fast growth (>10MB/s) or likely leak
/// - Cyan:   informational data (PID, program name, etc.)
/// - White:  general text
pub mod app;
pub mod events;
pub mod flamegraph;
pub mod hotspot;
pub mod theme;
pub mod timeline;

pub use app::{App, DisplayMode};
pub use events::{poll_event, AppEvent};
pub use theme::Theme;

use std::io;
use std::time::Duration;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use allocmap_core::SampleFrame;
use tokio::sync::mpsc;

/// Initialize the terminal for TUI rendering.
pub fn init_terminal() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend).map_err(io::Error::other)
}

/// Restore the terminal to its original state.
pub fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()
}

/// Install a panic hook that restores the terminal before printing the panic message.
pub fn install_panic_hook() {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Best-effort terminal cleanup — ignore errors
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_hook(info);
    }));
}

/// Run the TUI event loop.
///
/// Reads `SampleFrame`s from `rx`, updates app state, and redraws the terminal.
/// Returns when the user presses `q` or `duration` (if set) has elapsed.
pub async fn run_tui_loop(
    app: &mut App,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    rx: &mut mpsc::Receiver<SampleFrame>,
    duration: Option<Duration>,
) -> io::Result<()> {
    use ratatui::layout::{Constraint, Direction, Layout};
    use ratatui::style::{Color, Style};
    use ratatui::widgets::{Block, Borders, Paragraph};

    let start = std::time::Instant::now();

    loop {
        // Check duration limit
        if let Some(dur) = duration {
            if start.elapsed() >= dur {
                break;
            }
        }
        if app.should_quit {
            break;
        }

        // Drain any pending frames (non-blocking)
        while let Ok(frame) = rx.try_recv() {
            app.push_frame(frame);
        }

        // Draw the current state
        terminal.draw(|f| {
            let size = f.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // header block
                    Constraint::Length(1), // stats bar
                    Constraint::Min(10),   // main content
                    Constraint::Length(1), // keybindings hint
                ])
                .split(size);

            // ── Header ─────────────────────────────────────────────────────────
            let elapsed = app.elapsed_secs();
            let replay_tag = if app.is_replay {
                format!(" (REPLAY ×{:.2})", app.replay_speed)
            } else {
                String::new()
            };
            let header_text = format!(
                " allocmap · pid={} ({}){} · {:02}:{:02}:{:02} · {} samples ",
                app.pid,
                app.program_name,
                replay_tag,
                elapsed / 3600,
                (elapsed % 3600) / 60,
                elapsed % 60,
                app.total_samples,
            );
            let header = Paragraph::new(header_text)
                .style(Style::default().fg(Color::Cyan))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Theme::border()),
                );
            f.render_widget(header, chunks[0]);

            // ── Stats bar ──────────────────────────────────────────────────────
            let heap_str = timeline::format_bytes(app.current_heap_bytes());
            let growth = app.growth_rate_bytes_per_sec();
            let growth_str = if growth >= 0.0 {
                format!("△ +{}/s", timeline::format_bytes(growth as u64))
            } else {
                format!("▽ -{}/s", timeline::format_bytes((-growth) as u64))
            };
            let thread_count = app.latest_frame().map(|f| f.thread_count).unwrap_or(1);
            let peak_str = timeline::format_bytes(app.peak_heap_bytes.max(app.current_heap_bytes()));
            let stats_text = format!(
                " LIVE: {} / {} peak  {}  ALLOCS: {}/s  FREES: {}/s  THREADS: {}",
                heap_str,
                peak_str,
                growth_str,
                timeline::format_bytes(app.current_alloc_rate() as u64),
                timeline::format_bytes(app.current_free_rate() as u64),
                thread_count,
            );
            let stats_color = if growth > 10.0 * 1_048_576.0 {
                Color::Red
            } else if growth > 1_048_576.0 {
                Color::Yellow
            } else {
                Color::Green
            };
            let stats = Paragraph::new(stats_text).style(Style::default().fg(stats_color));
            f.render_widget(stats, chunks[1]);

            // ── Main content ───────────────────────────────────────────────────
            match app.mode {
                DisplayMode::Timeline => {
                    timeline::render_timeline(f, app, chunks[2]);
                }
                DisplayMode::Hotspot => {
                    hotspot::render_hotspot(f, app, chunks[2]);
                }
                DisplayMode::Flamegraph => {
                    flamegraph::render_flamegraph(f, app, chunks[2]);
                }
                DisplayMode::Threads => {
                    render_threads_panel(f, app, chunks[2]);
                }
            }

            // ── Keybindings hint ───────────────────────────────────────────────
            let keys_text = if app.is_replay {
                " [q]quit  [t]timeline  [h]hotspot  [T]threads  [Space]pause  [+/-]speed  [g]start  [G]end  [↑↓]scroll "
            } else {
                " [q]quit  [t]timeline  [h]hotspot  [f]flamegraph  [T]threads  [↑↓]scroll  [Enter]expand "
            };
            let keys = Paragraph::new(keys_text)
            .style(Style::default().fg(Color::DarkGray));
            f.render_widget(keys, chunks[3]);
        })?;

        // Poll for a terminal event (100ms timeout)
        if let Ok(Some(event)) = poll_event(Duration::from_millis(100)) {
            app.on_event(event);
        }

        // Yield to the async runtime briefly to allow the sampler to produce frames
        tokio::time::sleep(Duration::from_millis(16)).await;
    }

    Ok(())
}

/// Render the threads panel showing active thread IDs for the target process.
fn render_threads_panel(
    f: &mut ratatui::Frame,
    app: &App,
    area: ratatui::layout::Rect,
) {
    use ratatui::layout::Constraint;
    use ratatui::style::{Color, Style};
    use ratatui::widgets::{Block, Borders, Paragraph, Row, Table};

    let block = Block::default()
        .title(" Thread List ")
        .borders(Borders::ALL)
        .border_style(Theme::border());

    if let Some(frame) = app.latest_frame() {
        if frame.thread_ids.is_empty() {
            let msg = Paragraph::new(
                " Thread IDs not available for this recording. ",
            )
            .style(Style::default().fg(Color::Yellow))
            .block(block);
            f.render_widget(msg, area);
        } else {
            let rows: Vec<Row> = frame
                .thread_ids
                .iter()
                .map(|tid| {
                    let role = if *tid == app.pid { "main" } else { "worker" };
                    Row::new(vec![format!("{}", tid), role.to_string()])
                })
                .collect();

            let header = Row::new(vec!["TID", "Role"])
                .style(Style::default().fg(Color::Cyan));

            let widths = [Constraint::Length(10), Constraint::Min(10)];
            let table = Table::new(rows, widths)
                .header(header)
                .block(block);
            f.render_widget(table, area);
        }
    } else {
        let msg = Paragraph::new(" No data yet — waiting for samples... ")
            .style(Style::default().fg(Color::DarkGray))
            .block(block);
        f.render_widget(msg, area);
    }
}
