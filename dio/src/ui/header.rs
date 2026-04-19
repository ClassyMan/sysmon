use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Tabs;

use crate::app::{App, ViewMode};
use crate::ui::theme;
use sysmon_shared::terminal_theme::palette;

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
        Span::styled(" FAST ", Style::default().fg(palette().bg_color()).bg(palette().bright_yellow()).add_modifier(Modifier::BOLD))
    } else {
        Span::raw("")
    };

    let title = Line::from(vec![
        Span::styled(" DISK ", Style::default().fg(theme::read_color()).add_modifier(Modifier::BOLD)),
        fast_span,
        Span::styled(
            format!(" {} | {} | {}ms ",
                hw_summary,
                view_label,
                app.refresh_rate.as_millis(),
            ),
            Style::default().fg(theme::label_color()),
        ),
    ]);

    let tabs = Tabs::new(device_names)
        .select(app.selected_device)
        .style(Style::default().fg(theme::label_color()))
        .highlight_style(
            Style::default()
                .fg(theme::selected_tab_color())
                .add_modifier(Modifier::BOLD),
        )
        .divider("|")
        .padding(" ", " ")
        .block(ratatui::widgets::Block::default().title(title));

    frame.render_widget(tabs, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
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

    #[test]
    fn test_header_shows_disk() {
        let app = App::with_capacity(100);
        let backend = TestBackend::new(120, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, frame.area(), &app))
            .unwrap();

        let output = buffer_to_string(&terminal);
        assert!(output.contains("DISK"), "expected 'DISK' in header, got:\n{output}");
    }

    #[test]
    fn test_header_shows_fast() {
        let mut app = App::with_capacity(100);
        app.fast_mode = true;
        let backend = TestBackend::new(120, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, frame.area(), &app))
            .unwrap();

        let output = buffer_to_string(&terminal);
        assert!(output.contains("FAST"), "expected 'FAST' in header, got:\n{output}");
    }
}
