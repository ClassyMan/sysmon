use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;
use sysmon_shared::line_chart::{self, LineChart};
use sysmon_shared::terminal_theme::palette;

fn border_color() -> Color { palette().surface() }
fn label_color() -> Color { palette().label() }
fn total_color() -> Color { palette().bright_green() }

fn usage_color(pct: f64) -> Color {
    let p = palette();
    if pct < 30.0 {
        p.bright_green()
    } else if pct < 60.0 {
        p.bright_yellow()
    } else if pct < 85.0 {
        p.lerp(11, 9, 0.5) // bright yellow→orange midpoint
    } else {
        p.bright_red()
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
        Span::styled(" FAST ", Style::default().fg(palette().bg_color()).bg(palette().bright_yellow()).add_modifier(Modifier::BOLD))
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
        Span::styled(" CPU ", Style::default().fg(total_color()).add_modifier(Modifier::BOLD)),
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
            Style::default().fg(label_color()),
        ),
    ]));
    frame.render_widget(text, area);
}

fn draw_core_bars(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width < 20 || inner.height == 0 {
        return;
    }

    let cols = 4usize;
    let col_width = inner.width as usize / cols;
    let bar_width = col_width.saturating_sub(11); // "XX[████]NNN.N% " → 11 chars fixed

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
        buf.set_string(x_start, row, &label, Style::default().fg(label_color()));

        // Bar fill with pipe characters
        let bar_x = x_start + label.len() as u16;
        let filled = (usage / 100.0 * bar_width as f64).round() as usize;

        for bx in 0..bar_width {
            if bx < filled {
                buf.set_string(bar_x + bx as u16, row, BAR_FILL, Style::default().fg(color));
            } else {
                buf.set_string(bar_x + bx as u16, row, " ", Style::default().fg(palette().mix_with_bg(0, 0.5)));
            }
        }

        // Closing bracket and percentage
        let suffix = format!("]{:>5.1}% ", usage);
        buf.set_string(
            bar_x + bar_width as u16,
            row,
            &suffix,
            Style::default().fg(label_color()),
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
        color: total_color(),
        name: label,
    }])
    .block(
        Block::default()
            .title(" Total Utilization ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color())),
    )
    .x_bounds([0.0, capacity - 1.0])
    .y_bounds([0.0, 100.0])
    .x_labels([format!("{}s", app.scrollback_secs), "0s".to_string()])
    .y_labels(["0%".to_string(), "100%".to_string()]);

    frame.render_widget(chart, area);
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn test_usage_color_low_is_green() {
        assert_eq!(usage_color(10.0), palette().bright_green());
    }

    #[test]
    fn test_usage_color_medium_is_yellow() {
        assert_eq!(usage_color(45.0), palette().bright_yellow());
    }

    #[test]
    fn test_usage_color_high_is_orange() {
        assert_eq!(usage_color(70.0), palette().lerp(11, 9, 0.5));
    }

    #[test]
    fn test_usage_color_critical_is_red() {
        assert_eq!(usage_color(95.0), palette().bright_red());
    }

    #[test]
    fn test_render_empty_app_no_panic() {
        let app = App::with_capacity(100);
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
    }

    #[test]
    fn test_render_with_data_no_panic() {
        let mut app = App::with_capacity(100);
        for value in [10.0, 35.0, 65.0, 90.0] {
            app.total_history.push(value);
        }
        app.core_usages = vec![15.0, 45.0, 75.0, 95.0];
        app.total_usage = 55.0;
        app.temp_celsius = Some(62.0);
        app.load_avg = (1.5, 2.0, 1.8);

        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
    }

    #[test]
    fn test_header_shows_cpu() {
        let app = App::with_capacity(100);
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let output = buffer_to_string(&terminal);
        assert!(output.contains("CPU"), "expected 'CPU' in header, got:\n{output}");
    }

    #[test]
    fn test_header_shows_fast_when_active() {
        let mut app = App::with_capacity(100);
        app.fast_mode = true;
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let output = buffer_to_string(&terminal);
        assert!(output.contains("FAST"), "expected 'FAST' in header, got:\n{output}");
    }

    #[test]
    fn test_header_shows_model_name() {
        let app = App::with_capacity(100);
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let output = buffer_to_string(&terminal);
        assert!(output.contains("Test CPU"), "expected model name in header, got:\n{output}");
    }

    #[test]
    fn test_header_shows_temperature() {
        let mut app = App::with_capacity(100);
        app.temp_celsius = Some(72.0);
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let output = buffer_to_string(&terminal);
        assert!(output.contains("72°C"), "expected temperature in header, got:\n{output}");
    }

    #[test]
    fn test_render_narrow_terminal_no_panic() {
        let app = App::with_capacity(100);
        let backend = TestBackend::new(30, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
    }
}
