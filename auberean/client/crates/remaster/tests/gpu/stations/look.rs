//! Behaviour: none (experimental)
//!
//! The remaster's whole look at once, as a player who ticks the remaster sees it: every pass on
//! together draws every station with the validation layer quiet, and draws it near the frame
//! without the passes.
//!
//! `AUBEREAN_REMASTER_CAPTURE_DIR`, when set, names a folder each station's frame with the look is
//! written to.

use crate::common::{frame, records, turn, write_png, STATIONS};

const W: u32 = 640;
const H: u32 = 480;

/// The frames the look is drawn before it is looked at: three of the anti-aliasing's sequences.
const FRAMES: usize = 24;

/// **The whole look draws every station**: with every pass of the look on, after three of the
/// anti-aliasing's sequences, each station's land and what stands on it (every pixel but the
/// sky's) is within ten levels of the frame without the passes on average (measured on Windows:
/// 2.3 to 7.8; the passes change the light, not what is drawn), and the validation layer reports
/// nothing. The sky is left out: the physical sky turns the day's dome another color on purpose
/// (by 2.7 levels over the whole frame at the camp, whose frame is three fifths sky), and its own
/// test holds it (`world::sky`).
#[test]
fn the_whole_look_draws_every_station() {
    let _turn = turn();
    let capture = std::env::var_os("AUBEREAN_REMASTER_CAPTURE_DIR").map(std::path::PathBuf::from);
    let records = records();
    let mut load = |id: u32| auberean_draw::picture::load(records, id);
    let mut plain = auberean_remaster::Remaster::with(&dereth_gfx::DeviceDesc {
        debug: true,
        ..dereth_gfx::DeviceDesc::default()
    })
    .expect("the remaster's device");
    let mut looked = auberean_remaster::Remaster::with(&dereth_gfx::DeviceDesc {
        debug: true,
        ..dereth_gfx::DeviceDesc::default()
    })
    .expect("the remaster's device");
    looked.look();
    for s in &STATIONS {
        let f = frame(s);
        let off = plain.render(&f, W, H, &mut load).expect("the frame").rgba;
        let classes = plain
            .read_classes()
            .expect("the classes")
            .expect("a frame was drawn");
        let mut on = off.clone();
        for _ in 0..FRAMES {
            on = looked.render(&f, W, H, &mut load).expect("the frame").rgba;
        }
        // Every pixel but the sky's (class 0 none, 5 the sky's objects).
        let land: Vec<(&[u8], &[u8])> = off
            .chunks(4)
            .zip(on.chunks(4))
            .zip(&classes)
            .filter(|(_, c)| **c != 0 && **c != 5)
            .map(|(p, _)| p)
            .collect();
        #[allow(clippy::cast_precision_loss)] // pixel counts
        let mean = land
            .iter()
            .map(|(a, b)| f64::from((0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap_or(0)))
            .sum::<f64>()
            / land.len().max(1) as f64;
        eprintln!(
            "{}: the look {mean:.2} levels from the frame without it over {} pixels of land",
            s.name,
            land.len()
        );
        if let Some(dir) = &capture {
            write_png(&dir.join(format!("{}-look.png", s.name)), W, H, &on);
            write_png(&dir.join(format!("{}-plain.png", s.name)), W, H, &off);
        }
        assert!(
            mean < 10.0,
            "{}: the look moved the frame {mean:.2} levels",
            s.name
        );
    }
    let errors = dereth_gfx::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
}
