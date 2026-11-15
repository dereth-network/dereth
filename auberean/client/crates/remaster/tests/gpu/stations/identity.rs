//! Behaviour: none (experimental)
//!
//! At its identity, with no pass of its own over the world, the remaster draws what the retail
//! device draws: at each station, on a device of its own with the validation layer on, the frame's
//! pixels lie within a level or two of the retail device's, and the validation layer finds
//! nothing. The retail device draws with two samples to the pixel and the remaster with one, and
//! the remaster leaves out what the shared passes take the place of (the near terrain's shadow map,
//! the volumes of shadow) and what it does not draw yet (the shores' alpha, what lies under the
//! water seen past it, the lens flares), so the comparison is by levels, not bytes. Measured on
//! 2026-11-14: means 0.10 to 1.00 levels, 0.2 to 2.6% of the pixels over 16.
//!
//! `AUBEREAN_REMASTER_CAPTURE_DIR`, when set, names a folder each station's two frames and their
//! difference are written to.

use dereth_gfx::DeviceDesc;

use crate::common::{frame, records, turn, write_png, STATIONS};

const W: u32 = 640;
const H: u32 = 480;

/// How two frames differ: the mean difference in display levels over the three channels, and the
/// share of pixels whose largest channel differs by more than 16.
#[derive(Debug, Clone, Copy)]
struct Apart {
    mean: f64,
    over16: f64,
}

fn apart(a: &[u8], b: &[u8]) -> Apart {
    let n = a.len() / 4;
    let (mut sum, mut over) = (0u64, 0usize);
    for i in 0..n {
        let mut worst = 0;
        for c in 0..3 {
            let e = a[i * 4 + c].abs_diff(b[i * 4 + c]);
            sum += u64::from(e);
            worst = worst.max(e);
        }
        if worst > 16 {
            over += 1;
        }
    }
    #[allow(clippy::cast_precision_loss)] // pixel counts
    Apart {
        mean: sum as f64 / (3 * n) as f64,
        over16: over as f64 / n as f64,
    }
}

/// The difference of two frames as a picture: each channel's difference, four times brighter.
fn heat(a: &[u8], b: &[u8]) -> Vec<u8> {
    a.chunks(4)
        .zip(b.chunks(4))
        .flat_map(|(x, y)| {
            let d = |c: usize| x[c].abs_diff(y[c]).saturating_mul(4);
            [d(0), d(1), d(2), 255]
        })
        .collect()
}

/// **At its identity the remaster draws the retail frame at every station**: the mean difference
/// is under one and a half display levels, and under 4% of the pixels differ by more than 16 in
/// any channel; the validation layer finds nothing.
#[test]
fn at_its_identity_the_remaster_draws_the_retail_frame_at_every_station() {
    let _turn = turn();
    let capture = std::env::var_os("AUBEREAN_REMASTER_CAPTURE_DIR").map(std::path::PathBuf::from);
    let records = records();
    let mut load = |id: u32| auberean_draw::picture::load(records, id);
    let mut retail = auberean_gpu::Renderer::new().expect("a hardware device");
    let mut remaster = auberean_remaster::Remaster::with(&DeviceDesc {
        debug: true,
        ..DeviceDesc::default()
    })
    .expect("the remaster's device");
    let mut found = Vec::new();
    for s in &STATIONS {
        let f = frame(s);
        let a = retail
            .render(&f, W, H, &mut load)
            .expect("the retail frame");
        let b = remaster.render(&f, W, H, &mut load).unwrap_or_else(|e| {
            panic!(
                "{}: {e}; the validation layer: {:#?}",
                s.name,
                dereth_gfx::take_validation_errors()
            )
        });
        let d = apart(&a.rgba, &b.rgba);
        eprintln!(
            "{}: mean {:.2} levels, {:.1}% over 16",
            s.name,
            d.mean,
            d.over16 * 100.0
        );
        if let Some(dir) = &capture {
            write_png(&dir.join(format!("{}-retail.png", s.name)), W, H, &a.rgba);
            write_png(&dir.join(format!("{}-remaster.png", s.name)), W, H, &b.rgba);
            write_png(
                &dir.join(format!("{}-difference.png", s.name)),
                W,
                H,
                &heat(&a.rgba, &b.rgba),
            );
        }
        if d.mean >= 1.5 || d.over16 >= 0.04 {
            found.push(format!("{}: {d:?}", s.name));
        }
    }
    drop(remaster);
    let errors = dereth_gfx::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    assert!(found.is_empty(), "frames apart from retail: {found:#?}");
}
