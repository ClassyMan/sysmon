use std::time::Duration;

use anyhow::Result;

use std::collections::HashMap;

use crate::collector::{diskstats, hwinfo};
use crate::collector::hwinfo::DiskHwInfo;
use crate::input::AppAction;
use crate::model::device::DeviceSeries;
use crate::model::process::{ProcessIoTable, ProcessIoTracker};
use crate::ui::animation::AnimatedGif;
use ratatui_image::picker::Picker;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    pub animation: Option<AnimatedGif>,
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
            animation: None,
            process_tracker: ProcessIoTracker::new(),
            normal_refresh_ms: refresh_ms,
            normal_scrollback_secs: scrollback_secs,
        }
    }

    pub fn load_drive_animation(&mut self, picker: &mut Picker) {
        self.animation = AnimatedGif::load_drive(picker);
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

fn compute_capacity(refresh_ms: u64, scrollback_secs: u64, term_width: usize) -> usize {
    let time_based = (scrollback_secs * 1000 / refresh_ms) as usize;
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
                devices: Vec::new(),
                selected_device: 0,
                view_mode: ViewMode::AllDevices,
                should_quit: false,
                show_help: false,
                refresh_rate: Duration::from_millis(500),
                show_all: false,
                ring_capacity: capacity,
                process_table: ProcessIoTable::new(),
                fast_mode: false,
                disk_hw: HashMap::new(),
                animation: None,
                process_tracker: ProcessIoTracker::new(),
                normal_refresh_ms: 500,
                normal_scrollback_secs: 60,
            }
        }
    }

    #[test]
    fn test_with_capacity_initial_state() {
        let app = App::with_capacity(100);

        assert_eq!(app.view_mode, ViewMode::AllDevices);
        assert_eq!(app.selected_device, 0);
        assert!(!app.should_quit);
        assert!(!app.fast_mode);
        assert!(!app.show_help);
    }

    #[test]
    fn test_handle_action_quit() {
        let mut app = App::with_capacity(100);
        app.handle_action(AppAction::Quit);

        assert!(app.should_quit);
    }

    #[test]
    fn test_handle_action_cycle_view_full_cycle() {
        let mut app = App::with_capacity(100);

        assert_eq!(app.view_mode, ViewMode::AllDevices);

        app.handle_action(AppAction::CycleView);
        assert_eq!(app.view_mode, ViewMode::SingleDevice);

        app.handle_action(AppAction::CycleView);
        assert_eq!(app.view_mode, ViewMode::ProcessTable);

        app.handle_action(AppAction::CycleView);
        assert_eq!(app.view_mode, ViewMode::AllDevices);
    }

    #[test]
    fn test_handle_action_toggle_process_view_on() {
        let mut app = App::with_capacity(100);
        app.handle_action(AppAction::ToggleProcessView);

        assert_eq!(app.view_mode, ViewMode::ProcessTable);
    }

    #[test]
    fn test_handle_action_toggle_process_view_off() {
        let mut app = App::with_capacity(100);
        app.view_mode = ViewMode::ProcessTable;

        app.handle_action(AppAction::ToggleProcessView);

        assert_eq!(app.view_mode, ViewMode::AllDevices);
    }

    #[test]
    fn test_handle_action_next_device() {
        let mut app = App::with_capacity(100);
        app.devices.push(DeviceSeries::new("sda".to_string(), 10));
        app.devices.push(DeviceSeries::new("sdb".to_string(), 10));
        app.devices.push(DeviceSeries::new("sdc".to_string(), 10));

        assert_eq!(app.selected_device, 0);

        app.handle_action(AppAction::NextDevice);
        assert_eq!(app.selected_device, 1);

        app.handle_action(AppAction::NextDevice);
        assert_eq!(app.selected_device, 2);

        app.handle_action(AppAction::NextDevice);
        assert_eq!(app.selected_device, 0);
    }

    #[test]
    fn test_handle_action_prev_device() {
        let mut app = App::with_capacity(100);
        app.devices.push(DeviceSeries::new("sda".to_string(), 10));
        app.devices.push(DeviceSeries::new("sdb".to_string(), 10));
        app.devices.push(DeviceSeries::new("sdc".to_string(), 10));

        assert_eq!(app.selected_device, 0);

        app.handle_action(AppAction::PrevDevice);
        assert_eq!(app.selected_device, 2);

        app.handle_action(AppAction::PrevDevice);
        assert_eq!(app.selected_device, 1);

        app.handle_action(AppAction::PrevDevice);
        assert_eq!(app.selected_device, 0);
    }

    #[test]
    fn test_handle_action_next_device_empty() {
        let mut app = App::with_capacity(100);

        app.handle_action(AppAction::NextDevice);

        assert_eq!(app.selected_device, 0);
    }

    #[test]
    fn test_handle_action_toggle_help() {
        let mut app = App::with_capacity(100);

        assert!(!app.show_help);

        app.handle_action(AppAction::ToggleHelp);
        assert!(app.show_help);

        app.handle_action(AppAction::ToggleHelp);
        assert!(!app.show_help);
    }

    #[test]
    fn test_handle_action_increase_refresh() {
        let mut app = App::with_capacity(100);
        app.handle_action(AppAction::IncreaseRefresh);

        assert_eq!(app.refresh_rate, Duration::from_millis(250));
    }

    #[test]
    fn test_handle_action_increase_refresh_min() {
        let mut app = App::with_capacity(100);
        app.refresh_rate = Duration::from_millis(100);

        app.handle_action(AppAction::IncreaseRefresh);

        assert_eq!(app.refresh_rate, Duration::from_millis(100));
    }

    #[test]
    fn test_handle_action_decrease_refresh() {
        let mut app = App::with_capacity(100);
        app.handle_action(AppAction::DecreaseRefresh);

        assert_eq!(app.refresh_rate, Duration::from_millis(1000));
    }

    #[test]
    fn test_handle_action_decrease_refresh_max() {
        let mut app = App::with_capacity(100);
        app.refresh_rate = Duration::from_millis(5000);

        app.handle_action(AppAction::DecreaseRefresh);

        assert_eq!(app.refresh_rate, Duration::from_millis(5000));
    }

    #[test]
    fn test_handle_action_toggle_fast_mode() {
        let mut app = App::with_capacity(100);
        app.devices.push(DeviceSeries::new("sda".to_string(), 10));

        app.handle_action(AppAction::ToggleFastMode);

        assert!(app.fast_mode);
        assert_eq!(app.refresh_rate, Duration::from_millis(FAST_REFRESH_MS));
        assert!(app.devices.is_empty());
    }

    #[test]
    fn test_handle_action_none() {
        let mut app = App::with_capacity(100);
        let original_view_mode = app.view_mode;
        let original_selected = app.selected_device;
        let original_should_quit = app.should_quit;
        let original_fast_mode = app.fast_mode;
        let original_show_help = app.show_help;

        app.handle_action(AppAction::None);

        assert_eq!(app.view_mode, original_view_mode);
        assert_eq!(app.selected_device, original_selected);
        assert_eq!(app.should_quit, original_should_quit);
        assert_eq!(app.fast_mode, original_fast_mode);
        assert_eq!(app.show_help, original_show_help);
    }
}
