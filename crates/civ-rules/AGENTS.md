# civ-rules — local rules

This crate describes balance; it never decides anything. If you find yourself
writing an `if`, the logic belongs in `civ-core`.

- Every field is annotated with the spec rule id it implements. Keep that up to
  date — it is how a reader connects a number to its meaning.
- Adding a field means adding it to `assets/rules/default.ron` **and** to
  `validate.rs`, so a malformed file fails at load with a precise message
  rather than ten turns into a match.
- Bump `RULESET_VERSION` when the *shape* of `Ruleset` changes, so old files
  fail loudly instead of deserialising into something surprising.
- Changing any default value changes `Ruleset::hash()`, which invalidates every
  golden replay. That is intended. Re-record them in a separate commit.
