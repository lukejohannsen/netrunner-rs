---
name: steward
description: How an agent drives a pull request in this repository to merge — when to bring main in, and what to do about CI — so several agents working at once do not re-run each other's CI.
---

# Driving a PR here

Several agents open PRs against `main` at the same time. Each CI run costs
about 16 minutes, so the expensive mistake is re-running one that already
passed. AGENTS.md (Git Hygiene → Branches, Merging) is the rule; this is how
it applies to the events a PR sends you.

- **`main` moved and the PR is green:** do nothing. Do not rebase, do not
  merge `main` in, do not re-run CI. `main` is not required to be up to date
  with a branch before it merges.
- **"Base branch recovered":** only act if this PR's CI is red *because of*
  the failure on `main`. A green PR ignores it; a PR whose failure is its
  own is fixed on its own.
- **Merge conflict:** rebase onto `main`, resolve, validate, push. This is
  the one routine reason to bring `main` in. (On a branch someone else
  created, merge `main` instead of rebasing, as the general rules say.)
- **The branch needs something that landed on `main`** (a new API, a file
  `main` rewrote): rebase onto it, as for a conflict.
- **After the merge:** watch `main`'s CI run on the merge. It is skipped
  when it would only repeat the PR's green run; when it runs and goes red,
  the last merge fixes it forward before anything else lands.
