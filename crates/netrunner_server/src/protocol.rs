//! The wire messages, which live in `netrunner_protocol` so a client can
//! name them without depending on the server. Re-exported whole, so every
//! `netrunner_server::protocol::…` path keeps resolving.

pub use netrunner_protocol::*;
