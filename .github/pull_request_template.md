<!--
  Keep this short. The gates check what can be checked mechanically; this is
  for the things a reviewer has to judge.
-->

Closes #

## What changed

<!-- One paragraph. What is different afterwards? -->

## Spec rules

<!--
  Rule ids this touches, e.g. ECON-004, TERR-030.
  If a rule was added or reworded, say which file in spec/rules/ changed.
-->

## Behaviour

- [ ] This does **not** change simulation behaviour — no golden replay hash moved.
- [ ] This **does** change behaviour. Hashes were re-recorded in a separate
      commit, and the scenarios that moved are listed below with a reason.

<!--
  If any hash moved, list them:
  - tests/replays/town-economy.ron — towns now round up, so two extra gold appear on round 3
-->

## Checks

- [ ] `cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `cargo xtask ci`
- [ ] New rules have a test declaring `covers!(...)`
- [ ] Any new randomness uses its own `SeedDomain`
- [ ] Commits are Conventional Commits and name no AI assistant
