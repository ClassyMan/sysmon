use ratatui::style::{Color, Style};
use sysmon_shared::terminal_theme::palette;

pub fn read_color() -> Color { palette().bright_cyan() }
pub fn write_color() -> Color { palette().bright_red() }
pub fn selected_tab_color() -> Color { palette().bright_cyan() }
pub fn label_color() -> Color { palette().muted_label() }
pub fn border_color() -> Color { palette().muted_label() }
pub fn help_border_color() -> Color { palette().bright_cyan() }

pub fn border_style() -> Style {
    Style::default().fg(border_color())
}
