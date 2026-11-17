---
name: upstream-rebase
description: Rebase this fork's personal commits onto upstream Horizon main (origin is the fork, upstream is horizon-rust/horizon), resolve conflicts and API drift, review only the rebase delta, run the fork-aware CI self-check, and squash fixes into the right commits. Use when upstream moved and a feat/... bookmark must be rebased, re-adapted or re-verified.
---

# Rebase the fork onto upstream

`origin` is the personal fork (`0xgsvs/horizon`), `upstream` is `horizon-rust/horizon`. Personal work sits on
bookmarks such as `feat/ely.by`, on top of a small fork-tooling stack; `main` and `main@upstream` are
upstream's. **Never rewrite an upstream commit.** In this repository `trunk()` resolves to `main@origin` (the
fork's copy), not to upstream, so name `main@upstream` explicitly everywhere.

The loop is **snapshot, fetch, rebase, resolve bottom-up, adapt beyond conflicts, review the delta, gates,
absorb and describe**. Steps 3-5 are judgment; do not script them.

## 0. Snapshot

```sh
BK=feat/ely.by
STAMP=$(date +%F-%H%M%S)
jj st                                   # no uncommitted work
jj log -r 'conflicts()' --no-graph      # must be empty before starting
OLD_UPSTREAM=$(jj log -r 'main@upstream' --no-graph -T commit_id)
FORK_BASE=$(jj log -r "roots(main@upstream..$BK)-" --no-graph -T commit_id)
mkdir -p ".local/rebase/$STAMP"
printf 'OLD_UPSTREAM=%s\nFORK_BASE=%s\n' "$OLD_UPSTREAM" "$FORK_BASE" \
  | tee ".local/rebase/$STAMP/bases.txt"
jj log -r "main@upstream..$BK" --no-graph \
  -T 'commit_id ++ "\t" ++ description.first_line() ++ "\n"' \
  | tee ".local/rebase/$STAMP/before.txt"
```

`before.txt` keeps the old commit ids for the per-commit review diffs; `bases.txt` keeps the old upstream tip
and the fork base (`. .local/rebase/$STAMP/bases.txt` in a later shell). The upstream delta a rebase inserts is
`FORK_BASE..main@upstream`, which is often larger than the new tip's parent chain when the fork sat on an older
main. `jj op log`, `jj undo` and `jj op restore` are the undo. Do not pin the pre-rebase commits with a bookmark or a tag: a second visible
copy of a rewritten change makes `jj log` and jjui mark every rewritten commit divergent, and the old commits
stay addressable by the ids in `before.txt` while the rewrite is in `jj op log`. `.local/` is ignored by git.

## 1. Fetch

```sh
jj git fetch --remote upstream          # add --remote origin when the fork moved too
jj log -r "$OLD_UPSTREAM" -n 1          # previous upstream tip (bases.txt)
jj log -r 'main@upstream' -n 1
```

If `main@upstream` did not move, stop: there is nothing to rebase. `main@upstream-` is the new tip's parent,
not the previous tip; `OLD_UPSTREAM` from step 0 is.

Use jj, not `git fetch` + `git rebase`: a conflicted jj commit is exported to Git with `.jjconflict-*` trees,
and Git then reports thousands of staged deletions that jj does not have.

## 2. Rebase the whole stack

```sh
jj rebase -b "$BK" -d main@upstream
```

`-b` moves the branch (the bookmark's ancestors missing from the destination) and its descendants. Use
`-s <first-personal-change>` when the personal commits are not all ancestors of the bookmark. Never rebase
only `@`.

## 3. Resolve conflicts bottom-up

```sh
jj log -r 'conflicts()' --no-graph
jj resolve --list
```

`@` is often listed only because it inherits a parent's conflict; resolve the **lowest real** conflicted
commit first:

```sh
jj new <lowest-conflicted-commit>
# edit the conflicted paths
jj diff
jj squash                                # into that commit
jj log -r 'conflicts()'                  # it must drop off
jj new <next-conflicted-commit>          # then the next one up
```

Re-read the conflict list after every squash: rewriting a lower commit rebases its descendants, and an upper
conflict can change shape or disappear. Squashing into the branch tip leaves the lower commit conflicted and
every build broken.

Rules that keep a resolution correct:

- **Markers are not the truth.** Read both real sides with `jj file show -r <rev> <path>`. Use
  `jj diff --git --from <a> --to <b> -- <path>` instead of decoding `%%%%%%%` blocks on large files.
- **An empty destination block means upstream removed or moved the region.** Accept the removal, or port the
  behavior to its new home; do not resurrect deleted code.
- **A deleted path is usually a moved file.** Check `jj file list -r <destination> <dir>`, then port the hunks
  to the new path.
- **Never union-merge a refactor.** When upstream split a file or changed a signature, write the destination's
  file plus only the branch's feature, adapted to the new API. Find each hunk's new home by grepping the
  destination tree, not the conflicted working copy.
- **Do not take the branch's import or test edits wholesale.** Upstream's newer tests may still use what the
  branch dropped; keep upstream's tests and adapt only the feature's call sites.
- **`launcher/lang/*.json`:** take upstream's side. This fork does not curate keys; new strings use `tr()` and
  fall back to English, and upstream's i18n test no longer requires a merge.

## 4. Adapt beyond conflicts

A conflict-free file can still be stale: an upstream signature changed, a new entry point bypasses the
feature, or a moved module no longer calls it. The compiler and the upstream log are the audit.

```sh
jj log -r "$FORK_BASE..main@upstream" --no-graph --stat
jj log -r "$FORK_BASE..main@upstream" --no-graph \
  -T 'commit_id.short() ++ " " ++ description.first_line() ++ "\n"' -- <paths the fork touches>
```

This is everything the rebase inserts; the new tip's parent alone misses commits when the fork base is older.

- Compile every affected workspace: `cargo check -p horizon-game` (gameplay and its executable),
  `-p horizon-client`, and `cargo check --manifest-path launcher/Cargo.toml --all-targets`.
- Grep for new call sites of the fork's API and for every new screen, menu or packet path upstream added; a
  feature that compiles can still be unreachable.
- Update `docs/api/` and tests when behavior changed. A new outside host belongs in `horizon-addresses`, never
  hardcoded (the address gate rejects it).
- If a file is near the 3000-line gate, put new tests in a sibling `*_tests.rs`.

## 5. Review only the rebase delta

The feature was reviewed when it landed; review what this rebase changed, not the branch again. Give the
reviewer the whole fork delta only as context for intent, never as the thing to re-review.

For each rewritten commit, compare its own patch, not old-to-new: `jj diff --from <old> --to <new>` carries
the whole upstream delta, while `jj interdiff --from <old-id> --to <new-id>` shows only what the rewrite did
to that one commit's patch. The old ids are in `.local/rebase/$STAMP/before.txt`, `FORK_BASE` and
`OLD_UPSTREAM` in `bases.txt`:

```sh
jj log -r "main@upstream..$BK" --no-graph \
  -T 'commit_id ++ "\t" ++ description.first_line() ++ "\n"'
# per commit: the old id from before.txt, the new one from the line above
jj interdiff --from <old-id> --to <new-id>    # no output: the rewrite left this patch alone
jj evolog -r <rev>                            # every rewrite of one change, with the operation behind it
```

Only the commits whose `jj interdiff` prints something need reading; that delta is the review input, not the
whole fork delta.

While a pre-rebase ref (a bookmark or tag) or the branch's remote still points at the old commits, `jj log`
and jjui show each rewritten change as **divergent**: the old commit and its new version share a change id.
That is the normal old-vs-new view, not a conflict. Address the old ones with the ids in `before.txt`, and
delete any temporary ref once the review is done.

Delegate twice, with the `programming` skill loaded:

1. **Before editing.** Give the reviewer the upstream delta (`$FORK_BASE..main@upstream`), the fork delta
   (`main@upstream..$BK`), the `range-diff` output, the paths and the compile results, and ask for API-drift,
   lost-behavior and security risks plus a minimal plan. Read-only: no edits, no builds in this pass.
2. **After editing.** For a non-trivial resolution, or one that adapted a signature or an entry point, give the
   per-commit patch deltas from `.local/rebase/$STAMP/` and the acceptance criteria, and ask for findings with
   `file:line`, severity and a minimal fix, then `APPROVE` or `CHANGES REQUIRED`. Skip this pass only when the
   rebase is mechanical and the first pass made no edit; any code change after a review reopens it.

In OmO: `task({ subagent_type: "omo-native-code-reviewer", load_skills: ["programming"], run_in_background: true })`.
One reviewer is enough; do not fan out. Skip commits whose old and new versions are identical.

## 6. Gates

```sh
python3 tools/pr_ready.py --base upstream/main
```

That is the fork-aware self-check and already runs CI's set for the changed files: rustfmt, clippy with
warnings denied, the touched crates' tests with CI's features, the launcher workspace (both feature legs),
the Python tests, `check_public_tree`, `check_addresses` and the file-size gate. Run it once at the end; do not
re-implement it in the loop.

During the loop use only cheap checks:

```sh
cargo fmt --all
cargo check -p <touched-crate>
cargo check --manifest-path launcher/Cargo.toml --all-targets
```

Skip clippy in the loop: `pr_ready` runs it with CI's features and warnings denied. Run a targeted clippy only
when you suspect a lint. `launcher/` is a separate workspace: root commands do not touch it, and CI runs clippy
twice (`--features no-auto-install,download-without-owner` too). Run one root-workspace cargo leg at a time:
they queue on `target/`'s lock (a waiting cargo has no `rustc` children). The launcher has its own `target/`, so
its leg may run alongside a root leg; both are CPU-heavy.

`pr_ready` is the slowest step (a cold clippy 20+ minutes): start it in the background as soon as the stack is
final, then finish the delta review while it runs. A docs-only change after it starts does not need a full
rerun when the code tree is identical; rerun `check_public_tree.py`, `check_repository_hygiene.py`,
`check_layers.py` and `check_addresses.py` on the final tree instead.

Known environment-only failures: `gfx` depth-texture tests fail on this WSL box (`TextureLoad from depth
textures is not supported in GLSL`). Report them; do not chase code that cannot reach them. A newer local
rustfmt can also flag upstream `crates/horizon-client/src/game/**` files that the branch does not touch
(2026-11-17: 53 files under rustfmt 1.10.0). Check `jj diff --name-only --from main@upstream --to "$BK"`; if none of the
flagged paths is in the branch, report it and do not reformat upstream. `tests/test_hotspots_carry.py` fails
when the global git config sets an external diff (`diff.external`, e.g. difftastic): the tool's `git diff
-U0` output loses its hunks. Rerun with `GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1`, or fix the tool to
isolate its git environment.

## 7. Squash and describe

- Pure conflict resolution: resolve in `@`, then `jj absorb --into "main@upstream..$BK"` (line-based, into the
  personal commit that last touched each line) and inspect `jj log`/`jj diff`. Restricting `--into` keeps the
  rewrite inside the personal stack; upstream commits must stay untouched. If a hunk lands in the wrong
  commit, use `jj squash --into <rev>` for the whole file.
- New behavior discovered while adapting gets its own commit, not a squash into an unrelated one.
- After absorbing, re-read the commit (`jj show <rev>`) and update its description when its scope changed
  (`jj describe -r <rev>`).
- `jj describe -r <rev> -m "type(scope): subject"` - Conventional Commits, subject only.
- Rewriting a commit moves its bookmark; otherwise `jj bookmark set "$BK" -r <tip>`.

## 8. Done when

```sh
jj st                                   # clean
jj log -r 'conflicts()' --no-graph      # empty
jj log -r "main@upstream..$BK" --no-graph
jj diff --stat --from main@upstream --to "$BK"
```

The stack is on `main@upstream`, every commit still builds, the delta review has no open `CHANGES REQUIRED`,
`pr_ready.py --base upstream/main` is green or red only on checks provably outside the branch (step 6), and the
branch bookmark is where you expect it. `jj log -r 'divergent()' --no-graph` is empty once no pre-rebase ref is
left (a pushed branch's old commits stay visible until it is pushed). Pushing is a separate decision; this loop
ends locally.

## Why a skill and not a script

The deterministic parts are jj commands and `pr_ready.py`; the resolution and adaptation steps depend on why
the two sides differ (moved, deleted, renamed, signature changed, new entry point), which a merge script cannot
see. An automatic resolver would produce a tree that compiles but silently drops behavior. A plugin adds
packaging without doing the judgment either. Keep this file as the loop; change it when the repo's gates or
jj version change.
