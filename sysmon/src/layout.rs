use ratatui::layout::{Constraint, Layout, Rect};

pub fn grid_cols(panel_count: usize) -> usize {
    if panel_count <= 3 { 1 } else { 2 }
}

pub fn neighbor(index: usize, panel_count: usize, dir: Direction) -> Option<usize> {
    if panel_count == 0 {
        return None;
    }
    let cols = grid_cols(panel_count);
    let row = index / cols;
    let col = index % cols;
    let rows = (panel_count + cols - 1) / cols;

    let (new_row, new_col) = match dir {
        Direction::Up => (row.checked_sub(1)?, col),
        Direction::Down => (row + 1, col),
        Direction::Left => (row, col.checked_sub(1)?),
        Direction::Right => (row, col + 1),
    };

    if new_row >= rows || new_col >= cols {
        return None;
    }
    let new_index = new_row * cols + new_col;
    if new_index >= panel_count {
        return None;
    }
    Some(new_index)
}

#[derive(Clone, Copy)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

pub fn compute_grid(area: Rect, panel_count: usize) -> Vec<Rect> {
    if panel_count == 0 {
        return Vec::new();
    }

    let cols = grid_cols(panel_count);
    let rows = (panel_count + cols - 1) / cols;

    let row_constraints: Vec<Constraint> = (0..rows)
        .map(|_| Constraint::Ratio(1, rows as u32))
        .collect();
    let row_areas = Layout::vertical(row_constraints).split(area);

    let mut rects = Vec::new();
    let mut placed = 0;
    for (row_idx, &row_area) in row_areas.iter().enumerate() {
        let remaining = panel_count - placed;
        let panels_in_row = if row_idx == rows - 1 { remaining } else { cols };
        let col_constraints: Vec<Constraint> = (0..panels_in_row)
            .map(|_| Constraint::Ratio(1, panels_in_row as u32))
            .collect();
        let col_areas = Layout::horizontal(col_constraints).split(row_area);
        rects.extend_from_slice(&col_areas);
        placed += panels_in_row;
    }
    rects
}
