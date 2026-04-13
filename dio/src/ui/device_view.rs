use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::{Block, Borders};

use crate::app::App;
use crate::model::device::DeviceSeries;
use sysmon_shared::ring_buffer::RingBuffer;
use sysmon_shared::line_chart::{self, LineChart};
use crate::model::types::nice_ceil;
use crate::ui::theme;

struct ChartSpec<'a> {
    title: &'a str,
    buf: &'a RingBuffer,
    color: ratatui::style::Color,
    format_value: fn(f64) -> String,
    refresh_ms: f64,
}

pub fn render_all(frame: &mut Frame, area: Rect, app: &App) {
    if app.devices.is_empty() {
        let block = Block::default()
            .title(" No devices found ")
            .borders(Borders::ALL)
            .style(theme::border_style());
        frame.render_widget(block, area);
        return;
    }

    let refresh_ms = app.refresh_rate.as_millis() as f64;

    let constraints: Vec<Constraint> = app
        .devices
        .iter()
        .map(|_| Constraint::Ratio(1, app.devices.len() as u32))
        .collect();

    let areas = Layout::vertical(constraints).split(area);

    for (idx, device_area) in areas.iter().enumerate() {
        if let Some(device) = app.devices.get(idx) {
            render_device(frame, *device_area, device, refresh_ms);
        }
    }
}

pub fn render_single(frame: &mut Frame, area: Rect, app: &App) {
    if let Some(device) = app.devices.get(app.selected_device) {
        let refresh_ms = app.refresh_rate.as_millis() as f64;
        render_device(frame, area, device, refresh_ms);
    }
}

fn render_device(frame: &mut Frame, area: Rect, device: &DeviceSeries, refresh_ms: f64) {
    // 2x2 grid: IOPS (left) | Latency (right), each split into read (top) / write (bottom)
    let [left_area, right_area] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .areas(area);

    let [read_iops_area, write_iops_area] =
        Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
            .areas(left_area);

    let [read_lat_area, write_lat_area] =
        Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
            .areas(right_area);

    let y_max_iops = nice_ceil(device.iops_y.current());
    let y_max_lat = nice_ceil(device.latency_y.current());

    render_single_chart(frame, read_iops_area, &ChartSpec {
        title: &format!("{} — read IOPS", device.name),
        buf: &device.read_iops,
        color: theme::READ_COLOR,
        format_value: crate::model::types::human_iops,
        refresh_ms,
    }, y_max_iops);

    render_single_chart(frame, write_iops_area, &ChartSpec {
        title: &format!("{} — write IOPS", device.name),
        buf: &device.write_iops,
        color: theme::WRITE_COLOR,
        format_value: crate::model::types::human_iops,
        refresh_ms,
    }, y_max_iops);

    render_single_chart(frame, read_lat_area, &ChartSpec {
        title: "read latency",
        buf: &device.read_latency,
        color: theme::READ_COLOR,
        format_value: crate::model::types::human_latency,
        refresh_ms,
    }, y_max_lat);

    render_single_chart(frame, write_lat_area, &ChartSpec {
        title: "write latency",
        buf: &device.write_latency,
        color: theme::WRITE_COLOR,
        format_value: crate::model::types::human_latency,
        refresh_ms,
    }, y_max_lat);
}

fn render_single_chart(frame: &mut Frame, area: Rect, spec: &ChartSpec, y_max: f64) {
    let mut data = Vec::new();
    spec.buf.as_chart_data(&mut data);

    let x_max = (spec.buf.capacity() as f64).max(1.0);
    let total_secs = spec.buf.capacity() as f64 * spec.refresh_ms / 1000.0;
    let current = spec.buf.latest().unwrap_or(0.0);
    let fmt = spec.format_value;

    let chart = LineChart::new(vec![line_chart::Dataset {
        data: &data,
        color: spec.color,
        name: fmt(current),
    }])
    .block(
        Block::default()
            .title(format!(" {} ", spec.title))
            .borders(Borders::ALL)
            .style(theme::border_style()),
    )
    .x_bounds([0.0, x_max])
    .y_bounds([0.0, y_max])
    .x_labels([format!("-{:.0}s", total_secs), "now".to_string()])
    .y_labels(["0".to_string(), fmt(y_max)]);

    frame.render_widget(chart, area);
}
