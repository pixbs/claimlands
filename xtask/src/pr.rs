//! The pull-request body gate: a merge must close its issue.
//!
//! GitHub closes an issue on merge only when the pull request **body** contains
//! a closing keyword — `Closes #42`, `Fixes #42`, `Resolves #42`. A bare `#42`
//! is a link, not an instruction, and the issue stays open.
//!
//! That is easy to get wrong here, because the commit convention
//! (`feat(worldgen): build the dual (#2)`) looks like it should be enough, and
//! `gh pr create --fill` copies the commit message over the template that would
//! have carried the keyword. The result is silent: the pull request merges, CI
//! is green, and the issue is still open with nothing to say it was done.
//!
//! # Why the body arrives in the environment
//!
//! A pull request body is written by whoever opened it, which on a public
//! repository is anyone. Interpolating it into a workflow's `run:` script —
//! `cargo xtask check-pr-body "${{ github.event.pull_request.body }}"` — pastes
//! that text straight into a shell, and a body containing a backtick or `$( )`
//! executes as the runner. So the workflow puts it in `env:` instead, where it
//! is data rather than script, and this reads it from there.

use std::fmt::Write as _;

/// The keywords GitHub actually honours.
///
/// Deliberately exactly GitHub's list and no more. Accepting something GitHub
/// ignores — `Closes: #42` with a colon, say — would pass the gate and still
/// leave the issue open, which is worse than no gate at all: it would say the
/// problem was solved.
const KEYWORDS: [&str; 9] = [
    "close", "closes", "closed", "fix", "fixes", "fixed", "resolve", "resolves", "resolved",
];

/// The environment variable the workflow puts the body in.
const BODY_VAR: &str = "PR_BODY";

pub fn check() -> Result<String, String> {
    let body = std::env::var(BODY_VAR).map_err(|_| {
        format!(
            "{BODY_VAR} is not set.\n\n\
             This gate reads the pull request body from the environment rather \
             than from an argument, because a body is untrusted text and a \
             shell would run it. To try it by hand:\n\n    \
             PR_BODY='Closes #42' cargo xtask check-pr-body"
        )
    })?;

    judge(&body)
}

fn judge(body: &str) -> Result<String, String> {
    match closing_reference(body) {
        Some(issue) => Ok(format!("the body closes #{issue}")),
        None => Err(explain(body)),
    }
}

/// What went wrong, and what to type instead.
///
/// The three failures look identical from the outside — no keyword — and take
/// different fixes, so they are told apart here rather than in the author's
/// head.
fn explain(body: &str) -> String {
    let mut out = String::from(
        "this pull request body has no closing keyword, so merging it will \
         leave its issue open.\n\n",
    );

    if body.trim().is_empty() {
        out.push_str(
            "The body is empty. `gh pr create --fill` copies the commit message \
             and discards the template; write the body out instead.\n",
        );
    } else if unfilled_template(body) {
        out.push_str(
            "The template's `Closes #` line is still blank. Put the issue \
             number after it.\n",
        );
    } else if let Some(number) = bare_reference(body) {
        let _ = writeln!(
            out,
            "The body mentions #{number}, but a bare reference only links — it \
             does not close. Write `Closes #{number}`."
        );
    } else {
        out.push_str("Add a `Closes #<issue>` line, usually as the first line.\n");
    }

    let _ = write!(
        out,
        "\nGitHub honours: {}.\nSee `.github/pull_request_template.md`.",
        KEYWORDS.join(", ")
    );
    out
}

/// The template left as it ships: `Closes #` with nothing after it.
fn unfilled_template(body: &str) -> bool {
    body.lines()
        .any(|l| l.trim().eq_ignore_ascii_case("closes #"))
}

/// The first `#123` in the body, whatever precedes it.
fn bare_reference(body: &str) -> Option<u32> {
    let mut rest = body;
    while let Some(at) = rest.find('#') {
        rest = &rest[at + 1..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(n) = digits.parse() {
            return Some(n);
        }
    }
    None
}

/// The issue a closing keyword in `body` points at, if there is one.
fn closing_reference(body: &str) -> Option<u32> {
    // ASCII-lowercased so the scan is case-insensitive without moving any byte
    // offsets — `to_lowercase` can change a string's length on some characters,
    // and the offsets are used to index back into the same buffer.
    let lower = body.to_ascii_lowercase();
    let bytes = lower.as_bytes();

    for keyword in KEYWORDS {
        let mut from = 0;
        while let Some(at) = lower[from..].find(keyword) {
            let start = from + at;
            let end = start + keyword.len();
            from = end;

            // A whole word, so `disclosed #42` and `closest #42` do not count.
            let before_ok = start == 0 || !is_word_byte(bytes[start - 1]);
            let after_ok = bytes.get(end).is_none_or(|b| !is_word_byte(*b));
            if !before_ok || !after_ok {
                continue;
            }

            if let Some(issue) = issue_after(&lower[end..]) {
                return Some(issue);
            }
        }
    }
    None
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// The issue reference immediately following a keyword.
///
/// GitHub accepts `#42`, `owner/repo#42` and the full issue URL, and requires
/// whitespace between the keyword and the reference.
fn issue_after(rest: &str) -> Option<u32> {
    if !rest.starts_with([' ', '\t']) {
        return None;
    }
    let rest = rest.trim_start_matches([' ', '\t']);

    if let Some(url) = rest.strip_prefix("http") {
        // `.../issues/42`, however the host and owner are spelled.
        let at = url.find("/issues/")?;
        return leading_number(&url[at + "/issues/".len()..]);
    }

    let hash = rest.find('#')?;
    let (owner, tail) = rest.split_at(hash);
    // Anything between the keyword and the `#` must be an `owner/repo`, not
    // prose that happens to have a number further along the paragraph.
    if !owner.is_empty()
        && !owner
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c))
    {
        return None;
    }
    leading_number(&tail[1..])
}

fn leading_number(s: &str) -> Option<u32> {
    let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_every_keyword_github_honours() {
        for keyword in KEYWORDS {
            let body = format!("{keyword} #12\n\nSome description.");
            assert_eq!(
                closing_reference(&body),
                Some(12),
                "`{keyword} #12` should close"
            );
        }
    }

    #[test]
    fn is_case_insensitive_and_finds_the_keyword_anywhere() {
        assert_eq!(closing_reference("Closes #12"), Some(12));
        assert_eq!(closing_reference("CLOSES #12"), Some(12));
        assert_eq!(
            closing_reference("## What\n\nA thing.\n\nFixes #7"),
            Some(7)
        );
    }

    #[test]
    fn accepts_a_cross_repository_reference_and_a_url() {
        assert_eq!(closing_reference("Closes pixbs/claimlands#12"), Some(12));
        assert_eq!(
            closing_reference("Resolves https://github.com/pixbs/claimlands/issues/34"),
            Some(34)
        );
    }

    #[test]
    fn rejects_a_bare_reference() {
        // The exact failure this gate exists for: the commit convention writes
        // `(#2)`, which links and does not close.
        assert_eq!(
            closing_reference("feat(worldgen): build the dual (#2)"),
            None
        );
        assert_eq!(closing_reference("See #12 for context."), None);
    }

    #[test]
    fn rejects_the_template_left_unfilled() {
        let body = "Closes #\n\n## What changed\n\nA thing.";
        assert_eq!(closing_reference(body), None);
        assert!(unfilled_template(body));
    }

    #[test]
    fn rejects_a_keyword_that_is_part_of_a_longer_word() {
        assert_eq!(closing_reference("disclosed #12"), None);
        assert_eq!(closing_reference("closest #12"), None);
        assert_eq!(closing_reference("prefixes #12"), None);
        // ...but a keyword against punctuation is still a keyword.
        assert_eq!(closing_reference("(closes #12)"), Some(12));
    }

    #[test]
    fn requires_whitespace_between_the_keyword_and_the_issue() {
        // GitHub does not accept a colon here, so neither does this.
        assert_eq!(closing_reference("Closes: #12"), None);
        assert_eq!(closing_reference("Closes#12"), None);
    }

    #[test]
    fn does_not_reach_across_prose_to_find_a_number() {
        // Without the owner/repo guard, the `#12` two sentences later would
        // satisfy the `closes` at the start.
        assert_eq!(
            closing_reference("This closes the gap we left. See issue #12."),
            None
        );
    }

    #[test]
    fn an_empty_body_is_refused_with_the_fill_explanation() {
        let err = judge("").unwrap_err();
        assert!(err.contains("--fill"), "got: {err}");
    }

    #[test]
    fn an_unfilled_template_is_refused_with_its_own_explanation() {
        let err = judge("Closes #\n\n## What changed").unwrap_err();
        assert!(err.contains("still blank"), "got: {err}");
    }

    #[test]
    fn a_bare_reference_is_refused_with_the_number_it_should_have_closed() {
        let err = judge("feat(worldgen): build the dual (#2)").unwrap_err();
        assert!(err.contains("Closes #2"), "got: {err}");
    }

    #[test]
    fn a_good_body_reports_which_issue_it_closes() {
        assert_eq!(
            judge("Closes #40\n\nAll of it.").unwrap(),
            "the body closes #40"
        );
    }
}
