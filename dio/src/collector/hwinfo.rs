use std::fs;
use std::path::Path;

/// Static + live hardware info for a block device.
#[derive(Clone, Debug)]
pub struct DiskHwInfo {
    pub model: String,
    pub capacity_gb: f64,
    pub transport: String,
    pub temp_celsius: Option<f64>,
}

impl DiskHwInfo {
    pub fn summary(&self) -> String {
        let temp_str = self.temp_celsius.map_or_else(
            String::new,
            |t| format!(" | {:.0}°C", t),
        );
        format!(
            "{} | {:.0}GB | {}{}",
            self.model, self.capacity_gb, self.transport, temp_str,
        )
    }
}

pub fn read_disk_hwinfo(device_name: &str) -> Option<DiskHwInfo> {
    let block_path = format!("/sys/block/{}", device_name);
    if !Path::new(&block_path).exists() {
        return None;
    }

    let model = read_trimmed(&format!("{}/device/model", block_path))
        .or_else(|| read_nvme_model(device_name))
        .unwrap_or_default();

    if model.is_empty() {
        return None;
    }

    let sectors = read_trimmed(&format!("{}/size", block_path))
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let capacity_gb = sectors as f64 * 512.0 / 1_000_000_000.0;

    let transport = detect_transport(device_name);
    let temp_celsius = read_nvme_temp(device_name);

    Some(DiskHwInfo {
        model,
        capacity_gb,
        transport,
        temp_celsius,
    })
}

pub fn refresh_temp(info: &mut DiskHwInfo, device_name: &str) {
    info.temp_celsius = read_nvme_temp(device_name);
}

fn read_nvme_model(device_name: &str) -> Option<String> {
    let nvme_name = device_name.trim_end_matches(|c: char| c.is_ascii_digit() && c != '0')
        .trim_end_matches('n');
    read_trimmed(&format!("/sys/class/nvme/{}/model", nvme_name))
}

fn read_nvme_temp(device_name: &str) -> Option<f64> {
    let nvme_name = device_name.trim_end_matches(|c: char| c.is_ascii_digit() && c != '0')
        .trim_end_matches('n');
    let hwmon_dir = format!("/sys/class/nvme/{}", nvme_name);
    let entries = fs::read_dir(&hwmon_dir).ok()?;

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("hwmon") {
            let temp_path = entry.path().join("temp1_input");
            if let Some(val) = read_trimmed(&temp_path.to_string_lossy()) {
                if let Ok(millideg) = val.parse::<f64>() {
                    return Some(millideg / 1000.0);
                }
            }
        }
    }
    None
}

fn detect_transport(device_name: &str) -> String {
    if device_name.starts_with("nvme") {
        let nvme_name = device_name.trim_end_matches(|c: char| c.is_ascii_digit() && c != '0')
            .trim_end_matches('n');
        let transport = read_trimmed(&format!("/sys/class/nvme/{}/transport", nvme_name))
            .unwrap_or_default();
        match transport.as_str() {
            "pcie" => "NVMe PCIe".to_string(),
            "tcp" => "NVMe/TCP".to_string(),
            "rdma" => "NVMe/RDMA".to_string(),
            other if !other.is_empty() => format!("NVMe/{}", other),
            _ => "NVMe".to_string(),
        }
    } else if device_name.starts_with("sd") {
        let rotational = read_trimmed(&format!("/sys/block/{}/queue/rotational", device_name))
            .unwrap_or_default();
        if rotational == "1" { "SATA HDD".to_string() } else { "SATA SSD".to_string() }
    } else {
        "block".to_string()
    }
}

fn read_trimmed(path: &str) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_hwinfo(model: &str, capacity_gb: f64, transport: &str, temp: Option<f64>) -> DiskHwInfo {
        DiskHwInfo {
            model: model.to_string(),
            capacity_gb,
            transport: transport.to_string(),
            temp_celsius: temp,
        }
    }

    #[test]
    fn test_summary_with_all_fields() {
        let info = make_hwinfo("Samsung 990 Pro", 1000.0, "NVMe PCIe", Some(42.0));
        let result = info.summary();
        assert_eq!(result, "Samsung 990 Pro | 1000GB | NVMe PCIe | 42°C");
    }

    #[test]
    fn test_summary_without_temperature() {
        let info = make_hwinfo("WD Blue", 500.0, "SATA SSD", None);
        let result = info.summary();
        assert_eq!(result, "WD Blue | 500GB | SATA SSD");
    }

    #[test]
    fn test_summary_with_empty_model() {
        let info = make_hwinfo("", 256.0, "NVMe", Some(55.0));
        let result = info.summary();
        assert_eq!(result, " | 256GB | NVMe | 55°C");
    }
}
