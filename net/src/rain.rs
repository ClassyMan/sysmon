use ratatui::style::Color;

const RX_CHARS: [char; 6] = ['·', ':', '╎', '┊', '┆', '│'];
const TX_CHARS: [char; 6] = ['·', ':', '╎', '┊', '┆', '│'];

const RX_COLORS: [Color; 5] = [
    Color::Rgb(30, 60, 90),
    Color::Rgb(50, 100, 150),
    Color::Rgb(80, 160, 220),
    Color::Rgb(120, 200, 255),
    Color::Rgb(200, 240, 255),
];

const TX_COLORS: [Color; 5] = [
    Color::Rgb(90, 60, 20),
    Color::Rgb(150, 100, 30),
    Color::Rgb(220, 150, 50),
    Color::Rgb(255, 190, 80),
    Color::Rgb(255, 230, 170),
];

#[derive(Clone)]
pub struct Drop {
    pub col: u16,
    pub row_frac: f64,
    pub speed: f64,
    pub intensity: usize,
    pub is_rx: bool,
}

impl Drop {
    pub fn char(&self) -> char {
        let chars = if self.is_rx { &RX_CHARS } else { &TX_CHARS };
        chars[self.intensity.min(chars.len() - 1)]
    }

    pub fn color(&self) -> Color {
        let colors = if self.is_rx { &RX_COLORS } else { &TX_COLORS };
        colors[self.intensity.min(colors.len() - 1)]
    }

    pub fn row(&self) -> u16 {
        self.row_frac as u16
    }
}

pub struct RainState {
    pub drops: Vec<Drop>,
    rng_state: u64,
}

impl RainState {
    pub fn new() -> Self {
        Self {
            drops: Vec::new(),
            rng_state: 0x12345678_9abcdef0,
        }
    }

    pub fn tick(&mut self, width: u16, height: u16, rx_rate: f64, tx_rate: f64) {
        if width == 0 || height == 0 {
            return;
        }

        let half = height / 2;

        // Move existing drops
        self.drops.retain_mut(|drop| {
            drop.row_frac += drop.speed;
            if drop.is_rx {
                drop.row_frac < half as f64
            } else {
                drop.row() < height
            }
        });

        // Spawn new RX drops (fall from top of upper half)
        let rx_drop_count = rate_to_drops(rx_rate, width);
        for _ in 0..rx_drop_count {
            let col = self.rand_range(0, width as u64) as u16;
            let intensity = rate_to_intensity(rx_rate);
            let speed = 0.3 + (intensity as f64) * 0.15;
            self.drops.push(Drop {
                col,
                row_frac: 0.0,
                speed,
                intensity,
                is_rx: true,
            });
        }

        // Spawn new TX drops (fall from top of lower half)
        let tx_drop_count = rate_to_drops(tx_rate, width);
        for _ in 0..tx_drop_count {
            let col = self.rand_range(0, width as u64) as u16;
            let intensity = rate_to_intensity(tx_rate);
            let speed = 0.3 + (intensity as f64) * 0.15;
            self.drops.push(Drop {
                col,
                row_frac: half as f64,
                speed,
                intensity,
                is_rx: false,
            });
        }
    }

    fn rand_range(&mut self, min: u64, max: u64) -> u64 {
        if max <= min {
            return min;
        }
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 7;
        self.rng_state ^= self.rng_state << 17;
        min + (self.rng_state % (max - min))
    }
}

fn rate_to_drops(bytes_per_sec: f64, width: u16) -> usize {
    if bytes_per_sec <= 0.0 {
        return 0;
    }
    let base = (bytes_per_sec.log10() - 1.0).max(0.0);
    let density = (base * 0.8).min(width as f64 * 0.4);
    density.round() as usize
}

fn rate_to_intensity(bytes_per_sec: f64) -> usize {
    if bytes_per_sec <= 0.0 {
        return 0;
    }
    let level = (bytes_per_sec.log10() - 1.0).max(0.0);
    (level as usize).min(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rain_state_new() {
        let state = RainState::new();
        assert!(state.drops.is_empty());
    }

    #[test]
    fn test_tick_spawns_drops_with_traffic() {
        let mut state = RainState::new();
        state.tick(80, 40, 100_000.0, 50_000.0);
        assert!(!state.drops.is_empty());
    }

    #[test]
    fn test_tick_no_drops_at_zero_rate() {
        let mut state = RainState::new();
        state.tick(80, 40, 0.0, 0.0);
        assert!(state.drops.is_empty());
    }

    #[test]
    fn test_drops_expire_past_boundary() {
        let mut state = RainState::new();
        state.tick(80, 10, 100_000.0, 0.0);
        for _ in 0..100 {
            state.tick(80, 10, 0.0, 0.0);
        }
        assert!(state.drops.is_empty());
    }

    #[test]
    fn test_drop_char_and_color() {
        let drop = Drop {
            col: 0,
            row_frac: 5.0,
            speed: 1.0,
            intensity: 2,
            is_rx: true,
        };
        assert_eq!(drop.char(), '╎');
        assert_eq!(drop.row(), 5);
        assert!(matches!(drop.color(), Color::Rgb(_, _, _)));
    }

    #[test]
    fn test_rate_to_intensity_scaling() {
        assert_eq!(rate_to_intensity(0.0), 0);
        assert_eq!(rate_to_intensity(10.0), 0);
        assert!(rate_to_intensity(1_000_000.0) >= 3);
    }
}
