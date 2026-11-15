//! Behaviour: none (experimental)
//!
//! The sway of the grass and the cut-out leaves in the moment's wind. Fixture: the camp's ferns and
//! the open land's grass by day, the frame's wind set to blow east and then north.

use auberean_remaster::Remaster;

use crate::common::{frame, records, settled, turn, STATIONS};

const W: u32 = 640;
const H: u32 = 480;
/// The wind, meters a second.
const WIND: f32 = 4.0;

/// The pixels two frames differ in by more than a level in a channel.
fn apart(a: &[u8], b: &[u8]) -> usize {
    a.chunks(4)
        .zip(b.chunks(4))
        .filter(|(x, y)| (0..3).any(|c| x[c].abs_diff(y[c]) > 1))
        .count()
}

/// **In the wind the grass and the leaves sway; with none, or with the sway off, they hold
/// still**: at the camp and the open land by day, with the sway on, the frame with the wind
/// blowing east and the one with it blowing north differ in at least one pixel in five hundred
/// (the plants lean another way); with no wind the frame is the one without the sway, and with the
/// sway off the two winds draw the same frame, as retail's plants hold still.
#[test]
fn in_the_wind_the_grass_and_the_leaves_sway_and_with_none_they_hold_still() {
    let _turn = turn();
    let records = records();
    let mut load = |id: u32| auberean_draw::picture::load(records, id);
    for name in ["camp-c156", "open-cc8b"] {
        let s = *STATIONS
            .iter()
            .find(|s| s.name == name)
            .expect("the station");
        let mut east = frame(&s);
        east.wind = [WIND, 0.0, 0.0];
        let mut north = east.clone();
        north.wind = [0.0, WIND, 0.0];
        let mut still = east.clone();
        still.wind = [0.0; 3];
        let mut r = Remaster::with(&dereth_gfx::DeviceDesc {
            debug: true,
            ..dereth_gfx::DeviceDesc::default()
        })
        .expect("the remaster's device");
        let mut draw = |r: &mut Remaster, f: &auberean_draw::scene::Frame| {
            settled(|| r.render(f, W, H, &mut load).expect("the frame").rgba)
        };
        let (held_east, n0) = draw(&mut r, &east);
        let (held_north, n1) = draw(&mut r, &north);
        r.set_sway(true);
        let (sway_east, n2) = draw(&mut r, &east);
        let (sway_north, n3) = draw(&mut r, &north);
        let (sway_still, n4) = draw(&mut r, &still);
        let noise = n0.max(n1).max(n2).max(n3).max(n4);
        let n = sway_east.len() / 4;
        let swayed = apart(&sway_east, &sway_north);
        eprintln!(
            "{name}: east and north apart in {swayed} of {n} pixels with the sway, {} without; \
             no wind {} from the frame without the sway (noise {noise})",
            apart(&held_east, &held_north),
            apart(&sway_still, &held_east)
        );
        assert!(
            swayed * 500 >= n,
            "{name}: the plants swayed in only {swayed} pixels"
        );
        assert!(
            apart(&held_east, &held_north) <= noise,
            "{name}: without the sway the wind moved the frame"
        );
        assert!(
            apart(&sway_still, &held_east) <= noise,
            "{name}: with no wind the sway moved the frame"
        );
    }
    let errors = dereth_gfx::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
}
