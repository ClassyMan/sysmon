pub mod animation;
mod device_view;
mod header;
mod help_overlay;
mod process_view;
pub mod theme;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};

use crate::app::{App, ViewMode};

pub fn render(frame: &mut Frame, app: &mut App) {
    render_in(frame, frame.area(), app);
}

pub fn render_in(frame: &mut Frame, area: Rect, app: &mut App) {
    let [header_area, main_area] =
        Layout::vertical([Constraint::Length(3), Constraint::Fill(1)]).areas(area);

    header::render(frame, header_area, app);

    let term_cols = crossterm::terminal::size().map(|(w, _)| w).unwrap_or(80);
    let base = main_area.height / 2;
    let anim_height = if term_cols < 240 {
        base.max(14).min(main_area.height)
    } else {
        base
    };
    let anim_width = app
        .animation
        .as_ref()
        .map(|a| a.width_for_height(anim_height))
        .unwrap_or(0)
        .min(main_area.width.saturating_sub(1));

    let (anim_area, content_area) = if anim_width > 0 {
        let [a, c] = Layout::horizontal([
            Constraint::Length(anim_width),
            Constraint::Fill(1),
        ])
        .areas(main_area);
        let anim_rect = Rect {
            x: a.x,
            y: a.y,
            width: a.width,
            height: anim_height,
        };
        (Some(anim_rect), c)
    } else {
        (None, main_area)
    };

    match app.view_mode {
        ViewMode::AllDevices => device_view::render_all(frame, content_area, app),
        ViewMode::SingleDevice => device_view::render_single(frame, content_area, app),
        ViewMode::ProcessTable => process_view::render(frame, content_area, &app.process_table),
    }

    if let (Some(area), Some(anim)) = (anim_area, app.animation.as_mut()) {
        anim.set_tint(theme::border_color());
        anim.render(frame, area);
    }

    if app.show_help {
        help_overlay::render(frame, frame.area());
    }
}
