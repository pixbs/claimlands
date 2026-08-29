# Golden replays

Gate 3, the primary regression net.

Each `.ron` file describes a starting world, a list of commands, and the state
hash the run must end on. The hash covers every simulation-visible field, so a
change to *any* rule moves it — including rules the author never intended to
touch. That is the point: it catches the regression nobody predicted.

## Running them

```bash
cargo test -p civ-core --test golden
```

or, with per-file output:

```bash
cargo run -p civ-cli -- golden verify
```

## Adding one

1. Write the scenario as a new `.ron` file, copying the shape of an existing
   one. Set `ruleset_hash` and `expected_state_hash` to `0`.
2. Run `cargo run -p civ-cli -- golden record tests/replays/your-file.ron`.
3. Read the resulting file and check the hash is being recorded against the
   behaviour you actually intended.
4. Commit it. Fill in `description` with a sentence about what it protects, and
   `rules` with the spec ids it exercises.

## When a hash legitimately changes

A deliberate balance or rule change invalidates every replay at once, which
looks alarming and is meant to. **Re-record them in a commit of its own**, so
the diff shows exactly which scenarios moved and a reviewer can check that the
list matches the intent.

A hash that changes in a commit that was not supposed to change behaviour is a
regression. Do not re-record it — find out what moved.
