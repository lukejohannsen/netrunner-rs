# Sound effects

Sounds are the middle tier of the client's three-tier assets (see
`AGENTS.md`, "Desktop client conventions"): every sound has a stand-in
synthesized in code (`src/audio.rs`, `synth`), an `.ogg` in this directory
replaces it, and a file of the same name under
`<data dir>/netrunner/assets/sfx/` replaces that.

## Which sound is which

Each sound is a **set** of recordings, and one is drawn at random every
time it plays, so five cards drawn are not one sample five times. A file
belongs to a set when its name is one of the set's stems, alone or
followed by `-<n>` or `_<n>`: `card-slide-3.ogg` is a Take. So a variant
is added by dropping in `card-place-5.ogg`, and a recording replaced by
dropping in a file with its name.

| Sound | Stems | When (the person's choice, 1 October 2026) | Shipped |
|---|---|---|---|
| Opening | `card-shuffle`, then `card-fan` | a game starts: the shuffle, then a fan once the shuffle is done | 1 shuffle, 2 fans |
| Place | `card-place` | a card played to the table (a server, the rig, a score area) or to a discard pile | 4 |
| Take | `card-shove`, `card-slide` | a card taken into a hand, from anywhere | 4 + 8 |
| Chips | `chips-stack` | credits taken, by either side | 3 |
| Toggle | `toggle` | a toggle button, a pill of a choice row | 1 |
| Switch | `switch` | a selection that changes the screen | 1 |
| Back | `back` | a Back button, Escape, or a screen left for the one it was opened from | 1 |
| Deck add | `deck-add` | a card the deck builder added (a refused add is silent) | 1 |

The board's sounds are read off the same record its highlights are
(`src/models/sound.rs`): both sides' moves are heard, an action makes at
most three of one kind, and they play 90 ms apart. Credits spent, cards
removed from the game or shuffled into a deck make none.

The volume is Settings → **Sound effects**, read every frame.

## Nothing heard on Linux

Bevy plays through cpal, which on Linux speaks **ALSA** (or JACK) and
nothing else. On a PipeWire desktop, ALSA reaches PipeWire only through
the bridge package — `pipewire-alsa` on Debian and Ubuntu. Without it,
ALSA's `default` cannot open (`ALSA lib pcm_dmix.c: unable to open
slave`, the one line the log shows), and cpal **falls back without a
word to the first device it can open** — on the machine this was found
on (1 October 2026), the discrete GPU's HDMI port with nothing plugged
into it, so music and sounds played to no one. `aplay -D default
<file.wav>` failing the same way is the check; installing the bridge is
the fix, and takes effect for the next program started.

## Licensing

A file committed here is a separately licensed work and needs a row in
[`../CREDITS.md`](../CREDITS.md): its owner, their website, its licence
and anything changed. If the project made it, including with AI
assistance, it is GPL-3.0-or-later. If someone else did, it keeps their
licence, with the licence text beside it as `LICENSE-<name>.txt`, and
its maker is credited by name and website. Find out who made it before
committing it.

Everything here today is Kenney's (https://www.kenney.nl), from three
packs — *Casino Audio*, *Interface Sounds* and *UI SFX Set* — all CC0,
with the three licence texts in [`LICENSE-Kenney.txt`](LICENSE-Kenney.txt).
Only what is played is kept: of the pack's nineteen chip sounds, three.
