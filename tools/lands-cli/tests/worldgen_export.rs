//! The export, run as the binary a person actually types.
//!
//! The unit tests in `worldgen.rs` check the text each writer produces. These
//! check the things only a real process can show: that the file lands on disk,
//! that a bad frequency exits non-zero instead of panicking, and that two runs
//! agree byte for byte — which is the claim that makes this export usable as a
//! diff between two versions of the generator.

use std::path::PathBuf;
use std::process::{Command, Output};

/// Somewhere to write, unique per test so a parallel run cannot collide.
fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("worldgen-export");
    std::fs::create_dir_all(&dir).expect("the target tmpdir is writable");
    dir.join(name)
}

fn export(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lands-cli"))
        .args(["worldgen", "export"])
        .args(args)
        .output()
        .expect("the cli binary was built for this test")
}

fn lines_starting(text: &str, prefix: &str) -> usize {
    text.lines().filter(|l| l.starts_with(prefix)).count()
}

#[test]
fn the_mesh_obj_has_the_counts_the_closed_form_predicts() {
    for (n, vertices, triangles) in [(1u32, 12, 20), (8, 642, 1280)] {
        let out = scratch(&format!("mesh-n{n}.obj"));
        let run = export(&[
            "--freq",
            &n.to_string(),
            "--kind",
            "mesh",
            "--format",
            "obj",
            "--out",
            out.to_str().unwrap(),
        ]);
        assert!(run.status.success(), "n={n}: {:?}", run);

        let obj = std::fs::read_to_string(&out).expect("the export wrote its file");
        assert_eq!(
            lines_starting(&obj, "v "),
            vertices,
            "n={n} vertex_count(n)"
        );
        assert_eq!(
            lines_starting(&obj, "f "),
            triangles,
            "n={n} triangle_count(n)"
        );
    }
}

#[test]
fn the_tiles_obj_has_one_face_per_tile() {
    // The dual turns each mesh vertex into a tile and each triangle into a
    // corner, so the two counts swap round.
    for (n, tiles, corners) in [(1u32, 12, 20), (8, 642, 1280)] {
        let out = scratch(&format!("tiles-n{n}.obj"));
        let run = export(&["--freq", &n.to_string(), "--out", out.to_str().unwrap()]);
        assert!(run.status.success(), "n={n}: {:?}", run);

        let obj = std::fs::read_to_string(&out).expect("the export wrote its file");
        assert_eq!(lines_starting(&obj, "f "), tiles, "n={n}: 10n^2+2 tiles");
        assert_eq!(lines_starting(&obj, "v "), corners, "n={n}: 20n^2 corners");
        assert!(obj.contains("# pentagons 12"));
    }
}

#[test]
fn the_json_export_carries_the_geometry_and_the_fingerprint() {
    let out = scratch("mesh-n8.json");
    let run = export(&[
        "--freq",
        "8",
        "--kind",
        "mesh",
        "--format",
        "json",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert!(run.status.success(), "{run:?}");

    let json = std::fs::read_to_string(&out).expect("the export wrote its file");
    assert!(json.contains("\"vertices\": ["));
    assert!(json.contains("\"triangles\": ["));
    assert!(json.contains("\"structure_hash\": \"0xea54c44e04547e1d\""));
    assert!(json.contains("\"vertex_count\": 642"));
    assert!(json.contains("\"triangle_count\": 1280"));
}

#[test]
fn two_exports_of_the_same_frequency_are_byte_identical() {
    // A deterministic generator whose export is not deterministic is useless
    // for the thing an export is for: telling whether the planet changed.
    for (kind, format, ext) in [
        ("tiles", "obj", "obj"),
        ("tiles", "json", "json"),
        ("mesh", "obj", "obj"),
        ("mesh", "json", "json"),
    ] {
        let mut written = Vec::new();
        for run in 0..2 {
            let out = scratch(&format!("repeat-{kind}-{run}.{ext}"));
            let status = export(&[
                "--freq",
                "6",
                "--kind",
                kind,
                "--format",
                format,
                "--out",
                out.to_str().unwrap(),
            ]);
            assert!(status.status.success(), "{kind}/{format}: {status:?}");
            written.push(std::fs::read(&out).expect("the export wrote its file"));
        }
        assert_eq!(
            written[0], written[1],
            "{kind}/{format} is not reproducible"
        );
        assert!(!written[0].is_empty());
    }
}

#[test]
fn an_out_of_range_frequency_exits_non_zero_with_a_readable_message() {
    for bad in ["0", "513"] {
        let out = scratch("never-written.obj");
        let _ = std::fs::remove_file(&out);

        let run = export(&["--freq", bad, "--out", out.to_str().unwrap()]);
        assert!(!run.status.success(), "--freq {bad} should fail");

        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(
            stderr.contains(&format!("geodesic frequency must be 1..=512, got {bad}")),
            "--freq {bad} said: {stderr}"
        );
        assert!(
            !out.exists(),
            "a refused frequency must not leave a half-written file behind"
        );
    }
}

#[test]
fn a_frequency_clap_itself_rejects_also_exits_non_zero() {
    // Guards the other half of the range check: clap refuses anything that is
    // not a `u32` before the command ever runs.
    let out = scratch("never-written-2.obj");
    let run = export(&["--freq", "-1", "--out", out.to_str().unwrap()]);
    assert!(!run.status.success());
}
