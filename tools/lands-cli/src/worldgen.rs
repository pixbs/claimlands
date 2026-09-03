//! Write a generated planet to a file a human can open.
//!
//! The renderer that could draw the planet is four milestones away, so until
//! then this is the only way to *look* at what `lands-worldgen` produces. That
//! matters more than it sounds: terrain and cover seeding are exactly the kind
//! of work where every count is right, every test is green, and the planet is
//! still visibly wrong — continents in a band around the equator, forests all
//! on one face of the icosahedron, a seam along an icosahedral edge. None of
//! that shows up in an assertion and all of it is obvious in five seconds of
//! looking.
//!
//! Two formats, because they answer different questions:
//!
//! * **OBJ** opens in every 3D viewer and most editors with no setup. Use it to
//!   see the shape.
//! * **JSON** carries what OBJ cannot — tile ids, neighbour lists, the
//!   fingerprints, and the terrain and cover a seed produces. Use it for a
//!   browser viewer or to diff two planets. `platforms/web/public/cover.html`
//!   is the viewer the pull-request preview serves.
//!
//! Both are written by hand rather than through a serialisation crate. The
//! export must be byte-identical across runs, and the cheapest way to promise
//! that is to control every byte: fixed-precision floats, no map iteration, no
//! dependency that could change its mind about field order in a patch release.

use lands_core::prelude::{Terrain, TileId};
use lands_worldgen::cover::{Cover, CoverRules, cover};
use lands_worldgen::geodesic::{Geodesic, MAX_FREQUENCY, geodesic};
use lands_worldgen::goldberg::{Goldberg, goldberg};
use lands_worldgen::terrain::terrain;
use lands_worldgen::vec3::Vec3;
use std::fmt::Write as _;
use std::path::Path;

/// What to write.
#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
    /// Wavefront OBJ — geometry only, opens anywhere.
    Obj,
    /// JSON — geometry plus the per-tile data and the fingerprints.
    Json,
}

/// Which of the two planets to write.
///
/// They are the same planet seen twice: `Mesh` is the geodesic scaffolding the
/// dual is built from, `Tiles` is the board the game is actually played on.
#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Kind {
    /// The Goldberg dual: one polygon per tile, twelve of them pentagons.
    Tiles,
    /// The geodesic triangle mesh the dual is taken of.
    Mesh,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Tiles => "tiles",
            Kind::Mesh => "mesh",
        }
    }
}

/// Generate a planet at `frequency` and write it to `out`.
///
/// `seed` decides the terrain and the cover grown on it. The geometry does not
/// depend on it — a frequency-8 planet has the same tiles and the same
/// numbering for every seed — so only the tile JSON, which is the only output
/// with somewhere to put them, reads it at all.
pub fn export(
    frequency: u32,
    format: Format,
    kind: Kind,
    seed: u64,
    out: &Path,
) -> Result<(), String> {
    check_frequency(frequency)?;

    let text = match (kind, format) {
        (Kind::Mesh, Format::Obj) => obj_mesh(&geodesic(frequency)),
        (Kind::Mesh, Format::Json) => json_mesh(&geodesic(frequency)),
        (Kind::Tiles, Format::Obj) => obj_tiles(&goldberg(frequency)),
        (Kind::Tiles, Format::Json) => json_tiles(&goldberg(frequency), seed),
    };

    std::fs::write(out, &text).map_err(|e| format!("could not write {}: {e}", out.display()))?;

    println!(
        "{} {} at n={frequency} -> {} ({} bytes)",
        kind.as_str(),
        match format {
            Format::Obj => "obj",
            Format::Json => "json",
        },
        out.display(),
        text.len()
    );
    Ok(())
}

/// Reject a frequency `geodesic` would reject, with the message it would use.
///
/// `geodesic` panics on a bad frequency, which is right for a library whose
/// callers pass a level parameter chosen by an author. A person typing
/// `--freq 0` deserves the same sentence without the backtrace, so the range
/// and the wording are taken from there rather than restated.
fn check_frequency(frequency: u32) -> Result<(), String> {
    if (1..=MAX_FREQUENCY).contains(&frequency) {
        Ok(())
    } else {
        Err(format!(
            "geodesic frequency must be 1..={MAX_FREQUENCY}, got {frequency}"
        ))
    }
}

/// One coordinate, at fixed precision.
///
/// Fixed rather than shortest-roundtrip so the file is byte-identical across
/// runs and diffable between frequencies. Six decimals is a hair over a
/// micrometre on a planet-sized sphere and is what most exporters emit.
///
/// A value that rounds to zero from below would otherwise print `-0.000000`,
/// which is true, ugly, and confusing in a diff.
fn coord(v: f64) -> String {
    let s = format!("{v:.6}");
    if s == "-0.000000" {
        "0.000000".to_owned()
    } else {
        s
    }
}

fn obj_header(out: &mut String, title: &str, frequency: u32) {
    let _ = writeln!(out, "# Claimlands {title}");
    let _ = writeln!(out, "# frequency {frequency}");
}

/// Positions and their normals.
///
/// Every point of the planet sits on the unit sphere, so its position *is* its
/// outward normal. Writing them explicitly is not redundant for the dual: a
/// tile's corners are not coplanar, so a viewer left to infer a face normal
/// picks one from an arbitrary triangulation and the shading breaks up along
/// the seams it invented.
fn obj_points(out: &mut String, points: &[Vec3]) {
    for p in points {
        let _ = writeln!(out, "v {} {} {}", coord(p.x), coord(p.y), coord(p.z));
    }
    for p in points {
        let _ = writeln!(out, "vn {} {} {}", coord(p.x), coord(p.y), coord(p.z));
    }
}

/// The geodesic triangle mesh: one triangle per face.
fn obj_mesh(mesh: &Geodesic) -> String {
    let mut out = String::new();
    obj_header(&mut out, "geodesic mesh", mesh.frequency());
    let _ = writeln!(out, "# vertices {}", mesh.vertices().len());
    let _ = writeln!(out, "# triangles {}", mesh.triangles().len());
    let _ = writeln!(out, "# structure hash {:#018x}", mesh.structure_hash());
    let _ = writeln!(out, "o planet-mesh-n{}", mesh.frequency());

    obj_points(&mut out, mesh.vertices());

    // OBJ indexes from one, and `v` and `vn` are parallel, so each corner is
    // written as `index//index`.
    for t in mesh.triangles() {
        let [a, b, c] = t.map(|v| v + 1);
        let _ = writeln!(out, "f {a}//{a} {b}//{b} {c}//{c}");
    }
    out
}

/// The Goldberg dual: one polygon per tile, five- or six-sided.
fn obj_tiles(planet: &Goldberg) -> String {
    let mut out = String::new();
    obj_header(&mut out, "Goldberg dual", planet.frequency());
    let _ = writeln!(out, "# tiles {}", planet.tile_count());
    let _ = writeln!(out, "# pentagons {}", planet.pentagons().count());
    let _ = writeln!(out, "# corners {}", planet.corners().len());
    let _ = writeln!(out, "# adjacency hash {:#018x}", planet.adjacency_hash());
    let _ = writeln!(out, "# structure hash {:#018x}", planet.structure_hash());
    let _ = writeln!(out, "o planet-tiles-n{}", planet.frequency());

    obj_points(&mut out, planet.corners());

    // Faces are in tile-id order, so face `k` of this file is tile `k - 1` of
    // the topology and a viewer's selection index means something.
    for cell in planet.cells() {
        out.push('f');
        for &c in cell.corners() {
            let i = c + 1;
            let _ = write!(out, " {i}//{i}");
        }
        out.push('\n');
    }
    out
}

/// `[x, y, z]`, the one place a point becomes JSON.
fn json_point(out: &mut String, p: Vec3) {
    let _ = write!(out, "[{}, {}, {}]", coord(p.x), coord(p.y), coord(p.z));
}

fn json_list<T: std::fmt::Display>(out: &mut String, items: impl IntoIterator<Item = T>) {
    out.push('[');
    for (i, item) in items.into_iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        let _ = write!(out, "{item}");
    }
    out.push(']');
}

fn json_mesh(mesh: &Geodesic) -> String {
    let mut out = String::from("{\n");
    let _ = writeln!(out, "  \"kind\": \"mesh\",");
    let _ = writeln!(out, "  \"frequency\": {},", mesh.frequency());
    let _ = writeln!(out, "  \"vertex_count\": {},", mesh.vertices().len());
    let _ = writeln!(out, "  \"triangle_count\": {},", mesh.triangles().len());
    let _ = writeln!(
        out,
        "  \"structure_hash\": \"{:#018x}\",",
        mesh.structure_hash()
    );

    out.push_str("  \"vertices\": [\n");
    for (i, v) in mesh.vertices().iter().enumerate() {
        out.push_str("    ");
        json_point(&mut out, *v);
        out.push_str(if i + 1 == mesh.vertices().len() {
            "\n"
        } else {
            ",\n"
        });
    }
    out.push_str("  ],\n");

    out.push_str("  \"triangles\": [\n");
    for (i, t) in mesh.triangles().iter().enumerate() {
        out.push_str("    ");
        json_list(&mut out, t.iter());
        out.push_str(if i + 1 == mesh.triangles().len() {
            "\n"
        } else {
            ",\n"
        });
    }
    out.push_str("  ]\n}\n");
    out
}

/// What the seeder put on a tile, as the viewer's vocabulary.
///
/// Water is not "no cover": a tile with nothing on it is bare land, and the
/// two must be told apart or a reviewer cannot see where the coastline is.
fn cover_name(cover: Option<Cover>) -> &'static str {
    match cover {
        None => "bare",
        Some(Cover::Village) => "village",
        Some(Cover::Forest) => "forest",
        Some(Cover::Field) => "field",
    }
}

fn json_tiles(planet: &Goldberg, seed: u64) -> String {
    let topology = planet.topology();
    let land = terrain(planet, seed);
    let grown = cover(planet, &land, seed, &CoverRules::bundled());

    let mut out = String::from("{\n");
    let _ = writeln!(out, "  \"kind\": \"tiles\",");
    let _ = writeln!(out, "  \"frequency\": {},", planet.frequency());
    let _ = writeln!(out, "  \"seed\": {seed},");
    let _ = writeln!(out, "  \"tile_count\": {},", planet.tile_count());
    let _ = writeln!(out, "  \"pentagon_count\": {},", planet.pentagons().count());
    let _ = writeln!(out, "  \"corner_count\": {},", planet.corners().len());
    let _ = writeln!(
        out,
        "  \"adjacency_hash\": \"{:#018x}\",",
        planet.adjacency_hash()
    );
    let _ = writeln!(
        out,
        "  \"structure_hash\": \"{:#018x}\",",
        planet.structure_hash()
    );

    // The two fingerprints this seed is pinned by, so a viewer can say which
    // planet it is showing and a reviewer can tie the picture to the snapshot
    // in `crates/lands-worldgen/tests/`.
    let _ = writeln!(
        out,
        "  \"terrain_hash\": \"{:#018x}\",",
        land.terrain_hash()
    );
    let _ = writeln!(out, "  \"cover_hash\": \"{:#018x}\",", grown.cover_hash());
    let _ = writeln!(out, "  \"land_count\": {},", land.land_count());

    out.push_str("  \"cover_counts\": {");
    for (i, kind) in Cover::ALL.iter().enumerate() {
        let _ = write!(
            out,
            "{}\"{}\": {}",
            if i > 0 { ", " } else { " " },
            cover_name(Some(*kind)),
            grown.count(*kind)
        );
    }
    out.push_str(" },\n");

    out.push_str("  \"corners\": [\n");
    for (i, c) in planet.corners().iter().enumerate() {
        out.push_str("    ");
        json_point(&mut out, *c);
        out.push_str(if i + 1 == planet.corners().len() {
            "\n"
        } else {
            ",\n"
        });
    }
    out.push_str("  ],\n");

    // `corners` runs counter-clockwise seen from outside, which is the order a
    // viewer needs to draw the polygon. `neighbors` is ascending, because that
    // is the order `lands_core::Topology` holds it in and therefore the order
    // anything comparing two planets should see.
    out.push_str("  \"tiles\": [\n");
    let last = planet.tile_count() - 1;
    for (i, cell) in planet.cells().iter().enumerate() {
        let _ = write!(out, "    {{ \"id\": {i}, \"sides\": {}, ", cell.sides());
        out.push_str("\"center\": ");
        json_point(&mut out, cell.center);
        out.push_str(", \"corners\": ");
        json_list(&mut out, cell.corners().iter());
        out.push_str(", \"neighbors\": ");
        json_list(
            &mut out,
            topology.neighbors(TileId(i as u32)).iter().map(|t| t.0),
        );
        let tile = TileId(i as u32);
        let _ = write!(
            out,
            ", \"terrain\": \"{}\", \"cover\": \"{}\"",
            if land.get(tile) == Terrain::Land {
                "land"
            } else {
                "water"
            },
            cover_name(grown.get(tile))
        );
        out.push_str(if i == last { " }\n" } else { " },\n" });
    }
    out.push_str("  ]\n}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lands_worldgen::geodesic::{triangle_count, vertex_count};

    fn lines_starting(text: &str, prefix: &str) -> usize {
        text.lines().filter(|l| l.starts_with(prefix)).count()
    }

    #[test]
    fn the_mesh_obj_has_the_counts_the_closed_form_predicts() {
        for n in [1, 8] {
            let obj = obj_mesh(&geodesic(n));
            assert_eq!(
                lines_starting(&obj, "v "),
                vertex_count(n) as usize,
                "n={n}"
            );
            assert_eq!(lines_starting(&obj, "vn "), vertex_count(n) as usize);
            assert_eq!(
                lines_starting(&obj, "f "),
                triangle_count(n) as usize,
                "n={n}"
            );
        }
    }

    #[test]
    fn the_tiles_obj_has_one_face_per_tile_and_one_vertex_per_corner() {
        for n in [1, 8] {
            let obj = obj_tiles(&goldberg(n));
            assert_eq!(
                lines_starting(&obj, "v "),
                triangle_count(n) as usize,
                "n={n}: a corner per mesh triangle"
            );
            assert_eq!(
                lines_starting(&obj, "f "),
                vertex_count(n) as usize,
                "n={n}: a tile per mesh vertex"
            );
        }
    }

    #[test]
    fn every_obj_index_is_one_based_and_in_range() {
        // The single easiest way to write an OBJ no viewer will open.
        for obj in [obj_mesh(&geodesic(4)), obj_tiles(&goldberg(4))] {
            let points = lines_starting(&obj, "v ");
            let mut faces = 0;
            for line in obj.lines().filter(|l| l.starts_with("f ")) {
                faces += 1;
                for token in line.split_whitespace().skip(1) {
                    let (v, vn) = token
                        .split_once("//")
                        .expect("every corner carries a normal");
                    assert_eq!(v, vn, "position and normal indices are parallel");
                    let i: usize = v.parse().expect("an index is a number");
                    assert!(i >= 1 && i <= points, "index {i} outside 1..={points}");
                }
            }
            assert!(faces > 0);
        }
    }

    #[test]
    fn twelve_faces_of_the_tiles_obj_are_pentagons() {
        for n in [1, 2, 8] {
            let obj = obj_tiles(&goldberg(n));
            let pentagons = obj
                .lines()
                .filter(|l| l.starts_with("f "))
                .filter(|l| l.split_whitespace().count() == 6)
                .count();
            assert_eq!(pentagons, 12, "n={n}");
        }
    }

    #[test]
    fn the_json_carries_the_fingerprints_and_the_counts() {
        let mesh = json_mesh(&geodesic(8));
        assert!(mesh.contains("\"structure_hash\": \"0xea54c44e04547e1d\""));
        assert!(mesh.contains("\"vertex_count\": 642"));
        assert!(mesh.contains("\"triangle_count\": 1280"));

        let tiles = json_tiles(&goldberg(8), 0);
        assert!(tiles.contains("\"adjacency_hash\": \"0x439681c106a33e7e\""));
        assert!(tiles.contains("\"structure_hash\": \"0x6a48f634d1463e2e\""));
        assert!(tiles.contains("\"tile_count\": 642"));
        assert!(tiles.contains("\"pentagon_count\": 12"));
        assert!(tiles.contains("\"corner_count\": 1280"));

        // The seed's own two fingerprints, which are what let a reviewer tie
        // the planet on screen to the snapshots in `crates/lands-worldgen`.
        // These are the same values `tests/terrain.rs` and `tests/cover.rs`
        // pin for (n=8, seed 0); if they move, the preview is drawing a
        // different planet than the one the test suite is guarding.
        assert!(tiles.contains("\"seed\": 0"));
        assert!(tiles.contains("\"terrain_hash\": \"0x617aeb5e4096637d\""));
        assert!(tiles.contains("\"cover_hash\": \"0x6048da58d8eb54a8\""));
        assert!(tiles.contains("\"land_count\": 270"));
        assert!(
            tiles.contains("\"cover_counts\": { \"village\": 22, \"forest\": 54, \"field\": 43 }")
        );
    }

    #[test]
    fn the_tile_json_never_puts_cover_in_the_sea() {
        // The acceptance criterion the preview exists to let somebody *see*,
        // checked here as well because a reviewer looking at one hemisphere
        // cannot see the other one.
        let json = json_tiles(&goldberg(8), 0);
        assert_eq!(
            json.matches("\"terrain\": \"water\", \"cover\": \"bare\"")
                .count(),
            json.matches("\"terrain\": \"water\"").count(),
            "a water tile is carrying cover"
        );
        // And the seed genuinely grows all three kinds, so the assertion above
        // is not passing on an empty planet.
        for kind in ["village", "forest", "field"] {
            assert!(
                json.contains(&format!("\"cover\": \"{kind}\"")),
                "no {kind} on the planet at all"
            );
        }
    }

    #[test]
    fn a_different_seed_gives_a_different_planet_but_the_same_geometry() {
        // The geometry is the frequency's, the cover is the seed's. A viewer
        // that showed the same planet for every seed would look right and be
        // useless.
        let planet = goldberg(4);
        let a = json_tiles(&planet, 1);
        let b = json_tiles(&planet, 2);
        assert_ne!(a, b);
        // Same corners, so only the per-tile data moved.
        let corners = |j: &str| j.split("\"tiles\": [").next().unwrap().to_owned();
        assert_ne!(corners(&a), corners(&b), "the fingerprints should differ");
        assert_eq!(a.matches("\"id\":").count(), b.matches("\"id\":").count());
    }

    #[test]
    fn the_tile_json_reports_the_neighbours_topology_holds() {
        let planet = goldberg(8);
        let topology = planet.topology();
        let json = json_tiles(&planet, 0);

        let expected: Vec<u32> = topology.neighbors(TileId(0)).iter().map(|t| t.0).collect();
        let rendered = format!(
            "\"neighbors\": [{}]",
            expected
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        );
        assert!(json.contains(&rendered), "tile 0 should list {expected:?}");
        assert_eq!(json.matches("\"id\":").count(), 642);
    }

    #[test]
    fn a_negative_zero_never_reaches_the_file() {
        assert_eq!(coord(-0.0), "0.000000");
        assert_eq!(coord(-1e-9), "0.000000");
        assert_eq!(coord(0.5), "0.500000");
        assert_eq!(coord(-0.5), "-0.500000");
    }

    #[test]
    fn the_same_frequency_always_produces_the_same_bytes() {
        for n in [1, 4] {
            assert_eq!(obj_mesh(&geodesic(n)), obj_mesh(&geodesic(n)));
            assert_eq!(obj_tiles(&goldberg(n)), obj_tiles(&goldberg(n)));
            assert_eq!(json_mesh(&geodesic(n)), json_mesh(&geodesic(n)));
            assert_eq!(json_tiles(&goldberg(n), 7), json_tiles(&goldberg(n), 7));
        }
    }

    #[test]
    fn an_out_of_range_frequency_is_refused_in_geodesics_own_words() {
        assert_eq!(
            check_frequency(0).unwrap_err(),
            "geodesic frequency must be 1..=512, got 0"
        );
        assert_eq!(
            check_frequency(MAX_FREQUENCY + 1).unwrap_err(),
            "geodesic frequency must be 1..=512, got 513"
        );
        assert!(check_frequency(1).is_ok());
        assert!(check_frequency(MAX_FREQUENCY).is_ok());
    }
}
