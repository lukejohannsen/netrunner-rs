//! What a player and a server sign about a game (Phase 4 §5 stage c,
//! `docs/identity-and-rating.md` §3): a seat commitment from each proved
//! player, and a receipt from the server when the game ends.
//!
//! **The question they answer is narrow:** can a player deny a game, and
//! can a server invent one? A player signs once, when seated, that they
//! sat this match on this side with this deck against this key; the
//! server signs the finished record's hash with the result. Rejected for
//! now: a signature per action, a hash chain over each player's moves.
//! That is what someone who does not trust the server would need to check
//! a game (federation, a rating that travels), and it costs a signature on
//! the hottest message in the protocol while a rating is one operator's
//! claim anyway. `Receipt::action_chain` holds the room.
//!
//! **Each is signed as the payload text it is** (`netrunner_identity::Signed`):
//! these structs are what the text says, parsed after the signature over
//! it has been checked, never re-serialized to be verified.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use netrunner_core::rules::{Deck, Side};
use netrunner_identity::{sha256_hex, PublicKey, Signed};
use netrunner_session::GameEndReason;

/// The tag a seat commitment is signed under.
pub const SEAT_TAG: &[u8] = b"netrunner-seat-v1";
/// The tag a receipt is signed under.
pub const RECEIPT_TAG: &[u8] = b"netrunner-receipt-v1";
/// The tag a tournament registration is signed under.
pub const REGISTRATION_TAG: &[u8] = b"netrunner-registration-v1";

/// A player's word that they entered a tournament with these two decks
/// (Phase 4 §7 stage 6a): which event at which server, as which key,
/// each deck as its salted `deck_hash`. Signed by the player and
/// published by the server beside the registration, it is the player's
/// receipt that the server deals every one of their games from a list
/// they chose and the server did not alter: at the reveal, the list and
/// the salt check against it. The salt is the player's own, kept by both.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistrationStatement {
    pub tournament: String,
    pub server_key: PublicKey,
    pub key: PublicKey,
    pub corp_hash: String,
    pub runner_hash: String,
}

/// A player's word that they sat a match: which one, at which server, on
/// which side, as which key against which, with which deck.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatStatement {
    pub match_id: Uuid,
    pub server_key: PublicKey,
    pub side: Side,
    pub key: PublicKey,
    /// `None` when the other seat proved no key.
    pub opponent_key: Option<PublicKey>,
    /// `deck_hash` of the deck this seat plays, salted.
    pub deck_hash: String,
    /// Unix seconds.
    pub started_at: u64,
    /// For a tournament table's game, the seat's half of the shuffle
    /// (Phase 4 §7 stage 6c): the table, the commitment to the server's
    /// secret the player saw before sitting, and the nonce the player sat
    /// with. `None` for a game outside a tournament. The player's
    /// signature over it is what keeps a server from swapping the nonce
    /// or the commitment after the fact: the reveal must agree with it.
    pub table: Option<TableSeat>,
}

/// A tournament seat's terms for the game's seed: which table, the
/// commitment the player saw, and the nonce the player brought.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableSeat {
    pub tournament: String,
    /// Into the tournament's rounds, from 0.
    pub round: usize,
    /// Into the round's tables, from 0.
    pub table: usize,
    pub seed_commitment: String,
    pub nonce: String,
}

/// The tag every seed hash is taken under, so neither hash can be
/// mistaken for any other SHA-256 in the protocol.
const SEED_DOMAIN: &str = "netrunner-seed-v1";

/// The longest nonce a seat may bring, and the only characters it may
/// hold: the hash below separates its parts with newlines, so a nonce
/// that held one could be read two ways.
pub const MAX_NONCE: usize = 64;

/// Whether `nonce` is one a seat may bring: 1 to `MAX_NONCE` ASCII
/// letters and digits.
pub fn nonce_is_valid(nonce: &str) -> bool {
    (1..=MAX_NONCE).contains(&nonce.len()) && nonce.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

/// What the server publishes when a round is paired, for each table: a
/// hash of a secret it holds (Phase 4 §7 stage 6c). Published before
/// either player brings a nonce, so the secret cannot be chosen to suit
/// them; revealed with the result, so anyone can check it was the one.
pub fn seed_commitment(secret: &str) -> String {
    sha256_hex(format!("{SEED_DOMAIN}\ncommit\n{secret}").as_bytes())
}

/// A tournament game's seed: the server's secret and both players'
/// nonces, hashed together, the first eight bytes read as a number. No
/// one of the three chooses it: the server fixed its secret before the
/// nonces existed, and each player chose theirs knowing only the
/// secret's hash.
pub fn table_seed(secret: &str, corp_nonce: &str, runner_nonce: &str) -> u64 {
    let hash = sha256_hex(format!("{SEED_DOMAIN}\nseed\n{secret}\n{corp_nonce}\n{runner_nonce}").as_bytes());
    u64::from_str_radix(&hash[..16], 16).expect("a SHA-256 is hex")
}

/// One table's seed as a tournament publishes it: the commitment from
/// the moment the round was paired, and — once the table has a result —
/// the reveal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableSeed {
    /// Into the tournament's rounds, from 0.
    pub round: usize,
    /// Into the round's tables, from 0.
    pub table: usize,
    pub commitment: String,
    pub reveal: Option<SeedReveal>,
}

/// The server's secret, revealed with a table's result, and the two
/// nonces its game was seeded with — `None` for a table whose game never
/// started (a no-show recorded, a forfeit, a draw agreed at the table).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeedReveal {
    pub secret: String,
    pub corp_nonce: Option<String>,
    pub runner_nonce: Option<String>,
}

impl TableSeed {
    /// Checks the reveal against the commitment: `Ok(Some(seed))` for a
    /// game that was played, the seed it must have been dealt from;
    /// `Ok(None)` for one never revealed, or revealed with no game;
    /// `Err` when the secret is not the one committed to.
    pub fn check(&self) -> Result<Option<u64>, String> {
        let Some(reveal) = &self.reveal else { return Ok(None) };
        if seed_commitment(&reveal.secret) != self.commitment {
            return Err("the revealed secret is not the one the server committed to".to_string());
        }
        Ok(match (&reveal.corp_nonce, &reveal.runner_nonce) {
            (Some(corp), Some(runner)) => Some(table_seed(&reveal.secret, corp, runner)),
            _ => None,
        })
    }
}

/// The hash a seat commitment names a deck by: SHA-256 of a salt and the
/// deck's canonical text — the identity, then each card and its count in
/// card order, one per line, with copies of a card merged.
///
/// **Salted, and the salt told to that seat alone.** A receipt carries
/// both commitments to both players, and a well-known decklist's hash
/// would be confirmed by anyone who guessed the list and hashed it: the
/// commitment would leak exactly what Phase 4 §7 keeps private. The
/// server and the seat know the salt; the seat checks the hash against
/// its own deck before it signs, and either can reveal the salt later to
/// show which deck was played.
pub fn deck_hash(salt: &str, deck: &Deck) -> String {
    let mut cards: std::collections::BTreeMap<String, u32> = std::collections::BTreeMap::new();
    for (card, count) in &deck.cards {
        *cards.entry(card_text(card)).or_default() += count;
    }
    let mut text = format!("{salt}\n{}\n", card_text(&deck.identity));
    for (card, count) in cards {
        text.push_str(&format!("{count} {card}\n"));
    }
    sha256_hex(text.as_bytes())
}

/// A card id as the JSON it is on the wire, so the canonical text does not
/// depend on a `Display` nobody promised to keep.
fn card_text(card: &netrunner_core::dsl::CardId) -> String {
    serde_json::to_string(card).expect("a card id serializes")
}

/// What the server signs when a match ends: the record's hash with the
/// result, both seats and their commitments, and what the record can only
/// be replayed against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    pub match_id: Uuid,
    pub server_key: PublicKey,
    pub corp: ReceiptSeat,
    pub runner: ReceiptSeat,
    /// `None` for a stall, which is nobody's.
    pub winner: Option<Side>,
    pub reason: Option<GameEndReason>,
    /// Whether the game counts: two different proved keys, and a winner.
    pub rated: bool,
    /// The server's build, and a hash of the card pool it played with:
    /// "this record replays" is only checkable against the rules it was
    /// played under, and a record older than a rules change fails replay
    /// with `ReplayError::Diverged`.
    pub engine: String,
    pub card_pool: String,
    /// SHA-256 of the record file (`matches/<yyyy-mm>/<match id>.jsonl`),
    /// exactly as written.
    pub record: String,
    pub started_at: u64,
    pub ended_at: u64,
    /// Reserved for a per-action hash chain; always `None` today.
    pub action_chain: Option<String>,
    /// A tournament table's game: its seed's commitment and reveal, so
    /// the record this receipt hashes can be checked to have been dealt
    /// from the seed the three parties made (`TableSeed::check` against
    /// the record header's `seed`). `None` outside a tournament.
    pub table: Option<TableSeed>,
}

/// One seat as a receipt names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptSeat {
    /// The label the seat played under; a bot's seat name for a bot.
    pub name: String,
    /// The key it proved, if it did.
    pub key: Option<PublicKey>,
    /// Its signed `SeatStatement`, if it gave one. A proved seat that
    /// withholds its signature is still rated — withholding must not be
    /// a way to make a lost game count for nothing — and its receipt
    /// simply lacks the commitment.
    pub commitment: Option<Signed>,
}

impl Receipt {
    /// The receipt a signed statement holds, if `server_key` signed it
    /// under `RECEIPT_TAG` and it parses.
    pub fn read(signed: &Signed, server_key: &PublicKey) -> Result<Receipt, String> {
        if signed.key != *server_key {
            return Err(format!("signed by {}, not this server ({})", signed.key.fingerprint(), server_key.fingerprint()));
        }
        let payload = signed.verify(RECEIPT_TAG).map_err(|error| error.to_string())?;
        serde_json::from_str(payload).map_err(|error| format!("not a receipt: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The commitment holds only for its secret; the seed moves with each
    /// of the three parts; a reveal with no game has no seed to check.
    #[test]
    fn a_table_seed_is_made_by_all_three_and_checked_against_the_commitment() {
        let commitment = seed_commitment("secret");
        let mut seed = TableSeed { round: 0, table: 0, commitment: commitment.clone(), reveal: None };
        assert_eq!(seed.check(), Ok(None), "nothing revealed yet");
        seed.reveal = Some(SeedReveal { secret: "secret".into(), corp_nonce: Some("a1".into()), runner_nonce: Some("b2".into()) });
        assert_eq!(seed.check(), Ok(Some(table_seed("secret", "a1", "b2"))));
        let made = table_seed("secret", "a1", "b2");
        assert!(made != table_seed("secret2", "a1", "b2") && made != table_seed("secret", "a2", "b2") && made != table_seed("secret", "a1", "b3"));
        assert_ne!(made, table_seed("secret", "b2", "a1"), "the chairs are not interchangeable");
        seed.reveal = Some(SeedReveal { secret: "other".into(), corp_nonce: Some("a1".into()), runner_nonce: Some("b2".into()) });
        assert!(seed.check().is_err(), "a secret other than the committed one");
        seed.reveal = Some(SeedReveal { secret: "secret".into(), corp_nonce: None, runner_nonce: None });
        assert_eq!(seed.check(), Ok(None), "a table nobody played");
        assert!(nonce_is_valid("abc123") && !nonce_is_valid("") && !nonce_is_valid("a\nb") && !nonce_is_valid(&"a".repeat(MAX_NONCE + 1)));
    }

    #[test]
    fn a_deck_hash_depends_on_the_salt_and_the_list_and_not_its_order() {
        let deck = netrunner_core::decks::by_id("brick_stack").unwrap().to_deck();
        let mut reordered = deck.clone();
        reordered.cards.reverse();
        assert_eq!(deck_hash("s", &deck), deck_hash("s", &reordered));
        assert_ne!(deck_hash("s", &deck), deck_hash("t", &deck), "the salt changes it");
        let mut other = deck.clone();
        other.cards.pop();
        assert_ne!(deck_hash("s", &deck), deck_hash("s", &other));
    }
}
