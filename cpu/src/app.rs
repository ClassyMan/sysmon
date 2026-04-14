use anyhow::Result;

use crate::collector::{self, CpuInfo, CpuSnapshot};
use sysmon_shared::ring_buffer::RingBuffer;

const FAST_REFRESH_MS: u64 = 25;
const FAST_SCROLLBACK_SECS: u64 = 3;

pub struct App {
    pub total_history: RingBuffer,
    pub core_histories: Vec<RingBuffer>,
    pub core_usages: Vec<f64>,
    pub total_usage: f64,
    pub temp_celsius: Option<f64>,
    pub load_avg: (f64, f64, f64),
    pub cpu_info: CpuInfo,
    pub should_quit: bool,
    pub scrollback_secs: u64,
    pub fast_mode: bool,
    pub refresh_ms: u64,
    normal_refresh_ms: u64,
    normal_scrollback_secs: u64,
    prev_snapshot: Option<CpuSnapshot>,
}

impl App {
    pub fn new(refresh_ms: u64, scrollback_secs: u64) -> Self {
        let capacity = min_capacity(refresh_ms, scrollback_secs);
        let cpu_info = collector::read_cpu_info();
        let core_count = cpu_info.threads;

        Self {
            total_history: RingBuffer::new(capacity),
            core_histories: (0..core_count).map(|_| RingBuffer::new(capacity)).collect(),
            core_usages: vec![0.0; core_count],
            total_usage: 0.0,
            temp_celsius: None,
            load_avg: (0.0, 0.0, 0.0),
            cpu_info,
            should_quit: false,
            scrollback_secs,
            fast_mode: false,
            refresh_ms,
            normal_refresh_ms: refresh_ms,
            normal_scrollback_secs: scrollback_secs,
            prev_snapshot: None,
        }
    }

    pub fn chart_capacity(&self) -> usize {
        self.total_history.capacity()
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
        self.total_history = RingBuffer::new(capacity);
        self.core_histories = (0..self.cpu_info.threads)
            .map(|_| RingBuffer::new(capacity))
            .collect();
        self.prev_snapshot = None;
    }

    pub fn refresh_rate(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.refresh_ms)
    }

    pub fn tick(&mut self) -> Result<()> {
        let snapshot = collector::read_cpu_snapshot()?;

        if let Some(prev) = &self.prev_snapshot {
            self.total_usage = collector::usage_pct(&prev.total, &snapshot.total);
            self.total_history.push(self.total_usage);

            for (idx, (prev_core, curr_core)) in prev.per_core.iter()
                .zip(snapshot.per_core.iter())
                .enumerate()
            {
                let usage = collector::usage_pct(prev_core, curr_core);
                if idx < self.core_usages.len() {
                    self.core_usages[idx] = usage;
                }
                if idx < self.core_histories.len() {
                    self.core_histories[idx].push(usage);
                }
            }
        }

        self.temp_celsius = collector::read_cpu_temp();
        self.load_avg = collector::read_load_avg();
        self.prev_snapshot = Some(snapshot);
        Ok(())
    }
}

fn min_capacity(refresh_ms: u64, scrollback_secs: u64) -> usize {
    let time_based = ((scrollback_secs * 1000) / refresh_ms) as usize;
    let term_width = crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(200);
    time_based.max(term_width)
}
