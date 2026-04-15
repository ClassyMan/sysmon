use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};

use crate::app::{App, ViewMode};
use crate::collector::human_rate;
use sysmon_shared::line_chart::{self, LineChart};

const DOWN_COLOR: Color = Color::Rgb(100, 230, 220);
const UP_COLOR: Color = Color::Rgb(180, 120, 255);
const BORDER_COLOR: Color = Color::DarkGray;
const LABEL_COLOR: Color = Color::Gray;

pub fn render(frame: &mut Frame, app: &App) {
    render_in(frame, frame.area(), app);
}

pub fn render_in(frame: &mut Frame, area: Rect, app: &App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(6),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(area);

    draw_header(frame, outer[0], app);
    match app.view_mode {
        ViewMode::Charts => draw_charts(frame, outer[1], app),
        ViewMode::Rain => draw_rain(frame, outer[1], app),
    }
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
        let parts: Vec<&str> = [info.name.as_str(), speed.as_str(), info.operstate.as_str()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect();
        parts.join(" | ")
    });

    let text = Paragraph::new(Line::from(vec![
        Span::styled(" NET ", Style::default().fg(DOWN_COLOR).add_modifier(Modifier::BOLD)),
        fast_span,
        Span::styled(
            format!(" {} | {}ms | {}s ", hw_info, app.refresh_ms, app.scrollback_secs),
            Style::default().fg(LABEL_COLOR),
        ),
    ]));
    frame.render_widget(text, area);
}

fn draw_charts(frame: &mut Frame, area: Rect, app: &App) {
    let [rx_area, tx_area] =
        Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
            .areas(area);

    let mut rx_data = Vec::new();
    let mut tx_data = Vec::new();
    app.rx_history.as_chart_data(&mut rx_data);
    app.tx_history.as_chart_data(&mut tx_data);

    let capacity = app.chart_capacity() as f64;

    let rx_max = auto_scale(app.rx_y.current());
    let tx_max = auto_scale(app.tx_y.current());

    let rx_label = app.latest_rates.as_ref().map_or_else(
        || "Down: --".to_string(),
        |r| format!("Down: {}", human_rate(r.rx_bytes_per_sec)),
    );
    let tx_label = app.latest_rates.as_ref().map_or_else(
        || "Up: --".to_string(),
        |r| format!("Up: {}", human_rate(r.tx_bytes_per_sec)),
    );

    let rx_chart = LineChart::new(vec![line_chart::Dataset {
        data: &rx_data,
        color: DOWN_COLOR,
        name: rx_label,
    }])
    .block(
        Block::default()
            .title(" Download ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER_COLOR)),
    )
    .x_bounds([0.0, capacity - 1.0])
    .y_bounds([0.0, rx_max])
    .x_labels([format!("{}s", app.scrollback_secs), "0s".to_string()])
    .y_labels(["0".to_string(), human_rate(rx_max)]);

    let tx_chart = LineChart::new(vec![line_chart::Dataset {
        data: &tx_data,
        color: UP_COLOR,
        name: tx_label,
    }])
    .block(
        Block::default()
            .title(" Upload ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER_COLOR)),
    )
    .x_bounds([0.0, capacity - 1.0])
    .y_bounds([0.0, tx_max])
    .x_labels([format!("{}s", app.scrollback_secs), "0s".to_string()])
    .y_labels(["0".to_string(), human_rate(tx_max)]);

    frame.render_widget(rx_chart, rx_area);
    frame.render_widget(tx_chart, tx_area);
}

fn draw_rain(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BORDER_COLOR));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 2 || inner.width < 2 {
        return;
    }

    let half = inner.height / 2;
    let divider_row = inner.y + half;

    // Draw faint divider line
    let buf = frame.buffer_mut();
    for col in inner.x..inner.right() {
        if let Some(cell) = buf.cell_mut((col, divider_row)) {
            cell.set_char('·');
            cell.set_style(Style::default().fg(Color::Rgb(40, 40, 40)));
        }
    }

    // Draw labels on divider
    let rx_label = format!(" Down {} ", app.latest_rates.as_ref()
        .map_or("--".to_string(), |r| human_rate(r.rx_bytes_per_sec)));
    let tx_label = format!(" Up {} ", app.latest_rates.as_ref()
        .map_or("--".to_string(), |r| human_rate(r.tx_bytes_per_sec)));

    buf.set_string(
        inner.x + 1,
        divider_row,
        &rx_label,
        Style::default().fg(DOWN_COLOR).add_modifier(Modifier::BOLD),
    );
    let tx_x = inner.right().saturating_sub(tx_label.len() as u16 + 1);
    buf.set_string(
        tx_x,
        divider_row,
        &tx_label,
        Style::default().fg(UP_COLOR).add_modifier(Modifier::BOLD),
    );

    // Render matrix streams
    for stream in &app.rain.streams {
        for trail_cell in &stream.trail {
            let abs_row = inner.y + trail_cell.row;

            if abs_row < inner.y || abs_row >= inner.bottom() || abs_row == divider_row {
                continue;
            }
            if stream.col < inner.x || stream.col >= inner.right() {
                continue;
            }

            if let Some(cell) = buf.cell_mut((stream.col, abs_row)) {
                cell.set_char(trail_cell.ch);
                cell.set_style(Style::default().fg(trail_cell.color(stream.is_rx)));
            }
        }
    }
}

fn draw_rx_gauge(frame: &mut Frame, area: Rect, app: &App) {
    let rx_rate = app.latest_rates.as_ref().map_or(0.0, |r| r.rx_bytes_per_sec);
    let rx_max = app.rx_y.current().max(1.0);
    let pct = (rx_rate / rx_max * 100.0).clamp(0.0, 100.0);
    let label = format!("Down: {}", human_rate(rx_rate));

    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(BORDER_COLOR)))
        .gauge_style(Style::default().fg(DOWN_COLOR).add_modifier(Modifier::BOLD))
        .label(Span::styled(label, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)))
        .ratio(pct / 100.0);

    frame.render_widget(gauge, area);
}

fn draw_tx_gauge(frame: &mut Frame, area: Rect, app: &App) {
    let tx_rate = app.latest_rates.as_ref().map_or(0.0, |r| r.tx_bytes_per_sec);
    let tx_max = app.tx_y.current().max(1.0);
    let pct = (tx_rate / tx_max * 100.0).clamp(0.0, 100.0);
    let label = format!("Up: {}", human_rate(tx_rate));

    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(BORDER_COLOR)))
        .gauge_style(Style::default().fg(UP_COLOR).add_modifier(Modifier::BOLD))
        .label(Span::styled(label, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)))
        .ratio(pct / 100.0);

    frame.render_widget(gauge, area);
}

fn auto_scale(observed_max: f64) -> f64 {
    if observed_max <= 0.0 {
        return 1000.0;
    }
    let padded = observed_max * 1.2;
    let steps: &[f64] = &[
        1_000.0, 2_000.0, 5_000.0, 10_000.0, 20_000.0, 50_000.0,
        100_000.0, 200_000.0, 500_000.0,
        1_000_000.0, 2_000_000.0, 5_000_000.0, 10_000_000.0,
        20_000_000.0, 50_000_000.0, 100_000_000.0,
        500_000_000.0, 1_000_000_000.0,
    ];
    steps.iter()
        .find(|&&step| step >= padded)
        .copied()
        .unwrap_or(padded.ceil())
}
