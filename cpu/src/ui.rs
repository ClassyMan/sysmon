use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;
use sysmon_shared::line_chart::{self, LineChart};

const BORDER_COLOR: Color = Color::DarkGray;
const LABEL_COLOR: Color = Color::Gray;
const TOTAL_COLOR: Color = Color::Rgb(120, 255, 180);

fn usage_color(pct: f64) -> Color {
    if pct < 30.0 {
        Color::Rgb(80, 220, 100)
    } else if pct < 60.0 {
        Color::Rgb(220, 220, 60)
    } else if pct < 85.0 {
        Color::Rgb(255, 160, 40)
    } else {
        Color::Rgb(255, 70, 70)
    }
}

const BAR_FILL: &str = "|";

pub fn render(frame: &mut Frame, app: &App) {
    render_in(frame, frame.area(), app);
}

pub fn render_in(frame: &mut Frame, area: Rect, app: &App) {
    let core_count = app.core_histories.len();
    let bar_rows = (core_count + 3) / 4; // 4 cores per row

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(bar_rows as u16 + 2),
            Constraint::Min(6),
        ])
        .split(area);

    draw_header(frame, outer[0], app);
    draw_core_bars(frame, outer[1], app);
    draw_total_chart(frame, outer[2], app);
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

fn draw_core_bars(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BORDER_COLOR));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width < 20 || inner.height == 0 {
        return;
    }

    let core_count = app.core_usages.len();
    let cols = 4usize;
    let col_width = inner.width as usize / cols;
    let bar_width = col_width.saturating_sub(10); // "XX[████] NNN% "

    let buf = frame.buffer_mut();

    for (idx, &usage) in app.core_usages.iter().enumerate() {
        let col_idx = idx % cols;
        let row_idx = idx / cols;

        if row_idx >= inner.height as usize {
            break;
        }

        let x_start = inner.x + (col_idx * col_width) as u16;
        let row = inner.y + row_idx as u16;
        let color = usage_color(usage);

        // Core label: "0["
        let label = format!("{:>2}[", idx);
        buf.set_string(x_start, row, &label, Style::default().fg(LABEL_COLOR));

        // Bar fill with pipe characters
        let bar_x = x_start + label.len() as u16;
        let filled = (usage / 100.0 * bar_width as f64).round() as usize;

        for bx in 0..bar_width {
            if bx < filled {
                buf.set_string(bar_x + bx as u16, row, BAR_FILL, Style::default().fg(color));
            } else {
                buf.set_string(bar_x + bx as u16, row, " ", Style::default().fg(Color::Rgb(40, 40, 40)));
            }
        }

        // Closing bracket and percentage
        let suffix = format!("]{:>5.1}%", usage);
        buf.set_string(
            bar_x + bar_width as u16,
            row,
            &suffix,
            Style::default().fg(LABEL_COLOR),
        );
    }
}

fn draw_total_chart(frame: &mut Frame, area: Rect, app: &App) {
    let mut total_data = Vec::new();
    app.total_history.as_chart_data(&mut total_data);

    let capacity = app.chart_capacity() as f64;
    let label = format!("Total: {:.0}%", app.total_usage);

    let chart = LineChart::new(vec![line_chart::Dataset {
        data: &total_data,
        color: TOTAL_COLOR,
        name: label,
    }])
    .block(
        Block::default()
            .title(" Total Utilization ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER_COLOR)),
    )
    .x_bounds([0.0, capacity - 1.0])
    .y_bounds([0.0, 100.0])
    .x_labels([format!("{}s", app.scrollback_secs), "0s".to_string()])
    .y_labels(["0%".to_string(), "100%".to_string()]);

    frame.render_widget(chart, area);
}
