use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// The TUI's top-level screen regions: header bar, central board, the
/// action selector menu, a match log panel — one line per resolved action,
/// either side — and, during a lesson, a coaching panel beside the board.
///
/// There used to be a second, log-less 3-region layout for the remote path,
/// because a channel-backed client had no event source to populate a log
/// from: `ClientView` carries only the current masked state, and the local
/// path's log came from a `SinglePlayerSession` observer the server had no
/// equivalent of. Now that the shared `netrunner_session::Session` records
/// a `MatchHistory` on every path and the server forwards it as
/// `ServerMessage::ActionLog`, both paths have a log and both render this.
///
/// The coaching panel is the one optional region, and it is a parameter
/// rather than a third builder: the two paths differ in exactly one
/// boolean, and a second copy of the vertical split would drift from the
/// first the way the log-less layout did.
pub struct LayoutRegions {
    pub header: Rect,
    pub board: Rect,
    pub actions: Rect,
    pub log: Rect,
    /// Beside the board, taking 40% of its width, when a lesson is live.
    pub coach: Option<Rect>,
}

pub fn build_layout(area: Rect, with_coach: bool) -> LayoutRegions {
    let [header, board_row, actions, log] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(10), Constraint::Length(8), Constraint::Length(8)])
        .areas(area);

    let (board, coach) = if with_coach {
        let [board, coach] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .areas(board_row);
        (board, Some(coach))
    } else {
        (board_row, None)
    };

    LayoutRegions { header, board, actions, log, coach }
}
