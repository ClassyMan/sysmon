use std::time::Instant;

const DECAY_SECS: u64 = 60;

/// Tracks a Y-axis maximum that expands instantly on new peaks
/// but only contracts after a sustained period of lower values.
pub struct StickyMax {
    displayed: f64,
    last_peak: Instant,
}

impl StickyMax {
    pub fn new() -> Self {
        Self {
            displayed: 0.0,
            last_peak: Instant::now(),
        }
    }

    pub fn update(&mut self, current_max: f64) -> f64 {
        if current_max >= self.displayed {
            self.displayed = current_max;
            self.last_peak = Instant::now();
        } else if self.last_peak.elapsed().as_secs() >= DECAY_SECS {
            self.displayed = current_max;
            self.last_peak = Instant::now();
        }
        self.displayed
    }

    pub fn current(&self) -> f64 {
        self.displayed
    }

    pub fn reset(&mut self) {
        self.displayed = 0.0;
        self.last_peak = Instant::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ratchets_up() {
        let mut sm = StickyMax::new();
        assert_eq!(sm.update(10.0), 10.0);
        assert_eq!(sm.update(50.0), 50.0);
        assert_eq!(sm.update(30.0), 50.0); // stays at peak
    }

    #[test]
    fn test_does_not_decay_immediately() {
        let mut sm = StickyMax::new();
        sm.update(100.0);
        assert_eq!(sm.update(1.0), 100.0);
        assert_eq!(sm.update(5.0), 100.0);
    }

    #[test]
    fn test_reset_clears_state() {
        let mut sticky_max = StickyMax::new();
        sticky_max.update(75.0);
        assert_eq!(sticky_max.current(), 75.0);

        sticky_max.reset();
        assert_eq!(sticky_max.current(), 0.0);
    }

    #[test]
    fn test_current_returns_displayed_value() {
        let mut sticky_max = StickyMax::new();
        assert_eq!(sticky_max.current(), 0.0);

        sticky_max.update(42.0);
        assert_eq!(sticky_max.current(), 42.0);

        sticky_max.update(10.0);
        assert_eq!(sticky_max.current(), 42.0);
    }

    #[test]
    fn test_update_with_zero() {
        let mut sticky_max = StickyMax::new();
        let result = sticky_max.update(0.0);
        assert_eq!(result, 0.0);
        assert_eq!(sticky_max.current(), 0.0);
    }

    #[test]
    fn test_multiple_peaks_takes_highest() {
        let mut sticky_max = StickyMax::new();
        sticky_max.update(20.0);
        sticky_max.update(80.0);
        sticky_max.update(50.0);
        sticky_max.update(90.0);
        sticky_max.update(60.0);

        assert_eq!(sticky_max.current(), 90.0);
    }
}
