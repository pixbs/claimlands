//! Source, commit and hook hygiene checks.
//!
//! * [`check_todos`] — every `TODO` must name an issue, so intent is never
//!   lost in a comment nobody is tracking.
//! * [`check_commits`] — the attribution ban, enforced over a commit range.
//! * [`check_hooks`] — every hook in `.githooks/` is executable, because git
//!   ignores one that is not and says so only in a hint.

use crate::{bullets, files_with_extension};
use std::path::Path;
use std::process::Command;

/// Tokens that must never appear anywhere the authorship of a change is
/// recorded: a commit message, body, trailer or author field, a branch name, or
/// a pull request body.
///
/// Enforced here rather than trusted to a prompt, because a prompt is a
/// request and this is a requirement. The local `commit-msg` hook catches it
/// first; this catches it again in CI, where `--no-verify` cannot help.
///
/// Shared with [`crate::pr`], so the name a commit may not carry is the same
/// name a branch and a pull request body may not carry. One list, or the three
/// drift apart and the ban has a hole in whichever one was forgotten.
pub(crate) const BANNED_ATTRIBUTION: &[&str] = &[
    "claude",
    "codex",
    "kiwi",
    "anthropic",
    "copilot",
    "chatgpt",
    "openai",
    "gpt-4",
    "gpt-5",
    "co-authored-by: ai",
    "generated with",
    "ai-generated",
];

/// The first banned token `haystack` names, if it names one.
///
/// Case-insensitive, because the ban is on the name and not on its spelling.
pub(crate) fn attribution_in(haystack: &str) -> Option<&'static str> {
    let lower = haystack.to_lowercase();
    BANNED_ATTRIBUTION
        .iter()
        .find(|banned| lower.contains(**banned))
        .copied()
}

/// Every `TODO` must carry an issue number: `// TODO(#123): ...`.
pub fn check_todos(root: &Path) -> Result<String, String> {
    let mut problems = Vec::new();
    let mut tracked = 0;

    for path in files_with_extension(root, &["rs", "toml", "ron", "wgsl"]) {
        // This file defines the rule, so its own examples are not violations.
        if path.ends_with("text.rs") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if !text.contains("TODO") && !text.contains("FIXME") {
            continue;
        }

        let display = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");

        for (n, line) in text.lines().enumerate() {
            for marker in ["TODO", "FIXME"] {
                let Some(index) = line.find(marker) else {
                    continue;
                };
                let rest = &line[index + marker.len()..];
                // Only `TODO(` or `TODO:` is a marker. Prose that merely uses
                // the word — as this file's own docs do — is not a violation.
                if !rest.starts_with('(') && !rest.starts_with(':') {
                    continue;
                }
                if is_issue_reference(rest) {
                    tracked += 1;
                } else {
                    problems.push(format!(
                        "{display}:{} — bare `{marker}`; write `{marker}(#123):` so the \
                         work is tracked",
                        n + 1
                    ));
                }
            }
        }
    }

    if problems.is_empty() {
        Ok(format!("{tracked} TODO(s), all referencing an issue"))
    } else {
        Err(bullets(problems))
    }
}

/// `(#123)` immediately after the marker.
fn is_issue_reference(rest: &str) -> bool {
    let Some(inner) = rest.strip_prefix('(') else {
        return false;
    };
    let Some(digits) = inner.strip_prefix('#') else {
        return false;
    };
    let count = digits.chars().take_while(char::is_ascii_digit).count();
    count > 0 && digits[count..].starts_with(')')
}

/// Verify no commit in `range` mentions an AI assistant.
///
/// Defaults to `origin/main..HEAD`, which is what a PR needs checking over.
pub fn check_commits(range: Option<&str>) -> Result<String, String> {
    let range = range.unwrap_or("origin/main..HEAD");

    let output = Command::new("git")
        .args(["log", "--format=%H%x00%an%x00%ae%x00%B%x01", range])
        .output()
        .map_err(|e| format!("could not run git: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "git log {range} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut problems = Vec::new();
    let mut checked = 0;

    for record in text.split('\u{1}').filter(|r| !r.trim().is_empty()) {
        let mut parts = record.trim_start().splitn(4, '\0');
        let hash = parts.next().unwrap_or_default();
        let author = parts.next().unwrap_or_default();
        let email = parts.next().unwrap_or_default();
        let body = parts.next().unwrap_or_default();
        checked += 1;

        let haystack = format!("{author}\n{email}\n{body}");
        if let Some(banned) = attribution_in(&haystack) {
            problems.push(format!(
                "{}: mentions `{banned}` — commit messages and authorship must not \
                 name an AI assistant (see CONTRIBUTING.md)",
                &hash[..hash.len().min(8)]
            ));
        }
    }

    if problems.is_empty() {
        Ok(format!("{checked} commit(s) in {range}, no AI attribution"))
    } else {
        Err(bullets(problems))
    }
}

/// Verify every hook in `.githooks/` is committed executable.
///
/// Git silently declines to run a hook without the executable bit — the only
/// sign is a `hint:` line that scrolls past — so the whole local half of the
/// attribution ban can be lost without a single check going red. The bit is
/// easy to drop: a rebase, a filesystem that does not carry it, an editor that
/// writes a new hook 644. This gate is what notices.
///
/// The index mode is what other clones get, so that is what is checked; a
/// working tree with `core.fileMode = false` would report the wrong thing.
pub fn check_hooks(root: &Path) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "--stage", "--", ".githooks"])
        .output()
        .map_err(|e| format!("could not run git: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "git ls-files failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let listing = String::from_utf8_lossy(&output.stdout);
    let (checked, problems) = hook_modes(&listing);

    if !problems.is_empty() {
        return Err(format!(
            "{}\n\nRestore the bit with `git update-index --chmod=+x <path>`.",
            bullets(problems)
        ));
    }
    if checked == 0 {
        return Err(
            "no hooks tracked under `.githooks/` — CONTRIBUTING.md tells every \
             contributor to point `core.hooksPath` at it"
                .to_string(),
        );
    }

    Ok(format!("{checked} hook(s) in .githooks/, all executable"))
}

/// Split `git ls-files --stage` output into a count and the entries whose mode
/// is not `100755`.
fn hook_modes(listing: &str) -> (usize, Vec<String>) {
    let mut checked = 0;
    let mut problems = Vec::new();

    for line in listing.lines().filter(|l| !l.trim().is_empty()) {
        // `<mode> <object> <stage>\t<path>`
        let Some((meta, path)) = line.split_once('\t') else {
            continue;
        };
        let Some(mode) = meta.split_whitespace().next() else {
            continue;
        };
        checked += 1;
        if mode != "100755" {
            problems.push(format!(
                "{path} is mode {mode}, not 100755 — git ignores a hook that is not \
                 executable, and only says so in a hint"
            ));
        }
    }

    (checked, problems)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_todo_that_names_an_issue() {
        assert!(is_issue_reference("(#123): wire this up"));
        assert!(is_issue_reference("(#7)"));
    }

    #[test]
    fn rejects_an_untracked_todo() {
        assert!(!is_issue_reference(": wire this up"));
        assert!(!is_issue_reference(" later"));
        assert!(!is_issue_reference("(123)"), "the # is required");
        assert!(!is_issue_reference("(#)"), "a number is required");
    }

    #[test]
    fn an_executable_hook_passes() {
        let listing = "100755 a5c40fb 0\t.githooks/commit-msg\n";
        let (checked, problems) = hook_modes(listing);
        assert_eq!(checked, 1);
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn a_hook_committed_644_is_reported_with_its_path() {
        let listing = "100644 a5c40fb 0\t.githooks/commit-msg\n\
                       100755 b1c2d3e 0\t.githooks/pre-commit\n";
        let (checked, problems) = hook_modes(listing);
        assert_eq!(checked, 2);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains(".githooks/commit-msg"), "{problems:?}");
        assert!(problems[0].contains("100644"), "{problems:?}");
    }

    #[test]
    fn an_empty_hooks_directory_counts_nothing() {
        assert_eq!(hook_modes("").0, 0);
    }

    #[test]
    fn this_repository_ships_its_hooks_executable() {
        let root = crate::repo_root();
        check_hooks(&root).expect("every file in .githooks/ must be mode 100755");
    }

    #[test]
    fn the_banned_list_is_lowercase_so_matching_works() {
        for banned in BANNED_ATTRIBUTION {
            assert_eq!(
                *banned,
                banned.to_lowercase(),
                "`{banned}` must be lowercase; the haystack is lowercased before matching"
            );
        }
    }
}
