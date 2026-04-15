use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::spectrum::SpectrumAnalyzer;

const BORDER_COLOR: Color = Color::DarkGray;
const LABEL_COLOR: Color = Color::Gray;
const TITLE_COLOR: Color = Color::Rgb(255, 120, 255);
const PEAK_COLOR: Color = Color::Rgb(255, 255, 255);

const BAR_BLOCKS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

pub fn render(
    frame: &mut Frame,
    analyzer: &SpectrumAnalyzer,
    sample_rate: u32,
    device_name: &str,
) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(6),
            Constraint::Length(1),
        ])
        .split(frame.area());

    draw_header(frame, outer[0], device_name, sample_rate);
    draw_spectrum(frame, outer[1], analyzer, sample_rate);
    draw_freq_labels(frame, outer[2]);
}

fn draw_header(frame: &mut Frame, area: Rect, device_name: &str, sample_rate: u32) {
    let text = Paragraph::new(Line::from(vec![
        Span::styled(" AUDIO ", Style::default().fg(TITLE_COLOR).add_modifier(Modifier::BOLD)),
        Span::styled(
            format!(" {} | {}Hz ", device_name, sample_rate),
            Style::default().fg(LABEL_COLOR),
        ),
    ]));
    frame.render_widget(text, area);
}

fn draw_spectrum(frame: &mut Frame, area: Rect, analyzer: &SpectrumAnalyzer, sample_rate: u32) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BORDER_COLOR));
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
            } else if peak_row_sub > cell_bottom && peak_row_sub <= cell_top {
                if let Some(cell) = buf.cell_mut((col, row)) {
                    cell.set_char('─');
                    cell.set_style(Style::default().fg(PEAK_COLOR));
                }
            }
        }
    }
}

fn bar_color(row_from_bottom: usize, total_height: usize) -> Color {
    if total_height == 0 {
        return Color::Rgb(0, 200, 100);
    }
    let frac = row_from_bottom as f32 / total_height as f32;
    if frac < 0.25 {
        // Bottom quarter: deep blue-green
        let blend = frac / 0.25;
        Color::Rgb(0, (100.0 + blend * 100.0) as u8, (80.0 + blend * 80.0) as u8)
    } else if frac < 0.5 {
        // Lower mid: green to cyan
        let blend = (frac - 0.25) / 0.25;
        Color::Rgb(0, 200, (160.0 + blend * 60.0) as u8)
    } else if frac < 0.75 {
        // Upper mid: cyan to magenta
        let blend = (frac - 0.5) / 0.25;
        Color::Rgb((blend * 220.0) as u8, (220.0 - blend * 120.0) as u8, 255)
    } else {
        // Top quarter: magenta to hot pink/white
        let blend = (frac - 0.75) / 0.25;
        Color::Rgb(255, (100.0 + blend * 100.0) as u8, (255.0 - blend * 55.0) as u8)
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
            spans.push(Span::styled(*label, Style::default().fg(LABEL_COLOR)));
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
            .draw(|frame| render(frame, &analyzer, 44100, "Test Device"))
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
            .draw(|frame| render(frame, &analyzer, 44100, "Test Device"))
            .unwrap();
    }

    #[test]
    fn test_header_shows_audio() {
        let analyzer = SpectrumAnalyzer::new(4096);
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &analyzer, 44100, "Test Device"))
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
            .draw(|frame| render(frame, &analyzer, 44100, "My Sound Card"))
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
            .draw(|frame| render(frame, &analyzer, 48000, "Test"))
            .unwrap();
        let output = buffer_to_string(&terminal);
        assert!(
            output.contains("48000Hz"),
            "expected sample rate in header, got:\n{output}"
        );
    }

    #[test]
    fn test_bar_color_bottom_quarter() {
        let color = bar_color(0, 20);
        assert!(matches!(color, Color::Rgb(0, _, _)));
    }

    #[test]
    fn test_bar_color_top_quarter() {
        let color = bar_color(19, 20);
        assert!(matches!(color, Color::Rgb(255, _, _)));
    }

    #[test]
    fn test_bar_color_zero_height() {
        let color = bar_color(0, 0);
        assert!(matches!(color, Color::Rgb(0, 200, 100)));
    }

    #[test]
    fn test_render_narrow_terminal_no_panic() {
        let analyzer = SpectrumAnalyzer::new(4096);
        let backend = TestBackend::new(15, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &analyzer, 44100, "Test"))
            .unwrap();
    }
}
