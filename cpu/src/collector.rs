use std::fs;
use anyhow::Result;

#[derive(Debug, Clone)]
pub struct CpuInfo {
    pub model: String,
    pub cores: usize,
    pub threads: usize,
    pub max_freq_mhz: f64,
}

pub fn read_cpu_info() -> CpuInfo {
    let model = fs::read_to_string("/proc/cpuinfo")
        .unwrap_or_default()
        .lines()
        .find(|l| l.starts_with("model name"))
        .and_then(|l| l.split(':').nth(1))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "Unknown CPU".to_string());

    let threads = fs::read_to_string("/proc/cpuinfo")
        .unwrap_or_default()
        .lines()
        .filter(|l| l.starts_with("processor"))
        .count();

    let cores = fs::read_to_string("/sys/devices/system/cpu/cpu0/topology/core_cpus_list")
        .ok()
        .map(|_| threads / 2)
        .unwrap_or(threads);

    let max_freq_mhz = fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_max_freq")
        .ok()
        .and_then(|s| s.trim().parse::<f64>().ok())
        .map(|khz| khz / 1000.0)
        .unwrap_or(0.0);

    CpuInfo { model, cores, threads, max_freq_mhz }
}

#[derive(Debug, Clone)]
pub struct CpuSnapshot {
    pub total: CpuTimes,
    pub per_core: Vec<CpuTimes>,
}

#[derive(Debug, Clone, Default)]
pub struct CpuTimes {
    pub user: u64,
    pub nice: u64,
    pub system: u64,
    pub idle: u64,
    pub iowait: u64,
    pub irq: u64,
    pub softirq: u64,
    pub steal: u64,
}

impl CpuTimes {
    pub fn total(&self) -> u64 {
        self.user + self.nice + self.system + self.idle + self.iowait
            + self.irq + self.softirq + self.steal
    }

    pub fn busy(&self) -> u64 {
        self.total() - self.idle - self.iowait
    }
}

pub fn usage_pct(prev: &CpuTimes, curr: &CpuTimes) -> f64 {
    let total_delta = curr.total().saturating_sub(prev.total());
    if total_delta == 0 {
        return 0.0;
    }
    let busy_delta = curr.busy().saturating_sub(prev.busy());
    (busy_delta as f64 / total_delta as f64) * 100.0
}

pub fn read_cpu_snapshot() -> Result<CpuSnapshot> {
    let content = fs::read_to_string("/proc/stat")?;
    parse_stat(&content)
}

fn parse_stat(content: &str) -> Result<CpuSnapshot> {
    let mut total = CpuTimes::default();
    let mut per_core = Vec::new();

    for line in content.lines() {
        if line.starts_with("cpu ") {
            total = parse_cpu_line(line);
        } else if line.starts_with("cpu") {
            per_core.push(parse_cpu_line(line));
        }
    }

    Ok(CpuSnapshot { total, per_core })
}

fn parse_cpu_line(line: &str) -> CpuTimes {
    let fields: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|f| f.parse().ok())
        .collect();

    CpuTimes {
        user: *fields.first().unwrap_or(&0),
        nice: *fields.get(1).unwrap_or(&0),
        system: *fields.get(2).unwrap_or(&0),
        idle: *fields.get(3).unwrap_or(&0),
        iowait: *fields.get(4).unwrap_or(&0),
        irq: *fields.get(5).unwrap_or(&0),
        softirq: *fields.get(6).unwrap_or(&0),
        steal: *fields.get(7).unwrap_or(&0),
    }
}

pub fn read_cpu_temp() -> Option<f64> {
    for hwmon in fs::read_dir("/sys/class/hwmon").ok()?.flatten() {
        let name = fs::read_to_string(hwmon.path().join("name")).unwrap_or_default();
        if name.trim() == "k10temp" || name.trim() == "coretemp" {
            let temp = fs::read_to_string(hwmon.path().join("temp1_input"))
                .ok()?
                .trim()
                .parse::<f64>()
                .ok()?;
            return Some(temp / 1000.0);
        }
    }
    None
}

pub fn read_core_freq_mhz(core_idx: usize) -> Option<f64> {
    let path = format!("/sys/devices/system/cpu/cpu{}/cpufreq/scaling_cur_freq", core_idx);
    fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse::<f64>().ok())
        .map(|khz| khz / 1000.0)
}

pub fn read_load_avg() -> (f64, f64, f64) {
    fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|s| {
            let fields: Vec<f64> = s.split_whitespace()
                .take(3)
                .filter_map(|f| f.parse().ok())
                .collect();
            if fields.len() >= 3 {
                Some((fields[0], fields[1], fields[2]))
            } else {
                None
            }
        })
        .unwrap_or((0.0, 0.0, 0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROC_STAT: &str = "\
cpu  1649804 24286 887701 57600351 64543 0 41092 0 193462 0
cpu0 206875 1586 106795 3390400 8328 0 34470 0 24031 0
cpu1 90949 1269 47855 3620025 4285 0 1184 0 15328 0
cpu2 55168 832 35525 3677405 1434 0 350 0 3243 0
cpu3 215758 1620 118741 3410734 16194 0 135 0 26984 0
intr 123456789";

    #[test]
    fn test_parse_stat() {
        let snap = parse_stat(PROC_STAT).unwrap();
        assert_eq!(snap.per_core.len(), 4);
        assert_eq!(snap.total.user, 1649804);
        assert_eq!(snap.per_core[0].user, 206875);
    }

    #[test]
    fn test_usage_pct() {
        let prev = CpuTimes { user: 100, system: 50, idle: 800, ..Default::default() };
        let curr = CpuTimes { user: 200, system: 100, idle: 1050, ..Default::default() };
        let pct = usage_pct(&prev, &curr);
        // busy_delta = (200+100) - (100+50) = 150
        // total_delta = (200+100+1050) - (100+50+800) = 400
        // pct = 150/400 = 37.5%
        assert!((pct - 37.5).abs() < 0.1, "got {}", pct);
    }

    #[test]
    fn test_usage_pct_idle() {
        let prev = CpuTimes { idle: 1000, ..Default::default() };
        let curr = CpuTimes { idle: 2000, ..Default::default() };
        let pct = usage_pct(&prev, &curr);
        assert_eq!(pct, 0.0);
    }

    #[test]
    fn test_cpu_times_total_and_busy() {
        let times = CpuTimes { user: 100, system: 50, idle: 800, iowait: 50, ..Default::default() };
        assert_eq!(times.total(), 1000);
        assert_eq!(times.busy(), 150);
    }
}
