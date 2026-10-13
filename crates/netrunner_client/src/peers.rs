//! The people this player has played directly, by name (Phase 7 §15):
//! the saved peers and recent connections under the Local / P2P hub,
//! each a Quick connect.
//!
//! **A peer is a person, a server is a place.** A server (`servers`) has
//! queues, a profile and a key it proved; a peer is a friend's address or
//! ticket, named by the name they gave the handshake. The two lists are
//! kept apart — this file beside `servers.json`, never rows of it — so
//! the servers page never lists a friend's laptop as a server to make
//! the Default, and the hub never lists the club's daemon as a friend.
//!
//! **A ticket is worth saving because a host's ticket is the host's
//! key** (`peer`, decided 12 October 2026): a friend who hosts on their
//! identity gives out the same ticket every time, and the relay finds
//! the key wherever their machine is. An address is worth saving where
//! it lasts — a LAN address, a port the router opens. Either is only a
//! way to reach a room that is open: Quick connect to a friend who is
//! not hosting fails with the machine's reason and keeps the row.
//!
//! **Written as the connection is made, read-change-write** as the
//! servers are (`PeerBook::update`): a join that gets as far as the room
//! is remembered, named by the host's name once the room says it; a
//! second join of the same target moves the row to the top with its new
//! time, and keeps the name the person gave it. Most recent first.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The file beside `identity.key` and `servers.json`.
pub const PEERS_FILE: &str = "peers.json";

/// The longest name a peer is given.
pub const MAX_NAME: usize = 40;

/// One person played directly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Peer {
    /// What the person calls them: the host's name from the room, until
    /// renamed.
    pub name: String,
    /// What is dialled: a `ws://` URL as `hosting::normalize_address`
    /// writes it, or a host's ticket.
    pub target: String,
    /// When a room of theirs was last joined, in Unix seconds.
    pub last_connected: u64,
}

/// Every peer, most recent first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeerBook {
    #[serde(default)]
    pub peers: Vec<Peer>,
}

/// `<identity dir>/peers.json`.
pub fn path_in(identity_dir: &Path) -> PathBuf {
    identity_dir.join(PEERS_FILE)
}

impl PeerBook {
    /// A missing file is nobody played yet, not an error.
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let json = std::fs::read_to_string(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
        serde_json::from_str(&json).map_err(|e| format!("{} is not a list of peers: {e}", path.display()))
    }

    /// Temp file and rename, like the settings.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
        }
        let json = serde_json::to_string_pretty(self).expect("a list of peers serializes");
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| format!("could not write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("could not replace {}: {e}", path.display()))
    }

    /// The file read, changed and written back in one call. Nothing is
    /// written when `change` refuses.
    pub fn update<T>(path: &Path, change: impl FnOnce(&mut PeerBook) -> Result<T, String>) -> Result<T, String> {
        let mut book = PeerBook::load(path)?;
        let answer = change(&mut book)?;
        book.save(path)?;
        Ok(answer)
    }

    pub fn find(&self, target: &str) -> Option<&Peer> {
        self.peers.iter().find(|peer| same(&peer.target, target))
    }

    /// A room joined: the row for `target` moved to the top with this
    /// time, named `name` if it is new — a row the person has named
    /// keeps its name.
    pub fn remember(&mut self, name: &str, target: &str, at: u64) {
        let target = target.trim();
        let kept = self.peers.iter().position(|peer| same(&peer.target, target)).map(|index| self.peers.remove(index));
        let name = kept.as_ref().map_or_else(|| trimmed_name(name, target), |peer| peer.name.clone());
        self.peers.insert(0, Peer { name, target: target.to_string(), last_connected: at });
    }

    pub fn rename(&mut self, target: &str, name: &str) -> Result<(), String> {
        let name = trimmed_name(name, target);
        let peer = self.peers.iter_mut().find(|peer| same(&peer.target, target)).ok_or_else(|| "that peer is no longer listed".to_string())?;
        peer.name = name;
        Ok(())
    }

    pub fn remove(&mut self, target: &str) -> Result<(), String> {
        let index = self.peers.iter().position(|peer| same(&peer.target, target)).ok_or_else(|| "that peer is no longer listed".to_string())?;
        self.peers.remove(index);
        Ok(())
    }
}

/// A change to the list, as a screen asks for it and the file takes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerEdit {
    Remember { name: String, target: String },
    Rename { target: String, name: String },
    Remove(String),
}

impl PeerEdit {
    pub fn apply(&self, book: &mut PeerBook) -> Result<(), String> {
        match self {
            PeerEdit::Remember { name, target } => {
                book.remember(name, target, crate::servers::now());
                Ok(())
            }
            PeerEdit::Rename { target, name } => book.rename(target, name),
            PeerEdit::Remove(target) => book.remove(target),
        }
    }
}

/// A name as the file keeps it: trimmed and bounded, the target itself
/// when nothing was given.
fn trimmed_name(name: &str, target: &str) -> String {
    let name: String = name.trim().chars().take(MAX_NAME).collect();
    if name.is_empty() { target.trim().to_string() } else { name }
}

fn same(a: &str, b: &str) -> bool {
    a.trim() == b.trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_join_is_remembered_most_recent_first_and_keeps_a_given_name() {
        let mut book = PeerBook::default();
        book.remember("Sam", "ws://192.168.1.50:8080", 10);
        book.remember("Luke", "endpoint7g8f", 20);
        assert_eq!(book.peers.iter().map(|peer| peer.name.as_str()).collect::<Vec<_>>(), ["Luke", "Sam"]);
        book.rename("ws://192.168.1.50:8080", "Sam (laptop)").unwrap();
        book.remember("Sam", " ws://192.168.1.50:8080 ", 30);
        assert_eq!((book.peers[0].name.as_str(), book.peers[0].last_connected), ("Sam (laptop)", 30), "a second join moves the row up and keeps its name");
        assert_eq!(book.peers.len(), 2);
        book.remember("", "ws://10.0.0.2:8080", 40);
        assert_eq!(book.peers[0].name, "ws://10.0.0.2:8080", "no name given: the target is the name");
        book.remove("endpoint7g8f").unwrap();
        assert!(book.find("endpoint7g8f").is_none());
        assert!(book.remove("endpoint7g8f").is_err());
    }

    #[test]
    fn the_file_is_written_whole_and_read_back() {
        let dir = std::env::temp_dir().join(format!("netrunner_peers_{}", std::process::id()));
        let path = path_in(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(PeerBook::load(&path).unwrap(), PeerBook::default(), "no file is nobody yet");
        PeerBook::update(&path, |book| PeerEdit::Remember { name: "Sam".into(), target: "ws://192.168.1.50:8080".into() }.apply(book)).unwrap();
        PeerBook::update(&path, |book| PeerEdit::Rename { target: "ws://192.168.1.50:8080".into(), name: "Samantha".into() }.apply(book)).unwrap();
        let book = PeerBook::load(&path).unwrap();
        assert_eq!(book.peers[0].name, "Samantha");
        assert!(PeerBook::update(&path, |book| PeerEdit::Remove("nobody".into()).apply(book)).is_err());
        assert_eq!(PeerBook::load(&path).unwrap(), book, "nothing is written when the change refuses");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
