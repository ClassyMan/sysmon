use ratatui::style::{Color, Style};

pub const READ_COLOR: Color = Color::Rgb(180, 120, 255);  // purple
pub const WRITE_COLOR: Color = Color::Rgb(255, 150, 230); // pink
pub const HEADER_BG: Color = Color::DarkGray;
pub const SELECTED_TAB_COLOR: Color = Color::Cyan;
pub const LABEL_COLOR: Color = Color::Gray;
pub const BORDER_COLOR: Color = Color::DarkGray;
pub const HELP_BORDER_COLOR: Color = Color::Cyan;

pub fn border_style() -> Style {
    Style::default().fg(BORDER_COLOR)
}
