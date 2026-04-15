use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::collector::{ApodEntry, FetchState};
use crate::theme::ThemePalette;

#[derive(Clone, Copy, PartialEq)]
pub enum ViewMode {
    Ascii,
    Pixels,
    Themed,
    Photo,
}

impl ViewMode {
    const ALL: &[ViewMode] = &[
        ViewMode::Ascii,
        ViewMode::Pixels,
        ViewMode::Themed,
        ViewMode::Photo,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ViewMode::Ascii => "ASCII",
            ViewMode::Pixels => "Pixels",
            ViewMode::Themed => "Themed",
            ViewMode::Photo => "Photo",
        }
    }
}

pub struct App {
    pub shared: Arc<Mutex<FetchState>>,
    pub entries: Vec<ApodEntry>,
    pub selected: usize,
    pub scroll_offset: u16,
    pub should_quit: bool,
    pub last_error: Option<String>,
    pub last_update: Option<Instant>,
    pub view_mode: ViewMode,
    pub palette: ThemePalette,
}

impl App {
    pub fn new(shared: Arc<Mutex<FetchState>>) -> Self {
        Self {
            shared,
            entries: Vec::new(),
            selected: 0,
            scroll_offset: 0,
            should_quit: false,
            last_error: None,
            last_update: None,
            view_mode: ViewMode::Themed,
            palette: crate::theme::detect(),
        }
    }

    pub fn toggle_view(&mut self) {
        let all = ViewMode::ALL;
        let idx = all.iter().position(|&m| m == self.view_mode).unwrap_or(0);
        self.view_mode = all[(idx + 1) % all.len()];
    }

    pub fn selected_entry(&self) -> Option<&ApodEntry> {
        self.entries.get(self.selected)
    }

    pub fn tick(&mut self) {
        let mut state = self.shared.lock().unwrap();
        if state.entries_updated {
            if let Some(entries) = state.entries.take() {
                self.entries = entries;
                if !self.entries.is_empty() {
                    self.selected = self.entries.len() - 1;
                }
                state.entries_updated = false;
                self.last_update = Some(Instant::now());
                self.last_error = None;
                self.scroll_offset = 0;
            }
        }
        if let Some(err) = state.error.take() {
            self.last_error = Some(err);
        }
    }

    pub fn select_next(&mut self) {
        if !self.entries.is_empty() {
            self.selected = (self.selected + 1) % self.entries.len();
            self.scroll_offset = 0;
        }
    }

    pub fn select_prev(&mut self) {
        if !self.entries.is_empty() {
            self.selected = if self.selected == 0 {
                self.entries.len() - 1
            } else {
                self.selected - 1
            };
            self.scroll_offset = 0;
        }
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::FetchState;

    fn test_app(entries: Vec<ApodEntry>) -> App {
        let shared = Arc::new(Mutex::new(FetchState::new(3_600_000, "DEMO_KEY".to_string())));
        let mut app = App::new(shared);
        app.entries = entries;
        app
    }

    fn test_entries() -> Vec<ApodEntry> {
        (0..3)
            .map(|i| ApodEntry {
                title: format!("Entry {i}"),
                explanation: format!("Explanation {i}"),
                date: format!("2026-04-{:02}", 13 + i),
                copyright: None,
                media_type: "image".to_string(),
                image: None,
                ascii_art: None,
            })
            .collect()
    }

    #[test]
    fn test_initial_state() {
        let app = test_app(Vec::new());
        assert!(!app.should_quit);
        assert!(app.entries.is_empty());
        assert_eq!(app.selected, 0);
        assert_eq!(app.scroll_offset, 0);
        assert!(app.last_error.is_none());
    }

    #[test]
    fn test_select_next_wraps() {
        let mut app = test_app(test_entries());
        assert_eq!(app.selected, 0);
        app.select_next();
        assert_eq!(app.selected, 1);
        app.select_next();
        assert_eq!(app.selected, 2);
        app.select_next();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_select_prev_wraps() {
        let mut app = test_app(test_entries());
        app.select_prev();
        assert_eq!(app.selected, 2);
    }

    #[test]
    fn test_select_next_empty() {
        let mut app = test_app(Vec::new());
        app.select_next();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_select_prev_empty() {
        let mut app = test_app(Vec::new());
        app.select_prev();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_scroll_resets_on_navigate() {
        let mut app = test_app(test_entries());
        app.scroll_down();
        app.scroll_down();
        assert_eq!(app.scroll_offset, 2);
        app.select_next();
        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn test_scroll_up_clamps_at_zero() {
        let mut app = test_app(test_entries());
        app.scroll_up();
        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn test_tick_picks_up_entries() {
        let mut app = test_app(Vec::new());
        {
            let mut state = app.shared.lock().unwrap();
            state.entries = Some(test_entries());
            state.entries_updated = true;
        }
        app.tick();
        assert_eq!(app.entries.len(), 3);
        assert_eq!(app.selected, 2); // defaults to most recent
        assert!(app.last_update.is_some());
    }

    #[test]
    fn test_tick_picks_up_error() {
        let mut app = test_app(Vec::new());
        {
            let mut state = app.shared.lock().unwrap();
            state.error = Some("Network error".to_string());
        }
        app.tick();
        assert_eq!(app.last_error.as_deref(), Some("Network error"));
    }

    #[test]
    fn test_selected_entry() {
        let app = test_app(test_entries());
        let entry = app.selected_entry().unwrap();
        assert_eq!(entry.title, "Entry 0");
    }
}
