# Sound effects

Sounds are the middle tier of the client's three-tier assets (see
`AGENTS.md`, "Desktop client conventions"): every sound has a stand-in
synthesized in code (`src/audio.rs`, `synth`), an `.ogg` in this directory
replaces it, and a file of the same name under
`<data dir>/netrunner/assets/sfx/` replaces that.

## Which sound is which

Each sound is a **set** of recordings, and one is drawn at random every
time it plays, so five cards drawn are not one sample five times. A file
belongs to a set when its name is the set's stem, alone or followed by
`-<n>` or `_<n>`: `data-in-3.ogg` is a Take. So a variant is added by
dropping in `lock-in-4.ogg`, and a recording replaced by dropping in a
file with its name.

The theme is the setting's rather than the table's (the person's choice,
4 October 2026): no paper and no chips.

| Sound | Stem | When | Shipped |
|---|---|---|---|
| Opening | `boot` | a game starts: the connection comes up | 1 |
| Place | `lock-in` | a card played to the table (a server, the rig) or to a discard pile | 3 |
| Take | `data-in` | a card taken into a hand, from anywhere | 3 |
| Chips | `credit` | credits taken, by either side | 3 |
| Click | `click` | a button pressed that has no sound of its own | 1 |
| Toggle | `toggle` | a toggle button, a pill of a choice row | 1 |
| Switch | `switch` | a selection that changes the screen | 1 |
| Back | `back` | a Back button, Escape, or a screen left for the one it was opened from | 1 |
| Deck add | `deck-add` | a card the deck builder added (a refused add is silent) | 1 |
| Open | `panel-open` | a sheet, a card's menu, the options, the keys, the timing chart, a drop-down's list, the card reader or the phase panel opens | 1 |
| Close | `panel-close` | the same, closing | 1 |
| Jack in | `jack-in` | a run begins | 1 |
| Jack out | `jack-out` | the Runner jacks out | 1 |
| Approach | `ice-approach` | the run approaches a piece of ice | 1 |
| Encounter | `ice-encounter` | the run encounters it | 1 |
| Run success | `run-success` | the run is declared successful | 1 |
| Run ended | `run-ended` | the run is ended by a card | 1 |
| Rez | `rez` | a card is turned faceup | 1 |
| Damage | `damage` | the Runner suffers damage, of any kind: net, meat or core | 1 |
| Tag | `tag` | the Runner takes a tag | 1 |
| Advance | `advance` | a card is advanced, or has advancement counters placed on it | 1 |
| Score | `score` | the Corp scores an agenda | 1 |
| Steal | `steal` | the Runner steals one | 1 |
| Turn start | `turn-start` | a turn begins | 1 |
| Win | `win` | the match ends and the person won | 1 |
| Loss | `loss` | the match ends and they did not | 1 |

The board's sounds are read off the record (`src/models/sound.rs`): where
a card went and what a number became off the same `Transition`s the
highlights are drawn from, and what happened — a run's steps, a rez, an
agenda — off the action's own events, a run's with the beat that shows
them. Both sides' moves are heard, an action makes at most three of one
kind, and they play 90 ms apart. Credits spent, cards removed from the
game or shuffled into a deck, and an ice passed make none.

The volume is Settings → **Sound effects**, read every frame.

## Where the files come from

They are made in this project, in code: `examples/render_sfx.rs` holds
one recipe per file, and

    scripts/render_sfx.sh          # render, encode, install here
    scripts/render_sfx.sh --play   # the same, then play each by name

renders them to `target/sfx/wav/` and writes the `.ogg` files in this
directory with whichever of ffmpeg, oggenc and VLC is installed. To
change a sound, change its recipe and run the script; do not edit a file
here by hand. A test (`audio::tests`) fails when a sound has no file or
no row above, or a file here is no sound's.

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

Everything here today is the project's own, synthesized by
`examples/render_sfx.rs`, and so GPL-3.0-or-later. (The first set, of
1 October 2026, was Kenney's CC0 recordings of cards and chips; it was
replaced whole on 4 October 2026.)
