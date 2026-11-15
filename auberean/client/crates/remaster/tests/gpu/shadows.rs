//! Behaviour: none (experimental)
//!
//! The sun's shadows, the shared passes' cascades over the frame's own casters: they only take
//! light away (no pixel is brighter with them than without), and they take some (what stands
//! between a pixel and the sun shuts its sunlight out).
//!
//! `AUBEREAN_REMASTER_CAPTURE_DIR`, when set, names a folder each station's frames without and
//! with the shadows are written to.

use auberean_remaster::render::ShadowSettings;

use crate::common::{frame, records, settled, turn, write_png, STATIONS};

const W: u32 = 640;
const H: u32 = 480;

/// **The sun's shadows only darken, and darken something**: at the outdoor daytime stations, with
/// them on, no pixel is brighter than with them off by more than a level in any channel (no more
/// pixels than the frame, without them or with them, varies by from drawing to drawing,
/// [`settled`]), and at least one in a thousand is darker by more than eight. Each brightened
/// pixel is named, with the sun's share there.
#[test]
fn the_suns_shadows_only_darken_and_darken_what_stands_out_of_the_sun() {
    let _turn = turn();
    let capture = std::env::var_os("AUBEREAN_REMASTER_CAPTURE_DIR").map(std::path::PathBuf::from);
    let records = records();
    let mut load = |id: u32| auberean_draw::picture::load(records, id);
    let mut remaster = auberean_remaster::Remaster::with(&dereth_gfx::DeviceDesc {
        debug: true,
        ..dereth_gfx::DeviceDesc::default()
    })
    .expect("the remaster's device");
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
        remaster.set_shadows(ShadowSettings::default());
        let (off, off_noise) = settled(|| draw(&mut remaster));
        remaster.set_shadows(ShadowSettings {
            on: true,
            ..ShadowSettings::default()
        });
        let (on, on_noise) = settled(|| draw(&mut remaster));
        let noise = off_noise.max(on_noise);
        let sun = remaster.read_sun().ok().flatten().unwrap_or_default();
        let (mut brighter, mut darker) = (0usize, 0usize);
        for (k, (a, b)) in off.chunks(4).zip(on.chunks(4)).enumerate() {
            if (0..3).any(|c| b[c] > a[c].saturating_add(1)) {
                brighter += 1;
                if brighter <= 8 {
                    eprintln!(
                        "{}: brighter at ({}, {}): off {:?}, on {:?}, sun {}",
                        s.name,
                        k % W as usize,
                        k / W as usize,
                        &a[..3],
                        &b[..3],
                        sun.get(k).copied().unwrap_or(255)
                    );
                }
            }
            if (0..3).any(|c| a[c] > b[c].saturating_add(8)) {
                darker += 1;
            }
        }
        let n = off.len() / 4;
        eprintln!(
            "{}: {brighter} brighter, {darker} of {n} darker by over 8, noise {noise}",
            s.name
        );
        if let Some(dir) = &capture {
            let rgba: Vec<u8> = sun.iter().flat_map(|&x| [x, x, x, 255]).collect();
            write_png(&dir.join(format!("{}-sun.png", s.name)), W, H, &rgba);
            write_png(&dir.join(format!("{}-shadows-off.png", s.name)), W, H, &off);
            write_png(&dir.join(format!("{}-shadows-on.png", s.name)), W, H, &on);
        }
        assert!(
            brighter <= noise,
            "{}: the shadows brightened {brighter} pixels, the noise {noise}",
            s.name
        );
        assert!(
            darker * 1000 >= n,
            "{}: the shadows darkened almost nothing",
            s.name
        );
    }
    let errors = dereth_gfx::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
}
