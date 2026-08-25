use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// The 3 top-level screen regions: header bar, central board, and the
/// action selector menu. No event-log panel — `ClientView` carries no
/// event stream (only the current masked state), so there's nothing to
/// populate one from; see `app::App`'s doc comment.
pub struct LayoutRegions {
    pub header: Rect,
    pub board: Rect,
    pub actions: Rect,
}

pub fn build_layout(area: Rect) -> LayoutRegions {
    let [header, board, actions] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(10), Constraint::Length(8)])
        .areas(area);

    LayoutRegions { header, board, actions }
}
