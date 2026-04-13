use std::time::Duration;

use anyhow::Result;

use std::collections::HashMap;

use crate::collector::{diskstats, hwinfo};
use crate::collector::hwinfo::DiskHwInfo;
use crate::input::AppAction;
use crate::model::device::DeviceSeries;
use crate::model::process::{ProcessIoTable, ProcessIoTracker};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    AllDevices,
    SingleDevice,
    ProcessTable,
}

const FAST_REFRESH_MS: u64 = 25;
const FAST_SCROLLBACK_SECS: u64 = 3;

pub struct App {
    pub devices: Vec<DeviceSeries>,
    pub selected_device: usize,
    pub view_mode: ViewMode,
    pub should_quit: bool,
    pub show_help: bool,
    pub refresh_rate: Duration,
    pub show_all: bool,
    pub ring_capacity: usize,
    pub process_table: ProcessIoTable,
    pub fast_mode: bool,
    pub disk_hw: HashMap<String, DiskHwInfo>,
    process_tracker: ProcessIoTracker,
    normal_refresh_ms: u64,
    normal_scrollback_secs: u64,
}

impl App {
    pub fn new(refresh_ms: u64, scrollback_secs: u64, show_all: bool) -> Self {
        let ring_capacity = min_capacity(refresh_ms, scrollback_secs);

        Self {
            devices: Vec::new(),
            selected_device: 0,
            view_mode: ViewMode::AllDevices,
            should_quit: false,
            show_help: false,
            refresh_rate: Duration::from_millis(refresh_ms),
            show_all,
            ring_capacity,
            process_table: ProcessIoTable::new(),
            fast_mode: false,
            disk_hw: HashMap::new(),
            process_tracker: ProcessIoTracker::new(),
            normal_refresh_ms: refresh_ms,
            normal_scrollback_secs: scrollback_secs,
        }
    }

    pub fn tick(&mut self) -> Result<()> {
        diskstats::collect(&mut self.devices, self.show_all, self.ring_capacity)?;

        if self.selected_device >= self.devices.len() && !self.devices.is_empty() {
            self.selected_device = self.devices.len() - 1;
        }

        for device in &self.devices {
            self.disk_hw
                .entry(device.name.clone())
                .and_modify(|info| hwinfo::refresh_temp(info, &device.name))
                .or_insert_with(|| {
                    hwinfo::read_disk_hwinfo(&device.name)
                        .unwrap_or(DiskHwInfo {
                            model: String::new(),
                            capacity_gb: 0.0,
                            transport: String::new(),
                            temp_celsius: None,
                        })
                });
        }

        if self.view_mode == ViewMode::ProcessTable {
            let (entries, degraded) = self.process_tracker.collect();
            self.process_table.update(entries, degraded);
        }

        Ok(())
    }

    pub fn handle_action(&mut self, action: AppAction) {
        match action {
            AppAction::Quit => self.should_quit = true,
            AppAction::CycleView => {
                self.view_mode = match self.view_mode {
                    ViewMode::AllDevices => ViewMode::SingleDevice,
                    ViewMode::SingleDevice => ViewMode::ProcessTable,
                    ViewMode::ProcessTable => ViewMode::AllDevices,
                };
            }
            AppAction::ToggleProcessView => {
                self.view_mode = if self.view_mode == ViewMode::ProcessTable {
                    ViewMode::AllDevices
                } else {
                    ViewMode::ProcessTable
                };
            }
            AppAction::NextDevice => {
                if !self.devices.is_empty() {
                    self.selected_device = (self.selected_device + 1) % self.devices.len();
                }
            }
            AppAction::PrevDevice => {
                if !self.devices.is_empty() {
                    self.selected_device = if self.selected_device == 0 {
                        self.devices.len() - 1
                    } else {
                        self.selected_device - 1
                    };
                }
            }
            AppAction::ToggleHelp => {
                self.show_help = !self.show_help;
            }
            AppAction::CycleSortColumn => {
                self.process_table.cycle_sort();
            }
            AppAction::ReverseSortDirection => {
                self.process_table.toggle_sort_direction();
            }
            AppAction::IncreaseRefresh => {
                let current_ms = self.refresh_rate.as_millis() as u64;
                let new_ms = (current_ms / 2).max(100);
                self.refresh_rate = Duration::from_millis(new_ms);
            }
            AppAction::DecreaseRefresh => {
                let current_ms = self.refresh_rate.as_millis() as u64;
                let new_ms = (current_ms * 2).min(5000);
                self.refresh_rate = Duration::from_millis(new_ms);
            }
            AppAction::ToggleFastMode => {
                self.fast_mode = !self.fast_mode;
                if self.fast_mode {
                    self.refresh_rate = Duration::from_millis(FAST_REFRESH_MS);
                    self.ring_capacity = min_capacity(FAST_REFRESH_MS, FAST_SCROLLBACK_SECS);
                } else {
                    self.refresh_rate = Duration::from_millis(self.normal_refresh_ms);
                    self.ring_capacity =
                        min_capacity(self.normal_refresh_ms, self.normal_scrollback_secs);
                }
                self.devices.clear();
            }
            AppAction::None => {}
        }
    }
}

fn min_capacity(refresh_ms: u64, scrollback_secs: u64) -> usize {
    let time_based = (scrollback_secs * 1000 / refresh_ms) as usize;
    let term_width = crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(200);
    time_based.max(term_width)
}
