#!/usr/bin/env bash
# The merge bar as one command: exits 0 when a branch has passed what CI's
# required `ci` check runs on a pull request, plus the desktop crate, which
# CI leaves to the weekly platforms.yml and AGENTS.md (Testing Rule) leaves
# to whoever merges. "Done" is this exiting 0, run by whoever claims done.
#
# Mirrors ci.yml, cheapest step first:
#   workflows  zizmor (offline, severity low) and actionlint over .github/
#   deny       cargo deny check advisories bans licenses sources, with the
#              action's own default arguments (`--all-features` resolves the
#              graph only; nothing is built with it)
#   secrets    gitleaks over the whole history (CI scans a PR's commits);
#              commits only, so a secret in an uncommitted file passes here
#   clippy     cargo clippy --workspace --all-targets -- -D warnings
#   test       cargo test --workspace: ci.yml's `test` and `sweeps` jobs in
#              one command, the sweeps at their default 32 seeds
# with one difference on purpose: the desktop crate is in, because nothing
# else checks it before a merge.
#
# CI-only, not run here: the 256-seed sweeps (deep-sweep.yml, weekly, and
# still yours to run before merging engine-level work), everything in
# platforms.yml (Windows, macOS, beta, rustdoc, each optional feature, the
# Postgres store, cargo-machete), the weekly rules-watch.yml, and main's
# post-merge run. `gh workflow run platforms.yml --ref <branch>` asks for
# the platforms before a merge.
#
# Deliberately absent, for the reasons ci.yml's header gives: `cargo fmt
# --check` (the tree has never been rustfmt-formatted) and `--all-features`
# on a build. RUSTFLAGS is not set to `-D warnings` for the test build:
# clippy above already denies every rustc warning over the same targets,
# and a second set of flags would rebuild the workspace from scratch beside
# the ordinary `cargo test` cache. Export CARGO_PROFILE_DEV_OPT_LEVEL=2 to
# build as CI does; the sweeps are faster for it.
#
# A tool that is missing is a failure, never a skip: a gate that quietly
# runs less than it says is how a green result stops meaning anything.

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

current=setup
trap 'echo "gate: FAILED at ${current}" >&2' ERR

step() {
    current=$1
    printf '\n==> %s\n' "$1"
}

step "tools"
missing=()
# actionlint checks `run:` blocks with shellcheck only when that is on PATH,
# and says nothing when it is not, so shellcheck is required too.
for tool in cargo cargo-clippy cargo-deny zizmor actionlint shellcheck gitleaks; do
    command -v "$tool" >/dev/null 2>&1 || missing+=("$tool")
done
if ((${#missing[@]})); then
    echo "gate: missing required tools: ${missing[*]}" >&2
    false
fi

step "workflows (zizmor)"
zizmor --no-online-audits --min-severity low .

step "workflows (actionlint)"
# ci.yml's one flag, for the same reason: actionlint does not know zizmor's
# `$/` form for a local action.
actionlint -ignore '\$/\.github'

step "deny"
cargo deny --all-features --log-level warn check advisories bans licenses sources

step "secrets (gitleaks)"
gitleaks git --redact --no-banner .

step "clippy"
cargo clippy --workspace --all-targets --locked -- -D warnings

step "test"
cargo test --workspace --locked

current=finished
printf '\ngate: passed\n'
