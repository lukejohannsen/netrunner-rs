//! Who a player is to a server: an Ed25519 key the player holds, and the
//! signed statements that prove it (Phase 4 §5, `docs/identity-and-rating.md`).
//!
//! **A key is who someone is; a name is a label.** The rating a server
//! keeps is filed under `key:<base32>` (`PublicKey::rating_id`), beside the
//! `bot:` ids `bench` uses, so two people may both be called "luke" and a
//! person may rename themselves without starting the ladder again.
//! Accounts were rejected: a password means a user table, a reset flow and
//! a secret the *server* must protect, for a hobby server. A keypair puts
//! the only secret on the machine of the person it belongs to.
//!
//! **Proved by a challenge that names the server.** The server sends a
//! nonce and its own key; the client signs `AUTH_TAG ‖ server key ‖ nonce`
//! (`auth_statement`). The server's key inside the signed bytes is what
//! stops a hostile server relaying an honest one's nonce to a visiting
//! client and logging in there as them: the honest server finds a
//! signature made for somebody else's key and refuses. The tag keeps an
//! auth signature from ever being read as another kind of statement.
//!
//! **Pure.** No files, no clock and no randomness: a key is made from 32
//! bytes the caller drew (`Identity::from_secret`), and read and written
//! as text (`Identity::to_file_text`) by whoever owns the file — the
//! client its `identity.key`, the server its own, each mode 0600. That is
//! the split `netrunner_rating` keeps, and it keeps an RNG crate out of
//! something both ends and every test link.

use std::fmt;

use data_encoding::BASE32_NOPAD;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The domain tag of the login statement. A later kind of statement (a
/// seat commitment, a receipt) takes a tag of its own, so no signature is
/// ever good for two of them.
pub const AUTH_TAG: &[u8] = b"netrunner-auth-v1";

/// What begins an identity file, so a file of some other kind is refused
/// by name rather than read as 32 random bytes.
const FILE_HEADER: &str = "netrunner-identity-v1";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdentityError {
    #[error("not a key: {0}")]
    BadKey(String),
    #[error("not a signature: {0}")]
    BadSignature(String),
    #[error("not a nonce: {0}")]
    BadNonce(String),
    #[error("not a netrunner identity file")]
    BadFile,
    #[error("the signature does not prove that key")]
    Unproved,
}

/// A player's (or a server's) secret key. `Debug` prints the public half
/// only, so a key never reaches a log by accident; the secret is wiped
/// when it is dropped.
#[derive(Clone)]
pub struct Identity(SigningKey);

impl Identity {
    /// The key made from `secret`, which the caller drew from its own
    /// RNG — `rand::random()` in the client and the server.
    pub fn from_secret(secret: [u8; 32]) -> Self {
        Identity(SigningKey::from_bytes(&secret))
    }

    pub fn public_key(&self) -> PublicKey {
        PublicKey(self.0.verifying_key().to_bytes())
    }

    /// The answer to a server's challenge: this key's signature over
    /// `auth_statement(server_key, nonce)`.
    pub fn prove(&self, server_key: &PublicKey, nonce: &Nonce) -> Signature {
        Signature(self.0.sign(&auth_statement(server_key, nonce)).to_bytes())
    }

    /// The identity file's contents: a header line and the secret in
    /// base32. Text rather than 32 raw bytes, so a person copying the
    /// file to a second machine can see it is the right one.
    pub fn to_file_text(&self) -> String {
        format!("{FILE_HEADER}\n{}\n", encode(&self.0.to_bytes()))
    }

    pub fn from_file_text(text: &str) -> Result<Self, IdentityError> {
        let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
        if lines.next() != Some(FILE_HEADER) {
            return Err(IdentityError::BadFile);
        }
        let secret = lines.next().and_then(|line| decode::<32>(line).ok()).ok_or(IdentityError::BadFile)?;
        Ok(Identity::from_secret(secret))
    }
}

impl fmt::Debug for Identity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Identity").field(&self.public_key()).finish()
    }
}

/// A public key: who someone is to a server. On the wire and in files it
/// is its base32 text (`Display`, `FromStr`); a string that is not a valid
/// Ed25519 point is refused when it is read, never later when it is used.
///
/// **Held as its 32 bytes**, checked once when made, rather than as the
/// decompressed `VerifyingKey`: that is about 200 bytes, and a key rides in
/// messages, errors and events that are moved about far more often than
/// a proof is verified.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PublicKey([u8; 32]);

impl PublicKey {
    /// The id a server's rating book files this key under.
    pub fn rating_id(&self) -> String {
        format!("key:{self}")
    }

    /// The first twelve characters of the key in three groups
    /// (`abcd-efgh-ijkl`): what a person reads to tell two keys apart,
    /// where the whole key is 52 characters. Sixty bits, which is plenty
    /// for a glance and never a substitute for comparing the whole key.
    pub fn fingerprint(&self) -> String {
        let text = self.to_string();
        format!("{}-{}-{}", &text[0..4], &text[4..8], &text[8..12])
    }

    pub fn to_bytes(&self) -> [u8; 32] {
        self.0
    }

    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self, IdentityError> {
        VerifyingKey::from_bytes(bytes).map(|_| PublicKey(*bytes)).map_err(|error| IdentityError::BadKey(error.to_string()))
    }

    /// Whether `signature` is this key's answer to `nonce` from the server
    /// whose key is `server_key`. Strict verification: a signature a
    /// lenient verifier would also take (a small-order key, a
    /// non-canonical encoding) is refused.
    pub fn verify_proof(&self, server_key: &PublicKey, nonce: &Nonce, signature: &Signature) -> Result<(), IdentityError> {
        let signature = ed25519_dalek::Signature::from_bytes(&signature.0);
        // Checked when this key was made, so it decompresses.
        let key = VerifyingKey::from_bytes(&self.0).map_err(|_| IdentityError::Unproved)?;
        key.verify_strict(&auth_statement(server_key, nonce), &signature).map_err(|_| IdentityError::Unproved)
    }
}

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&encode(&self.to_bytes()))
    }
}

impl fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PublicKey({self})")
    }
}

impl std::str::FromStr for PublicKey {
    type Err = IdentityError;

    /// The base32 key, with or without `rating_id`'s `key:` in front.
    fn from_str(text: &str) -> Result<Self, IdentityError> {
        let text = text.trim();
        let text = text.strip_prefix("key:").unwrap_or(text);
        let bytes = decode::<32>(text).map_err(IdentityError::BadKey)?;
        PublicKey::from_bytes(&bytes)
    }
}

/// What a server asks a key to sign: fresh for every connection, so a
/// proof is good for the connection it was made on and no other.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Nonce(pub [u8; 32]);

impl fmt::Debug for Nonce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Nonce({})", encode(&self.0))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Signature(pub [u8; 64]);

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Signature({})", encode(&self.0))
    }
}

/// The bytes a login signs: `AUTH_TAG ‖ server key ‖ nonce`. Fixed-length
/// fields after a fixed tag, so there is nothing to delimit and no
/// encoding to get wrong — the statement is signed as bytes and never as
/// re-serialized JSON.
pub fn auth_statement(server_key: &PublicKey, nonce: &Nonce) -> Vec<u8> {
    [AUTH_TAG, &server_key.to_bytes(), &nonce.0].concat()
}

/// Base32, RFC 4648, lower case and unpadded: no characters that need
/// escaping in a URL, a file name or JSON, and nothing a person reading a
/// fingerprint aloud confuses with case.
fn encode(bytes: &[u8]) -> String {
    BASE32_NOPAD.encode(bytes).to_ascii_lowercase()
}

fn decode<const N: usize>(text: &str) -> Result<[u8; N], String> {
    let bytes = BASE32_NOPAD.decode(text.trim().to_ascii_uppercase().as_bytes()).map_err(|error| error.to_string())?;
    <[u8; N]>::try_from(bytes).map_err(|bytes| format!("{} bytes, not {N}", bytes.len()))
}

impl Serialize for PublicKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for PublicKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?.parse().map_err(serde::de::Error::custom)
    }
}

impl Serialize for Nonce {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for Nonce {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        decode(&text).map(Nonce).map_err(|error| serde::de::Error::custom(IdentityError::BadNonce(error)))
    }
}

impl Serialize for Signature {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for Signature {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        decode(&text).map(Signature).map_err(|error| serde::de::Error::custom(IdentityError::BadSignature(error)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(byte: u8) -> Identity {
        Identity::from_secret([byte; 32])
    }

    #[test]
    fn a_proof_verifies_for_its_key_its_server_and_its_nonce_only() {
        let (player, server, other_server) = (key(1), key(2).public_key(), key(3).public_key());
        let nonce = Nonce([7; 32]);
        let proof = player.prove(&server, &nonce);
        assert_eq!(player.public_key().verify_proof(&server, &nonce, &proof), Ok(()));
        assert_eq!(key(4).public_key().verify_proof(&server, &nonce, &proof), Err(IdentityError::Unproved), "another key");
        assert_eq!(player.public_key().verify_proof(&server, &Nonce([8; 32]), &proof), Err(IdentityError::Unproved), "another nonce");
        // The relay: a hostile server passes an honest one's nonce to a
        // visitor, who signs for the server it is talking to.
        assert_eq!(player.public_key().verify_proof(&other_server, &nonce, &proof), Err(IdentityError::Unproved), "another server");
    }

    #[test]
    fn a_key_survives_its_file_and_the_wire() {
        let identity = key(9);
        let back = Identity::from_file_text(&identity.to_file_text()).unwrap();
        assert_eq!(back.public_key(), identity.public_key());
        assert_eq!(Identity::from_file_text("something else\nabc\n").unwrap_err(), IdentityError::BadFile);

        let public = identity.public_key();
        let json = serde_json::to_string(&public).unwrap();
        assert_eq!(json, format!("\"{public}\""));
        assert_eq!(serde_json::from_str::<PublicKey>(&json).unwrap(), public);
        assert_eq!(public.rating_id().parse::<PublicKey>().unwrap(), public, "a rating id reads back as its key");
        assert_eq!(public.to_string().len(), 52);
        assert!(public.to_string().chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
        assert_eq!(public.fingerprint().len(), 14);

        let proof = identity.prove(&public, &Nonce([1; 32]));
        let json = serde_json::to_string(&(Nonce([1; 32]), proof)).unwrap();
        assert_eq!(serde_json::from_str::<(Nonce, Signature)>(&json).unwrap(), (Nonce([1; 32]), proof));
    }

    #[test]
    fn a_key_that_is_not_a_point_or_not_32_bytes_is_refused_when_read() {
        assert!(matches!("abc".parse::<PublicKey>(), Err(IdentityError::BadKey(_))));
        assert!(serde_json::from_str::<PublicKey>("\"not base32!\"").is_err());
    }

    #[test]
    fn debug_never_prints_the_secret() {
        let identity = key(5);
        let secret = encode(&[5; 32]);
        let debug = format!("{identity:?}");
        assert!(!debug.contains(&secret), "{debug}");
        assert!(debug.contains(&identity.public_key().to_string()));
    }
}
