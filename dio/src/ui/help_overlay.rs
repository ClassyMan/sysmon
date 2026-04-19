use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::ui::theme;

const HELP_TEXT: &[(&str, &str)] = &[
    ("q / Esc / C-c", "Quit"),
    ("Tab", "Cycle view: all -> single -> processes"),
    ("d / Right", "Next device"),
    ("D / Left", "Previous device"),
    ("p", "Toggle process view"),
    ("s", "Cycle sort column (process view)"),
    ("r", "Reverse sort direction"),
    ("+ / =", "Faster refresh rate"),
    ("-", "Slower refresh rate"),
    ("f", "Toggle fast mode (50ms / 2s window)"),
    ("?", "Toggle this help"),
];

pub fn render(frame: &mut Frame, area: Rect) {
    let popup_width = 52;
    let popup_height = (HELP_TEXT.len() + 4) as u16;

    let [popup_area] = Layout::vertical([Constraint::Length(popup_height)])
        .flex(Flex::Center)
        .areas(
            Layout::horizontal([Constraint::Length(popup_width)])
                .flex(Flex::Center)
                .areas::<1>(area)[0],
        );

    frame.render_widget(Clear, popup_area);

    let lines: Vec<Line> = HELP_TEXT
        .iter()
        .map(|(key, desc)| {
            Line::from(vec![
                Span::styled(
                    format!("{:>14}", key),
                    Style::default()
                        .fg(theme::selected_tab_color())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(*desc, Style::default().fg(theme::label_color())),
            ])
        })
        .collect();

    let help = Paragraph::new(lines).block(
        Block::default()
            .title(" Keybindings ")
            .borders(Borders::ALL)
            .style(Style::default().fg(theme::help_border_color())),
    );

    frame.render_widget(help, popup_area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_help_overlay_no_panic() {
        let backend = TestBackend::new(80, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, frame.area()))
            .unwrap();
    }
}
