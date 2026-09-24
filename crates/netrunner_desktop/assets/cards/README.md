# Card backs

Both printings' backs ship here, and Settings → **Card backs** picks
one (Null Signal Games' by default, Fantasy Flight Games' the other),
as jinteki.net offers them:

    backs/nsg/back-corp.png     backs/nsg/back-runner.png
    backs/ffg/back-corp.png     backs/ffg/back-runner.png

They are the copies jinteki.net ships in its own repository, unchanged,
and are all rights reserved: shipped with credit to their owners and
removed if an owner asks (`../CREDITS.md`, "Art under no licence").
Changing the setting mid-game swaps every back on screen at once, under
the same image handles (`src/card_images.rs`).

A player's own back beats either: `back-corp.png` / `back-runner.png`
under `<data dir>/netrunner/assets/cards/`. With no file at all, the
back is drawn in code (`src/card_back.rs`: the side's colour, an inner
rim, a circuit-trace pattern), so the client never lacks one.

Any size works — the face is drawn at 5:7 and the image is stretched to
it, so a 5:7 source keeps its shape; a sixteen-bit PNG (NSG's are) is
narrowed to eight-bit sRGB on load so it is not drawn washed out.

Card *fronts* have no file tier: they are downloaded from NetrunnerDB
into the cache directory on request and never committed. NetrunnerDB's
750 × 1050 scan is asked for first, which every System Gateway and
Elevation card has, and its 300 × 420 one where there is no larger;
`netrunner_cli cards images` lists the cards still at the smaller size.
Each face is drawn from a copy resampled to the width it covers, so a
scan is never shrunk more than 1.25× by the renderer.
