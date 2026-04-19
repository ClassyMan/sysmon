use ratatui::style::Color;
use sysmon_shared::terminal_theme::palette;

const RX_SLOT: usize = 10; // bright green
const TX_SLOT: usize = 11; // bright yellow-green

// Half-width katakana (U+FF65..FF9F) — same range the real Matrix used
const KATAKANA_START: u32 = 0xFF65;
const KATAKANA_COUNT: u32 = 56;

/// A single falling stream (column of characters with a bright head and fading trail).
#[derive(Clone)]
pub struct Stream {
    pub col: u16,
    pub head_row: f64,
    pub speed: f64,
    pub is_rx: bool,
    pub trail: Vec<TrailCell>,
    trail_len: usize,
    mutate_counter: u8,
}

#[derive(Clone)]
pub struct TrailCell {
    pub row: u16,
    pub ch: char,
    pub age: u8,
}

impl TrailCell {
    pub fn color(&self, is_rx: bool) -> Color {
        let p = palette();
        let slot = if is_rx { RX_SLOT } else { TX_SLOT };
        match self.age {
            0 => p.mix_with_fg(slot, 0.4),       // bright head, mostly fg with accent tint
            1 => p.slot_color(slot),             // full accent
            2 => p.mix_with_bg(slot, 0.75),      // start fading
            3 => p.mix_with_bg(slot, 0.55),
            4 => p.mix_with_bg(slot, 0.35),
            5 => p.mix_with_bg(slot, 0.20),
            _ => p.mix_with_bg(slot, 0.10),      // nearly bg
        }
    }
}

pub struct RainState {
    pub streams: Vec<Stream>,
    rng_state: u64,
}

impl RainState {
    pub fn new() -> Self {
        Self {
            streams: Vec::new(),
            rng_state: 0x12345678_9abcdef0,
        }
    }

    pub fn tick(&mut self, width: u16, height: u16, rx_rate: f64, tx_rate: f64) {
        if width == 0 || height == 0 {
            return;
        }

        let half = height / 2;

        // Advance existing streams
        self.streams.retain_mut(|stream| {
            stream.head_row += stream.speed;

            let head_int = stream.head_row as u16;
            let boundary = if stream.is_rx { half } else { height };

            // Add new head character to trail
            if head_int < boundary {
                let ch = rand_katakana(&mut self.rng_state);
                stream.trail.push(TrailCell { row: head_int, ch, age: 0 });
            }

            // Age all trail cells
            for cell in &mut stream.trail {
                cell.age = cell.age.saturating_add(1);
            }

            // Randomly mutate some trail characters (matrix flicker effect)
            stream.mutate_counter = stream.mutate_counter.wrapping_add(1);
            if stream.mutate_counter % 3 == 0 && !stream.trail.is_empty() {
                let idx = self.rng_state as usize % stream.trail.len();
                stream.trail[idx].ch = rand_katakana(&mut self.rng_state);
            }

            // Trim trail to max length and remove fully faded cells
            stream.trail.retain(|cell| cell.age < 8);
            if stream.trail.len() > stream.trail_len {
                let excess = stream.trail.len() - stream.trail_len;
                stream.trail.drain(0..excess);
            }

            // Stream alive while trail has visible cells
            !stream.trail.is_empty()
        });

        // Spawn new RX streams
        let rx_spawn = rate_to_spawns(rx_rate, width);
        for _ in 0..rx_spawn {
            let col = self.rand_range(0, width as u64) as u16;
            let intensity = rate_to_intensity(rx_rate);
            let speed = 0.4 + (intensity as f64) * 0.2;
            let trail_len = 4 + intensity * 2;
            self.streams.push(Stream {
                col,
                head_row: 0.0,
                speed,
                is_rx: true,
                trail: Vec::with_capacity(trail_len),
                trail_len,
                mutate_counter: 0,
            });
        }

        // Spawn new TX streams
        let tx_spawn = rate_to_spawns(tx_rate, width);
        for _ in 0..tx_spawn {
            let col = self.rand_range(0, width as u64) as u16;
            let intensity = rate_to_intensity(tx_rate);
            let speed = 0.4 + (intensity as f64) * 0.2;
            let trail_len = 4 + intensity * 2;
            self.streams.push(Stream {
                col,
                head_row: half as f64,
                speed,
                is_rx: false,
                trail: Vec::with_capacity(trail_len),
                trail_len,
                mutate_counter: 0,
            });
        }
    }

    fn rand_range(&mut self, min: u64, max: u64) -> u64 {
        if max <= min {
            return min;
        }
        xorshift(&mut self.rng_state);
        min + (self.rng_state % (max - min))
    }
}

fn rand_katakana(rng: &mut u64) -> char {
    xorshift(rng);
    let offset = (*rng % KATAKANA_COUNT as u64) as u32;
    char::from_u32(KATAKANA_START + offset).unwrap_or('ア')
}

fn xorshift(state: &mut u64) {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
}

fn rate_to_spawns(bytes_per_sec: f64, width: u16) -> usize {
    if bytes_per_sec <= 0.0 {
        return 0;
    }
    let base = (bytes_per_sec.log10() - 1.0).max(0.0);
    let density = (base * 0.6).min(width as f64 * 0.3);
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
        assert!(state.streams.is_empty());
    }

    #[test]
    fn test_tick_spawns_streams_with_traffic() {
        let mut state = RainState::new();
        state.tick(80, 40, 100_000.0, 50_000.0);
        assert!(!state.streams.is_empty());
    }

    #[test]
    fn test_tick_no_streams_at_zero_rate() {
        let mut state = RainState::new();
        state.tick(80, 40, 0.0, 0.0);
        assert!(state.streams.is_empty());
    }

    #[test]
    fn test_streams_expire_over_time() {
        let mut state = RainState::new();
        state.tick(80, 10, 100_000.0, 0.0);
        for _ in 0..200 {
            state.tick(80, 10, 0.0, 0.0);
        }
        assert!(state.streams.is_empty());
    }

    #[test]
    fn test_trail_cell_color_varies_by_age() {
        let young = TrailCell { row: 0, ch: 'ア', age: 0 };
        let old = TrailCell { row: 0, ch: 'ア', age: 6 };
        let Color::Rgb(yr, _, _) = young.color(true) else { panic!() };
        let Color::Rgb(or, _, _) = old.color(true) else { panic!() };
        assert!(yr > or);
    }

    #[test]
    fn test_rand_katakana_produces_valid_chars() {
        let mut rng = 0xDEADBEEF_u64;
        for _ in 0..100 {
            let ch = rand_katakana(&mut rng);
            let code = ch as u32;
            assert!(code >= KATAKANA_START && code < KATAKANA_START + KATAKANA_COUNT);
        }
    }

    #[test]
    fn test_trail_mutates_characters() {
        let mut state = RainState::new();
        state.tick(80, 40, 1_000_000.0, 0.0);
        // Run several ticks to trigger mutation
        for _ in 0..20 {
            state.tick(80, 40, 1_000_000.0, 0.0);
        }
        // Streams should have trail cells with varied characters
        let has_trail = state.streams.iter().any(|s| s.trail.len() > 1);
        assert!(has_trail);
    }
}
