//! A one-frame headless capture is byte-identical across runs: the client binary is spawned with
//! `--headless --frames 1 --capture`, three times as separate processes, and the PNGs and the
//! scene-load log lines are compared byte for byte. The frame is the static scene over Holtburg
//! (49 landblocks of terrain with the scenery, buildings and static objects around it) under the
//! data-patch screen, seen from a fixed camera, so every dat record behind the scene has to land in
//! the same place to reproduce the bytes. No socket is opened: the invocation names no server.
//! Fixture: the retail dats and a graphics device; a missing install, or a device the client
//! cannot open, fails.

use crate::common::client_dir;

use std::path::Path;
use std::process::Command;

fn run_once(out: &Path, dat_dir: &Path, extra: &[&str]) -> Result<(Vec<u8>, String), String> {
    let exe = env!("CARGO_BIN_EXE_dereth-client");
    // The run's home folder is a scratch one beside the capture, so its crash log lands there and
    // not in the settings folder of whoever runs the test.
    let home = out.with_extension("home");
    std::fs::create_dir_all(&home).map_err(|e| format!("creating {}: {e}", home.display()))?;
    let output = Command::new(exe)
        .args(["--headless", "--frames", "1", "--capture"])
        .arg(out)
        .arg("--dat-dir")
        .arg(dat_dir)
        .args(extra)
        .env("USERPROFILE", &home)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &home)
        .output()
        .map_err(|e| format!("spawning {exe}: {e}"))?;
    let log = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        return Err(format!(
            "exit {:?}
{log}",
            output.status.code()
        ));
    }
    let bytes = std::fs::read(out).map_err(|e| format!("reading {}: {e}", out.display()))?;
    Ok((bytes, log))
}

/// Behaviour: presentation.headless.a-one-frame-capture-is-byte-identical-across-runs
#[test]
fn the_headless_capture_is_byte_identical_across_three_runs() {
    let dat_dir = client_dir();
    // A worktree without the retail dats must fail here, not pass having launched nothing.
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this test's oracle and there are none at {} -- \
         set DERETH_TEST_DAT_DIR",
        dat_dir.display()
    );

    let dir = std::env::temp_dir().join("dere-client-gate");
    std::fs::create_dir_all(&dir).expect("a scratch directory");

    let mut images: Vec<Vec<u8>> = Vec::new();
    let mut logs: Vec<String> = Vec::new();
    for i in 0..3 {
        let out = dir.join(format!("out{i}.png"));
        match run_once(&out, &dat_dir, &[]) {
            Ok((bytes, log)) => {
                images.push(bytes);
                logs.push(log);
            }
            // A machine whose graphics device the client cannot open fails here too: a test that
            // cannot run is a failure, never a pass.
            Err(e) => panic!("run {i} failed: {e}"),
        }
    }

    assert!(!images[0].is_empty(), "the capture is empty");
    assert_eq!(images[0], images[1], "run 0 and run 1 differ");
    assert_eq!(images[1], images[2], "run 1 and run 2 differ");

    // A PNG, and one that carries real pixels rather than a cleared frame: the eight-byte signature
    // then IHDR, and a file large enough that 800x600 of retail art actually got in.
    assert_eq!(&images[0][..8], b"\x89PNG\r\n\x1a\n");
    assert!(
        images[0].len() > 10_000,
        "{} bytes is not a drawn frame",
        images[0].len()
    );

    // ...and that it is the world, not a cleared frame that happens to compress the same way. The
    // startup line names what was loaded; the static-scene tests check the pixels.
    assert!(logs[0].contains("landblock 0xA9B4"), "{}", logs[0]);
    assert!(!logs[0].contains("0 blocks"), "{}", logs[0]);
    // Every line but the output path, which differs by construction.
    let scene_lines = |log: &String| -> Vec<String> {
        log.lines()
            .filter(|l| !l.contains("wrote"))
            .map(str::to_string)
            .collect()
    };
    assert_eq!(
        scene_lines(&logs[0]),
        scene_lines(&logs[1]),
        "the scene load is not deterministic"
    );
    assert_eq!(
        scene_lines(&logs[1]),
        scene_lines(&logs[2]),
        "the scene load is not deterministic"
    );
}

/// With `--no-world` the client falls back to a full-screen quad of the retail surface
/// `0x0600378B`; that frame stays reachable and is byte-identical across two runs.
#[test]
fn the_first_pixel_quad_is_still_reachable_and_still_byte_identical() {
    let dat_dir = client_dir();
    // A worktree without the retail dats must fail here, not pass having launched nothing.
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this test's oracle and there are none at {} -- \
         set DERETH_TEST_DAT_DIR",
        dat_dir.display()
    );
    let dir = std::env::temp_dir().join("dere-client-gate-o5");
    std::fs::create_dir_all(&dir).expect("a scratch directory");

    let mut images: Vec<Vec<u8>> = Vec::new();
    for i in 0..2 {
        let out = dir.join(format!("quad{i}.png"));
        match run_once(&out, &dat_dir, &["--no-world"]) {
            Ok((bytes, log)) => {
                assert!(log.contains("surface 0x0600378B"), "{log}");
                images.push(bytes);
            }
            Err(e) => panic!("run {i} failed: {e}"),
        }
    }
    assert_eq!(images[0], images[1]);
}
