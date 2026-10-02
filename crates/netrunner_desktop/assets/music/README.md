# Music

Every `.ogg` (Ogg Vorbis) in this folder is a track, across the same two
file tiers as everything else: this directory, and
`<data dir>/netrunner/assets/music/`, whose files add to these and replace
one of the same name.

- **The menus loop one theme**: `glass-and-morning-sky.ogg`
  (`audio::MENU_THEME`, the person's choice, 1 October 2026). Without that
  file the menus are silent rather than playing another track in its
  place.
- **A game plays every track**, the theme included, one after another in
  a random order that never repeats the track just heard. The replay
  board counts as a game.
- **A change of place fades** the old track out (0.9 s) and the new one
  in (1.5 s).

The volume is Settings → **Music**, read every frame; at 0 nothing is
decoded at all.

**Music has no drawn tier.** Every other asset has a stand-in made in
code, and sound effects are synthesized when missing, but a tune made in
code would be worse than the silence a missing folder leaves.

| File | Title |
|---|---|
| `glass-and-morning-sky.ogg` | Glass and Morning Sky — the menus' theme |
| `city-life-rain-over-platform-seven.ogg` | City Life: Rain Over Platform Seven |
| `haas-bioroid-clinical-grace.ogg` | Haas-Bioroid: Clinical Grace |
| `jinteki-patented-life.ogg` | Jinteki: Patented Life |
| `nbn-prime-time-authority.ogg` | NBN: Prime Time Authority |
| `weyland-consortium-hostile-takeover.ogg` | Weyland Consortium: Hostile Takeover |

The four faction titles are kept in the file names, so a later change can
play a Corp's own track; today every game draws from all six.

## Licensing

These six were generated with AI assistance for the project, so they are
the netrunner-rs contributors', GPL-3.0-or-later, each with its row in
[`../CREDITS.md`](../CREDITS.md). Anything else committed here follows the
same rule as every asset: find out who made it, and give it a row.
