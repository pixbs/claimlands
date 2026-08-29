# Working in parallel

How several agents share this repository without treading on each other.

## One agent, one worktree, one branch, one issue

No shared checkouts. Each agent gets its own working copy:

```bash
git worktree add ../hex-planet-42 -b core/42-territory-split
cd ../hex-planet-42
git config core.hooksPath .githooks
```

Branch names are `<crate-scope>/<issue#>-<slug>`:

```
core/42-territory-split
render/71-cloud-banding
levels/88-share-codes
```

The scope matches the crate labels on issues, so `gh issue list --label
crate:core --label ready` is an agent's queue.

## Dispatching an agent

```bash
cargo xtask brief 2
```

That prints a complete prompt for issue #2 — worktree and branch commands, what
to read, the issue body, the definition of done, and the stop conditions — ready
to paste into whatever agent you are using.

**The prompt is deliberately thin, because the repository is thick.** Every rule
worth stating lives in `AGENTS.md`, the per-crate `AGENTS.md`, `spec/rules/` and
`docs/determinism.md`. A prompt that restated any of it would become a second
source of truth and drift from the first, so the brief points at those files
instead of copying them. It also strips the issue's own human-facing footer for
the same reason.

Three things *are* spelled out in the prompt, because they are the cases where a
failing gate looks like an obstacle to remove rather than a bug to fix:

| An agent sees | The prompt tells it |
|---|---|
| A golden replay hash changed | You changed behaviour. Do **not** `golden record` to make it pass — that deletes the only evidence the regression exists. |
| `spec-coverage` fails | Write the rule in `spec/rules/` and add `covers!()`. The gate is telling you it is undocumented or untested. |
| `check-deps` fails | You pointed a dependency the wrong way. Redesign — do not edit the layer table. |

The single most valuable line is the first one. An agent that re-records a hash
to get a green build has defeated the entire regression net in one commit, and
the diff looks innocuous.

The prompt also names the conditions to **stop and ask** rather than guess: work
that needs another crate, ambiguous acceptance criteria, or a rule that is not
written down yet. Those are the situations where a confident wrong answer costs
more than a question.

## Stacked pull requests

GitHub shipped native stacked pull requests to public preview on 2026-07-30, so
no third-party service is needed.

Manage the local branch chain with [git-town](https://www.git-town.com):

```bash
git town hack   core/42-territory-split     # branch off master
git town append core/43-territory-merge     # branch off the previous one
git town sync                               # rebase the whole chain on master
```

Then publish the chain:

```bash
gh stack link
```

Rules of thumb for stacking:

- **Each branch must pass CI on its own.** A stack of green commits is
  reviewable; a stack that only works at the tip is one big pull request
  wearing a disguise.
- **Split by risk, not by size.** A refactor and the feature it enables belong
  in separate branches even when both are small, because the reviewer needs to
  see that the refactor changed nothing.
- **Re-record golden replays in their own branch**, never mixed with the change
  that moved them.

## Merge queue

Enabled on `master`. Every pull request must be current with `master` before it
lands, which is what stops two independently-green changes from combining into
a broken one.

## What CI runs

| Job | Gate | Runs when |
|---|---|---|
| `lint` | 1 | always |
| `test` | 2, 3, 4 | always |
| `determinism` (Linux/macOS/Windows) | 5 | always |
| `repo-rules` (`cargo xtask ci`) | 6, 12 | always |
| `deny` | 6 | always |
| `coverage` | 10 | always |
| `commit-hygiene` | — | always |
| `mobile` | 9 | only when platform, render or FFI paths change, plus nightly |

The `mobile` job is path-scoped because a twelve-minute NDK build on a pull
request that only touches the economy is pure friction.

## Commits

Conventional Commits, scope = crate:

```
feat(core): territory split conserves the treasury (#42)
fix(render): stop cloud banding at the poles (#71)
docs(spec): pin the tie-break for equidistant capitals (#88)
```

**Never mention an AI assistant** in a message, body, trailer, co-author or
author field. Enforced in three places, because a prompt-level instruction is a
request and this is a requirement:

1. `.githooks/commit-msg` — local, immediate.
2. The `commit-hygiene` CI job — scans the whole pull request range, so
   `--no-verify` does not help.
3. Branch protection requires that job, so the merge queue cannot admit a
   violating commit.

## Work tracking

**GitHub Issues and Projects**, not an in-repo backlog file.

With many agents, a markdown task list in the repository is a permanent
merge-conflict magnet. Issues have no such problem, are API-accessible so an
agent can self-serve its queue, and link natively to the stacked-PR feature.

- Issue templates force the fields an agent actually needs: target crate, spec
  rule ids, acceptance criteria, and which tests must be added.
- Labels mirror crate names (`crate:core`, `crate:render`) plus `ready`,
  `blocked`, `needs-spec`.
- A Projects v2 board with a **Ready for agent** column is the dispatch queue.
- Bare `TODO` comments are banned in CI. Only `// TODO(#123):` passes, so no
  intent is ever lost in a comment nobody is tracking.

## Before opening a pull request

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo xtask ci
```

## When a golden replay hash changes

This is the moment that matters most, so it is worth being explicit.

A hash moving means **behaviour changed**. There are exactly two cases:

- **You meant it.** Re-record with
  `cargo run -p civ-cli -- golden record <file>`, in a commit of its own, and
  list in the pull request which scenarios moved and why. A reviewer should be
  able to check that the list matches the intent.

- **You did not mean it.** You have found a regression. Do not re-record —
  find out what moved. This is the gate doing its job.
