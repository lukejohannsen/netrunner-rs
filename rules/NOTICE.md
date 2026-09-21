# The Comprehensive Rules are Null Signal Games'

`comprehensive-rules.md` and `manifest.json` in this directory are derived
from the **Netrunner Comprehensive Rules**, © Null Signal Games, published at
<https://rules.nullsignal.games/>. The version and effective date are at the
top of `comprehensive-rules.md`.

- **They are not covered by this repository's licence.** The GPL-3.0-or-later
  that covers the code here grants nothing over this text; it remains Null
  Signal Games' own. No licence for it was found — the page states none, and
  Null Signal Games' public repositories do not include it — so it is
  reproduced as a reference for implementing the rules, and nothing more.
- **The text is unmodified** apart from its markup: the page's HTML is
  flattened to one line per rule, card links become the card's name, the
  printed symbols become bracketed words (`[click]`, `[credit]`), and the
  appendix's timing-structure steps are numbered as the printed appendix
  numbers them. `manifest.json` holds the numbers, anchors and hashes, no
  text.
- **It will be removed at Null Signal Games' request.** Nothing in the
  workspace needs it to build or run; the one test that reads it
  (`crates/netrunner_core/tests/rules_citations.rs`) checks this project's
  citations against the rule numbers.

Netrunner is a trademark of R. Talsorian Games, Inc. Android is a trademark
and © of Fantasy Flight Games. The rules state that they are not associated
with or endorsed by Fantasy Flight Games, R. Talsorian Games, or Wizards of
the Coast, and neither is this project associated with or endorsed by Null
Signal Games.

`scripts/rules_sync.py` regenerates both files; never edit them by hand.
