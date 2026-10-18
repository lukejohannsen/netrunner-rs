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
//! a few hundred characters that nobody types (Phase 7 §7). **A paste
//! that reads nothing says why**, after the cursor, until the next key
//! (`TextField::note`): it used to do nothing at all, and an API key that
//! would not paste looked like a key combination the client ignored.
//!
//! **A masked field shows a dot per character** (`TextField::masked`):
//! an API key is typed or pasted once and never read back off the
//! screen, so a person at a shared desk or a screenshot gives nothing
//! away. The count of dots still says that something was typed.

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
    /// Drawn as one dot per character rather than the text: a secret.
    pub masked: bool,
    /// Why the last Ctrl+V put nothing in, drawn after the cursor and
    /// cleared by the next key. On the field rather than a screen's
    /// notice line, because every screen with a field would otherwise
    /// need a line of its own for it.
    pub note: Option<String>,
}

impl TextField {
    pub fn new(text: impl Into<String>, max_len: usize) -> Self {
        TextField { text: text.into(), max_len, masked: false, note: None }
    }

    pub fn masked(text: impl Into<String>, max_len: usize) -> Self {
        TextField { text: text.into(), max_len, masked: true, note: None }
    }

    /// What the field draws: the text, or a dot per character.
    pub fn shown(&self) -> String {
        if self.masked { "•".repeat(self.text.chars().count()) } else { self.text.clone() }
    }

    /// The line drawn in the field: what [`shown`](Self::shown) says, the
    /// cursor, and a failed paste's reason after it.
    pub fn line(&self) -> String {
        match &self.note {
            Some(note) => format!("{}|   {note}", self.shown()),
            None => format!("{}|", self.shown()),
        }
    }

    /// Pastes `text` as one line: its line breaks dropped, each line
    /// trimmed, control characters left out, cut at `max_len`.
    fn paste(&mut self, text: &str) {
        let line: String = text.lines().map(str::trim).collect();
        if line.is_empty() {
            self.note = Some("The clipboard holds no text to paste".to_string());
            return;
        }
        for c in line.chars().filter(|c| !c.is_control()) {
            push(self, c);
        }
    }
}

/// The clipboard's text, or why there is none to read. The clipboard is
/// for what nobody types — a ticket pasted in and copied out on the Online
/// screen, a NetrunnerDB link on the Decks screen, an API key into a
/// field — and a deck is a file (`files`); that is what the
/// `system_clipboard` feature is kept for.
pub(crate) fn read_clipboard(clipboard: Option<&mut bevy::clipboard::Clipboard>) -> Result<String, String> {
    let Some(clipboard) = clipboard else { return Err("There is no clipboard to read here".to_string()) };
    match clipboard.fetch_text() {
        bevy::clipboard::ClipboardRead::Ready(Ok(text)) => Ok(text),
        bevy::clipboard::ClipboardRead::Ready(Err(error)) => Err(format!("The clipboard could not be read: {error}")),
        _ => Err("The clipboard is not ready; try again".to_string()),
    }
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
            if !matches!(key, Key::Control | Key::Super | Key::Shift | Key::Alt) {
                field.note = None;
            }
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
                    match read_clipboard(clipboard.as_deref_mut()) {
                        Ok(text) => field.paste(&text),
                        Err(reason) => field.note = Some(reason),
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
                text.0 = field.line();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_paste_is_one_line_cut_at_the_limit() {
        let mut field = TextField::new("ab", 10);
        field.paste("  cd\r\n ef \n");
        assert_eq!(field.text, "abcdef");
        assert_eq!(field.note, None);
        field.paste("ghijklmnop");
        assert_eq!(field.text, "abcdefghij", "cut at max_len");
    }

    #[test]
    fn a_paste_of_no_text_says_so_and_a_masked_field_shows_dots() {
        let mut field = TextField::masked("", 400);
        field.paste(" \n ");
        assert_eq!(field.text, "");
        assert_eq!(field.line(), "|   The clipboard holds no text to paste");
        field.note = None;
        field.paste("sk-ant");
        assert_eq!(field.line(), "••••••|", "the key never shows");
    }
}
