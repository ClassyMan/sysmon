use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};

use crate::app::App;
use sysmon_shared::line_chart::{self, LineChart};

const BORDER_COLOR: Color = Color::DarkGray;
const LABEL_COLOR: Color = Color::Gray;
const TOTAL_COLOR: Color = Color::Rgb(120, 255, 180);

const CORE_COLORS: [Color; 8] = [
    Color::Rgb(255, 120, 120),
    Color::Rgb(255, 180, 80),
    Color::Rgb(255, 255, 100),
    Color::Rgb(120, 255, 120),
    Color::Rgb(100, 220, 255),
    Color::Rgb(140, 140, 255),
    Color::Rgb(220, 140, 255),
    Color::Rgb(255, 140, 200),
];

fn core_color(idx: usize) -> Color {
    CORE_COLORS[idx % CORE_COLORS.len()]
}

pub fn render(frame: &mut Frame, app: &App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),      // header
            Constraint::Length(3),      // total gauge
            Constraint::Min(6),         // per-core charts
        ])
        .split(frame.area());

    draw_header(frame, outer[0], app);
    draw_total_gauge(frame, outer[1], app);
    draw_core_charts(frame, outer[2], app);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let fast_span = if app.fast_mode {
        Span::styled(" FAST ", Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD))
    } else {
        Span::raw("")
    };

    let temp_str = app.temp_celsius.map_or_else(String::new, |t| format!(" | {:.0}°C", t));
    let freq_str = if app.cpu_info.max_freq_mhz > 0.0 {
        format!(" | max {:.0}MHz", app.cpu_info.max_freq_mhz)
    } else {
        String::new()
    };
    let load = app.load_avg;

    let text = Paragraph::new(Line::from(vec![
        Span::styled(" CPU ", Style::default().fg(TOTAL_COLOR).add_modifier(Modifier::BOLD)),
        fast_span,
        Span::styled(
            format!(
                " {} | {}C/{}T{}{} | load {:.2} {:.2} {:.2} | {}ms ",
                app.cpu_info.model,
                app.cpu_info.cores,
                app.cpu_info.threads,
                freq_str,
                temp_str,
                load.0, load.1, load.2,
                app.refresh_ms,
            ),
            Style::default().fg(LABEL_COLOR),
        ),
    ]));
    frame.render_widget(text, area);
}

fn draw_total_gauge(frame: &mut Frame, area: Rect, app: &App) {
    let label = format!("Total: {:.0}%", app.total_usage);
    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(BORDER_COLOR)))
        .gauge_style(Style::default().fg(TOTAL_COLOR).add_modifier(Modifier::BOLD))
        .label(Span::styled(label, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)))
        .ratio((app.total_usage / 100.0).clamp(0.0, 1.0));
    frame.render_widget(gauge, area);
}

fn draw_core_charts(frame: &mut Frame, area: Rect, app: &App) {
    let core_count = app.core_histories.len();
    if core_count == 0 {
        return;
    }

    // Arrange in a grid: 2 columns, as many rows as needed
    let rows_needed = (core_count + 1) / 2;
    let row_constraints: Vec<Constraint> = (0..rows_needed)
        .map(|_| Constraint::Ratio(1, rows_needed as u32))
        .collect();

    let row_areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(area);

    let capacity = app.chart_capacity() as f64;
    let x_labels = [format!("{}s", app.scrollback_secs), "0s".to_string()];

    for row_idx in 0..rows_needed {
        let [left_area, right_area] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(row_areas[row_idx]);

        let left_core = row_idx * 2;
        let right_core = row_idx * 2 + 1;

        draw_single_core(frame, left_area, app, left_core, capacity, &x_labels);
        if right_core < core_count {
            draw_single_core(frame, right_area, app, right_core, capacity, &x_labels);
        }
    }
}

fn draw_single_core(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    core_idx: usize,
    capacity: f64,
    x_labels: &[String; 2],
) {
    let mut data = Vec::new();
    app.core_histories[core_idx].as_chart_data(&mut data);

    let usage = app.core_usages.get(core_idx).copied().unwrap_or(0.0);
    let color = core_color(core_idx);

    let freq = crate::collector::read_core_freq_mhz(core_idx);
    let freq_str = freq.map_or_else(String::new, |f| format!(" {:.0}MHz", f));

    let label = format!("{:.0}%{}", usage, freq_str);

    let chart = LineChart::new(vec![line_chart::Dataset {
        data: &data,
        color,
        name: label,
    }])
    .block(
        Block::default()
            .title(format!(" core {} ", core_idx))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER_COLOR)),
    )
    .x_bounds([0.0, capacity - 1.0])
    .y_bounds([0.0, 100.0])
    .x_labels(x_labels.clone())
    .y_labels(["0%".to_string(), "100%".to_string()]);

    frame.render_widget(chart, area);
}
