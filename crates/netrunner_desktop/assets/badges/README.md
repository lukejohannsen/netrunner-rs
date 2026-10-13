# Badges

A badge is a mark a server grants a player (an event's milestone, an
operator's grant; Phase 4 §10–§11), and the player wears one beside their
name on that server. The desktop draws it wherever it appears: the server
profile page's Badge line, the grid a badge is picked from (Phase 7 §14),
and an event's milestone line, where it is drawn before anyone has earned
it — the ring until a file of its stem exists.

| What | Path | Box (logical) | Draw at | Fit |
|---|---|---|---|---|
| A badge's glyph | `badges/<stem>.png` | 40 square on the profile page, 56 in the picker | **128 × 128**, with alpha | fit |

`<stem>` is the badge's name as the server grants it, lower case, every run
of anything but a letter or a digit one hyphen: "First Gauntlet!" is
`first-gauntlet.png` (`widgets::badge::stem`).

**The drawn tier** (`src/widgets/badge.rs`) is a ring in a colour the
name picks from the theme, with the name's initials inside, so a badge
nobody has drawn is still a distinct mark. Basic graphics draws the ring
alone.

A badge's name is the server's to choose, so a bundled glyph only fits
the badges a server this project knows grants. A player's own folder
(`<data dir>/netrunner/assets/badges/`) is where to give any badge a
picture.
