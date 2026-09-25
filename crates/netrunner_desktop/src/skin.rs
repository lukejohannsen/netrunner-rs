//! The board's furniture, dressed in pictures instead of outlines.
//!
//! Archives, R&D, HQ and the remotes were a background colour, a
//! one-pixel border and a corner radius. They still are, when nobody has
//! drawn anything: that is the procedural tier, and it is what every slot
//! below falls back to. A skin is a folder of pictures under the same
//! three tiers as a table, and it replaces those outlines one slot at a
//! time.
//!
//! **A skin changes how a box is painted, never how big it is.** Sizes
//! belong to `models::layout` and to the widgets; a picture is fitted to
//! the box it is handed. This is not tidiness. `face_width` budgets the
//! window down to the pixel so the board never scrolls, and
//! `spawn_actions_menu` computes its own placement from the *hard-coded*
//! height of a button — art that resized anything would break one or
//! both, silently. [`Dressing::apply`] therefore only ever writes
//! colours and an `ImageNode`, and never touches `Node`.
//!
//! **The caller says what it would have drawn.** A tile's border is its
//! card's faction colour; a server column's is the accent when it
//! welcomes a drag and the Runner's red when a run is on. Those are
//! runtime facts this module has no business knowing, so [`Skin::dress`]
//! takes the [`Drawn`] the caller would have used and either overrides it
//! with a picture or hands it straight back.
//!
//! **Two levels of fallback, which is what makes a half-finished skin
//! usable**: a slot nobody has drawn is drawn as before, and a *state*
//! nobody has drawn borrows its base slot's picture. One `tile.png`
//! dresses a rezzed tile and an unrezzed one.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy::ui::widget::NodeImageMode;

use netrunner_client::settings::{Settings, Skin as Choice, Table, SKIN_AUTO, SKIN_DRAWN};
use netrunner_client::skin::{Manifest, Mode, SlotArt, MANIFEST_FILE};

use crate::assets;

/// Keeps the [`Skin`] resource in step with the settings and the table.
pub struct SkinPlugin;

impl Plugin for SkinPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Skin>().add_systems(Update, refresh);
    }
}

/// Reloads the skin when the choice or the table behind it moves.
///
/// Keyed on the pair rather than on `Settings` changing, because `Auto`
/// follows the *table*: switching field can change the chrome without the
/// skin setting being touched. Loading decodes files, so it must happen
/// when something actually changed and not on every frame that saves a
/// volume slider.
///
/// Does nothing without `Assets<Image>`, which is the headless tests: the
/// skin stays empty there and every slot is drawn.
fn refresh(
    mut commands: Commands,
    core: Res<crate::core::ClientCore>,
    drawn: Option<Res<crate::table::LastTable>>,
    images: Option<ResMut<Assets<Image>>>,
    mut last: Local<Option<(Choice, Option<String>, bool)>>,
) {
    let in_play = in_play(&core.settings.desktop.table, drawn.and_then(|drawn| drawn.0.clone()));
    let now = (core.settings.desktop.skin.clone(), in_play.clone(), core.settings.desktop.basic_graphics);
    if last.as_ref() == Some(&now) {
        return;
    }
    let Some(mut images) = images else { return };
    *last = Some(now);
    commands.insert_resource(rebuild(&core.settings, in_play.as_deref(), &mut images));
}

/// The table folder the board is on, for `Auto` to follow: the named one,
/// or under random the one the last match drew — which is the match in
/// play, since a pick is made on entering the board. Before any match has
/// drawn one, random suggests nothing.
fn in_play(table: &Table, drawn: Option<String>) -> Option<String> {
    match table {
        Table::Named(folder) => Some(folder.clone()),
        Table::Random => drawn,
    }
}

/// Where skins live, under either asset tier.
pub const DIR: &str = "skins";

/// The tint value that means "whatever the board would have drawn here",
/// so one white picture can serve every faction colour and every alarm.
pub const TINT_STATE: &str = "state";

/// A part of the board that can be dressed.
///
/// **A server's own picture is not a slot** — an Archives vault, an R&D
/// tower. The box it needs is reserved now (the header became a plate,
/// 16:9 for every server, `layout::plate_height`), so a picture cannot
/// change the layout; the picture itself is board art
/// (`crate::board_art`) rather than a nine-sliced frame, which is the
/// wrong shape for a building.
///
/// Flat rather than a slot-and-state pair: every variant is a thing that
/// can be drawn, and each knows which slot it falls back to. The manifest
/// spells it as [`Slot::key`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Slot {
    ServerColumn,
    ServerColumnWelcomes,
    ServerColumnUnderRun,
    ServerHeader,
    ServerHeaderWelcomes,
    Tile,
    TileRezzed,
    TileUnrezzed,
    Button,
    ButtonHover,
    ButtonPressed,
    ButtonDisabled,
    CompactButton,
    CompactButtonHover,
    CompactButtonPressed,
    PhaseChip,
    PhaseChipPast,
    PhaseChipNow,
    PhaseChipAhead,
    HudCell,
    HudCellAlarm,
    HudCellOpens,
    SubDot,
    SubDotPending,
    SubDotBroken,
    SubDotResolved,
    /// Every [`crate::widgets::panel`] in the client, not only the
    /// board's: the sheet, the decision pop-up, the actions menu, and the
    /// main menu, settings, profile and new-game screens. The same reach
    /// `Button` already has, and the authoring guide says so, because a
    /// panel picture that stayed on the board would be the surprise.
    Panel,
    /// The centred panel an overlay puts up: the card's sheet, a zone's,
    /// the score area, the options, the list of keys. Its own state so a
    /// skin can dress the game's surfaces without repainting the main
    /// menu and the settings, which `Panel` also reaches.
    PanelSheet,
    /// The decision pop-up, whose border the board draws in the accent.
    PanelDecision,
    /// The menu a card's click opens, drawn the same way.
    PanelMenu,
    /// The phase panel at the head of the right column: where the turn
    /// is, and where a run is.
    PanelPhase,
    /// The panel the Runner's identity appears in, in the right column,
    /// while a run is on: the picture of who is breaking in.
    PanelRun,
    /// The same place while the run encounters a piece of ICE: the ICE
    /// takes the Runner's panel, so a skin can frame the thing being
    /// broken apart from the one breaking it.
    PanelEncounter,
    /// The wash over the board behind a sheet. Not a panel's state — it
    /// is the thing *behind* the panel, so it falls back to nothing and
    /// a skin that paints panels leaves it alone unless it means to.
    OverlayScrim,
}

impl Slot {
    /// Every slot, in the order the gallery lists them.
    pub const ALL: [Slot; 34] = [
        Slot::ServerColumn,
        Slot::ServerColumnWelcomes,
        Slot::ServerColumnUnderRun,
        Slot::ServerHeader,
        Slot::ServerHeaderWelcomes,
        Slot::Tile,
        Slot::TileRezzed,
        Slot::TileUnrezzed,
        Slot::Button,
        Slot::ButtonHover,
        Slot::ButtonPressed,
        Slot::ButtonDisabled,
        Slot::CompactButton,
        Slot::CompactButtonHover,
        Slot::CompactButtonPressed,
        Slot::PhaseChip,
        Slot::PhaseChipPast,
        Slot::PhaseChipNow,
        Slot::PhaseChipAhead,
        Slot::HudCell,
        Slot::HudCellAlarm,
        Slot::HudCellOpens,
        Slot::SubDot,
        Slot::SubDotPending,
        Slot::SubDotBroken,
        Slot::SubDotResolved,
        Slot::Panel,
        Slot::PanelSheet,
        Slot::PanelDecision,
        Slot::PanelMenu,
        Slot::PanelPhase,
        Slot::PanelRun,
        Slot::PanelEncounter,
        Slot::OverlayScrim,
    ];

    /// How the manifest spells it.
    pub fn key(self) -> &'static str {
        match self {
            Slot::ServerColumn => "server.column",
            Slot::ServerColumnWelcomes => "server.column.welcomes",
            Slot::ServerColumnUnderRun => "server.column.run",
            Slot::ServerHeader => "server.header",
            Slot::ServerHeaderWelcomes => "server.header.welcomes",
            Slot::Tile => "tile",
            Slot::TileRezzed => "tile.rezzed",
            Slot::TileUnrezzed => "tile.unrezzed",
            Slot::Button => "button",
            Slot::ButtonHover => "button.hover",
            Slot::ButtonPressed => "button.pressed",
            Slot::ButtonDisabled => "button.disabled",
            Slot::CompactButton => "compact.button",
            Slot::CompactButtonHover => "compact.button.hover",
            Slot::CompactButtonPressed => "compact.button.pressed",
            Slot::PhaseChip => "phase.chip",
            Slot::PhaseChipPast => "phase.chip.past",
            Slot::PhaseChipNow => "phase.chip.now",
            Slot::PhaseChipAhead => "phase.chip.ahead",
            Slot::HudCell => "hud.cell",
            Slot::HudCellAlarm => "hud.cell.alarm",
            Slot::HudCellOpens => "hud.cell.opens",
            Slot::SubDot => "sub.dot",
            Slot::SubDotPending => "sub.dot.pending",
            Slot::SubDotBroken => "sub.dot.broken",
            Slot::SubDotResolved => "sub.dot.resolved",
            Slot::Panel => "panel",
            Slot::PanelSheet => "panel.sheet",
            Slot::PanelDecision => "panel.decision",
            Slot::PanelMenu => "panel.menu",
            Slot::PanelPhase => "panel.phase",
            Slot::PanelRun => "panel.run",
            Slot::PanelEncounter => "panel.encounter",
            Slot::OverlayScrim => "overlay.scrim",
        }
    }

    /// The slot a pointer resting on this one moves to, if it has a
    /// hovered state at all. Derived rather than stored at the call site,
    /// so a button never has to name its own three slots.
    pub fn hovered(self) -> Option<Slot> {
        match self {
            Slot::Button => Some(Slot::ButtonHover),
            Slot::CompactButton => Some(Slot::CompactButtonHover),
            _ => None,
        }
    }

    /// The slot a press moves to.
    pub fn pressed(self) -> Option<Slot> {
        match self {
            Slot::Button => Some(Slot::ButtonPressed),
            Slot::CompactButton => Some(Slot::CompactButtonPressed),
            _ => None,
        }
    }

    /// The slot this one borrows a picture from when nobody has drawn it,
    /// or `None` when it is itself a base.
    ///
    /// Exactly one level deep, deliberately: a chain would mean a skin's
    /// pictures could be found two steps from where they were written,
    /// which is harder to predict than it is useful.
    pub fn base(self) -> Option<Slot> {
        Some(match self {
            Slot::ServerColumnWelcomes | Slot::ServerColumnUnderRun => Slot::ServerColumn,
            Slot::ServerHeaderWelcomes => Slot::ServerHeader,
            Slot::TileRezzed | Slot::TileUnrezzed => Slot::Tile,
            Slot::ButtonHover | Slot::ButtonPressed | Slot::ButtonDisabled => Slot::Button,
            Slot::CompactButtonHover | Slot::CompactButtonPressed => Slot::CompactButton,
            Slot::PhaseChipPast | Slot::PhaseChipNow | Slot::PhaseChipAhead => Slot::PhaseChip,
            Slot::HudCellAlarm | Slot::HudCellOpens => Slot::HudCell,
            Slot::SubDotPending | Slot::SubDotBroken | Slot::SubDotResolved => Slot::SubDot,
            Slot::PanelSheet | Slot::PanelDecision | Slot::PanelMenu | Slot::PanelPhase | Slot::PanelRun | Slot::PanelEncounter => Slot::Panel,
            _ => return None,
        })
    }
}

/// What the board would paint here without a skin: the colours the call
/// site already had in hand.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drawn {
    pub bg: Color,
    pub border: Color,
}

impl Drawn {
    pub fn new(bg: Color, border: Color) -> Self {
        Self { bg, border }
    }
}

/// How a slot is to be painted.
#[derive(Debug, Clone)]
pub enum Dressing {
    Drawn(Drawn),
    Art { image: Handle<Image>, mode: NodeImageMode, tint: Color },
}

impl Dressing {
    /// Writes it onto an already-spawned entity.
    ///
    /// Only colours and an `ImageNode` — never `Node`. Two consequences,
    /// both deliberate: a dressed node keeps its border *width*, because
    /// width is layout and changing it would move everything around it
    /// (the border is hidden by colouring it away instead); and the
    /// corner radius stays the caller's, because `border_radius` is a
    /// field of `Node` rather than a component of its own. A picture
    /// rounds its own corners.
    pub fn apply(self, entity: &mut EntityCommands) {
        match self {
            Dressing::Drawn(drawn) => {
                entity.insert((BackgroundColor(drawn.bg), BorderColor::all(drawn.border)));
            }
            Dressing::Art { image, mode, tint } => {
                entity.insert((
                    ImageNode { image_mode: mode, color: tint, ..ImageNode::new(image) },
                    BackgroundColor(Color::NONE),
                    BorderColor::all(Color::NONE),
                ));
            }
        }
    }

    /// Whether a picture was found, for the gallery and the tests.
    pub fn is_art(&self) -> bool {
        matches!(self, Dressing::Art { .. })
    }
}

/// One slot's resolved picture.
#[derive(Debug, Clone)]
struct Art {
    image: Handle<Image>,
    mode: NodeImageMode,
}

/// The skin in force: the pictures it managed to load, and what it is
/// called.
///
/// Empty is the normal case and a working one — an empty skin dresses
/// nothing and every slot is drawn. That is also what the headless tests
/// get, since they build the client without an asset plugin, which is why
/// this item leaves their assertions untouched.
#[derive(Resource, Default)]
pub struct Skin {
    pub folder: Option<String>,
    pub manifest: Manifest,
    art: HashMap<Slot, Art>,
    tints: HashMap<Slot, Color>,
}

impl Skin {
    /// How a slot is painted, given what the caller would have drawn.
    pub fn dress(&self, slot: Slot, fallback: Drawn) -> Dressing {
        let found = self.art.get(&slot).or_else(|| self.art.get(&slot.base()?));
        match found {
            Some(art) => Dressing::Art {
                image: art.image.clone(),
                mode: art.mode.clone(),
                // A state's own tint wins; then its base's; then the
                // picture as it was drawn.
                tint: self
                    .tints
                    .get(&slot)
                    .copied()
                    .or_else(|| slot.base().and_then(|base| self.tints.get(&base).copied()))
                    .unwrap_or(Color::WHITE),
            },
            None => Dressing::Drawn(fallback),
        }
    }

    /// Whether the board should draw its own contact shadows over this
    /// skin.
    pub fn wants_shadows(&self) -> bool {
        self.manifest.wants_shadows()
    }

    /// Whether anything at all is dressed.
    pub fn is_empty(&self) -> bool {
        self.art.is_empty()
    }
}

/// Every installed skin, minus the reserved names — a folder called
/// `auto` or `drawn` could never be selected, so offering it would be a
/// row that does nothing.
pub fn available() -> Vec<String> {
    assets::list_dirs(DIR).into_iter().filter(|name| name != SKIN_AUTO && name != SKIN_DRAWN).collect()
}

/// Which folder a choice resolves to, given the table in play.
///
/// Pure, so the `Auto` rule is testable: `Auto` takes the table's
/// suggestion and falls back to the undressed board, `Drawn` refuses one
/// even when the table offers, and a named skin that is not installed
/// falls back to undressed rather than to something else — the same rule
/// a missing table follows, and for the same reason.
pub fn resolve(choice: &Choice, suggested: Option<&str>, installed: &[String]) -> Option<String> {
    let wanted = match choice {
        Choice::Drawn => return None,
        Choice::Auto => suggested?,
        Choice::Named(name) => name,
    };
    installed.iter().find(|name| *name == wanted).cloned()
}

/// The skin a table folder suggests, if it has a manifest naming one.
pub fn suggested_by(folder: &str) -> Option<String> {
    crate::table::manifest(folder).skin
}

/// Loads `folder`'s manifest and every picture it names.
///
/// A slot whose file will not decode is simply absent, which means it is
/// drawn — one unreadable PNG costs its own slot and nothing else.
pub fn load(folder: &str, images: &mut Assets<Image>) -> Skin {
    let manifest = match assets::read(&format!("{DIR}/{folder}/{MANIFEST_FILE}")) {
        Some(bytes) => match std::str::from_utf8(&bytes) {
            Ok(text) => Manifest::parse(folder, text),
            Err(_) => Manifest::for_folder(folder),
        },
        None => Manifest::for_folder(folder),
    };
    let mut art = HashMap::new();
    let mut tints = HashMap::new();
    for slot in Slot::ALL {
        let Some(entry) = manifest.slot(slot.key()) else { continue };
        if let Some(tint) = tint_of(entry) {
            tints.insert(slot, tint);
        }
        let Some(file) = entry.file.as_deref() else { continue };
        let Some(extension) = file.rsplit('.').next() else { continue };
        let Some(bytes) = assets::read(&format!("{DIR}/{folder}/{file}")) else { continue };
        let Some(image) = crate::card_images::decode(&bytes, extension) else { continue };
        art.insert(slot, Art { image: images.add(image), mode: node_mode(entry) });
    }
    Skin { folder: Some(folder.to_string()), manifest, art, tints }
}

/// The tint a slot entry asks for, or `None` for "as drawn".
///
/// `"state"` is the useful one: it means "whatever colour the board would
/// have used", so a single white picture serves every faction colour, the
/// alarm red and both sides. It is resolved at the call site, where the
/// colour is known, so here it only has to be recognised.
fn tint_of(entry: &SlotArt) -> Option<Color> {
    let text = entry.tint.as_deref()?;
    if text.eq_ignore_ascii_case(TINT_STATE) {
        return None;
    }
    let (r, g, b) = entry.tint_rgb()?;
    Some(Color::srgb_u8(r, g, b))
}

/// Whether a slot's entry asked for the state's own colour.
pub fn wants_state_tint(manifest: &Manifest, slot: Slot) -> bool {
    manifest
        .slot(slot.key())
        .or_else(|| manifest.slot(slot.base()?.key()))
        .and_then(|entry| entry.tint.as_deref())
        .is_some_and(|tint| tint.eq_ignore_ascii_case(TINT_STATE))
}

fn node_mode(entry: &SlotArt) -> NodeImageMode {
    match entry.mode() {
        Mode::Stretch => NodeImageMode::Stretch,
        Mode::Fit => NodeImageMode::Auto,
        Mode::Sliced => {
            let (l, t, r, b) = entry.insets.map_or((0.0, 0.0, 0.0, 0.0), |insets| insets.sides());
            NodeImageMode::Sliced(TextureSlicer {
                border: BorderRect { min_inset: Vec2::new(l, t), max_inset: Vec2::new(r, b) },
                // Stretched rather than tiled: a bubble's edges are a
                // gradient, and tiling one shows the seam.
                center_scale_mode: SliceScaleMode::Stretch,
                sides_scale_mode: SliceScaleMode::Stretch,
                max_corner_scale: 1.0,
            })
        }
    }
}

/// Rebuilds [`Skin`] from the settings and the table in play — or the
/// undressed board under basic graphics, which loads no file at all.
pub fn rebuild(settings: &Settings, table_in_play: Option<&str>, images: &mut Assets<Image>) -> Skin {
    if settings.desktop.basic_graphics {
        return Skin::default();
    }
    let suggested = table_in_play.and_then(suggested_by);
    match resolve(&settings.desktop.skin, suggested.as_deref(), &available()) {
        Some(folder) => load(&folder, images),
        None => Skin::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn every_slot_has_its_own_key_and_a_base_that_terminates() {
        let mut keys: Vec<&str> = Slot::ALL.iter().map(|slot| slot.key()).collect();
        let before = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), before, "two slots share a manifest key");
        for slot in Slot::ALL {
            // One level, never a chain: a base is itself a base.
            if let Some(base) = slot.base() {
                assert_eq!(base.base(), None, "{} borrows from {}, which borrows again", slot.key(), base.key());
                assert!(slot.key().starts_with(base.key()), "{} should read as a state of {}", slot.key(), base.key());
            }
        }
    }

    #[test]
    fn an_empty_skin_draws_everything_and_dresses_nothing() {
        let skin = Skin::default();
        let fallback = Drawn::new(Color::BLACK, Color::WHITE);
        assert!(skin.is_empty());
        for slot in Slot::ALL {
            let dressing = skin.dress(slot, fallback);
            assert!(!dressing.is_art(), "{} should be drawn", slot.key());
            match dressing {
                Dressing::Drawn(drawn) => assert_eq!(drawn, fallback, "the caller's own colours, handed back"),
                Dressing::Art { .. } => unreachable!(),
            }
        }
        assert!(skin.wants_shadows(), "nothing to carry its own");
    }

    /// The promise that makes a half-finished skin worth starting: one
    /// picture in the base slot dresses every state of it.
    #[test]
    fn a_state_nobody_drew_borrows_the_base_slots_picture() {
        let mut skin = Skin::default();
        skin.art.insert(Slot::Tile, Art { image: Handle::default(), mode: NodeImageMode::Stretch });
        let fallback = Drawn::new(Color::BLACK, Color::WHITE);
        assert!(skin.dress(Slot::Tile, fallback).is_art());
        assert!(skin.dress(Slot::TileRezzed, fallback).is_art(), "a rezzed tile borrows `tile`");
        assert!(skin.dress(Slot::TileUnrezzed, fallback).is_art(), "and so does an unrezzed one");
        // A different slot entirely is still drawn — the fallback is to
        // the base, not to any picture that happens to be loaded.
        assert!(!skin.dress(Slot::Button, fallback).is_art());
        assert!(!skin.dress(Slot::ServerColumn, fallback).is_art());
    }

    /// The same promise for the panels, which is the case it was written
    /// for: one `panel.png` dresses the sheet, the decision pop-up and
    /// the actions menu, and the wash behind a sheet is left alone —
    /// `overlay.scrim` is not a panel's state, it is the thing behind
    /// one, so it has no base to borrow from.
    #[test]
    fn one_panel_picture_dresses_the_pop_up_and_the_menu_but_not_the_wash() {
        let mut skin = Skin::default();
        skin.art.insert(Slot::Panel, Art { image: Handle::default(), mode: NodeImageMode::Stretch });
        let fallback = Drawn::new(Color::BLACK, Color::WHITE);
        assert!(skin.dress(Slot::Panel, fallback).is_art());
        assert!(skin.dress(Slot::PanelSheet, fallback).is_art(), "the sheet borrows `panel`");
        assert!(skin.dress(Slot::PanelDecision, fallback).is_art(), "the decision pop-up borrows `panel`");
        assert!(skin.dress(Slot::PanelMenu, fallback).is_art(), "and so does the actions menu");
        assert!(!skin.dress(Slot::OverlayScrim, fallback).is_art(), "the wash is drawn until somebody draws it");
    }

    #[test]
    fn a_states_own_tint_beats_its_bases_and_both_beat_the_plain_picture() {
        let mut skin = Skin::default();
        skin.art.insert(Slot::Button, Art { image: Handle::default(), mode: NodeImageMode::Stretch });
        let fallback = Drawn::new(Color::BLACK, Color::WHITE);
        let tint_of = |skin: &Skin, slot| match skin.dress(slot, fallback) {
            Dressing::Art { tint, .. } => tint,
            Dressing::Drawn(_) => panic!("expected art"),
        };
        assert_eq!(tint_of(&skin, Slot::ButtonHover), Color::WHITE, "undrawn and untinted is the picture as it is");
        skin.tints.insert(Slot::Button, Color::srgb(1.0, 0.0, 0.0));
        assert_eq!(tint_of(&skin, Slot::ButtonHover), Color::srgb(1.0, 0.0, 0.0), "the base's tint reaches its states");
        skin.tints.insert(Slot::ButtonHover, Color::srgb(0.0, 1.0, 0.0));
        assert_eq!(tint_of(&skin, Slot::ButtonHover), Color::srgb(0.0, 1.0, 0.0), "its own wins");
        assert_eq!(tint_of(&skin, Slot::Button), Color::srgb(1.0, 0.0, 0.0), "and does not leak back to the base");
    }

    #[test]
    fn a_choice_resolves_against_what_the_table_suggests_and_what_is_installed() {
        let installed = names(&["neon-chrome", "brass"]);
        // Auto takes the table's suggestion.
        assert_eq!(resolve(&Choice::Auto, Some("brass"), &installed), Some("brass".to_string()));
        assert_eq!(resolve(&Choice::Auto, None, &installed), None, "no suggestion, no skin");
        assert_eq!(resolve(&Choice::Auto, Some("gone"), &installed), None, "suggested but not installed");
        // Drawn refuses one even when the table offers.
        assert_eq!(resolve(&Choice::Drawn, Some("brass"), &installed), None);
        // A named skin ignores the suggestion entirely.
        assert_eq!(resolve(&Choice::Named("neon-chrome".to_string()), Some("brass"), &installed), Some("neon-chrome".to_string()));
        assert_eq!(resolve(&Choice::Named("gone".to_string()), Some("brass"), &installed), None, "never falls through to another skin");
    }

    #[test]
    fn insets_become_a_nine_slice_and_an_icon_keeps_its_aspect() {
        let sliced = SlotArt { insets: Some(netrunner_client::skin::Insets::Sides([1.0, 2.0, 3.0, 4.0])), ..SlotArt::default() };
        match node_mode(&sliced) {
            NodeImageMode::Sliced(slicer) => {
                assert_eq!(slicer.border.min_inset, Vec2::new(1.0, 2.0));
                assert_eq!(slicer.border.max_inset, Vec2::new(3.0, 4.0));
            }
            other => panic!("{other:?}"),
        }
        let icon = SlotArt { mode: Some(Mode::Fit), ..SlotArt::default() };
        assert!(matches!(node_mode(&icon), NodeImageMode::Auto));
        assert!(matches!(node_mode(&SlotArt::default()), NodeImageMode::Stretch), "nothing to preserve");
    }

    /// `"state"` is not a colour, and must not be read as a failed one.
    #[test]
    fn the_state_tint_is_recognised_rather_than_parsed() {
        let entry = SlotArt { tint: Some("state".to_string()), ..SlotArt::default() };
        assert_eq!(tint_of(&entry), None, "resolved at the call site, where the colour is known");
        let manifest = Manifest::parse("s", r#"{"slots":{"tile":{"tint":"state"}}}"#);
        assert!(wants_state_tint(&manifest, Slot::Tile));
        assert!(wants_state_tint(&manifest, Slot::TileRezzed), "a state inherits the ask");
        assert!(!wants_state_tint(&manifest, Slot::Button));
    }
}
