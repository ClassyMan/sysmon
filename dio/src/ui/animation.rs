use std::io::Cursor;
use std::time::Instant;

use image::AnimationDecoder;
use image::DynamicImage;
use image::GenericImageView;
use image::Rgba;
use image::RgbaImage;
use image::codecs::gif::GifDecoder;
use image::imageops::FilterType;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui_image::Resize;
use ratatui_image::StatefulImage;
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;

const DRIVE_GIF: &[u8] = include_bytes!("../../assets/drive.gif");
const UPSCALE: u32 = 8;

pub struct AnimatedGif {
    raw_frames: Vec<RgbaImage>,
    cumulative_ms: Vec<u64>,
    total_ms: u64,
    start: Instant,
    cell_w_px: u16,
    cell_h_px: u16,
    pixel_w: u32,
    pixel_h: u32,
    picker: Picker,
    protocols: Vec<StatefulProtocol>,
    tint: [u8; 3],
}

impl AnimatedGif {
    pub fn load_drive(picker: &mut Picker) -> Option<Self> {
        Self::load(picker, DRIVE_GIF)
    }

    fn load(picker: &mut Picker, bytes: &[u8]) -> Option<Self> {
        let decoder = GifDecoder::new(Cursor::new(bytes)).ok()?;
        let decoded = decoder.into_frames().collect_frames().ok()?;
        if decoded.is_empty() {
            return None;
        }

        let mut raw_frames = Vec::with_capacity(decoded.len());
        let mut cumulative_ms = Vec::with_capacity(decoded.len());
        let mut total_ms = 0u64;
        let mut pixel_w = 0u32;
        let mut pixel_h = 0u32;

        for frame in decoded {
            let (num, denom) = frame.delay().numer_denom_ms();
            let ms = if denom == 0 { 80 } else { (num as u64) / (denom as u64) };
            let ms = ms.max(20);
            total_ms += ms;
            cumulative_ms.push(total_ms);
            let buf = frame.into_buffer();
            let (w, h) = buf.dimensions();
            pixel_w = pixel_w.max(w);
            pixel_h = pixel_h.max(h);
            raw_frames.push(buf);
        }

        let (cell_w_px, cell_h_px) = picker.font_size();
        let cell_w_px = if cell_w_px == 0 { 8 } else { cell_w_px };
        let cell_h_px = if cell_h_px == 0 { 17 } else { cell_h_px };

        let initial_tint = [0, 0, 0];
        let protocols = build_protocols(picker, &raw_frames, initial_tint);

        Some(Self {
            raw_frames,
            cumulative_ms,
            total_ms,
            start: Instant::now(),
            cell_w_px,
            cell_h_px,
            pixel_w,
            pixel_h,
            picker: picker.clone(),
            protocols,
            tint: initial_tint,
        })
    }

    pub fn set_tint(&mut self, color: Color) {
        let rgb = color_to_rgb(color);
        if rgb == self.tint {
            return;
        }
        self.tint = rgb;
        self.protocols = build_protocols(&mut self.picker, &self.raw_frames, rgb);
    }

    /// Number of columns needed to render the gif at `rows` text-rows tall
    /// while preserving its pixel aspect ratio.
    pub fn width_for_height(&self, rows: u16) -> u16 {
        if self.pixel_h == 0 || rows == 0 {
            return 0;
        }
        let target_px_h = rows as u32 * self.cell_h_px as u32;
        let target_px_w = target_px_h * self.pixel_w / self.pixel_h;
        ((target_px_w + self.cell_w_px as u32 / 2) / self.cell_w_px as u32) as u16
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        if area.width == 0 || area.height == 0 || self.protocols.is_empty() {
            return;
        }
        let elapsed = (self.start.elapsed().as_millis() as u64) % self.total_ms.max(1);
        let idx = self
            .cumulative_ms
            .iter()
            .position(|&c| elapsed < c)
            .unwrap_or(0);
        let widget = StatefulImage::default().resize(Resize::Fit(None));
        frame.render_stateful_widget(widget, area, &mut self.protocols[idx]);
    }
}

fn build_protocols(
    picker: &mut Picker,
    frames: &[RgbaImage],
    tint: [u8; 3],
) -> Vec<StatefulProtocol> {
    frames
        .iter()
        .map(|raw| {
            let tinted = tint_frame(raw, tint);
            let (w, h) = tinted.dimensions();
            let upscaled = DynamicImage::ImageRgba8(tinted).resize_exact(
                w * UPSCALE,
                h * UPSCALE,
                FilterType::Nearest,
            );
            picker.new_resize_protocol(upscaled)
        })
        .collect()
}

fn tint_frame(src: &RgbaImage, tint: [u8; 3]) -> RgbaImage {
    let (w, h) = src.dimensions();
    let mut out = RgbaImage::new(w, h);
    for (x, y, px) in src.enumerate_pixels() {
        let [r, g, b, a] = px.0;
        let max_channel = r.max(g).max(b);
        let pixel = if max_channel > 30 {
            Rgba([tint[0], tint[1], tint[2], a])
        } else {
            Rgba([0, 0, 0, a])
        };
        out.put_pixel(x, y, pixel);
    }
    out
}

fn color_to_rgb(color: Color) -> [u8; 3] {
    match color {
        Color::Rgb(r, g, b) => [r, g, b],
        _ => [180, 180, 180],
    }
}
