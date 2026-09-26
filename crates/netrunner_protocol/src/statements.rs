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
