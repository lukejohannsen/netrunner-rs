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

## 2. A deck builder in the TUI — OPEN

Today deck building is `deck new/add/remove/show/validate` on the command
line. The menu gains **Decks**: list every deck (built-in and saved),
open one to read it, **create** (side → identity → name) or **copy** an
existing deck as a starting point, then edit in a card browser filtered by
type, faction and a search, with copies added and removed, card text in
the existing inspector (`app::card_modal`), and legality re-checked as the
list changes against the format in Settings. Save, rename, delete. Built-in
decks stay read-only — `deck_store` already refuses to overwrite one,
because they are published lists.

## 3. Network play from the menu — OPEN

Two decisions taken with the user (13 September 2026):

- **A player brings their own deck.** `ClientMessage::Connect` has no deck
  today — the daemon deals both, pinned by its flags or rotating the
  sample pool — so a deck built in §2 could not be played online.
  `Connect` gains an optional deck (`serde(default)`, so an older client
  still connects and is dealt one as now), validated at the server against
  its `--format` and the embedded card pool and refused with
  `ConnectRejected` if illegal. Rejected: server-dealt decks only, which
  needs no protocol change but makes the deck builder local-only.
- **Host and Join, both from the menu.** *Host* runs a human-vs-human
  `netrunner_server::Server` inside the TUI process on a chosen port and
  shows the address to share; the host then joins its own server over
  loopback like any client, so it gets a masked `ClientView` and nothing
  more. *Join* takes an address, an optional room, a preferred side and a
  deck. *Browse* lists the daemon's matches (`ListMatches`) and spectates
  one. Rejected: join-only, which still needs someone to start a daemon
  with flags.
