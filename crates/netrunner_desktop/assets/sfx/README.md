# Sound effects

Sounds are the middle tier of the client's three-tier assets (see
`AGENTS.md`, "Desktop client conventions"): every effect is synthesized
in-app so the client is never silent for want of a file, an `.ogg` in
this directory replaces the synthesized one, and a file of the same name
under `<data dir>/netrunner/assets/sfx/` replaces that.

The file names are the `Sfx` variants in `src/audio/mod.rs`, lowercase,
once that module lands (Phase 7 §4). Nothing is committed here yet.

A file committed here must be licensed for redistribution under terms
compatible with this repository's GPL-3.0-or-later — CC0 or an OFL-like
grant — and say so in a `LICENSE-<name>.txt` beside it.
