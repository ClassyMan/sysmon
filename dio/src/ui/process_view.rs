use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Text;
use ratatui::widgets::{Block, Borders, Cell, Row, Table};

use crate::model::process::{ProcessIoTable, SortColumn};
use crate::model::types::human_bytes;
use crate::ui::theme;

pub fn render(frame: &mut Frame, area: Rect, table: &ProcessIoTable) {
    let columns: &[(&str, Option<SortColumn>)] = &[
        ("PID", Some(SortColumn::Pid)),
        ("Command", None),
        ("Read/s", Some(SortColumn::ReadBytes)),
        ("Write/s", Some(SortColumn::WriteBytes)),
        ("Total/s", Some(SortColumn::TotalBytes)),
    ];

    let header_cells = columns.iter().map(|(label, sort_col)| {
        let marker = match sort_col {
            Some(col) if *col == table.sort_column => {
                if table.sort_descending {
                    " v"
                } else {
                    " ^"
                }
            }
            _ => "",
        };
        Cell::from(format!("{}{}", label, marker))
    });

    let header = Row::new(header_cells)
        .style(
            Style::default()
                .fg(theme::SELECTED_TAB_COLOR)
                .add_modifier(Modifier::BOLD),
        )
        .height(1);

    let rows = table.entries.iter().map(|entry| {
        Row::new(vec![
            Cell::from(Text::raw(entry.pid.to_string())),
            Cell::from(Text::raw(entry.comm.clone())),
            Cell::from(Text::raw(human_bytes(entry.read_bytes_per_sec))),
            Cell::from(Text::raw(human_bytes(entry.write_bytes_per_sec))),
            Cell::from(Text::raw(human_bytes(entry.total_bytes_per_sec()))),
        ])
    });

    let title = if table.permission_degraded {
        " Processes (own-user only, run as root for all) "
    } else {
        " Processes "
    };

    let widths = [
        ratatui::layout::Constraint::Length(8),
        ratatui::layout::Constraint::Fill(1),
        ratatui::layout::Constraint::Length(14),
        ratatui::layout::Constraint::Length(14),
        ratatui::layout::Constraint::Length(14),
    ];

    let process_table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .style(theme::border_style()),
        )
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    frame.render_widget(process_table, area);
}
