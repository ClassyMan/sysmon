use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};

use crate::app::App;
use crate::collector::human_rate;

const RX_COLOR: Color = Color::Rgb(100, 200, 255);   // cool blue
const TX_COLOR: Color = Color::Rgb(255, 180, 80);    // warm amber
const BORDER_COLOR: Color = Color::DarkGray;
const LABEL_COLOR: Color = Color::Gray;
const ZERO_LINE_COLOR: Color = Color::Rgb(60, 60, 60);

const WAVE_BLOCKS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

pub fn render(frame: &mut Frame, app: &App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),     // header
            Constraint::Min(6),        // waveform
            Constraint::Length(3),     // RX gauge
            Constraint::Length(3),     // TX gauge
        ])
        .split(frame.area());

    draw_header(frame, outer[0], app);
    draw_waveform(frame, outer[1], app);
    draw_rx_gauge(frame, outer[2], app);
    draw_tx_gauge(frame, outer[3], app);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let fast_span = if app.fast_mode {
        Span::styled(" FAST ", Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD))
    } else {
        Span::raw("")
    };

    let hw_info = app.selected_info().map_or_else(String::new, |info| {
        let speed = info.speed_mbps.map_or_else(String::new, |s| {
            if s >= 1000 { format!("{}Gbps", s / 1000) } else { format!("{}Mbps", s) }
        });
        let parts: Vec<&str> = [info.name.as_str(), speed.as_str(), info.ip.as_str(), info.operstate.as_str()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect();
        parts.join(" | ")
    });

    let text = Paragraph::new(Line::from(vec![
        Span::styled(" NET ", Style::default().fg(RX_COLOR).add_modifier(Modifier::BOLD)),
        fast_span,
        Span::styled(
            format!(" {} | {}ms | {}s ", hw_info, app.refresh_ms, app.scrollback_secs),
            Style::default().fg(LABEL_COLOR),
        ),
    ]));
    frame.render_widget(text, area);
}

fn draw_waveform(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BORDER_COLOR));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 2 || inner.width < 4 {
        return;
    }

    let half_height = inner.height / 2;
    let center_row = inner.y + half_height;
    let width = inner.width as usize;

    // Draw center line
    for col in inner.x..inner.right() {
        let cell = frame.buffer_mut().cell_mut((col, center_row));
        if let Some(cell) = cell {
            cell.set_char('─');
            cell.set_style(Style::default().fg(ZERO_LINE_COLOR));
        }
    }

    // Get data
    let mut rx_data = Vec::new();
    let mut tx_data = Vec::new();
    app.rx_history.as_chart_data(&mut rx_data);
    app.tx_history.as_chart_data(&mut tx_data);

    let rx_max = app.rx_y.current().max(1.0);
    let tx_max = app.tx_y.current().max(1.0);

    // RX waveform: grows UPWARD from center
    draw_half_wave(
        frame,
        inner.x,
        center_row,
        width,
        half_height,
        &rx_data,
        rx_max,
        RX_COLOR,
        true,
    );

    // TX waveform: grows DOWNWARD from center
    let bottom_half = inner.height - half_height - 1;
    draw_half_wave(
        frame,
        inner.x,
        center_row,
        width,
        bottom_half,
        &tx_data,
        tx_max,
        TX_COLOR,
        false,
    );

    // Labels on center line
    let rx_label = format!(" RX {} ", app.latest_rates.as_ref()
        .map_or("--".to_string(), |r| human_rate(r.rx_bytes_per_sec)));
    let tx_label = format!(" TX {} ", app.latest_rates.as_ref()
        .map_or("--".to_string(), |r| human_rate(r.tx_bytes_per_sec)));

    let buf = frame.buffer_mut();
    buf.set_string(
        inner.x + 1,
        center_row,
        &rx_label,
        Style::default().fg(RX_COLOR).add_modifier(Modifier::BOLD),
    );
    let tx_label_x = inner.right().saturating_sub(tx_label.len() as u16 + 1);
    buf.set_string(
        tx_label_x,
        center_row,
        &tx_label,
        Style::default().fg(TX_COLOR).add_modifier(Modifier::BOLD),
    );
}

fn draw_half_wave(
    frame: &mut Frame,
    start_x: u16,
    center_row: u16,
    width: usize,
    max_rows: u16,
    data: &[(f64, f64)],
    y_max: f64,
    color: Color,
    upward: bool,
) {
    if data.is_empty() || max_rows == 0 {
        return;
    }

    let data_len = data.len();
    let sub_pixels = max_rows as f64 * 8.0;

    let data_slice = if data_len <= width {
        data
    } else {
        &data[data_len - width..]
    };

    let col_offset = if data_slice.len() < width {
        width - data_slice.len()
    } else {
        0
    };

    let buf = frame.buffer_mut();

    for (idx, &(_, value)) in data_slice.iter().enumerate() {
        let col = start_x + (col_offset + idx) as u16;
        let normalized = (value / y_max).clamp(0.0, 1.0);
        let pixel_height = (normalized * sub_pixels).round() as u16;

        if pixel_height == 0 {
            continue;
        }

        let full_rows = pixel_height / 8;
        let remainder = (pixel_height % 8) as usize;

        for row_offset in 0..full_rows {
            let row = if upward {
                center_row.saturating_sub(1 + row_offset)
            } else {
                center_row + 1 + row_offset
            };
            if let Some(cell) = buf.cell_mut((col, row)) {
                cell.set_char('█');
                cell.set_style(Style::default().fg(color));
            }
        }

        if remainder > 0 {
            let row = if upward {
                center_row.saturating_sub(1 + full_rows)
            } else {
                center_row + 1 + full_rows
            };
            let block_char = if upward {
                WAVE_BLOCKS[remainder]
            } else {
                // For downward, use upper blocks: ▔ doesn't exist in standard,
                // use the complementary lower block
                WAVE_BLOCKS[remainder]
            };
            if let Some(cell) = buf.cell_mut((col, row)) {
                cell.set_char(block_char);
                cell.set_style(Style::default().fg(color));
            }
        }
    }
}

fn draw_rx_gauge(frame: &mut Frame, area: Rect, app: &App) {
    let rx_rate = app.latest_rates.as_ref().map_or(0.0, |r| r.rx_bytes_per_sec);
    let rx_max = app.rx_y.current().max(1.0);
    let pct = (rx_rate / rx_max * 100.0).clamp(0.0, 100.0);
    let label = format!("RX: {}", human_rate(rx_rate));

    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(BORDER_COLOR)))
        .gauge_style(Style::default().fg(RX_COLOR).add_modifier(Modifier::BOLD))
        .label(Span::styled(label, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)))
        .ratio(pct / 100.0);

    frame.render_widget(gauge, area);
}

fn draw_tx_gauge(frame: &mut Frame, area: Rect, app: &App) {
    let tx_rate = app.latest_rates.as_ref().map_or(0.0, |r| r.tx_bytes_per_sec);
    let tx_max = app.tx_y.current().max(1.0);
    let pct = (tx_rate / tx_max * 100.0).clamp(0.0, 100.0);
    let label = format!("TX: {}", human_rate(tx_rate));

    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(BORDER_COLOR)))
        .gauge_style(Style::default().fg(TX_COLOR).add_modifier(Modifier::BOLD))
        .label(Span::styled(label, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)))
        .ratio(pct / 100.0);

    frame.render_widget(gauge, area);
}
