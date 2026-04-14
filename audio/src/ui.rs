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
        return Color::Green;
    }
    let frac = row_from_bottom as f32 / total_height as f32;
    if frac < 0.4 {
        Color::Rgb(0, 200, 100)
    } else if frac < 0.65 {
        Color::Rgb(0, 200, 200)
    } else if frac < 0.8 {
        Color::Rgb(200, 100, 255)
    } else {
        Color::Rgb(255, 60, 120)
    }
}

fn draw_freq_labels(frame: &mut Frame, area: Rect) {
    let width = area.width as usize;
    if width < 20 {
        return;
    }

    let labels = ["0Hz", "5K", "10K", "15K", "20K"];
    let max_freq = 22050.0_f32;

    let mut spans = Vec::new();
    let mut last_pos = 0usize;

    for label in &labels {
        let freq: f32 = match *label {
            "0Hz" => 0.0,
            "5K" => 5000.0,
            "10K" => 10000.0,
            "15K" => 15000.0,
            "20K" => 20000.0,
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
