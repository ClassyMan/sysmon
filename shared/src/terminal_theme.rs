//! Resolve the host terminal's palette. Tries, in order:
//!
//! 1. **Config-file parsing** — detects Ghostty / Kitty from env and reads
//!    their config files directly. This is the most accurate source because
//!    we get exactly the 16 colors the theme defines, including bg/fg.
//! 2. **OSC 11 / OSC 4 query** — fallback for other terminals. Requires raw
//!    mode. Some terminals (notably Ghostty) don't respond to OSC 4 for all
//!    slots, which produces a mixed palette.
//! 3. **Catppuccin Mocha default** — last resort.

use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use ratatui::style::Color;

#[derive(Clone, Debug)]
pub struct Palette {
    pub bg: [u8; 3],
    pub fg: [u8; 3],
    pub colors: [[u8; 3]; 16],
}

static GLOBAL: OnceLock<Palette> = OnceLock::new();

/// Resolve the terminal palette once and store it globally. Tries config
/// files first, then OSC query, then the Catppuccin Mocha default.
pub fn init() {
    let resolved = load_from_config().unwrap_or_else(query);
    let _ = GLOBAL.set(resolved);
}

/// Try to load the palette by parsing the terminal's config file directly.
/// Returns `None` if terminal detection fails or parsing doesn't produce a
/// usable palette.
fn load_from_config() -> Option<Palette> {
    if is_ghostty() {
        if let Some(p) = load_ghostty_palette() {
            return Some(p);
        }
    }
    if is_kitty() {
        if let Some(p) = load_kitty_palette() {
            return Some(p);
        }
    }
    None
}

fn is_ghostty() -> bool {
    std::env::var("TERM_PROGRAM").ok().as_deref() == Some("ghostty")
        || std::env::var("GHOSTTY_RESOURCES_DIR").is_ok()
}

fn is_kitty() -> bool {
    std::env::var("KITTY_WINDOW_ID").is_ok()
        || std::env::var("TERM").ok().as_deref() == Some("xterm-kitty")
}

fn home_config_dir() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg));
        }
    }
    std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".config"))
}

fn load_ghostty_palette() -> Option<Palette> {
    let config = home_config_dir()?.join("ghostty/config");
    let contents = std::fs::read_to_string(&config).ok()?;

    let theme_name = contents.lines().find_map(|line| {
        let line = line.trim();
        let rest = line.strip_prefix("theme")?.trim_start();
        let rest = rest.strip_prefix('=')?.trim();
        if rest.is_empty() { None } else { Some(rest.to_string()) }
    })?;

    let theme_text = find_ghostty_theme(&theme_name)?;
    parse_ghostty_theme(&theme_text)
}

fn find_ghostty_theme(name: &str) -> Option<String> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(cfg) = home_config_dir() {
        candidates.push(cfg.join(format!("ghostty/themes/{}", name)));
    }
    if let Ok(res) = std::env::var("GHOSTTY_RESOURCES_DIR") {
        candidates.push(PathBuf::from(res).join(format!("themes/{}", name)));
    }
    candidates.extend([
        PathBuf::from(format!("/snap/ghostty/current/share/ghostty/themes/{}", name)),
        PathBuf::from(format!("/usr/share/ghostty/themes/{}", name)),
        PathBuf::from(format!("/usr/local/share/ghostty/themes/{}", name)),
    ]);

    for path in candidates {
        if let Ok(text) = std::fs::read_to_string(&path) {
            return Some(text);
        }
    }
    None
}

fn parse_ghostty_theme(text: &str) -> Option<Palette> {
    // Same init strategy as parse_kitty_config: zero the brights so
    // synthesis can fill unset ones, keep Catppuccin basics/bg/fg as
    // fallbacks.
    let mut palette = Palette::default();
    for i in 8..16 {
        palette.colors[i] = [0, 0, 0];
    }
    let mut slots_set = 0;

    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = strip_kv(line, "palette") {
            if let Some((idx_str, color)) = rest.split_once('=') {
                if let Ok(idx) = idx_str.trim().parse::<usize>() {
                    if idx < 16 {
                        if let Some(rgb) = parse_hex_color(color.trim()) {
                            palette.colors[idx] = rgb;
                            if idx < 8 { slots_set += 1; }
                        }
                    }
                }
            }
        } else if let Some(rest) = strip_kv(line, "background") {
            if let Some(rgb) = parse_hex_color(rest) {
                palette.bg = rgb;
            }
        } else if let Some(rest) = strip_kv(line, "foreground") {
            if let Some(rgb) = parse_hex_color(rest) {
                palette.fg = rgb;
            }
        }
    }

    if slots_set >= 6 {
        palette.synthesize_brights();
        Some(palette)
    } else {
        None
    }
}

fn load_kitty_palette() -> Option<Palette> {
    let config = home_config_dir()?.join("kitty/kitty.conf");
    parse_kitty_config(&config)
}

fn parse_kitty_config(path: &Path) -> Option<Palette> {
    // Zero the brights up front so `synthesize_brights()` can fill in any
    // the theme leaves unset. Keep Catppuccin bg/fg/basics so partial
    // themes still get reasonable fallbacks for slots they don't touch.
    let mut palette = Palette::default();
    for i in 8..16 {
        palette.colors[i] = [0, 0, 0];
    }
    let mut slots_set = 0;
    let mut visited: Vec<PathBuf> = Vec::new();
    let mut stack: Vec<PathBuf> = vec![path.to_path_buf()];

    while let Some(file) = stack.pop() {
        if visited.iter().any(|v| v == &file) {
            continue;
        }
        visited.push(file.clone());

        let contents = match std::fs::read_to_string(&file) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let dir = file.parent().map(Path::to_path_buf).unwrap_or_default();
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(rest) = line.strip_prefix("include ") {
                let included = rest.trim();
                let expanded = expand_kitty_path(included, &dir);
                stack.push(expanded);
            } else if let Some((key, value)) = line.split_once(char::is_whitespace) {
                let key = key.trim();
                let value = value.trim();
                if let Some(idx_str) = key.strip_prefix("color") {
                    if let Ok(idx) = idx_str.parse::<usize>() {
                        if idx < 16 {
                            if let Some(rgb) = parse_hex_color(value) {
                                palette.colors[idx] = rgb;
                                if idx < 8 { slots_set += 1; }
                            }
                        }
                    }
                } else if key == "background" {
                    if let Some(rgb) = parse_hex_color(value) {
                        palette.bg = rgb;
                    }
                } else if key == "foreground" {
                    if let Some(rgb) = parse_hex_color(value) {
                        palette.fg = rgb;
                    }
                }
            }
        }
    }

    if slots_set >= 6 {
        palette.synthesize_brights();
        Some(palette)
    } else {
        None
    }
}

fn expand_kitty_path(raw: &str, base_dir: &Path) -> PathBuf {
    let expanded = if let Some(rest) = raw.strip_prefix("~/") {
        std::env::var("HOME")
            .map(|h| PathBuf::from(h).join(rest))
            .unwrap_or_else(|_| PathBuf::from(raw))
    } else {
        PathBuf::from(raw)
    };
    if expanded.is_absolute() {
        expanded
    } else {
        base_dir.join(expanded)
    }
}

fn strip_kv<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(key)?.trim_start();
    let rest = rest.strip_prefix('=')?.trim();
    if rest.is_empty() { None } else { Some(rest) }
}

fn parse_hex_color(s: &str) -> Option<[u8; 3]> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some([r, g, b])
}

/// Return the cached palette, falling back to the Catppuccin Mocha default
/// if `init()` was never called.
pub fn palette() -> &'static Palette {
    GLOBAL.get_or_init(Palette::default)
}

impl Palette {
    pub fn slot_color(&self, idx: usize) -> Color {
        let c = self.colors[idx.min(15)];
        Color::Rgb(c[0], c[1], c[2])
    }

    fn slot(&self, idx: usize) -> Color { self.slot_color(idx) }

    pub fn surface(&self) -> Color { self.slot(0) }       // dark gray (borders)
    pub fn red(&self) -> Color { self.slot(1) }
    pub fn green(&self) -> Color { self.slot(2) }
    pub fn yellow(&self) -> Color { self.slot(3) }
    pub fn blue(&self) -> Color { self.slot(4) }
    pub fn magenta(&self) -> Color { self.slot(5) }       // pink/magenta accent
    pub fn cyan(&self) -> Color { self.slot(6) }          // teal/cyan accent
    pub fn label(&self) -> Color { self.slot(7) }         // light gray (labels)

    pub fn bright_surface(&self) -> Color { self.slot(8) }
    pub fn bright_red(&self) -> Color { self.slot(9) }
    pub fn bright_green(&self) -> Color { self.slot(10) }
    pub fn bright_yellow(&self) -> Color { self.slot(11) }
    pub fn bright_blue(&self) -> Color { self.slot(12) }
    pub fn bright_magenta(&self) -> Color { self.slot(13) }
    pub fn bright_cyan(&self) -> Color { self.slot(14) }
    pub fn bright_label(&self) -> Color { self.slot(15) }

    pub fn bg_color(&self) -> Color { Color::Rgb(self.bg[0], self.bg[1], self.bg[2]) }
    pub fn fg_color(&self) -> Color { Color::Rgb(self.fg[0], self.fg[1], self.fg[2]) }

    pub fn muted_label(&self) -> Color { self.lerp(2, 7, 0.5) }

    /// Linear interpolation between two palette slots (raw rgb).
    pub fn lerp(&self, slot_a: usize, slot_b: usize, t: f64) -> Color {
        lerp_rgb(self.colors[slot_a.min(15)], self.colors[slot_b.min(15)], t)
    }

    /// Mix a palette slot toward the background. t=0 → background, t=1 → slot.
    pub fn mix_with_bg(&self, slot: usize, t: f64) -> Color {
        lerp_rgb(self.bg, self.colors[slot.min(15)], t)
    }

    /// Mix a palette slot toward the foreground. t=0 → foreground, t=1 → slot.
    pub fn mix_with_fg(&self, slot: usize, t: f64) -> Color {
        lerp_rgb(self.fg, self.colors[slot.min(15)], t)
    }

    /// Relative luminance of the background (0.0..=1.0, Rec.709 linear).
    pub fn bg_luminance(&self) -> f64 {
        rgb_luminance(self.bg)
    }

    /// True when the background is darker than the midline. Used to decide
    /// which direction to lift or darken synthesized bright slots.
    pub fn is_dark(&self) -> bool {
        self.bg_luminance() < 0.5
    }

    /// WCAG-style contrast ratio between two sRGB triples, clamped to
    /// [1.0, 21.0]. Provided as a utility; no current consumer uses it.
    pub fn contrast_ratio(&self, fg: [u8; 3], bg: [u8; 3]) -> f64 {
        let l1 = rgb_luminance(fg);
        let l2 = rgb_luminance(bg);
        let (hi, lo) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// True when all 8 bright slots are [0,0,0]. Diagnostic utility —
    /// `synthesize_brights` decides per-slot rather than relying on this,
    /// so production code doesn't call it. Tests use it to lock in the
    /// invariant that Alien Blood and Catppuccin default both have
    /// populated brights.
    #[cfg(test)]
    fn brights_uninitialized(&self) -> bool {
        self.colors[8..16].iter().all(|c| *c == [0, 0, 0])
    }

    /// Fill bright slots that are still [0,0,0] by lightening (dark bg) or
    /// darkening (light bg) the corresponding basic. This is a plausibility
    /// fallback — it does not reconstruct what the theme author intended.
    /// Per-slot: slots already set to non-zero values are left alone, so
    /// this is safely a no-op on a fully populated palette (Alien Blood,
    /// Catppuccin default). Accepts the trade that a theme legitimately
    /// setting a bright slot to pure black would be overwritten; no known
    /// terminal theme does so.
    fn synthesize_brights(&mut self) {
        let dark = self.is_dark();
        let anchor: [u8; 3] = if dark { [255, 255, 255] } else { [0, 0, 0] };
        let t = if dark { 0.35 } else { 0.20 };
        // Slot 8 (bright surface) follows fg rather than the anchor so
        // "bright gray" tracks the theme's text color.
        if self.colors[8] == [0, 0, 0] {
            self.colors[8] = lerp_rgb_raw(self.colors[0], self.fg, 0.30);
        }
        for i in 1..=7 {
            if self.colors[8 + i] == [0, 0, 0] {
                self.colors[8 + i] = lerp_rgb_raw(self.colors[i], anchor, t);
            }
        }
    }
}

fn lerp_rgb_raw(a: [u8; 3], b: [u8; 3], t: f64) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| ((x as f64) * (1.0 - t) + (y as f64) * t) as u8;
    [mix(a[0], b[0]), mix(a[1], b[1]), mix(a[2], b[2])]
}

fn lerp_rgb(a: [u8; 3], b: [u8; 3], t: f64) -> Color {
    let [r, g, b_] = lerp_rgb_raw(a, b, t);
    Color::Rgb(r, g, b_)
}

/// Relative luminance of an sRGB triple using Rec.709 coefficients,
/// not gamma-corrected. 0.0 = black, 1.0 = white.
///
/// Linear (not sRGB) because sysmon only needs a coarse dark/light signal;
/// the two disagree by a few percent near the midline and not at all at the
/// extremes.
pub fn rgb_luminance(rgb: [u8; 3]) -> f64 {
    let r = rgb[0] as f64 / 255.0;
    let g = rgb[1] as f64 / 255.0;
    let b = rgb[2] as f64 / 255.0;
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            bg: [30, 30, 46],
            fg: [205, 214, 244],
            colors: [
                [69, 71, 90],
                [243, 139, 168],
                [166, 227, 161],
                [249, 226, 175],
                [137, 180, 250],
                [245, 194, 231],
                [148, 226, 213],
                [186, 194, 222],
                [88, 91, 112],
                [243, 139, 168],
                [166, 227, 161],
                [249, 226, 175],
                [137, 180, 250],
                [245, 194, 231],
                [148, 226, 213],
                [205, 214, 244],
            ],
        }
    }
}

/// Query the terminal for its actual colors. Returns the default palette if
/// the terminal doesn't respond, isn't a TTY, or returns malformed data.
pub fn query() -> Palette {
    query_inner(Duration::from_millis(150)).unwrap_or_default()
}

fn query_inner(timeout: Duration) -> Option<Palette> {
    let mut stdout = std::io::stdout();
    let _ = write!(stdout, "\x1b]11;?\x1b\\");
    let _ = write!(stdout, "\x1b]10;?\x1b\\");
    for i in 0..16 {
        let _ = write!(stdout, "\x1b]4;{i};?\x1b\\");
    }
    stdout.flush().ok()?;

    let bytes = read_with_deadline(timeout)?;
    let text = std::str::from_utf8(&bytes).ok()?;
    Some(parse_responses(text))
}

fn read_with_deadline(timeout: Duration) -> Option<Vec<u8>> {
    let stdin_fd = std::io::stdin().as_raw_fd();
    let deadline = Instant::now() + timeout;
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 512];

    loop {
        let remaining = match deadline.checked_duration_since(Instant::now()) {
            Some(d) => d,
            None => break,
        };
        let mut pfd = libc::pollfd {
            fd: stdin_fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let n = unsafe { libc::poll(&mut pfd, 1, remaining.as_millis() as i32) };
        if n <= 0 {
            break;
        }
        let read =
            unsafe { libc::read(stdin_fd, chunk.as_mut_ptr() as *mut _, chunk.len()) };
        if read <= 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..read as usize]);

        // We expect 10 OSC responses (bg, fg, 8 palette). Each ends with BEL
        // or ST. Stop early once we've seen them all.
        let terminators = buf
            .iter()
            .filter(|&&b| b == 0x07 || b == b'\\')
            .count();
        if terminators >= 10 {
            break;
        }
    }

    if buf.is_empty() { None } else { Some(buf) }
}

fn parse_responses(text: &str) -> Palette {
    let mut palette = Palette::default();
    for chunk in text.split('\x1b') {
        let body = match chunk.strip_prefix(']') {
            Some(b) => b,
            None => continue,
        };
        let body = body.trim_end_matches(['\x07', '\\']);

        if let Some(rest) = body.strip_prefix("11;") {
            if let Some(rgb) = parse_rgb(rest) {
                palette.bg = rgb;
            }
        } else if let Some(rest) = body.strip_prefix("10;") {
            if let Some(rgb) = parse_rgb(rest) {
                palette.fg = rgb;
            }
        } else if let Some(rest) = body.strip_prefix("4;") {
            if let Some((idx_str, color)) = rest.split_once(';') {
                if let (Ok(idx), Some(rgb)) = (idx_str.parse::<usize>(), parse_rgb(color)) {
                    if idx < 8 {
                        palette.colors[idx] = rgb;
                    }
                }
            }
        }
    }
    palette
}

fn parse_rgb(s: &str) -> Option<[u8; 3]> {
    let s = s.trim().strip_prefix("rgb:")?;
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() != 3 {
        return None;
    }
    let parse_component = |p: &str| -> Option<u8> {
        let val = u32::from_str_radix(p, 16).ok()?;
        match p.len() {
            1 => Some((val * 0x11) as u8),
            2 => Some(val as u8),
            4 => Some((val >> 8) as u8),
            _ => None,
        }
    };
    Some([
        parse_component(parts[0])?,
        parse_component(parts[1])?,
        parse_component(parts[2])?,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rgb_4digit_hex() {
        assert_eq!(parse_rgb("rgb:1e1e/1e1e/2e2e"), Some([0x1e, 0x1e, 0x2e]));
    }

    #[test]
    fn test_parse_rgb_2digit_hex() {
        assert_eq!(parse_rgb("rgb:1e/1e/2e"), Some([0x1e, 0x1e, 0x2e]));
    }

    #[test]
    fn test_parse_rgb_invalid() {
        assert_eq!(parse_rgb("nope"), None);
        assert_eq!(parse_rgb("rgb:zz/00/00"), None);
        assert_eq!(parse_rgb("rgb:00/00"), None);
    }

    #[test]
    fn test_parse_responses_bg() {
        let input = "\x1b]11;rgb:1e1e/1e1e/2e2e\x07";
        let palette = parse_responses(input);
        assert_eq!(palette.bg, [0x1e, 0x1e, 0x2e]);
    }

    #[test]
    fn test_parse_responses_palette() {
        let input = "\x1b]4;4;rgb:8989/b4b4/fafa\x1b\\";
        let palette = parse_responses(input);
        assert_eq!(palette.colors[4], [0x89, 0xb4, 0xfa]);
    }

    #[test]
    fn test_parse_responses_full_set() {
        let mut input = String::from("\x1b]11;rgb:1e/1e/2e\x07");
        input.push_str("\x1b]10;rgb:cd/d6/f4\x07");
        for i in 0..8u8 {
            input.push_str(&format!("\x1b]4;{i};rgb:{i:02x}/{i:02x}/{i:02x}\x07"));
        }
        let palette = parse_responses(&input);
        assert_eq!(palette.bg, [0x1e, 0x1e, 0x2e]);
        assert_eq!(palette.fg, [0xcd, 0xd6, 0xf4]);
        for i in 0..8 {
            assert_eq!(palette.colors[i], [i as u8, i as u8, i as u8]);
        }
    }

    #[test]
    fn test_parse_responses_empty_returns_default() {
        let palette = parse_responses("");
        let default = Palette::default();
        assert_eq!(palette.bg, default.bg);
    }

    #[test]
    fn test_parse_ghostty_alien_blood() {
        let text = r#"palette = 0=#112616
palette = 1=#7f2b27
palette = 2=#2f7e25
palette = 3=#717f24
palette = 4=#2f6a7f
palette = 5=#47587f
palette = 6=#327f77
palette = 7=#647d75
background = #0f1610
foreground = #637d75
cursor-color = #73fa91
"#;
        let palette = parse_ghostty_theme(text).expect("should parse");
        assert_eq!(palette.bg, [0x0f, 0x16, 0x10]);
        assert_eq!(palette.fg, [0x63, 0x7d, 0x75]);
        assert_eq!(palette.colors[0], [0x11, 0x26, 0x16]);
        assert_eq!(palette.colors[1], [0x7f, 0x2b, 0x27]);
        assert_eq!(palette.colors[5], [0x47, 0x58, 0x7f]);
        assert_eq!(palette.colors[7], [0x64, 0x7d, 0x75]);
    }

    #[test]
    fn test_parse_ghostty_missing_returns_none() {
        // Only 3 slots set — below threshold
        let text = "palette = 0=#000000\npalette = 1=#111111\npalette = 2=#222222\n";
        assert!(parse_ghostty_theme(text).is_none());
    }

    #[test]
    fn test_parse_hex_color() {
        assert_eq!(parse_hex_color("#1e1e2e"), Some([0x1e, 0x1e, 0x2e]));
        assert_eq!(parse_hex_color("1e1e2e"), Some([0x1e, 0x1e, 0x2e]));
        assert_eq!(parse_hex_color("#xyzxyz"), None);
        assert_eq!(parse_hex_color("#123"), None);
    }

    #[test]
    fn test_strip_kv() {
        assert_eq!(strip_kv("theme = Alien Blood", "theme"), Some("Alien Blood"));
        assert_eq!(strip_kv("theme=Alien Blood", "theme"), Some("Alien Blood"));
        assert_eq!(strip_kv("theme =", "theme"), None);
        assert_eq!(strip_kv("background = #0f1610", "background"), Some("#0f1610"));
    }

    #[test]
    fn test_parse_kitty_config_inline() {
        let dir = std::env::temp_dir().join("sysmon_kitty_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let config = dir.join("kitty.conf");
        std::fs::write(&config, r#"
background #1e1e2e
foreground #cdd6f4
color0 #45475a
color1 #f38ba8
color2 #a6e3a1
color3 #f9e2af
color4 #89b4fa
color5 #f5c2e7
color6 #94e2d5
color7 #bac2de
"#).unwrap();
        let palette = parse_kitty_config(&config).expect("should parse");
        assert_eq!(palette.bg, [0x1e, 0x1e, 0x2e]);
        assert_eq!(palette.colors[1], [0xf3, 0x8b, 0xa8]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_parse_kitty_config_follows_include() {
        let dir = std::env::temp_dir().join("sysmon_kitty_include_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let theme = dir.join("theme.conf");
        std::fs::write(&theme, r#"
background #000000
color0 #111111
color1 #222222
color2 #333333
color3 #444444
color4 #555555
color5 #666666
color6 #777777
"#).unwrap();
        let main = dir.join("kitty.conf");
        std::fs::write(&main, "include theme.conf\n").unwrap();
        let palette = parse_kitty_config(&main).expect("should parse");
        assert_eq!(palette.bg, [0, 0, 0]);
        assert_eq!(palette.colors[5], [0x66, 0x66, 0x66]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // ---------------------------------------------------------------
    // Alien Blood ground-truth lock-in
    //
    // The user's kitty runs Alien Blood inline in `~/.config/kitty/kitty.conf`
    // and the resulting rendering across every sysmon tool is approved as
    // correct. The fixtures and tests below freeze that output: any future
    // change that moves a single RGB byte will fail one of these tests.
    // ---------------------------------------------------------------

    const ALIEN_BLOOD_KITTY_CONF: &str = "\
color0 #112615
color1 #7f2b26
color2 #2f7e25
color3 #707f23
color4 #2f697f
color5 #47577e
color6 #317f76
color7 #647d75
color8 #3c4711
color9 #df8008
color10 #18e000
color11 #bde000
color12 #00a9df
color13 #0058df
color14 #00dfc3
color15 #73f990
background #0f160f
foreground #637d75
";

    fn alien_blood_palette() -> Palette {
        Palette {
            bg: [0x0f, 0x16, 0x0f],
            fg: [0x63, 0x7d, 0x75],
            colors: [
                [0x11, 0x26, 0x15],
                [0x7f, 0x2b, 0x26],
                [0x2f, 0x7e, 0x25],
                [0x70, 0x7f, 0x23],
                [0x2f, 0x69, 0x7f],
                [0x47, 0x57, 0x7e],
                [0x31, 0x7f, 0x76],
                [0x64, 0x7d, 0x75],
                [0x3c, 0x47, 0x11],
                [0xdf, 0x80, 0x08],
                [0x18, 0xe0, 0x00],
                [0xbd, 0xe0, 0x00],
                [0x00, 0xa9, 0xdf],
                [0x00, 0x58, 0xdf],
                [0x00, 0xdf, 0xc3],
                [0x73, 0xf9, 0x90],
            ],
        }
    }

    #[test]
    fn test_parse_kitty_alien_blood_exact() {
        let dir = std::env::temp_dir().join("sysmon_kitty_alien_blood_exact");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let config = dir.join("kitty.conf");
        std::fs::write(&config, ALIEN_BLOOD_KITTY_CONF).unwrap();
        let parsed = parse_kitty_config(&config).expect("should parse");
        let expected = alien_blood_palette();
        assert_eq!(parsed.bg, expected.bg, "bg");
        assert_eq!(parsed.fg, expected.fg, "fg");
        for i in 0..16 {
            assert_eq!(parsed.colors[i], expected.colors[i], "slot {i}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_alien_blood_fixture_matches_parsed() {
        let dir = std::env::temp_dir().join("sysmon_kitty_fixture_match");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let config = dir.join("kitty.conf");
        std::fs::write(&config, ALIEN_BLOOD_KITTY_CONF).unwrap();
        let parsed = parse_kitty_config(&config).expect("should parse");
        let fixture = alien_blood_palette();
        assert_eq!(parsed.bg, fixture.bg);
        assert_eq!(parsed.fg, fixture.fg);
        assert_eq!(parsed.colors, fixture.colors);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_alien_blood_semantic_helpers() {
        // Hardcoded expected RGBs — inlining the `lerp` formula here would
        // let a regression in `lerp_rgb` pass silently. Frozen numbers are
        // the whole point of the lock-in.
        let p = alien_blood_palette();
        assert_eq!(p.surface(), Color::Rgb(0x11, 0x26, 0x15));
        assert_eq!(p.red(), Color::Rgb(0x7f, 0x2b, 0x26));
        assert_eq!(p.green(), Color::Rgb(0x2f, 0x7e, 0x25));
        assert_eq!(p.yellow(), Color::Rgb(0x70, 0x7f, 0x23));
        assert_eq!(p.blue(), Color::Rgb(0x2f, 0x69, 0x7f));
        assert_eq!(p.magenta(), Color::Rgb(0x47, 0x57, 0x7e));
        assert_eq!(p.cyan(), Color::Rgb(0x31, 0x7f, 0x76));
        assert_eq!(p.label(), Color::Rgb(0x64, 0x7d, 0x75));
        assert_eq!(p.bright_surface(), Color::Rgb(0x3c, 0x47, 0x11));
        assert_eq!(p.bright_red(), Color::Rgb(0xdf, 0x80, 0x08));
        assert_eq!(p.bright_green(), Color::Rgb(0x18, 0xe0, 0x00));
        assert_eq!(p.bright_yellow(), Color::Rgb(0xbd, 0xe0, 0x00));
        assert_eq!(p.bright_blue(), Color::Rgb(0x00, 0xa9, 0xdf));
        assert_eq!(p.bright_magenta(), Color::Rgb(0x00, 0x58, 0xdf));
        assert_eq!(p.bright_cyan(), Color::Rgb(0x00, 0xdf, 0xc3));
        assert_eq!(p.bright_label(), Color::Rgb(0x73, 0xf9, 0x90));
        assert_eq!(p.bg_color(), Color::Rgb(0x0f, 0x16, 0x0f));
        assert_eq!(p.fg_color(), Color::Rgb(0x63, 0x7d, 0x75));
        // Muted label: every tool's border_color / label_color; shared line_chart axis.
        assert_eq!(p.muted_label(), Color::Rgb(73, 125, 77));
        // lerp(11, 9, 0.5) — gpu temp_color, cpu usage_color for 60-85% band.
        assert_eq!(p.lerp(11, 9, 0.5), Color::Rgb(206, 176, 4));
        // mix_with_bg(0, 0.5) — cpu/net dim columns.
        assert_eq!(p.mix_with_bg(0, 0.5), Color::Rgb(16, 30, 18));
        // mix_with_bg(14, 0.25) — poly selected_bg.
        assert_eq!(p.mix_with_bg(14, 0.25), Color::Rgb(11, 72, 60));
        // mix_with_bg(7, 0.4) — poly muted_color.
        assert_eq!(p.mix_with_bg(7, 0.4), Color::Rgb(49, 63, 55));
    }

    // ---------------------------------------------------------------
    // Brightness helpers
    // ---------------------------------------------------------------

    #[test]
    fn test_rgb_luminance_extremes() {
        assert!((rgb_luminance([0, 0, 0])).abs() < 1e-9);
        assert!((rgb_luminance([255, 255, 255]) - 1.0).abs() < 1e-9);
        assert!((rgb_luminance([255, 0, 0]) - 0.2126).abs() < 1e-4);
        assert!((rgb_luminance([0, 255, 0]) - 0.7152).abs() < 1e-4);
        assert!((rgb_luminance([0, 0, 255]) - 0.0722).abs() < 1e-4);
    }

    #[test]
    fn test_is_dark_alien_blood() {
        assert!(alien_blood_palette().is_dark());
    }

    #[test]
    fn test_is_dark_light_theme() {
        let light = Palette { bg: [240, 240, 240], ..Palette::default() };
        assert!(!light.is_dark());
    }

    #[test]
    fn test_contrast_ratio_sanity() {
        let p = Palette::default();
        assert!((p.contrast_ratio([0, 0, 0], [255, 255, 255]) - 21.0).abs() < 1e-9);
        assert!((p.contrast_ratio([255, 255, 255], [255, 255, 255]) - 1.0).abs() < 1e-9);
    }

    // ---------------------------------------------------------------
    // Missing-brights fallback
    // ---------------------------------------------------------------

    #[test]
    fn test_brights_uninitialized_default_is_false() {
        assert!(!Palette::default().brights_uninitialized());
    }

    #[test]
    fn test_brights_uninitialized_alien_blood_is_false() {
        assert!(!alien_blood_palette().brights_uninitialized());
    }

    #[test]
    fn test_synthesize_brights_no_op_on_alien_blood() {
        let mut p = alien_blood_palette();
        let expected = alien_blood_palette();
        p.synthesize_brights();
        assert_eq!(p.bg, expected.bg);
        assert_eq!(p.fg, expected.fg);
        assert_eq!(p.colors, expected.colors);
    }

    #[test]
    fn test_parse_kitty_only_basics_fills_brights() {
        let dir = std::env::temp_dir().join("sysmon_kitty_basics_only");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let config = dir.join("kitty.conf");
        // Only color0-color7 + bg + fg — no brights defined.
        std::fs::write(&config, "\
background #0f160f
foreground #637d75
color0 #112615
color1 #7f2b26
color2 #2f7e25
color3 #707f23
color4 #2f697f
color5 #47577e
color6 #317f76
color7 #647d75
").unwrap();
        let palette = parse_kitty_config(&config).expect("should parse");
        // Every bright slot must now be non-zero (synthesis fired).
        for i in 8..16 {
            assert_ne!(palette.colors[i], [0, 0, 0], "bright slot {i} not synthesized");
        }
        // For slots 1..=7 in a dark theme, bright should be lighter than basic.
        for i in 1..=7 {
            let basic = rgb_luminance(palette.colors[i]);
            let bright = rgb_luminance(palette.colors[i + 8]);
            assert!(
                bright > basic,
                "bright slot {} ({:?}, lum={}) not lighter than basic {} ({:?}, lum={})",
                i + 8,
                palette.colors[i + 8],
                bright,
                i,
                palette.colors[i],
                basic,
            );
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_parse_ghostty_only_basics_fills_brights() {
        let text = "\
palette = 0=#112615
palette = 1=#7f2b26
palette = 2=#2f7e25
palette = 3=#707f23
palette = 4=#2f697f
palette = 5=#47577e
palette = 6=#317f76
palette = 7=#647d75
background = #0f160f
foreground = #637d75
";
        let palette = parse_ghostty_theme(text).expect("should parse");
        for i in 8..16 {
            assert_ne!(palette.colors[i], [0, 0, 0], "bright slot {i} not synthesized");
        }
        for i in 1..=7 {
            let basic = rgb_luminance(palette.colors[i]);
            let bright = rgb_luminance(palette.colors[i + 8]);
            assert!(bright > basic, "slot {}: bright not lighter than basic", i + 8);
        }
    }

    #[test]
    fn test_synthesize_brights_light_theme() {
        // Light background, zeroed brights, moderate-luminance basics.
        let mut p = Palette {
            bg: [240, 240, 240],
            fg: [20, 20, 20],
            colors: [
                [200, 200, 200], // slot 0 (surface)
                [180, 100, 100], // 1
                [100, 180, 100], // 2
                [180, 180, 100], // 3
                [100, 100, 180], // 4
                [180, 100, 180], // 5
                [100, 180, 180], // 6
                [120, 120, 120], // 7
                [0, 0, 0],       // 8..15 zeroed → synthesize
                [0, 0, 0],
                [0, 0, 0],
                [0, 0, 0],
                [0, 0, 0],
                [0, 0, 0],
                [0, 0, 0],
                [0, 0, 0],
            ],
        };
        assert!(!p.is_dark(), "test fixture must have light bg");
        p.synthesize_brights();
        // On a light theme, brights should be DARKER than basics so they pop
        // against the pale background.
        for i in 1..=7 {
            let basic = rgb_luminance(p.colors[i]);
            let bright = rgb_luminance(p.colors[i + 8]);
            assert!(
                bright < basic,
                "light theme: bright slot {} ({:?}) should be darker than basic {} ({:?})",
                i + 8,
                p.colors[i + 8],
                i,
                p.colors[i],
            );
        }
        // Slot 8 follows fg, which is dark — should be darker than slot 0.
        let s0 = rgb_luminance(p.colors[0]);
        let s8 = rgb_luminance(p.colors[8]);
        assert!(s8 < s0, "slot 8 should lean toward fg (dark) on light theme");
    }
}
