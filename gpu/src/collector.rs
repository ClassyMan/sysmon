use std::process::Command;
use anyhow::Result;

#[derive(Debug, Clone)]
pub struct GpuSnapshot {
    pub name: String,
    pub driver: String,
    pub pcie_gen: String,
    pub pcie_width: String,
    pub vram_total_mib: f64,
    pub vram_used_mib: f64,
    pub gpu_util_pct: f64,
    pub mem_util_pct: f64,
    pub temp_celsius: f64,
    pub power_watts: f64,
    pub power_limit_watts: f64,
    pub clock_gpu_mhz: f64,
    pub clock_mem_mhz: f64,
    pub fan_pct: f64,
}

impl GpuSnapshot {
    pub fn vram_pct(&self) -> f64 {
        if self.vram_total_mib <= 0.0 {
            return 0.0;
        }
        (self.vram_used_mib / self.vram_total_mib) * 100.0
    }

    pub fn power_pct(&self) -> f64 {
        if self.power_limit_watts <= 0.0 {
            return 0.0;
        }
        (self.power_watts / self.power_limit_watts) * 100.0
    }

    pub fn header_line(&self) -> String {
        format!(
            "{} | PCIe Gen{}x{} | GPU {}MHz | MEM {}MHz | {}°C | {:.0}W/{:.0}W | Fan {}%",
            self.name,
            self.pcie_gen,
            self.pcie_width,
            self.clock_gpu_mhz as u64,
            self.clock_mem_mhz as u64,
            self.temp_celsius as u64,
            self.power_watts,
            self.power_limit_watts,
            self.fan_pct as u64,
        )
    }

    pub fn vram_label(&self) -> String {
        format!(
            "VRAM: {:.0}/{:.0} MiB ({:.0}%)",
            self.vram_used_mib,
            self.vram_total_mib,
            self.vram_pct(),
        )
    }
}

#[derive(Debug, Clone)]
pub struct GpuProcess {
    pub pid: u32,
    pub name: String,
    pub proc_type: String,
    pub vram_mib: u64,
    pub gpu_pct: Option<f64>,
    pub mem_pct: Option<f64>,
}

pub fn read_gpu_snapshot() -> Result<GpuSnapshot> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,driver_version,pcie.link.gen.current,pcie.link.width.current,memory.total,memory.used,memory.free,utilization.gpu,utilization.memory,temperature.gpu,power.draw,power.limit,clocks.gr,clocks.mem,fan.speed",
            "--format=csv,noheader,nounits",
        ])
        .output()?;

    if !output.status.success() {
        anyhow::bail!("nvidia-smi failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    let line = String::from_utf8_lossy(&output.stdout).trim().to_string();
    parse_gpu_csv(&line)
}

fn parse_gpu_csv(line: &str) -> Result<GpuSnapshot> {
    let fields: Vec<&str> = line.split(", ").collect();
    if fields.len() < 15 {
        anyhow::bail!("unexpected nvidia-smi output: {}", line);
    }

    Ok(GpuSnapshot {
        name: fields[0].to_string(),
        driver: fields[1].to_string(),
        pcie_gen: fields[2].to_string(),
        pcie_width: fields[3].to_string(),
        vram_total_mib: fields[4].parse().unwrap_or(0.0),
        vram_used_mib: fields[5].parse().unwrap_or(0.0),
        gpu_util_pct: fields[7].parse().unwrap_or(0.0),
        mem_util_pct: fields[8].parse().unwrap_or(0.0),
        temp_celsius: fields[9].parse().unwrap_or(0.0),
        power_watts: fields[10].parse().unwrap_or(0.0),
        power_limit_watts: fields[11].parse().unwrap_or(0.0),
        clock_gpu_mhz: fields[12].parse().unwrap_or(0.0),
        clock_mem_mhz: fields[13].parse().unwrap_or(0.0),
        fan_pct: fields[14].parse().unwrap_or(0.0),
    })
}

pub fn read_gpu_processes() -> Vec<GpuProcess> {
    let output = Command::new("nvidia-smi")
        .args(["pmon", "-c", "1", "-s", "mu"])
        .output();

    let output = match output {
        Ok(out) if out.status.success() => out,
        _ => return Vec::new(),
    };

    let text = String::from_utf8_lossy(&output.stdout);
    parse_pmon(&text)
}

fn parse_pmon(text: &str) -> Vec<GpuProcess> {
    text.lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 12 {
                return None;
            }

            let pid: u32 = fields[1].parse().ok()?;
            let proc_type = fields[2].to_string();
            let vram_mib: u64 = fields[3].parse().unwrap_or(0);
            let gpu_pct = fields[5].parse::<f64>().ok();
            let mem_pct = fields[6].parse::<f64>().ok();
            let name = fields[11].to_string();

            Some(GpuProcess {
                pid,
                name,
                proc_type,
                vram_mib,
                gpu_pct,
                mem_pct,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const GPU_CSV: &str = "NVIDIA GeForce RTX 3090, 590.48.01, 2, 16, 24576, 1481, 22644, 11, 10, 28, 24.13, 350.00, 480, 810, 0";

    #[test]
    fn test_parse_gpu_csv() {
        let snap = parse_gpu_csv(GPU_CSV).unwrap();
        assert_eq!(snap.name, "NVIDIA GeForce RTX 3090");
        assert_eq!(snap.vram_total_mib, 24576.0);
        assert_eq!(snap.vram_used_mib, 1481.0);
        assert_eq!(snap.gpu_util_pct, 11.0);
        assert_eq!(snap.temp_celsius, 28.0);
        assert!((snap.power_watts - 24.13).abs() < 0.01);
    }

    #[test]
    fn test_vram_pct() {
        let snap = parse_gpu_csv(GPU_CSV).unwrap();
        let pct = snap.vram_pct();
        assert!(pct > 5.0 && pct < 10.0);
    }

    #[test]
    fn test_power_pct() {
        let snap = parse_gpu_csv(GPU_CSV).unwrap();
        let pct = snap.power_pct();
        assert!(pct > 5.0 && pct < 10.0);
    }

    const PMON_OUTPUT: &str = "\
# gpu         pid   type     fb   ccpm     sm    mem    enc    dec    jpg    ofa    command
# Idx           #    C/G     MB     MB      %      %      %      %      %      %    name
    0       8548     G    912      0     12      9      -      -      -      -    Xorg
    0       8788     G    110      0     15     11      -      -      -      -    gnome-shell
    0     269934   C+G    127      0      -      -      -      -      -      -    zed-editor";

    #[test]
    fn test_parse_pmon() {
        let procs = parse_pmon(PMON_OUTPUT);
        assert_eq!(procs.len(), 3);
        assert_eq!(procs[0].pid, 8548);
        assert_eq!(procs[0].name, "Xorg");
        assert_eq!(procs[0].vram_mib, 912);
        assert_eq!(procs[0].gpu_pct, Some(12.0));
        assert_eq!(procs[2].name, "zed-editor");
        assert_eq!(procs[2].gpu_pct, None);
    }
}
