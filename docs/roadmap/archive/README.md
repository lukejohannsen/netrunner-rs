# The roadmap archive

The closed record. When an item in an area roadmap under `docs/roadmap/` closes, its full entry — the decision, the alternative rejected, the load-bearing numbers — is written as always and then **moved here verbatim, heading included**, under the same parent heading in the file of the same name. The live area file keeps one sentence and the address.

Headings are kept exactly, so an address a code comment cites ("Phase 7 §4bj", "Phase 2 §5 item 35", "RWR Stage 3b", "Rules Audit T8") resolves here by grep when it no longer resolves in the live file. `ROADMAP-2026-09-29.md` is the index as it stood the day the archive was made; its history paragraphs live nowhere else.

Nothing here is needed to orient. Open a file in this directory only when a number or a decision in the live roadmap needs its provenance. The `CR <number>` citation gate (`crates/netrunner_core/tests/rules_citations.rs`) scans `docs/**`, so text moved here stays checked.
