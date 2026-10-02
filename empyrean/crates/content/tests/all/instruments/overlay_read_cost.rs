//! Vectors: the configured world.pack and an empty SQLite overlay.
//! Measures read timings with and without an overlay.
//! Fixture: retail world content and an empty local overlay.

use crate::support::overlay_fixture::*;

#[cfg(feature = "real-content")]
#[test]
#[ignore = "instrument: measures overlay read timings; run explicitly by name"]
fn the_overlay_seam_costs_nothing_without_an_overlay() {
    let path = empyrean_common::test_paths::world_pack();
    let run = |db: &PackContent| {
        let wcids: Vec<u32> = db.get_all_weenie_class_names().keys().copied().collect();
        let t = std::time::Instant::now();
        let mut n = 0usize;
        for &wcid in &wcids {
            n += db
                .base()
                .get_weenie(wcid)
                .map_or(0, |w| w.weenie_properties_int.len());
        }
        for lb in 0..=u16::MAX {
            db.clear_cached_instances_by_landblock(lb);
            n += db.get_cached_instances_by_landblock(lb).len();
        }
        (t.elapsed().as_secs_f64(), n)
    };
    let plain = PackContent::new(Pack::open(&path).unwrap());
    let with = PackContent::new(Pack::open(&path).unwrap());
    let dir = tmp("seam-cost");
    let o = ContentOverlay::open(
        &dir.join("overlay.sqlite"),
        BaseInputs {
            sql: dir.join("none.sql"),
            patches: Vec::new(),
            era: empyrean_common::era::EraId::Eor,
        },
        with.base().pack().header(),
        default_now(),
    )
    .unwrap();
    with.base().attach_overlay(Arc::new(o)).unwrap();
    let mut best = (f64::MAX, f64::MAX);
    for _ in 0..3 {
        let (a, na) = run(&plain);
        let (b, nb) = run(&with);
        assert_eq!(na, nb);
        best = (best.0.min(a), best.1.min(b));
    }
    println!(
        "no overlay {:.3}s, empty overlay {:.3}s ({:+.1}%)",
        best.0,
        best.1,
        (best.1 / best.0 - 1.0) * 100.0
    );
}
