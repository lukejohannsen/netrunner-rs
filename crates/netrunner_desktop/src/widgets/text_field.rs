//! A one-line text field driven by the keyboard.
//!
//! Bevy 0.19 ships `EditableText`, a full editor over parley with
//! selection, IME and clipboard. A player's name is one line of up to
//! thirty-two characters, edited the way the terminal client edits it —
//! type, Backspace, Enter commits, Escape cancels — and that model is
//! forty lines here against a widget whose focus and cursor plumbing the
//! screen would still have to own. If a screen ever needs more than a
//! line (deck notes), that is the point to adopt `EditableText`.
//!
//! **Ctrl+V (Cmd+V) pastes**, its line breaks dropped: a host's ticket is
//! a few hundred characters that nobody types (Phase 7 §7).

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::ButtonState;
use bevy::prelude::*;

use crate::nav::InputCaptured;

/// A field that is being edited. Only one exists at a time; the screen
/// that spawns it removes it on [`TextFieldEvent::Committed`] or
/// [`TextFieldEvent::Cancelled`].
#[derive(Component, Debug, Clone)]
pub struct TextField {
    pub text: String,
    pub max_len: usize,
}

/// What the field did this frame. A screen reads it off the component's
/// entity and decides what a commit means.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub enum TextFieldEvent {
    Committed(String),
    Cancelled,
}

/// Feeds key presses into every [`TextField`] and mirrors the text into
/// its first `Text` child. Holds [`InputCaptured`] while a field exists,
/// so Escape cancels the edit rather than leaving the screen. An Escape
/// a widget earlier in `nav::Captures` has already taken — an open
/// drop-down closing — is not also a cancel, or one key would do two
/// things; nor is an Enter, which an open drop-down's list takes to
/// choose its highlighted row.
pub fn edit_text_fields(
    mut commands: Commands,
    mut keys: MessageReader<KeyboardInput>,
    mut fields: Query<(Entity, &mut TextField, &Children)>,
    mut texts: Query<&mut Text>,
    mut captured: ResMut<InputCaptured>,
    held: Option<Res<ButtonInput<KeyCode>>>,
    mut clipboard: Option<ResMut<bevy::clipboard::Clipboard>>,
) {
    let command = held.is_some_and(|held| held.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight, KeyCode::SuperLeft, KeyCode::SuperRight]));
    let escape_taken = captured.0;
    if !fields.is_empty() {
        captured.0 = true;
    }
    let presses: Vec<Key> = keys.read().filter(|key| key.state == ButtonState::Pressed).map(|key| key.logical_key.clone()).collect();
    for (entity, mut field, children) in &mut fields {
        for key in &presses {
            match key {
                Key::Enter if escape_taken => {}
                Key::Enter => {
                    commands.entity(entity).insert(TextFieldEvent::Committed(field.text.trim().to_string()));
                }
                Key::Escape if escape_taken => {}
                Key::Escape => {
                    commands.entity(entity).insert(TextFieldEvent::Cancelled);
                }
                Key::Backspace => {
                    field.text.pop();
                }
                Key::Space => push(&mut field, ' '),
                Key::Character(chars) if command && chars.eq_ignore_ascii_case("v") => {
                    if let Some(bevy::clipboard::ClipboardRead::Ready(Ok(text))) = clipboard.as_deref_mut().map(|clipboard| clipboard.fetch_text()) {
                        for c in text.lines().map(str::trim).collect::<String>().chars().filter(|c| !c.is_control()) {
                            push(&mut field, c);
                        }
                    }
                }
                // Another shortcut, not text.
                Key::Character(_) if command => {}
                Key::Character(chars) => {
                    for c in chars.chars().filter(|c| !c.is_control()) {
                        push(&mut field, c);
                    }
                }
                _ => {}
            }
        }
        for child in children.iter() {
            if let Ok(mut text) = texts.get_mut(child) {
                text.0 = format!("{}|", field.text);
                break;
            }
        }
    }
}

fn push(field: &mut TextField, c: char) {
    if field.text.chars().count() < field.max_len {
        field.text.push(c);
    }
}
