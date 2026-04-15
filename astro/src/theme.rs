use std::fs;
use std::path::PathBuf;

/// Terminal color palette for themed image rendering.
/// Uses all 8 ANSI colors (0-7) plus background to cover the full hue range.
#[derive(Clone, Debug)]
pub struct ThemePalette {
    pub bg: [u8; 3],
    pub colors: [[u8; 3]; 8], // ANSI colors 0-7
}

impl Default for ThemePalette {
    fn default() -> Self {
        // Catppuccin Mocha fallback
        Self {
            bg: [30, 30, 46],
            colors: [
                [69, 71, 90],    // color0 - surface
                [243, 139, 168], // color1 - red
                [166, 227, 161], // color2 - green
                [249, 226, 175], // color3 - yellow
                [137, 180, 250], // color4 - blue
                [245, 194, 231], // color5 - pink
                [148, 226, 213], // color6 - teal
                [186, 194, 222], // color7 - text
            ],
        }
    }
}

impl ThemePalette {
    /// Split-tone color grading: preserves the image's contrast and color
    /// variation while shifting hues toward the theme's palette.
    ///
    /// Technique from film color grading:
    /// - Shadows get rotated toward the dark theme hue (e.g. purple)
    /// - Highlights get rotated toward the bright theme hue (e.g. teal)
    /// - Saturation is preserved and boosted slightly
    /// - Lightness is remapped to the theme's luminance range
    ///
    /// The hue targets are derived from the theme's own colors, so this
    /// adapts automatically to any terminal theme.
    /// Color-grade a pixel into the terminal's palette.
    ///
    /// For each pixel:
    /// 1. Find the nearest theme color by hue (preserves warm/cool character)
    /// 2. For grayscale pixels, pick a theme color based on brightness
    /// 3. Shift the hue toward the matched theme color
    /// 4. Remap lightness to the theme's luminance range
    /// 5. Inject saturation into desaturated areas, preserve it in colorful ones
    pub fn blend(&self, rgb: [u8; 3]) -> [u8; 3] {
        let (h, s, l) = rgb_to_hsl(rgb);

        // Remap lightness to theme's luminance range
        let bg_l = rgb_to_hsl(self.bg).2;
        let max_l = self.colors.iter()
            .map(|c| rgb_to_hsl(*c).2)
            .fold(0.0_f64, f64::max);
        let new_l = bg_l + l * (max_l - bg_l);

        // Chromatic theme colors (skip color0/color7 which are near-grey)
        let chromatic: Vec<(f64, f64)> = self.colors[1..7]
            .iter()
            .map(|c| {
                let (ch, cs, _) = rgb_to_hsl(*c);
                (ch, cs)
            })
            .collect();

        // Grayscale strategy: pick theme color by brightness for variety
        let idx = ((l * (chromatic.len() as f64 - 0.01)) as usize)
            .min(chromatic.len() - 1);
        let (gray_hue, gray_theme_sat) = chromatic[idx];
        let gray_result = hsl_to_rgb(gray_hue, gray_theme_sat * 0.6, new_l);

        // Color strategy: match to nearest theme color by hue
        let (nearest_hue, nearest_sat) = chromatic
            .iter()
            .min_by(|a, b| {
                hue_distance(h, a.0)
                    .partial_cmp(&hue_distance(h, b.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .copied()
            .unwrap_or((h, s));
        let color_hue = lerp_angle(nearest_hue, h, 0.4);
        let color_s = (s + (nearest_sat * 0.7 - s) * (1.0 - s)).min(1.0);
        let color_result = hsl_to_rgb(color_hue, color_s, new_l);

        // Smooth crossfade: s < 0.15 → mostly grayscale treatment,
        // s > 0.35 → mostly color treatment. No hard cutoff.
        let blend = ((s - 0.15) / 0.20).clamp(0.0, 1.0);
        lerp_rgb(gray_result, color_result, blend)
    }
}

fn rgb_to_hsl(rgb: [u8; 3]) -> (f64, f64, f64) {
    let r = rgb[0] as f64 / 255.0;
    let g = rgb[1] as f64 / 255.0;
    let b = rgb[2] as f64 / 255.0;

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;

    if (max - min).abs() < 1e-10 {
        return (0.0, 0.0, l);
    }

    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };

    let h = if (max - r).abs() < 1e-10 {
        let mut h = (g - b) / d;
        if g < b {
            h += 6.0;
        }
        h
    } else if (max - g).abs() < 1e-10 {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };

    (h * 60.0, s, l)
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> [u8; 3] {
    if s.abs() < 1e-10 {
        let v = (l * 255.0) as u8;
        return [v, v, v];
    }

    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let h = h / 360.0;

    let hue_to_rgb = |t: f64| -> f64 {
        let t = ((t % 1.0) + 1.0) % 1.0;
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 1.0 / 2.0 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };

    [
        (hue_to_rgb(h + 1.0 / 3.0) * 255.0) as u8,
        (hue_to_rgb(h) * 255.0) as u8,
        (hue_to_rgb(h - 1.0 / 3.0) * 255.0) as u8,
    ]
}

fn hue_distance(a: f64, b: f64) -> f64 {
    let d = (a - b).abs();
    if d > 180.0 { 360.0 - d } else { d }
}

fn lerp_rgb(a: [u8; 3], b: [u8; 3], t: f64) -> [u8; 3] {
    [
        (a[0] as f64 * (1.0 - t) + b[0] as f64 * t) as u8,
        (a[1] as f64 * (1.0 - t) + b[1] as f64 * t) as u8,
        (a[2] as f64 * (1.0 - t) + b[2] as f64 * t) as u8,
    ]
}

/// Interpolate between two angles (in degrees) on the hue circle,
/// always taking the shortest path around the circle.
fn lerp_angle(a: f64, b: f64, t: f64) -> f64 {
    let mut diff = b - a;
    if diff > 180.0 {
        diff -= 360.0;
    } else if diff < -180.0 {
        diff += 360.0;
    }
    ((a + diff * t) % 360.0 + 360.0) % 360.0
}

fn parse_hex(hex: &str) -> Option<[u8; 3]> {
    let hex = hex.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some([r, g, b])
}

/// Detect the terminal color palette from kitty config files.
///
/// Checks in order:
/// 1. ~/.config/kitty/current-theme.conf (kitty theme override)
/// 2. ~/.config/kitty/kitty.conf (main config, may include theme)
///
/// Extracts ANSI colors 0, 4, 6, 7 plus background/foreground.
/// These slots have consistent semantic meaning across themes:
///   color0 = dark surface, color4 = blue, color6 = cyan/teal, color7 = light
pub fn detect() -> ThemePalette {
    let home = match std::env::var("HOME") {
        Ok(h) => PathBuf::from(h),
        Err(_) => return ThemePalette::default(),
    };

    let config_dir = std::env::var("KITTY_CONF_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home.join(".config").join("kitty"));

    let mut colors = std::collections::HashMap::new();

    // Read theme file first (higher priority), then main config
    for filename in &["current-theme.conf", "kitty.conf"] {
        let path = config_dir.join(filename);
        if let Ok(content) = fs::read_to_string(&path) {
            for line in content.lines() {
                let line = line.trim();
                if line.starts_with('#') || line.is_empty() {
                    continue;
                }
                let parts: Vec<&str> = line.splitn(2, char::is_whitespace).collect();
                if parts.len() == 2 {
                    let key = parts[0].trim();
                    let val = parts[1].trim();
                    // Only set if not already set (theme file takes priority)
                    colors.entry(key.to_string()).or_insert_with(|| val.to_string());
                }
            }
        }
    }

    let get = |key: &str| -> Option<[u8; 3]> {
        colors.get(key).and_then(|v| parse_hex(v))
    };

    let defaults = ThemePalette::default();
    let mut palette_colors = defaults.colors;
    for i in 0..8 {
        if let Some(c) = get(&format!("color{i}")) {
            palette_colors[i] = c;
        }
    }
    ThemePalette {
        bg: get("background").unwrap_or(defaults.bg),
        colors: palette_colors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_valid() {
        assert_eq!(parse_hex("#1E1E2E"), Some([30, 30, 46]));
    }

    #[test]
    fn test_parse_hex_no_hash() {
        assert_eq!(parse_hex("89B4FA"), Some([137, 180, 250]));
    }

    #[test]
    fn test_parse_hex_invalid() {
        assert_eq!(parse_hex("nope"), None);
        assert_eq!(parse_hex("#GG0000"), None);
        assert_eq!(parse_hex(""), None);
    }

    #[test]
    fn test_blend_dark_is_dark() {
        let palette = ThemePalette::default();
        let result = palette.blend([0, 0, 0]);
        // Should be near theme background lightness
        let brightness = (result[0] as u16 + result[1] as u16 + result[2] as u16) / 3;
        assert!(brightness < 50);
    }

    #[test]
    fn test_blend_bright_is_bright() {
        let palette = ThemePalette::default();
        let result = palette.blend([255, 255, 255]);
        let brightness = (result[0] as u16 + result[1] as u16 + result[2] as u16) / 3;
        assert!(brightness > 100);
    }

    #[test]
    fn test_blend_preserves_relative_difference() {
        let palette = ThemePalette::default();
        let dark = palette.blend([50, 50, 50]);
        let bright = palette.blend([200, 200, 200]);
        let dark_l = (dark[0] as u16 + dark[1] as u16 + dark[2] as u16) / 3;
        let bright_l = (bright[0] as u16 + bright[1] as u16 + bright[2] as u16) / 3;
        assert!(bright_l > dark_l);
    }

    #[test]
    fn test_blend_different_inputs_give_different_outputs() {
        let palette = ThemePalette::default();
        let red = palette.blend([255, 0, 0]);
        let blue = palette.blend([0, 0, 255]);
        // Different hues in → different outputs (not flat)
        assert_ne!(red, blue);
    }

    #[test]
    fn test_rgb_hsl_roundtrip() {
        let original = [137, 180, 250];
        let (h, s, l) = rgb_to_hsl(original);
        let back = hsl_to_rgb(h, s, l);
        assert!((original[0] as i16 - back[0] as i16).abs() <= 1);
        assert!((original[1] as i16 - back[1] as i16).abs() <= 1);
        assert!((original[2] as i16 - back[2] as i16).abs() <= 1);
    }

    #[test]
    fn test_lerp_angle_shortest_path() {
        assert!((lerp_angle(350.0, 10.0, 0.5) - 0.0).abs() < 1.0);
        assert!((lerp_angle(10.0, 350.0, 0.5) - 0.0).abs() < 1.0);
    }

    #[test]
    fn test_default_is_catppuccin_mocha() {
        let p = ThemePalette::default();
        assert_eq!(p.bg, [30, 30, 46]);
        assert_eq!(p.colors[4], [137, 180, 250]); // blue
        assert_eq!(p.colors[1], [243, 139, 168]); // red
        assert_eq!(p.colors[6], [148, 226, 213]); // teal
    }
}
