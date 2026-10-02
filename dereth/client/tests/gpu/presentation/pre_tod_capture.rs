//! The world of February 2005 (the `portal.dat` and `cell.dat` from before Throne of Destiny)
//! draws under today's screens: the client binary, with no window, draws one frame of Holtburg from
//! the older world set with the end-of-retail files beside it for the interface, and saves it.
//! The landscape is the older region's palette-shifted terrain, the buildings and scenery the
//! older art, and the screens the end-of-retail ones.
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`), the February 2005 dats
//! (`DERETH_TEST_PRETOD_DAT_DIR`) and a graphics device; a missing input fails.

use crate::common::client_dir;

use std::process::Command;

/// Behaviour: presentation.headless.the-world-before-throne-of-destiny-draws-under-todays-screens
#[test]
fn the_february_2005_holtburg_draws_under_todays_screens() {
    let dat_dir = client_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this test's interface and there are none at {} -- set \
         DERETH_TEST_DAT_DIR",
        dat_dir.display()
    );
    if let Some(msg) = dereth_dat::testing::pre_tod_shortfall() {
        panic!("{msg}");
    }
    let world = dereth_dat::testing::pre_tod_dat_dir().unwrap_or_default();

    let dir = std::env::temp_dir().join("dere-client-pre-tod-capture");
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    let out = dir.join("holtburg-2005.png");
    let _ = std::fs::remove_file(&out);
    let home = dir.join("home");
    std::fs::create_dir_all(&home).expect("a scratch home");
    let exe = env!("CARGO_BIN_EXE_dereth-client");
    let output = Command::new(exe)
        // Several frames: the landscape a later frame draws is the one a player sees.
        .args(["--headless", "--frames", "3", "--capture"])
        .arg(&out)
        .arg("--dat-dir")
        .arg(&dat_dir)
        .arg("--world-dat-dir")
        .arg(&world)
        .env("USERPROFILE", &home)
        .env("APPDATA", &home)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &home)
        .output()
        .expect("the client starts");
    let log = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "exit {:?}\n{log}",
        output.status.code()
    );
    assert!(!log.contains("WARN") && !log.contains("ERROR"), "{log}");

    // Holtburg's window of 17x17 blocks, every one meshed, with the palette-shift surfaces.
    let line = log
        .lines()
        .find(|l| l.contains("landblock 0xA9B4"))
        .unwrap_or_else(|| panic!("no scene line\n{log}"));
    assert!(line.contains("289 blocks"), "{line}");
    let surfaces: u32 = line
        .rsplit(", ")
        .next()
        .and_then(|s| s.split(' ').next())
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("{line}"));
    assert!(surfaces > 50, "{line}");

    // The frame is a picture: many distinct colours, not a cleared target.
    let decoder = png::Decoder::new(std::io::BufReader::new(
        std::fs::File::open(&out).expect("the capture was written"),
    ));
    let mut reader = decoder.read_info().expect("a PNG");
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("one frame");
    let bpp = info.color_type.samples();
    let colours: std::collections::BTreeSet<[u8; 3]> = buf[..info.buffer_size()]
        .chunks(bpp)
        .map(|p| [p[0], p[1], p[2]])
        .collect();
    assert!(colours.len() > 1000, "{} colours", colours.len());

    // The ground in front, either side of the body, is the textured landscape and not one
    // shaded colour: the bottom quarter's outer halves hold thousands of colours.
    let (w, h) = (info.width as usize, info.height as usize);
    let ground: std::collections::BTreeSet<[u8; 3]> = (h * 3 / 4..h)
        .flat_map(|y| {
            (0..w / 4)
                .chain(w * 3 / 4..w)
                .map(move |x| (y * w + x) * bpp)
        })
        .map(|i| [buf[i], buf[i + 1], buf[i + 2]])
        .collect();
    assert!(ground.len() > 2000, "{} ground colours", ground.len());
    let _ = std::fs::remove_dir_all(&dir);
}
