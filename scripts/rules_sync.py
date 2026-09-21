#!/usr/bin/env python3
"""Keep the committed copy of the Comprehensive Rules in step with Null Signal Games'.

The rules the engine implements are published as one HTML page at
https://rules.nullsignal.games/, revised a few times a year. Until this
script the engine cited them by number ("CR 1.16.1a") from a reading of the
live page on the day, and nothing noticed when the page moved on. So:

* **`rules/comprehensive-rules.md` is the copy an agent greps** -- one line
  per rule, `- **1.16.1a** (`rule_cost_no_interrupt`) <text>`, with the
  chapters and sections as headings. One line per rule is what lets a grep
  for a number land on exactly that rule, and what makes a new version a
  readable `git diff`. The text is Null Signal Games', reproduced unmodified
  but for the markup (see `rules/NOTICE.md`); card links become the card's
  name and the printed symbols become this repo's spellings (`[click]`,
  `[credit]`, `[trash]`, ...), the ones card text is quoted with elsewhere.
* **`rules/manifest.json` is the copy a program reads** -- the version, and
  for every chapter, section and rule its number and a hash of its text,
  keyed by the page's anchor. The anchors (`rule_cost`, `sec_checkpoints`)
  are words, and survive a renumbering that the numbers do not, so a rule
  that moved is told apart from a rule that changed. A section's hash covers
  its title and the prose that belongs to no rule (the snippets, the timing
  structure lists), so an edit to a timing structure is a change too.
* **`--check` is the watch.** It parses the live page, compares it with the
  committed manifest without writing anything, and names every rule added,
  removed, changed or renumbered, and every citation in the repo that points
  at one of those -- the to-do list for adopting the new version, the way the
  Linked Clause gate is the erratum list for card text. It exits 1 on any
  difference. `.github/workflows/rules-watch.yml` runs it weekly.

No fetch date is written anywhere: a sync of an unchanged page must be an
empty diff, or a version bump could not be told from a re-run.

Standard library only, so it runs on a bare CI image.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import urllib.request
from dataclasses import dataclass, field
from html.parser import HTMLParser
from pathlib import Path

SOURCE = "https://rules.nullsignal.games/"
ROOT = Path(__file__).resolve().parent.parent
RULES_DIR = ROOT / "rules"
TEXT_FILE = RULES_DIR / "comprehensive-rules.md"
MANIFEST_FILE = RULES_DIR / "manifest.json"

# The page draws its symbols as <img class="Symbol" alt="...">. These are the
# spellings card text is quoted with in this repo's doc comments and card
# files; `sub` and `recurring` are the two whose alt is not the house word.
SYMBOLS = {
    "click": "[click]",
    "credit": "[credit]",
    "interrupt": "[interrupt]",
    "link": "[link]",
    "mu": "[mu]",
    "recurring": "[recurring-credit]",
    "sub": "[subroutine]",
    "trash": "[trash]",
    "trashcost": "[trash-cost]",
}

# Where citations are looked for, relative to the repo root. The copy under
# `rules/` is not a citation of itself.
CITATION_GLOBS = ["crates/**/*.rs", "docs/**/*.md", "*.md"]
CITATION = re.compile(r"(?<![A-Za-z])(?:CR|Comprehensive Rules)\s+(\d+(?:\.\d+)*[a-z]?)")


@dataclass
class Entry:
    """A chapter, a section or a rule, in page order."""

    kind: str  # "chapter" | "section" | "rule"
    anchor: str
    number: str
    text: str = ""
    examples: list[str] = field(default_factory=list)
    # Prose under a section heading that belongs to no rule: snippets and
    # timing-structure lines, the latter as (depth, text).
    prose: list[tuple[int, str]] = field(default_factory=list)

    def digest(self) -> str:
        body = [self.text, *self.examples, *(f"{d}:{t}" for d, t in self.prose)]
        return hashlib.sha256("\n".join(body).encode()).hexdigest()[:16]


@dataclass
class Rules:
    version: str
    effective: str
    changes: list[str]
    entries: list[Entry]


def clean(text: str) -> str:
    return re.sub(r"\s+", " ", text).strip()


class RulesParser(HTMLParser):
    """Walk the page once, collecting headings, rules, examples and prose.

    Text is routed by a stack of "sinks": whatever element is collecting
    (a rule's text, an example, a heading, a timing line) is on top, and
    everything else -- the link icons, card thumbnails, the table of
    contents -- is skipped by name.
    """

    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.entries: list[Entry] = []
        self.changes: list[str] = []
        self.version = ""
        self.effective = ""
        self.stack: list[tuple[str, str]] = []  # (tag, role)
        self.buf: list[str] = []
        self.role: str | None = None
        self.last_rule_number = ""
        self.pending: Entry | None = None
        self.timing_depth = 0
        self.skip = 0
        self.in_main = False
        self.in_changes = False
        self.in_effective = False
        self.in_summary = False

    # -- helpers -----------------------------------------------------------

    def _start_capture(self, role: str) -> None:
        self.role = role
        self.buf = []

    def _finish_capture(self) -> str:
        text = clean("".join(self.buf))
        self.role = None
        self.buf = []
        return text

    def _section(self) -> Entry | None:
        for entry in reversed(self.entries):
            if entry.kind in ("section", "chapter"):
                return entry
        return None

    # -- parser callbacks --------------------------------------------------

    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        cls = a.get("class", "") or ""
        role = ""
        if self.skip or tag in ("svg", "script", "style") or "ThumbnailImageContainer" in cls \
                or "RuleAnchor" in cls:
            if tag not in ("img", "br", "use", "path"):
                self.skip += 1
                role = "skip"
            self.stack.append((tag, role))
            return

        if tag == "main":
            self.in_main = True
        elif tag == "summary" and not self.in_main:
            self.in_summary = True
            self.buf = []
        elif tag == "b" and not self.in_main and not self.effective:
            self.in_effective = True
            self.buf = []
        elif tag == "ul" and not self.in_main and self.version and not self.changes:
            # The first list after the summary is the changes; the next is
            # the acknowledgments.
            self.in_changes = True
        elif tag == "li" and self.in_changes:
            self._start_capture("change")
            role = "change"
        elif tag in ("h1", "h2") and ("Chapter" in cls or "Section" in cls):
            kind = "chapter" if "Chapter" in cls else "section"
            self.entries.append(Entry(kind, a["id"], ""))
            self._start_capture("heading")
            role = "heading"
        elif tag == "li" and cls in ("Rule", "SubRule"):
            self.pending = Entry("rule", a["id"], "")
            self.entries.append(self.pending)
            role = "rule"
        elif tag == "a" and "RuleLink" in cls and self.pending is not None and not self.pending.number:
            self._start_capture("number")
            role = "number"
        elif tag == "span" and cls in ("RuleText", "SubSection") and self.pending is not None:
            self._start_capture("ruletext")
            role = "ruletext"
        elif tag == "li" and cls == "Example":
            self._start_capture("example")
            role = "example"
        elif tag == "p" and cls == "Snippet":
            self._start_capture("snippet")
            role = "snippet"
        elif tag == "li" and "TimingStructureL" in cls:
            if self.role == "timing":  # a parent line's text ends at its child list
                self._flush_timing()
            self.timing_depth = int(re.search(r"TimingStructureL(\d)", cls).group(1))
            self._start_capture("timing")
            role = "timing"
        elif tag == "img" and "Symbol" in cls and self.role:
            alt = a.get("alt", "")
            self.buf.append(SYMBOLS.get(alt, f"[{alt}]"))
        self.stack.append((tag, role))

    def _flush_timing(self) -> None:
        text = self._finish_capture()
        section = self._section()
        if text and section is not None:
            section.prose.append((self.timing_depth, text))

    def handle_endtag(self, tag):
        if tag == "summary" and self.in_summary:
            self.in_summary = False
            m = re.search(r"\(v([\d.]+)\)", "".join(self.buf))
            if m:
                self.version = m.group(1)
            self.buf = []
        if tag == "b" and self.in_effective:
            self.in_effective = False
            self.effective = clean("".join(self.buf))
            self.buf = []
        if tag == "ul" and self.in_changes:
            self.in_changes = False
        # Pop to the matching tag; the page is well formed, so this is one pop.
        while self.stack:
            t, role = self.stack.pop()
            if role == "skip":
                self.skip -= 1
            elif role == "heading":
                entry = self.entries[-1]
                m = re.match(r"(\d+(?:\.\d+)*)\.\s*(.*)", self._finish_capture())
                entry.number, entry.text = m.group(1), m.group(2)
            elif role == "number":
                raw = self._finish_capture().rstrip(".")
                if re.fullmatch(r"[a-z]", raw):
                    raw = self.last_rule_number + raw
                else:
                    self.last_rule_number = raw
                self.pending.number = raw
            elif role == "ruletext":
                self.pending.text = self._finish_capture()
            elif role == "rule":
                self.pending = None
            elif role == "example":
                text = self._finish_capture()
                target = self.pending or (self.entries[-1] if self.entries else None)
                if target is not None:
                    target.examples.append(text)
            elif role == "snippet":
                section = self._section()
                if section is not None:
                    section.prose.append((0, self._finish_capture()))
            elif role == "timing":
                if self.role == "timing":
                    self._flush_timing()
            elif role == "change":
                self.changes.append(self._finish_capture())
            if t == tag:
                break

    def handle_data(self, data):
        if self.skip:
            return
        if self.role or self.in_summary or self.in_effective:
            # A rule's text and its examples nest; an example's text must not
            # also land in the rule's, so the innermost capture takes it.
            self.buf.append(data)


def parse(page: str) -> Rules:
    parser = RulesParser()
    parser.feed(page)
    parser.close()
    rules = Rules(parser.version, parser.effective, parser.changes, parser.entries)
    problems = []
    if not rules.version:
        problems.append("no version found in the Summary of Changes")
    if not rules.effective:
        problems.append("no effective date found")
    numbers = [e.number for e in rules.entries]
    if len(set(numbers)) != len(numbers):
        dup = sorted({n for n in numbers if numbers.count(n) > 1})
        problems.append(f"duplicate numbers: {dup[:10]}")
    if any(not e.number or (e.kind == "rule" and not e.text) for e in rules.entries):
        problems.append("an entry with no number or no text")
    if len(rules.entries) < 1000:
        problems.append(f"only {len(rules.entries)} entries -- the page layout has changed")
    if problems:
        sys.exit("rules_sync: the page did not parse as expected:\n  " + "\n  ".join(problems))
    return rules


def fetch() -> str:
    req = urllib.request.Request(SOURCE, headers={"User-Agent": "netrunner-rs rules_sync"})
    with urllib.request.urlopen(req, timeout=60) as resp:
        return resp.read().decode("utf-8")


# -- output ------------------------------------------------------------------


def render_text(rules: Rules) -> str:
    out = [
        f"# Netrunner Comprehensive Rules v{rules.version}",
        "",
        f"Effective {rules.effective}. © Null Signal Games, from <{SOURCE}>, reproduced for",
        "reference and not covered by this repository's licence — see [NOTICE.md](NOTICE.md).",
        "",
        "Generated by `scripts/rules_sync.py`; never edit by hand. One line per rule: its",
        "number in bold, its anchor on the page, then its text. Cite a rule in code as",
        "`CR <number>`, which `crates/netrunner_core/tests/rules_citations.rs` checks",
        "against `manifest.json`.",
        "",
        f"## Summary of Changes (v{rules.version})",
        "",
        *(f"- {c}" for c in rules.changes),
    ]
    for e in rules.entries:
        if e.kind == "chapter":
            out += ["", f"# {e.number}. {e.text} (`{e.anchor}`)"]
        elif e.kind == "section":
            out += ["", f"## {e.number}. {e.text} (`{e.anchor}`)", ""]
        else:
            out.append(f"- **{e.number}** (`{e.anchor}`) {e.text}")
        # The page leaves a timing structure's steps unnumbered in the markup
        # and its text says "go to (d)", so the steps are numbered the way
        # the printed appendix does: phases 1, 2, 3, their steps (a), (b).
        counters = [0, 0, 0, 0]
        for depth, text in e.prose:
            if depth == 0:
                out += [text, ""]
                continue
            counters[depth] += 1
            counters[depth + 1:] = [0] * (len(counters) - depth - 1)
            label = {1: f"{counters[1]}.", 2: f"({chr(96 + counters[2])})"}.get(depth)
            out.append(f"{'  ' * (depth - 1)}- " + (f"{label} {text}" if label else text))
        for ex in e.examples:
            out.append(f"  - *{ex}*" if e.kind == "rule" else f"- *{ex}*")
    return "\n".join(out) + "\n"


def render_manifest(rules: Rules) -> str:
    # One entry per line, in page order: a version bump diffs as the rules
    # that moved, not as a reflowed blob.
    lines = [
        "{",
        f'  "version": {json.dumps(rules.version)},',
        f'  "effective": {json.dumps(rules.effective)},',
        f'  "source": {json.dumps(SOURCE)},',
        '  "entries": [',
    ]
    rows = [
        "    " + json.dumps({"anchor": e.anchor, "number": e.number, "kind": e.kind, "sha": e.digest()})
        for e in rules.entries
    ]
    lines.append(",\n".join(rows))
    lines += ["  ]", "}"]
    return "\n".join(lines) + "\n"


# -- check -------------------------------------------------------------------


def citations() -> dict[str, list[str]]:
    """Every cited number in the repo, with the places that cite it."""
    found: dict[str, list[str]] = {}
    seen: set[Path] = set()
    for pattern in CITATION_GLOBS:
        for path in sorted(ROOT.glob(pattern)):
            if path in seen or "target" in path.parts or path.is_relative_to(RULES_DIR):
                continue
            seen.add(path)
            for n, line in enumerate(path.read_text(errors="replace").splitlines(), 1):
                for m in CITATION.finditer(line):
                    found.setdefault(m.group(1), []).append(f"{path.relative_to(ROOT)}:{n}")
    return found


def check(live: Rules) -> int:
    committed = json.loads(MANIFEST_FILE.read_text())
    old = {e["anchor"]: e for e in committed["entries"]}
    new = {e.anchor: e for e in live.entries}
    added = [a for a in new if a not in old]
    removed = [a for a in old if a not in new]
    changed = [a for a in new if a in old and new[a].digest() != old[a]["sha"]]
    moved = [a for a in new if a in old and new[a].number != old[a]["number"]]

    report = []
    if live.version != committed["version"]:
        report.append(f"Version: v{committed['version']} → v{live.version} (effective {live.effective})")
    for title, anchors, fmt in [
        ("Added", added, lambda a: f"{new[a].number} `{a}`"),
        ("Removed", removed, lambda a: f"{old[a]['number']} `{a}`"),
        ("Text changed", changed, lambda a: f"{new[a].number} `{a}`"),
        ("Renumbered", moved, lambda a: f"{old[a]['number']} → {new[a].number} `{a}`"),
    ]:
        if anchors:
            report += ["", f"### {title} ({len(anchors)})", *(f"- {fmt(a)}" for a in anchors)]

    # A citation is by the number the committed copy gave the rule, so the
    # places to re-read are the ones citing an old number whose rule moved on.
    by_number = {e["number"]: e["anchor"] for e in committed["entries"]}
    touched = set(removed) | set(changed) | set(moved)
    hits = [
        (num, by_number[num], places)
        for num, places in sorted(citations().items())
        if by_number.get(num) in touched
    ]
    if hits:
        report += ["", "### Citations to re-read"]
        for num, anchor, places in hits:
            state = "removed" if anchor in removed else "renumbered" if anchor in moved else "changed"
            report.append(f"- CR {num} (`{anchor}`, {state}): " + ", ".join(places))

    if not report:
        print(f"rules_sync: v{live.version} matches the committed copy")
        return 0
    if live.changes and live.version != committed["version"]:
        report += ["", f"### NSG's own summary (v{live.version})", *(f"- {c}" for c in live.changes)]
    print("\n".join(report).lstrip())
    return 1


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--check", action="store_true", help="compare the live page with the committed copy; write nothing")
    ap.add_argument("--from", dest="source", type=Path, help="parse a saved copy of the page instead of fetching it")
    args = ap.parse_args()

    page = args.source.read_text() if args.source else fetch()
    rules = parse(page)
    if args.check:
        return check(rules)
    RULES_DIR.mkdir(exist_ok=True)
    TEXT_FILE.write_text(render_text(rules))
    MANIFEST_FILE.write_text(render_manifest(rules))
    print(f"rules_sync: wrote v{rules.version} ({len(rules.entries)} entries) to {RULES_DIR.relative_to(ROOT)}/")
    return 0


if __name__ == "__main__":
    sys.exit(main())
