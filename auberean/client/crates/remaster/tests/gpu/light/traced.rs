//! Behaviour: none (experimental)
//!
//! The sun's shadows and the ambient occlusion traced through the frame's acceleration structures,
//! where the device traces rays: like the cascades and the screen-space occlusion they stand in
//! for, they only take light away and they take some; the traced sun's share agrees with the
//! cascades' where both see the sun shut out; and a frame with nothing to trace (seen from inside)
//! draws them as before.
//!
//! On a device without ray queries (the Mac's) each test says so and passes: there the cascades
//! and the screen-space occlusion are the only ones.
//!
//! `AUBEREAN_REMASTER_CAPTURE_DIR`, when set, names a folder each station's frames and sun's
//! shares are written to.

use auberean_remaster::render::ShadowSettings;
use dereth_gfx_passes::ao::AoSettings;
use dereth_gfx_passes::traced::{RtAoSettings, RtShadowSettings};

use crate::common::{frame, records, settled, turn, write_png, STATIONS};

const W: u32 = 640;
const H: u32 = 480;
/// The frames the interior is drawn for, as the occlusion gathers over them.
const FRAMES: usize = 9;

fn remaster() -> Option<auberean_remaster::Remaster> {
    let remaster = auberean_remaster::Remaster::with(&dereth_gfx::DeviceDesc {
        debug: true,
        ..dereth_gfx::DeviceDesc::default()
    })
    .expect("the remaster's device");
    if remaster.traces_rays() {
        Some(remaster)
    } else {
        eprintln!("{}: no ray queries, nothing traced", remaster.adapter);
        None
    }
}

/// Pixels of `on` brighter than `off` by more than a level, and darker by more than `by`.
fn moved(off: &[u8], on: &[u8], by: u8) -> (usize, usize) {
    let (mut brighter, mut darker) = (0, 0);
    for (a, b) in off.chunks(4).zip(on.chunks(4)) {
        if (0..3).any(|c| b[c] > a[c].saturating_add(1)) {
            brighter += 1;
        }
        if (0..3).any(|c| a[c] > b[c].saturating_add(by)) {
            darker += 1;
        }
    }
    (brighter, darker)
}

/// **The traced sun's shadows only darken, darken something, and fall where the cascades' do**: at
/// the outdoor daytime stations, traced, no pixel is brighter than with no shadows by more than a
/// level (no more than the frame varies by, [`settled`]), at least one in a thousand is darker by
/// more than eight, of the pixels both see in the sun or out of it (the share under a quarter, or
/// over three quarters) at least nine in ten agree, and of those the cascades shut out of it at
/// least 95 in a hundred are traced at under a half. The frames were traced.
#[test]
fn the_traced_suns_shadows_only_darken_and_fall_where_the_cascades_do() {
    let _turn = turn();
    let Some(mut remaster) = remaster() else {
        return;
    };
    let capture = std::env::var_os("AUBEREAN_REMASTER_CAPTURE_DIR").map(std::path::PathBuf::from);
    let records = records();
    let mut load = |id: u32| auberean_draw::picture::load(records, id);
    for s in STATIONS
        .iter()
        .filter(|s| s.inside.is_none() && !s.midnight)
    {
        let f = frame(s);
        let mut draw = |remaster: &mut auberean_remaster::Remaster| {
            remaster
                .render(&f, W, H, &mut load)
                .expect("the frame")
                .rgba
        };
        remaster.set_rt_shadows(RtShadowSettings::default());
        remaster.set_shadows(ShadowSettings::default());
        let (off, off_noise) = settled(|| draw(&mut remaster));
        remaster.set_shadows(ShadowSettings {
            on: true,
            ..ShadowSettings::default()
        });
        let (cascades, _) = settled(|| draw(&mut remaster));
        let cascades_sun = remaster.read_sun().ok().flatten().unwrap_or_default();
        remaster.set_rt_shadows(RtShadowSettings {
            on: true,
            ..RtShadowSettings::default()
        });
        let before = remaster.traced_frames().0;
        let (traced, on_noise) = settled(|| draw(&mut remaster));
        let traced_sun = remaster.read_sun().ok().flatten().unwrap_or_default();
        let (frames, _, instances) = remaster.traced_frames();
        let noise = off_noise.max(on_noise);
        let (brighter, darker) = moved(&off, &traced, 8);
        // Of the pixels the cascades see out of the sun, how many the traced share sees out of
        // it too; and of those both see clearly, how many agree.
        let (mut both, mut agree, mut shut, mut shut_too) = (0usize, 0usize, 0usize, 0usize);
        for (&a, &b) in cascades_sun.iter().zip(&traced_sun) {
            let side = |x: u8| (x < 64).then_some(false).or((x > 191).then_some(true));
            if let (Some(x), Some(y)) = (side(a), side(b)) {
                both += 1;
                agree += usize::from(x == y);
            }
            if a < 64 {
                shut += 1;
                shut_too += usize::from(b < 128);
            }
        }
        let n = off.len() / 4;
        eprintln!(
            "{}: traced in {} frames over {instances} instances; {brighter} brighter, {darker} of \
             {n} darker by over 8, noise {noise}; the sun's shares agree on {agree} of {both}; \
             {shut_too} of the {shut} the cascades shut out are shut out traced",
            s.name,
            frames - before
        );
        if let Some(dir) = &capture {
            let grey = |v: &[u8]| -> Vec<u8> { v.iter().flat_map(|&x| [x, x, x, 255]).collect() };
            write_png(
                &dir.join(format!("{}-sun-cascades.png", s.name)),
                W,
                H,
                &grey(&cascades_sun),
            );
            write_png(
                &dir.join(format!("{}-sun-traced.png", s.name)),
                W,
                H,
                &grey(&traced_sun),
            );
            write_png(&dir.join(format!("{}-shadows-off.png", s.name)), W, H, &off);
            write_png(
                &dir.join(format!("{}-shadows-cascades.png", s.name)),
                W,
                H,
                &cascades,
            );
            write_png(
                &dir.join(format!("{}-shadows-traced.png", s.name)),
                W,
                H,
                &traced,
            );
        }
        assert!(
            frames > before && instances > 0,
            "{}: nothing was traced",
            s.name
        );
        assert!(
            brighter <= noise,
            "{}: the traced shadows brightened {brighter} pixels, the noise {noise}",
            s.name
        );
        assert!(
            darker * 1000 >= n,
            "{}: the traced shadows darkened almost nothing",
            s.name
        );
        assert!(
            agree * 10 >= both * 9,
            "{}: the traced and the cascades' shares agree on {agree} of {both}",
            s.name
        );
        assert!(
            shut_too * 100 >= shut * 95,
            "{}: of the {shut} pixels the cascades shut out of the sun, {shut_too} are traced so",
            s.name
        );
    }
    let errors = dereth_gfx::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
}

/// **The traced occlusion only darkens and darkens something**: at the outdoor stations, traced,
/// no pixel is brighter than with no occlusion by more than a level (beyond the frame's own
/// noise), and at least one in a thousand is darker by more than four. The frames were traced.
#[test]
fn the_traced_occlusion_only_darkens_and_darkens_what_stands_close() {
    let _turn = turn();
    let Some(mut remaster) = remaster() else {
        return;
    };
    let capture = std::env::var_os("AUBEREAN_REMASTER_CAPTURE_DIR").map(std::path::PathBuf::from);
    let records = records();
    let mut load = |id: u32| auberean_draw::picture::load(records, id);
    for s in STATIONS.iter().filter(|s| s.inside.is_none()) {
        let f = frame(s);
        let mut draw = |remaster: &mut auberean_remaster::Remaster| {
            remaster
                .render(&f, W, H, &mut load)
                .expect("the frame")
                .rgba
        };
        remaster.set_rt_ao(RtAoSettings::default());
        remaster.set_ao(AoSettings::default());
        let (off, off_noise) = settled(|| draw(&mut remaster));
        remaster.set_ao(AoSettings {
            on: true,
            ..AoSettings::default()
        });
        remaster.set_rt_ao(RtAoSettings {
            on: true,
            ..RtAoSettings::default()
        });
        let before = remaster.traced_frames().1;
        let (traced, on_noise) = settled(|| draw(&mut remaster));
        let (_, frames, instances) = remaster.traced_frames();
        let noise = off_noise.max(on_noise);
        let (brighter, darker) = moved(&off, &traced, 4);
        let n = off.len() / 4;
        eprintln!(
            "{}: traced in {} frames over {instances} instances; {brighter} brighter, {darker} of \
             {n} darker by over 4, noise {noise}",
            s.name,
            frames - before
        );
        if let Some(dir) = &capture {
            write_png(
                &dir.join(format!("{}-occlusion-off.png", s.name)),
                W,
                H,
                &off,
            );
            write_png(
                &dir.join(format!("{}-occlusion-traced.png", s.name)),
                W,
                H,
                &traced,
            );
        }
        assert!(
            frames > before && instances > 0,
            "{}: nothing was traced",
            s.name
        );
        assert!(
            brighter <= noise,
            "{}: the traced occlusion brightened {brighter} pixels, the noise {noise}",
            s.name
        );
        assert!(
            darker * 1000 >= n,
            "{}: the traced occlusion darkened almost nothing",
            s.name
        );
    }
    let errors = dereth_gfx::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
}

/// **Seen from inside, nothing is traced, and the screen-space occlusion draws as before**: at the
/// interior station, with the traced occlusion on, no frame is traced and the frame is the one a
/// remaster with it off draws, byte for byte.
#[test]
fn seen_from_inside_nothing_is_traced_and_the_frame_is_the_untraced_one() {
    let _turn = turn();
    let records = records();
    let mut load = |id: u32| auberean_draw::picture::load(records, id);
    let s = STATIONS
        .iter()
        .find(|s| s.inside.is_some())
        .expect("an interior station");
    let f = frame(s);
    // Two remasters, the occlusion traced on one, each drawing the same frames: the occlusion
    // gathers over the frames, so each is drawn as often.
    let mut drawn = Vec::new();
    for traced in [false, true] {
        let Some(mut remaster) = remaster() else {
            return;
        };
        remaster.set_ao(AoSettings {
            on: true,
            indoors: true,
            ..AoSettings::default()
        });
        remaster.set_rt_ao(RtAoSettings {
            on: traced,
            ..RtAoSettings::default()
        });
        let mut last = Vec::new();
        for _ in 0..FRAMES {
            last = remaster
                .render(&f, W, H, &mut load)
                .expect("the frame")
                .rgba;
        }
        assert_eq!(
            remaster.traced_frames(),
            (0, 0, 0),
            "{}: a frame was traced",
            s.name
        );
        drawn.push(last);
    }
    assert!(drawn[0] == drawn[1], "{}: the frame changed", s.name);
}
