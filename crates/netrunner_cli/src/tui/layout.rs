use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// The TUI's 4 top-level screen regions: header bar, central board, the
/// action selector menu, and a match log panel — one line per resolved
/// action, either side.
///
/// There used to be a second, log-less 3-region layout for the remote path,
/// because a channel-backed client had no event source to populate a log
/// from: `ClientView` carries only the current masked state, and the local
/// path's log came from a `SinglePlayerSession` observer the server had no
/// equivalent of. Now that the shared `netrunner_session::Session` records
/// a `MatchHistory` on every path and the server forwards it as
/// `ServerMessage::ActionLog`, both paths have a log and both render this.
pub struct LayoutRegions {
    pub header: Rect,
    pub board: Rect,
    pub actions: Rect,
    pub log: Rect,
}

pub fn build_layout_with_log(area: Rect) -> LayoutRegions {
    let [header, board, actions, log] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(10), Constraint::Length(8), Constraint::Length(8)])
        .areas(area);

    LayoutRegions { header, board, actions, log }
}
