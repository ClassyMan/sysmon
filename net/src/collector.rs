use std::fs;
use anyhow::Result;

#[derive(Debug, Clone, Default)]
pub struct NetSnapshot {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub rx_errors: u64,
    pub tx_errors: u64,
    pub rx_drops: u64,
    pub tx_drops: u64,
}

#[derive(Debug, Clone, Default)]
pub struct NetRates {
    pub rx_bytes_per_sec: f64,
    pub tx_bytes_per_sec: f64,
    pub rx_packets_per_sec: f64,
    pub tx_packets_per_sec: f64,
}

impl NetRates {
    pub fn from_deltas(prev: &NetSnapshot, curr: &NetSnapshot, interval_secs: f64) -> Self {
        let delta = |old: u64, new: u64| new.saturating_sub(old) as f64 / interval_secs;
        Self {
            rx_bytes_per_sec: delta(prev.rx_bytes, curr.rx_bytes),
            tx_bytes_per_sec: delta(prev.tx_bytes, curr.tx_bytes),
            rx_packets_per_sec: delta(prev.rx_packets, curr.rx_packets),
            tx_packets_per_sec: delta(prev.tx_packets, curr.tx_packets),
        }
    }
}

#[derive(Debug, Clone)]
pub struct InterfaceInfo {
    pub name: String,
    pub ip: String,
    pub speed_mbps: Option<u64>,
    pub operstate: String,
}

pub fn list_interfaces() -> Vec<InterfaceInfo> {
    let Ok(entries) = fs::read_dir("/sys/class/net") else {
        return Vec::new();
    };

    let mut ifaces: Vec<InterfaceInfo> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if name == "lo"
                || name.starts_with("veth")
                || name.starts_with("br-")
                || name.starts_with("docker")
            {
                return None;
            }

            let operstate = read_trimmed(&format!("/sys/class/net/{}/operstate", name))
                .unwrap_or_default();

            let speed_mbps = read_trimmed(&format!("/sys/class/net/{}/speed", name))
                .and_then(|s| s.parse::<i64>().ok())
                .filter(|&s| s > 0)
                .map(|s| s as u64);

            let ip = read_ip(&name);

            Some(InterfaceInfo {
                name,
                ip,
                speed_mbps,
                operstate,
            })
        })
        .collect();

    // Sort: physical interfaces with IPs first, then virtual/down interfaces
    ifaces.sort_by(|iface_a, iface_b| {
        let score = |iface: &InterfaceInfo| -> u8 {
            let is_physical = iface.name.starts_with("enp")
                || iface.name.starts_with("eth")
                || iface.name.starts_with("wlp")
                || iface.name.starts_with("wlan");
            let has_ip = !iface.ip.is_empty();
            let is_up = iface.operstate == "up";
            match (is_physical, has_ip, is_up) {
                (true, true, true) => 0,
                (true, true, false) => 1,
                (false, true, true) => 2,
                (false, true, false) => 3,
                (_, false, _) => 4,
            }
        };
        score(iface_a).cmp(&score(iface_b))
    });

    ifaces
}

pub fn read_net_snapshot(interface: &str) -> Result<NetSnapshot> {
    let content = fs::read_to_string("/proc/net/dev")?;
    parse_net_dev(&content, interface)
}

fn parse_net_dev(content: &str, interface: &str) -> Result<NetSnapshot> {
    for line in content.lines().skip(2) {
        let line = line.trim();
        let Some((iface, rest)) = line.split_once(':') else {
            continue;
        };
        if iface.trim() != interface {
            continue;
        }

        let fields: Vec<u64> = rest
            .split_whitespace()
            .filter_map(|f| f.parse().ok())
            .collect();

        if fields.len() < 16 {
            continue;
        }

        return Ok(NetSnapshot {
            rx_bytes: fields[0],
            rx_packets: fields[1],
            rx_errors: fields[2],
            rx_drops: fields[3],
            tx_bytes: fields[8],
            tx_packets: fields[9],
            tx_errors: fields[10],
            tx_drops: fields[11],
        });
    }

    anyhow::bail!("interface {} not found in /proc/net/dev", interface)
}

fn read_ip(interface: &str) -> String {
    let output = std::process::Command::new("ip")
        .args(["-4", "-brief", "addr", "show", interface])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            text.split_whitespace()
                .nth(2)
                .unwrap_or("")
                .split('/')
                .next()
                .unwrap_or("")
                .to_string()
        }
        _ => String::new(),
    }
}

fn read_trimmed(path: &str) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

pub fn human_rate(bytes_per_sec: f64) -> String {
    if bytes_per_sec >= 1_000_000_000.0 {
        format!("{:.1} GB/s", bytes_per_sec / 1_000_000_000.0)
    } else if bytes_per_sec >= 1_000_000.0 {
        format!("{:.1} MB/s", bytes_per_sec / 1_000_000.0)
    } else if bytes_per_sec >= 1_000.0 {
        format!("{:.1} KB/s", bytes_per_sec / 1_000.0)
    } else {
        format!("{:.0} B/s", bytes_per_sec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROC_NET_DEV: &str = "\
Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo: 6820891   49907    0    0    0     0          0         0  6820891   49907    0    0    0     0       0          0
enp5s0: 1433819608 1429417    0    0    0     0          0      9146 743965975  767927    0    0    0     0       0          0
wlp4s0: 981325664 7940576    0    0    0     0          0         0 137260438  147874    0    0    0     0       0          0";

    #[test]
    fn test_parse_net_dev_known_interface() {
        let snap = parse_net_dev(PROC_NET_DEV, "enp5s0").unwrap();
        assert_eq!(snap.rx_bytes, 1433819608);
        assert_eq!(snap.tx_bytes, 743965975);
        assert_eq!(snap.rx_packets, 1429417);
        assert_eq!(snap.tx_packets, 767927);
    }

    #[test]
    fn test_parse_net_dev_missing_interface() {
        let result = parse_net_dev(PROC_NET_DEV, "eth99");
        assert!(result.is_err());
    }

    #[test]
    fn test_net_rates_from_deltas() {
        let prev = NetSnapshot { rx_bytes: 1000, tx_bytes: 500, ..Default::default() };
        let curr = NetSnapshot { rx_bytes: 2000, tx_bytes: 1500, ..Default::default() };
        let rates = NetRates::from_deltas(&prev, &curr, 1.0);
        assert!((rates.rx_bytes_per_sec - 1000.0).abs() < 0.01);
        assert!((rates.tx_bytes_per_sec - 1000.0).abs() < 0.01);
    }

    #[test]
    fn test_human_rate_formatting() {
        assert_eq!(human_rate(500.0), "500 B/s");
        assert_eq!(human_rate(1500.0), "1.5 KB/s");
        assert_eq!(human_rate(2_500_000.0), "2.5 MB/s");
        assert_eq!(human_rate(1_500_000_000.0), "1.5 GB/s");
    }
}
