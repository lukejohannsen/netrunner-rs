# Card backs

The two card backs are the client's three-tier assets in full (see
`AGENTS.md`, "Desktop client conventions"): both are drawn in-app
(`src/card_back.rs` — the side's colour, an inner rim, a circuit-trace
pattern) so the client never lacks one; the official Null Signal Games
backs are fetched into the cache directory (`images/back-corp.png`,
`images/back-runner.png`, beside the scans) on the same opt-in as the
scans and the icon font (Settings → card images) and put in place of the
drawn ones, mid-game if that is when they land; a PNG here replaces
both; and a file of the same name under `<data dir>/netrunner/assets/cards/`
replaces that.

    back-corp.png
    back-runner.png

Null Signal Games does not publish its backs, and they are not this
project's to redistribute, so nothing is committed here: the fetch reads
the copies jinteki.net serves for its own table
(`netrunner_card_sync::CARD_BACK_CORP_URL`, `CARD_BACK_RUNNER_URL`), for
the player's own screen, exactly as the scans are read from NetrunnerDB.
A drop-in is still the way to use a different back. Any size works — the
face is drawn at 5:7 and the image is stretched to it, so a 5:7 source
keeps its shape; a sixteen-bit PNG (the official ones are) is narrowed
to eight-bit sRGB on load so it is not drawn washed out.

Card *fronts* have no file tier: they are downloaded from NetrunnerDB
into the cache directory on request and never committed. NetrunnerDB's
750 × 1050 scan is asked for first, which every System Gateway and
Elevation card has, and its 300 × 420 one where there is no larger;
`netrunner_cli cards images` lists the cards still at the smaller size.
Each face is drawn from a copy resampled to the width it covers, so a
scan is never shrunk more than 1.25× by the renderer.
