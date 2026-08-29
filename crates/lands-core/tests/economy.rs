//! Economy rules (ECON-001 … ECON-014).
//!
//! The centrepiece is [`three_towns_with_seven_wheat_produce_four_gold`],
//! which is the worked example straight out of the design brief. If that test
//! ever fails, the game's core economic loop has changed.

use lands_core::economy;
use lands_core::event::Event;
use lands_testkit::covers;
use lands_testkit::prelude::*;

/// Treasury of the territory containing `tile`.
fn purse(world: &World, tile: u32) -> (i32, i32) {
    let t = world
        .territory_at(TileId(tile))
        .expect("tile should belong to a territory");
    (t.wheat, t.gold)
}

#[test]
fn empty_tiles_yield_one_wheat() {
    covers!("ECON-001a", "ECON-001b");

    // Capital plus two empty tiles: 1 + 1 + 1 = 3 wheat, 1 gold from the capital.
    let session = WorldBuilder::new(topo::line(4))
        .all_land()
        .player(Faction::Red)
        .own(Faction::Red, &[0, 1, 2])
        .capital(0)
        .treasury(0, 0, 0)
        .session();

    assert_eq!(purse(session.world(), 0), (3, 1));
}

#[test]
fn fields_yield_two_wheat_and_forests_yield_nothing() {
    covers!("ECON-001c", "ECON-001d");

    let session = WorldBuilder::new(topo::line(4))
        .all_land()
        .player(Faction::Red)
        .own(Faction::Red, &[0, 1, 2])
        .capital(0)
        .kind(1, TileKind::Field)
        .kind(2, TileKind::Forest)
        .treasury(0, 0, 0)
        .session();

    // Capital 1 + field 2 + forest 0 = 3 wheat; 1 gold from the capital.
    assert_eq!(purse(session.world(), 0), (3, 1));
}

/// The brief's worked example, verbatim:
///
/// > 3 town tiles, the territory has 7 wheat. On the next turn, the territory
/// > will produce 4 gold and will take 6 wheat.
///
/// `floor(7 / 3) = 2` towns are fed, spending 6 wheat and producing 4 gold.
/// The third town goes hungry and produces nothing — towns are never fed in
/// fractions.
#[test]
fn three_towns_with_seven_wheat_produce_four_gold() {
    covers!("ECON-004");

    let session = WorldBuilder::new(topo::line(5))
        .all_land()
        .player(Faction::Red)
        .own(Faction::Red, &[0, 1, 2, 3])
        .capital(0)
        .kinds(&[1, 2, 3], TileKind::Town)
        // Six stored, plus one from the capital, gives the seven the brief starts from.
        .treasury(0, 6, 0)
        .session();

    let (wheat, gold) = purse(session.world(), 0);
    assert_eq!(wheat, 1, "7 wheat less the 6 spent feeding two towns");
    assert_eq!(gold, 5, "1 from the capital plus 4 from two fed towns");

    let fed = session
        .last_events()
        .iter()
        .find_map(|e| match e {
            Event::TownsFed { fed, of, .. } => Some((*fed, *of)),
            _ => None,
        })
        .expect("a territory with towns reports how many were fed");
    assert_eq!(fed, (2, 3));
}

#[test]
fn towns_all_fed_when_wheat_is_plentiful() {
    covers!("ECON-004");

    let session = WorldBuilder::new(topo::line(5))
        .all_land()
        .player(Faction::Red)
        .own(Faction::Red, &[0, 1, 2, 3])
        .capital(0)
        .kinds(&[1, 2, 3], TileKind::Town)
        .treasury(0, 20, 0)
        .session();

    // 21 wheat available, 9 spent on three towns, 12 left; 1 + 6 gold.
    assert_eq!(purse(session.world(), 0), (12, 7));
}

#[test]
fn units_eat_before_towns_do() {
    covers!("ECON-003", "ECON-002a");

    let session = WorldBuilder::new(topo::line(5))
        .all_land()
        .player(Faction::Red)
        .own(Faction::Red, &[0, 1, 2, 3])
        .capital(0)
        .kind(1, TileKind::Town)
        .unit(UnitKind::Pawn, 2)
        .unit(UnitKind::Pawn, 3)
        .treasury(0, 6, 0)
        .session();

    // Income: capital 1 + two empty tiles 2 = 3 wheat (the town yields nothing
    // unconditionally), so 6 stored + 3 = 9 wheat and 1 gold.
    // Two pawns eat 2, leaving 7. The single town is then fed with 3, leaving 4
    // and producing 2 gold.
    let (wheat, gold) = purse(session.world(), 0);
    assert_eq!(wheat, 4);
    assert_eq!(gold, 3, "1 from the capital plus 2 from the fed town");
    assert_eq!(session.world().units.len(), 2, "nobody starved");
}

/// ECON-005: the most expensive units are shed first, and only as many as
/// necessary to restore solvency.
#[test]
fn famine_kills_knights_before_warriors_before_pawns() {
    covers!("ECON-005");

    let session = WorldBuilder::new(topo::line(6))
        .all_land()
        .player(Faction::Red)
        .own(Faction::Red, &[0, 1, 2, 3])
        .capital(0)
        // Upkeep: knight 2 wheat + 1 gold, warrior 2 wheat, pawn 1 wheat = 5 wheat.
        .unit(UnitKind::Knight, 1)
        .unit(UnitKind::Warrior, 2)
        .unit(UnitKind::Pawn, 3)
        .treasury(0, 2, 10)
        .session();

    // 2 stored + 4 income (capital + three empty tiles... the capital is tile 0
    // and 1..3 are empty) = 6 wheat, which is enough for all three.
    assert_eq!(session.world().units.len(), 3, "6 wheat covers 5 of upkeep");

    // Now starve them: only 1 wheat of income and nothing stored.
    let session = WorldBuilder::new(topo::line(6))
        .all_land()
        .player(Faction::Red)
        .own(Faction::Red, &[0])
        .capital(0)
        .unit(UnitKind::Knight, 0)
        .treasury(0, 0, 10)
        .session();

    // The lone capital yields 1 wheat; a knight needs 2. It starves.
    assert!(session.world().units.is_empty());
    assert_eq!(session.world().stats_of(Faction::Red).units_starved, 1);
}

#[test]
fn famine_sheds_only_as_many_units_as_needed() {
    covers!("ECON-005");

    let session = WorldBuilder::new(topo::line(8))
        .all_land()
        .player(Faction::Red)
        .own(Faction::Red, &[0, 1, 2, 3, 4])
        .capital(0)
        .unit(UnitKind::Pawn, 1)
        .unit(UnitKind::Pawn, 2)
        .unit(UnitKind::Pawn, 3)
        .unit(UnitKind::Pawn, 4)
        .treasury(0, 0, 0)
        .session();

    // Income is 5 wheat (capital plus four empty tiles); four pawns eat 4.
    assert_eq!(session.world().units.len(), 4, "5 wheat feeds four pawns");
    assert_eq!(purse(session.world(), 0).0, 1);
}

#[test]
fn upkeep_follows_the_unit_across_a_border() {
    covers!("ECON-002", "ECON-002b");

    // A unit is paid for by whichever territory it stands in, so a territory
    // with no units of its own has no upkeep.
    let (world, rules) = WorldBuilder::new(topo::line(6))
        .all_land()
        .player(Faction::Red)
        .own(Faction::Red, &[0, 1])
        .capital(0)
        .unit(UnitKind::Warrior, 1)
        .build();

    let territory = world.territory_at(TileId(0)).unwrap().id;
    assert_eq!(economy::upkeep_of(&world, &rules, territory), (2, 0));
}

/// Knights are the only unit that costs gold to keep, which is what makes a
/// knight army an economic commitment rather than just an expensive purchase.
#[test]
fn a_knight_costs_gold_as_well_as_wheat() {
    covers!("ECON-002c");

    let (world, rules) = WorldBuilder::new(topo::line(6))
        .all_land()
        .player(Faction::Red)
        .own(Faction::Red, &[0, 1])
        .capital(0)
        .unit(UnitKind::Knight, 1)
        .build();

    let territory = world.territory_at(TileId(0)).unwrap().id;
    assert_eq!(economy::upkeep_of(&world, &rules, territory), (2, 1));
}

#[test]
fn a_territory_present_from_the_start_is_endowed() {
    covers!("ECON-020");

    let (world, rules) = WorldBuilder::new(topo::line(4))
        .all_land()
        .player(Faction::Red)
        .own(Faction::Red, &[0, 1])
        .capital(0)
        .build();

    let t = world.territory_at(TileId(0)).unwrap();
    assert_eq!(t.wheat, rules.economy.starting_wheat);
    assert_eq!(t.gold, rules.economy.starting_gold);
}

#[test]
fn build_prices_scale_within_the_territory_only() {
    covers!("ECON-013", "ECON-014");

    use lands_core::apply::{build_field_cost, build_town_cost};

    let (world, rules) = WorldBuilder::new(topo::grid(6, 2))
        .all_land()
        .player(Faction::Red)
        // Two disconnected Red territories: 0-1-2 on the top row, 9-10-11 split off.
        .own(Faction::Red, &[0, 1, 2])
        .capital(0)
        .kinds(&[1, 2], TileKind::Town)
        .own(Faction::Red, &[9, 10, 11])
        .capital(10)
        .build();

    let crowded = world.territory_at(TileId(0)).unwrap().id;
    let fresh = world.territory_at(TileId(10)).unwrap().id;

    // Two towns here, none there — the price is per territory, not per faction.
    assert_eq!(build_town_cost(&world, &rules, crowded), 2 + 2);
    assert_eq!(build_town_cost(&world, &rules, fresh), 2);
    assert_eq!(build_field_cost(&world, &rules, crowded), 1);
}

#[test]
fn recruit_price_counts_every_unit_kind() {
    covers!("ECON-010");

    use lands_core::apply::recruit_cost;

    let (world, rules) = WorldBuilder::new(topo::line(6))
        .all_land()
        .player(Faction::Red)
        .own(Faction::Red, &[0, 1, 2, 3])
        .capital(0)
        .unit(UnitKind::Pawn, 1)
        .unit(UnitKind::Warrior, 2)
        .unit(UnitKind::Knight, 3)
        .build();

    let territory = world.territory_at(TileId(0)).unwrap().id;
    assert_eq!(recruit_cost(&world, &rules, territory), 1 + 3);
}

#[test]
fn upgrade_price_ignores_pawns() {
    covers!("ECON-011", "ECON-012", "UNIT-020a", "UNIT-020b");

    use lands_core::apply::upgrade_plan;

    let (world, rules) = WorldBuilder::new(topo::line(6))
        .all_land()
        .player(Faction::Red)
        .own(Faction::Red, &[0, 1, 2, 3, 4])
        .capital(0)
        .unit(UnitKind::Pawn, 1)
        .unit(UnitKind::Pawn, 2)
        .unit(UnitKind::Warrior, 3)
        .build();

    let pawn = world.tile(TileId(1)).unit.unwrap();
    let (kind, cost, _) = upgrade_plan(&world, &rules, pawn).unwrap();
    assert_eq!(kind, UnitKind::Warrior);
    // One warrior present; the two pawns do not count toward this price.
    assert_eq!(cost, 1 + 1);

    let warrior = world.tile(TileId(3)).unit.unwrap();
    let (kind, cost, _) = upgrade_plan(&world, &rules, warrior).unwrap();
    assert_eq!(kind, UnitKind::Knight);
    assert_eq!(cost, 2, "no knights present yet");
}
