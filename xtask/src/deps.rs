//! Gate 6: the dependency direction.
//!
//! The crate graph is the isolation guarantee that lets many agents work in
//! parallel. `cargo-deny` can ban third-party crates but cannot express "no
//! edge may point back toward `civ-core`", so that rule lives here.
//!
//! Each crate is assigned a layer. A crate may depend only on **strictly
//! lower** layers. If you find yourself wanting to raise a layer to make a
//! dependency legal, that is the signal to reconsider the design — or to write
//! an ADR arguing the layering itself is wrong.

use crate::{bullets, member_manifests};
use std::path::Path;

/// Layer of each workspace crate. Lower may not depend on higher.
///
/// Keep this table and `docs/architecture.md` §3 in step.
const LAYERS: &[(&str, u32)] = &[
    ("civ-rules", 0),    // pure data definitions
    ("civ-core", 1),     // the simulation
    ("civ-worldgen", 2), // planet generation
    ("civ-ai", 2),       // brains; emit commands, never mutate
    ("civ-levels", 2),   // level format
    ("civ-procgen", 3),  // CPU meshes and textures, no GPU types
    ("civ-render", 4),   // wgpu
    ("civ-app", 5),      // orchestration
    ("civ-ffi", 6),      // C ABI for the shells
    ("civ-desktop", 7),  // dev harness
    ("civ-cli", 7),      // headless tools
    ("level-editor", 7),
];

/// Test-only crates. They may depend on anything, but shipped code may only
/// reach them from `[dev-dependencies]`.
const TEST_ONLY: &[&str] = &["civ-testkit"];

/// Crates that never ship to a player's device. They may use test scaffolding
/// as an ordinary dependency, because there is no binary for it to bloat and
/// no runtime for it to affect.
const DEV_TOOLS: &[&str] = &["civ-cli", "level-editor", "xtask", "civ-desktop"];

pub fn check(root: &Path) -> Result<String, String> {
    let mut problems = Vec::new();
    let mut edges = 0;

    for manifest in member_manifests(root) {
        let text = std::fs::read_to_string(&manifest)
            .map_err(|e| format!("could not read {}: {e}", manifest.display()))?;

        let Some(name) = package_name(&text) else {
            problems.push(format!("{} has no [package] name", manifest.display()));
            continue;
        };

        for (dep, section) in workspace_deps(&text) {
            edges += 1;
            if let Some(problem) = judge(&name, &dep, section) {
                problems.push(problem);
            }
        }
    }

    if problems.is_empty() {
        Ok(format!(
            "{edges} internal dependency edges, all pointing outward"
        ))
    } else {
        Err(format!(
            "the crate graph has edges that break the layering \
             (see docs/architecture.md section 3):\n{}",
            bullets(problems)
        ))
    }
}

/// Which section of the manifest a dependency was declared in.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Normal,
    Dev,
    Build,
}

fn judge(from: &str, to: &str, section: Section) -> Option<String> {
    if TEST_ONLY.contains(&to) {
        // A dev-dependency on the testkit is exactly what it is for, including
        // civ-core's own tests depending on it — Cargo allows that cycle.
        // Dev tools never ship, so they may use it outright.
        if section == Section::Dev || DEV_TOOLS.contains(&from) {
            return None;
        }
        return Some(format!(
            "`{from}` depends on `{to}` outside [dev-dependencies]; \
             test scaffolding must never reach shipped code"
        ));
    }
    if TEST_ONLY.contains(&from) {
        return None; // The testkit may reach anywhere.
    }

    let from_layer = layer(from)?;
    let to_layer = layer(to)?;

    if to_layer >= from_layer {
        Some(format!(
            "`{from}` (layer {from_layer}) depends on `{to}` (layer {to_layer}); \
             dependencies must point strictly downward"
        ))
    } else {
        None
    }
}

fn layer(name: &str) -> Option<u32> {
    LAYERS.iter().find(|(n, _)| *n == name).map(|(_, l)| *l)
}

fn package_name(manifest: &str) -> Option<String> {
    let mut in_package = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if in_package && let Some(value) = line.strip_prefix("name") {
            return value
                .trim_start_matches(|c: char| c == '=' || c.is_whitespace())
                .trim_matches('"')
                .split('"')
                .next()
                .map(str::to_owned);
        }
    }
    None
}

/// Workspace-internal dependencies declared by a manifest, with their section.
///
/// A deliberately small TOML reader: it only needs to recognise
/// `civ-x = { ... }` and `civ-x.workspace = true` at the top level of a
/// dependency table, which is the only form this repo uses.
fn workspace_deps(manifest: &str) -> Vec<(String, Section)> {
    let mut out = Vec::new();
    let mut section = None;

    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('[') {
            section = match trimmed {
                "[dependencies]" => Some(Section::Normal),
                "[dev-dependencies]" => Some(Section::Dev),
                "[build-dependencies]" => Some(Section::Build),
                _ => None,
            };
            continue;
        }
        let Some(section) = section else { continue };

        let Some(key) = trimmed.split(['=', '.']).next() else {
            continue;
        };
        let key = key.trim();
        if key.starts_with("civ-") || key == "level-editor" || key == "xtask" {
            out.push((key.to_owned(), section));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_package_name() {
        let manifest = "[package]\nname = \"civ-core\"\nversion = \"0.1.0\"\n";
        assert_eq!(package_name(manifest).as_deref(), Some("civ-core"));
    }

    #[test]
    fn finds_dependencies_and_their_section() {
        let manifest = "\
[package]
name = \"civ-core\"

[dependencies]
civ-rules = { workspace = true }
serde = \"1\"

[dev-dependencies]
civ-testkit = { workspace = true }
";
        let deps = workspace_deps(manifest);
        assert_eq!(deps.len(), 2);
        assert_eq!(deps[0].0, "civ-rules");
        assert!(deps[0].1 == Section::Normal);
        assert_eq!(deps[1].0, "civ-testkit");
        assert!(deps[1].1 == Section::Dev);
    }

    #[test]
    fn rejects_an_upward_dependency() {
        // The exact mistake this gate exists to catch.
        assert!(judge("civ-core", "civ-render", Section::Normal).is_some());
        assert!(judge("civ-core", "civ-worldgen", Section::Normal).is_some());
    }

    #[test]
    fn accepts_a_downward_dependency() {
        assert!(judge("civ-render", "civ-procgen", Section::Normal).is_none());
        assert!(judge("civ-core", "civ-rules", Section::Normal).is_none());
    }

    #[test]
    fn rejects_a_sideways_dependency() {
        // Same layer: civ-ai and civ-worldgen must not know about each other.
        assert!(judge("civ-ai", "civ-worldgen", Section::Normal).is_some());
    }

    #[test]
    fn shipped_code_reaches_the_testkit_only_through_dev_dependencies() {
        assert!(judge("civ-core", "civ-testkit", Section::Dev).is_none());
        assert!(judge("civ-core", "civ-testkit", Section::Normal).is_some());
        assert!(
            judge("civ-app", "civ-testkit", Section::Normal).is_some(),
            "the app ships, so it must not carry test scaffolding"
        );
    }

    #[test]
    fn dev_tools_may_use_the_testkit_outright() {
        assert!(judge("civ-cli", "civ-testkit", Section::Normal).is_none());
    }
}
