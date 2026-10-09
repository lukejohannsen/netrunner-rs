//! Cards seen moving on the board: the flights `models::flight` plans,
//! flown over the table.
//!
//! **The board is respawned whole on every view** (`screens::game::redraw`),
//! so a card that moved is not a node that moved: the node it was is gone
//! and a node where it now is has taken its place. The movement is drawn
//! *over* that, as a flying copy under the screen root: when a redraw is
//! about to draw transitions, `flights` reads where every box on the
//! old board is laid out (a [`Survey`]), waits one frame for the new
//! board to be laid out in its turn, reads it again and plans each
//! `CardMoved` between the two. The copy sets off from the old box, the
//! card it is landing on is hidden until it lands, and when it has, the
//! copy goes and the board's own card is what is there — so the board is
//! never a frame ahead of what it has shown, and a board redrawn mid-
//! flight (the next message, a window resize) only moves the destination,
//! which the flight re-reads every frame.
//!
//! **Why not move the real node:** `raise_hand` does, for a hovered or
//! dragged hand card, because that card's node lives through the hover.
//! A moving card's node does not live through the redraw that moves it,
//! and a flying copy is the one thing on the board that does not care
//! which redraw it is over. It is `Pickable::IGNORE`, so a click through
//! it lands on the board, and it is drawn under the actions menu and the
//! sheets, so nothing a person is reading is covered by a card in flight.
//!
//! **The destination is found by the same walk, every frame**, never
//! remembered as an entity: the entity dies with the next redraw, the
//! place does not.

use std::time::Duration;

use bevy::prelude::*;

use netrunner_client::board::{Target, Transition};
use netrunner_client::card_face::Face;
use netrunner_core::rules::Side;

use crate::card_images::CardImages;
use crate::core::ClientCore;
use crate::models::flight::{self, Place, Plan, Survey};
use crate::models::layout::Anchor;
use crate::screens::game::{AvatarBar, Click, Dirty, DropPlace, Ghost, HostedChip, Model, ServerColumn, ServerPlate};
use crate::screens::AppScreen;
use crate::theme::Theme;
use crate::widgets::anchor_of;
use crate::widgets::card_face::{spawn_back, spawn_face, FaceSize};

/// Drawn over the board and the pop-up, under the actions menu (15) and
/// the sheets (20): a card in flight covers nothing a person is reading.
const FLIGHT_LAYER: i32 = 11;

/// The transitions a redraw took, with the board they were read against,
/// waiting for the board after it to be laid out.
struct Pending {
    transitions: Vec<Transition>,
    before: Survey,
    /// Frames since the redraw: the new board is laid out after the
    /// first.
    frames: u8,
}

/// Where the board's boxes are this frame, and what is waiting to fly.
#[derive(Resource, Default)]
pub struct Flights {
    survey: Survey,
    /// The entity drawn at each of `survey`'s entries.
    entities: Vec<Entity>,
    pending: Option<Pending>,
}

impl Flights {
    /// The entity drawn at the `nth` box for `place` this frame.
    fn entity_at(&self, place: &Place, nth: usize) -> Option<Entity> {
        self.survey.find(place, nth).map(|index| self.entities[index])
    }
}

/// A card in flight: its plan, when it was launched, and the face width
/// its copy was spawned at, which the box it is drawn in is scaled from.
#[derive(Component, Debug, Clone)]
pub struct Flight {
    pub plan: Plan,
    launched: Duration,
    face: f32,
}

/// The board's own card at the end of a flight, hidden until it lands.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arriving;

type Boxes<'w, 's> = Query<
    'w,
    's,
    (Entity, Option<&'static Click>, Option<&'static ServerPlate>, Option<&'static ServerColumn>, Option<&'static AvatarBar>, Option<&'static DropPlace>, Has<Ghost>, Has<HostedChip>, &'static ComputedNode, &'static UiGlobalTransform),
    Or<(With<Click>, With<ServerPlate>, With<ServerColumn>, With<AvatarBar>, With<DropPlace>)>,
>;

/// The place a drawn node stands for, if any. A hosted Trojan's chip and
/// its ghost both carry the Trojan's `Install` target; the rig face is
/// the card, so neither stands for it.
fn place_of(click: Option<&Click>, plate: Option<&ServerPlate>, column: Option<&ServerColumn>, bar: Option<&AvatarBar>, drop: Option<&DropPlace>, ghost: bool, chip: bool) -> Option<Place> {
    if ghost || chip {
        return None;
    }
    if let Some(plate) = plate {
        return Some(Place::Plate(plate.0));
    }
    if let Some(bar) = bar {
        return Some(Place::Bar(bar.side));
    }
    if let Some(column) = column {
        return Some(Place::Column(column.0));
    }
    match click {
        Some(Click::Target(Target::Install(id))) => Some(Place::Install(*id)),
        Some(Click::Target(Target::HandCard(card))) => Some(Place::HandCard(card.clone())),
        Some(Click::Target(Target::Pile(pile))) => Some(Place::Pile(*pile)),
        _ => drop.filter(|drop| drop.0.contains(&Target::Rig)).map(|_| Place::Rig),
    }
}

/// Reads the laid-out board into a survey, left to right and top to
/// bottom, so the `nth` copy of a card in the hand is the `nth` from the
/// left.
fn survey(boxes: &Boxes) -> (Survey, Vec<Entity>) {
    let mut found: Vec<(Place, Anchor, Entity)> = boxes
        .iter()
        .filter_map(|(entity, click, plate, column, bar, drop, ghost, chip, node, transform)| place_of(click, plate, column, bar, drop, ghost, chip).map(|place| (place, anchor_of(node, transform), entity)))
        .collect();
    found.sort_by(|a, b| a.1.x.total_cmp(&b.1.x).then(a.1.y.total_cmp(&b.1.y)));
    let entities = found.iter().map(|(_, _, entity)| *entity).collect();
    (Survey { entries: found.into_iter().map(|(place, anchor, _)| (place, anchor)).collect() }, entities)
}

/// Runs after `poll` has applied the messages and before `redraw` takes
/// their transitions: surveys the board as it stands, launches the
/// flights a redraw a frame ago called for, notes the transitions the
/// redraw this frame is about to take, moves every card in the air and
/// hides the card each is landing on.
#[allow(clippy::too_many_arguments)]
pub(crate) fn flights(
    mut commands: Commands,
    mut state: ResMut<Flights>,
    dirty: Res<Dirty>,
    model: Option<Res<Model>>,
    time: Res<Time>,
    core: Res<ClientCore>,
    theme: Res<Theme>,
    images: Res<CardImages>,
    roots: Query<Entity, (With<DespawnOnExit<AppScreen>>, With<Node>)>,
    boxes: Boxes,
    mut flying: Query<(Entity, &mut Flight, &mut Node, &mut UiTransform, &mut Visibility), Without<Arriving>>,
    mut arriving: Query<(Entity, &mut Visibility), With<Arriving>>,
    dev: Option<ResMut<crate::dev::Dev>>,
) {
    let Some(model) = model else { return };
    // `NETRUNNER_HOLD_FLIGHT`: once the autoplay is done, every flight
    // freezes halfway and none lands, so a screenshot catches them.
    let mut dev = dev;
    let held = dev.as_ref().is_some_and(|dev| dev.hold_flight && dev.autoplayed >= dev.autoplay);
    let game = &model.0;
    let (survey, entities) = survey(&boxes);
    state.survey = survey;
    state.entities = entities;
    let now = time.elapsed();
    let speed = core.settings.desktop.animation_speed;

    // Instant: nothing flies, and whatever was in the air lands now.
    let Some(flight) = flight::flight_for(speed) else {
        state.pending = None;
        for (entity, ..) in &flying {
            commands.entity(entity).despawn();
        }
        for (entity, mut visibility) in &mut arriving {
            *visibility = Visibility::Inherited;
            commands.entity(entity).remove::<Arriving>();
        }
        return;
    };

    // The cards the flights in the air are landing on, hidden below until
    // they have: a flight launched this frame is not in `flying` until
    // the next, and its card would show for that frame.
    let mut landing_on: Vec<(Place, usize)> = Vec::new();

    // The board redrawn a frame ago is laid out: plan against it.
    if state.pending.as_ref().is_some_and(|pending| pending.frames >= 1) {
        let Pending { transitions, before, .. } = state.pending.take().expect("checked");
        if let Some(root) = roots.iter().next() {
            for plan in flight::plan(&transitions, &before, &state.survey, game.side) {
                landing_on.push((plan.to.place.clone(), plan.to.nth));
                commands.entity(root).with_children(|parent| launch(parent, &theme, &core, &images, plan, now));
            }
        }
    } else if let Some(pending) = state.pending.as_mut() {
        pending.frames += 1;
    }

    // A redraw this frame will take these: remember the board they left.
    if dirty.board() && !game.transitions.is_empty() {
        state.pending = Some(Pending { transitions: game.transitions.clone(), before: state.survey.clone(), frames: 0 });
    }

    // Move what is in the air, and land what has arrived.
    for (entity, mut in_flight, mut node, mut transform, mut visibility) in &mut flying {
        let Some(mut t) = flight::progress(in_flight.launched, in_flight.plan.order, flight, speed, now) else {
            // Not yet set off: its card waits hidden where it is.
            landing_on.push((in_flight.plan.to.place.clone(), in_flight.plan.to.nth));
            continue;
        };
        if held {
            t = t.min(0.5);
            if t >= 0.5 && let Some(dev) = dev.as_mut() {
                dev.flight_held = true;
            }
        }
        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        landing_on.push((in_flight.plan.to.place.clone(), in_flight.plan.to.nth));
        // The destination as the board draws it now, in case it moved.
        if let Some(index) = state.survey.find(&in_flight.plan.to.place, in_flight.plan.to.nth) {
            in_flight.plan.to.anchor = state.survey.anchor(index);
        }
        let box_ = flight::between(in_flight.plan.from.anchor, in_flight.plan.to.anchor, t);
        let face = in_flight.face;
        node.left = px(box_.x - face / 2.0);
        node.top = px(box_.y - face * 1.4 / 2.0);
        let scale = if face > 0.0 { (box_.width / face).max(0.05) } else { 1.0 };
        transform.scale = Vec2::splat(scale);
        *visibility = Visibility::Inherited;
    }

    // The card each flight lands on is hidden until it has; every other
    // hidden card is shown again.
    let mut hide: Vec<Entity> = Vec::new();
    for (place, nth) in &landing_on {
        if flight::hides(place) && let Some(entity) = state.entity_at(place, *nth) {
            hide.push(entity);
        }
    }
    for (entity, mut visibility) in &mut arriving {
        if !hide.contains(&entity) {
            *visibility = Visibility::Inherited;
            commands.entity(entity).remove::<Arriving>();
        }
    }
    for entity in hide {
        if let Ok((_, mut visibility)) = arriving.get_mut(entity) {
            if *visibility != Visibility::Hidden {
                *visibility = Visibility::Hidden;
            }
        } else if let Ok(mut entity) = commands.get_entity(entity) {
            entity.insert((Arriving, Visibility::Hidden));
        }
    }
}

/// Spawns the flying copy for `plan`: the card's face, or the back of
/// the side it belongs to, at the width of the face it leaves or lands
/// on, hidden until its stagger is up.
fn launch(parent: &mut ChildSpawnerCommands, theme: &Theme, core: &ClientCore, images: &CardImages, plan: Plan, now: Duration) {
    // A card arriving at a face is drawn at that face's width, so its
    // picture is the one the board already decoded; one leaving a face
    // for a pile is drawn at the width it left.
    let face = if flight::hides(&plan.to.place) { plan.to.anchor.width } else { plan.from.anchor.width };
    let face = face.round().max(1.0);
    let size = FaceSize::Board(face as u16);
    let start = plan.from.anchor;
    let back: Side = plan.back;
    let card = plan.card.clone();
    let mut copy = parent.spawn((
        Flight { plan, launched: now, face },
        Node { position_type: PositionType::Absolute, left: px(start.x - face / 2.0), top: px(start.y - face * 1.4 / 2.0), width: px(face), height: px(face * 1.4), ..default() },
        UiTransform { scale: Vec2::splat(if face > 0.0 { (start.width / face).max(0.05) } else { 1.0 }), ..UiTransform::IDENTITY },
        GlobalZIndex(FLIGHT_LAYER),
        Pickable::IGNORE,
        Visibility::Hidden,
    ));
    copy.with_children(|copy| match card.as_ref().and_then(|id| core.registry.get(id)) {
        Some(def) => {
            let drawn = Face::of(def, &core.settings.art);
            let image = images.of(&drawn, size).or_else(|| drawn.picture.as_ref().and_then(|picture| images.nearest_face(picture)));
            spawn_face(copy, theme, &drawn, size, image, Pickable::IGNORE);
        }
        None => {
            spawn_back(copy, theme, images.back(back), back, size, Pickable::IGNORE);
        }
    });
}
