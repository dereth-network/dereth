use super::*;
use crate::particles::EmitterHost;

// A placed static whose setup names a default animation plays it. The butterfly that scenery
// generation grows west of Shoushi (block 0xD655, land cell 0x1B) is left out of the static bake
// and becomes a live object whose parts pose from its animation every frame, on the February 2005
// world and on the end-of-retail world alike; the rock-like static next to it, whose setup names
// no default animation and no default script, stays baked.

/// The butterfly: two wing parts and a 90-frame default animation.
const BUTTERFLY: DataId = DataId(0x0200_0493);
/// Its land cell.
const BUTTERFLY_CELL: CellId = CellId(0xD655_001B);
/// A static a few metres east whose setup names no default animation and no default script.
const STILL: DataId = DataId(0x0200_03E0);

/// The two worlds: the February 2005 files with the end-of-retail ones beside them, and the
/// end-of-retail files. A missing input fails.
fn worlds() -> Vec<(&'static str, Arc<RetailDatStore>)> {
    if let Some(msg) = dereth_dat::testing::classic_shortfall() {
        panic!("{msg}");
    }
    let classic = dereth_dat::testing::classic_dat_dir().unwrap_or_default();
    let older = RetailDatStore::open_classic_with_modern(&classic, &dereth_dat::testing::dat_dir())
        .unwrap_or_else(|e| panic!("the February 2005 world did not open: {e}"));
    vec![
        ("the February 2005 world", Arc::new(older)),
        (
            "the end-of-retail world",
            Arc::new(dereth_dat::testing::open_store_or_fail()),
        ),
    ]
}

/// Block 0xD655's bake list and live placements, as the bake takes them with the world's own look.
fn placements(store: &RetailDatStore) -> (Vec<land::BakeItem>, Vec<EmitterPlacement>) {
    block_placements(store, (0xD6, 0x55))
}

/// A block's bake list and live placements, as the bake takes them with the world's own look.
fn block_placements(
    store: &RetailDatStore,
    (bx, by): (i32, i32),
) -> (Vec<land::BakeItem>, Vec<EmitterPlacement>) {
    let region = load_region(store).expect("the region");
    let lb = read_landblock(store, bx, by).expect("the block");
    let table = dereth_terrain::land::mesh::height_table(&region);
    let mesh = dereth_terrain::land::mesh::generate_landblock_with_table(
        &lb,
        &region,
        &table,
        bx,
        by,
        1,
        dereth_terrain::land::mesh::Direction::InViewerBlock,
    );
    let content = land_content(store, &region, &lb, &mesh, bx, by, true);
    land::land_placements(
        store,
        &content.placed,
        content.lbi.as_ref().filter(|_| content.full_detail),
        &content.object_slots,
        &|_, _| false,
    )
}

/// Every part's placed frame, the pose a drawn frame takes.
fn pose(h: &EmitterHost) -> Vec<Frame> {
    h.driver.part_array.parts.iter().map(|p| p.pos).collect()
}

fn moved(a: &[Frame], b: &[Frame]) -> f32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| x.origin.sub(y.origin).mag2().sqrt())
        .fold(0.0, f32::max)
}

/// Behaviour: rendering.scenery.a-default-animation-plays-on-a-placed-static
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn the_butterfly_west_of_shoushi_is_a_live_object_whose_wings_move_from_frame_to_frame() {
    for (name, store) in worlds() {
        let (items, emitters) = placements(&store);
        let (slot, p) = emitters
            .iter()
            .enumerate()
            .find(|(_, p)| p.setup == BUTTERFLY && p.cell == BUTTERFLY_CELL)
            .unwrap_or_else(|| panic!("{name}: the butterfly in land cell 0xD655001B is not live"));
        assert!(
            p.animated,
            "{name}: the butterfly's setup names a default animation"
        );
        assert!(
            !items.iter().any(|it| it.0 == BUTTERFLY),
            "{name}: the butterfly is baked as well as live"
        );

        let assets: Arc<dyn dereth_animation::data::AnimAssets> =
            Arc::new(DatAnimAssets::new(Arc::clone(&store)));
        let mut host =
            EmitterHost::spawn(&assets, p, slot, (0xD6 as f32 * 192.0, 0x55 as f32 * 192.0))
                .unwrap_or_else(|| panic!("{name}: the butterfly's setup loads"));
        assert_eq!(pose(&host).len(), 2, "{name}: two wings");

        // The first tick starts the clock; nothing has had time to move.
        assert!(
            !host.animate(0.0),
            "{name}: the first tick advances nothing"
        );
        let start = pose(&host);
        // Half a second later the wings are elsewhere: at 30 frames a second that is fifteen
        // frames into the flight, which bobs the butterfly tens of centimetres.
        assert!(
            host.animate(0.5),
            "{name}: half a second advances the animation"
        );
        let half = pose(&host);
        assert!(
            moved(&start, &half) > 0.05,
            "{name}: the wings moved {} m in half a second",
            moved(&start, &half)
        );
        // And on: the animation loops rather than stopping at its last frame.
        let mut t = 0.5;
        let mut last = half.clone();
        let mut still = 0;
        for _ in 0..250 {
            t += 0.04;
            assert!(
                host.animate(t),
                "{name}: a 25th of a second advances it at {t}"
            );
            let now = pose(&host);
            if moved(&last, &now) < 1e-5 {
                still += 1;
            }
            last = now;
        }
        assert!(
            still < 10,
            "{name}: the wings stood still on {still} of 250 frames across ten seconds"
        );

        // A frame shorter than the physics quantum is carried into the next one, and a gap over
        // two seconds restarts the clock without moving anything.
        let before = pose(&host);
        assert!(
            !host.animate(t + 0.01),
            "{name}: a hundredth of a second is carried"
        );
        assert!(
            moved(&before, &pose(&host)) < 1e-6,
            "{name}: and moves nothing"
        );
        assert!(
            host.animate(t + 0.04),
            "{name}: the carried time is spent on the next frame"
        );
        // It steps when the world's physics would: a frame within the tick's tolerance of a whole
        // quantum (two frames of a 60 Hz display, read a little early) is a step.
        let t = t + 0.04;
        assert!(
            host.animate(t + 0.032),
            "{name}: a frame a millisecond and a third short of the quantum is a step, as it is \
             for the world"
        );
        let t = t + 0.032;
        let before = pose(&host);
        assert!(
            !host.animate(t + 3.0),
            "{name}: a three-second gap advances nothing"
        );
        assert!(
            moved(&before, &pose(&host)) < 1e-6,
            "{name}: and moves nothing"
        );
    }
}

/// Behaviour: rendering.scenery.a-placed-static-with-no-default-animation-stays-baked
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn a_static_with_no_default_animation_or_script_stays_in_the_bake_and_is_not_live() {
    for (name, store) in worlds() {
        let (items, emitters) = placements(&store);
        let baked = items
            .iter()
            .filter(|it| {
                it.0 == STILL
                    && (it.1.origin.x - 97.4).abs() < 0.1
                    && (it.1.origin.y - 63.8).abs() < 0.1
            })
            .count();
        assert_eq!(baked, 1, "{name}: the static at (97.4, 63.8) is baked once");
        assert!(
            !emitters.iter().any(|p| p.setup == STILL),
            "{name}: the static is not a live object"
        );
        // Everything not animated is still baked: only the animated placements left the bake.
        let animated = emitters.iter().filter(|p| p.animated).count();
        assert!(animated >= 1, "{name}: the block has an animated placement");
        assert!(
            items.len() > 20,
            "{name}: the block's bake list holds only {} placements",
            items.len()
        );
    }
}

/// A bat a kilometre east of Holtburg: the butterfly's setup again, grown by scenery generation in
/// land cell 0x0F of block 0xADB2. Its two wings are set out 3 m from the placement's centre, and
/// its animation's first frame sets it turning.
const BAT_CELL: CellId = CellId(0xADB2_000F);

/// The turn the bat's animation sets, in radians each time the animation steps, clockwise.
const BAT_TURN: f32 = 0.026_797_784;

/// Behaviour: rendering.scenery.an-animated-static-turns-by-the-spin-its-animation-sets
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn a_bat_east_of_holtburg_flies_round_its_placement_once_every_eight_seconds() {
    let store = Arc::new(dereth_dat::testing::open_store_or_fail());
    let (_, emitters) = block_placements(&store, (0xAD, 0xB2));
    let (slot, p) = emitters
        .iter()
        .enumerate()
        .find(|(_, p)| p.setup == BUTTERFLY && p.cell == BAT_CELL)
        .expect("the bat in land cell 0xADB2000F is live");
    assert!(p.animated, "the bat's setup names a default animation");
    let assets: Arc<dyn dereth_animation::data::AnimAssets> =
        Arc::new(DatAnimAssets::new(Arc::clone(&store)));
    let mut host = EmitterHost::spawn(&assets, p, slot, (0xAD as f32 * 192.0, 0xB2 as f32 * 192.0))
        .expect("the bat's setup loads");
    let centre = host.frame.origin;
    let mut stats = HostHookStats::default();
    let mut sounds = Vec::new();
    // One static tick as the scene runs it: the animation's step, then the hooks it raised.
    let mut tick = |host: &mut EmitterHost, t: f64| {
        host.animate(t);
        SceneDraw::drain_host_hooks(host, Vec3::ZERO, None, &mut sounds, &mut stats);
    };
    // Where the bat is from the placement's centre: its bearing, and its distance across the
    // ground.
    let bat = |host: &EmitterHost| {
        let d = host.driver.part_array.parts[0].pos.origin.sub(centre);
        #[allow(clippy::cast_possible_truncation)]
        let bearing = dereth_primitives::num::math::atan2(f64::from(d.y), f64::from(d.x)) as f32;
        (bearing, (d.x * d.x + d.y * d.y).sqrt())
    };
    // The first tick starts the clock.
    tick(&mut host, 0.0);
    let (mut last, _) = bat(&host);
    let mut turned = 0.0f32;
    // Eight seconds of the world's thirty steps a second.
    for i in 1..=240 {
        tick(&mut host, f64::from(i) / 30.0);
        let (bearing, across) = bat(&host);
        assert!(
            (across - 3.0).abs() < 0.01,
            "step {i}: the bat is {across} m from the centre, not on its 3 m circle"
        );
        let mut step = bearing - last;
        if step > std::f32::consts::PI {
            step -= std::f32::consts::TAU;
        } else if step < -std::f32::consts::PI {
            step += std::f32::consts::TAU;
        }
        assert!(step <= 1e-5, "step {i}: the bat turned back by {step}");
        turned += step;
        last = bearing;
    }
    // The first step plays the frame that sets the turn; each one after turns it. The wings are
    // placed in world coordinates over thirty kilometres out, held there to about 4 mm, which
    // sways the bearing read off them by a thousandth of a radian or so.
    assert!(
        (turned + 239.0 * BAT_TURN).abs() < 0.005,
        "the bat turned {turned} rad clockwise-negative in eight seconds, not {}",
        -239.0 * BAT_TURN
    );
    assert!(
        turned < -std::f32::consts::TAU,
        "a whole circle and more in eight seconds: {turned}"
    );
    assert!(stats.omega_hooks >= 1, "the animation set the turn");
}
