use std::time::Instant;

use anyhow::Result;

use crate::collector::device_filter;
use crate::model::device::{DeviceSeries, DiskStatSnapshot};

/// Reads /proc/diskstats and updates all tracked device series.
///
/// New devices are added to the list. Devices that disappear are
/// marked inactive (their charts stop updating but remain visible).
pub fn collect(
    devices: &mut Vec<DeviceSeries>,
    show_all: bool,
    ring_capacity: usize,
) -> Result<()> {
    let now = Instant::now();
    let diskstats = procfs::diskstats()?;

    // Mark all devices inactive; we'll re-activate the ones we see.
    for device in devices.iter_mut() {
        device.active = false;
    }

    for stat in &diskstats {
        let name = &stat.name;

        if !device_filter::should_track(name, stat.reads, stat.writes, show_all) {
            continue;
        }

        // Only show whole disks by default (skip partitions like sda1, nvme0n1p1)
        // unless show_all is set.
        if !show_all && !device_filter::is_whole_disk(name) {
            continue;
        }

        let snapshot = DiskStatSnapshot {
            timestamp: now,
            reads_completed: stat.reads,
            writes_completed: stat.writes,
            sectors_read: stat.sectors_read,
            sectors_written: stat.sectors_written,
            time_reading_ms: stat.time_reading,
            time_writing_ms: stat.time_writing,
            in_progress: stat.in_progress,
            io_time_ms: stat.time_in_progress,
        };

        match devices.iter_mut().find(|dev| dev.name == *name) {
            Some(series) => {
                series.push_snapshot(snapshot);
            }
            None => {
                let mut series = DeviceSeries::new(name.clone(), ring_capacity);
                series.push_snapshot(snapshot);
                devices.push(series);
            }
        }
    }

    Ok(())
}
