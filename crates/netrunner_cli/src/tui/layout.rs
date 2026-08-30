use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// The 3 top-level screen regions: header bar, central board, and the
/// action selector menu. No event-log panel — `ClientView` carries no
/// event stream (only the current masked state), so there's nothing to
/// populate one from; see `app::App`'s doc comment. Used by the remote
/// (`MatchSession`/channel-backed) TUI path — see `build_layout_with_log`
/// for the local (`SinglePlayerSession`-backed) path, which does have an
/// event source (`SinglePlayerSession::with_observer`) to populate a log
/// panel from.
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

/// The local-play TUI's 4 top-level screen regions: `build_layout`'s 3
/// regions plus a `log` panel — populated live from
/// `SinglePlayerSession::with_observer`, one line per resolved action
/// (human or bot).
pub struct LocalLayoutRegions {
    pub header: Rect,
    pub board: Rect,
    pub actions: Rect,
    pub log: Rect,
}

pub fn build_layout_with_log(area: Rect) -> LocalLayoutRegions {
    let [header, board, actions, log] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(10), Constraint::Length(8), Constraint::Length(8)])
        .areas(area);

    LocalLayoutRegions { header, board, actions, log }
}
