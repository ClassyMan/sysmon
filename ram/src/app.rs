use anyhow::Result;

use crate::collector::{
    self, HardwareInfo, MemInfo, PsiSnapshot, VmRates, VmStatSnapshot,
};
use sysmon_shared::ring_buffer::RingBuffer;
use sysmon_shared::sticky_max::StickyMax;

const FAST_REFRESH_MS: u64 = 25;
const FAST_SCROLLBACK_SECS: u64 = 3;

pub struct App {
    pub alloc_history: RingBuffer,
    pub free_history: RingBuffer,
    pub swapin_history: RingBuffer,
    pub swapout_history: RingBuffer,
    pub fault_history: RingBuffer,
    pub major_fault_history: RingBuffer,
    pub psi_some_history: RingBuffer,
    pub psi_full_history: RingBuffer,

    pub latest_info: Option<MemInfo>,
    pub latest_rates: Option<VmRates>,
    pub latest_psi: Option<PsiSnapshot>,
    pub hardware: HardwareInfo,
    pub should_quit: bool,
    pub scrollback_secs: u64,
    pub fast_mode: bool,
    pub refresh_ms: u64,

    pub throughput_y: StickyMax,
    pub swap_io_y: StickyMax,
    pub faults_y: StickyMax,
    pub psi_y: StickyMax,

    normal_refresh_ms: u64,
    normal_scrollback_secs: u64,
    prev_vmstat: Option<VmStatSnapshot>,
}

impl App {
    pub fn new(refresh_ms: u64, scrollback_secs: u64) -> Self {
        let capacity = min_capacity(refresh_ms, scrollback_secs);
        let hardware = collector::read_hardware_info();
        Self {
            alloc_history: RingBuffer::new(capacity),
            free_history: RingBuffer::new(capacity),
            swapin_history: RingBuffer::new(capacity),
            swapout_history: RingBuffer::new(capacity),
            fault_history: RingBuffer::new(capacity),
            major_fault_history: RingBuffer::new(capacity),
            psi_some_history: RingBuffer::new(capacity),
            psi_full_history: RingBuffer::new(capacity),
            latest_info: None,
            latest_rates: None,
            latest_psi: None,
            hardware,
            should_quit: false,
            scrollback_secs,
            fast_mode: false,
            refresh_ms,
            throughput_y: StickyMax::new(),
            swap_io_y: StickyMax::new(),
            faults_y: StickyMax::new(),
            psi_y: StickyMax::new(),
            normal_refresh_ms: refresh_ms,
            normal_scrollback_secs: scrollback_secs,
            prev_vmstat: None,
        }
    }

    pub fn chart_capacity(&self) -> usize {
        self.alloc_history.capacity()
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
        self.alloc_history = RingBuffer::new(capacity);
        self.free_history = RingBuffer::new(capacity);
        self.swapin_history = RingBuffer::new(capacity);
        self.swapout_history = RingBuffer::new(capacity);
        self.fault_history = RingBuffer::new(capacity);
        self.major_fault_history = RingBuffer::new(capacity);
        self.psi_some_history = RingBuffer::new(capacity);
        self.psi_full_history = RingBuffer::new(capacity);

        self.throughput_y.reset();
        self.swap_io_y.reset();
        self.faults_y.reset();
        self.psi_y.reset();

        self.prev_vmstat = None;
        self.latest_rates = None;
    }

    pub fn refresh_rate(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.refresh_ms)
    }

    pub fn tick(&mut self) -> Result<()> {
        let info = collector::read_meminfo()?;
        let vmstat = collector::read_vmstat()?;
        let psi = collector::read_psi().ok();

        if let Some(prev) = &self.prev_vmstat {
            let interval_secs = self.refresh_ms as f64 / 1000.0;
            let rates = VmRates::from_deltas(prev, &vmstat, interval_secs);

            self.alloc_history.push(rates.alloc_mb_per_sec);
            self.free_history.push(rates.free_mb_per_sec);
            self.swapin_history.push(rates.swapin_mb_per_sec);
            self.swapout_history.push(rates.swapout_mb_per_sec);
            self.fault_history.push(rates.fault_per_sec);
            self.major_fault_history.push(rates.major_fault_per_sec);

            self.throughput_y.update(
                self.alloc_history.max().max(self.free_history.max()),
            );
            self.swap_io_y.update(
                self.swapin_history.max().max(self.swapout_history.max()),
            );
            self.faults_y.update(
                self.fault_history.max().max(self.major_fault_history.max()),
            );

            self.latest_rates = Some(rates);
        }

        if let Some(psi_snap) = &psi {
            self.psi_some_history.push(psi_snap.some_avg10);
            self.psi_full_history.push(psi_snap.full_avg10);
            self.psi_y.update(
                self.psi_some_history.max().max(self.psi_full_history.max()),
            );
        }

        self.prev_vmstat = Some(vmstat);
        self.latest_psi = psi;
        self.latest_info = Some(info);
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
                alloc_history: RingBuffer::new(capacity),
                free_history: RingBuffer::new(capacity),
                swapin_history: RingBuffer::new(capacity),
                swapout_history: RingBuffer::new(capacity),
                fault_history: RingBuffer::new(capacity),
                major_fault_history: RingBuffer::new(capacity),
                psi_some_history: RingBuffer::new(capacity),
                psi_full_history: RingBuffer::new(capacity),
                latest_info: None,
                latest_rates: None,
                latest_psi: None,
                hardware: HardwareInfo { summary: "test".to_string() },
                should_quit: false,
                scrollback_secs: 60,
                fast_mode: false,
                refresh_ms: 500,
                throughput_y: StickyMax::new(),
                swap_io_y: StickyMax::new(),
                faults_y: StickyMax::new(),
                psi_y: StickyMax::new(),
                normal_refresh_ms: 500,
                normal_scrollback_secs: 60,
                prev_vmstat: None,
            }
        }
    }

    #[test]
    fn test_with_capacity_initial_state() {
        let app = App::with_capacity(100);

        assert!(!app.fast_mode);
        assert!(!app.should_quit);
        assert!(app.alloc_history.is_empty());
        assert!(app.free_history.is_empty());
        assert!(app.swapin_history.is_empty());
        assert!(app.swapout_history.is_empty());
        assert!(app.fault_history.is_empty());
        assert!(app.major_fault_history.is_empty());
        assert!(app.psi_some_history.is_empty());
        assert!(app.psi_full_history.is_empty());
        assert!(app.latest_info.is_none());
        assert!(app.latest_rates.is_none());
        assert!(app.latest_psi.is_none());
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
        app.alloc_history.push(42.0);
        app.free_history.push(42.0);

        app.toggle_fast_mode();

        assert!(app.alloc_history.is_empty());
        assert!(app.free_history.is_empty());
        assert_ne!(app.alloc_history.capacity(), 100);
        assert!(app.prev_vmstat.is_none());
    }

    #[test]
    fn test_toggle_fast_mode_resets_sticky_maxes() {
        let mut app = App::with_capacity(100);
        app.throughput_y.update(999.0);
        app.swap_io_y.update(999.0);
        app.faults_y.update(999.0);
        app.psi_y.update(999.0);

        app.toggle_fast_mode();

        assert_eq!(app.throughput_y.current(), 0.0);
        assert_eq!(app.swap_io_y.current(), 0.0);
        assert_eq!(app.faults_y.current(), 0.0);
        assert_eq!(app.psi_y.current(), 0.0);
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
        let result = compute_capacity(500, 60, 80);
        assert_eq!(result, 120);
    }

    #[test]
    fn test_compute_capacity_term_width_wins() {
        let result = compute_capacity(25, 3, 200);
        assert_eq!(result, 200);
    }
}
