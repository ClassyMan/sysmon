mod device_view;
mod header;
mod help_overlay;
mod process_view;
pub mod theme;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};

use crate::app::{App, ViewMode};

pub fn render(frame: &mut Frame, app: &App) {
    render_in(frame, frame.area(), app);
}

pub fn render_in(frame: &mut Frame, area: Rect, app: &App) {
    let [header_area, main_area] =
        Layout::vertical([Constraint::Length(3), Constraint::Fill(1)]).areas(area);

    header::render(frame, header_area, app);

    match app.view_mode {
        ViewMode::AllDevices => device_view::render_all(frame, main_area, app),
        ViewMode::SingleDevice => device_view::render_single(frame, main_area, app),
        ViewMode::ProcessTable => process_view::render(frame, main_area, &app.process_table),
    }

    if app.show_help {
        help_overlay::render(frame, frame.area());
    }
}
