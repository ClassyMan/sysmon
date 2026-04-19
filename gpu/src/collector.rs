use std::fs;
use std::sync::OnceLock;

use anyhow::{anyhow, Context, Result};
use nvml_wrapper::enum_wrappers::device::{Clock, TemperatureSensor};
use nvml_wrapper::enums::device::UsedGpuMemory;
use nvml_wrapper::Nvml;

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

static NVML: OnceLock<Result<Nvml, String>> = OnceLock::new();

fn nvml() -> Result<&'static Nvml> {
    let outcome = NVML
        .get_or_init(|| Nvml::init().map_err(|err| format!("NVML init failed: {err}")));
    outcome.as_ref().map_err(|msg| anyhow!("{msg}"))
}

const BYTES_PER_MIB: f64 = 1024.0 * 1024.0;

pub fn read_gpu_snapshot() -> Result<GpuSnapshot> {
    let nvml = nvml()?;
    let device = nvml
        .device_by_index(0)
        .context("no NVIDIA device at index 0")?;

    let driver = nvml.sys_driver_version().unwrap_or_default();
    let name = device.name().unwrap_or_default();
    let pcie_gen = device
        .current_pcie_link_gen()
        .map(|value| value.to_string())
        .unwrap_or_else(|_| "?".into());
    let pcie_width = device
        .current_pcie_link_width()
        .map(|value| value.to_string())
        .unwrap_or_else(|_| "?".into());

    let memory = device.memory_info().context("failed to read memory info")?;
    let utilization = device
        .utilization_rates()
        .context("failed to read utilization rates")?;
    let temperature = device.temperature(TemperatureSensor::Gpu).unwrap_or(0) as f64;
    let power_milliwatts = device.power_usage().unwrap_or(0) as f64;
    let power_limit_milliwatts = device.power_management_limit().unwrap_or(0) as f64;
    let clock_graphics = device.clock_info(Clock::Graphics).unwrap_or(0) as f64;
    let clock_memory = device.clock_info(Clock::Memory).unwrap_or(0) as f64;
    let fan_percent = device.fan_speed(0).unwrap_or(0) as f64;

    Ok(GpuSnapshot {
        name,
        driver,
        pcie_gen,
        pcie_width,
        vram_total_mib: memory.total as f64 / BYTES_PER_MIB,
        vram_used_mib: memory.used as f64 / BYTES_PER_MIB,
        gpu_util_pct: utilization.gpu as f64,
        mem_util_pct: utilization.memory as f64,
        temp_celsius: temperature,
        power_watts: power_milliwatts / 1000.0,
        power_limit_watts: power_limit_milliwatts / 1000.0,
        clock_gpu_mhz: clock_graphics,
        clock_mem_mhz: clock_memory,
        fan_pct: fan_percent,
    })
}

pub fn read_gpu_processes() -> Vec<GpuProcess> {
    let Ok(nvml) = nvml() else {
        return Vec::new();
    };
    let Ok(device) = nvml.device_by_index(0) else {
        return Vec::new();
    };

    let compute_processes = device.running_compute_processes().unwrap_or_default();
    let graphics_processes = device.running_graphics_processes().unwrap_or_default();

    let mut merged: Vec<GpuProcess> = Vec::new();
    for process in &compute_processes {
        merged.push(GpuProcess {
            pid: process.pid,
            name: read_process_name(process.pid),
            proc_type: "C".to_string(),
            vram_mib: used_memory_to_mib(&process.used_gpu_memory),
            gpu_pct: None,
            mem_pct: None,
        });
    }
    for process in &graphics_processes {
        if let Some(existing) = merged.iter_mut().find(|entry| entry.pid == process.pid) {
            existing.proc_type = "C+G".to_string();
        } else {
            merged.push(GpuProcess {
                pid: process.pid,
                name: read_process_name(process.pid),
                proc_type: "G".to_string(),
                vram_mib: used_memory_to_mib(&process.used_gpu_memory),
                gpu_pct: None,
                mem_pct: None,
            });
        }
    }
    merged
}

fn used_memory_to_mib(used: &UsedGpuMemory) -> u64 {
    match used {
        UsedGpuMemory::Used(bytes) => bytes / (1024 * 1024),
        UsedGpuMemory::Unavailable => 0,
    }
}

fn read_process_name(pid: u32) -> String {
    fs::read_to_string(format!("/proc/{pid}/comm"))
        .map(|contents| contents.trim().to_string())
        .unwrap_or_else(|_| format!("pid {pid}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_snapshot() -> GpuSnapshot {
        GpuSnapshot {
            name: "NVIDIA GeForce RTX 3090".to_string(),
            driver: "590.48.01".to_string(),
            pcie_gen: "4".to_string(),
            pcie_width: "16".to_string(),
            vram_total_mib: 24576.0,
            vram_used_mib: 1481.0,
            gpu_util_pct: 11.0,
            mem_util_pct: 10.0,
            temp_celsius: 28.0,
            power_watts: 24.13,
            power_limit_watts: 350.0,
            clock_gpu_mhz: 480.0,
            clock_mem_mhz: 810.0,
            fan_pct: 0.0,
        }
    }

    #[test]
    fn test_vram_pct() {
        let pct = sample_snapshot().vram_pct();
        assert!(pct > 5.0 && pct < 10.0);
    }

    #[test]
    fn test_power_pct() {
        let pct = sample_snapshot().power_pct();
        assert!(pct > 5.0 && pct < 10.0);
    }

    #[test]
    fn test_vram_pct_zero_total() {
        let snap = GpuSnapshot {
            vram_total_mib: 0.0,
            vram_used_mib: 100.0,
            ..sample_snapshot()
        };
        assert_eq!(snap.vram_pct(), 0.0);
    }

    #[test]
    fn test_power_pct_zero_limit() {
        let snap = GpuSnapshot {
            power_limit_watts: 0.0,
            power_watts: 100.0,
            ..sample_snapshot()
        };
        assert_eq!(snap.power_pct(), 0.0);
    }

    #[test]
    fn test_power_pct_full_load() {
        let snap = GpuSnapshot {
            power_watts: 350.0,
            power_limit_watts: 350.0,
            ..sample_snapshot()
        };
        assert!((snap.power_pct() - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_header_line_format() {
        let header = sample_snapshot().header_line();
        assert!(header.contains("NVIDIA GeForce RTX 3090"));
        assert!(header.contains("PCIe Gen"));
        assert!(header.contains("°C"));
        assert!(header.contains("Fan"));
    }

    #[test]
    fn test_header_line_contains_clocks() {
        let header = sample_snapshot().header_line();
        assert!(header.contains("480MHz"), "expected GPU clock in header: {header}");
        assert!(header.contains("810MHz"), "expected MEM clock in header: {header}");
    }

    #[test]
    fn test_vram_label_format() {
        let label = sample_snapshot().vram_label();
        assert!(label.contains("VRAM:"));
        assert!(label.contains("MiB"));
        assert!(label.contains("%"));
    }

    #[test]
    fn test_vram_label_with_real_data() {
        let label = sample_snapshot().vram_label();
        assert!(label.contains("1481"));
        assert!(label.contains("24576"));
    }

    #[test]
    fn test_used_memory_to_mib_used() {
        let mib = used_memory_to_mib(&UsedGpuMemory::Used(1024 * 1024 * 128));
        assert_eq!(mib, 128);
    }

    #[test]
    fn test_used_memory_to_mib_unavailable() {
        let mib = used_memory_to_mib(&UsedGpuMemory::Unavailable);
        assert_eq!(mib, 0);
    }
}
