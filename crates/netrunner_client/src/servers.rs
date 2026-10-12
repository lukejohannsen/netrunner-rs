//! The servers this player knows, by name (Phase 7 §14): one list both
//! clients read, the connections manager edits and the connection driver
//! writes a server's key into.
//!
//! **One row a server, wherever it was met.** It replaced two things that
//! each held half of it: `known_servers.json`, the servers' keys by
//! address with no name and nothing that showed it, and `Settings::server`,
//! the last address connected to. A server is a row the first time a
//! connection to it is made or its key is met, named by its address until
//! the person names it; one row may be the **Default**, the server Play
//! Online connects to on opening; and where nothing is the default, Join
//! starts from the one connected to last.
//!
//! **A server's key is remembered on first contact, known-hosts style**
//! (`docs/identity-and-rating.md` §2), when the server says it keeps its
//! key (`ServerMessage::Challenge::lasting`): a later connection to the
//! same address that meets another key is refused, naming both. Changing
//! a row's address forgets its key, since that is another server; a
//! ticket names its host's key itself and is never pinned
//! (`remote::Target::servers`).
//!
//! **Beside the key file, not in `settings.toml`**, for the reason
//! `known_servers.json` was: the connection driver writes the keys from a
//! task of its own while a screen holds the settings, and a struct saved
//! whole by one writer loses the other's change. Every change here is
//! therefore a read, a change and a write of the file in one call
//! (`ServerBook::update`), never a copy kept and saved later. The file is
//! not a secret.
//!
//! **Not the relay.** The design notes gave each server profile a relay;
//! the relay is what *hosting* asks n0 for to be reachable by ticket, and
//! a connection to a server never uses one, so it stays the one setting
//! it is (`Settings::relay`).

use std::path::{Path, PathBuf};

use netrunner_identity::PublicKey;
use serde::{Deserialize, Serialize};


/// The file beside `identity.key`.
pub const SERVERS_FILE: &str = "servers.json";

/// The longest name a server is given.
pub const MAX_NAME: usize = 40;

/// One server the player knows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Server {
    /// What the person calls it; its address until they say otherwise.
    pub name: String,
    /// What is dialled: a `ws://` URL as `hosting::normalize_address`
    /// writes it, or a host's ticket.
    pub address: String,
    /// The key it proved on first contact, if it keeps one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<PublicKey>,
    /// When a connection to it was last made, in Unix seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_connected: Option<u64>,
}

/// Every server the player knows, in the order they were met or added.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerBook {
    #[serde(default)]
    pub servers: Vec<Server>,
    /// The Default's address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

/// `<identity dir>/servers.json`: where a screen reads the book without
/// making a key, as `Credentials::servers` is where a connection does.
pub fn path_in(identity_dir: &Path) -> PathBuf {
    identity_dir.join(SERVERS_FILE)
}

/// Two addresses name one server when they are the same once trimmed.
fn same(a: &str, b: &str) -> bool {
    a.trim() == b.trim()
}

impl ServerBook {
    /// A missing file is nobody met yet, not an error.
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let json = std::fs::read_to_string(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
        serde_json::from_str(&json).map_err(|e| format!("{} is not a list of servers: {e}", path.display()))
    }

    /// Temp file and rename, like the settings.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
        }
        let json = serde_json::to_string_pretty(self).expect("a list of servers serializes");
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| format!("could not write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("could not replace {}: {e}", path.display()))
    }

    /// The file read, changed by `change` and written back in one call —
    /// the only way anything changes it, so the driver's key and a
    /// screen's name never overwrite each other. Nothing is written when
    /// `change` refuses.
    pub fn update<T>(path: &Path, change: impl FnOnce(&mut ServerBook) -> Result<T, String>) -> Result<T, String> {
        let mut book = ServerBook::load(path)?;
        let answer = change(&mut book)?;
        book.save(path)?;
        Ok(answer)
    }

    pub fn find(&self, address: &str) -> Option<&Server> {
        self.servers.iter().find(|server| same(&server.address, address))
    }

    fn position(&self, address: &str) -> Option<usize> {
        self.servers.iter().position(|server| same(&server.address, address))
    }

    /// The key remembered for the server at `address`.
    pub fn key_of(&self, address: &str) -> Option<PublicKey> {
        self.find(address).and_then(|server| server.key)
    }

    pub fn default_server(&self) -> Option<&Server> {
        let address = self.default.as_deref()?;
        self.find(address)
    }

    /// Where Join starts: the Default, else the server connected to last.
    pub fn start_address(&self) -> Option<&str> {
        self.default_server()
            .or_else(|| self.servers.iter().filter(|server| server.last_connected.is_some()).max_by_key(|server| server.last_connected))
            .map(|server| server.address.as_str())
    }

    /// A row for `address`, made if there is none, named by its address.
    fn row(&mut self, address: &str) -> &mut Server {
        let index = match self.position(address) {
            Some(index) => index,
            None => {
                let address = address.trim().to_string();
                self.servers.push(Server { name: address.clone(), address, key: None, last_connected: None });
                self.servers.len() - 1
            }
        };
        &mut self.servers[index]
    }

    /// A server's key met on first contact.
    pub fn remember_key(&mut self, address: &str, key: PublicKey) {
        self.row(address).key = Some(key);
    }

    /// A connection to `address` was made, at `at`.
    pub fn connected(&mut self, address: &str, at: u64) {
        self.row(address).last_connected = Some(at);
    }

    /// A name the person typed, checked against every row but `except`'s.
    fn checked_name(&self, name: &str, except: Option<usize>) -> Result<String, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("A server needs a name".to_string());
        }
        if name.chars().count() > MAX_NAME {
            return Err(format!("A name is at most {MAX_NAME} characters"));
        }
        if self.servers.iter().enumerate().any(|(index, server)| Some(index) != except && server.name.to_lowercase() == name.to_lowercase()) {
            return Err(format!("A server is already called {name}"));
        }
        Ok(name.to_string())
    }

    /// An address the person typed, as it is dialled, checked against
    /// every row but `except`'s.
    fn checked_address(&self, address: &str, except: Option<usize>) -> Result<String, String> {
        if address.trim().is_empty() {
            return Err("A server needs an address".to_string());
        }
        let address = crate::hosting::normalize_address(address);
        if let Some(other) = self.servers.iter().enumerate().find(|(index, server)| Some(*index) != except && same(&server.address, &address)).map(|(_, server)| server) {
            return Err(format!("{address} is already listed, as {}", other.name));
        }
        Ok(address)
    }

    /// A server the person added by hand. Its index.
    pub fn add(&mut self, name: &str, address: &str) -> Result<usize, String> {
        let name = self.checked_name(name, None)?;
        let address = self.checked_address(address, None)?;
        self.servers.push(Server { name, address, key: None, last_connected: None });
        Ok(self.servers.len() - 1)
    }

    /// The row at `address` renamed and readdressed. A new address is
    /// another server: its key is forgotten, and the Default follows it.
    pub fn edit(&mut self, address: &str, name: &str, new_address: &str) -> Result<(), String> {
        let index = self.position(address).ok_or_else(|| format!("{address} is not listed"))?;
        let name = self.checked_name(name, Some(index))?;
        let new_address = self.checked_address(new_address, Some(index))?;
        let server = &mut self.servers[index];
        server.name = name;
        if !same(&server.address, &new_address) {
            if self.default.as_deref().is_some_and(|default| same(default, &server.address)) {
                self.default = Some(new_address.clone());
            }
            let server = &mut self.servers[index];
            server.address = new_address;
            server.key = None;
            server.last_connected = None;
        }
        Ok(())
    }

    /// The row at `address` gone, and with it the Default if it was.
    pub fn remove(&mut self, address: &str) -> Result<(), String> {
        let index = self.position(address).ok_or_else(|| format!("{address} is not listed"))?;
        self.servers.remove(index);
        if self.default.as_deref().is_some_and(|default| same(default, address)) {
            self.default = None;
        }
        Ok(())
    }

    /// The Default is the row at `address`, or none.
    pub fn set_default(&mut self, address: Option<&str>) -> Result<(), String> {
        self.default = match address {
            Some(address) => Some(self.find(address).ok_or_else(|| format!("{address} is not listed"))?.address.clone()),
            None => None,
        };
        Ok(())
    }
}

/// A change a manager makes to the list, as a value: the screen decides
/// it, checks it against its copy and hands it to whatever writes the
/// file, which applies it to the file as it stands (`ServerBook::update`).
/// Both clients' managers make the same four.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerEdit {
    Add { name: String, address: String },
    /// The row at `address`, renamed and readdressed.
    Edit { address: String, name: String, new_address: String },
    Remove(String),
    /// The Default is the row at this address, or none.
    Default(Option<String>),
}

impl ServerEdit {
    pub fn apply(&self, book: &mut ServerBook) -> Result<(), String> {
        match self {
            ServerEdit::Add { name, address } => book.add(name, address).map(|_| ()),
            ServerEdit::Edit { address, name, new_address } => book.edit(address, name, new_address),
            ServerEdit::Remove(address) => book.remove(address),
            ServerEdit::Default(address) => book.set_default(address.as_deref()),
        }
    }

    /// What it would do to `book`, or why it cannot: the check a form
    /// makes before it lets the change go.
    pub fn check(&self, book: &ServerBook) -> Result<(), String> {
        self.apply(&mut book.clone())
    }
}

/// Now, in Unix seconds, for `ServerBook::connected`.
pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(seed: u8) -> PublicKey {
        netrunner_identity::Identity::from_secret([seed; 32]).public_key()
    }

    #[test]
    fn a_server_met_is_a_row_named_by_its_address_until_named() {
        let mut book = ServerBook::default();
        book.remember_key("ws://play.example.net:8080", key(1));
        book.connected(" ws://play.example.net:8080 ", 10);
        assert_eq!(book.servers, vec![Server { name: "ws://play.example.net:8080".into(), address: "ws://play.example.net:8080".into(), key: Some(key(1)), last_connected: Some(10) }]);
        book.edit("ws://play.example.net:8080", "The Club", "ws://play.example.net:8080").unwrap();
        assert_eq!(book.servers[0].name, "The Club");
        assert_eq!(book.key_of("ws://play.example.net:8080"), Some(key(1)), "a rename keeps the key");
    }

    #[test]
    fn join_starts_from_the_default_else_the_last_connected() {
        let mut book = ServerBook::default();
        assert_eq!(book.start_address(), None);
        book.add("Home", "localhost").unwrap();
        book.add("Club", "club.example.net:9000").unwrap();
        assert_eq!(book.start_address(), None, "never connected to, and no default");
        book.connected("ws://localhost:8080", 5);
        book.connected("ws://club.example.net:9000", 9);
        assert_eq!(book.start_address(), Some("ws://club.example.net:9000"));
        book.set_default(Some("ws://localhost:8080")).unwrap();
        assert_eq!(book.start_address(), Some("ws://localhost:8080"));
        book.remove("ws://localhost:8080").unwrap();
        assert_eq!(book.default, None, "a removed default is no default");
        assert_eq!(book.start_address(), Some("ws://club.example.net:9000"));
    }

    #[test]
    fn a_name_and_an_address_are_each_one_rows() {
        let mut book = ServerBook::default();
        book.add("Club", "club.example.net").unwrap();
        assert!(book.add("club", "other.example.net").unwrap_err().contains("already called"));
        assert!(book.add("Other", "ws://club.example.net:8080").unwrap_err().contains("already listed, as Club"));
        assert!(book.add("  ", "x").unwrap_err().contains("name"));
        assert!(book.add("X", " ").unwrap_err().contains("address"));
        assert!(book.add(&"n".repeat(MAX_NAME + 1), "y").unwrap_err().contains("at most"));
        book.add("Other", "other.example.net").unwrap();
        assert!(book.edit("ws://other.example.net:8080", "Club", "other.example.net").unwrap_err().contains("already called"));
    }

    #[test]
    fn a_new_address_is_another_server_and_the_default_follows_it() {
        let mut book = ServerBook::default();
        book.remember_key("ws://old.example.net:8080", key(2));
        book.connected("ws://old.example.net:8080", 3);
        book.set_default(Some("ws://old.example.net:8080")).unwrap();
        book.edit("ws://old.example.net:8080", "Moved", "new.example.net").unwrap();
        let moved = &book.servers[0];
        assert_eq!((moved.address.as_str(), moved.key, moved.last_connected), ("ws://new.example.net:8080", None, None));
        assert_eq!(book.default.as_deref(), Some("ws://new.example.net:8080"));
    }

    #[test]
    fn the_file_is_changed_in_one_call_and_a_refusal_writes_nothing() {
        let dir = std::env::temp_dir().join(format!("netrunner_client_servers_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join(SERVERS_FILE);
        assert_eq!(ServerBook::load(&path).unwrap(), ServerBook::default(), "no file is nobody met");
        ServerBook::update(&path, |book| book.add("Club", "club.example.net")).unwrap();
        ServerBook::update(&path, |book| {
            book.remember_key("ws://club.example.net:8080", key(3));
            Ok(())
        })
        .unwrap();
        assert!(ServerBook::update(&path, |book| book.add("Club", "elsewhere")).is_err());
        let book = ServerBook::load(&path).unwrap();
        assert_eq!((book.servers.len(), book.servers[0].name.as_str(), book.servers[0].key), (1, "Club", Some(key(3))), "the driver's key beside the screen's name");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
