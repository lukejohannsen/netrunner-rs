# Phase 6 — A menu-driven client

**The client was driven by its flags.** Every way of playing existed —
a rated game against a named rung, the lesson tracks, deck building,
network play — but each was reached by a subcommand or a switch, and the
one screen a player could reach without typing any (Phase 5's start
screen) played one game and exited. Tested by hand on 13 September 2026:
"it is okay but it is driven too much by switches to launch."

**The goal is that `netrunner_cli` with no arguments is the whole game.**
A menu drives the configuration — who you play (computer or a person
over the network), which side, which decks, the computer's strength and
style — and a finished game comes back to it.

**The flags stay.** `--headless`, `bench`, `diag` and every before/after
measurement in this roadmap are command lines, and a menu that replaced
them would make every recorded number unreproducible. The rule is the one
Phase 5's start screen set: **the menu is the flag path, not a second
one.** A choice folds into the `Config` the flags would have produced and
runs the same function they run, so there is one seating rule, one rating
rule and one deck resolver.

**Out of scope, deliberately:** two people on one screen. Hot-seat play
shows each player the other's hidden cards, which is the thing the
masking layer exists to prevent (AGENTS.md §2); human against human is
network play.

Three PRs, in order: the menu and its shell (§1), a deck builder in the
TUI (§2), and network play from the menu (§3).

---

## 1. The main menu, and a game that returns to it — DONE (13 September 2026)

`feat/main-menu`. With no side flag, the TUI opens a main menu — Play vs
Computer, Learn to Play, Ratings, Settings, Quit — instead of the
one-shot start screen, and **every game it launches returns to it**: the
new-game form reopens on the game just played (chair, decks, style,
rated) with the rung moved to the suggestion re-read after that game, so
Enter is "play again, at the rung the game-over modal just named".

- **One terminal for everything.** `play_local`, `play_lessons` and
  `play_starter_game` draw on the caller's terminal; the old `run_*`
  entry points are thin `init`/`restore` wrappers the flag path still
  calls. A game that fails to start — an illegal saved deck, a ratings
  file that will not open — is a notice under the menu rather than a
  drop to the shell.
- **Each launch clones the base config.** Mutating one shared `Config`
  would let the style chosen for the computer's Corp in one game follow
  into the starter game after it; pinned by
  `a_style_chosen_for_one_game_does_not_follow_into_the_next`.
- **Learn to Play is every `learn` subcommand as a list** — both tracks,
  each lesson, the four starter games — launched through one
  `learn::play(terminal, LearnPick, config)` the subcommand also uses.
- **Ratings** draws `ratings::standing_lines`, the same lines
  `netrunner_cli ratings` prints.
- **Settings** — the player name games are rated under and the format
  decks are checked against — persist to
  `<data dir>/netrunner/settings.json` (`NETRUNNER_SETTINGS_FILE`).
  **A setting fills a flag the command line left unset and never beats a
  typed one** (`settings::was_flagged` reads clap's `ValueSource`, at the
  top level and after a subcommand, since both flags are global). An
  edit made in the menu does apply to the session over a launch flag: it
  is the newer request. **Measurements never read the file** —
  `settings::applies_to` excludes `bench`, `diag` and `--headless`, whose
  numbers must be reproducible from their command line.
- **A rung flag now seats a bot** (`Config::seats_bot`). The human's
  chair was read off `--corp`/`--runner` alone, so `netrunner_cli
  --runner-level 3` — the invocation `ratings` tells a new player to type
  — had two human chairs and went to the start screen, dropping the rung,
  since #15 (and was an error before it). A rung overrides the kind
  everywhere else; it now decides the chair too.
- The new-game form gained `r` to toggle rated (`StartChoice::rated`
  folds into `--unrated`), and Esc goes back rather than quitting.

Tested as state machines without a terminal (eleven menu tests, two form
tests, four settings tests), and **driven by hand in tmux against a
scratch data directory**: menu → form → game → quit → form (unrated
kept); Learn → lesson → quit → Learn; Settings name and format saved,
surviving a relaunch and reaching `netrunner_cli ratings`; `--runner
heuristic` still skips the menu and `--runner-level 1` alone seats the
player as Corp; an empty saved deck reports "deck has
0 card(s) but its identity requires at least 40" under the form. No
rules change; nothing outside `netrunner_cli`.

## 2. A deck builder in the TUI — DONE (13 September 2026)

`feat/tui-deck-builder`, stacked on §1. The main menu gains **Decks**:
every deck, the player's saved ones first, each marked legal or not in
the Settings format. A built-in deck opens **read-only** (list, notes,
how to play) with `c` to copy it; a saved one opens in the **editor**.
`n` starts a deck from side → identity (the format's, with each
identity's minimum size and influence) → name; `c` copies any deck,
keeping its list, notes and style as `Custom`; `d` deletes a saved deck
after a `y`. The id is a slug of the name made unique (`brick_stack_copy`),
because it is the filename and what `--corp-deck` takes.

The editor is the deck on the left and the card pool on the right:
Enter/`+` adds, `-` removes, Tab switches lists, `/` searches titles and
type lines, `t` and `f` cycle a type and a faction filter, `i` opens the
card's text in the game's inspector modal (`app::card_modal`, "Engine
reads it as" included), `r` renames, `y` cycles the bot style
(`DeckFile::style`, so a saved deck plays in the style its builder chose).
**Every edit is saved as it is made**, as `deck add` does — a deck under
construction is legitimately illegal, so there is no "valid enough to
save" moment to wait for.

- **Legality is the validators', and the running totals are the core's.**
  The verdict line is `DeckFile::validate`, both validators, exactly what
  starting a game runs. The totals — cards against the identity's
  minimum, influence against its budget, agenda points against the range
  for the current size — are a new `netrunner_core::deck::tally_deck` /
  `DeckFile::tally`, which never fails on a rule. It shares one
  `influence_per_copy` with the validator, so the builder's influence
  number is the gate's; `a_tally_agrees_with_the_validator_and_survives_an_illegal_deck`
  pins that over all 28 embedded decks. Copying the arithmetic into the
  client was rejected: it is the kind of rule re-derivation the client
  contract forbids, and two copies drift.
- **The one rule the builder applies itself is the copy limit on `+`**,
  read off the card (`deck_limit`, else the validator's
  `MAX_COPIES_PER_CARD`) — letting a fourth copy in only to flag it would
  be a worse builder, and the validator still decides.
- **The pool is the format's**: only cards whose pack the format allows
  and that are not banned, so a Startup player never sees a Core Set card
  the validator would refuse (`the_pool_is_the_decks_side_in_the_format_and_filters_narrow_it`).
- `deck_store::delete` refuses a built-in id and a missing file.

**Found while building it, fixed separately:** the deckbuilding validator
has **no out-of-faction agenda rule**. Agendas print no influence, so a
Weyland *Above the Law* in a Haas-Bioroid deck costs 0 and the deck
validates — the builder showed it at 0 influence and "Legal" once the
deck was full. Netrunner forbids an agenda from another faction outright
(neutral agendas excepted). Every published sample deck is unaffected;
the fix is a validator rule in `netrunner_core`, landed on its own as
#19 (`DeckValidationError::OutOfFactionAgenda`; Rules Audit, last section).

Seven builder tests and one deck-store test, without a terminal; driven
by hand in tmux against a scratch data directory — new deck through side,
identity and name; search, add to the copy limit, card text; copy of
*Brick Stack* (44/40 cards, 15/15 influence, 18 of 18–20 points, legal in
startup); back to the list; delete.

## 3. Network play from the menu — DONE (13 September 2026)

`feat/online-from-the-menu`, stacked on §2. The main menu gains **Play
Online**, with the two decisions taken with the user carried out:

- **A player brings their own deck.** `ClientMessage::Connect` carries an
  optional deck (`serde(default)`, so an older client still connects and
  is dealt one), and **the deck is the seat**: its side overrides a
  preference and a contradicting one is refused. The server validates it
  against its own `--format` at the door, and `start_match` deals it in
  place of the pin or the rotation for that side. Two Corp decks never
  pair — each waits for a Runner. The protocol record is Phase 4 §3.
  Rejected: server-dealt decks only, which would have left §2's decks
  local.
- **Host and Join, both from the menu.** *Host* binds a human-vs-human
  `netrunner_server::Server` in this process — on the whole network by
  default, or this machine only — shows the address to hand over
  (`ws://<the routed LAN address>:port`, found by asking the routing
  table, nothing sent), and **joins its own server over loopback**, so
  the host plays through a masked `ClientView` like their opponent and
  the real `GameState` lives in the server task, not the screen. *Join*
  takes an address (`ws://` and `:8080` filled in when left off), an
  optional room and a deck — or "let the host deal", for either side or
  a preferred one. *Watch* lists a server's matches and spectates one.
  Rejected: join-only, which still needs someone to start a daemon with
  flags.

**Nothing waits on the network with the keyboard dead.** A connection is
a task (`remote::spawn_connect`) the menu polls every frame (`Menu::tick`),
so the lobby wait draws — the host's line keeps the address to share —
and Esc abandons it. **Abandoning had to close the socket**: dropping the
client's writer half left the reader waiting for a frame that, in the
lobby, only comes when an opponent arrives, so the daemon would have
paired that opponent with someone who had gone. The writer now closes
with a `Close` frame when its sender goes
(`abandoning_the_wait_leaves_the_lobby`). Binding the port and one
`ListMatches` are the only blocking calls, bounded at 3 s under
`block_in_place`, the pattern `Reconnector` set. A hosted server stops
with the game, or when the host stops waiting.

The flag path gained the same: `--deck` brings a deck in `--mode remote`,
the player's own name goes over the wire rather than `"CLI Player"`, and
both paths play through one `tui::play_remote`, which warns on the header
when an older daemon ignored the brought deck. Hosted games are unrated —
the local book is the human-vs-bot ladder.

**Verified over real sockets** — `a_host_and_a_joiner_are_seated_with_the_decks_they_brought`
drives two `OnlineScreen`s through the keys a person presses: one hosts
with *Stolen Goods*, one joins by address with *Brick Stack*, both are
seated on their decks' sides with their decks, a third lists the match
(under the players' own names) and spectates it. Four lobby tests on the
server (a brought deck decides the seat; illegal or contradictory is
refused at the door; same-side decks never pair; a bot daemon plays it).
**Driven by hand** with two TUIs in tmux: host on a loopback port with
*Stolen Goods*, join from the second with *Brick Stack* — R&D 39 (44 − 5),
both keep, the log agrees on both screens — quit on each lands on Play
Online, and the port is closed afterwards; Esc while hosting stops the
server too.

**Open:** hosted games are unrated (**closed as by design, 20 September 2026**, Phase 4 §5: the host's process holds the seed, so a rating is only ever a separate server's); a lobby place still cannot be resumed
from the client (Phase 4 §3); the last-used server address is not
remembered between sessions.
