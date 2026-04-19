use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::{App, ViewMode};
use crate::collector::DecodedImage;
use sysmon_shared::terminal_theme::palette;

fn astro_color() -> Color { palette().bright_cyan() }
fn border_color() -> Color { palette().muted_label() }
fn label_color() -> Color { palette().muted_label() }
fn nav_color() -> Color { palette().bright_cyan() }
fn mode_color() -> Color { palette().bright_yellow() }

use crate::theme::ThemePalette;

pub fn render(frame: &mut Frame, app: &App) {
    render_in(frame, frame.area(), app);
}

pub fn render_in(frame: &mut Frame, area: Rect, app: &App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(6),
            Constraint::Length(1),
        ])
        .split(area);

    draw_header(frame, outer[0], app);
    draw_content(frame, outer[1], app);
    draw_navigation(frame, outer[2], app);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let title = app
        .selected_entry()
        .map(|e| format!(" {} ", e.title))
        .unwrap_or_default();

    let status = if let Some(ref err) = app.last_error {
        format!(" {err}")
    } else if let Some(last) = app.last_update {
        let ago = last.elapsed().as_secs();
        format!(" Updated {ago}s ago")
    } else {
        " Fetching...".to_string()
    };

    let mode_span = Span::styled(
        format!(" [{}] ", app.view_mode.label()),
        Style::default()
            .fg(mode_color())
            .add_modifier(Modifier::BOLD),
    );

    let text = Paragraph::new(Line::from(vec![
        Span::styled(
            " ASTRO ",
            Style::default()
                .fg(astro_color())
                .add_modifier(Modifier::BOLD),
        ),
        mode_span,
        Span::styled(title, Style::default().fg(palette().fg_color())),
        Span::styled(status, Style::default().fg(label_color())),
    ]));
    frame.render_widget(text, area);
}

fn draw_content(frame: &mut Frame, area: Rect, app: &App) {
    let entry = match app.selected_entry() {
        Some(e) => e,
        None => {
            let msg = if app.last_error.is_some() {
                "Failed to fetch APOD data"
            } else {
                "Fetching astronomy picture of the day..."
            };
            let placeholder = Paragraph::new(msg)
                .style(Style::default().fg(label_color()))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(border_color())),
                );
            frame.render_widget(placeholder, area);
            return;
        }
    };

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    draw_image_panel(frame, chunks[0], entry, app.view_mode, &app.palette);
    draw_text_panel(frame, chunks[1], entry, app.scroll_offset);
}

fn draw_image_panel(
    frame: &mut Frame,
    area: Rect,
    entry: &crate::collector::ApodEntry,
    view_mode: ViewMode,
    palette: &ThemePalette,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width < 2 || inner.height < 2 {
        return;
    }

    let no_content = entry.image.is_none() && entry.ascii_art.is_none();
    if no_content {
        let msg = if entry.media_type == "video" {
            "Video content"
        } else {
            "Loading image..."
        };
        let p = Paragraph::new(msg).style(Style::default().fg(label_color()));
        frame.render_widget(p, inner);
        return;
    }

    match view_mode {
        ViewMode::Ascii => draw_ascii_art(frame, inner, entry),
        ViewMode::Pixels => draw_pixel_art(frame, inner, entry),
        ViewMode::Themed => draw_themed_art(frame, inner, entry, palette),
        ViewMode::Photo => draw_photo(frame, inner, entry),
    }
}

fn draw_ascii_art(frame: &mut Frame, area: Rect, entry: &crate::collector::ApodEntry) {
    let art = match &entry.ascii_art {
        Some(a) => a,
        None => {
            let p = Paragraph::new("Generating ASCII art...")
                .style(Style::default().fg(label_color()));
            frame.render_widget(p, area);
            return;
        }
    };

    // Use the source image to color each ASCII character
    let image = entry.image.as_ref();
    let art_lines: Vec<&str> = art.lines().collect();
    let art_height = art_lines.len();
    let art_width = art_lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);

    let buf = frame.buffer_mut();

    for (row, line) in art_lines.iter().enumerate().take(area.height as usize) {
        for (col, ch) in line.chars().enumerate().take(area.width as usize) {
            let x = area.x + col as u16;
            let y = area.y + row as u16;

            let color = if let Some(img) = image {
                // Map ASCII art position to source image pixel
                let src_x = if art_width > 1 {
                    (col as u64 * img.width as u64 / art_width as u64) as u32
                } else {
                    0
                };
                let src_y = if art_height > 1 {
                    (row as u64 * img.height as u64 / art_height as u64) as u32
                } else {
                    0
                };
                let src_x = src_x.min(img.width.saturating_sub(1));
                let src_y = src_y.min(img.height.saturating_sub(1));
                let idx = (src_y * img.width + src_x) as usize;
                if let Some(&rgb) = img.pixels.get(idx) {
                    Color::Rgb(rgb[0], rgb[1], rgb[2])
                } else {
                    label_color()
                }
            } else {
                label_color()
            };

            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_char(ch);
                cell.set_style(Style::default().fg(color));
            }
        }
    }
}

fn draw_pixel_art(frame: &mut Frame, area: Rect, entry: &crate::collector::ApodEntry) {
    let image = match &entry.image {
        Some(img) => img,
        None => {
            let p = Paragraph::new("Loading image...")
                .style(Style::default().fg(label_color()));
            frame.render_widget(p, area);
            return;
        }
    };

    let target_w = area.width as u32;
    let target_h = area.height as u32 * 2;

    let resized = resize_nearest(image, target_w, target_h);
    let buf = frame.buffer_mut();

    for row in 0..area.height {
        let py_top = (row as u32) * 2;
        let py_bot = py_top + 1;

        for col in 0..area.width {
            let px = col as u32;
            let top = resized.pixel_at(px, py_top);
            let bot = resized.pixel_at(px, py_bot);

            let x = area.x + col;
            let y = area.y + row;

            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_char('\u{2580}');
                cell.set_style(
                    Style::default()
                        .fg(Color::Rgb(top[0], top[1], top[2]))
                        .bg(Color::Rgb(bot[0], bot[1], bot[2])),
                );
            }
        }
    }
}

fn draw_themed_art(frame: &mut Frame, area: Rect, entry: &crate::collector::ApodEntry, palette: &ThemePalette) {
    let image = match &entry.image {
        Some(img) => img,
        None => {
            let p = Paragraph::new("Loading image...")
                .style(Style::default().fg(label_color()));
            frame.render_widget(p, area);
            return;
        }
    };

    let target_w = area.width as u32;
    let target_h = area.height as u32 * 2;

    let resized = resize_nearest(image, target_w, target_h);
    let buf = frame.buffer_mut();

    for row in 0..area.height {
        let py_top = (row as u32) * 2;
        let py_bot = py_top + 1;

        for col in 0..area.width {
            let top = palette.blend(resized.pixel_at(col as u32, py_top));
            let bot = palette.blend(resized.pixel_at(col as u32, py_bot));

            let x = area.x + col;
            let y = area.y + row;

            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_char('\u{2580}');
                cell.set_style(
                    Style::default()
                        .fg(Color::Rgb(top[0], top[1], top[2]))
                        .bg(Color::Rgb(bot[0], bot[1], bot[2])),
                );
            }
        }
    }
}

fn draw_photo(frame: &mut Frame, area: Rect, entry: &crate::collector::ApodEntry) {
    let image = match &entry.image {
        Some(img) => img,
        None => {
            let p = Paragraph::new("Loading image...")
                .style(Style::default().fg(label_color()));
            frame.render_widget(p, area);
            return;
        }
    };

    // Reconstruct DynamicImage and use high-quality Lanczos3 resize
    let rgb_img = match image::RgbImage::from_raw(
        image.width,
        image.height,
        image.pixels.iter().flat_map(|p| p.iter().copied()).collect(),
    ) {
        Some(img) => img,
        None => return,
    };

    let target_w = area.width as u32;
    let target_h = area.height as u32 * 2;

    // Aspect-ratio-preserving resize with Lanczos3 for smoothest result
    let dyn_img = image::DynamicImage::ImageRgb8(rgb_img);
    let resized = dyn_img.resize(target_w, target_h, image::imageops::FilterType::Lanczos3);
    let rgb = resized.to_rgb8();
    let (rw, rh) = rgb.dimensions();

    // Center in area
    let offset_x = (target_w.saturating_sub(rw)) / 2;
    let offset_y = (target_h.saturating_sub(rh)) / 2;

    let buf = frame.buffer_mut();

    for row in 0..area.height {
        let py_top = (row as u32) * 2;
        let py_bot = py_top + 1;

        for col in 0..area.width {
            let px = col as u32;

            let get_pixel = |px: u32, py: u32| -> [u8; 3] {
                let sx = px.checked_sub(offset_x).filter(|&x| x < rw);
                let sy = py.checked_sub(offset_y).filter(|&y| y < rh);
                match (sx, sy) {
                    (Some(x), Some(y)) => rgb.get_pixel(x, y).0,
                    _ => [0, 0, 0],
                }
            };

            let top = get_pixel(px, py_top);
            let bot = get_pixel(px, py_bot);

            let x = area.x + col as u16;
            let y = area.y + row;

            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_char('\u{2580}');
                cell.set_style(
                    Style::default()
                        .fg(Color::Rgb(top[0], top[1], top[2]))
                        .bg(Color::Rgb(bot[0], bot[1], bot[2])),
                );
            }
        }
    }
}

fn draw_text_panel(
    frame: &mut Frame,
    area: Rect,
    entry: &crate::collector::ApodEntry,
    scroll: u16,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width < 4 || inner.height < 2 {
        return;
    }

    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(Span::styled(
        &entry.title,
        Style::default()
            .fg(astro_color())
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    lines.push(Line::from(Span::styled(
        &entry.date,
        Style::default().fg(label_color()),
    )));

    if let Some(ref cr) = entry.copyright {
        lines.push(Line::from(Span::styled(
            format!("Credit: {cr}"),
            Style::default().fg(label_color()),
        )));
    }

    // Clickable APOD link — date format YYYY-MM-DD → apYYMMDD
    let date_compact = entry.date.replace('-', "");
    let url = format!(
        "https://apod.nasa.gov/apod/ap{}.html",
        &date_compact[2..]  // strip century: 20260415 → 260415
    );
    lines.push(Line::from(Span::styled(
        url,
        Style::default()
            .fg(nav_color())
            .add_modifier(Modifier::UNDERLINED),
    )));
    lines.push(Line::from(""));

    lines.push(Line::from(Span::styled(
        &entry.explanation,
        Style::default().fg(palette().fg_color()),
    )));

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .scroll((scroll, 0));

    frame.render_widget(paragraph, inner);
}

fn draw_navigation(frame: &mut Frame, area: Rect, app: &App) {
    if app.entries.is_empty() {
        return;
    }
    let total = app.entries.len();
    let current = app.selected + 1;
    let text = Paragraph::new(Line::from(vec![
        Span::styled(" \u{2190} h ", Style::default().fg(nav_color())),
        Span::styled(
            format!("[{current}/{total}]"),
            Style::default()
                .fg(palette().fg_color())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" l \u{2192} ", Style::default().fg(nav_color())),
        Span::styled("\u{2191}\u{2193} scroll", Style::default().fg(nav_color())),
    ]));
    frame.render_widget(text, area);
}

struct ResizedImage {
    width: u32,
    height: u32,
    pixels: Vec<[u8; 3]>,
}

impl ResizedImage {
    fn pixel_at(&self, x: u32, y: u32) -> [u8; 3] {
        if x >= self.width || y >= self.height {
            return [0, 0, 0];
        }
        let idx = (y * self.width + x) as usize;
        self.pixels.get(idx).copied().unwrap_or([0, 0, 0])
    }
}

fn resize_nearest(src: &DecodedImage, target_w: u32, target_h: u32) -> ResizedImage {
    if src.width == 0 || src.height == 0 || target_w == 0 || target_h == 0 {
        return ResizedImage {
            width: target_w,
            height: target_h,
            pixels: vec![[0; 3]; (target_w * target_h) as usize],
        };
    }

    let src_aspect = src.width as f64 / src.height as f64;
    let target_aspect = target_w as f64 / target_h as f64;

    let (scaled_w, scaled_h) = if src_aspect > target_aspect {
        (target_w, ((target_w as f64 / src_aspect) as u32).max(1))
    } else {
        (((target_h as f64 * src_aspect) as u32).max(1), target_h)
    };

    let offset_x = (target_w - scaled_w) / 2;
    let offset_y = (target_h - scaled_h) / 2;

    let mut pixels = vec![[0u8; 3]; (target_w * target_h) as usize];

    for y in 0..scaled_h {
        let src_y = ((y as f64 / scaled_h as f64) * src.height as f64) as u32;
        let src_y = src_y.min(src.height - 1);

        for x in 0..scaled_w {
            let src_x = ((x as f64 / scaled_w as f64) * src.width as f64) as u32;
            let src_x = src_x.min(src.width - 1);
            let src_idx = (src_y * src.width + src_x) as usize;

            let dst_x = x + offset_x;
            let dst_y = y + offset_y;
            let dst_idx = (dst_y * target_w + dst_x) as usize;

            if let Some(pixel) = src.pixels.get(src_idx) {
                if dst_idx < pixels.len() {
                    pixels[dst_idx] = *pixel;
                }
            }
        }
    }

    ResizedImage {
        width: target_w,
        height: target_h,
        pixels,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::{ApodEntry, DecodedImage, FetchState};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::sync::{Arc, Mutex};

    fn test_app_empty() -> App {
        App::new(Arc::new(Mutex::new(FetchState::new(
            3_600_000,
            "DEMO_KEY".to_string(),
        ))))
    }

    fn test_app_with_entries() -> App {
        let mut app = test_app_empty();
        app.entries = vec![
            ApodEntry {
                title: "The Horsehead Nebula".to_string(),
                explanation: "A dark molecular cloud in Orion.".to_string(),
                date: "2026-04-14".to_string(),
                copyright: Some("NASA/ESA".to_string()),
                media_type: "image".to_string(),
                image: Some(DecodedImage {
                    width: 4,
                    height: 4,
                    pixels: vec![[255, 0, 0]; 16],
                }),
                ascii_art: Some("@@@@\n@@@@\n@@@@\n@@@@".to_string()),
            },
            ApodEntry {
                title: "Mars Close Approach".to_string(),
                explanation: "Mars at opposition.".to_string(),
                date: "2026-04-15".to_string(),
                copyright: None,
                media_type: "video".to_string(),
                image: None,
                    ascii_art: None,
            },
        ];
        app
    }

    #[test]
    fn test_render_empty_no_panic() {
        let app = test_app_empty();
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
    }

    #[test]
    fn test_render_with_entries_no_panic() {
        let app = test_app_with_entries();
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
    }

    #[test]
    fn test_render_narrow_no_panic() {
        let app = test_app_with_entries();
        let backend = TestBackend::new(40, 15);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
    }

    #[test]
    fn test_header_shows_astro() {
        let app = test_app_empty();
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let buf = terminal.backend().buffer();
        let mut output = String::new();
        for col in 0..buf.area.width {
            output.push_str(buf[(col, 0)].symbol());
        }
        assert!(output.contains("ASTRO"));
    }

    #[test]
    fn test_header_shows_fetching() {
        let app = test_app_empty();
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let buf = terminal.backend().buffer();
        let mut output = String::new();
        for col in 0..buf.area.width {
            output.push_str(buf[(col, 0)].symbol());
        }
        assert!(output.contains("Fetching"));
    }

    #[test]
    fn test_resize_nearest_identity() {
        let src = DecodedImage {
            width: 4,
            height: 4,
            pixels: vec![[255, 0, 0]; 16],
        };
        let result = resize_nearest(&src, 4, 4);
        assert_eq!(result.width, 4);
        assert_eq!(result.height, 4);
        assert_eq!(result.pixel_at(0, 0), [255, 0, 0]);
    }

    #[test]
    fn test_resize_nearest_downscale() {
        let src = DecodedImage {
            width: 8,
            height: 8,
            pixels: vec![[100, 200, 50]; 64],
        };
        let result = resize_nearest(&src, 4, 4);
        assert_eq!(result.width, 4);
        assert_eq!(result.height, 4);
        assert_eq!(result.pixel_at(0, 0), [100, 200, 50]);
    }

    #[test]
    fn test_resize_nearest_wide_image_letterboxed() {
        let src = DecodedImage {
            width: 8,
            height: 2,
            pixels: vec![[255, 255, 255]; 16],
        };
        let result = resize_nearest(&src, 8, 8);
        // Top rows should be black (letterbox)
        assert_eq!(result.pixel_at(0, 0), [0, 0, 0]);
        // Middle rows should have content
        assert_eq!(result.pixel_at(4, 4), [255, 255, 255]);
    }

    #[test]
    fn test_resize_nearest_tall_image_pillarboxed() {
        let src = DecodedImage {
            width: 2,
            height: 8,
            pixels: vec![[128, 128, 128]; 16],
        };
        let result = resize_nearest(&src, 8, 8);
        // Left edge should be black (pillarbox)
        assert_eq!(result.pixel_at(0, 0), [0, 0, 0]);
        // Center should have content
        assert_eq!(result.pixel_at(4, 4), [128, 128, 128]);
    }

    #[test]
    fn test_resize_nearest_empty_source() {
        let src = DecodedImage {
            width: 0,
            height: 0,
            pixels: Vec::new(),
        };
        let result = resize_nearest(&src, 4, 4);
        assert_eq!(result.pixel_at(0, 0), [0, 0, 0]);
    }

    #[test]
    fn test_pixel_at_out_of_bounds() {
        let img = ResizedImage {
            width: 2,
            height: 2,
            pixels: vec![[255, 0, 0]; 4],
        };
        assert_eq!(img.pixel_at(99, 99), [0, 0, 0]);
    }
}
