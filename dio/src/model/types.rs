/// Formats a bytes-per-second value into a human-readable string.
///
/// Examples: "0 B/s", "1.0 KB/s", "2.3 MB/s", "1.1 GB/s"
pub fn human_bytes(bytes_per_sec: f64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;

    if bytes_per_sec < KB {
        format!("{:.0} B/s", bytes_per_sec)
    } else if bytes_per_sec < MB {
        format!("{:.1} KB/s", bytes_per_sec / KB)
    } else if bytes_per_sec < GB {
        format!("{:.1} MB/s", bytes_per_sec / MB)
    } else {
        format!("{:.1} GB/s", bytes_per_sec / GB)
    }
}

/// Formats an IOPS value into a human-readable string.
///
/// Examples: "0", "512", "1.2K", "45.3K"
pub fn human_iops(iops: f64) -> String {
    if iops < 1000.0 {
        format!("{:.0}", iops)
    } else if iops < 1_000_000.0 {
        format!("{:.1}K", iops / 1000.0)
    } else {
        format!("{:.1}M", iops / 1_000_000.0)
    }
}

/// Rounds a value up to a "nice" chart axis boundary.
///
/// Produces values in a 1-2-5 sequence scaled by powers of 10.
/// Examples: 173 -> 200, 0.8 -> 1.0, 5200 -> 10000, 0 -> 1.0
pub fn nice_ceil(value: f64) -> f64 {
    if value <= 0.0 {
        return 1.0;
    }

    let exponent = value.log10().floor();
    let fraction = value / 10.0_f64.powf(exponent);
    let nice_fraction = if fraction <= 1.0 {
        1.0
    } else if fraction <= 2.0 {
        2.0
    } else if fraction <= 5.0 {
        5.0
    } else {
        10.0
    };

    nice_fraction * 10.0_f64.powf(exponent)
}

/// Formats a latency in milliseconds to a human-readable string.
///
/// /proc/diskstats reports time in whole milliseconds, so sub-ms values
/// are rounding artifacts. We display at ms precision, not us.
pub fn human_latency(latency_ms: f64) -> String {
    if latency_ms < 1000.0 {
        format!("{:.1}ms", latency_ms)
    } else {
        format!("{:.1}s", latency_ms / 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_human_bytes() {
        assert_eq!(human_bytes(0.0), "0 B/s");
        assert_eq!(human_bytes(512.0), "512 B/s");
        assert_eq!(human_bytes(1024.0), "1.0 KB/s");
        assert_eq!(human_bytes(1_500_000.0), "1.4 MB/s");
        assert_eq!(human_bytes(2_500_000_000.0), "2.3 GB/s");
    }

    #[test]
    fn test_human_iops() {
        assert_eq!(human_iops(0.0), "0");
        assert_eq!(human_iops(512.0), "512");
        assert_eq!(human_iops(1200.0), "1.2K");
        assert_eq!(human_iops(45_300.0), "45.3K");
    }

    #[test]
    fn test_nice_ceil() {
        assert_eq!(nice_ceil(0.0), 1.0);
        assert_eq!(nice_ceil(0.8), 1.0);
        assert_eq!(nice_ceil(1.0), 1.0);
        assert_eq!(nice_ceil(1.5), 2.0);
        assert_eq!(nice_ceil(3.0), 5.0);
        assert_eq!(nice_ceil(7.0), 10.0);
        assert_eq!(nice_ceil(173.0), 200.0);
        assert_eq!(nice_ceil(5200.0), 10000.0);
    }

    #[test]
    fn test_human_latency() {
        assert_eq!(human_latency(0.5), "0.5ms");
        assert_eq!(human_latency(1.0), "1.0ms");
        assert_eq!(human_latency(15.3), "15.3ms");
        assert_eq!(human_latency(1500.0), "1.5s");
    }
}
