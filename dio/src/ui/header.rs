use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Tabs;

use crate::app::{App, ViewMode};
use crate::ui::theme;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let device_names: Vec<Line> = app
        .devices
        .iter()
        .map(|dev| {
            let suffix = if dev.active { "" } else { " (gone)" };
            Line::raw(format!("{}{}", dev.name, suffix))
        })
        .collect();

    let view_label = match app.view_mode {
        ViewMode::AllDevices => "[all]",
        ViewMode::SingleDevice => "[single]",
        ViewMode::ProcessTable => "[procs]",
    };

    let hw_summary = app.devices.get(app.selected_device)
        .and_then(|dev| app.disk_hw.get(&dev.name))
        .filter(|info| !info.model.is_empty())
        .map(|info| info.summary())
        .unwrap_or_default();

    let fast_span = if app.fast_mode {
        Span::styled(" FAST ", Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD))
    } else {
        Span::raw("")
    };

    let title = Line::from(vec![
        Span::styled(" DISK ", Style::default().fg(theme::READ_COLOR).add_modifier(Modifier::BOLD)),
        fast_span,
        Span::styled(
            format!(" {} | {} | {}ms ",
                hw_summary,
                view_label,
                app.refresh_rate.as_millis(),
            ),
            Style::default().fg(theme::LABEL_COLOR),
        ),
    ]);

    let tabs = Tabs::new(device_names)
        .select(app.selected_device)
        .style(Style::default().fg(theme::LABEL_COLOR).bg(theme::HEADER_BG))
        .highlight_style(
            Style::default()
                .fg(theme::SELECTED_TAB_COLOR)
                .add_modifier(Modifier::BOLD),
        )
        .divider("|")
        .padding(" ", " ")
        .block(
            ratatui::widgets::Block::default()
                .title(title)
                .style(Style::default().bg(theme::HEADER_BG)),
        );

    frame.render_widget(tabs, area);
}
