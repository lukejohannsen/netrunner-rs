//! Every card in the catalog draws as a text face, without a window: no
//! layout panics, and what the face says is what `Face::of` laid out.
//!
//! Under `MinimalPlugins` there is no `Assets<Image>` and no symbol
//! font, so this is the fallback path — the glyphs are `Symbol::fallback`
//! — which is also the path a player with a missing font file gets.

use bevy::prelude::*;

use netrunner_client::card_face::Face;
use netrunner_desktop::theme::Theme;
use netrunner_desktop::widgets::card_face::{spawn_face, BodyText, FaceSize};

#[test]
fn every_catalog_card_draws_as_a_text_face_that_says_its_text() {
    let registry = netrunner_client::decks::sample_deck_registry();
    let catalog = netrunner_client::cards::catalog(&registry);
    assert!(catalog.len() > 200);
    let theme = Theme::default();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    let mut faces = Vec::new();
    for card in &catalog {
        let face = Face::of(card);
        let root = app.world_mut().spawn(Node::default()).id();
        let mut entity = None;
        app.world_mut().commands().entity(root).with_children(|parent| {
            entity = Some(spawn_face(parent, &theme, &face, FaceSize::Thumb, None, ()));
            spawn_face(parent, &theme, &face, FaceSize::Large, None, ());
            spawn_face(parent, &theme, &face, FaceSize::Board(180), None, ());
            spawn_face(parent, &theme, &face, FaceSize::Board(72), None, ());
        });
        app.world_mut().flush();
        faces.push((face, entity.unwrap()));
    }
    app.update();
    // The body text under each face reads, span by span, as the face's
    // fallback rendering, its arrows drawn as the font can.
    let mut bodies = app.world_mut().query_filtered::<(&ChildOf, &Children), With<BodyText>>();
    let mut spans = app.world_mut().query::<&TextSpan>();
    let mut checked = 0;
    for (face, root) in &faces {
        let expected = netrunner_desktop::widgets::symbols::arrowless(&face.body_text(false));
        let found = bodies
            .iter(app.world())
            .find(|(parent, _)| ancestor_is(app.world(), parent.parent(), *root))
            .map(|(_, children)| children.iter().filter_map(|child| spans.get(app.world(), child).ok()).map(|span| span.0.clone()).collect::<String>());
        assert_eq!(found.as_deref(), Some(expected.as_str()), "{}", face.title);
        checked += 1;
    }
    assert_eq!(checked, catalog.len());
}

fn ancestor_is(world: &World, mut entity: Entity, root: Entity) -> bool {
    loop {
        if entity == root {
            return true;
        }
        match world.get::<ChildOf>(entity) {
            Some(parent) => entity = parent.parent(),
            None => return false,
        }
    }
}
