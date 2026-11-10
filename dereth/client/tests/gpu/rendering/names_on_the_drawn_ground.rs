//! Under the remaster, a body standing outdoors is drawn on the ground the remaster draws, which
//! lies off where physics puts it, and grass grows from that ground. What the interface lays
//! over the body goes with it: the name over it stands over the body as it is drawn, hidden
//! neither by the grass nor by the ground's relief, and the ring under the selection lies on
//! the drawn ground at its feet, showing over the grass.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware device: a person standing in
//! the grass at Holtburg where the drawn ground stands above retail's, the camera low in the
//! grass a few meters off, the remaster on as the player turns it on.
//!
//! Behaviour: none (experimental Horizon interface over the remaster)

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use dereth_horizon::art::Art;
use dereth_horizon::draw::{Anchor, Argb, DrawList, Rect, Spot};

use dereth_scene::world_scene::SceneWrites;

use crate::instruments::drawn_ground_names::{frame, open, PERSON};
use crate::instruments::nameplate_frames::{H, W};

const MAGENTA: Argb = 0xFFFF_00FF;
const GREEN: Argb = 0xFF00_FF00;

/// Whether the BGRA pixel `p` is `colour`, give or take rounding.
fn is(p: &[u8], colour: Argb) -> bool {
    let [b, g, r] = [colour & 0xFF, (colour >> 8) & 0xFF, (colour >> 16) & 0xFF];
    [p[0], p[1], p[2]]
        .iter()
        .zip([b, g, r])
        .all(|(&got, want)| u32::from(got).abs_diff(want) <= 2)
}

/// Whether the BGRA pixel `p` is mostly the ring's green: its picture is soft-edged and blended
/// over the world.
fn greenish(p: &[u8]) -> bool {
    p[1] > 160 && p[0] < 110 && p[2] < 110
}

/// How many pixels of `bgra` are the ring's green.
fn ring_pixels(bgra: &[u8]) -> usize {
    bgra.as_chunks::<4>()
        .0
        .iter()
        .filter(|p| greenish(p.as_slice()))
        .count()
}

/// How many pixels of `bgra` are `colour`, inside `rect` where one is given.
fn count(bgra: &[u8], colour: Argb, rect: Option<Rect>) -> usize {
    bgra.as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .filter(|(i, p)| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
            let (x, y) = ((*i as u32 % W) as f32 + 0.5, (*i as u32 / W) as f32 + 0.5);
            rect.is_none_or(|r| r.contains(x, y)) && is(p.as_slice(), colour)
        })
        .count()
}

/// The person's name, a flat rectangle standing on the point over his body, given that point's
/// depth, as the client follows it; and where it is.
fn name(scene: &dereth_scene::world_scene::WorldScene) -> (DrawList, Rect) {
    use dereth_scene::world_scene::SceneReads;
    let mut list = DrawList::default();
    let (x, y, _) = scene.target_top(PERSON, (W, H)).expect("on screen");
    let rect = Rect::new((x - 24.0).round(), (y - 16.0).round(), 48.0, 10.0);
    let from = list.mark();
    list.fill(rect, MAGENTA);
    list.anchor_in_world(PERSON, Anchor::Top, from, (x, y), (rect.x, rect.y));
    list.follow(|id, _| {
        let (x, y, depth) = scene.target_top(id, (W, H))?;
        Some(Spot { x, y, depth })
    });
    (list, rect)
}

/// A person in the grass on ground drawn above retail's: his name is drawn whole, every pixel
/// of it, and the ring under him shows as much as it does laid over everything, but for what
/// his own legs hide.
#[test]
fn a_name_over_a_body_on_raised_drawn_ground_is_whole_and_its_ring_shows() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let (mut r, mut shot) = open(&store, None);
    let origin = shot
        .scene
        .objects
        .get(&PERSON)
        .expect("the person stands")
        .frame
        .origin;
    let lift = shot.scene.remaster_shift_at(origin);
    assert!(
        lift > 0.1,
        "the drawn ground stands above retail's under him: {lift} m"
    );
    let art = Art::empty();
    let picture = dereth_horizon::ring::picture(&art);
    dereth_client_runtime::present::Presentation::overlay_upload(
        &mut r,
        dereth_horizon::ring::RING_TEXTURE,
        &picture,
    )
    .expect("the ring uploads");
    let marker = dereth_client_contract::overlay::GroundMarker {
        tint: GREEN,
        // Wide, so it reaches well into the grass round him.
        scale: 3.0,
        ..dereth_horizon::ring::marker(Some((PERSON, dereth_horizon::ui::game::Relation::Npc)), 0.0)
            .expect("a ring")
    };
    let (list, rect) = name(&shot.scene);
    let shown = frame(&mut r, &mut shot, &art, &list, Some((&marker, false)));
    let whole = frame(
        &mut r,
        &mut shot,
        &art,
        &DrawList::default(),
        Some((&marker, true)),
    );
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let area = (rect.w as usize) * (rect.h as usize);
    let named = count(&shown, MAGENTA, Some(rect));
    let (ring, all) = (ring_pixels(&shown), ring_pixels(&whole));
    println!(
        "drawn {lift:.3} m up; the name {named} of {area} pixels; the ring {ring} of {all} pixels"
    );
    assert_eq!(named, area, "the name is whole");
    assert!(all > 300, "the ring is on screen: {all} pixels");
    assert!(
        ring * 10 >= all * 7,
        "the ring shows over the grass: {ring} of {all} pixels"
    );
    shot.scene
        .draw
        .set_remaster(false, &store, &mut r.gpu)
        .expect("the remaster stops");
    shot.scene.release_textures(&mut r.gpu);
}
