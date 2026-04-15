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

    let size_str = read_trimmed(&format!("{}/size", block_path)).unwrap_or_default();
    let transport_str = if device_name.starts_with("nvme") {
        let nvme = nvme_base_name(device_name);
        read_trimmed(&format!("/sys/class/nvme/{}/transport", nvme)).unwrap_or_default()
    } else {
        String::new()
    };
    let rotational_str = read_trimmed(&format!("{}/queue/rotational", block_path)).unwrap_or_default();
    let temp_str = read_nvme_temp_raw(device_name);

    Some(parse_disk_hwinfo(
        &model,
        &size_str,
        device_name,
        &transport_str,
        &rotational_str,
        temp_str.as_deref(),
    ))
}

pub fn refresh_temp(info: &mut DiskHwInfo, device_name: &str) {
    let temp_str = read_nvme_temp_raw(device_name);
    info.temp_celsius = temp_str.as_deref().and_then(parse_temp_celsius);
}

fn parse_disk_hwinfo(
    model: &str,
    size_sectors_str: &str,
    device_name: &str,
    nvme_transport_str: &str,
    rotational_str: &str,
    temp_millideg_str: Option<&str>,
) -> DiskHwInfo {
    DiskHwInfo {
        model: model.to_string(),
        capacity_gb: parse_capacity_gb(size_sectors_str),
        transport: parse_transport(device_name, nvme_transport_str, rotational_str),
        temp_celsius: temp_millideg_str.and_then(parse_temp_celsius),
    }
}

fn parse_capacity_gb(size_sectors_str: &str) -> f64 {
    let sectors = size_sectors_str.parse::<u64>().unwrap_or(0);
    sectors as f64 * 512.0 / 1_000_000_000.0
}

fn parse_transport(device_name: &str, nvme_transport: &str, rotational: &str) -> String {
    if device_name.starts_with("nvme") {
        match nvme_transport.trim() {
            "pcie" => "NVMe PCIe".to_string(),
            "tcp" => "NVMe/TCP".to_string(),
            "rdma" => "NVMe/RDMA".to_string(),
            other if !other.is_empty() => format!("NVMe/{}", other),
            _ => "NVMe".to_string(),
        }
    } else if device_name.starts_with("sd") {
        if rotational.trim() == "1" { "SATA HDD".to_string() } else { "SATA SSD".to_string() }
    } else {
        "block".to_string()
    }
}

fn parse_temp_celsius(millideg_str: &str) -> Option<f64> {
    millideg_str.trim().parse::<f64>().ok().map(|m| m / 1000.0)
}

fn nvme_base_name(device_name: &str) -> String {
    device_name
        .trim_end_matches(|c: char| c.is_ascii_digit() && c != '0')
        .trim_end_matches('n')
        .to_string()
}

fn read_nvme_model(device_name: &str) -> Option<String> {
    let nvme_name = nvme_base_name(device_name);
    read_trimmed(&format!("/sys/class/nvme/{}/model", nvme_name))
}

fn read_nvme_temp_raw(device_name: &str) -> Option<String> {
    if !device_name.starts_with("nvme") {
        return None;
    }
    let nvme_name = nvme_base_name(device_name);
    let hwmon_dir = format!("/sys/class/nvme/{}", nvme_name);
    let entries = fs::read_dir(&hwmon_dir).ok()?;

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("hwmon") {
            let temp_path = entry.path().join("temp1_input");
            if let Some(val) = read_trimmed(&temp_path.to_string_lossy()) {
                return Some(val);
            }
        }
    }
    None
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

    #[test]
    fn test_parse_capacity_gb_1tb() {
        let gb = parse_capacity_gb("2000409264");
        assert!((gb - 1024.2).abs() < 0.1);
    }

    #[test]
    fn test_parse_capacity_gb_zero() {
        assert_eq!(parse_capacity_gb("0"), 0.0);
    }

    #[test]
    fn test_parse_capacity_gb_invalid() {
        assert_eq!(parse_capacity_gb("not_a_number"), 0.0);
    }

    #[test]
    fn test_parse_capacity_gb_empty() {
        assert_eq!(parse_capacity_gb(""), 0.0);
    }

    #[test]
    fn test_parse_transport_nvme_pcie() {
        assert_eq!(parse_transport("nvme0n1", "pcie", ""), "NVMe PCIe");
    }

    #[test]
    fn test_parse_transport_nvme_tcp() {
        assert_eq!(parse_transport("nvme1n1", "tcp", ""), "NVMe/TCP");
    }

    #[test]
    fn test_parse_transport_nvme_rdma() {
        assert_eq!(parse_transport("nvme0n1", "rdma", ""), "NVMe/RDMA");
    }

    #[test]
    fn test_parse_transport_nvme_unknown() {
        assert_eq!(parse_transport("nvme0n1", "fc", ""), "NVMe/fc");
    }

    #[test]
    fn test_parse_transport_nvme_empty() {
        assert_eq!(parse_transport("nvme0n1", "", ""), "NVMe");
    }

    #[test]
    fn test_parse_transport_sata_ssd() {
        assert_eq!(parse_transport("sda", "", "0"), "SATA SSD");
    }

    #[test]
    fn test_parse_transport_sata_hdd() {
        assert_eq!(parse_transport("sdb", "", "1"), "SATA HDD");
    }

    #[test]
    fn test_parse_transport_unknown_device() {
        assert_eq!(parse_transport("vda", "", ""), "block");
    }

    #[test]
    fn test_parse_temp_celsius_normal() {
        assert_eq!(parse_temp_celsius("42000"), Some(42.0));
    }

    #[test]
    fn test_parse_temp_celsius_with_whitespace() {
        assert_eq!(parse_temp_celsius(" 38500\n"), Some(38.5));
    }

    #[test]
    fn test_parse_temp_celsius_invalid() {
        assert_eq!(parse_temp_celsius("not_a_number"), None);
    }

    #[test]
    fn test_parse_temp_celsius_empty() {
        assert_eq!(parse_temp_celsius(""), None);
    }

    #[test]
    fn test_nvme_base_name_partition() {
        // Partitions like nvme0n1p1 aren't passed to this function —
        // dio filters to whole-disk devices. But if they were, the
        // trim only handles the namespace suffix (n1), not partitions.
        assert_eq!(nvme_base_name("nvme0n1p1"), "nvme0n1p");
    }

    #[test]
    fn test_nvme_base_name_namespace() {
        assert_eq!(nvme_base_name("nvme0n1"), "nvme0");
    }

    #[test]
    fn test_nvme_base_name_bare() {
        assert_eq!(nvme_base_name("nvme0"), "nvme0");
    }

    #[test]
    fn test_parse_disk_hwinfo_full() {
        let info = parse_disk_hwinfo(
            "Samsung 990 Pro",
            "2000409264",
            "nvme0n1",
            "pcie",
            "",
            Some("42000"),
        );
        assert_eq!(info.model, "Samsung 990 Pro");
        assert!((info.capacity_gb - 1024.2).abs() < 0.1);
        assert_eq!(info.transport, "NVMe PCIe");
        assert_eq!(info.temp_celsius, Some(42.0));
    }

    #[test]
    fn test_parse_disk_hwinfo_sata_no_temp() {
        let info = parse_disk_hwinfo(
            "WD Blue",
            "976773168",
            "sda",
            "",
            "0",
            None,
        );
        assert_eq!(info.model, "WD Blue");
        assert_eq!(info.transport, "SATA SSD");
        assert_eq!(info.temp_celsius, None);
    }
}
