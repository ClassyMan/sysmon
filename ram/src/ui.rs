use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};

use crate::app::App;
use crate::collector::{human_count, human_rate};
use sysmon_shared::line_chart::{self, LineChart};
use sysmon_shared::terminal_theme::palette;

fn alloc_color() -> Color { palette().bright_yellow() }
fn free_color() -> Color { palette().bright_cyan() }
fn swapin_color() -> Color { palette().bright_red() }
fn swapout_color() -> Color { palette().bright_red() }
fn fault_color() -> Color { palette().bright_green() }
fn major_fault_color() -> Color { palette().bright_red() }
fn psi_some_color() -> Color { palette().bright_yellow() }
fn psi_full_color() -> Color { palette().bright_red() }
fn border_color() -> Color { palette().surface() }
fn label_color() -> Color { palette().label() }

pub fn render(frame: &mut Frame, app: &App) {
    render_in(frame, frame.area(), app);
}

pub fn render_in(frame: &mut Frame, area: Rect, app: &App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(area);

    draw_header(frame, outer[0], app);

    if area.height < 24 {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(outer[1]);
        draw_row(frame, rows[0], app, RowKind::Ram);
        draw_row(frame, rows[1], app, RowKind::Psi);
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(outer[1]);

    draw_row(frame, rows[0], app, RowKind::Ram);
    draw_row(frame, rows[1], app, RowKind::Swap);
    draw_row(frame, rows[2], app, RowKind::Faults);
    draw_row(frame, rows[3], app, RowKind::Psi);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let mode_span = if app.fast_mode {
        Span::styled(" FAST ", Style::default().fg(palette().bg_color()).bg(palette().bright_yellow()).add_modifier(Modifier::BOLD))
    } else {
        Span::raw("")
    };

    let text = Paragraph::new(Line::from(vec![
        Span::styled(" RAM ", Style::default().fg(alloc_color()).add_modifier(Modifier::BOLD)),
        mode_span,
        Span::styled(
            format!(" {} | {}ms | {}s ", app.hardware.summary, app.refresh_ms, app.scrollback_secs),
            Style::default().fg(label_color()),
        ),
    ]));
    frame.render_widget(text, area);
}

enum RowKind { Ram, Swap, Faults, Psi }

fn draw_row(frame: &mut Frame, area: Rect, app: &App, kind: RowKind) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(30), Constraint::Length(30)])
        .split(area);

    match kind {
        RowKind::Ram => {
            draw_throughput_chart(frame, cols[0], app);
            draw_ram_gauge(frame, cols[1], app);
        }
        RowKind::Swap => {
            draw_swap_io_chart(frame, cols[0], app);
            draw_swap_gauge(frame, cols[1], app);
        }
        RowKind::Faults => {
            draw_faults_chart(frame, cols[0], app);
            draw_dirty_gauge(frame, cols[1], app);
        }
        RowKind::Psi => {
            draw_psi_chart(frame, cols[0], app);
            draw_psi_gauge(frame, cols[1], app);
        }
    }
}

fn draw_throughput_chart(frame: &mut Frame, area: Rect, app: &App) {
    let mut alloc_data = Vec::new();
    let mut free_data = Vec::new();
    app.alloc_history.as_chart_data(&mut alloc_data);
    app.free_history.as_chart_data(&mut free_data);

    let alloc_label = app.latest_rates.as_ref().map_or_else(
        || "alloc: --".to_string(),
        |r| format!("alloc: {}", human_rate(r.alloc_mb_per_sec)),
    );
    let free_label = app.latest_rates.as_ref().map_or_else(
        || "free: --".to_string(),
        |r| format!("free:  {}", human_rate(r.free_mb_per_sec)),
    );
    let y_max = auto_scale_max(app.throughput_y.current());

    render_split_chart(frame, area, app, y_max, |v| human_rate(v),
        " alloc ", line_chart::Dataset { data: &alloc_data, color: alloc_color(), name: alloc_label },
        " free ",  line_chart::Dataset { data: &free_data, color: free_color(), name: free_label },
    );
}

fn draw_swap_io_chart(frame: &mut Frame, area: Rect, app: &App) {
    let mut swapin_data = Vec::new();
    let mut swapout_data = Vec::new();
    app.swapin_history.as_chart_data(&mut swapin_data);
    app.swapout_history.as_chart_data(&mut swapout_data);

    let swapin_label = app.latest_rates.as_ref().map_or_else(
        || "in: --".to_string(),
        |r| format!("in:  {}", human_rate(r.swapin_mb_per_sec)),
    );
    let swapout_label = app.latest_rates.as_ref().map_or_else(
        || "out: --".to_string(),
        |r| format!("out: {}", human_rate(r.swapout_mb_per_sec)),
    );
    let y_max = auto_scale_max(app.swap_io_y.current());

    render_split_chart(frame, area, app, y_max, |v| human_rate(v),
        " swap in ",  line_chart::Dataset { data: &swapin_data, color: swapin_color(), name: swapin_label },
        " swap out ", line_chart::Dataset { data: &swapout_data, color: swapout_color(), name: swapout_label },
    );
}

fn draw_faults_chart(frame: &mut Frame, area: Rect, app: &App) {
    let mut fault_data = Vec::new();
    let mut major_data = Vec::new();
    app.fault_history.as_chart_data(&mut fault_data);
    app.major_fault_history.as_chart_data(&mut major_data);

    let fault_label = app.latest_rates.as_ref().map_or_else(
        || "minor: --".to_string(),
        |r| format!("minor: {}", human_count(r.fault_per_sec)),
    );
    let major_label = app.latest_rates.as_ref().map_or_else(
        || "major: --".to_string(),
        |r| format!("major: {}", human_count(r.major_fault_per_sec)),
    );
    let y_max = auto_scale_max(app.faults_y.current());

    render_split_chart(frame, area, app, y_max, |v| human_count(v),
        " minor ", line_chart::Dataset { data: &fault_data, color: fault_color(), name: fault_label },
        " major ", line_chart::Dataset { data: &major_data, color: major_fault_color(), name: major_label },
    );
}

fn draw_psi_chart(frame: &mut Frame, area: Rect, app: &App) {
    let mut some_data = Vec::new();
    let mut full_data = Vec::new();
    app.psi_some_history.as_chart_data(&mut some_data);
    app.psi_full_history.as_chart_data(&mut full_data);

    let some_label = app.latest_psi.as_ref().map_or_else(
        || "some: --".to_string(),
        |p| p.some_label(),
    );
    let full_label = app.latest_psi.as_ref().map_or_else(
        || "full: --".to_string(),
        |p| p.full_label(),
    );
    let y_max = auto_scale_pct(app.psi_y.current());

    render_split_chart(frame, area, app, y_max, |v| format!("{:.0}%", v),
        " some ", line_chart::Dataset { data: &some_data, color: psi_some_color(), name: some_label },
        " full ", line_chart::Dataset { data: &full_data, color: psi_full_color(), name: full_label },
    );
}

fn render_split_chart(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    y_max: f64,
    format_y: impl Fn(f64) -> String,
    left_title: &str,
    left_ds: line_chart::Dataset<'_>,
    right_title: &str,
    right_ds: line_chart::Dataset<'_>,
) {
    let capacity = app.chart_capacity() as f64;
    let x_labels = [format!("{}s", app.scrollback_secs), "0s".to_string()];
    let y_labels = ["0".to_string(), format_y(y_max)];

    let [left_area, right_area] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .areas(area);

    let left_chart = LineChart::new(vec![left_ds])
        .block(
            Block::default()
                .title(left_title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color())),
        )
        .x_bounds([0.0, capacity - 1.0])
        .y_bounds([0.0, y_max])
        .x_labels(x_labels.clone())
        .y_labels(y_labels.clone());

    let right_chart = LineChart::new(vec![right_ds])
        .block(
            Block::default()
                .title(right_title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color())),
        )
        .x_bounds([0.0, capacity - 1.0])
        .y_bounds([0.0, y_max])
        .x_labels(x_labels)
        .y_labels(y_labels);

    frame.render_widget(left_chart, left_area);
    frame.render_widget(right_chart, right_area);
}

fn draw_ram_gauge(frame: &mut Frame, area: Rect, app: &App) {
    let (pct, label) = app.latest_info.as_ref().map_or_else(
        || (0.0, "RAM: --%".to_string()),
        |info| (info.ram_pct(), info.ram_label()),
    );
    draw_gauge(frame, area, &label, pct, alloc_color());
}

fn draw_swap_gauge(frame: &mut Frame, area: Rect, app: &App) {
    let (pct, label) = app.latest_info.as_ref().map_or_else(
        || (0.0, "SWP: --%".to_string()),
        |info| (info.swap_pct(), info.swap_label()),
    );
    draw_gauge(frame, area, &label, pct, swapin_color());
}

fn draw_dirty_gauge(frame: &mut Frame, area: Rect, app: &App) {
    let label = app.latest_info.as_ref().map_or_else(
        || "Dirty+WB: --".to_string(),
        |info| info.dirty_label(),
    );
    let dirty_kb = app.latest_info.as_ref().map_or(0, |i| i.dirty_writeback_kb());
    let ram_total = app.latest_info.as_ref().map_or(1, |i| i.ram_total_kb.max(1));
    let pct = (dirty_kb as f64 / ram_total as f64) * 100.0;
    draw_gauge(frame, area, &label, pct, fault_color());
}

fn draw_psi_gauge(frame: &mut Frame, area: Rect, app: &App) {
    let label = app.latest_psi.as_ref().map_or_else(
        || "PSI: --".to_string(),
        |psi| psi.summary_label(),
    );
    let pct = app.latest_psi.as_ref().map_or(0.0, |psi| psi.severity_pct());
    let color = if pct >= 10.0 { psi_full_color() }
        else if pct >= 1.0 { psi_some_color() }
        else { palette().bright_green() };
    draw_gauge(frame, area, &label, pct, color);
}

fn draw_gauge(frame: &mut Frame, area: Rect, label: &str, pct: f64, color: Color) {
    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color())),
        )
        .gauge_style(Style::default().fg(color).add_modifier(Modifier::BOLD))
        .label(Span::styled(
            label,
            Style::default().fg(palette().fg_color()).add_modifier(Modifier::BOLD),
        ))
        .ratio(pct.clamp(0.0, 100.0) / 100.0);

    frame.render_widget(gauge, area);
}

fn auto_scale_max(observed_max: f64) -> f64 {
    if observed_max <= 0.0 {
        return 10.0;
    }
    let padded = observed_max * 1.2;
    let steps: &[f64] = &[
        1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0,
        1000.0, 2000.0, 5000.0, 10000.0, 50000.0, 100000.0,
    ];
    steps.iter()
        .find(|&&step| step >= padded)
        .copied()
        .unwrap_or(padded.ceil())
}

fn auto_scale_pct(observed_max: f64) -> f64 {
    if observed_max <= 0.0 {
        return 5.0;
    }
    let padded = observed_max * 1.2;
    let steps: &[f64] = &[1.0, 2.0, 5.0, 10.0, 25.0, 50.0, 100.0];
    steps.iter()
        .find(|&&step| step >= padded)
        .copied()
        .unwrap_or(100.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::{MemInfo, PsiSnapshot, VmRates};
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
    fn test_auto_scale_max() {
        assert_eq!(auto_scale_max(0.0), 10.0);
        assert_eq!(auto_scale_max(0.8), 1.0);
        assert_eq!(auto_scale_max(3.5), 5.0);
        assert_eq!(auto_scale_max(42.0), 100.0);
        assert_eq!(auto_scale_max(800.0), 1000.0);
    }

    #[test]
    fn test_auto_scale_pct() {
        assert_eq!(auto_scale_pct(0.0), 5.0);
        assert_eq!(auto_scale_pct(0.5), 1.0);
        assert_eq!(auto_scale_pct(3.0), 5.0);
        assert_eq!(auto_scale_pct(80.0), 100.0);
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
        for value in [10.0, 20.0, 30.0, 25.0, 15.0] {
            app.alloc_history.push(value);
            app.free_history.push(value * 0.5);
        }
        app.latest_info = Some(MemInfo {
            ram_total_kb: 32_000_000,
            ram_used_kb: 16_000_000,
            swap_total_kb: 8_000_000,
            swap_used_kb: 1_000_000,
            dirty_kb: 512,
            writeback_kb: 128,
        });
        app.latest_rates = Some(VmRates {
            alloc_mb_per_sec: 150.0,
            free_mb_per_sec: 80.0,
            fault_per_sec: 5000.0,
            major_fault_per_sec: 2.0,
            swapin_mb_per_sec: 0.0,
            swapout_mb_per_sec: 0.0,
        });
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
    }

    #[test]
    fn test_header_shows_ram() {
        let app = App::with_capacity(100);
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();

        let output = buffer_to_string(&terminal);
        assert!(output.contains("RAM"), "expected 'RAM' in header, got:\n{output}");
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
    fn test_gauge_shows_percentage() {
        let mut app = App::with_capacity(100);
        app.latest_info = Some(MemInfo {
            ram_total_kb: 16_000_000,
            ram_used_kb: 8_000_000,
            swap_total_kb: 4_000_000,
            swap_used_kb: 0,
            dirty_kb: 0,
            writeback_kb: 0,
        });
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();

        let output = buffer_to_string(&terminal);
        assert!(output.contains("50%"), "expected '50%' in gauge, got:\n{output}");
    }

    #[test]
    fn test_psi_gauge_shows_healthy() {
        let mut app = App::with_capacity(100);
        app.latest_psi = Some(PsiSnapshot {
            some_avg10: 0.0,
            full_avg10: 0.0,
            some_total_us: 0,
            full_total_us: 0,
        });
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();

        let output = buffer_to_string(&terminal);
        assert!(output.contains("healthy"), "expected 'healthy' in PSI gauge, got:\n{output}");
    }
}
