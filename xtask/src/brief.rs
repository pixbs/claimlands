//! `cargo xtask brief <issue>` — emit a ready-to-paste agent prompt.
//!
//! # Why this is short
//!
//! Everything durable an agent needs is already in the repository: `AGENTS.md`,
//! the per-crate `AGENTS.md`, `spec/rules/`, `docs/determinism.md`. A prompt
//! that restated any of it would become a second source of truth and drift from
//! the first. So this emits **dispatch instructions only** — which issue, where
//! to work, when to stop — and points at the repo for the rules.
//!
//! The parts that *are* spelled out here are the ones where a gate failing
//! looks like an obstacle to remove rather than a bug to fix. An agent that
//! re-records a golden hash to make CI green has defeated the entire
//! regression net, so that instruction earns its place in the prompt rather
//! than sitting only in a document.

use std::process::Command;

const REPO: &str = "pixbs/claimlands";

/// Separates the fields `gh` returns. Chosen to be something no issue body
/// would ever contain, since bodies are arbitrary markdown.
const FIELD_SEP: &str = "<<<xtask-field>>>";

pub fn emit(issue: Option<&str>) -> Result<String, String> {
    let number = issue.ok_or_else(|| {
        "usage: cargo xtask brief <issue-number>\n\n\
         Example: cargo xtask brief 2"
            .to_owned()
    })?;
    number
        .parse::<u32>()
        .map_err(|_| format!("`{number}` is not an issue number"))?;

    let fields = fetch(number)?;
    Ok(render(number, &fields))
}

struct Issue {
    title: String,
    url: String,
    labels: String,
    milestone: String,
    body: String,
}

/// One `gh` call with a sentinel-delimited result, so xtask needs no JSON
/// parser. The sentinel is plain ASCII rather than a NUL byte, which cannot be
/// written into a shell argument portably.
fn fetch(number: &str) -> Result<Issue, String> {
    let out = Command::new("gh")
        .args([
            "issue",
            "view",
            number,
            "--repo",
            REPO,
            "--json",
            "title,url,labels,milestone,body",
            "--jq",
            &format!(
                r#"[.title, .url, ([.labels[].name] | join(",")), (.milestone.title // ""), .body] | join("{FIELD_SEP}")"#
            ),
        ])
        .output()
        .map_err(|e| format!("could not run gh: {e}. Is the GitHub CLI installed and logged in?"))?;

    if !out.status.success() {
        return Err(format!(
            "gh could not read issue #{number}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }

    let text = String::from_utf8_lossy(&out.stdout);
    let parts: Vec<&str> = text.trim_end().split(FIELD_SEP).collect();
    if parts.len() < 5 {
        return Err(format!("unexpected gh output for issue #{number}"));
    }

    Ok(Issue {
        title: parts[0].to_owned(),
        url: parts[1].to_owned(),
        labels: parts[2].to_owned(),
        milestone: parts[3].to_owned(),
        body: parts[4..].join(FIELD_SEP),
    })
}

/// `feat(worldgen): port the ...` -> `worldgen`.
///
/// Falls back to the `crate:` label, then to `misc`, so a badly-titled issue
/// still produces a usable branch name.
fn scope_of(issue: &Issue) -> String {
    if let Some(open) = issue.title.find('(')
        && let Some(close) = issue.title[open..].find(')')
    {
        let scope = &issue.title[open + 1..open + close];
        if !scope.is_empty() {
            return scope.to_owned();
        }
    }
    issue
        .labels
        .split(',')
        .find_map(|l| l.trim().strip_prefix("crate:").map(str::to_owned))
        .unwrap_or_else(|| "misc".to_owned())
}

/// The part of the title after `type(scope): `, as a branch-safe slug.
///
/// Truncates at a word boundary rather than mid-word, so the branch name still
/// reads as English in `git branch` output.
fn slug_of(title: &str) -> String {
    const LIMIT: usize = 40;

    let tail = title.split_once(": ").map_or(title, |(_, rest)| rest);
    let mut slug = String::new();
    for c in tail.chars() {
        if c.is_ascii_alphanumeric() {
            slug.extend(c.to_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }

    let slug = slug.trim_matches('-');
    if slug.len() <= LIMIT {
        return slug.to_owned();
    }
    match slug[..LIMIT].rfind('-') {
        // Only cut at a boundary if enough of the title survives to identify it.
        Some(cut) if cut >= 12 => slug[..cut].to_owned(),
        _ => slug[..LIMIT].trim_end_matches('-').to_owned(),
    }
}

/// Strip the shared footer every issue carries.
///
/// That footer exists for a human reading the issue on GitHub. Inside a brief it
/// would repeat, almost word for word, what the prompt says a few lines later —
/// and two copies of an instruction is how one of them ends up stale.
fn without_footer(body: &str) -> &str {
    match body.find("\n---\nRead `AGENTS.md` before starting.") {
        Some(cut) => body[..cut].trim_end(),
        None => body.trim_end(),
    }
}

/// Which crate directory the agent should read the local AGENTS.md from.
fn crate_dir(issue: &Issue) -> Option<String> {
    issue
        .labels
        .split(',')
        .find_map(|l| l.trim().strip_prefix("crate:"))
        .map(|c| match c {
            "core" => "crates/lands-core".to_owned(),
            "worldgen" => "crates/lands-worldgen".to_owned(),
            "render" => "crates/lands-render".to_owned(),
            "app" => "crates/lands-app".to_owned(),
            "ai" => "crates/lands-ai".to_owned(),
            "levels" => "crates/lands-levels".to_owned(),
            other => format!("crates/lands-{other}"),
        })
}

fn render(number: &str, issue: &Issue) -> String {
    let scope = scope_of(issue);
    let slug = slug_of(&issue.title);
    let branch = format!("{scope}/{number}-{slug}");

    // Built as a list and numbered afterwards, because the crate-local entry is
    // conditional and hard-coded numbers would skip a step without it.
    let mut reading = vec![
        "`AGENTS.md` at the repository root — the contract. It is short.".to_owned(),
        "Every `spec/rules/*.md` section for the rule ids this issue names.".to_owned(),
    ];
    if let Some(dir) = crate_dir(issue) {
        reading.push(format!(
            "`{dir}/AGENTS.md` — rules local to the crate you are changing"
        ));
    }
    reading.push(
        "`docs/determinism.md` if you are touching `lands-core`. Non-negotiable\n   \
         there: no floating point, no `HashMap`/`HashSet`, no clock, and\n   \
         randomness only via `lands_core::rng::stream`."
            .to_owned(),
    );
    let reading = reading
        .iter()
        .enumerate()
        .map(|(i, line)| format!("{}. {line}", i + 1))
        .collect::<Vec<_>>()
        .join("\n");

    let milestone = if issue.milestone.is_empty() {
        String::new()
    } else {
        format!(" · milestone {}", issue.milestone)
    };

    // Reuse the issue's own commit type so the example matches what the agent
    // will actually be writing.
    let scope_example = format!(
        "{}({scope}): <what changed> (#{number})",
        verb_of(&issue.title)
    );

    format!(
        r#"# Read this first

**Nothing you produce may name an assistant.** Not the branch, not a commit
message, body, trailer, co-author or author field, and not the pull request —
no "Generated with ..." footer, no "Co-Authored-By:" line. Write the work as
its author.

**Never name the branch after a tool.** `claude/...`, `codex/...` and the like
are refused; a branch is named for the work. Use the one below and do not
invent your own:

    {branch}

Both are gates, not requests: the `commit-msg` hook, `cargo xtask
check-commits` over the whole pull request range, and `cargo xtask check-pr`
over its body and branch. `--no-verify` reaches none of them.

---

Work issue #{number} in {REPO}: {title}

{url}
Labels: {labels}{milestone}

## Set up an isolated worktree

    git worktree add ../claimlands-{number} -b {branch}
    cd ../claimlands-{number}
    git config core.hooksPath .githooks

Work only in this worktree. Other agents are working in theirs at the same time.

## Read before writing any code

{reading}

Do not restate those rules back to me — just follow them.

## The issue

{body}

## Show your work in the preview

If this issue produces anything spatial or graphical, it must be visible in the
WebGPU preview — your own file under `crates/lands-app/src/debug/` plus one
appended line in the registry. One file per feature, so several agents can add
panels at once without touching each other's work.

Tests prove the numbers are right, not that the planet is right. Cover scattered
evenly and cover properly clumped produce identical share statistics, and only
one of them is correct.

Work with no spatial or graphical output — a trait, a file format, CI, tooling —
is exempt. Do not invent a contrived panel to satisfy this.

## Done means all four of these pass

    cargo fmt --all
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    cargo xtask ci

Then open the pull request against `master`. **Never push to `master` directly.**

Write the body from `.github/pull_request_template.md` and keep its first line,
with the number filled in:

    Closes #{number}

Do **not** use `gh pr create --fill`. It replaces the body with your commit
message, and the commit convention (`(#{number})`) is a bare reference that links
the issue without closing it — the pull request merges and the issue stays open.
`cargo xtask check-pr` runs on every pull request and will fail.

    gh pr create --base master --title "<your commit subject>" --body-file <your body>

## Three failures that look like obstacles but are findings

- **A golden replay hash changed.** You changed behaviour. If this issue did not
  ask you to, that is your regression — find it. Do NOT run `golden record` to
  make the test pass; that deletes the only evidence the bug exists.
- **`spec-coverage` fails on a rule you added.** Write the rule in
  `spec/rules/` with a new id and add `covers!("YOUR-ID")` to its test. The gate
  is telling you the rule is undocumented or untested.
- **`check-deps` fails.** You added a dependency pointing the wrong way in the
  crate graph. Redesign the change. Do not edit the layer table in
  `xtask/src/deps.rs` to make it legal.

## Adding randomness

Append a new variant to `SeedDomain`. Never renumber or remove an existing one —
those numbers are baked into every replay ever recorded, and reordering them
silently invalidates the whole corpus.

## Stop and ask rather than guessing when

- the work needs a change in a crate this issue does not name
- the acceptance criteria are ambiguous or look wrong
- a rule you need is not written in `spec/` and the issue does not say to write it

## Commits

Conventional Commits, scope = crate:

    {scope_example}

**Never name an AI assistant** in a commit message, body, trailer, co-author or
author field. A local hook and a CI job over the whole PR range both enforce it,
so `--no-verify` will not help.

If a golden replay hash legitimately moved, re-record it in a **separate commit**
and list in the PR which scenarios moved and why.
"#,
        number = number,
        REPO = REPO,
        title = issue.title,
        url = issue.url,
        labels = issue.labels,
        milestone = milestone,
        branch = branch,
        reading = reading,
        body = without_footer(&issue.body),
        scope_example = scope_example,
    )
}

/// Reuse the issue title's own conventional-commit type for the example.
fn verb_of(title: &str) -> &str {
    match title.split(['(', ':']).next().unwrap_or("feat").trim() {
        "fix" => "fix",
        "docs" => "docs",
        "chore" => "chore",
        "refactor" => "refactor",
        "test" => "test",
        "perf" => "perf",
        "ci" => "ci",
        _ => "feat",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(title: &str, labels: &str) -> Issue {
        Issue {
            title: title.to_owned(),
            url: String::new(),
            labels: labels.to_owned(),
            milestone: String::new(),
            body: String::new(),
        }
    }

    #[test]
    fn takes_the_scope_from_a_conventional_title() {
        assert_eq!(scope_of(&issue("feat(worldgen): port it", "")), "worldgen");
    }

    #[test]
    fn falls_back_to_the_crate_label() {
        assert_eq!(
            scope_of(&issue("do a thing", "crate:render,ready")),
            "render"
        );
    }

    #[test]
    fn falls_back_again_when_there_is_nothing_to_go_on() {
        assert_eq!(scope_of(&issue("do a thing", "ready")), "misc");
    }

    #[test]
    fn slugs_are_branch_safe_and_bounded() {
        let s =
            slug_of("feat(worldgen): build the Goldberg dual and emit the lands_core::Topology");
        assert!(s.len() <= 40, "got {} chars: {s}", s.len());
        assert!(
            s.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        );
        assert!(!s.starts_with('-') && !s.ends_with('-'));
        assert!(s.starts_with("build-the-goldberg-dual"));
    }

    #[test]
    fn slugs_truncate_at_a_word_boundary() {
        // The old behaviour cut mid-word, giving `...-emit-the-lan`.
        let s =
            slug_of("feat(worldgen): build the Goldberg dual and emit the lands_core::Topology");
        assert!(
            !s.ends_with("-lan") && !s.ends_with("the-lan"),
            "slug was cut mid-word: {s}"
        );
        assert_eq!(s, "build-the-goldberg-dual-and-emit-the");
    }

    #[test]
    fn short_titles_are_left_alone() {
        assert_eq!(slug_of("chore: tidy up"), "tidy-up");
    }

    #[test]
    fn the_human_facing_footer_is_stripped() {
        let body = "## What\nDo the thing.\n\n---\nRead `AGENTS.md` before starting. Stay inside.\n\nDefinition of done: everything passes.";
        assert_eq!(without_footer(body), "## What\nDo the thing.");
    }

    #[test]
    fn a_body_without_a_footer_survives_intact() {
        assert_eq!(
            without_footer("## What\nDo the thing."),
            "## What\nDo the thing."
        );
    }

    #[test]
    fn maps_a_crate_label_to_its_directory() {
        assert_eq!(
            crate_dir(&issue("x", "crate:core")).as_deref(),
            Some("crates/lands-core")
        );
        assert_eq!(crate_dir(&issue("x", "tooling")), None);
    }

    #[test]
    fn reuses_the_titles_commit_type() {
        assert_eq!(verb_of("chore: replace the placeholders"), "chore");
        assert_eq!(verb_of("feat(ai): a brain"), "feat");
        assert_eq!(verb_of("something odd"), "feat");
    }
}
