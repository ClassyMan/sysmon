use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;
use crate::spectrum::SpectrumAnalyzer;
use sysmon_shared::terminal_theme::palette;

fn border_color() -> Color { palette().surface() }
fn label_color() -> Color { palette().label() }
fn title_color() -> Color { palette().bright_cyan() }
fn peak_color() -> Color { Color::Rgb(0xff, 0x55, 0x55) }

const BAR_BLOCKS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

pub fn render(frame: &mut Frame, app: &App) {
    render_in(frame, frame.area(), app);
}

pub fn render_in(frame: &mut Frame, area: Rect, app: &App) {
    let status = match app.capture_error() {
        Some(err) => format!("ERROR: {err}"),
        None => format!("buf={} peak={:.4}", app.buffer_len(), app.peak_amplitude()),
    };
    render_parts(
        frame,
        area,
        &app.analyzer,
        app.sample_rate(),
        &app.device_name(),
        &status,
    );
}

pub fn render_parts(
    frame: &mut Frame,
    area: Rect,
    analyzer: &SpectrumAnalyzer,
    sample_rate: u32,
    device_name: &str,
    status: &str,
) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(6),
            Constraint::Length(1),
        ])
        .split(area);

    draw_header(frame, outer[0], device_name, sample_rate, status);
    draw_spectrum(frame, outer[1], analyzer, sample_rate);
    draw_freq_labels(frame, outer[2]);
}

fn draw_header(frame: &mut Frame, area: Rect, device_name: &str, sample_rate: u32, status: &str) {
    let text = Paragraph::new(Line::from(vec![
        Span::styled(" AUDIO ", Style::default().fg(title_color()).add_modifier(Modifier::BOLD)),
        Span::styled(
            format!(" {} | {}Hz | {} ", device_name, sample_rate, status),
            Style::default().fg(label_color()),
        ),
    ]));
    frame.render_widget(text, area);
}

fn draw_spectrum(frame: &mut Frame, area: Rect, analyzer: &SpectrumAnalyzer, sample_rate: u32) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width < 4 || inner.height < 2 {
        return;
    }

    let bar_count = inner.width as usize;
    let height = inner.height as usize;
    let values = analyzer.get_bar_values(bar_count, sample_rate);
    let peaks = analyzer.get_peak_values(bar_count, sample_rate);

    let buf = frame.buffer_mut();

    for (col_idx, (&value, &peak)) in values.iter().zip(peaks.iter()).enumerate() {
        let col = inner.x + col_idx as u16;
        let bar_height_sub = (value * height as f32 * 8.0).round() as usize;
        let peak_row_sub = (peak * height as f32 * 8.0).round() as usize;

        // Draw the bar from bottom up
        for row_idx in 0..height {
            let row = inner.y + (height - 1 - row_idx) as u16;
            let cell_bottom = row_idx * 8;
            let cell_top = cell_bottom + 8;

            if bar_height_sub >= cell_top {
                let ch = '█';
                let color = bar_color(row_idx, height);
                if let Some(cell) = buf.cell_mut((col, row)) {
                    cell.set_char(ch);
                    cell.set_style(Style::default().fg(color));
                }
            } else if bar_height_sub > cell_bottom {
                let fill = bar_height_sub - cell_bottom;
                let ch = BAR_BLOCKS[fill.min(8)];
                let color = bar_color(row_idx, height);
                if let Some(cell) = buf.cell_mut((col, row)) {
                    cell.set_char(ch);
                    cell.set_style(Style::default().fg(color));
                }
            }
        }

        // Overlay peak marker on top — always visible, even inside the bar region
        if peak_row_sub > 0 {
            let peak_row_idx = (peak_row_sub.saturating_sub(1)) / 8;
            if peak_row_idx < height {
                let row = inner.y + (height - 1 - peak_row_idx) as u16;
                if let Some(cell) = buf.cell_mut((col, row)) {
                    cell.set_char('━');
                    cell.set_style(Style::default().fg(peak_color()).add_modifier(Modifier::BOLD));
                }
            }
        }
    }
}

fn bar_color(row_from_bottom: usize, total_height: usize) -> Color {
    let p = palette();
    if total_height == 0 {
        return p.bright_green();
    }
    let frac = row_from_bottom as f64 / total_height as f64;
    // All-green gradient: forest (slot 2) → bright (slot 10) → mint (slot 15).
    if frac < 0.5 {
        p.lerp(2, 10, frac / 0.5)
    } else {
        p.lerp(10, 15, (frac - 0.5) / 0.5)
    }
}

fn draw_freq_labels(frame: &mut Frame, area: Rect) {
    let width = area.width as usize;
    if width < 20 {
        return;
    }

    let labels = ["0Hz", "2K", "4K", "6K", "8K", "10K", "12K"];
    let max_freq = 12000.0_f32;

    let mut spans = Vec::new();
    let mut last_pos = 0usize;

    for label in &labels {
        let freq: f32 = match *label {
            "0Hz" => 0.0,
            "2K" => 2000.0,
            "4K" => 4000.0,
            "6K" => 6000.0,
            "8K" => 8000.0,
            "10K" => 10000.0,
            "12K" => 12000.0,
            _ => 0.0,
        };
        let frac = freq / max_freq;
        let target_col = (frac * (width - 1) as f32).round() as usize;
        if target_col >= last_pos {
            let padding = target_col - last_pos;
            spans.push(Span::raw(" ".repeat(padding)));
            spans.push(Span::styled(*label, Style::default().fg(label_color())));
            last_pos = target_col + label.len();
        }
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spectrum::SpectrumAnalyzer;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn buffer_to_string(terminal: &Terminal<TestBackend>) -> String {
        let buf = terminal.backend().buffer();
        let mut output = String::new();
        for row in 0..buf.area.height {
            for col in 0..buf.area.width {
                let cell = &buf[(col, row)];
                output.push_str(cell.symbol());
            }
            output.push('\n');
        }
        output
    }

    #[test]
    fn test_render_empty_spectrum_no_panic() {
        let analyzer = SpectrumAnalyzer::new(4096);
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_parts(frame, frame.area(), &analyzer, 44100, "Test Device", "buf=0"))
            .unwrap();
    }

    #[test]
    fn test_render_with_audio_data_no_panic() {
        let mut analyzer = SpectrumAnalyzer::new(4096);
        let samples: Vec<f32> = (0..4096)
            .map(|idx| (2.0 * std::f32::consts::PI * 440.0 * idx as f32 / 44100.0).sin())
            .collect();
        analyzer.process(&samples);

        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_parts(frame, frame.area(), &analyzer, 44100, "Test Device", "buf=0"))
            .unwrap();
    }

    #[test]
    fn test_header_shows_audio() {
        let analyzer = SpectrumAnalyzer::new(4096);
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_parts(frame, frame.area(), &analyzer, 44100, "Test Device", "buf=0"))
            .unwrap();
        let output = buffer_to_string(&terminal);
        assert!(
            output.contains("AUDIO"),
            "expected 'AUDIO' in header, got:\n{output}"
        );
    }

    #[test]
    fn test_header_shows_device_name() {
        let analyzer = SpectrumAnalyzer::new(4096);
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_parts(frame, frame.area(), &analyzer, 44100, "My Sound Card", "buf=0"))
            .unwrap();
        let output = buffer_to_string(&terminal);
        assert!(
            output.contains("My Sound Card"),
            "expected device name in header, got:\n{output}"
        );
    }

    #[test]
    fn test_header_shows_sample_rate() {
        let analyzer = SpectrumAnalyzer::new(4096);
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_parts(frame, frame.area(), &analyzer, 48000, "Test", "buf=0"))
            .unwrap();
        let output = buffer_to_string(&terminal);
        assert!(
            output.contains("48000Hz"),
            "expected sample rate in header, got:\n{output}"
        );
    }

    #[test]
    fn test_bar_color_bottom_matches_palette_green() {
        assert_eq!(bar_color(0, 20), palette().bright_green());
    }

    #[test]
    fn test_bar_color_gradient_differs_by_height() {
        assert_ne!(bar_color(0, 20), bar_color(19, 20));
    }

    #[test]
    fn test_bar_color_zero_height_returns_green() {
        assert_eq!(bar_color(0, 0), palette().bright_green());
    }

    #[test]
    fn test_render_narrow_terminal_no_panic() {
        let analyzer = SpectrumAnalyzer::new(4096);
        let backend = TestBackend::new(15, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render_parts(frame, frame.area(), &analyzer, 44100, "Test", "buf=0"))
            .unwrap();
    }
}
