use std::path::Path;

/// Returns true if this device should be shown by default.
///
/// Filters out loop devices, ram disks, and devices that have never
/// seen any I/O. Keeps dm-* devices (common on encrypted systems).
pub fn should_track(name: &str, reads: u64, writes: u64, show_all: bool) -> bool {
    if show_all {
        return reads > 0 || writes > 0;
    }

    if name.starts_with("loop") || name.starts_with("ram") {
        return false;
    }

    if reads == 0 && writes == 0 {
        return false;
    }

    true
}

/// Returns true if the device is a whole disk (not a partition).
///
/// Whole disks have a directory at /sys/block/<name>.
/// Partitions live under /sys/block/<parent>/<name>.
pub fn is_whole_disk(name: &str) -> bool {
    Path::new(&format!("/sys/block/{}", name)).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filters_loop_devices() {
        assert!(!should_track("loop0", 100, 50, false));
        assert!(!should_track("loop15", 100, 50, false));
    }

    #[test]
    fn test_filters_ram_devices() {
        assert!(!should_track("ram0", 100, 50, false));
    }

    #[test]
    fn test_filters_zero_io() {
        assert!(!should_track("sda", 0, 0, false));
    }

    #[test]
    fn test_keeps_real_devices() {
        assert!(should_track("sda", 100, 50, false));
        assert!(should_track("nvme0n1", 100, 50, false));
        assert!(should_track("dm-0", 100, 50, false));
    }

    #[test]
    fn test_show_all_includes_loops_with_io() {
        assert!(should_track("loop0", 100, 50, true));
        assert!(!should_track("loop0", 0, 0, true));
    }
}
