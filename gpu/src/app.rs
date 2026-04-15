use anyhow::Result;

use crate::collector::{self, GpuProcess, GpuSnapshot};
use sysmon_shared::ring_buffer::RingBuffer;
use sysmon_shared::sticky_max::StickyMax;

const FAST_REFRESH_MS: u64 = 25;
const FAST_SCROLLBACK_SECS: u64 = 3;

pub struct App {
    pub gpu_util_history: RingBuffer,
    pub mem_util_history: RingBuffer,
    pub vram_pct_history: RingBuffer,
    pub temp_history: RingBuffer,
    pub power_history: RingBuffer,

    pub latest: Option<GpuSnapshot>,
    pub processes: Vec<GpuProcess>,
    pub should_quit: bool,
    pub scrollback_secs: u64,
    pub fast_mode: bool,
    pub refresh_ms: u64,
    pub gpu_util_y: StickyMax,
    pub power_y: StickyMax,
    normal_refresh_ms: u64,
    normal_scrollback_secs: u64,
}

impl App {
    pub fn new(refresh_ms: u64, scrollback_secs: u64) -> Self {
        let capacity = min_capacity(refresh_ms, scrollback_secs);
        Self {
            gpu_util_history: RingBuffer::new(capacity),
            mem_util_history: RingBuffer::new(capacity),
            vram_pct_history: RingBuffer::new(capacity),
            temp_history: RingBuffer::new(capacity),
            power_history: RingBuffer::new(capacity),
            latest: None,
            processes: Vec::new(),
            should_quit: false,
            scrollback_secs,
            fast_mode: false,
            refresh_ms,
            gpu_util_y: StickyMax::new(),
            power_y: StickyMax::new(),
            normal_refresh_ms: refresh_ms,
            normal_scrollback_secs: scrollback_secs,
        }
    }

    pub fn chart_capacity(&self) -> usize {
        self.gpu_util_history.capacity()
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

        let capacity = min_capacity(new_refresh, new_scrollback);
        self.gpu_util_history = RingBuffer::new(capacity);
        self.mem_util_history = RingBuffer::new(capacity);
        self.vram_pct_history = RingBuffer::new(capacity);
        self.temp_history = RingBuffer::new(capacity);
        self.power_history = RingBuffer::new(capacity);
        self.gpu_util_y.reset();
        self.power_y.reset();
    }

    pub fn refresh_rate(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.refresh_ms)
    }

    pub fn tick(&mut self) -> Result<()> {
        let snap = collector::read_gpu_snapshot()?;

        self.gpu_util_history.push(snap.gpu_util_pct);
        self.mem_util_history.push(snap.mem_util_pct);
        self.vram_pct_history.push(snap.vram_pct());
        self.temp_history.push(snap.temp_celsius);
        self.power_history.push(snap.power_watts);

        self.gpu_util_y.update(
            self.gpu_util_history.max().max(self.mem_util_history.max()),
        );
        self.power_y.update(snap.power_limit_watts);

        self.processes = collector::read_gpu_processes();
        self.latest = Some(snap);
        Ok(())
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
    use std::time::Duration;

    impl App {
        pub fn with_capacity(capacity: usize) -> Self {
            Self {
                gpu_util_history: RingBuffer::new(capacity),
                mem_util_history: RingBuffer::new(capacity),
                vram_pct_history: RingBuffer::new(capacity),
                temp_history: RingBuffer::new(capacity),
                power_history: RingBuffer::new(capacity),
                latest: None,
                processes: Vec::new(),
                should_quit: false,
                scrollback_secs: 60,
                fast_mode: false,
                refresh_ms: 500,
                gpu_util_y: StickyMax::new(),
                power_y: StickyMax::new(),
                normal_refresh_ms: 500,
                normal_scrollback_secs: 60,
            }
        }
    }

    #[test]
    fn test_with_capacity_initial_state() {
        let app = App::with_capacity(100);
        assert!(!app.fast_mode);
        assert!(!app.should_quit);
        assert!(app.gpu_util_history.is_empty());
        assert!(app.mem_util_history.is_empty());
        assert!(app.vram_pct_history.is_empty());
        assert!(app.temp_history.is_empty());
        assert!(app.power_history.is_empty());
        assert!(app.latest.is_none());
        assert!(app.processes.is_empty());
    }

    #[test]
    fn test_chart_capacity_matches_constructor() {
        let app = App::with_capacity(100);
        assert_eq!(app.chart_capacity(), 100);
    }

    #[test]
    fn test_toggle_fast_mode_activates() {
        let mut app = App::with_capacity(100);
        app.toggle_fast_mode();
        assert!(app.fast_mode);
        assert_eq!(app.refresh_ms, FAST_REFRESH_MS);
        assert_eq!(app.scrollback_secs, FAST_SCROLLBACK_SECS);
    }

    #[test]
    fn test_toggle_fast_mode_clears_histories() {
        let mut app = App::with_capacity(100);
        app.gpu_util_history.push(42.0);
        app.temp_history.push(65.0);

        app.toggle_fast_mode();

        assert!(app.gpu_util_history.is_empty());
        assert!(app.temp_history.is_empty());
        assert_ne!(app.gpu_util_history.capacity(), 100);
    }

    #[test]
    fn test_toggle_fast_mode_resets_sticky_maxes() {
        let mut app = App::with_capacity(100);
        app.gpu_util_y.update(999.0);
        app.power_y.update(999.0);

        app.toggle_fast_mode();

        assert_eq!(app.gpu_util_y.current(), 0.0);
        assert_eq!(app.power_y.current(), 0.0);
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

    #[test]
    fn test_refresh_rate_returns_duration() {
        let app = App::with_capacity(100);
        assert_eq!(app.refresh_rate(), Duration::from_millis(500));
    }

    #[test]
    fn test_compute_capacity_time_based_wins() {
        assert_eq!(compute_capacity(500, 60, 80), 120);
    }

    #[test]
    fn test_compute_capacity_term_width_wins() {
        assert_eq!(compute_capacity(25, 3, 200), 200);
    }
}
