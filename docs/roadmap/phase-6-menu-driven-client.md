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

**Where it stands (29 September 2026).** All three sections are built; the record is in
[the archive](archive/phase-6-menu-driven-client.md).

## Open

- ~~The last-used server address is not remembered between sessions (§3).~~ Done (`feat/remember-the-server-address`, 9 October 2026): `Settings::server` is the last server connected to, written by either client as the connection is made — never as the address is typed — and read as `--server`'s default by the terminal client and as the Join form's start by the desktop, which had hard-coded the loopback address and so did not remember it either.
- Nobody has yet played the menu, the deck builder and online play end to end in a real game, or online across two machines (`ROADMAP.md`, next item 3).
- Closed as by design: hosted games are unrated (Phase 4 §5); a lobby place is no longer a thing to resume (Phase 4 §7 stage 4a, attach-only).

## Closed — one line each

- **§1** — The main menu, and a game that returns to it (`feat/main-menu`, 13 September 2026).
- **§2** — A deck builder in the TUI (`feat/tui-deck-builder`, 13 September 2026).
- **§3** — Network play from the menu (`feat/online-from-the-menu`, 13 September 2026).
