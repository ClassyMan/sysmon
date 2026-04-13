mod device_view;
mod header;
mod help_overlay;
mod process_view;
pub mod theme;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};

use crate::app::{App, ViewMode};

pub fn render(frame: &mut Frame, app: &App) {
    let [header_area, main_area] =
        Layout::vertical([Constraint::Length(3), Constraint::Fill(1)]).areas(frame.area());

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
