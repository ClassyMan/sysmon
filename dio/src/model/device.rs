use std::time::Instant;

use sysmon_shared::ring_buffer::RingBuffer;
use sysmon_shared::sticky_max::StickyMax;

/// Raw cumulative counters from a single read of /proc/diskstats for one device.
pub struct DiskStatSnapshot {
    pub timestamp: Instant,
    pub reads_completed: u64,
    pub writes_completed: u64,
    pub sectors_read: u64,
    pub sectors_written: u64,
    pub time_reading_ms: u64,
    pub time_writing_ms: u64,
    pub in_progress: u64,
    pub io_time_ms: u64,
}

/// Computed rates between two consecutive snapshots.
struct DeviceRates {
    read_iops: f64,
    write_iops: f64,
    read_bytes_per_sec: f64,
    write_bytes_per_sec: f64,
    queue_depth: f64,
    read_latency_ms: f64,
    write_latency_ms: f64,
    utilization_pct: f64,
}

impl DeviceRates {
    /// Computes rates from two consecutive snapshots.
    ///
    /// Sector size in /proc/diskstats is always 512 bytes (kernel ABI guarantee).
    fn from_delta(prev: &DiskStatSnapshot, curr: &DiskStatSnapshot) -> Self {
        let dt_secs = curr.timestamp.duration_since(prev.timestamp).as_secs_f64();
        if dt_secs <= 0.0 {
            return Self::zero(curr.in_progress as f64);
        }

        let read_ops_delta = curr.reads_completed.saturating_sub(prev.reads_completed);
        let write_ops_delta = curr.writes_completed.saturating_sub(prev.writes_completed);
        let sectors_read_delta = curr.sectors_read.saturating_sub(prev.sectors_read);
        let sectors_written_delta = curr.sectors_written.saturating_sub(prev.sectors_written);
        let time_reading_delta = curr.time_reading_ms.saturating_sub(prev.time_reading_ms);
        let time_writing_delta = curr.time_writing_ms.saturating_sub(prev.time_writing_ms);
        let io_time_delta = curr.io_time_ms.saturating_sub(prev.io_time_ms);

        let read_latency_ms = if read_ops_delta > 0 {
            time_reading_delta as f64 / read_ops_delta as f64
        } else {
            0.0
        };

        let write_latency_ms = if write_ops_delta > 0 {
            time_writing_delta as f64 / write_ops_delta as f64
        } else {
            0.0
        };

        let utilization_pct = (io_time_delta as f64 / (dt_secs * 1000.0) * 100.0).min(100.0);

        Self {
            read_iops: read_ops_delta as f64 / dt_secs,
            write_iops: write_ops_delta as f64 / dt_secs,
            read_bytes_per_sec: sectors_read_delta as f64 * 512.0 / dt_secs,
            write_bytes_per_sec: sectors_written_delta as f64 * 512.0 / dt_secs,
            queue_depth: curr.in_progress as f64,
            read_latency_ms,
            write_latency_ms,
            utilization_pct,
        }
    }

    fn zero(queue_depth: f64) -> Self {
        Self {
            read_iops: 0.0,
            write_iops: 0.0,
            read_bytes_per_sec: 0.0,
            write_bytes_per_sec: 0.0,
            queue_depth,
            read_latency_ms: 0.0,
            write_latency_ms: 0.0,
            utilization_pct: 0.0,
        }
    }
}

/// All time-series data for a single block device.
pub struct DeviceSeries {
    pub name: String,
    prev_snapshot: Option<DiskStatSnapshot>,
    pub read_iops: RingBuffer,
    pub write_iops: RingBuffer,
    pub read_throughput: RingBuffer,
    pub write_throughput: RingBuffer,
    pub queue_depth: RingBuffer,
    pub read_latency: RingBuffer,
    pub write_latency: RingBuffer,
    pub utilization: RingBuffer,
    pub active: bool,
    pub iops_y: StickyMax,
    pub latency_y: StickyMax,
}

impl DeviceSeries {
    pub fn new(name: String, capacity: usize) -> Self {
        Self {
            name,
            prev_snapshot: None,
            read_iops: RingBuffer::new(capacity),
            write_iops: RingBuffer::new(capacity),
            read_throughput: RingBuffer::new(capacity),
            write_throughput: RingBuffer::new(capacity),
            queue_depth: RingBuffer::new(capacity),
            read_latency: RingBuffer::new(capacity),
            write_latency: RingBuffer::new(capacity),
            utilization: RingBuffer::new(capacity),
            active: true,
            iops_y: StickyMax::new(),
            latency_y: StickyMax::new(),
        }
    }

    /// Pushes a new snapshot, computing rates from the previous one.
    /// The first snapshot establishes a baseline; no rates are pushed.
    pub fn push_snapshot(&mut self, snapshot: DiskStatSnapshot) {
        if let Some(prev) = &self.prev_snapshot {
            let rates = DeviceRates::from_delta(prev, &snapshot);
            self.read_iops.push(rates.read_iops);
            self.write_iops.push(rates.write_iops);
            self.read_throughput.push(rates.read_bytes_per_sec);
            self.write_throughput.push(rates.write_bytes_per_sec);
            self.queue_depth.push(rates.queue_depth);
            self.read_latency.push(rates.read_latency_ms);
            self.write_latency.push(rates.write_latency_ms);
            self.utilization.push(rates.utilization_pct);

            self.iops_y.update(
                self.read_iops.max().max(self.write_iops.max()),
            );
            self.latency_y.update(
                self.read_latency.max().max(self.write_latency.max()),
            );
        }
        self.prev_snapshot = Some(snapshot);
        self.active = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn make_snapshot(
        timestamp: Instant,
        reads: u64,
        writes: u64,
        sectors_r: u64,
        sectors_w: u64,
        time_r_ms: u64,
        time_w_ms: u64,
        in_progress: u64,
        io_time_ms: u64,
    ) -> DiskStatSnapshot {
        DiskStatSnapshot {
            timestamp,
            reads_completed: reads,
            writes_completed: writes,
            sectors_read: sectors_r,
            sectors_written: sectors_w,
            time_reading_ms: time_r_ms,
            time_writing_ms: time_w_ms,
            in_progress,
            io_time_ms,
        }
    }

    #[test]
    fn test_rates_basic() {
        let now = Instant::now();
        let prev = make_snapshot(now, 100, 50, 2000, 1000, 500, 250, 0, 400);
        let curr = make_snapshot(
            now + Duration::from_secs(1),
            200,
            100,
            4000,
            2000,
            600,
            350,
            2,
            800,
        );

        let rates = DeviceRates::from_delta(&prev, &curr);

        assert!((rates.read_iops - 100.0).abs() < 0.01);
        assert!((rates.write_iops - 50.0).abs() < 0.01);
        assert!((rates.read_bytes_per_sec - 2000.0 * 512.0).abs() < 1.0);
        assert!((rates.write_bytes_per_sec - 1000.0 * 512.0).abs() < 1.0);
        assert_eq!(rates.queue_depth, 2.0);
        assert!((rates.read_latency_ms - 1.0).abs() < 0.01);
        assert!((rates.write_latency_ms - 2.0).abs() < 0.01);
        assert!((rates.utilization_pct - 40.0).abs() < 0.01);
    }

    #[test]
    fn test_rates_zero_ops() {
        let now = Instant::now();
        let prev = make_snapshot(now, 100, 50, 2000, 1000, 500, 250, 0, 400);
        let curr = make_snapshot(
            now + Duration::from_secs(1),
            100,
            50,
            2000,
            1000,
            500,
            250,
            0,
            400,
        );

        let rates = DeviceRates::from_delta(&prev, &curr);

        assert_eq!(rates.read_iops, 0.0);
        assert_eq!(rates.write_iops, 0.0);
        assert_eq!(rates.read_latency_ms, 0.0);
        assert_eq!(rates.write_latency_ms, 0.0);
    }

    #[test]
    fn test_utilization_capped_at_100() {
        let now = Instant::now();
        let prev = make_snapshot(now, 0, 0, 0, 0, 0, 0, 0, 0);
        let curr = make_snapshot(
            now + Duration::from_secs(1),
            100,
            0,
            0,
            0,
            0,
            0,
            0,
            1500,
        );

        let rates = DeviceRates::from_delta(&prev, &curr);
        assert!(rates.utilization_pct <= 100.0);
    }
}
