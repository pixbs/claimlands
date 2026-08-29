# ADR 0006 — Balance is data; rules have ids

**Status:** accepted · 2026-08-29

## Context

Game balance changes constantly and often by people who are not reading the
code — the base cost of a town, how much of a sacked capital's gold is looted,
how likely a forest is to spread. If those numbers live as literals in
`civ-core`, every tuning pass is a code change with the regression risk of one.

Separately, issues and pull requests need a precise way to refer to a rule.
"Make territory splitting work" is not checkable; "implement TERR-030" is.

## Decision

Two conventions, enforced together.

**1. Every balance number lives in `assets/rules/default.ron`,** loaded into a
validated `Ruleset`. `Ruleset::hash()` fingerprints it, and every replay records
the hash it was made against.

**2. Every rule has a stable id** — `ECON-004`, `TERR-030`, `UNIT-010b` —
defined in `spec/rules/`. Tests declare which they cover:

```rust
#[test]
fn towns_are_fed_whole() {
    covers!("ECON-004");
    // ...
}
```

`cargo xtask spec-coverage` checks **both directions**: a documented rule with
no test fails, and a test citing a nonexistent rule fails.

## Why

- A balance change becomes a reviewable one-line data diff.
- Alternate rulesets (tutorial, hardcore, a future science-tree variant) come
  free, and tests can vary one number without touching code.
- Checking coverage in both directions makes it structurally impossible to ship
  a rule that is documented but unimplemented, or implemented but untested.
- The ids give agents and reviewers a shared vocabulary that can be checked
  mechanically rather than argued about.

## Consequences

- Adding a rule means editing three places: the spec, the ruleset, and a test.
  That friction is deliberate.
- Changing balance invalidates every golden replay at once. The workflow is to
  re-record them in a commit of their own, so the diff shows which scenarios
  moved.
- `spec/` becomes a real document that must be kept true. It is the source of
  truth for rules; GitHub Issues are the source of truth for work.
