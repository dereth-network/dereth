//! Behaviour: none (experimental)
//!
//! Linear light, the shared post passes' tone curve, bloom and exposure over the remaster's frame:
//! below the curve's shoulder the frame is the one drawn in display values, above it bright colors
//! only roll off; the bloom lays a little of the frame's light round itself; a stop of exposure
//! brightens every pixel.
//!
//! `AUBEREAN_REMASTER_CAPTURE_DIR`, when set, names a folder each station's frames in display
//! values and in linear light (with the bloom, and a stop brighter) are written to.

use dereth_gfx_passes::post::PostSettings;

use crate::common::{frame, records, settled, turn, write_png, STATIONS};

const W: u32 = 640;
const H: u32 = 480;

/// The brightest display value the tone curve leaves as it is, in every channel: its shoulder,
/// 0.6 in linear light, is 203 of 255; a margin below it for the packed floats' steps.
const SHOULDER: u8 = 200;

/// The frame of `settings` at each station, drawn until it settles, and the device's noise.
fn frames(
    remaster: &mut auberean_remaster::Remaster,
    settings: PostSettings,
) -> Vec<(&'static str, Vec<u8>, usize)> {
    let records = records();
    let mut load = |id: u32| auberean_draw::picture::load(records, id);
    remaster.set_post(settings);
    STATIONS
        .iter()
        .map(|s| {
            let f = frame(s);
            let (picture, noise) = settled(|| {
                remaster
                    .render(&f, W, H, &mut load)
                    .expect("the frame")
                    .rgba
            });
            (s.name, picture, noise)
        })
        .collect()
}

fn device() -> auberean_remaster::Remaster {
    auberean_remaster::Remaster::with(&dereth_gfx::DeviceDesc {
        debug: true,
        ..dereth_gfx::DeviceDesc::default()
    })
    .expect("the remaster's device")
}

fn capture(name: &str, kind: &str, picture: &[u8]) {
    if let Some(dir) = std::env::var_os("AUBEREAN_REMASTER_CAPTURE_DIR") {
        let dir = std::path::PathBuf::from(dir);
        write_png(&dir.join(format!("{name}-{kind}.png")), W, H, picture);
    }
}

/// Linear light at no exposure, without the bloom.
const LINEAR: PostSettings = PostSettings {
    hdr: true,
    taa: false,
    bloom: 0.0,
    exposure: 0.0,
    frames: 8,
    check: false,
};

/// **Below the tone curve's shoulder, linear light draws the frame drawn in display values, or a
/// little brighter where it blends**: at every station, of the pixels whose display values are
/// below the shoulder in every channel, under one in two hundred is darker in linear light than in
/// display values by more than two levels (the packed floats' steps), and the frame is under three
/// levels from the frame drawn in display values on average. What moves is brighter: a blend mixed
/// in light rather than in display values (the leaves' edges, the reflection laid over the water)
/// comes out brighter than the same blend of display values.
#[test]
fn below_the_shoulder_linear_light_draws_the_frame_drawn_in_display_values() {
    let _turn = turn();
    let mut remaster = device();
    let shown = frames(&mut remaster, PostSettings::default());
    let linear = frames(&mut remaster, LINEAR);
    let mut faults = Vec::new();
    for ((name, a, _), (_, b, noise)) in shown.iter().zip(&linear) {
        let (mut below, mut sum, mut darker, mut brighter) = (0usize, 0u64, 0usize, 0usize);
        for (x, y) in a.chunks(4).zip(b.chunks(4)) {
            if !(0..3).all(|c| x[c] < SHOULDER) {
                continue;
            }
            below += 1;
            sum += u64::from((0..3).map(|c| x[c].abs_diff(y[c])).max().unwrap_or(0));
            if (0..3).any(|c| x[c] > y[c].saturating_add(2)) {
                darker += 1;
            }
            if (0..3).any(|c| y[c] > x[c].saturating_add(2)) {
                brighter += 1;
            }
        }
        #[allow(clippy::cast_precision_loss)] // pixel counts
        let mean = sum as f64 / below.max(1) as f64;
        eprintln!(
            "{name}: {below} pixels below the shoulder, {mean:.2} levels apart on average; \
             {darker} darker and {brighter} brighter by over 2 (noise {noise})"
        );
        capture(name, "display", a);
        capture(name, "linear", b);
        if below * 4 < a.len() / 4 {
            faults.push(format!("{name}: too little below the shoulder"));
        }
        if darker * 200 > below + noise * 200 {
            faults.push(format!("{name}: linear light darkened {darker} pixels"));
        }
        if mean >= 3.0 {
            faults.push(format!(
                "{name}: linear light moved the frame {mean:.2} levels"
            ));
        }
    }
    assert!(faults.is_empty(), "{faults:#?}");
    let errors = dereth_gfx::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
}

/// **The bloom lays a little of the frame's light round itself**: at every station, with the bloom
/// at its share, the frame is under two levels from the frame without it on average, and at least
/// one pixel in a thousand is more than two levels from it.
#[test]
fn the_bloom_lays_a_little_of_the_frames_light_round_itself() {
    let _turn = turn();
    let mut remaster = device();
    let plain = frames(&mut remaster, LINEAR);
    let bloomed = frames(
        &mut remaster,
        PostSettings {
            bloom: PostSettings::default().bloom,
            ..LINEAR
        },
    );
    for ((name, a, _), (_, b, _)) in plain.iter().zip(&bloomed) {
        let n = a.len() / 4;
        let d: Vec<u8> = a
            .chunks(4)
            .zip(b.chunks(4))
            .map(|(x, y)| (0..3).map(|c| x[c].abs_diff(y[c])).max().unwrap_or(0))
            .collect();
        #[allow(clippy::cast_precision_loss)] // pixel counts
        let mean = d.iter().map(|x| f64::from(*x)).sum::<f64>() / n as f64;
        let moved = d.iter().filter(|x| **x > 2).count();
        eprintln!(
            "{name}: the bloom moved the frame {mean:.2} levels on average, {moved} by over 2"
        );
        capture(name, "bloom", b);
        assert!(
            mean < 2.0,
            "{name}: the bloom moved the frame {mean:.2} levels"
        );
        assert!(moved * 1000 >= n, "{name}: the bloom did nothing");
    }
    let errors = dereth_gfx::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
}

/// **A stop of exposure brightens the frame**: at every station, a stop up, no pixel is darker
/// by more than a level in any channel, and the frame is brighter on average.
#[test]
fn a_stop_of_exposure_brightens_the_frame() {
    let _turn = turn();
    let mut remaster = device();
    let plain = frames(&mut remaster, LINEAR);
    let exposed = frames(
        &mut remaster,
        PostSettings {
            exposure: 1.0,
            ..LINEAR
        },
    );
    for ((name, a, _), (_, b, noise)) in plain.iter().zip(&exposed) {
        let darker = a
            .chunks(4)
            .zip(b.chunks(4))
            .filter(|(x, y)| (0..3).any(|c| x[c] > y[c].saturating_add(1)))
            .count();
        let sum = |p: &[u8]| {
            p.chunks(4)
                .flat_map(|x| &x[..3])
                .map(|x| u64::from(*x))
                .sum::<u64>()
        };
        #[allow(clippy::cast_precision_loss)] // pixel counts
        let gain = (sum(b) as f64 - sum(a) as f64) / (a.len() / 4 * 3) as f64;
        eprintln!("{name}: a stop up, {gain:.1} levels brighter on average, {darker} darker");
        capture(name, "exposed", b);
        assert!(
            darker <= *noise,
            "{name}: a stop up darkened {darker} pixels"
        );
        assert!(gain > 0.0, "{name}: a stop up did not brighten the frame");
    }
    let errors = dereth_gfx::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
}
