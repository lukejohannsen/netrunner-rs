# Identity and rating

An architectural plan, not a description of what is built. The roadmap entry
that owns it is **Phase 4 §5** (`docs/roadmap/phase-4-network.md`), which lists
the stages; this file is the reasoning, the way `docs/jinteki-comparison.md` is
the reasoning behind Phase 7 §8. Nothing here is locked down — the point is
that the next person to touch the server knows where a rating is meant to live
and why, and does not build a second answer beside it.

## The principle

**A rating is a claim to someone else.** It means something only between
people, and only when somebody other than the rated player keeps it.

Two things follow, and the first is already done (Phase 3 §2, 20 September
2026):

- **A game against a bot is casual, wherever it is played.** The client keeps
  a win/loss *record* against the rungs of the ladder, because that is what
  names the rung to try next, and it keeps no rating. A move can always be
  taken back. The rating crate has no people-against-bots track and the daemon
  rates no game with a bot in it.
- **A rating between people is a server's to keep.** Not the client's, not a
  file either player can reach. The rest of this document is how.

## What exists to stand on

- **The daemon already rates human against human.** `Track::HumanVsHuman`,
  `Shared::rate` (`netrunner_server/src/serve.rs`), a Glicko-2 `RatingBook`
  written after every match with a temp file and a rename, tested across a
  restart (`human_matches_are_rated_on_their_own_track_and_the_book_survives_a_restart`).
- **It is keyed on an unverified string.** `ClientMessage::Connect.player_name`
  is whatever the client typed, and `PendingHuman::seated` makes it the rating
  id. Anyone can be anyone. That is the hole.
- **The match record exists, and the server throws it away.**
  `MatchRecordHeader { seed, corp_deck, runner_deck, rules }` plus a JSON-Lines
  `MatchHistory` (`netrunner_session/src/history.rs`) replays bit-identically —
  `ARCHITECTURE.md`'s determinism guarantee — and `netrunner_cli::replay`
  already reports the action at which a record stops replaying
  (`ReplayError::Diverged`). Only `--headless --record` writes one;
  `MatchSession::run_with_outcome` drops the history `into_parts()` hands it.
- **`session_token` is a seat, not a person**, and its doc says so: a v4 `Uuid`
  worth exactly what the WebSocket it crossed was worth. It stays that.
- **No crypto in the tree, and room for it.** `ed25519-dalek` is BSD-3-Clause,
  already on `deny.toml`'s allow list (for `subtle`); `rand_core` 0.6, `sha2`,
  `zeroize` are already in the lock file transitively. No `deny.toml` edit.

## 1. Identity is a key; a name is a label

The client generates an **Ed25519 keypair** the first time it needs one. The
public key — shown to a person as a short base32 fingerprint — is who they are
to a server: the rating participant id is `key:<base32>`, beside the `bot:`
namespace `bench` already uses. `player_name` becomes a display label the
server stores against the key. Two people may both be called "luke"; a server
may mark the second for display, and never confuses their ratings.

- **The secret key has its own file**, `<data dir>/netrunner/identity.key`,
  mode 0600. **Not `settings.json`**: that is the file a person pastes into a
  bug report. The "one struct, every client" rule (`netrunner_client::settings`)
  is about two clients erasing each other's fields, and it is kept the same
  way — one `Identity` type in `netrunner_client`, used by both.
- **Losing the file loses the identity; copying it is how a second machine
  works.** Both are said plainly in the client where the key is shown.
  Recorded for later, not now: a *successor statement* (the old key signs the
  new one) so a key can be rotated without starting the ladder again.
- **Rejected: accounts.** A password or an e-mail means a user table, a reset
  flow and a secret the *server* must protect, for a hobby server someone runs
  from a VPS. A keypair puts the only secret on the machine of the person it
  belongs to, and the server stores nothing worth stealing.
- **A new pure crate, `netrunner_identity`** (`ed25519-dalek`, `sha2`; no
  I/O): key types, the signed-statement envelope (§3), verification. Files are
  the client's and the server's to own, the same split `netrunner_rating`
  keeps. `netrunner_core` stays at its three dependencies.

## 2. Proving it: a challenge, bound to the server

The server has a keypair too (`identity.key` in its `--data-dir`, made on
first start; a daemon with no data directory makes one at bind). The
handshake gains a step before `Attach` or `Resume`:

```
client → ClientMessage::Identify  { key }
server → ServerMessage::Challenge { nonce, server_key, lasting }
client → ClientMessage::Prove     { signature }
           over  "netrunner-auth-v1" ‖ server_key ‖ nonce
server → ServerMessage::Identified { key }
      or ServerMessage::IdentifyRefused { reason }, and the socket closes
```

- **The server's key is inside the signed bytes.** Without it, a hostile
  server could open its own connection to an honest one, relay that server's
  nonce to a visiting client, and log in there as them. With it, the honest
  server sees a signature made for somebody else's key and refuses.
- **The domain tag** stops an auth signature ever being replayed as another
  kind of statement (§3 uses different tags).
- **The client pins a server's key on first contact**, known-hosts style, and
  says so loudly if it changes. **Only a `lasting` key** (built 26 September
  2026): a daemon with no data directory — every test, every game hosted
  from a client's menu — makes a new key each run, and pinning that would
  only ever cry wolf, so the challenge says which kind it is.
- **A refused proof closes the socket** rather than letting the connection
  on unidentified: a client that meant to be rated learns it at once, not
  after a game that counted for nothing.
- **An unidentified `Attach` is still welcome, and plays unrated.** A casual
  seat costs nothing to offer. Two identified seats on a rating daemon is the
  only rated game.
- **Transport.** The daemon speaks `ws://`. The handshake protects the *key*;
  it does not protect the *session* — a `session_token` crossing in the clear
  can be taken by anyone on the path. A public rating server therefore sits
  behind TLS (`wss://` at a reverse proxy). That is a deployment requirement,
  written here so nobody mistakes the challenge for transport security; it is
  not something to build into the daemon.

## 3. Signing games: seat commitments and a server receipt

The server is the rating authority. The question signatures answer is narrower:
*can a player deny a game, and can a server invent one?*

- **At `MatchJoined`, each identified player signs a seat commitment:**
  `{ match_id, server_key, side, own_key, opponent_key, deck_hash, started_at }`
  under the tag `netrunner-seat-v1`. A player cannot later deny having sat the
  game, and a server cannot attribute a game to a key that never sat it.
- **At the end, the server keeps the record it already has** — the header and
  the JSON-Lines history — and adds a footer:
  `{ match_id, corp_key, runner_key, both seat commitments, winner,
  GameEndReason, engine version, card-pool hash, ended_at }`.
  It signs `sha256(record bytes)` together with the result. That is the
  **receipt**, and both players get it in a new
  `ServerMessage::Rated { receipt, before, after }` — which also closes Phase 3
  §2's long-standing "not built: telling the remote client its new rating".
  A player who keeps their receipts can show their history even if the server
  loses its own.
- **The seed leaves the host only inside a finished record.** During play it
  stays where Phase 4 §3 put it.
- **Statements are signed as bytes, never as re-serialized JSON.** The envelope
  carries the exact payload string that was signed, and a verifier checks the
  signature over those bytes before it parses them. There is no canonical form
  to get wrong.
- **The engine version and card-pool hash are in the footer** because "this
  record replays" is only checkable against the rules it was played under;
  `ReplayError::Diverged` is already the detector for a record older than a
  rules change.

**Not now, and room is left: a per-action hash chain.** Each `SubmitAction`
signed over the hash of that player's previous ones would let someone who does
*not* trust the server verify a game — which is what federation, or a rating
that travels between servers, would need. It costs a signature per action and
a change to the hottest message in the protocol, and it buys nothing while a
rating is one operator's claim anyway (§5). The footer reserves
`action_chain: Option<…>`; nothing else in this design moves when it arrives.

## 4. Storage: the results log is the truth, the book is a cache

Under the daemon's `--data-dir`:

| Path | What |
|---|---|
| `identity.key` | The server's keypair. |
| `players.json` | One entry per key: label, first seen, last seen. A map rewritten whole, not the `players.jsonl` first sketched here: last seen changes on every visit, so a log would grow a line per connection to say one fact per key. |
| `matches/<yyyy-mm>/<match_id>.jsonl` | Full records: header, history, footer. |
| `results.jsonl` | Append-only receipts, one line per rated game, in order. |
| `ratings.json` | The `RatingBook`, exactly as today. |

- **`ratings.json` is derived.** Folding `results.jsonl` through
  `RatingBook::record` in order rebuilds it exactly — one game is one rating
  period, and both updates read the pre-game standings, so nothing else is
  needed. A corrupted book, a change to `Glicko2::tau`, or voiding a cheater's
  games is a *rebuild* (`serve --rebuild-ratings`), not surgery on a number
  nobody can re-derive.
- **Files, written with a temp file and a rename**, as `save_ratings` does
  today. **Rejected for now: SQLite.** Nothing yet asks a question a fold over
  a log cannot answer. It becomes the right move when a ladder page or a match
  search does, and the log is what it would be loaded from.
- **A record holds two whole decklists.** A player may fetch their own
  records; publishing anyone else's is an operator's policy, off by default.

## 5. What is rated, and what a rating is worth

- **Rated:** two distinct identified keys, on a daemon with a data directory,
  seating no bot. A forfeit — surrender, disconnect, clock — is a loss and a
  stall is nobody's, as today.
- **Never rated: a game hosted in-process from the menu** (Phase 6 §3). The
  host's process holds the seed and the unmasked state of a game its own
  player is in. That entry's "Open: hosted games are unrated" closes as *by
  design*.
- **Take-backs.** `netrunner_session`'s `Rewind::Free` — nothing taught,
  `rng_step` unmoved, the other seat silent — is fair between people and is
  the only kind a rated game offers: one `ClientMessage`, one retracted log
  entry for the other seat (Phase 7 §4af reserved exactly this).
  `Rewind::Undo` needs the opponent's consent, or an unrated room. This is why
  the classifiers behind that line were kept when local play stopped charging
  for it.
- **The trust boundary, stated plainly: a rating is a claim by one server's
  operator.** Whoever runs the daemon can see every seed. Keys are free, so a
  new key is provisional — Glicko-2's deviation already says so, and a ladder
  lists a key only after some number of games. Smurfing and win-trading are an
  operator's moderation problem; the rebuildable log is what makes moderation
  possible, and cryptography does not solve either.

## 6. Stages

Each its own branch, in this order; each is useful without the next.

1. **The server keeps match records.** `run_with_outcome` returns the history
   it now drops; `serve` writes header + JSONL under `--data-dir`. No protocol
   change. Also gives Phase 7 §8 item 15 (the bug-report bundle) a server half.
   *Built 26 September 2026* (`feat/server-keeps-match-records`), after
   stage 2: written when the match ends, for every match.
2. **`netrunner_identity`, the client's key file, the handshake.** Ratings
   keyed by `key:<b32>`; `players.json`; an unidentified seat plays unrated.
   *Server half built 26 September 2026* (`feat/key-identity-server`): the
   crate, the messages, `--data-dir` (which replaced `--ratings-file`) and
   one key in both chairs going unrated. The client half
   (`feat/key-identity-client`, the same day) added the key file, the
   machine's handshake and pinning by address.
3. **Seat commitments, receipts, `ServerMessage::Rated`, `results.jsonl`,
   `--rebuild-ratings`.** *Built 26 September 2026* (`feat/signed-receipts`).
   Three departures from §3 above: the receipt sits beside the record
   (`<id>.receipt.json`) rather than in a footer line, so the record stays
   what replay reads; the deck hash is salted with a salt only its seat
   is told; and a withheld seat signature leaves the game rated, its
   receipt without that commitment.
4. **Client surfaces.** The game-over panel shows `Rated`; Profile shows "your
   standing at `<server>`" — fetched, and never stored as truth.
   *Built 26 September 2026* (`feat/rated-client-surfaces`): the standing
   is shown at every server in `known_servers.json`, and the client keeps
   its receipts in `receipts.jsonl`.
5. **The free take-back online.**

**Settled (25 September 2026, Phase 4 §6 item 2):** `ClientMessage` and
`ServerMessage` live in `netrunner_protocol`, which the server re-exports and
`netrunner_client` depends on without the server. `netrunner_identity`'s
envelope types can live beside them.
