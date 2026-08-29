//! The file format: what round-trips, and what is refused.

use lands_core::prelude::{Faction, Terrain, TileKind};
use lands_levels::{Level, LevelError, LevelPlayer, PlayerKind, TileOverride};

/// A level that passes every check, as the starting point for tests that then
/// break exactly one thing.
fn valid() -> Level {
    Level {
        id: "campaign/03-the-narrows".to_owned(),
        freq: 8,
        seed: 194_837,
        players: vec![
            LevelPlayer {
                faction: Faction::Red,
                kind: PlayerKind::Human,
            },
            LevelPlayer {
                faction: Faction::Blue,
                kind: PlayerKind::Ai {
                    profile: "aggressive-2".to_owned(),
                },
            },
        ],
        overrides: vec![
            TileOverride {
                id: 214,
                terrain: Terrain::Land,
                kind: TileKind::Capital,
                owner: Some(Faction::Red),
            },
            TileOverride {
                id: 297,
                terrain: Terrain::Land,
                kind: TileKind::Forest,
                owner: None,
            },
            TileOverride {
                id: 401,
                terrain: Terrain::Land,
                kind: TileKind::Capital,
                owner: Some(Faction::Blue),
            },
            TileOverride {
                id: 402,
                terrain: Terrain::Water,
                kind: TileKind::Empty,
                owner: None,
            },
        ],
    }
}

#[test]
fn round_trip_is_the_identity() {
    let level = valid();
    let text = level.to_ron().expect("a valid level serialises");
    let parsed = Level::from_ron(&text).expect("what we just wrote must parse");
    assert_eq!(level, parsed);
}

#[test]
fn round_trip_is_stable_over_a_second_pass() {
    // Identity on the value is not quite enough: the text must settle too, or
    // a level would grow a spurious diff every time a tool rewrote it.
    let once = valid().to_ron().unwrap();
    let twice = Level::from_ron(&once).unwrap().to_ron().unwrap();
    assert_eq!(once, twice);
}

#[test]
fn the_written_form_names_its_structs() {
    // The whole argument for RON over a compact string is that a reviewer can
    // read the diff, and the names are most of what makes it readable.
    let text = valid().to_ron().unwrap();
    assert!(text.starts_with("Level("), "got: {text}");
    assert!(text.contains("Player("), "got: {text}");
    assert!(text.contains("Tile("), "got: {text}");
}

#[test]
fn the_documented_format_parses() {
    // Kept in step with docs/architecture.md section 5 by hand; if that example
    // changes shape, this is where it is noticed.
    let level = Level::from_ron(
        r#"Level(
            id: "campaign/03-the-narrows",
            freq: 8,
            seed: 194837,
            players: [
                Player(faction: Red,  kind: Human),
                Player(faction: Blue, kind: Ai(profile: "aggressive-2")),
            ],
            overrides: [
                Tile(id: 214, terrain: Land, kind: Capital, owner: Some(Red)),
                Tile(id: 297, terrain: Land, kind: Forest,  owner: None),
                Tile(id: 401, terrain: Land, kind: Capital, owner: Some(Blue)),
            ],
        )"#,
    )
    .expect("the documented example must load");

    assert_eq!(level.id, "campaign/03-the-narrows");
    assert_eq!(level.tile_count(), 642, "10n^2+2 at n=8");
    assert_eq!(
        level.player(Faction::Blue).map(|p| &p.kind),
        Some(&PlayerKind::Ai {
            profile: "aggressive-2".to_owned()
        })
    );
}

#[test]
fn malformed_ron_fails_with_the_parser_message() {
    let err = Level::from_ron("Level(id: \"x\", freq: ").unwrap_err();
    assert!(
        matches!(&err, LevelError::Parse(m) if !m.is_empty()),
        "got: {err}"
    );
}

// ---- validation ---------------------------------------------------------
//
// Every message below names the field it is complaining about, the way
// `lands_rules::validate` does.

#[test]
fn rejects_an_override_out_of_range() {
    let mut level = valid();
    level.freq = 2; // 42 tiles, so 214 is well past the end.
    level.overrides[0].id = 214;
    let err = level.validate().unwrap_err();

    assert_eq!(
        err,
        LevelError::OverrideOutOfRange {
            index: 0,
            tile: 214,
            freq: 2,
            count: 42,
        }
    );
    assert!(err.to_string().contains("overrides[0].id"), "got: {err}");
}

#[test]
fn rejects_the_last_tile_id_plus_one() {
    // The off-by-one that a range check gets wrong: id 641 is the last tile of
    // a frequency-8 planet, 642 is not a tile.
    let mut level = valid();
    level.overrides[0].id = 641;
    assert!(level.validate().is_ok());

    level.overrides[0].id = 642;
    assert!(matches!(
        level.validate(),
        Err(LevelError::OverrideOutOfRange { tile: 642, .. })
    ));
}

#[test]
fn rejects_a_faction_with_no_capital() {
    let mut level = valid();
    level.overrides.retain(|t| t.owner != Some(Faction::Blue));
    let err = level.validate().unwrap_err();

    assert_eq!(
        err,
        LevelError::FactionWithoutCapital {
            index: 1,
            faction: Faction::Blue,
        }
    );
    assert!(err.to_string().contains("players[1]"), "got: {err}");
}

#[test]
fn rejects_a_faction_that_owns_tiles_but_no_capital() {
    // Owning ground is not the same as having somewhere to start from.
    let mut level = valid();
    for tile in &mut level.overrides {
        if tile.owner == Some(Faction::Blue) {
            tile.kind = TileKind::Field;
        }
    }
    assert!(matches!(
        level.validate(),
        Err(LevelError::FactionWithoutCapital {
            faction: Faction::Blue,
            ..
        })
    ));
}

#[test]
fn rejects_fewer_than_two_players() {
    let mut level = valid();
    level.players.truncate(1);
    let err = level.validate().unwrap_err();

    assert_eq!(
        err,
        LevelError::PlayerCount {
            min: 2,
            max: 4,
            found: 1,
        }
    );
    assert!(err.to_string().contains("players"), "got: {err}");
}

#[test]
fn rejects_more_than_four_players() {
    let mut level = valid();
    level.players = Faction::ALL
        .iter()
        .chain(std::iter::once(&Faction::Red))
        .map(|&faction| LevelPlayer {
            faction,
            kind: PlayerKind::Human,
        })
        .collect();

    assert!(matches!(
        level.validate(),
        Err(LevelError::PlayerCount { found: 5, .. })
    ));
}

#[test]
fn rejects_the_same_faction_twice() {
    let mut level = valid();
    level.players[1].faction = Faction::Red;

    assert_eq!(
        level.validate(),
        Err(LevelError::DuplicateFaction {
            index: 1,
            first: 0,
            faction: Faction::Red,
        })
    );
}

#[test]
fn rejects_two_overrides_for_one_tile() {
    let mut level = valid();
    level.overrides[1].id = level.overrides[0].id;

    assert_eq!(
        level.validate(),
        Err(LevelError::DuplicateOverride {
            index: 1,
            first: 0,
            tile: 214,
        })
    );
}

#[test]
fn rejects_an_owner_who_is_not_playing() {
    let mut level = valid();
    level.overrides[1].owner = Some(Faction::Green);

    assert_eq!(
        level.validate(),
        Err(LevelError::UnknownOwner {
            index: 1,
            faction: Faction::Green,
        })
    );
}

#[test]
fn rejects_water_that_is_owned_or_built_on() {
    // The same statement `lands_core::invariants` makes about a live world,
    // made before the world exists.
    let mut level = valid();
    level.overrides[3].owner = Some(Faction::Red);
    assert!(matches!(
        level.validate(),
        Err(LevelError::WaterNotNeutral { index: 3, .. })
    ));

    let mut level = valid();
    level.overrides[3].kind = TileKind::Town;
    assert!(matches!(
        level.validate(),
        Err(LevelError::WaterNotNeutral { index: 3, .. })
    ));
}

#[test]
fn rejects_a_capital_that_belongs_to_nobody() {
    let mut level = valid();
    level.overrides[1].kind = TileKind::Capital;

    assert_eq!(
        level.validate(),
        Err(LevelError::UnownedCapital { index: 1 })
    );
}

#[test]
fn rejects_an_out_of_range_frequency() {
    for freq in [0, 13, 1_000] {
        let mut level = valid();
        level.freq = freq;
        assert!(
            matches!(level.validate(), Err(LevelError::Frequency { found, .. }) if found == freq),
            "freq {freq} should be refused"
        );
    }
}

#[test]
fn rejects_an_empty_id() {
    let mut level = valid();
    level.id = "  ".to_owned();
    assert_eq!(level.validate(), Err(LevelError::MissingId));
}

#[test]
fn rejects_an_ai_with_no_profile() {
    let mut level = valid();
    level.players[1].kind = PlayerKind::Ai {
        profile: String::new(),
    };
    assert_eq!(
        level.validate(),
        Err(LevelError::EmptyAiProfile { index: 1 })
    );
}

#[test]
fn loading_validates() {
    // `from_ron` is the only door most callers use, so it has to close on the
    // same things `validate` does rather than only on syntax.
    let mut broken = valid();
    broken.players.truncate(1);

    let err = Level::from_ron(&broken.to_ron().unwrap()).unwrap_err();
    assert!(matches!(err, LevelError::PlayerCount { found: 1, .. }));
}
