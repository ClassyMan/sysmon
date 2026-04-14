use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};

use crate::app::App;
use crate::collector::human_volume;
use sysmon_shared::line_chart::{self, LineChart};

const POLY_COLOR: Color = Color::Rgb(120, 200, 255);
const BORDER_COLOR: Color = Color::DarkGray;
const LABEL_COLOR: Color = Color::Gray;
const SELECTED_BG: Color = Color::Rgb(30, 40, 60);
const MUTED_COLOR: Color = Color::Rgb(100, 100, 100);

fn price_color(pct: f64) -> Color {
    if pct >= 60.0 {
        Color::Rgb(100, 255, 100)
    } else if pct >= 40.0 {
        Color::Rgb(255, 220, 100)
    } else {
        Color::Rgb(255, 100, 100)
    }
}

pub fn render(frame: &mut Frame, app: &App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(frame.area());

    draw_header(frame, outer[0], app);
    draw_table(frame, outer[1], app);
    draw_chart(frame, outer[2], app);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let fast_span = if app.fast_mode {
        Span::styled(
            " FAST ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::raw("")
    };

    let status = if let Some(ref err) = app.last_error {
        format!(" {err}")
    } else if let Some(last) = app.last_update {
        let ago = last.elapsed().as_secs();
        format!(" Updated {ago}s ago")
    } else {
        " Fetching...".to_string()
    };

    let topic_span = Span::styled(
        format!(" [{}] ", app.topic.label()),
        Style::default()
            .fg(Color::Rgb(180, 140, 255))
            .add_modifier(Modifier::BOLD),
    );
    let sort_span = Span::styled(
        format!(" [{}] ", app.sort_order.label()),
        Style::default()
            .fg(Color::Rgb(255, 180, 100))
            .add_modifier(Modifier::BOLD),
    );

    let text = Paragraph::new(Line::from(vec![
        Span::styled(
            " POLY ",
            Style::default()
                .fg(POLY_COLOR)
                .add_modifier(Modifier::BOLD),
        ),
        fast_span,
        topic_span,
        sort_span,
        Span::styled(
            format!(
                "{} | {}s | {} events ",
                status,
                app.refresh_ms / 1000,
                app.events.len()
            ),
            Style::default().fg(LABEL_COLOR),
        ),
    ]));
    frame.render_widget(text, area);
}

fn draw_table(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title(" Trending Events ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BORDER_COLOR));

    if app.events.is_empty() {
        let msg = if app.last_error.is_some() {
            "Failed to fetch events"
        } else {
            "Fetching events..."
        };
        let placeholder = Paragraph::new(msg)
            .style(Style::default().fg(LABEL_COLOR))
            .block(block);
        frame.render_widget(placeholder, area);
        return;
    }

    let header_cells = ["", "Event", "Lead %", "Mkts", "24h Vol"]
        .iter()
        .map(|header| {
            Cell::from(*header).style(
                Style::default()
                    .fg(POLY_COLOR)
                    .add_modifier(Modifier::BOLD),
            )
        });
    let header = Row::new(header_cells).height(1);

    let rows = app.events.iter().enumerate().map(|(idx, event)| {
        let is_selected = idx == app.selected;
        let marker = if is_selected { ">" } else { " " };

        let lead_price = event
            .lead_market()
            .map(|m| m.yes_price)
            .unwrap_or(0.0);
        let price_str = format!("{:.0}%", lead_price);
        let mkts_str = format!("{}", event.market_count());
        let vol_str = human_volume(event.total_volume_24h);

        let row_style = if is_selected {
            Style::default().bg(SELECTED_BG).fg(Color::White)
        } else {
            Style::default().fg(LABEL_COLOR)
        };

        Row::new(vec![
            Cell::from(marker),
            Cell::from(event.title.clone()),
            Cell::from(price_str).style(Style::default().fg(price_color(lead_price))),
            Cell::from(mkts_str).style(Style::default().fg(MUTED_COLOR)),
            Cell::from(vol_str),
        ])
        .style(row_style)
    });

    let widths = [
        Constraint::Length(1),
        Constraint::Min(30),
        Constraint::Length(7),
        Constraint::Length(4),
        Constraint::Length(9),
    ];

    let table = Table::new(rows, widths).header(header).block(block);

    frame.render_widget(table, area);
}

fn draw_chart(frame: &mut Frame, area: Rect, app: &App) {
    let lead = app.selected_lead();
    let title = lead
        .map(|m| format!(" {} ({:.0}% Yes) ", m.question, m.yes_price))
        .unwrap_or_else(|| " Price History ".to_string());

    if app.price_history.is_empty() {
        let msg = if app.events.is_empty() {
            ""
        } else {
            "Loading price history..."
        };
        let placeholder = Paragraph::new(msg)
            .style(Style::default().fg(LABEL_COLOR))
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(BORDER_COLOR)),
            );
        frame.render_widget(placeholder, area);
        return;
    }

    let data_len = app.price_history.len() as f64;
    let y_min = app
        .price_history
        .iter()
        .map(|(_, price)| *price)
        .fold(f64::MAX, f64::min)
        .max(0.0);
    let y_max = app
        .price_history
        .iter()
        .map(|(_, price)| *price)
        .fold(f64::MIN, f64::max)
        .min(100.0);

    let y_floor = (y_min - 5.0).max(0.0);
    let y_ceil = (y_max + 5.0).min(100.0);

    let label = lead
        .map(|m| format!("{:.0}%", m.yes_price))
        .unwrap_or_default();

    let chart = LineChart::new(vec![line_chart::Dataset {
        data: &app.price_history,
        color: POLY_COLOR,
        name: label,
    }])
    .block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER_COLOR)),
    )
    .x_bounds([0.0, (data_len - 1.0).max(1.0)])
    .y_bounds([y_floor, y_ceil])
    .x_labels(["7d".to_string(), "now".to_string()])
    .y_labels([format!("{:.0}%", y_floor), format!("{:.0}%", y_ceil)])
    .rounded(true);

    frame.render_widget(chart, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::{Event, FetchState, SubMarket};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::sync::{Arc, Mutex};

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

    fn test_app_empty() -> App {
        App::new(Arc::new(Mutex::new(FetchState::new(30000))), 30000)
    }

    fn test_app_with_events() -> App {
        let shared = Arc::new(Mutex::new(FetchState::new(30000)));
        let mut app = App::new(shared, 30000);
        app.events = vec![
            Event {
                title: "2026 FIFA World Cup Winner".to_string(),
                markets: vec![
                    SubMarket {
                        question: "Will Spain win the 2026 FIFA World Cup?".to_string(),
                        yes_price: 17.1,
                        volume_24h: 454_000.0,
                        yes_token_id: "tok_1".to_string(),
                    },
                    SubMarket {
                        question: "Will France win the 2026 FIFA World Cup?".to_string(),
                        yes_price: 16.5,
                        volume_24h: 859_000.0,
                        yes_token_id: "tok_2".to_string(),
                    },
                ],
                total_volume_24h: 12_000_000.0,
            },
            Event {
                title: "Military action against Iran".to_string(),
                markets: vec![SubMarket {
                    question: "Military action against Iran ends by April 17?".to_string(),
                    yes_price: 99.9,
                    volume_24h: 5_600_000.0,
                    yes_token_id: "tok_3".to_string(),
                }],
                total_volume_24h: 5_600_000.0,
            },
        ];
        app.price_history = (0..100)
            .map(|idx| (idx as f64, 50.0 + (idx as f64 * 0.3).sin() * 15.0))
            .collect();
        app
    }

    #[test]
    fn test_render_empty_no_panic() {
        let app = test_app_empty();
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
    }

    #[test]
    fn test_render_with_events_no_panic() {
        let app = test_app_with_events();
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
    }

    #[test]
    fn test_header_shows_poly() {
        let app = test_app_empty();
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let output = buffer_to_string(&terminal);
        assert!(output.contains("POLY"), "expected 'POLY' in header, got:\n{output}");
    }

    #[test]
    fn test_header_shows_fast_when_active() {
        let mut app = test_app_empty();
        app.fast_mode = true;
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let output = buffer_to_string(&terminal);
        assert!(output.contains("FAST"), "expected 'FAST' in header, got:\n{output}");
    }

    #[test]
    fn test_header_shows_fetching_when_no_data() {
        let app = test_app_empty();
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let output = buffer_to_string(&terminal);
        assert!(output.contains("Fetching"), "expected 'Fetching' in header, got:\n{output}");
    }

    #[test]
    fn test_table_shows_event_title() {
        let app = test_app_with_events();
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let output = buffer_to_string(&terminal);
        assert!(
            output.contains("World Cup"),
            "expected event title in table, got:\n{output}"
        );
    }

    #[test]
    fn test_table_shows_trending_events_title() {
        let app = test_app_with_events();
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let output = buffer_to_string(&terminal);
        assert!(
            output.contains("Trending Events"),
            "expected 'Trending Events' in table title, got:\n{output}"
        );
    }

    #[test]
    fn test_render_narrow_terminal_no_panic() {
        let app = test_app_with_events();
        let backend = TestBackend::new(40, 15);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
    }

    #[test]
    fn test_price_color_high() {
        assert!(matches!(price_color(75.0), Color::Rgb(100, 255, 100)));
    }

    #[test]
    fn test_price_color_mid() {
        assert!(matches!(price_color(50.0), Color::Rgb(255, 220, 100)));
    }

    #[test]
    fn test_price_color_low() {
        assert!(matches!(price_color(20.0), Color::Rgb(255, 100, 100)));
    }
}
