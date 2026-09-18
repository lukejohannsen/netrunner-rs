# Sound effects

Sounds are the middle tier of the client's three-tier assets (see
`AGENTS.md`, "Desktop client conventions"): every effect is synthesized
in-app so the client is never silent for want of a file, an `.ogg` in
this directory replaces the synthesized one, and a file of the same name
under `<data dir>/netrunner/assets/sfx/` replaces that.

The file names are the `Sfx` variants in `src/audio/mod.rs`, lowercase,
once that module lands (Phase 7 §4). Nothing is committed here yet.

A file committed here is a separately licensed work and needs a row in
[`../CREDITS.md`](../CREDITS.md): its owner, their website, its licence
and anything changed. If the project made it, including with AI
assistance, it is GPL-3.0-or-later. If someone else did, it keeps their
licence, with the licence text beside it as `LICENSE-<name>.txt`, and
its maker is credited by name and website. Find out who made it before
committing it.
