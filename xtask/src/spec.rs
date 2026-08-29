//! Gate 12: spec coverage.
//!
//! Every rule documented in `spec/rules/` must have at least one test that
//! declares it with `covers!("ID")`, and every id a test declares must exist in
//! the spec. Checking both directions is what makes it impossible to ship a
//! rule that is documented but unimplemented, or implemented but undocumented.
//!
//! The practical payoff is vocabulary: an issue can say "implement TERR-011"
//! instead of "make capital relocation work", and the reviewer can check the
//! claim mechanically.
//!
//! A rule id looks like `ECON-004` or `UNIT-010a`. It is **defined** by a
//! heading (`### ECON-004 — ...`) or a table row naming it in bold
//! (`| **ECON-001a** | ... |`); mentions anywhere else are references.

use crate::{RuleSet, RuleSources, bullets, files_with_extension};
use std::path::Path;

pub fn check(root: &Path) -> Result<String, String> {
    let defined = defined_rules(&root.join("spec"))?;
    let covered = covered_rules(root)?;

    if defined.is_empty() {
        return Err("no rule ids found in spec/ — has the layout changed?".to_owned());
    }

    let mut problems = Vec::new();

    // An umbrella rule such as ECON-001 is the heading over ECON-001a..d; it
    // is satisfied when its sub-rules are tested, since it has no separate
    // substance of its own.
    let is_covered = |id: &String| {
        covered.contains_key(id)
            || covered
                .keys()
                .any(|c| c.len() == id.len() + 1 && c.starts_with(id.as_str()))
    };

    let uncovered: Vec<&String> = defined.keys().filter(|id| !is_covered(id)).collect();
    if !uncovered.is_empty() {
        problems.push(format!(
            "{} rule(s) are documented but have no test declaring `covers!(...)`:\n{}",
            uncovered.len(),
            bullets(
                uncovered
                    .iter()
                    .map(|id| format!("{id} (defined in {})", defined[*id].join(", ")))
            )
        ));
    }

    let phantom: Vec<&String> = covered
        .keys()
        .filter(|id| !defined.contains_key(*id))
        .collect();
    if !phantom.is_empty() {
        problems.push(format!(
            "{} test(s) cite a rule id that does not exist in spec/:\n{}",
            phantom.len(),
            bullets(
                phantom
                    .iter()
                    .map(|id| format!("{id} (cited in {})", covered[*id].join(", ")))
            )
        ));
    }

    if problems.is_empty() {
        Ok(format!(
            "{} rules defined, all covered by tests",
            defined.len()
        ))
    } else {
        Err(problems.join("\n\n"))
    }
}

/// Rule ids defined in the spec, mapped to the files defining them.
fn defined_rules(spec_dir: &Path) -> Result<RuleSources, String> {
    let mut out = RuleSources::new();

    for path in files_with_extension(spec_dir, &["md"]) {
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("could not read {}: {e}", path.display()))?;
        let file = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        for line in text.lines() {
            let trimmed = line.trim();

            // A heading at any level defines the rule it names first.
            if trimmed.starts_with('#') {
                let rest = trimmed.trim_start_matches('#').trim_start();
                if let Some(id) = leading_rule_id(rest) {
                    out.entry(id).or_default().push(file.clone());
                }
                continue;
            }

            // A table row defines every id it puts in bold.
            if trimmed.starts_with('|') {
                for id in bold_rule_ids(trimmed) {
                    out.entry(id).or_default().push(file.clone());
                }
            }
        }
    }

    for files in out.values_mut() {
        files.dedup();
    }
    Ok(out)
}

/// Rule ids cited by `covers!(...)` in test code, mapped to the files citing
/// them.
fn covered_rules(root: &Path) -> Result<RuleSources, String> {
    let mut out = RuleSources::new();

    for path in files_with_extension(root, &["rs"]) {
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("could not read {}: {e}", path.display()))?;
        if !text.contains("covers!") {
            continue;
        }
        let file = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");

        for id in covers_invocations(&text) {
            out.entry(id).or_default().push(file.clone());
        }
    }

    for files in out.values_mut() {
        files.dedup();
    }
    Ok(out)
}

/// Every id inside a `covers!(...)` invocation.
fn covers_invocations(text: &str) -> RuleSet {
    let mut out = RuleSet::new();
    let mut rest = text;

    while let Some(start) = rest.find("covers!") {
        rest = &rest[start + "covers!".len()..];
        let Some(open) = rest.find(['(', '[', '{']) else {
            break;
        };
        let Some(close) = rest.find([')', ']', '}']) else {
            break;
        };
        if close < open {
            continue;
        }
        for chunk in rest[open + 1..close].split(',') {
            let id = chunk.trim().trim_matches('"').trim();
            if is_rule_id(id) {
                out.insert(id.to_owned());
            }
        }
        rest = &rest[close..];
    }

    out
}

/// The rule id at the very start of a heading, if there is one.
fn leading_rule_id(heading: &str) -> Option<String> {
    let token = heading.split_whitespace().next()?;
    is_rule_id(token).then(|| token.to_owned())
}

/// Every `**ID**` in a table row.
fn bold_rule_ids(row: &str) -> RuleSet {
    let mut out = RuleSet::new();
    for part in row.split("**").skip(1).step_by(2) {
        let token = part.trim();
        if is_rule_id(token) {
            out.insert(token.to_owned());
        }
    }
    out
}

/// `ABCD-123` or `ABCD-123x`: three to five capitals, a dash, three digits,
/// and an optional lowercase sub-rule letter.
fn is_rule_id(token: &str) -> bool {
    let Some((prefix, suffix)) = token.split_once('-') else {
        return false;
    };
    if !(3..=5).contains(&prefix.len()) || !prefix.chars().all(|c| c.is_ascii_uppercase()) {
        return false;
    }
    let digits: String = suffix.chars().take_while(char::is_ascii_digit).collect();
    if digits.len() != 3 {
        return false;
    }
    let tail = &suffix[digits.len()..];
    tail.is_empty() || (tail.len() == 1 && tail.chars().all(|c| c.is_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_well_formed_rule_ids() {
        assert!(is_rule_id("ECON-004"));
        assert!(is_rule_id("UNIT-010a"));
        assert!(is_rule_id("TERR-030"));
        assert!(is_rule_id("VICT-002"));
    }

    #[test]
    fn rejects_things_that_merely_look_like_rule_ids() {
        assert!(!is_rule_id("ECON-4")); // too few digits
        assert!(!is_rule_id("Econ-004")); // not all caps
        assert!(!is_rule_id("EC-004")); // prefix too short
        assert!(!is_rule_id("ECON-004AB")); // bad suffix
        assert!(!is_rule_id("hello"));
        assert!(!is_rule_id("2026-01-01"));
    }

    #[test]
    fn a_heading_defines_the_rule_it_names() {
        assert_eq!(
            leading_rule_id("ECON-004 — Town production").as_deref(),
            Some("ECON-004")
        );
        assert_eq!(leading_rule_id("Income"), None);
    }

    #[test]
    fn a_table_row_defines_every_bold_id() {
        let row = "| **ECON-001a** | Empty | +1 | 0 |";
        let ids = bold_rule_ids(row);
        assert_eq!(ids.len(), 1);
        assert!(ids.contains("ECON-001a"));
    }

    #[test]
    fn prose_references_do_not_define_a_rule() {
        // A mention in body text must not count, or a typo would invent a rule.
        assert!(bold_rule_ids("see TERR-030 for the split").is_empty());
        assert_eq!(leading_rule_id("see TERR-030"), None);
    }

    #[test]
    fn finds_every_id_in_a_covers_invocation() {
        let src = r#"
            fn a() { covers!("ECON-004"); }
            fn b() { covers!("UNIT-010a", "UNIT-010b"); }
        "#;
        let ids = covers_invocations(src);
        assert_eq!(ids.len(), 3);
        assert!(ids.contains("ECON-004"));
        assert!(ids.contains("UNIT-010b"));
    }
}
