//! Source and commit hygiene checks.
//!
//! * [`check_todos`] — every `TODO` must name an issue, so intent is never
//!   lost in a comment nobody is tracking.
//! * [`check_commits`] — the attribution ban, enforced over a commit range.

use crate::{bullets, files_with_extension};
use std::path::Path;
use std::process::Command;

/// Tokens that must never appear in a commit message, body, trailer or author
/// field.
///
/// Enforced here rather than trusted to a prompt, because a prompt is a
/// request and this is a requirement. The local `commit-msg` hook catches it
/// first; this catches it again in CI, where `--no-verify` cannot help.
const BANNED_ATTRIBUTION: &[&str] = &[
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

        let haystack = format!("{author}\n{email}\n{body}").to_lowercase();
        for banned in BANNED_ATTRIBUTION {
            if haystack.contains(banned) {
                problems.push(format!(
                    "{}: mentions `{banned}` — commit messages and authorship must not \
                     name an AI assistant (see CONTRIBUTING.md)",
                    &hash[..hash.len().min(8)]
                ));
            }
        }
    }

    if problems.is_empty() {
        Ok(format!("{checked} commit(s) in {range}, no AI attribution"))
    } else {
        Err(bullets(problems))
    }
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
