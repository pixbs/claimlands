//! Repository automation: the quality gates that plain `cargo` cannot express.
//!
//! Run as `cargo xtask <command>` (see `.cargo/config.toml` for the alias).
//!
//! * `check-deps` — gate 6. Nothing may depend back toward `lands-core`.
//! * `spec-coverage` — gate 12. Every documented rule has a test, and every
//!   test cites a rule that exists.
//! * `check-commits` — the attribution ban, enforced over a commit range.
//! * `check-pr-body` — a merge must close the issue it implements.
//! * `check-todos` — no bare `TODO`; every one must name an issue.
//! * `check-hooks` — every hook in `.githooks/` is committed executable.
//! * `ci` — all of the above.
//!
//! `check-commits` and `check-pr-body` are not part of `ci`: both describe a
//! pull request rather than a working tree, so there is nothing for them to
//! read on a developer's machine. CI runs them from `commit-hygiene.yml`.
//!
//! Dependency-free on purpose: this is the tool that guards the dependency
//! graph, and CI builds it cold on every PR.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod brief;
mod deps;
mod pr;
mod spec;
mod text;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("ci");

    let root = repo_root();
    let result = match command {
        "check-deps" => deps::check(&root),
        "spec-coverage" => spec::check(&root),
        "check-todos" => text::check_todos(&root),
        "check-hooks" => text::check_hooks(&root),
        "check-commits" => text::check_commits(args.get(1).map(String::as_str)),
        "check-pr-body" => pr::check(),
        "brief" => brief::emit(args.get(1).map(String::as_str)),
        "ci" => run_all(&root),
        "help" | "--help" | "-h" => {
            print_help();
            return ExitCode::SUCCESS;
        }
        other => Err(format!(
            "unknown command `{other}`\n\nRun `cargo xtask help` for the list."
        )),
    };

    match result {
        Ok(summary) => {
            println!("{summary}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("\x1b[31merror\x1b[0m: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run_all(root: &Path) -> Result<String, String> {
    let mut out = String::new();
    let mut failures = Vec::new();

    for (name, result) in [
        ("check-deps", deps::check(root)),
        ("spec-coverage", spec::check(root)),
        ("check-todos", text::check_todos(root)),
        ("check-hooks", text::check_hooks(root)),
    ] {
        match result {
            Ok(summary) => {
                let _ = writeln!(out, "  \x1b[32mok\x1b[0m   {name}: {summary}");
            }
            Err(message) => {
                let _ = writeln!(out, "  \x1b[31mFAIL\x1b[0m {name}");
                failures.push(format!("{name}:\n{message}"));
            }
        }
    }

    if failures.is_empty() {
        Ok(out)
    } else {
        Err(format!("{out}\n{}", failures.join("\n\n")))
    }
}

fn print_help() {
    println!(
        "\
cargo xtask <command>

  check-deps           Verify no crate depends back toward lands-core (gate 6)
  spec-coverage        Verify every spec rule has a test and vice versa (gate 12)
  check-todos          Verify every TODO names an issue
  check-hooks          Verify every hook in .githooks/ is executable
  check-commits [RANGE]  Verify no AI attribution in commit messages
  check-pr-body        Verify the pull request body closes an issue.
                       Reads the body from $PR_BODY, never an argument.
  ci                   Run every gate that reads the working tree. The two
                       above it describe a pull request instead, so they are
                       not included; CI runs them from commit-hygiene.yml

  brief <ISSUE>        Print a ready-to-paste prompt for an agent to work
                       that issue (needs the `gh` CLI, logged in)

  help                 Show this message"
    );
}

/// The workspace root, found by walking up from this crate.
fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .expect("xtask always lives one level below the workspace root")
        .to_path_buf()
}

/// Shared helper: every `Cargo.toml` belonging to a workspace member.
pub(crate) fn member_manifests(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for dir in ["crates", "tools", "platforms"] {
        let Ok(entries) = std::fs::read_dir(root.join(dir)) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path().join("Cargo.toml"))
            .filter(|p| p.is_file())
            .collect();
        paths.sort();
        out.extend(paths);
    }
    out
}

/// Shared helper: every file under `dir` with one of `extensions`.
pub(crate) fn files_with_extension(dir: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];

    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            // Never descend into build output or version control.
            if name == "target" || name == ".git" || name == "reference" {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .is_some_and(|e| extensions.contains(&&*e.to_string_lossy()))
            {
                out.push(path);
            }
        }
    }

    out.sort();
    out
}

/// Format a list of problems as an indented block.
pub(crate) fn bullets<T: std::fmt::Display>(items: impl IntoIterator<Item = T>) -> String {
    items
        .into_iter()
        .map(|i| format!("  - {i}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) type RuleSet = BTreeSet<String>;
pub(crate) type RuleSources = BTreeMap<String, Vec<String>>;
