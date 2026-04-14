use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::collector::{Event, FetchState, SortOrder, SubMarket, Topic};

const FAST_REFRESH_MS: u64 = 5000;

pub struct App {
    pub shared: Arc<Mutex<FetchState>>,
    pub events: Vec<Event>,
    pub selected: usize,
    pub price_history: Vec<(f64, f64)>,
    pub should_quit: bool,
    pub fast_mode: bool,
    pub refresh_ms: u64,
    pub last_error: Option<String>,
    pub last_update: Option<Instant>,
    pub topic: Topic,
    pub sort_order: SortOrder,
    normal_refresh_ms: u64,
}

impl App {
    pub fn new(shared: Arc<Mutex<FetchState>>, refresh_ms: u64) -> Self {
        Self {
            shared,
            events: Vec::new(),
            selected: 0,
            price_history: Vec::new(),
            should_quit: false,
            fast_mode: false,
            refresh_ms,
            last_error: None,
            last_update: None,
            topic: Topic::All,
            sort_order: SortOrder::Volume24h,
            normal_refresh_ms: refresh_ms,
        }
    }

    pub fn selected_event(&self) -> Option<&Event> {
        self.events.get(self.selected)
    }

    pub fn selected_lead(&self) -> Option<&SubMarket> {
        self.selected_event().and_then(|e| e.lead_market())
    }

    pub fn tick(&mut self) {
        let mut state = self.shared.lock().unwrap();

        if state.events_updated {
            if let Some(events) = state.events.take() {
                self.events = events;
                if self.selected >= self.events.len() {
                    self.selected = 0;
                }
                state.events_updated = false;
                self.last_update = Some(Instant::now());
                self.last_error = None;
            }
        }

        if state.history_updated {
            if let Some(history) = state.price_history.take() {
                self.price_history = history;
                state.history_updated = false;
            }
        }

        if let Some(err) = state.error.take() {
            self.last_error = Some(err);
        }

        if let Some(lead) = self.selected_lead() {
            let current_request = state.requested_token_id.as_deref();
            if current_request != Some(&lead.yes_token_id) {
                state.requested_token_id = Some(lead.yes_token_id.clone());
            }
        }
    }

    pub fn select_next(&mut self) {
        if !self.events.is_empty() {
            self.selected = (self.selected + 1) % self.events.len();
            self.request_history_for_selected();
        }
    }

    pub fn select_prev(&mut self) {
        if !self.events.is_empty() {
            self.selected = if self.selected == 0 {
                self.events.len() - 1
            } else {
                self.selected - 1
            };
            self.request_history_for_selected();
        }
    }

    pub fn toggle_fast_mode(&mut self) {
        self.fast_mode = !self.fast_mode;
        self.refresh_ms = if self.fast_mode {
            FAST_REFRESH_MS
        } else {
            self.normal_refresh_ms
        };
        let mut state = self.shared.lock().unwrap();
        state.refresh_ms = self.refresh_ms;
    }

    pub fn cycle_topic(&mut self) {
        self.topic = self.topic.next();
        self.selected = 0;
        self.events.clear();
        self.price_history.clear();
        let mut state = self.shared.lock().unwrap();
        state.topic = self.topic;
        state.filter_changed = true;
    }

    pub fn cycle_topic_prev(&mut self) {
        self.topic = self.topic.prev();
        self.selected = 0;
        self.events.clear();
        self.price_history.clear();
        let mut state = self.shared.lock().unwrap();
        state.topic = self.topic;
        state.filter_changed = true;
    }

    pub fn cycle_sort(&mut self) {
        self.sort_order = self.sort_order.next();
        self.selected = 0;
        self.events.clear();
        self.price_history.clear();
        let mut state = self.shared.lock().unwrap();
        state.sort_order = self.sort_order;
        state.filter_changed = true;
    }

    fn request_history_for_selected(&mut self) {
        self.price_history.clear();
        if let Some(lead) = self.selected_lead() {
            let mut state = self.shared.lock().unwrap();
            state.requested_token_id = Some(lead.yes_token_id.clone());
            state.history_updated = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_events() -> Vec<Event> {
        vec![
            Event {
                title: "US Election".to_string(),
                markets: vec![
                    SubMarket {
                        question: "Will candidate A win?".to_string(),
                        yes_price: 65.0,
                        volume_24h: 1_500_000.0,
                        yes_token_id: "tok_1".to_string(),
                    },
                    SubMarket {
                        question: "Will candidate B win?".to_string(),
                        yes_price: 30.0,
                        volume_24h: 800_000.0,
                        yes_token_id: "tok_2".to_string(),
                    },
                ],
                total_volume_24h: 2_300_000.0,
            },
            Event {
                title: "Crypto Prices".to_string(),
                markets: vec![SubMarket {
                    question: "BTC above 100K?".to_string(),
                    yes_price: 42.0,
                    volume_24h: 250_000.0,
                    yes_token_id: "tok_3".to_string(),
                }],
                total_volume_24h: 250_000.0,
            },
            Event {
                title: "World Cup".to_string(),
                markets: vec![SubMarket {
                    question: "Will Spain win?".to_string(),
                    yes_price: 17.0,
                    volume_24h: 500_000.0,
                    yes_token_id: "tok_4".to_string(),
                }],
                total_volume_24h: 500_000.0,
            },
        ]
    }

    fn test_app(events: Vec<Event>) -> App {
        let shared = Arc::new(Mutex::new(FetchState::new(30000)));
        App {
            shared,
            events,
            selected: 0,
            price_history: Vec::new(),
            should_quit: false,
            fast_mode: false,
            refresh_ms: 30000,
            last_error: None,
            last_update: None,
            topic: Topic::All,
            sort_order: SortOrder::Volume24h,
            normal_refresh_ms: 30000,
        }
    }

    #[test]
    fn test_initial_state() {
        let app = test_app(Vec::new());
        assert!(!app.fast_mode);
        assert!(!app.should_quit);
        assert!(app.events.is_empty());
        assert_eq!(app.selected, 0);
        assert!(app.price_history.is_empty());
        assert!(app.last_error.is_none());
    }

    #[test]
    fn test_selected_lead_returns_highest_volume() {
        let app = test_app(test_events());
        let lead = app.selected_lead().unwrap();
        assert_eq!(lead.question, "Will candidate A win?");
    }

    #[test]
    fn test_select_next_wraps() {
        let mut app = test_app(test_events());
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
        let mut app = test_app(test_events());
        app.select_prev();
        assert_eq!(app.selected, 2);
        app.select_prev();
        assert_eq!(app.selected, 1);
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
    fn test_toggle_fast_mode() {
        let mut app = test_app(Vec::new());
        app.toggle_fast_mode();
        assert!(app.fast_mode);
        assert_eq!(app.refresh_ms, FAST_REFRESH_MS);
    }

    #[test]
    fn test_toggle_fast_mode_twice_restores() {
        let mut app = test_app(Vec::new());
        app.toggle_fast_mode();
        app.toggle_fast_mode();
        assert!(!app.fast_mode);
        assert_eq!(app.refresh_ms, 30000);
    }

    #[test]
    fn test_select_clears_price_history() {
        let mut app = test_app(test_events());
        app.price_history = vec![(0.0, 50.0), (1.0, 55.0)];
        app.select_next();
        assert!(app.price_history.is_empty());
    }

    #[test]
    fn test_tick_picks_up_events() {
        let mut app = test_app(Vec::new());
        {
            let mut state = app.shared.lock().unwrap();
            state.events = Some(test_events());
            state.events_updated = true;
        }
        app.tick();
        assert_eq!(app.events.len(), 3);
        assert!(app.last_update.is_some());
    }

    #[test]
    fn test_tick_picks_up_error() {
        let mut app = test_app(Vec::new());
        {
            let mut state = app.shared.lock().unwrap();
            state.error = Some("network timeout".to_string());
        }
        app.tick();
        assert_eq!(app.last_error.as_deref(), Some("network timeout"));
    }

    #[test]
    fn test_tick_picks_up_history() {
        let mut app = test_app(test_events());
        {
            let mut state = app.shared.lock().unwrap();
            state.price_history = Some(vec![(0.0, 50.0), (1.0, 55.0), (2.0, 60.0)]);
            state.history_updated = true;
        }
        app.tick();
        assert_eq!(app.price_history.len(), 3);
    }

    #[test]
    fn test_cycle_topic() {
        let mut app = test_app(test_events());
        assert_eq!(app.topic, Topic::All);
        app.cycle_topic();
        assert_eq!(app.topic, Topic::Politics);
        assert!(app.events.is_empty());
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn test_cycle_topic_prev() {
        let mut app = test_app(Vec::new());
        app.cycle_topic_prev();
        assert_eq!(app.topic, *Topic::ALL.last().unwrap());
    }

    #[test]
    fn test_cycle_sort() {
        let mut app = test_app(test_events());
        assert_eq!(app.sort_order, SortOrder::Volume24h);
        app.cycle_sort();
        assert_eq!(app.sort_order, SortOrder::Volume);
        assert!(app.events.is_empty());
    }

    #[test]
    fn test_cycle_topic_sets_filter_changed() {
        let mut app = test_app(Vec::new());
        app.cycle_topic();
        let state = app.shared.lock().unwrap();
        assert_eq!(state.topic, Topic::Politics);
        assert!(state.filter_changed);
    }

    #[test]
    fn test_cycle_sort_sets_filter_changed() {
        let mut app = test_app(Vec::new());
        app.cycle_sort();
        let state = app.shared.lock().unwrap();
        assert_eq!(state.sort_order, SortOrder::Volume);
        assert!(state.filter_changed);
    }

    #[test]
    fn test_tick_clamps_selected_on_shrink() {
        let mut app = test_app(test_events());
        app.selected = 2;
        {
            let mut state = app.shared.lock().unwrap();
            state.events = Some(vec![test_events()[0].clone()]);
            state.events_updated = true;
        }
        app.tick();
        assert_eq!(app.selected, 0);
    }
}
