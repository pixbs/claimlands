# lands-testkit — local rules

Shared test scaffolding. It may depend on anything; **shipped code may only
reach it from `[dev-dependencies]`**, and `cargo xtask check-deps` enforces
that. Dev tools (`lands-cli`, `level-editor`) are exempt because they never reach
a device.

- When a rule needs a new kind of situation to test, add a builder here rather
  than hand-rolling one in a test file. That is what keeps a thousand tests
  readable.
- `WorldBuilder` derives territories by running the real
  `territory::retopologize`, so a fixture can never describe a world the game
  itself could not produce. Keep it that way.
- `golden::save_hashes` patches only the two hash lines of a replay file, so
  authored comments survive re-recording. Do not replace it with a full
  re-serialisation.
