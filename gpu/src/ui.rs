use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Gauge, Paragraph, Row, Table};

use crate::app::App;
use sysmon_shared::line_chart::{self, LineChart};

const GPU_COLOR: Color = Color::Rgb(120, 255, 180);
const MEM_COLOR: Color = Color::Rgb(100, 200, 255);
const TEMP_COLOR: Color = Color::Rgb(255, 140, 100);
const POWER_COLOR: Color = Color::Rgb(255, 220, 100);
const VRAM_COLOR: Color = Color::Rgb(180, 120, 255);
const BORDER_COLOR: Color = Color::DarkGray;
const LABEL_COLOR: Color = Color::Gray;

pub fn render(frame: &mut Frame, app: &App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),      // header
            Constraint::Length(3),      // VRAM + power gauges
            Constraint::Min(8),         // charts
            Constraint::Length(10),     // process table
        ])
        .split(frame.area());

    draw_header(frame, outer[0], app);
    draw_gauges(frame, outer[1], app);
    draw_charts(frame, outer[2], app);
    draw_processes(frame, outer[3], app);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let fast_span = if app.fast_mode {
        Span::styled(" FAST ", Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD))
    } else {
        Span::raw("")
    };

    let hw_info = app.latest.as_ref().map_or_else(
        || "Waiting for nvidia-smi...".to_string(),
        |snap| snap.header_line(),
    );

    let text = Paragraph::new(Line::from(vec![
        Span::styled(" GPU ", Style::default().fg(GPU_COLOR).add_modifier(Modifier::BOLD)),
        fast_span,
        Span::styled(
            format!(" {} | {}ms ", hw_info, app.refresh_ms),
            Style::default().fg(LABEL_COLOR),
        ),
    ]));
    frame.render_widget(text, area);
}

fn draw_gauges(frame: &mut Frame, area: Rect, app: &App) {
    let [vram_area, power_area] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .areas(area);

    let vram_pct = app.latest.as_ref().map_or(0.0, |s| s.vram_pct());
    let vram_label = app.latest.as_ref().map_or_else(
        || "VRAM: --".to_string(),
        |s| s.vram_label(),
    );

    let vram_gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(BORDER_COLOR)))
        .gauge_style(Style::default().fg(VRAM_COLOR).add_modifier(Modifier::BOLD))
        .label(Span::styled(&vram_label, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)))
        .ratio(vram_pct.clamp(0.0, 100.0) / 100.0);
    frame.render_widget(vram_gauge, vram_area);

    let power_pct = app.latest.as_ref().map_or(0.0, |s| s.power_pct());
    let power_label = app.latest.as_ref().map_or_else(
        || "Power: --".to_string(),
        |s| format!("Power: {:.0}W / {:.0}W ({:.0}%)", s.power_watts, s.power_limit_watts, power_pct),
    );

    let power_gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(BORDER_COLOR)))
        .gauge_style(Style::default().fg(POWER_COLOR).add_modifier(Modifier::BOLD))
        .label(Span::styled(&power_label, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)))
        .ratio(power_pct.clamp(0.0, 100.0) / 100.0);
    frame.render_widget(power_gauge, power_area);
}

fn draw_charts(frame: &mut Frame, area: Rect, app: &App) {
    let [left_area, right_area] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .areas(area);

    let [gpu_area, temp_area] =
        Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
            .areas(left_area);

    let [mem_area, power_area] =
        Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
            .areas(right_area);

    let capacity = app.chart_capacity() as f64;
    let x_labels = [format!("{}s", app.scrollback_secs), "0s".to_string()];

    // GPU utilization
    let mut gpu_data = Vec::new();
    app.gpu_util_history.as_chart_data(&mut gpu_data);
    let gpu_label = app.latest.as_ref().map_or_else(
        || "GPU: --%".to_string(),
        |s| format!("GPU: {:.0}%", s.gpu_util_pct),
    );
    render_chart(frame, gpu_area, " GPU Utilization ", &gpu_data, GPU_COLOR,
        &gpu_label, capacity, 100.0, &x_labels, "100%");

    // Memory bandwidth utilization
    let mut mem_data = Vec::new();
    app.mem_util_history.as_chart_data(&mut mem_data);
    let mem_label = app.latest.as_ref().map_or_else(
        || "MEM: --%".to_string(),
        |s| format!("MEM: {:.0}%", s.mem_util_pct),
    );
    render_chart(frame, mem_area, " Memory Bus ", &mem_data, MEM_COLOR,
        &mem_label, capacity, 100.0, &x_labels, "100%");

    // Temperature
    let mut temp_data = Vec::new();
    app.temp_history.as_chart_data(&mut temp_data);
    let temp_label = app.latest.as_ref().map_or_else(
        || "Temp: --".to_string(),
        |s| format!("Temp: {:.0}°C", s.temp_celsius),
    );
    render_chart(frame, temp_area, " Temperature ", &temp_data, TEMP_COLOR,
        &temp_label, capacity, 100.0, &x_labels, "100°C");

    // Power
    let mut power_data = Vec::new();
    app.power_history.as_chart_data(&mut power_data);
    let power_max = app.latest.as_ref().map_or(350.0, |s| s.power_limit_watts);
    let power_label = app.latest.as_ref().map_or_else(
        || "Power: --".to_string(),
        |s| format!("Power: {:.0}W", s.power_watts),
    );
    render_chart(frame, power_area, " Power Draw ", &power_data, POWER_COLOR,
        &power_label, capacity, power_max, &x_labels, &format!("{:.0}W", power_max));
}

fn render_chart(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    data: &[(f64, f64)],
    color: Color,
    label: &str,
    capacity: f64,
    y_max: f64,
    x_labels: &[String; 2],
    y_max_label: &str,
) {
    let chart = LineChart::new(vec![line_chart::Dataset {
        data,
        color,
        name: label.to_string(),
    }])
    .block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER_COLOR)),
    )
    .x_bounds([0.0, capacity - 1.0])
    .y_bounds([0.0, y_max])
    .x_labels(x_labels.clone())
    .y_labels(["0".to_string(), y_max_label.to_string()]);

    frame.render_widget(chart, area);
}

fn draw_processes(frame: &mut Frame, area: Rect, app: &App) {
    let header_cells = ["PID", "TYPE", "GPU%", "MEM%", "VRAM", "Command"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(GPU_COLOR).add_modifier(Modifier::BOLD)));
    let header = Row::new(header_cells).height(1);

    let rows = app.processes.iter().map(|proc| {
        let gpu_str = proc.gpu_pct.map_or("-".to_string(), |p| format!("{:.0}%", p));
        let mem_str = proc.mem_pct.map_or("-".to_string(), |p| format!("{:.0}%", p));
        let vram_str = format!("{}MiB", proc.vram_mib);

        Row::new(vec![
            Cell::from(proc.pid.to_string()),
            Cell::from(proc.proc_type.clone()),
            Cell::from(gpu_str),
            Cell::from(mem_str),
            Cell::from(vram_str),
            Cell::from(proc.name.clone()),
        ])
        .style(Style::default().fg(LABEL_COLOR))
    });

    let widths = [
        Constraint::Length(8),
        Constraint::Length(5),
        Constraint::Length(6),
        Constraint::Length(6),
        Constraint::Length(9),
        Constraint::Min(20),
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .title(" Processes ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BORDER_COLOR)),
        );

    frame.render_widget(table, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::{GpuProcess, GpuSnapshot};
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

    fn test_snapshot() -> GpuSnapshot {
        GpuSnapshot {
            name: "NVIDIA GeForce RTX 3090".to_string(),
            driver: "590.48.01".to_string(),
            pcie_gen: "4".to_string(),
            pcie_width: "16".to_string(),
            vram_total_mib: 24576.0,
            vram_used_mib: 4096.0,
            gpu_util_pct: 45.0,
            mem_util_pct: 30.0,
            temp_celsius: 65.0,
            power_watts: 180.0,
            power_limit_watts: 350.0,
            clock_gpu_mhz: 1800.0,
            clock_mem_mhz: 9501.0,
            fan_pct: 55.0,
        }
    }

    #[test]
    fn test_render_empty_app_no_panic() {
        let app = App::with_capacity(100);
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
    }

    #[test]
    fn test_render_with_snapshot_no_panic() {
        let mut app = App::with_capacity(100);
        app.latest = Some(test_snapshot());
        for value in [20.0, 40.0, 60.0, 80.0] {
            app.gpu_util_history.push(value);
            app.mem_util_history.push(value * 0.5);
            app.temp_history.push(55.0 + value * 0.2);
            app.power_history.push(100.0 + value);
        }
        app.vram_pct_history.push(16.7);

        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
    }

    #[test]
    fn test_header_shows_gpu() {
        let app = App::with_capacity(100);
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let output = buffer_to_string(&terminal);
        assert!(output.contains("GPU"), "expected 'GPU' in header, got:\n{output}");
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
    fn test_header_shows_gpu_name_with_snapshot() {
        let mut app = App::with_capacity(100);
        app.latest = Some(test_snapshot());
        let backend = TestBackend::new(160, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let output = buffer_to_string(&terminal);
        assert!(output.contains("RTX 3090"), "expected GPU name in header, got:\n{output}");
    }

    #[test]
    fn test_render_with_processes_no_panic() {
        let mut app = App::with_capacity(100);
        app.latest = Some(test_snapshot());
        app.processes = vec![
            GpuProcess {
                pid: 1234,
                name: "Xorg".to_string(),
                proc_type: "G".to_string(),
                vram_mib: 512,
                gpu_pct: Some(15.0),
                mem_pct: Some(10.0),
            },
            GpuProcess {
                pid: 5678,
                name: "steam".to_string(),
                proc_type: "C+G".to_string(),
                vram_mib: 2048,
                gpu_pct: None,
                mem_pct: None,
            },
        ];

        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let output = buffer_to_string(&terminal);
        assert!(output.contains("Processes"), "expected 'Processes' table, got:\n{output}");
    }

    #[test]
    fn test_render_narrow_terminal_no_panic() {
        let mut app = App::with_capacity(100);
        app.latest = Some(test_snapshot());
        let backend = TestBackend::new(40, 15);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
    }
}
