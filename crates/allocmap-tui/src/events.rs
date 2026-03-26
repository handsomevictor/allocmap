use std::time::Duration;
use crossterm::event::{self, Event, KeyEvent, KeyEventKind};
use anyhow::Result;

/// Application-level events from the terminal
#[derive(Debug)]
pub enum AppEvent {
    Key(KeyEvent),
    Resize(u16, u16),
    Tick,
}

/// Poll for a terminal event with a timeout.
/// Returns Some(Tick) on timeout, Some(event) on event.
pub fn poll_event(timeout: Duration) -> Result<Option<AppEvent>> {
    if event::poll(timeout)? {
        match event::read()? {
            Event::Key(key) => {
                // Only handle key press events (not release or repeat)
                if key.kind == KeyEventKind::Press {
                    Ok(Some(AppEvent::Key(key)))
                } else {
                    Ok(Some(AppEvent::Tick))
                }
            }
            Event::Resize(w, h) => Ok(Some(AppEvent::Resize(w, h))),
            _ => Ok(Some(AppEvent::Tick)),
        }
    } else {
        Ok(Some(AppEvent::Tick))
    }
}
