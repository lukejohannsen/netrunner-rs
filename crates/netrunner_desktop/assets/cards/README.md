# Card backs

The two card backs are the middle tier of the client's three-tier assets
(see `AGENTS.md`, "Desktop client conventions"): both are drawn in-app
(`src/card_back.rs` — the side's colour, an inner rim, a circuit-trace
pattern) so the client never lacks one, a PNG here replaces the drawn
one, and a file of the same name under `<data dir>/netrunner/assets/cards/`
replaces that.

    back-corp.png
    back-runner.png

The official Null Signal Games card backs are the intended drop-in. They
are not this project's to redistribute, so they are not committed here;
put them in your data directory. Any size works — the face is drawn at
5:7 and the image is stretched to it, so a 5:7 source keeps its shape.

Card *fronts* have no file tier: they are downloaded from NetrunnerDB
into the cache directory on request (Settings → card images) and never
committed.
