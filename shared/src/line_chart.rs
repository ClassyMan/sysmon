use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Widget};

use crate::terminal_theme::palette;

pub struct Dataset<'a> {
    pub data: &'a [(f64, f64)],
    pub color: Color,
    pub name: String,
}

pub struct LineChart<'a> {
    datasets: Vec<Dataset<'a>>,
    block: Option<Block<'a>>,
    x_bounds: [f64; 2],
    y_bounds: [f64; 2],
    x_labels: [String; 2],
    y_labels: [String; 2],
    rounded: bool,
    left_aligned: bool,
    direction_colors: Option<(Color, Color)>,
}

impl<'a> LineChart<'a> {
    pub fn new(datasets: Vec<Dataset<'a>>) -> Self {
        Self {
            datasets,
            block: None,
            x_bounds: [0.0, 1.0],
            y_bounds: [0.0, 1.0],
            x_labels: [String::new(), String::new()],
            y_labels: [String::new(), String::new()],
            rounded: false,
            left_aligned: false,
            direction_colors: None,
        }
    }

    pub fn left_aligned(mut self, left_aligned: bool) -> Self {
        self.left_aligned = left_aligned;
        self
    }

    pub fn direction_colors(mut self, rise: Color, fall: Color) -> Self {
        self.direction_colors = Some((rise, fall));
        self
    }

    pub fn rounded(mut self, rounded: bool) -> Self {
        self.rounded = rounded;
        self
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    pub fn x_bounds(mut self, bounds: [f64; 2]) -> Self {
        self.x_bounds = bounds;
        self
    }

    pub fn y_bounds(mut self, bounds: [f64; 2]) -> Self {
        self.y_bounds = bounds;
        self
    }

    pub fn x_labels(mut self, labels: [String; 2]) -> Self {
        self.x_labels = labels;
        self
    }

    pub fn y_labels(mut self, labels: [String; 2]) -> Self {
        self.y_labels = labels;
        self
    }
}

impl Widget for LineChart<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 6 || area.height < 4 {
            return;
        }

        let inner = match &self.block {
            Some(block) => {
                let inner = block.inner(area);
                block.clone().render(area, buf);
                inner
            }
            None => area,
        };

        if inner.width < 6 || inner.height < 4 {
            return;
        }

        let y_gutter = self.y_labels.iter()
            .map(|l| l.len() as u16)
            .max()
            .unwrap_or(0)
            + 1;

        let data_area = Rect {
            x: inner.x + y_gutter,
            y: inner.y,
            width: inner.width.saturating_sub(y_gutter),
            height: inner.height.saturating_sub(1),
        };

        if data_area.width == 0 || data_area.height == 0 {
            return;
        }

        let label_style = Style::default().bold();

        // Y labels: top = max, bottom = min
        buf.set_string(inner.x, data_area.y, &self.y_labels[1], label_style);
        buf.set_string(
            inner.x,
            data_area.y + data_area.height.saturating_sub(1),
            &self.y_labels[0],
            label_style,
        );

        // X labels
        let x_row = data_area.y + data_area.height;
        buf.set_string(data_area.x, x_row, &self.x_labels[0], label_style);
        let right_x = data_area.right().saturating_sub(self.x_labels[1].len() as u16);
        buf.set_string(right_x, x_row, &self.x_labels[1], label_style);

        // Legend box (top-right of data area)
        render_legend(buf, data_area, &self.datasets);

        // Data lines (rendered last so they layer on top)
        for dataset in &self.datasets {
            render_line(buf, data_area, dataset, self.x_bounds, self.y_bounds, self.rounded, self.left_aligned, self.direction_colors);
        }
    }
}

fn render_legend(buf: &mut Buffer, area: Rect, datasets: &[Dataset]) {
    if datasets.is_empty() {
        return;
    }

    let max_name = datasets.iter().map(|d| d.name.len()).max().unwrap_or(0);
    let box_w = (max_name + 4) as u16;
    let box_h = (datasets.len() + 2) as u16;

    if box_w >= area.width || box_h >= area.height {
        return;
    }

    let bx = area.right() - box_w;
    let by = area.y;
    let bs = Style::default().fg(palette().muted_label());

    set_ch(buf, bx, by, '┌', bs);
    for x in (bx + 1)..(bx + box_w - 1) {
        set_ch(buf, x, by, '─', bs);
    }
    set_ch(buf, bx + box_w - 1, by, '┐', bs);

    for (i, ds) in datasets.iter().enumerate() {
        let row = by + 1 + i as u16;
        set_ch(buf, bx, row, '│', bs);
        buf.set_string(
            bx + 2,
            row,
            &format!("{:width$}", ds.name, width = max_name),
            Style::default().fg(ds.color),
        );
        set_ch(buf, bx + box_w - 1, row, '│', bs);
    }

    let bot = by + box_h - 1;
    set_ch(buf, bx, bot, '└', bs);
    for x in (bx + 1)..(bx + box_w - 1) {
        set_ch(buf, x, bot, '─', bs);
    }
    set_ch(buf, bx + box_w - 1, bot, '┘', bs);
}

fn render_line(
    buf: &mut Buffer,
    area: Rect,
    dataset: &Dataset,
    _x_bounds: [f64; 2],
    y_bounds: [f64; 2],
    rounded: bool,
    left_aligned: bool,
    direction_colors: Option<(Color, Color)>,
) {
    let data = dataset.data;
    if data.is_empty() || area.width == 0 || area.height == 0 {
        return;
    }

    let width = area.width as usize;
    let height = area.height as usize;
    let y_range = y_bounds[1] - y_bounds[0];

    if y_range <= 0.0 {
        return;
    }

    let data_len = data.len();
    let mut col_rows: Vec<Option<u16>> = vec![None; width];

    let value_to_row = |y: f64| -> u16 {
        let normalized = ((y - y_bounds[0]) / y_range).clamp(0.0, 1.0);
        ((1.0 - normalized) * (height.saturating_sub(1)) as f64).round() as u16
    };

    // Always 1:1 mapping — one data point per column.
    // When more points than columns: show only the most recent `width` points.
    // When fewer points than columns:
    //   right-align (default) — leave left empty, data grows from right edge.
    //   left-align — leave right empty, data starts from left edge.
    let (data_slice, col_offset) = if data_len <= width {
        let offset = if left_aligned { 0 } else { width - data_len };
        (data, offset)
    } else {
        (&data[data_len - width..], 0)
    };

    for (i, &(_, y)) in data_slice.iter().enumerate() {
        col_rows[col_offset + i] = Some(value_to_row(y));
    }

    let base_style = Style::default().fg(dataset.color);
    let mut prev_row: Option<u16> = None;

    let (top_left, top_right, bot_left, bot_right) = if rounded {
        ('╭', '╮', '╰', '╯')
    } else {
        ('┌', '┐', '└', '┘')
    };

    for (col, cell_row) in col_rows.iter().enumerate() {
        let Some(&row) = cell_row.as_ref() else {
            prev_row = None;
            continue;
        };

        let x = area.x + col as u16;

        let flat_style = direction_colors
            .map(|(rise, _)| Style::default().fg(rise))
            .unwrap_or(base_style);

        match prev_row {
            None => {
                set_ch(buf, x, area.y + row, '─', flat_style);
            }
            Some(prev) if prev == row => {
                set_ch(buf, x, area.y + row, '─', flat_style);
            }
            Some(prev) if prev > row => {
                // Value went UP (row numbers decrease upward)
                let style = direction_colors
                    .map(|(rise, _)| Style::default().fg(rise))
                    .unwrap_or(base_style);
                set_ch(buf, x, area.y + row, top_left, style);
                for r in (row + 1)..prev {
                    set_ch(buf, x, area.y + r, '│', style);
                }
                set_ch(buf, x, area.y + prev, bot_right, style);
            }
            Some(prev) => {
                // Value went DOWN
                let style = direction_colors
                    .map(|(_, fall)| Style::default().fg(fall))
                    .unwrap_or(base_style);
                set_ch(buf, x, area.y + prev, top_right, style);
                for r in (prev + 1)..row {
                    set_ch(buf, x, area.y + r, '│', style);
                }
                set_ch(buf, x, area.y + row, bot_left, style);
            }
        }

        prev_row = Some(row);
    }
}

fn set_ch(buf: &mut Buffer, x: u16, y: u16, ch: char, style: Style) {
    if let Some(cell) = buf.cell_mut((x, y)) {
        cell.set_char(ch);
        cell.set_style(style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::widgets::Borders;

    #[test]
    fn test_render_empty_data() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 40, 10));
        let chart = LineChart::new(vec![Dataset {
            data: &[],
            color: Color::White,
            name: "test".to_string(),
        }])
        .block(
            Block::default()
                .title(" Test ")
                .borders(Borders::ALL),
        )
        .x_bounds([0.0, 100.0])
        .y_bounds([0.0, 100.0])
        .x_labels(["0s".to_string(), "60s".to_string()])
        .y_labels(["0".to_string(), "100".to_string()]);

        chart.render(Rect::new(0, 0, 40, 10), &mut buf);
    }

    #[test]
    fn test_render_with_data() {
        let data: Vec<(f64, f64)> = (0..20).map(|i| (i as f64, (i * 5) as f64)).collect();
        let mut buf = Buffer::empty(Rect::new(0, 0, 40, 10));
        let chart = LineChart::new(vec![Dataset {
            data: &data,
            color: Color::Yellow,
            name: "alloc: 50 MB/s".to_string(),
        }])
        .block(
            Block::default()
                .title(" Test ")
                .borders(Borders::ALL),
        )
        .x_bounds([0.0, 19.0])
        .y_bounds([0.0, 100.0])
        .x_labels(["60s".to_string(), "0s".to_string()])
        .y_labels(["0".to_string(), "100".to_string()]);

        chart.render(Rect::new(0, 0, 40, 10), &mut buf);
    }

    #[test]
    fn test_render_tiny_area() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 3, 3));
        let chart = LineChart::new(vec![])
            .x_bounds([0.0, 1.0])
            .y_bounds([0.0, 1.0]);
        chart.render(Rect::new(0, 0, 3, 3), &mut buf);
    }
}
