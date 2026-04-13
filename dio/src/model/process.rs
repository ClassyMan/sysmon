use std::collections::{HashMap, HashSet};
use std::time::Instant;

pub struct ProcessIoEntry {
    pub pid: i32,
    pub comm: String,
    pub read_bytes_per_sec: f64,
    pub write_bytes_per_sec: f64,
}

impl ProcessIoEntry {
    pub fn total_bytes_per_sec(&self) -> f64 {
        self.read_bytes_per_sec + self.write_bytes_per_sec
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortColumn {
    TotalBytes,
    ReadBytes,
    WriteBytes,
    Pid,
}

impl SortColumn {
    pub fn next(self) -> Self {
        match self {
            Self::TotalBytes => Self::ReadBytes,
            Self::ReadBytes => Self::WriteBytes,
            Self::WriteBytes => Self::Pid,
            Self::Pid => Self::TotalBytes,
        }
    }
}

struct PrevIo {
    timestamp: Instant,
    read_bytes: u64,
    write_bytes: u64,
}

/// Tracks previous I/O counters for delta computation, keyed by PID.
pub struct ProcessIoTracker {
    prev: HashMap<i32, PrevIo>,
}

impl ProcessIoTracker {
    pub fn new() -> Self {
        Self {
            prev: HashMap::new(),
        }
    }

    /// Collects per-process I/O rates.
    ///
    /// Reads /proc/<pid>/io for all accessible processes, computes deltas
    /// from the previous snapshot. Processes we can't read (permission denied)
    /// are silently skipped.
    pub fn collect(&mut self) -> (Vec<ProcessIoEntry>, bool) {
        let now = Instant::now();
        let mut entries = Vec::new();
        let mut permission_degraded = false;

        let all_procs = match procfs::process::all_processes() {
            Ok(procs) => procs,
            Err(_) => return (entries, true),
        };

        let mut seen_pids = HashSet::new();

        for proc_result in all_procs {
            let process = match proc_result {
                Ok(proc) => proc,
                Err(_) => continue,
            };

            let pid = process.pid();
            seen_pids.insert(pid);

            let io = match process.io() {
                Ok(io) => io,
                Err(e) => {
                    if let procfs::ProcError::PermissionDenied(_) = e {
                        permission_degraded = true;
                    }
                    continue;
                }
            };

            let comm = process
                .stat()
                .map(|stat| stat.comm.clone())
                .unwrap_or_else(|_| String::from("?"));

            if let Some(prev) = self.prev.get(&pid) {
                let dt = now.duration_since(prev.timestamp).as_secs_f64();
                if dt > 0.0 {
                    let read_delta = io.read_bytes.saturating_sub(prev.read_bytes);
                    let write_delta = io.write_bytes.saturating_sub(prev.write_bytes);

                    let read_bps = read_delta as f64 / dt;
                    let write_bps = write_delta as f64 / dt;

                    if read_bps > 0.0 || write_bps > 0.0 {
                        entries.push(ProcessIoEntry {
                            pid,
                            comm,
                            read_bytes_per_sec: read_bps,
                            write_bytes_per_sec: write_bps,
                        });
                    }
                }
            }

            self.prev.insert(
                pid,
                PrevIo {
                    timestamp: now,
                    read_bytes: io.read_bytes,
                    write_bytes: io.write_bytes,
                },
            );
        }

        self.prev.retain(|pid, _| seen_pids.contains(pid));

        (entries, permission_degraded)
    }
}

/// Sorted, displayable process I/O table.
pub struct ProcessIoTable {
    pub entries: Vec<ProcessIoEntry>,
    pub sort_column: SortColumn,
    pub sort_descending: bool,
    pub permission_degraded: bool,
}

impl ProcessIoTable {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            sort_column: SortColumn::TotalBytes,
            sort_descending: true,
            permission_degraded: false,
        }
    }

    pub fn update(&mut self, mut entries: Vec<ProcessIoEntry>, permission_degraded: bool) {
        self.sort_entries(&mut entries);
        self.entries = entries;
        self.permission_degraded = permission_degraded;
    }

    pub fn cycle_sort(&mut self) {
        self.sort_column = self.sort_column.next();
        let mut entries = std::mem::take(&mut self.entries);
        self.sort_entries(&mut entries);
        self.entries = entries;
    }

    pub fn toggle_sort_direction(&mut self) {
        self.sort_descending = !self.sort_descending;
        self.entries.reverse();
    }

    fn sort_entries(&self, entries: &mut [ProcessIoEntry]) {
        let descending = self.sort_descending;
        entries.sort_by(|entry_a, entry_b| {
            let cmp = match self.sort_column {
                SortColumn::TotalBytes => entry_a
                    .total_bytes_per_sec()
                    .partial_cmp(&entry_b.total_bytes_per_sec()),
                SortColumn::ReadBytes => entry_a
                    .read_bytes_per_sec
                    .partial_cmp(&entry_b.read_bytes_per_sec),
                SortColumn::WriteBytes => entry_a
                    .write_bytes_per_sec
                    .partial_cmp(&entry_b.write_bytes_per_sec),
                SortColumn::Pid => Some(entry_a.pid.cmp(&entry_b.pid)),
            };
            let cmp = cmp.unwrap_or(std::cmp::Ordering::Equal);
            if descending {
                cmp.reverse()
            } else {
                cmp
            }
        });
    }
}
