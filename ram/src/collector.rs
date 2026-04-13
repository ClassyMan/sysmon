use std::fs;
use std::process::Command;
use anyhow::Result;

/// Snapshot of memory usage parsed from /proc/meminfo.
#[derive(Debug, Clone)]
pub struct MemInfo {
    pub ram_total_kb: u64,
    pub ram_used_kb: u64,
    pub swap_total_kb: u64,
    pub swap_used_kb: u64,
    pub dirty_kb: u64,
    pub writeback_kb: u64,
}

impl MemInfo {
    pub fn ram_pct(&self) -> f64 {
        pct(self.ram_used_kb, self.ram_total_kb)
    }

    pub fn swap_pct(&self) -> f64 {
        pct(self.swap_used_kb, self.swap_total_kb)
    }

    pub fn dirty_writeback_kb(&self) -> u64 {
        self.dirty_kb + self.writeback_kb
    }

    pub fn ram_label(&self) -> String {
        format!(
            "RAM: {:>3.0}%  {}/{}",
            self.ram_pct(),
            human_bytes_gib(self.ram_used_kb),
            human_bytes_gib(self.ram_total_kb),
        )
    }

    pub fn swap_label(&self) -> String {
        format!(
            "SWP: {:>3.0}%  {}/{}",
            self.swap_pct(),
            human_bytes_gib(self.swap_used_kb),
            human_bytes_gib(self.swap_total_kb),
        )
    }

    pub fn dirty_label(&self) -> String {
        format!(
            "Dirty+WB: {}",
            human_bytes_mib(self.dirty_writeback_kb()),
        )
    }
}

/// Raw counters from /proc/vmstat used to compute rates.
#[derive(Debug, Clone, Default)]
pub struct VmStatSnapshot {
    pub pgalloc_total: u64,
    pub pgfree: u64,
    pub pgfault: u64,
    pub pgmajfault: u64,
    pub pswpin: u64,
    pub pswpout: u64,
}

/// Per-tick rates derived from two consecutive VmStatSnapshots.
#[derive(Debug, Clone, Default)]
pub struct VmRates {
    pub alloc_mb_per_sec: f64,
    pub free_mb_per_sec: f64,
    pub fault_per_sec: f64,
    pub major_fault_per_sec: f64,
    pub swapin_mb_per_sec: f64,
    pub swapout_mb_per_sec: f64,
}

impl VmRates {
    pub fn from_deltas(prev: &VmStatSnapshot, curr: &VmStatSnapshot, interval_secs: f64) -> Self {
        let page_size_mb = 4096.0 / (1024.0 * 1024.0);
        let delta = |old: u64, new: u64| (new.saturating_sub(old)) as f64 / interval_secs;

        Self {
            alloc_mb_per_sec: delta(prev.pgalloc_total, curr.pgalloc_total) * page_size_mb,
            free_mb_per_sec: delta(prev.pgfree, curr.pgfree) * page_size_mb,
            fault_per_sec: delta(prev.pgfault, curr.pgfault),
            major_fault_per_sec: delta(prev.pgmajfault, curr.pgmajfault),
            swapin_mb_per_sec: delta(prev.pswpin, curr.pswpin) * page_size_mb,
            swapout_mb_per_sec: delta(prev.pswpout, curr.pswpout) * page_size_mb,
        }
    }
}

/// Memory pressure stall information from /proc/pressure/memory.
#[derive(Debug, Clone, Default)]
pub struct PsiSnapshot {
    pub some_avg10: f64,
    pub full_avg10: f64,
    pub some_total_us: u64,
    pub full_total_us: u64,
}

impl PsiSnapshot {
    pub fn some_label(&self) -> String {
        format!("some: {:.1}%", self.some_avg10)
    }

    pub fn full_label(&self) -> String {
        format!("full: {:.1}%", self.full_avg10)
    }

    pub fn summary_label(&self) -> String {
        if self.some_avg10 < 1.0 && self.full_avg10 < 1.0 {
            "PSI: healthy".to_string()
        } else if self.full_avg10 >= 10.0 {
            format!("PSI: CRITICAL ({:.0}% full stall)", self.full_avg10)
        } else if self.some_avg10 >= 10.0 {
            format!("PSI: stressed ({:.0}% some stall)", self.some_avg10)
        } else {
            format!("PSI: mild ({:.1}% some)", self.some_avg10)
        }
    }

    pub fn severity_pct(&self) -> f64 {
        self.some_avg10.max(self.full_avg10).clamp(0.0, 100.0)
    }
}

/// Static hardware info collected once at startup.
#[derive(Debug, Clone)]
pub struct HardwareInfo {
    pub summary: String,
}

fn cache_path() -> std::path::PathBuf {
    let base = match std::env::var("SUDO_USER") {
        Ok(user) if !user.is_empty() => {
            std::path::PathBuf::from(format!("/home/{}/.cache", user))
        }
        _ => dirs::cache_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp")),
    };
    base.join("ram").join("hardware.txt")
}

pub fn refresh_hardware_cache() -> Result<()> {
    let dimms = read_dmidecode_dimms();
    if dimms.is_empty() {
        anyhow::bail!(
            "dmidecode returned no DIMM data. Are you running with sudo?"
        );
    }

    let cache = cache_path();
    if let Some(parent) = cache.parent() {
        fs::create_dir_all(parent)?;
    }

    let lines: Vec<String> = dimms
        .iter()
        .map(|d| format!("{}\t{}\t{}\t{}", d.size, d.memory_type, d.speed, d.manufacturer))
        .collect();
    fs::write(&cache, lines.join("\n"))?;

    eprintln!("Cached {} DIMM(s) to {}", dimms.len(), cache.display());
    Ok(())
}

pub fn read_hardware_info() -> HardwareInfo {
    let total_gib = read_meminfo()
        .map(|info| info.ram_total_kb as f64 / (1024.0 * 1024.0))
        .unwrap_or(0.0);

    let dimms = read_cached_dimms()
        .or_else(|| {
            let live = read_dmidecode_dimms();
            if !live.is_empty() { Some(live) } else { None }
        });

    let summary = match dimms {
        Some(ref dimms) if !dimms.is_empty() => {
            let dimm_count = dimms.len();
            let first = &dimms[0];
            format!(
                "{:.0} GiB | {}x {} {} {} @ {} MT/s",
                total_gib,
                dimm_count,
                first.size,
                first.memory_type,
                first.manufacturer,
                first.speed,
            )
        }
        _ => {
            let board = fs::read_to_string("/sys/devices/virtual/dmi/id/board_name")
                .unwrap_or_default()
                .trim()
                .to_string();
            let vendor = fs::read_to_string("/sys/devices/virtual/dmi/id/board_vendor")
                .unwrap_or_default()
                .trim()
                .to_string();
            let board_str = if board.is_empty() {
                "Unknown board".to_string()
            } else {
                format!("{} {}", vendor, board)
            };
            format!(
                "{:.0} GiB | {} | (run: sudo ram --refresh-hardware)",
                total_gib, board_str
            )
        }
    };

    HardwareInfo { summary }
}

fn read_cached_dimms() -> Option<Vec<DimmInfo>> {
    let content = fs::read_to_string(cache_path()).ok()?;
    let dimms: Vec<DimmInfo> = content
        .lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 4 {
                Some(DimmInfo {
                    size: parts[0].to_string(),
                    memory_type: parts[1].to_string(),
                    speed: parts[2].to_string(),
                    manufacturer: parts[3].to_string(),
                })
            } else {
                None
            }
        })
        .collect();

    if dimms.is_empty() { None } else { Some(dimms) }
}

struct DimmInfo {
    size: String,
    memory_type: String,
    speed: String,
    manufacturer: String,
}

fn read_dmidecode_dimms() -> Vec<DimmInfo> {
    let output = Command::new("dmidecode")
        .args(["-t", "memory"])
        .output();

    let output = match output {
        Ok(out) if out.status.success() => out,
        _ => return Vec::new(),
    };

    let text = String::from_utf8_lossy(&output.stdout);
    let mut dimms = Vec::new();
    let mut in_device = false;
    let mut size = String::new();
    let mut mem_type = String::new();
    let mut speed = String::new();
    let mut manufacturer = String::new();

    for line in text.lines() {
        let trimmed = line.trim();

        if trimmed == "Memory Device" {
            in_device = true;
            size.clear();
            mem_type.clear();
            speed.clear();
            manufacturer.clear();
            continue;
        }

        if !in_device {
            continue;
        }

        if let Some(val) = trimmed.strip_prefix("Size: ") {
            size = val.to_string();
        } else if let Some(val) = trimmed.strip_prefix("Type: ") {
            mem_type = val.to_string();
        } else if let Some(val) = trimmed.strip_prefix("Configured Memory Speed: ") {
            speed = val.trim_end_matches(" MT/s").to_string();
        } else if let Some(val) = trimmed.strip_prefix("Manufacturer: ") {
            manufacturer = val.to_string();
        }

        if trimmed.is_empty() && in_device {
            in_device = false;
            if !size.contains("No Module") && !size.contains("Not Installed") && !size.is_empty() {
                dimms.push(DimmInfo {
                    size: size.clone(),
                    memory_type: mem_type.clone(),
                    speed: speed.clone(),
                    manufacturer: manufacturer.clone(),
                });
            }
        }
    }

    dimms
}

pub fn read_meminfo() -> Result<MemInfo> {
    let content = fs::read_to_string("/proc/meminfo")?;

    let mut mem_total: u64 = 0;
    let mut mem_available: u64 = 0;
    let mut swap_total: u64 = 0;
    let mut swap_free: u64 = 0;
    let mut dirty: u64 = 0;
    let mut writeback: u64 = 0;

    for line in content.lines() {
        let mut parts = line.split_whitespace();
        let key = parts.next().unwrap_or("");
        let value: u64 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);

        match key {
            "MemTotal:" => mem_total = value,
            "MemAvailable:" => mem_available = value,
            "SwapTotal:" => swap_total = value,
            "SwapFree:" => swap_free = value,
            "Dirty:" => dirty = value,
            "Writeback:" => writeback = value,
            _ => {}
        }
    }

    Ok(MemInfo {
        ram_total_kb: mem_total,
        ram_used_kb: mem_total.saturating_sub(mem_available),
        swap_total_kb: swap_total,
        swap_used_kb: swap_total.saturating_sub(swap_free),
        dirty_kb: dirty,
        writeback_kb: writeback,
    })
}

pub fn read_vmstat() -> Result<VmStatSnapshot> {
    let content = fs::read_to_string("/proc/vmstat")?;

    let mut snap = VmStatSnapshot::default();

    for line in content.lines() {
        let mut parts = line.split_whitespace();
        let key = parts.next().unwrap_or("");
        let value: u64 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);

        match key {
            "pgalloc_normal" | "pgalloc_dma" | "pgalloc_dma32" | "pgalloc_movable" => {
                snap.pgalloc_total += value;
            }
            "pgfree" => snap.pgfree = value,
            "pgfault" => snap.pgfault = value,
            "pgmajfault" => snap.pgmajfault = value,
            "pswpin" => snap.pswpin = value,
            "pswpout" => snap.pswpout = value,
            _ => {}
        }
    }

    Ok(snap)
}

pub fn read_psi() -> Result<PsiSnapshot> {
    let content = fs::read_to_string("/proc/pressure/memory")?;
    let mut snap = PsiSnapshot::default();

    for line in content.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        let avg10 = parts.iter()
            .find(|p| p.starts_with("avg10="))
            .and_then(|p| p.strip_prefix("avg10="))
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0);

        let total = parts.iter()
            .find(|p| p.starts_with("total="))
            .and_then(|p| p.strip_prefix("total="))
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);

        match parts[0] {
            "some" => {
                snap.some_avg10 = avg10;
                snap.some_total_us = total;
            }
            "full" => {
                snap.full_avg10 = avg10;
                snap.full_total_us = total;
            }
            _ => {}
        }
    }

    Ok(snap)
}

fn human_bytes_gib(kb: u64) -> String {
    let gib = kb as f64 / (1024.0 * 1024.0);
    if gib >= 1.0 {
        format!("{:.1}GiB", gib)
    } else {
        let mib = kb as f64 / 1024.0;
        format!("{:.0}MiB", mib)
    }
}

fn human_bytes_mib(kb: u64) -> String {
    let mib = kb as f64 / 1024.0;
    if mib >= 1024.0 {
        format!("{:.1}GiB", mib / 1024.0)
    } else {
        format!("{:.1}MiB", mib)
    }
}

pub fn human_rate(mb_per_sec: f64) -> String {
    if mb_per_sec >= 1024.0 {
        format!("{:.1} GB/s", mb_per_sec / 1024.0)
    } else if mb_per_sec >= 1.0 {
        format!("{:.0} MB/s", mb_per_sec)
    } else {
        format!("{:.1} MB/s", mb_per_sec)
    }
}

pub fn human_count(count: f64) -> String {
    if count >= 1_000_000.0 {
        format!("{:.1}M/s", count / 1_000_000.0)
    } else if count >= 1_000.0 {
        format!("{:.1}K/s", count / 1_000.0)
    } else {
        format!("{:.0}/s", count)
    }
}

fn pct(used: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    (used as f64 / total as f64) * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_percentages() {
        let info = MemInfo {
            ram_total_kb: 1000,
            ram_used_kb: 150,
            swap_total_kb: 500,
            swap_used_kb: 50,
            dirty_kb: 0,
            writeback_kb: 0,
        };
        assert!((info.ram_pct() - 15.0).abs() < 0.01);
        assert!((info.swap_pct() - 10.0).abs() < 0.01);
    }

    #[test]
    fn test_zero_total() {
        let info = MemInfo {
            ram_total_kb: 0,
            ram_used_kb: 0,
            swap_total_kb: 0,
            swap_used_kb: 0,
            dirty_kb: 0,
            writeback_kb: 0,
        };
        assert_eq!(info.ram_pct(), 0.0);
        assert_eq!(info.swap_pct(), 0.0);
    }

    #[test]
    fn test_human_rate() {
        assert_eq!(human_rate(0.5), "0.5 MB/s");
        assert_eq!(human_rate(42.0), "42 MB/s");
        assert_eq!(human_rate(1500.0), "1.5 GB/s");
    }

    #[test]
    fn test_human_count() {
        assert_eq!(human_count(50.0), "50/s");
        assert_eq!(human_count(1500.0), "1.5K/s");
        assert_eq!(human_count(2_500_000.0), "2.5M/s");
    }

    #[test]
    fn test_psi_summary() {
        let healthy = PsiSnapshot { some_avg10: 0.0, full_avg10: 0.0, ..Default::default() };
        assert!(healthy.summary_label().contains("healthy"));

        let critical = PsiSnapshot { some_avg10: 50.0, full_avg10: 25.0, ..Default::default() };
        assert!(critical.summary_label().contains("CRITICAL"));

        let stressed = PsiSnapshot { some_avg10: 15.0, full_avg10: 5.0, ..Default::default() };
        assert!(stressed.summary_label().contains("stressed"));

        let mild = PsiSnapshot { some_avg10: 3.0, full_avg10: 0.5, ..Default::default() };
        assert!(mild.summary_label().contains("mild"));
    }

    #[test]
    fn test_vm_rates_from_deltas() {
        let prev = VmStatSnapshot {
            pgalloc_total: 1000,
            pgfree: 500,
            pgfault: 100,
            pgmajfault: 10,
            pswpin: 0,
            pswpout: 0,
        };
        let curr = VmStatSnapshot {
            pgalloc_total: 2000,
            pgfree: 1500,
            pgfault: 600,
            pgmajfault: 12,
            pswpin: 0,
            pswpout: 0,
        };
        let rates = VmRates::from_deltas(&prev, &curr, 1.0);
        assert!(rates.alloc_mb_per_sec > 0.0);
        assert!(rates.free_mb_per_sec > 0.0);
        assert!((rates.fault_per_sec - 500.0).abs() < 0.01);
        assert!((rates.major_fault_per_sec - 2.0).abs() < 0.01);
    }
}
