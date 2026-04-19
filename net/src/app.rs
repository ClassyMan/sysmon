use std::cell::Cell;

use anyhow::Result;

use crate::collector::{self, InterfaceInfo, NetRates, NetSnapshot};
use crate::rain::RainState;
use sysmon_shared::ring_buffer::RingBuffer;
use sysmon_shared::sticky_max::StickyMax;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    Charts,
    Rain,
}

const FAST_REFRESH_MS: u64 = 25;
const FAST_SCROLLBACK_SECS: u64 = 3;
const RAIN_REFRESH_MS: u64 = 50;

pub struct App {
    pub rx_history: RingBuffer,
    pub tx_history: RingBuffer,
    pub latest_rates: Option<NetRates>,
    pub interfaces: Vec<InterfaceInfo>,
    pub selected_interface: usize,
    pub should_quit: bool,
    pub scrollback_secs: u64,
    pub fast_mode: bool,
    pub refresh_ms: u64,
    pub rx_y: StickyMax,
    pub tx_y: StickyMax,
    pub view_mode: ViewMode,
    pub rain: RainState,
    pub rain_panel_size: Cell<Option<(u16, u16)>>,
    normal_refresh_ms: u64,
    normal_scrollback_secs: u64,
    prev_snapshot: Option<NetSnapshot>,
}

impl App {
    pub fn new(refresh_ms: u64, scrollback_secs: u64) -> Self {
        let capacity = min_capacity(refresh_ms, scrollback_secs);
        let interfaces = collector::list_interfaces();
        Self {
            rx_history: RingBuffer::new(capacity),
            tx_history: RingBuffer::new(capacity),
            latest_rates: None,
            interfaces,
            selected_interface: 0,
            should_quit: false,
            scrollback_secs,
            fast_mode: false,
            refresh_ms: RAIN_REFRESH_MS,
            rx_y: StickyMax::new(),
            tx_y: StickyMax::new(),
            view_mode: ViewMode::Rain,
            rain: RainState::new(),
            rain_panel_size: Cell::new(None),
            normal_refresh_ms: refresh_ms,
            normal_scrollback_secs: scrollback_secs,
            prev_snapshot: None,
        }
    }

    pub fn selected_name(&self) -> &str {
        self.interfaces
            .get(self.selected_interface)
            .map(|i| i.name.as_str())
            .unwrap_or("?")
    }

    pub fn selected_info(&self) -> Option<&InterfaceInfo> {
        self.interfaces.get(self.selected_interface)
    }

    pub fn chart_capacity(&self) -> usize {
        self.rx_history.capacity()
    }

    pub fn next_interface(&mut self) {
        if !self.interfaces.is_empty() {
            self.selected_interface = (self.selected_interface + 1) % self.interfaces.len();
            self.reset_data();
        }
    }

    pub fn prev_interface(&mut self) {
        if !self.interfaces.is_empty() {
            self.selected_interface = if self.selected_interface == 0 {
                self.interfaces.len() - 1
            } else {
                self.selected_interface - 1
            };
            self.reset_data();
        }
    }

    pub fn toggle_fast_mode(&mut self) {
        self.fast_mode = !self.fast_mode;

        let (new_refresh, new_scrollback) = if self.fast_mode {
            (FAST_REFRESH_MS, FAST_SCROLLBACK_SECS)
        } else {
            (self.normal_refresh_ms, self.normal_scrollback_secs)
        };

        self.refresh_ms = new_refresh;
        self.scrollback_secs = new_scrollback;
        self.reset_data();

        let capacity = min_capacity(new_refresh, new_scrollback);
        self.rx_history = RingBuffer::new(capacity);
        self.tx_history = RingBuffer::new(capacity);
    }

    pub fn toggle_view(&mut self) {
        self.view_mode = match self.view_mode {
            ViewMode::Charts => {
                if !self.fast_mode {
                    self.refresh_ms = RAIN_REFRESH_MS;
                }
                ViewMode::Rain
            }
            ViewMode::Rain => {
                if !self.fast_mode {
                    self.refresh_ms = self.normal_refresh_ms;
                }
                ViewMode::Charts
            }
        };
    }

    pub fn refresh_rate(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.refresh_ms)
    }

    pub fn tick(&mut self) -> Result<()> {
        let name = self.selected_name().to_string();
        let snapshot = collector::read_net_snapshot(&name)?;

        if let Some(prev) = &self.prev_snapshot {
            let interval_secs = self.refresh_ms as f64 / 1000.0;
            let rates = NetRates::from_deltas(prev, &snapshot, interval_secs);

            self.rx_history.push(rates.rx_bytes_per_sec);
            self.tx_history.push(rates.tx_bytes_per_sec);
            self.rx_y.update(self.rx_history.max());
            self.tx_y.update(self.tx_history.max());

            self.latest_rates = Some(rates);
        }

        self.prev_snapshot = Some(snapshot);

        if self.view_mode == ViewMode::Rain {
            let (width, height) = self.rain_panel_size.get().unwrap_or_else(|| {
                let (w, h) = crossterm::terminal::size().unwrap_or((80, 24));
                (w, h.saturating_sub(8))
            });
            let rx = self.latest_rates.as_ref().map_or(0.0, |r| r.rx_bytes_per_sec);
            let tx = self.latest_rates.as_ref().map_or(0.0, |r| r.tx_bytes_per_sec);
            self.rain.tick(width, height, rx, tx);
        }

        Ok(())
    }

    fn reset_data(&mut self) {
        let capacity = min_capacity(self.refresh_ms, self.scrollback_secs);
        self.rx_history = RingBuffer::new(capacity);
        self.tx_history = RingBuffer::new(capacity);
        self.rx_y.reset();
        self.tx_y.reset();
        self.prev_snapshot = None;
        self.latest_rates = None;
        self.rain = RainState::new();
    }
}

fn compute_capacity(refresh_ms: u64, scrollback_secs: u64, term_width: usize) -> usize {
    let time_based = ((scrollback_secs * 1000) / refresh_ms) as usize;
    time_based.max(term_width)
}

fn min_capacity(refresh_ms: u64, scrollback_secs: u64) -> usize {
    let term_width = crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(200);
    compute_capacity(refresh_ms, scrollback_secs, term_width)
}

#[cfg(test)]
mod tests {
    use super::*;

    impl App {
        pub fn with_capacity(capacity: usize) -> Self {
            Self {
                rx_history: RingBuffer::new(capacity),
                tx_history: RingBuffer::new(capacity),
                latest_rates: None,
                interfaces: vec![
                    InterfaceInfo {
                        name: "eth0".to_string(),
                        ip: "192.168.1.1".to_string(),
                        speed_mbps: Some(1000),
                        operstate: "up".to_string(),
                    },
                    InterfaceInfo {
                        name: "wlan0".to_string(),
                        ip: "192.168.1.2".to_string(),
                        speed_mbps: None,
                        operstate: "up".to_string(),
                    },
                ],
                selected_interface: 0,
                should_quit: false,
                scrollback_secs: 60,
                fast_mode: false,
                refresh_ms: 500,
                rx_y: StickyMax::new(),
                tx_y: StickyMax::new(),
                view_mode: ViewMode::Charts,
                rain: RainState::new(),
                rain_panel_size: Cell::new(None),
                normal_refresh_ms: 500,
                normal_scrollback_secs: 60,
                prev_snapshot: None,
            }
        }
    }

    #[test]
    fn test_initial_state() {
        let app = App::with_capacity(100);
        assert_eq!(app.selected_name(), "eth0");
        assert!(!app.fast_mode);
        assert!(!app.should_quit);
    }

    #[test]
    fn test_next_interface_wraps() {
        let mut app = App::with_capacity(100);
        assert_eq!(app.selected_interface, 0);
        app.next_interface();
        assert_eq!(app.selected_interface, 1);
        app.next_interface();
        assert_eq!(app.selected_interface, 0);
    }

    #[test]
    fn test_prev_interface_wraps() {
        let mut app = App::with_capacity(100);
        app.prev_interface();
        assert_eq!(app.selected_interface, 1);
    }

    #[test]
    fn test_toggle_fast_mode() {
        let mut app = App::with_capacity(100);
        app.toggle_fast_mode();
        assert!(app.fast_mode);
        assert_eq!(app.refresh_ms, FAST_REFRESH_MS);
    }

    #[test]
    fn test_compute_capacity_takes_larger() {
        assert_eq!(compute_capacity(500, 60, 80), 120);
        assert_eq!(compute_capacity(25, 3, 200), 200);
    }

    #[test]
    fn test_selected_info_returns_first() {
        let app = App::with_capacity(100);
        let info = app.selected_info().unwrap();
        assert_eq!(info.name, "eth0");
        assert_eq!(info.speed_mbps, Some(1000));
    }

    #[test]
    fn test_toggle_view_charts_to_rain() {
        let mut app = App::with_capacity(100);
        assert_eq!(app.view_mode, ViewMode::Charts);
        app.toggle_view();
        assert_eq!(app.view_mode, ViewMode::Rain);
        assert_eq!(app.refresh_ms, RAIN_REFRESH_MS);
    }

    #[test]
    fn test_toggle_view_rain_to_charts() {
        let mut app = App::with_capacity(100);
        app.toggle_view();
        app.toggle_view();
        assert_eq!(app.view_mode, ViewMode::Charts);
        assert_eq!(app.refresh_ms, 500);
    }

    #[test]
    fn test_toggle_view_in_fast_mode_keeps_fast_refresh() {
        let mut app = App::with_capacity(100);
        app.toggle_fast_mode();
        let fast_refresh = app.refresh_ms;
        app.toggle_view();
        assert_eq!(app.refresh_ms, fast_refresh);
    }

    #[test]
    fn test_next_interface_resets_data() {
        let mut app = App::with_capacity(100);
        app.rx_history.push(42.0);
        app.rx_y.update(42.0);
        app.next_interface();
        assert!(app.rx_history.is_empty());
        assert_eq!(app.rx_y.current(), 0.0);
        assert!(app.prev_snapshot.is_none());
    }

    #[test]
    fn test_chart_capacity() {
        let app = App::with_capacity(100);
        assert_eq!(app.chart_capacity(), 100);
    }

    #[test]
    fn test_toggle_fast_mode_twice_restores() {
        let mut app = App::with_capacity(100);
        app.toggle_fast_mode();
        app.toggle_fast_mode();
        assert!(!app.fast_mode);
        assert_eq!(app.refresh_ms, 500);
        assert_eq!(app.scrollback_secs, 60);
    }
}
