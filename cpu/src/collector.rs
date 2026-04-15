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
    let cpuinfo_content = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let has_hyperthreading = fs::read_to_string("/sys/devices/system/cpu/cpu0/topology/core_cpus_list").is_ok();
    let max_freq_khz = fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_max_freq")
        .ok()
        .and_then(|s| s.trim().parse::<f64>().ok());

    parse_cpuinfo(&cpuinfo_content, has_hyperthreading, max_freq_khz)
}

fn parse_cpuinfo(content: &str, has_hyperthreading: bool, max_freq_khz: Option<f64>) -> CpuInfo {
    let model = content
        .lines()
        .find(|l| l.starts_with("model name"))
        .and_then(|l| l.split(':').nth(1))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "Unknown CPU".to_string());

    let threads = content
        .lines()
        .filter(|l| l.starts_with("processor"))
        .count();

    let cores = if has_hyperthreading { threads / 2 } else { threads };
    let max_freq_mhz = max_freq_khz.map(|khz| khz / 1000.0).unwrap_or(0.0);

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
        .and_then(|s| parse_loadavg(&s))
        .unwrap_or((0.0, 0.0, 0.0))
}

fn parse_loadavg(content: &str) -> Option<(f64, f64, f64)> {
    let fields: Vec<f64> = content
        .split_whitespace()
        .take(3)
        .filter_map(|f| f.parse().ok())
        .collect();
    if fields.len() >= 3 {
        Some((fields[0], fields[1], fields[2]))
    } else {
        None
    }
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

    #[test]
    fn test_usage_pct_zero_total_delta() {
        let same = CpuTimes { user: 100, idle: 900, ..Default::default() };
        assert_eq!(usage_pct(&same, &same), 0.0);
    }

    #[test]
    fn test_usage_pct_full_load() {
        let prev = CpuTimes { user: 0, idle: 1000, ..Default::default() };
        let curr = CpuTimes { user: 1000, idle: 1000, ..Default::default() };
        let pct = usage_pct(&prev, &curr);
        assert!((pct - 100.0).abs() < 0.1, "expected ~100%, got {}", pct);
    }

    #[test]
    fn test_parse_stat_empty_input() {
        let snap = parse_stat("").unwrap();
        assert_eq!(snap.per_core.len(), 0);
        assert_eq!(snap.total.total(), 0);
    }

    #[test]
    fn test_parse_stat_ignores_non_cpu_lines() {
        let input = "\
cpu  100 0 50 800 0 0 0 0 0 0
intr 123456789
ctxt 987654321
softirq 111111";
        let snap = parse_stat(input).unwrap();
        assert_eq!(snap.per_core.len(), 0);
        assert_eq!(snap.total.user, 100);
    }

    #[test]
    fn test_cpu_times_all_fields_contribute() {
        let times = CpuTimes {
            user: 10,
            nice: 20,
            system: 30,
            idle: 40,
            iowait: 50,
            irq: 60,
            softirq: 70,
            steal: 80,
        };
        assert_eq!(times.total(), 360);
        assert_eq!(times.busy(), 270); // total - idle - iowait = 360 - 40 - 50
    }

    const PROC_CPUINFO: &str = "\
processor\t: 0
vendor_id\t: AuthenticAMD
model name\t: AMD Ryzen 7 5800X 8-Core Processor
cpu MHz\t\t: 3800.000
processor\t: 1
vendor_id\t: AuthenticAMD
model name\t: AMD Ryzen 7 5800X 8-Core Processor
cpu MHz\t\t: 3800.000
processor\t: 2
vendor_id\t: AuthenticAMD
model name\t: AMD Ryzen 7 5800X 8-Core Processor
cpu MHz\t\t: 3800.000
processor\t: 3
vendor_id\t: AuthenticAMD
model name\t: AMD Ryzen 7 5800X 8-Core Processor
cpu MHz\t\t: 3800.000";

    #[test]
    fn test_parse_cpuinfo_model_and_threads() {
        let info = parse_cpuinfo(PROC_CPUINFO, false, None);
        assert_eq!(info.model, "AMD Ryzen 7 5800X 8-Core Processor");
        assert_eq!(info.threads, 4);
        assert_eq!(info.cores, 4);
        assert_eq!(info.max_freq_mhz, 0.0);
    }

    #[test]
    fn test_parse_cpuinfo_with_hyperthreading() {
        let info = parse_cpuinfo(PROC_CPUINFO, true, None);
        assert_eq!(info.threads, 4);
        assert_eq!(info.cores, 2);
    }

    #[test]
    fn test_parse_cpuinfo_with_freq() {
        let info = parse_cpuinfo(PROC_CPUINFO, false, Some(4500000.0));
        assert!((info.max_freq_mhz - 4500.0).abs() < 0.1);
    }

    #[test]
    fn test_parse_cpuinfo_empty() {
        let info = parse_cpuinfo("", false, None);
        assert_eq!(info.model, "Unknown CPU");
        assert_eq!(info.threads, 0);
        assert_eq!(info.cores, 0);
    }

    #[test]
    fn test_parse_loadavg_normal() {
        let result = parse_loadavg("1.50 2.00 1.80 3/1234 56789");
        assert_eq!(result, Some((1.50, 2.00, 1.80)));
    }

    #[test]
    fn test_parse_loadavg_empty() {
        assert_eq!(parse_loadavg(""), None);
    }

    #[test]
    fn test_parse_loadavg_too_few_fields() {
        assert_eq!(parse_loadavg("1.0 2.0"), None);
    }

    #[test]
    fn test_parse_loadavg_with_garbage() {
        assert_eq!(parse_loadavg("abc def ghi"), None);
    }

    #[test]
    fn test_parse_stat_multiple_cores() {
        let input = "\
cpu  1000 0 500 8000 0 0 0 0 0 0
cpu0 250 0 125 2000 0 0 0 0 0 0
cpu1 250 0 125 2000 0 0 0 0 0 0
cpu2 250 0 125 2000 0 0 0 0 0 0
cpu3 250 0 125 2000 0 0 0 0 0 0";
        let snap = parse_stat(input).unwrap();
        assert_eq!(snap.per_core.len(), 4);
        assert_eq!(snap.total.user, 1000);
        assert_eq!(snap.per_core[2].user, 250);
    }
}
