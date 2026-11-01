//! How a square blends the grounds and roads of its four corners into layers.

use auberean_primitives::DataId;
use auberean_world::terrain::{square_layers, Corner, Layer, Mask, MaskKind, SurfacePick};

fn ground(id: u32) -> SurfacePick {
    SurfacePick {
        base: DataId(id),
        ..SurfacePick::NONE
    }
}

/// One layer: its ground's corner, and its mask's owner, kind and rotation.
type Picked = (Corner, Option<(Corner, MaskKind, u8)>);

/// The layers of a square whose corners (SW, SE, NW, NE) show these grounds and no road, as
/// `(ground corner, mask owner, mask kind, rotation)`.
fn layers(sw: u32, se: u32, nw: u32, ne: u32) -> Vec<Picked> {
    let t = [ground(sw), ground(se), ground(nw), ground(ne)];
    let none = SurfacePick::NONE;
    square_layers([&t[0], &t[1], &t[2], &t[3]], [&none; 4], true)
        .into_iter()
        .map(|l: Layer| {
            assert!(!l.road);
            (
                l.ground,
                l.mask.map(|m: Mask| (m.owner, m.kind, m.rotation)),
            )
        })
        .collect()
}

/// Every way a square's four corners can share grounds gives the layers of the retail table:
/// the majority ground at the bottom, the others cut in with their corner or side masks at the
/// quarter turn that faces their corner, at most three layers.
///
/// Behaviour: world.terrain.a-square-blends-its-corners-by-the-blend-table
#[test]
fn a_square_blends_its_corners_by_the_blend_table() {
    use Corner::{Ne, Nw, Se, Sw};
    use MaskKind::{Corner as C, Side};
    assert_eq!(layers(1, 1, 1, 1), vec![(Sw, None)]);
    assert_eq!(layers(1, 1, 1, 2), vec![(Sw, None), (Ne, Some((Ne, C, 2)))]);
    assert_eq!(layers(1, 1, 2, 1), vec![(Sw, None), (Nw, Some((Nw, C, 1)))]);
    assert_eq!(layers(1, 2, 1, 1), vec![(Ne, None), (Se, Some((Se, C, 3)))]);
    assert_eq!(layers(2, 1, 1, 1), vec![(Ne, None), (Sw, Some((Sw, C, 4)))]);
    assert_eq!(
        layers(1, 2, 1, 2),
        vec![(Sw, None), (Ne, Some((Ne, Side, 2)))]
    );
    assert_eq!(
        layers(1, 1, 2, 2),
        vec![(Nw, None), (Se, Some((Ne, Side, 3)))]
    );
    assert_eq!(
        layers(1, 1, 2, 3),
        vec![(Sw, None), (Nw, Some((Nw, C, 1))), (Ne, Some((Ne, C, 2)))]
    );
    assert_eq!(
        layers(2, 3, 1, 1),
        vec![(Ne, None), (Se, Some((Se, C, 3))), (Sw, Some((Sw, C, 4)))]
    );
    assert_eq!(
        layers(2, 1, 3, 1),
        vec![(Ne, None), (Nw, Some((Nw, C, 1))), (Sw, Some((Sw, C, 4)))]
    );
    assert_eq!(
        layers(1, 2, 1, 3),
        vec![(Sw, None), (Ne, Some((Ne, C, 2))), (Se, Some((Se, C, 3)))]
    );
    assert_eq!(
        layers(1, 2, 3, 1),
        vec![(Sw, None), (Nw, Some((Nw, C, 1))), (Se, Some((Se, C, 3)))]
    );
    assert_eq!(
        layers(2, 1, 1, 3),
        vec![(Se, None), (Sw, Some((Sw, C, 4))), (Ne, Some((Ne, C, 2)))]
    );
    assert_eq!(
        layers(1, 2, 3, 4),
        vec![(Sw, None), (Nw, Some((Nw, C, 1))), (Ne, Some((Ne, C, 2)))]
    );
}

/// A road along a square's north edge is a side-masked road layer over the terrain; a road on all
/// four corners replaces the terrain with one unmasked road layer; and no square has more than
/// three layers, so a road over three terrain layers is not drawn.
///
/// Behaviour: world.terrain.a-road-is-a-masked-layer-over-the-ground
#[test]
fn a_road_is_a_masked_layer_over_the_ground() {
    let t = [ground(1), ground(1), ground(1), ground(1)];
    let none = SurfacePick::NONE;
    let road = ground(9);
    let north = square_layers(
        [&t[0], &t[1], &t[2], &t[3]],
        [&none, &none, &road, &road],
        true,
    );
    assert_eq!(north.len(), 2);
    assert_eq!(
        north[1].mask,
        Some(Mask {
            owner: Corner::Nw,
            kind: MaskKind::Side,
            rotation: 1
        })
    );
    assert!(north[1].road);
    let all = square_layers([&t[0], &t[1], &t[2], &t[3]], [&road; 4], true);
    assert_eq!(all.len(), 1);
    assert!(all[0].road && all[0].mask.is_none());
    let mixed = [ground(1), ground(2), ground(3), ground(4)];
    let full = square_layers(
        [&mixed[0], &mixed[1], &mixed[2], &mixed[3]],
        [&none, &none, &road, &road],
        true,
    );
    assert_eq!(full.len(), 3);
    assert!(full.iter().all(|l| !l.road));
}
