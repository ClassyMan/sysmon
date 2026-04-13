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
        assert_eq!(sm.update(1.0), 100.0); // still 100
        assert_eq!(sm.update(5.0), 100.0); // still 100
    }
}
