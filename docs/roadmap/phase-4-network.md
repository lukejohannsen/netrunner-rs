# Phase 4 — Network Resilience & Server Infrastructure

Area roadmap; the index is `ROADMAP.md`. Addresses: "Phase 4 §1"–"§6". The server sends **full masked `ClientView` snapshots** after every action, not deltas — the current design, not an oversight (§4).

**Where it stands (29 September 2026).** §1–§3 and §6 are built; §4 is deferred; §5's stages
(a)–(d) are built and (e) is open; §7's lobbies are at stage 4a with 4b–4d, 5 and 6 open. The closed
record is in [the archive](archive/phase-4-network.md).

## Open

- **§5 (e)**: the free take-back online (`Session::rewind` for an `External` seat, fair only while nothing was taught — AGENTS.md's Session Rule); key rotation is recorded and not designed.
- **§7 stages 4b, 4c, 4d**: a client that stays attached across games, and lobby browsing and lobby-making in the desktop and the terminal; **stage 5**, the rating inside lobbies; **stage 6**, tournaments (design below, none built).
- **§3 leftovers**: no graceful server shutdown; no spectator count cap or delayed omniscient stream; `netrunner_single_player/tests/common/mod.rs` still carries a filler fixture; the TUI header does not show the matchup.
- **§6 leftovers**: port mapping is untested against a real router (a manual check); the server's handshake is not sans-IO (item 2, "the server's side"); the terminal's settings form does not edit `relay`; a resume after a dropped QUIC connection is tested only over TCP (item 3).

## Closed — one line each

- **§1** — the event stream to clients: a per-viewer masked action log (`PublicHistoryEntry`) beside full masked snapshots.
- **§2** — reconnection and session recovery: a channel-backed seat is one connection's worth and can be replaced mid-match.
- **§3** — the multi-match daemon: a `Registry` under one mutex with lobby, rooms, cap and per-match seed; per-decision turn timers that forfeit; spectators masked as the intersection of both views; the daemon deals the published pool; a player brings their own deck (Phase 6 §3).
- **§5 (a)–(d)** — the server keeps match records; `netrunner_identity` and the Identify/Challenge/Prove handshake with `--data-dir`; seat commitments, signed receipts, `Rated` and `results.jsonl` as the truth; the clients' rated and standing lines and the player's own receipts (26 September 2026).
- **§6** — a host reachable from outside: IPv6 and one dual-stack socket with the router asked to open the port; a sans-IO client connection and its tokio driver; a peer-to-peer transport joined by ticket over iroh (25 September 2026). Rejected: STUN alone, raw WireGuard, Tailscale as a feature (it is the documented fallback, `docs/playing-online.md`).

## 4. Transport Efficiency — deferred
- [ ] State deltas instead of full snapshots — only once profiling shows full `ClientView` broadcasts are a bottleneck. Do not trade simple-and-correct away speculatively.

## 5. A rating is a server's to keep: identity is a key, a game leaves a signed receipt — stages (a)–(d) built, (e) open (26 September 2026)

`docs/server-tracked-rating`. **No code; the reasoning is `docs/identity-and-rating.md` and this entry is its index.** It follows `fix/local-play-is-casual` (Phase 3 §2, same day), which took the rating *out* of the client: a rating is a claim to someone else, so it means something only between people and only when somebody other than the rated player keeps it. What is left to decide is how a server keeps one, and the hole in what it does today.

**The daemon already rates human against human, under whatever name the client typed.** `Track::HumanVsHuman` and `Shared::rate` work and survive a restart (`tests/lobby.rs`), but the participant id is `Connect.player_name`, unverified (`PendingHuman::seated`). Anyone can be anyone. And the evidence is thrown away: `MatchRecordHeader` + the JSON-Lines `MatchHistory` replay bit-identically, and `MatchSession::run_with_outcome` drops the history it is handed.

**The decisions, each with what was rejected:**

- **Identity is an Ed25519 key the client holds; a name is a label.** Confirmed by the person on 26 September 2026: trust at login matters, and changing a name must change nothing, because the key is who someone is. The rating id is `key:<base32>`. The secret has its own 0600 file, never `settings.json` (the file people paste into bug reports). Rejected: accounts — a user table, a reset flow and a secret the *server* must protect, for a hobby server. A new pure crate `netrunner_identity` carries it; `netrunner_core` keeps its three dependencies, and `deny.toml` needs no edit (`ed25519-dalek` is BSD-3-Clause, already allowed).
- **Proved by a challenge that names the server.** `Identify` → `Challenge { nonce, server_key }` → `Prove`, signed over `"netrunner-auth-v1" ‖ server_key ‖ nonce`: the server's own key in the signed bytes is what stops a hostile server relaying an honest one's challenge to log in there as its visitor. An unidentified `Connect` still plays, unrated. The handshake protects the key, not the session: a public rating server sits behind TLS, as a deployment requirement and not daemon code.
- **Seat commitments and a server receipt, not a signature per action.** Each player signs once at `MatchJoined` (match, side, both keys, deck hash); the server signs the finished record's hash with the result and sends both players that receipt in a new `ServerMessage::Rated` — which is also Phase 3 §2's "not built: telling the remote client its new rating". Statements are signed as bytes, never as re-serialized JSON. Rejected *for now*: a per-action hash chain, which buys verification by someone who does not trust the server (federation, portable ratings) at a signature per `SubmitAction`; the record's footer reserves the field.
- **The results log is the truth and `ratings.json` is a cache.** Append-only receipts under `--data-dir`; folding them through `RatingBook::record` in order rebuilds the book exactly, so a corrupt book, a Glicko parameter change or voiding a cheater's games is a rebuild. Rejected for now: SQLite — nothing yet asks what a fold over a log cannot answer.
- **What is never rated:** any game with a bot in it (Phase 3 §2), and a game hosted in-process from the menu — the host's process holds the seed. Phase 6 §3's "Open: hosted games are unrated" closes as *by design*. Online, `Rewind::Free` is the only take-back a rated game offers; `Rewind::Undo` needs the other seat's consent — which is why the session's line and the engine's two classifiers were kept when local play stopped charging for it.
- **The trust boundary, stated:** a rating is a claim by one server's operator, who can see every seed. Keys are free, so a new key is provisional. Smurfing and win-trading are moderation, which the rebuildable log makes possible and cryptography does not solve.

**Stages, each its own branch and each useful without the next:** (a) the server keeps match records — no protocol change, and the server half of Phase 7 §8 item 15; (b) `netrunner_identity`, the client's key file, the handshake, ratings keyed by key; (c) seat commitments, receipts, `Rated`, `results.jsonl`, `--rebuild-ratings`; (d) client surfaces — the game-over panel and "your standing at `<server>`" on Profile, fetched and never stored as truth; (e) the free take-back online.

- **Left for later:** stage (e), the free take-back online. Rotating a key (the old one signing the new) is still recorded and not designed. Stage (d) will show the player their key, with "losing this file loses you; copying it plays as you elsewhere" beside it.

**Settled by §6 item 2 (25 September 2026):** the messages live in `netrunner_protocol`, which `netrunner_client` depends on without the server. **Open:** Key rotation (a successor statement signed by the old key) is recorded and not designed.

## 7. A public server: lobbies by format, decks nobody sees — OPEN (26 September 2026)

Asked for as: a server that simply exists on the internet, where people join a lobby for a format and are matched against a random opponent. Each player brings their own deck, and neither the opponent nor anyone browsing learns what it is or what it is called. Rated play is for this server; a game hosted peer to peer stays casual. Tournaments may come later.

**Decided with the person, 26 September 2026:**
- **A lobby per format, on one server.** It is not one daemon per format.
- **No picking an opponent.** A lobby pairs whoever is waiting. Listing the waiting players and challenging one was proposed and declined. A named room remains the way two people who know each other meet.
- **No dealt decks where the game is about the decks.** The player's surprise is the point, so the host dealing a deck to someone who brought none is not wanted there. Everyone has the built-in decks to bring.
- **Connecting asks for no deck; looking for a game does** (later the same day). A client attaches to a server with nothing but a name. It browses lobbies, which are open or closed by id and an optional password, and joins one — all without a deck. A deck is asked for only when the player looks for a game. Choosing a chair brings that side's deck; choosing a random chair brings one deck of each side. A player picks afresh for every game from all the decks their client holds, without reconnecting, and the same holds on a peer connection.
- **Lobbies are the server's and the players'.** The server has one open lobby per format it offers, and it never goes away. Any player may make a lobby, open or closed, in a format the server offers; a player's lobby goes when its last player leaves.

**Stages, each its own branch:**

1. **Lobbies by format** (`feat/format-lobbies`, built). `ServeOptions::formats` lists the lobbies, every format by default with Startup first; `netrunner_server --format startup,standard` narrows them. `Connect::format` names the lobby, and `None`, which is what an older client sends, is the first one. A format the daemon does not offer is refused, naming the ones it does. A player is paired only within their format and room, and a brought deck is checked against the lobby's format. `MatchList::lobbies` counts each lobby's public waiters, and `MatchSummary::format` names a match's lobby. A game hosted from a client's menu offers only the host's chosen format. The desktop's Host and Join pages choose the format as pills and offer the decks legal in it. The terminal joins its Settings format and names it in the form's title. Tests: two players in different formats wait apart, a Connect naming no format joins the first lobby, the list reports lobbies and a match's format, and an unoffered format is refused.
2. **A brought deck is private** (`feat/private-decks`, built). A saved deck's id is a slug of the name the player gave it, and it used to be sent to the opponent in `MatchJoined` and to anyone listing in `MatchSummary`. The server must see the whole list to validate it and run it; nobody else sees any of it but what the rules reveal. So:
   - Each seat is told only its own deck's id; the other side's is always empty. `SeatTicket::joined` is the one place the message is built, for the first seating and every resume.
   - `MatchSummary` names no decks at all, and gives the lobby instead. Hiding only brought decks while still naming dealt ones was rejected: a dealt list is still a list the opponent should not know, and a spectator is one message from telling them.
   - The identities stay public, because the rules show them.

   Tests: each seat, and a resume, is told its own deck and never the other's, and the list names neither. The rotation and pinning tests now read the identities off the views.
3. **A server deals nobody a deck** (`feat/no-dealt-decks`, built). `ServeOptions::deals` is off by default: a `Connect` with no deck is refused at the door, pointing to the built-in decks. `netrunner_server --deal` turns dealing back on, for a daemon that wants the old behaviour and for the tests of dealing itself. A bot seat is still dealt its own deck.
   - **Both clients no longer offer a deal.** `online::deck_choices` lists every deck legal in the format, built-in first then saved, Corp decks before Runner. `DeckChoice` and its three "let the host deal" entries are gone.
   - **The terminal's `--server` path always brings a deck:** `--deck`, or `--corp-deck`/`--runner-deck` when only `--side` is given. With neither it stops before connecting and says why.
   - A game hosted from a client's menu deals nothing either, so the host and the joiner each bring a deck, and two decks for the same side do not pair.

   Tests:
   - `a_server_deals_nobody_a_deck_by_default`: refused at the door, never queued, and a brought deck still queues.
   - The existing tests of dealing opt in with `deals: true`.
   - The desktop's and the terminal's host-and-join tests now bring a deck for each side.
4. **Attached connections and lobbies.** Four parts, each its own PR:
   - **(a) Server and protocol** (`feat/server-sessions-and-lobbies`, built).
     - `ClientMessage::Attach { player_name }` gives one task per socket (`serve::attached`), which carries the player between lobbies, into a game and back.
     - `ListLobbies`, `CreateLobby { name, format, closed, password }`, `JoinLobby { lobby, password }` and `LeaveLobby` need no deck.
     - `Seek { chair }` takes `Chair::Corp(deck)`, `Chair::Runner(deck)` or `Chair::Random { corp, runner }`. Each deck is checked against its side and the lobby's format. A random chair sits opposite a chosen one; two random chairs are seated by a coin off the match seed, so a `--seed` run seats the same way every time.
     - `CancelSeek` withdraws a seek, and so does a dropped socket.
     - **A match never holds an attached socket.** The seek's slot is the task's own channel pair, so when the match lets go of the seat the task sends `BackInLobby` and the player seeks again on the same socket.
     - A seat taken back by `Resume` is attached again: the ticket remembers its lobby.
     - The server's lobbies have their format's name as id; a player's has a six-character code with no look-alike characters. A closed lobby is never listed.
     - `Connect` was kept at first as a one-message join-and-seek.
     - Tests (`tests/attached.rs`): attach and join with no deck; a closed lobby's id and password, and the lobby disappearing when it empties; a chosen chair against a random one, then a second game with other decks on the same sockets; seek checks, cancel and withdrawal with the socket; a resumed seat returning to its lobby.
   - **`Connect` removed** (`feat/attach-only`, built, at the person's request: nothing has been released, so there is no older client to keep working). This supersedes parts of stages 1 and 3:
     - **The protocol:** `Connect` is gone, with its rooms, its preferred side and dealing to a player who brings no deck (`ServeOptions::deals`, `--deal`). The pinned and rotating decks are now only what a seated bot plays. Resuming a queue place is gone too, because a seek is withdrawn with its socket. Also gone are the compatibility shims kept for older clients: the `serde(default)` fields, and stage 1's `MatchList::lobbies`, which `ListLobbies` replaces.
     - **The server:** `Resume` always reattaches through an attached task. A lobby id is read as typed: a format's in any case, a player's code in upper case.
     - **The client core:** `connection::Goal::Play` takes a `Seat` — name, lobby, optional password and `Chair`. The machine attaches, joins the lobby, then seeks, each message sent when the one before it is answered. A refused lobby or seek ends the first connection with the reason. A drop while waiting starts again from attach; a drop while seated resumes. `remote::seat` and `seat_in_format` build a one-deck `Seat`, replacing `connect_message`.
     - **The clients:** the desktop Join page's Room becomes Lobby (an id, blank for the format's own) plus an optional Password. The terminal's form names a lobby, and the flag path takes `--lobby` and `--password` in place of `--room`.
     - Tests: `tests/lobby.rs` and `tests/reconnect.rs` rewritten on attach, join and seek, keeping every property they held that still means something. A bot's deck rotating or pinned is now read off the view's identities.
   - **(b) Client core, the rest:** a connection that stays attached across games — lobby browsing and a second game without reconnecting — rather than one game per connection as `Goal::Play` still is.
   - **(c) Desktop:** a lobby browser, making a lobby, and a find-game panel with the chair and its deck or decks.
   - **(d) Terminal:** the same.
5. **A rating the server keeps**: §5's stages (a)–(c), then (d)'s client surfaces.
6. **Tournaments, run as Null Signal Games runs them in person.** The design below is grounded in their Organized Play Policies (v1.6.2, October 2023; read 26 September 2026, section numbers theirs). None of it is built.

   **What their events do:**
   - **Structure.** Swiss rounds, then a cut (1.1.1–1.1.2). *Double-sided* Swiss plays both sides against the same opponent in a 65–70 minute round (1.1.5.1). *Single-sided* Swiss plays one game in 40 minutes, with the software choosing and balancing the sides (1.1.5.2); the World Championship runs eleven rounds of it. The cut takes the top 3–16 players (1.1.6), always single-sided (1.1.7), and double elimination except at casual events (1.1.9–1.1.10). The first cut round's higher seed picks sides; after that each player plays the side they have played less (1.1.11). Appendix II gives rounds and cut size by attendance.
   - **Scoring.** Per game: a win 3, a tie 1, a loss 0 (1.1.4). A bye scores a full round's win, goes to a random player in round 1 and afterwards to the lowest-scoring player who has not had one (1.1.3). The tiebreakers are Strength of Schedule, then Extended SoS, then random (1.1.6.2–1.1.6.4). A tied elimination game goes to the higher seed (1.1.12).
   - **Time.** When time is called, the player whose turn it is finishes it and the other takes one more turn. Then more agenda points wins, otherwise it is a tie (1.1.5.3). If time is called during the first game of a double-sided round, the second game is a tie (1.1.5.4).
   - **Decks.** At a competitive event decklists are required (1.3.2) and submitted before round 1 (2.1.4). The list is the truth over the physical deck (2.1.6), and a mismatch or an illegal list costs a game (4.4). Lists are private during Swiss. For the cut they are handed to the opponent to read for 3–5 minutes and taken back (1.1.8, 2.4.3). The World Championship takes them from NetrunnerDB private lists.
   - **Online.** A disconnected player has 5 minutes to reconnect, or loses the game (1.5.6). An asynchronous round is scheduled within 72 hours (1.5.5). A *sanctioned* online event must be played on jinteki.net (1.5.2), so events on this server would be unsanctioned community events.
   - **Conduct.** An intentional draw scores a tie, and in double-sided Swiss a "241" lets the first game decide the round. Both must be agreed with a judge within 5 minutes of the round starting (2.5.8–2.5.9). Outside assistance and collusion lead to disqualification (4.3).

   **What an engine removes, and what it does not.** Most of their penalty chapter is about human error that the engine makes impossible: missed triggers, illegal actions, overdraws, revealed cards, illegal board states, marked cards and deck-versus-list mismatches (4.4–4.8, 5). The server also plays from the registered list itself, so there is no physical deck to disagree with it. What remains is a question of trust — in the players, and in the server — and that is what the design answers.

   **The design:**
   - **Registration locks a pair of decks, one Corp and one Runner, for the whole event.** Both are submitted before round 1, validated against the event's format and held by the server, which deals every game from the registered list. Nothing is submitted per round, so there is nothing to swap mid-event. A commitment is published beside each registration: `SHA-256(salt ‖ canonical deck bytes)`, signed with the player's key (§5).
     - *Canonical* means sorted card ids and counts, never re-serialized JSON — §5's rule for signed statements.
     - The salt is random and kept by the player and the server. Without it, a hash of a well-known decklist can be confirmed by anyone who guesses the list and hashes it, and the commitment would leak exactly what the privacy stage protects.
     - The commitment is the player's receipt that the server ran their deck unaltered. At the end of the event, or at the cut, the list and salt are revealed and anyone can check them against it.
   - **Deck lists private through Swiss.** Opponents see only what the rules reveal (stage 2). At the cut each player is shown their opponent's list for a few minutes before the game, as at a table (1.1.8). The server records that it did.
   - **The shuffle can be checked.** The seed is a commit-reveal between the server and both players. The server commits to `H(server_secret)` when the pairing is posted; each player's seat commitment carries a nonce of their own; the seed is `H(server_secret ‖ nonce_corp ‖ nonce_runner)`, and the secret is revealed with the result. So the server cannot pick a favourable shuffle, and a player can replay their game from the record — which replays bit-identically today — to check every draw.
   - **Results are receipts, standings a fold.** Each game ends in a server-signed receipt (§5 stage c) naming the event, round, both keys, both deck commitments, the seed commitment and the result. Standings, pairings and the SoS and ESoS tiebreakers are a deterministic function of the receipts log and the published seeding. Anyone holding the log can recompute them, and a disputed standing is a recomputation.
   - **Identity is the key** (§5): a registration names a key, not a name, and a player may rename themselves mid-event.
   - **What still needs a person:** outside assistance, collusion and intentional draws. The protocol gives the organizer tools — an intentional draw or 241 offered by both players inside the first 5 minutes, and a drop — and cannot detect coaching. A tournament organizer role, signed by its own key, has the final say, as the policies give the organizer (3.2.1).
   - **Time is the clock the server already has**, set to the round length. The end-of-round rule (1.1.5.3) is a server rule: the current turn and one more, then agenda points. The reconnect grace is 5 minutes (1.5.6).

   Staging when it is built: registration and deck commitments; single-sided Swiss with the side balancing, byes and tiebreakers; the seed commit-reveal; the cut with its side-selection rules and decklist viewing; then double-sided Swiss and asynchronous rounds.
