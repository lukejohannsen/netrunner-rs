use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// The 4 top-level screen regions: header bar, central board, event log,
/// and the action selector menu.
pub struct LayoutRegions {
    pub header: Rect,
    pub board: Rect,
    pub log: Rect,
    pub actions: Rect,
}

pub fn build_layout(area: Rect) -> LayoutRegions {
    let [header, middle, actions] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(10), Constraint::Length(8)])
        .areas(area);

    let [board, log] = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
        .areas(middle);

    LayoutRegions { header, board, log, actions }
}
