use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{App, ViewMode};
use crate::collector::human_rate;
use sysmon_shared::line_chart::{self, LineChart};
use sysmon_shared::terminal_theme::palette;

fn down_color() -> Color { palette().bright_green() }
fn up_color() -> Color { palette().bright_yellow() }
fn border_color() -> Color { palette().muted_label() }
fn label_color() -> Color { palette().muted_label() }

pub fn render(frame: &mut Frame, app: &App) {
    render_in(frame, frame.area(), app);
}

pub fn render_in(frame: &mut Frame, area: Rect, app: &App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(4)])
        .split(area);

    draw_header(frame, outer[0], app);
    match app.view_mode {
        ViewMode::Charts => draw_charts(frame, outer[1], app),
        ViewMode::Rain => draw_rain(frame, outer[1], app),
    }
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let fast_span = if app.fast_mode {
        Span::styled(" FAST ", Style::default().fg(palette().bg_color()).bg(palette().bright_yellow()).add_modifier(Modifier::BOLD))
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
        Span::styled(" NET ", Style::default().fg(down_color()).add_modifier(Modifier::BOLD)),
        fast_span,
        Span::styled(
            format!(" {} | {}ms | {}s ", hw_info, app.refresh_ms, app.scrollback_secs),
            Style::default().fg(label_color()),
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
        color: down_color(),
        name: rx_label,
    }])
    .block(
        Block::default()
            .title(" Download ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color())),
    )
    .x_bounds([0.0, capacity - 1.0])
    .y_bounds([0.0, rx_max])
    .x_labels([format!("{}s", app.scrollback_secs), "0s".to_string()])
    .y_labels(["0".to_string(), human_rate(rx_max)]);

    let tx_chart = LineChart::new(vec![line_chart::Dataset {
        data: &tx_data,
        color: up_color(),
        name: tx_label,
    }])
    .block(
        Block::default()
            .title(" Upload ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color())),
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
        .border_style(Style::default().fg(border_color()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 2 || inner.width < 2 {
        return;
    }

    app.rain_panel_size.set(Some((inner.width, inner.height)));

    let half = inner.height / 2;
    let divider_row = inner.y + half;

    // Draw faint divider line
    let buf = frame.buffer_mut();
    for col in inner.x..inner.right() {
        if let Some(cell) = buf.cell_mut((col, divider_row)) {
            cell.set_char('·');
            cell.set_style(Style::default().fg(palette().mix_with_bg(0, 0.5)));
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
        Style::default().fg(down_color()).add_modifier(Modifier::BOLD),
    );
    let tx_x = inner.right().saturating_sub(tx_label.len() as u16 + 1);
    buf.set_string(
        tx_x,
        divider_row,
        &tx_label,
        Style::default().fg(up_color()).add_modifier(Modifier::BOLD),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::NetRates;
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
    fn test_auto_scale_zero_returns_default() {
        assert_eq!(auto_scale(0.0), 1000.0);
        assert_eq!(auto_scale(-5.0), 1000.0);
    }

    #[test]
    fn test_auto_scale_snaps_to_step() {
        assert_eq!(auto_scale(800.0), 1_000.0);
        assert_eq!(auto_scale(1500.0), 2_000.0);
        assert_eq!(auto_scale(3000.0), 5_000.0);
    }

    #[test]
    fn test_auto_scale_large_value() {
        assert_eq!(auto_scale(400_000_000.0), 500_000_000.0);
    }

    #[test]
    fn test_auto_scale_beyond_steps_uses_ceil() {
        let result = auto_scale(2_000_000_000.0);
        assert!(result > 1_000_000_000.0);
    }

    #[test]
    fn test_render_charts_empty_no_panic() {
        let app = App::with_capacity(100);
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
    }

    #[test]
    fn test_render_charts_with_data_no_panic() {
        let mut app = App::with_capacity(100);
        for rate in [1000.0, 5000.0, 2000.0, 8000.0] {
            app.rx_history.push(rate);
            app.tx_history.push(rate * 0.3);
        }
        app.rx_y.update(8000.0);
        app.tx_y.update(2400.0);
        app.latest_rates = Some(NetRates {
            rx_bytes_per_sec: 8000.0,
            tx_bytes_per_sec: 2400.0,
            rx_packets_per_sec: 50.0,
            tx_packets_per_sec: 20.0,
        });

        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
    }

    #[test]
    fn test_header_shows_net() {
        let app = App::with_capacity(100);
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let output = buffer_to_string(&terminal);
        assert!(output.contains("NET"), "expected 'NET' in header, got:\n{output}");
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
    fn test_render_rain_mode_no_panic() {
        let mut app = App::with_capacity(100);
        app.view_mode = ViewMode::Rain;
        app.latest_rates = Some(NetRates {
            rx_bytes_per_sec: 50_000.0,
            tx_bytes_per_sec: 10_000.0,
            rx_packets_per_sec: 100.0,
            tx_packets_per_sec: 40.0,
        });
        app.rain.tick(120, 30, 50_000.0, 10_000.0);

        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
    }

    #[test]
    fn test_render_narrow_terminal_no_panic() {
        let app = App::with_capacity(100);
        let backend = TestBackend::new(30, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
    }

    #[test]
    fn test_charts_show_download_upload_labels() {
        let app = App::with_capacity(100);
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let output = buffer_to_string(&terminal);
        assert!(output.contains("Download"), "expected 'Download' chart title, got:\n{output}");
        assert!(output.contains("Upload"), "expected 'Upload' chart title, got:\n{output}");
    }
}
