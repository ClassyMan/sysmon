use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
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
        ViewMode::AllDevices => " [all]",
        ViewMode::SingleDevice => " [single]",
        ViewMode::ProcessTable => " [procs]",
    };

    let fast_tag = if app.fast_mode { " FAST" } else { "" };
    let title = format!(" dio{} | {}ms{} ", view_label, app.refresh_rate.as_millis(), fast_tag);

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
