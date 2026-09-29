//! Outdoor objects register the cells reached by their part bounds, including billboards and building interiors.
//! Fixture: the shipped retail DAT records and recorded inputs.

use std::sync::Arc;

use dereth_physics::cell::{CellArray, CellResolver};
use dereth_physics::geom::bsp::{BspNode, BspNodeKind, BspTree};
use dereth_physics::geom::{BBox, Plane, Polygon, Sphere};
use dereth_physics::source::{CellPortal, EnvCellGeometry, PhysicsPart};
use dereth_physics::{landdefs, LandSource, LandblockCollision, StaticLandSource};
use dereth_primitives::{CellId, Frame, LandblockId, Position, Quat, Vec3};

const BLOCK: LandblockId = LandblockId(0xA9B4);

fn tree(radius: f32) -> BspTree {
    let sq = Polygon::new(vec![
        Vec3::new(-1.0, -1.0, 0.0),
        Vec3::new(1.0, -1.0, 0.0),
        Vec3::new(1.0, 1.0, 0.0),
        Vec3::new(-1.0, 1.0, 0.0),
    ]);
    BspTree {
        nodes: vec![BspNode {
            sphere: Sphere::new(Vec3::ZERO, radius),
            splitting_plane: Plane {
                normal: Vec3::new(0.0, 0.0, 1.0),
                d: 1000.0,
            },
            pos_child: None,
            neg_child: None,
            kind: BspNodeKind::Leaf {
                leaf_index: 0,
                solid: true,
            },
            in_polys: vec![0],
        }],
        polygons: vec![sq],
    }
}

fn flat_land() -> StaticLandSource {
    let mut land = StaticLandSource::linear();
    for dx in -1_i32..=1 {
        for dy in -1_i32..=1 {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            land.add_flat_block(LandblockId::new((0xA9 + dx) as u8, (0xB4 + dy) as u8), 10);
        }
    }
    land
}

/// One part standing at `origin` in landblock-relative metres, carrying `bb` as its
/// The graphics object's bounding box.
fn part_at(origin: Vec3, bb: BBox) -> PhysicsPart {
    PhysicsPart {
        pos: Position::new(outdoor_cell(origin), Frame::new(origin, Quat::IDENTITY)),
        gfxobj_scale: 1.0,
        physics_bsp: Some(Arc::new(tree(2.0))),
        bound_box: Some(bb),
        drawing_sphere: None,
        first_degrade_mode: None,
    }
}

///  on a landblock-relative point of `BLOCK`.
fn outdoor_cell(origin: Vec3) -> CellId {
    let mut id = BLOCK.cell(1);
    let mut o = origin;
    assert!(
        landdefs::adjust_to_outside(&mut id, &mut o),
        "{origin:?} is not in an outdoor cell"
    );
    id
}

/// Run `find_bbox_cell_list` and return the cell ids, sorted.
fn shadow_set(land: &dyn LandSource, parts: &[PhysicsPart]) -> Vec<u32> {
    let r = CellResolver::new(land);
    let pos = parts[0].pos;
    let mut arr = CellArray::default();
    r.find_bbox_cell_list(&pos, parts, &mut arr);
    let mut v: Vec<u32> = arr.cells.iter().map(|c| c.cell_id.0).collect();
    v.sort_unstable();
    v
}

/// A land cell id from its coordinates **inside** `BLOCK`: `1 + 8 * x + y` in the low sixteen bits,
/// which is what `((id & 0xFFFF) - 1) >> 3` and `(id - 1) & 7` invert.
fn cell(x: u32, y: u32) -> u32 {
    (u32::from(BLOCK.0) << 16) | (1 + 8 * x + y)
}

// =================================================================================================
// The premise: this object really is outdoors and really takes the bbox arm
// =================================================================================================

/// The fixture stands outdoors and takes the bounding box arm.
#[test]
fn the_fixture_stands_outdoors_and_takes_the_bounding_box_arm() {
    let origin = Vec3::new(108.0, 108.0, 20.0);
    let id = outdoor_cell(origin);
    assert!(
        landdefs::is_outdoors(id),
        "cell {:08X} must be a land cell",
        id.0
    );
    assert_eq!(
        id.0,
        cell(4, 4),
        "108 / 24 = 4.5, so the object stands in cell (4, 4)"
    );

    let g = dereth_physics::SetupGeometry {
        parts: vec![dereth_physics::source::SetupPart {
            physics_bsp: Some(Arc::new(tree(2.0))),
            bound_box: Some(BBox::new(
                Vec3::new(-1.0, -1.0, -1.0),
                Vec3::new(1.0, 1.0, 1.0),
            )),
            ..dereth_physics::source::SetupPart::default()
        }],
        ..dereth_physics::SetupGeometry::default()
    };
    assert!(
        g.caches_physics_bsp(),
        "without a part BSP takes the sorting-sphere arm and none \
         of this file's subject runs at all"
    );
}

// =================================================================================================
// The land cell's all-outside-cells add
// =================================================================================================

/// Behaviour: physics.cells.an-outdoor-object-registers-every-land-cell-its-part-boxes-reach
/// An outdoor object registers exactly the land cells its part boxes reach.
#[test]
fn an_outdoor_object_registers_exactly_the_land_cells_its_part_boxes_reach() {
    let land = flat_land();
    let small = BBox::new(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));

    // Dead centre of cell (4, 4). This is the degenerate-looking station and it is the one that
    // caught the defect: a body at a cell centre must reach exactly one cell, so any extra cell
    // in the answer is arithmetic rather than geometry.
    let centre = shadow_set(&land, &[part_at(Vec3::new(108.0, 108.0, 20.0), small)]);
    assert_eq!(
        centre,
        vec![cell(4, 4)],
        "a 2 m box at the exact centre of cell (4, 4) reaches no other cell; before this \
         read four -- (3,3), (3,4), (4,3), (4,4) -- because the box was transformed into a frame \
         half a cell off"
    );

    // Use an off-centre control: 108 is a multiple of 12 and
    // the exact cell centre, so the tidy case is perturbed and required to give the same answer.
    let offset = shadow_set(&land, &[part_at(Vec3::new(100.0, 100.0, 20.0), small)]);
    assert_eq!(
        offset,
        vec![cell(4, 4)],
        "still one cell 8 m off the centre: {offset:?}"
    );

    // A box that genuinely spans three columns: 90 .. 130 in x, 107 .. 109 in y.
    let wide = BBox::new(Vec3::new(-18.0, -1.0, -1.0), Vec3::new(22.0, 1.0, 1.0));
    let spanning = shadow_set(&land, &[part_at(Vec3::new(108.0, 108.0, 20.0), wide)]);
    assert_eq!(
        spanning,
        vec![cell(3, 4), cell(4, 4), cell(5, 4)],
        "x from 90 to 130 crosses the 96 and 120 boundaries, so cells (3,4), (4,4) and (5,4) and \
         nothing in y; before the eastern column was missing entirely and three southern \
         cells were present that the box never touches"
    );
}

/// The rectangle is the union over the parts and always contains the objects own cell.
#[test]
fn the_rectangle_is_the_union_over_the_parts_and_always_contains_the_objects_own_cell() {
    let land = flat_land();
    let here = Vec3::new(108.0, 108.0, 20.0);
    let small = BBox::new(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));

    // Part 0 anchors the object in cell (4, 4); part 1 sits 26 m north, in cell (4, 5).
    let a = part_at(here, small);
    let mut b = part_at(here, small);
    b.pos = Position::new(
        a.pos.cell,
        Frame::new(Vec3::new(108.0, 134.0, 20.0), Quat::IDENTITY),
    );
    let two = shadow_set(&land, &[a.clone(), b]);
    assert_eq!(
        two,
        vec![cell(4, 4), cell(4, 5)],
        "the union of a part in (4,4) and a part in (4,5): {two:?}"
    );

    let far_box = BBox::new(Vec3::new(-1.0, 49.0, -1.0), Vec3::new(1.0, 51.0, 1.0));
    let lone = part_at(here, far_box);
    assert_eq!(
        outdoor_cell(Vec3::new(108.0, 158.0, 20.0)),
        CellId(cell(4, 6)),
        "the premise: the box really does sit two cells north"
    );
    let one = shadow_set(&land, std::slice::from_ref(&lone));
    assert_eq!(
        one,
        vec![cell(4, 4), cell(4, 5), cell(4, 6)],
        "the zero seed keeps the object's own cell and everything between it and the box: {one:?}"
    );
}

/// An always 2d part registers from its bounding sphere and not from its box.
#[test]
fn an_always_2d_part_registers_from_its_bounding_sphere_and_not_from_its_box() {
    let land = flat_land();
    let here = Vec3::new(108.0, 108.0, 20.0);
    let small = BBox::new(Vec3::new(-0.5, -0.5, -0.5), Vec3::new(0.5, 0.5, 0.5));

    let mut mode1 = part_at(here, small);
    mode1.physics_bsp = Some(Arc::new(tree(13.0)));
    mode1.first_degrade_mode = Some(1);
    assert!(!mode1.always_2d(), "degrade_mode 1 is the 3D part");
    assert_eq!(
        shadow_set(&land, &[mode1.clone()]),
        vec![cell(4, 4)],
        "a 1 m box decides for a part that is not a billboard, even with a 13 m bounding sphere"
    );

    let mut mode5 = mode1;
    mode5.first_degrade_mode = Some(5);
    assert!(mode5.always_2d(), "degrade_mode 5 is the z-axis billboard");
    let billboard = shadow_set(&land, &[mode5.clone()]);
    let expected: Vec<u32> = (3..=5)
        .flat_map(|x| (3..=5).map(move |y| cell(x, y)))
        .collect();
    let mut expected_sorted = expected;
    expected_sorted.sort_unstable();
    assert_eq!(
        billboard, expected_sorted,
        "a 13 m bounding sphere at (108, 108) spans 95 .. 121 in both axes, so cells (3..5, 3..5)"
    );

    // The second asymmetry, on its own: the sphere arm reads the part's **own** origin and does
    // not transform it. Move the part's frame 26 m north while leaving the object's position
    // where it was; the box arm would transform that origin too, so this does
    // not by itself separate the two -- what it pins is that the origin used is the part's.
    let mut moved = mode5;
    moved.pos = Position::new(
        moved.pos.cell,
        Frame::new(Vec3::new(108.0, 134.0, 20.0), Quat::IDENTITY),
    );
    let shifted = shadow_set(&land, &[moved]);
    assert!(
        shifted.contains(&cell(4, 6)) && !shifted.contains(&cell(4, 3)),
        "the sphere follows the part's own origin north: {shifted:?}"
    );
}

// =================================================================================================
// Interior overlap through the sorting cell's transit-cell search
// =================================================================================================

/// A `LandSource` that answers [`LandSource::building_cells`], which `StaticLandSource` does not:
/// its default is the empty list, which is a world with no buildings, and that is precisely why
/// nothing has ever exercised `check_building_transit_bbox`.
///
/// The producer this stands in for exists and is `dereth-client`'s `DatLandSource`; what is missing
/// is not the producer but a **world under test**, so this supplies one land cell with one
/// interior cell behind it.
struct BuildingLand {
    inner: StaticLandSource,
    land_cell: CellId,
    interior: CellId,
    geom: Arc<EnvCellGeometry>,
}

impl LandSource for BuildingLand {
    fn landblock(&self, id: LandblockId) -> Option<Arc<LandblockCollision>> {
        self.inner.landblock(id)
    }
    fn height_table(&self) -> &[f32; dereth_physics::globals::LAND_HEIGHT_TABLE_LEN] {
        self.inner.height_table()
    }
    fn env_cell(&self, cell: CellId) -> Option<Arc<EnvCellGeometry>> {
        if cell == self.interior {
            Some(Arc::clone(&self.geom))
        } else {
            self.inner.env_cell(cell)
        }
    }
    fn building_cells(&self, cell: CellId) -> Vec<CellId> {
        if cell == self.land_cell {
            vec![self.interior]
        } else {
            Vec::new()
        }
    }
}

/// One room: a cell BSP that is a 6 m cube around the cell's own origin, with one portal so that
/// 's "no portals at all is not a cell" guard does not fire.
fn room(id: CellId, origin: Vec3) -> EnvCellGeometry {
    let half = 3.0_f32;
    // A cell BSP whose only node is a leaf: walks
    // positive children and running out of them means "inside", so a bare leaf is "everywhere".
    // The extent that decides is the BSP root sphere, which is the
    // cell's own bounding sphere -- so that is what is sized here.
    // The box/BSP intersection test descends positive children only and rejects a box
    // all eight of whose corners are behind a plane, so a room is a chain of six half-spaces
    // ending in a leaf. A single bare leaf would answer "yes" to every box on earth, which is a
    // cell BSP that cannot say no and therefore an assertion that cannot fail.
    let bound = Sphere::new(Vec3::ZERO, half * 3.0_f32.sqrt());
    let face = |n: Vec3, next: u32| BspNode {
        sphere: bound,
        splitting_plane: Plane { normal: n, d: half },
        pos_child: Some(next),
        neg_child: Some(next),
        kind: BspNodeKind::Node,
        in_polys: vec![],
    };
    let mut geom = EnvCellGeometry {
        id,
        frame: Frame::new(origin, Quat::IDENTITY),
        ..EnvCellGeometry::default()
    };
    geom.cell_bsp = Some(Arc::new(BspTree {
        nodes: vec![
            face(Vec3::new(1.0, 0.0, 0.0), 1),
            face(Vec3::new(-1.0, 0.0, 0.0), 2),
            face(Vec3::new(0.0, 1.0, 0.0), 3),
            face(Vec3::new(0.0, -1.0, 0.0), 4),
            face(Vec3::new(0.0, 0.0, 1.0), 5),
            face(Vec3::new(0.0, 0.0, -1.0), 6),
            BspNode {
                sphere: bound,
                splitting_plane: Plane {
                    normal: Vec3::new(0.0, 0.0, 1.0),
                    d: 1000.0,
                },
                pos_child: None,
                neg_child: None,
                kind: BspNodeKind::Leaf {
                    leaf_index: 0,
                    solid: false,
                },
                in_polys: vec![],
            },
        ],
        polygons: vec![],
    }));
    geom.portals = vec![CellPortal {
        other_cell_id: 0xFFFF_FFFF,
        portal: Polygon::new(vec![
            Vec3::new(-half, -half, 0.0),
            Vec3::new(half, -half, 0.0),
            Vec3::new(half, -half, 2.0),
            Vec3::new(-half, -half, 2.0),
        ]),
        portal_side: true,
        other_portal_id: 0,
        exact_match: false,
    }];
    geom
}

/// **The building's transit-cell search, reached through the box test** -- the other half of the
/// ** -- the other half of the outdoor arm, and the one
/// `StaticLandSource` structurally cannot reach: [`LandSource::building_cells`]' default is the
/// empty list, so every world every physics test has ever built has had no buildings in it.
///
/// A land cell with a building offers the interior cells behind that building's portals, and the
/// object standing in the street is registered in one of them when its part box reaches into it.
/// The judge is the **cell set**, and the negative arm is the calibration: move the same box 20 m
/// east, out of the room, and the interior must not appear -- without it "the box reached" and
/// "this arm adds every building cell it is offered" read alike.
#[test]
fn an_outdoor_object_whose_box_reaches_into_a_building_is_registered_in_that_interior_cell() {
    let here = Vec3::new(108.0, 108.0, 20.0);
    let land_cell = outdoor_cell(here);
    let interior = CellId((u32::from(BLOCK.0) << 16) | 0x0100);
    let land = BuildingLand {
        inner: flat_land(),
        land_cell,
        interior,
        geom: Arc::new(room(interior, here)),
    };

    // The premise: the world really does offer a building cell here, and really does not offer one
    // in the neighbouring land cell.
    assert_eq!(land.building_cells(land_cell), vec![interior]);
    assert!(land
        .building_cells(outdoor_cell(Vec3::new(132.0, 108.0, 20.0)))
        .is_empty());
    assert!(
        land.env_cell(interior).is_some(),
        "the interior must be resident, or it is skipped"
    );

    // A 2 m box standing in the middle of the room's 6 m cube.
    let inside = BBox::new(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0));
    let got = shadow_set(&land, &[part_at(here, inside)]);
    assert_eq!(
        got,
        vec![cell(4, 4), interior.0],
        "the street cell and the room behind the building's portal: {got:?}"
    );

    // The calibration: the same box, 20 m east. It is still in land cell (4, 4) -- so the building
    // is still offered -- but must refuse it.
    let outside = BBox::new(Vec3::new(19.0, -1.0, -1.0), Vec3::new(21.0, 1.0, 1.0));
    let missed = shadow_set(&land, &[part_at(here, outside)]);
    assert!(
        !missed.contains(&interior.0),
        "a box 20 m from the room must not be registered in it: {missed:?}"
    );
    assert!(
        missed.contains(&cell(4, 4)) && missed.contains(&cell(5, 4)),
        "and it must still be registered in the two land cells it does reach: {missed:?}"
    );
}

/// Every shipped part uses its first degradation mode to select billboard registration.
#[test]
fn every_shipped_part_uses_its_first_degrade_mode_for_billboarding() {
    use dereth_assets::{Decode, GfxObj, GfxObjDegradeInfo, Setup};
    use dereth_dat::DbType;
    use dereth_primitives::DataId;
    use std::collections::BTreeMap;

    let store =
        dereth_dat::testing::open_store().expect("the retail dats: set DERETH_TEST_DAT_DIR");
    let mut degrade: BTreeMap<u32, Option<i32>> = BTreeMap::new();
    let mut gfx: BTreeMap<u32, Option<(bool, Option<DataId>)>> = BTreeMap::new();

    let (mut setups, mut parts, mut with_bsp) = (0_u32, 0_u32, 0_u32);
    let (mut always_2d, mut both, mut mode_1) = (0_u32, 0_u32, 0_u32);
    let (mut setups_with_bsp, mut setups_with_bsp_and_2d) = (0_u32, 0_u32);
    let mut modes: BTreeMap<i32, u32> = BTreeMap::new();
    let mut both_gfx: Vec<u32> = Vec::new();
    let mut named: Vec<u32> = Vec::new();

    let ids = store.ids_of(DbType::Setup);
    assert!(!ids.is_empty(), "setups are exercised");
    for id in &ids {
        let bytes = store.read_typed(DbType::Setup, *id).expect("setup reads");
        let s = Setup::decode_payload(*id, &bytes).expect("setup decodes");
        setups += 1;
        let (mut any_bsp, mut any_2d) = (false, false);
        for p in &s.parts {
            parts += 1;
            let entry = gfx.entry(p.0).or_insert_with(|| {
                let b = store.read_typed(DbType::GfxObj, *p).ok()?;
                let g = GfxObj::decode_payload(*p, &b).ok()?;
                Some((g.physics_bsp.is_some(), g.did_degrade))
            });
            let Some((has_bsp, did)) = *entry else {
                continue;
            };
            if has_bsp {
                with_bsp += 1;
                any_bsp = true;
            }
            let mode = did.and_then(|d| {
                *degrade.entry(d.0).or_insert_with(|| {
                    let b = store.read_typed(DbType::DegradeInfo, d).ok()?;
                    let info = GfxObjDegradeInfo::decode_payload(d, &b).ok()?;
                    info.degrades.first().map(|g| g.degrade_mode)
                })
            });
            if let Some(m) = mode {
                *modes.entry(m).or_default() += 1;
                if m == 1 {
                    mode_1 += 1;
                }
            }
            let probe = PhysicsPart {
                first_degrade_mode: mode,
                ..PhysicsPart::default()
            };
            assert_eq!(
                probe.always_2d(),
                matches!(mode, Some(2 | 5)),
                "{id}: part {p} billboard mode"
            );
            if probe.always_2d() {
                always_2d += 1;
                any_2d = true;
                if has_bsp {
                    both += 1;
                    both_gfx.push(p.0);
                }
            }
        }
        if any_bsp {
            setups_with_bsp += 1;
            if any_2d {
                setups_with_bsp_and_2d += 1;
                named.push(id.0);
            }
        }
    }
    both_gfx.sort_unstable();
    both_gfx.dedup();
    named.sort_unstable();
    eprintln!(
        "census: setups={setups} parts={parts} with_bsp={with_bsp} always_2d={always_2d} \
         both={both} mode_1={mode_1} modes={modes:?} setups_with_bsp={setups_with_bsp} \
         of_which_2d={setups_with_bsp_and_2d} gfx={both_gfx:08X?} setups={named:08X?}"
    );

    assert_eq!(setups as usize, ids.len(), "every input setup is examined");
    assert!(
        parts > 0 && with_bsp > 0,
        "parts with collision geometry are exercised"
    );
    assert!(mode_1 > 0, "ordinary parts are exercised");
    assert!(
        modes.get(&2).is_some_and(|n| *n > 0),
        "full billboards are exercised"
    );
    assert!(
        modes.get(&5).is_some_and(|n| *n > 0),
        "axial billboards are exercised"
    );
    assert_eq!(
        always_2d,
        modes.get(&2).copied().unwrap_or(0) + modes.get(&5).copied().unwrap_or(0)
    );
    assert!(both > 0, "billboards with collision geometry are exercised");
    assert!(
        both_gfx.contains(&0x0100_215F),
        "the billboard collision regression object is present"
    );
    assert!(setups_with_bsp > 0);
    assert_eq!(setups_with_bsp_and_2d as usize, named.len());
    for id in [
        0x0200_0359,
        0x0200_0A02,
        0x0200_17D5,
        0x0200_1BCF,
        0x0200_1C2B,
    ] {
        assert!(
            named.contains(&id),
            "the billboard collision regression setup {id:08x} is present"
        );
    }
}
