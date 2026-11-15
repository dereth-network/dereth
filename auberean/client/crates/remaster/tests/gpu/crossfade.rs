//! Behaviour: none (experimental)
//!
//! The crossfade between an object's levels of detail. Fixture: the stations outdoors by day,
//! whose scenery stands at every distance.

use auberean_remaster::Remaster;

use crate::common::{frame, records, settled, turn, STATIONS};

const W: u32 = 640;
const H: u32 = 480;

/// **Objects at a change of their detail are drawn in both levels, each in part, and nothing else
/// moves**: at the daytime stations, with the crossfade on, some draws stand in the band about a
/// change of their object's level, and the frame differs from the one without the crossfade in
/// under one pixel in twenty-five (those objects alone); turned off again, it draws the frame it
/// drew before.
#[test]
fn objects_at_a_change_of_detail_are_drawn_in_both_levels_and_nothing_else_moves() {
    let _turn = turn();
    let records = records();
    let mut load = |id: u32| auberean_draw::picture::load(records, id);
    let mut faded = 0;
    for name in [
        "town-6fa9",
        "town-76b8",
        "camp-c156",
        "open-cc8b",
        "wide-6fa9",
    ] {
        let s = *STATIONS
            .iter()
            .find(|s| s.name == name)
            .expect("the station");
        let f = frame(&s);
        let mut r = Remaster::with(&dereth_gfx::DeviceDesc {
            debug: true,
            ..dereth_gfx::DeviceDesc::default()
        })
        .expect("the remaster's device");
        let mut draw = |r: &mut Remaster| r.render(&f, W, H, &mut load).expect("the frame").rgba;
        let (off, off_noise) = settled(|| draw(&mut r));
        r.set_crossfade(true);
        let on = draw(&mut r);
        let crossfaded = r.crossfade().1;
        r.set_crossfade(false);
        let (again, again_noise) = settled(|| draw(&mut r));
        let n = off.len() / 4;
        let apart = off
            .chunks(4)
            .zip(on.chunks(4))
            .filter(|(x, y)| (0..3).any(|c| x[c].abs_diff(y[c]) > 1))
            .count();
        eprintln!("{name}: {crossfaded} draws crossfaded; {apart} of {n} pixels moved");
        assert!(
            apart * 25 < n,
            "{name}: the crossfade moved {apart} of {n} pixels"
        );
        assert!(
            crate::common::repeat_noise(&off, &again) <= off_noise.max(again_noise),
            "{name}: turned off again, the frame isn't the one drawn before"
        );
        faded += crossfaded;
    }
    let errors = dereth_gfx::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    assert!(faded > 0, "no draw stood at a change of its level");
}
