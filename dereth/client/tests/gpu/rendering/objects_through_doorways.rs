//! An object standing outdoors stays visible from inside through an open doorway. When the
//! viewer's cell is an interior with outside views, the interior draw draws the landscape through
//! the openings, and that landscape pass draws each outdoor cell's **objects** too, before the
//! alpha flush, the depth clear and the re-stamping of the openings; an object drawn after the
//! stamp would be farther than it and lost. A held item takes its holder's cell and so its
//! holder's outdoor pass. Station: Holtburg's ground-floor room `0xA9B40143` facing north, three
//! outdoor openings in view (~4900 px² at 400x300), with a `0x0200004D` setup (never degraded
//! out) standing 11 m away at `(136.1, 22.1)` behind the left-hand opening. The control is
//! `SceneConfig::portal_depth_stamp = false`: the same frame with no opening stamped back, whose
//! object pixels are the reference for the shipped arm's 80% lower bound. Fixture: the retail
//! dats on a software device. Fails when the retail dats are absent; the pixel station skips
//! only without a device, the held-item station fails without one.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_client::character::CharacterInput;
use dereth_client::env_cells::{physics_geometry, EnvCellLoader};
use dereth_client::objects::ObjectStream;
use dereth_client::world::{SceneConfig, WorldScene};
use dereth_client_net::client_session::SessionEvent;
use dereth_dat::RetailDatStore;
use dereth_primitives::{CellId, Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};
use dereth_protocol::types::physicsdesc::{flags, PhysicsDesc};
use dereth_protocol::types::{PhysicsTimestamps, PublicWeenieDesc};
use dereth_render::device::Gpu;
use std::sync::Arc;

const W: u32 = 400;
const H: u32 = 300;
const HOLTBURG: u16 = 0xA9B4;

/// The ground-floor room of the Holtburg house with a cellar. Three outdoor openings are in view
/// from its standable point at `yaw = 0`.
const STATION: u32 = 0xA9B4_0143;
/// Where the object stands, in the block's own metres. Chosen by sweeping a ring around the body
/// and taking the bearing and range with the largest visible-through-the-opening area.
const OBJECT_XY: (f32, f32) = (136.1, 22.1);
/// `rendering::object_draw_distance`'s never-goes-dark setup, so nothing in this test turns on
/// `GfxObjDegradeInfo`'s terminator at 11 m.
const NEVER_DARK: u32 = 0x0200_004D;
const TARGET: ObjectId = ObjectId(0x5000_0F31);
/// The human and weapon setups of the recorded held-item fixture.
const HOLDER_SETUP: u32 = 0x0200_0001;
const HELD_SETUP: u32 = 0x0200_1713;
const HELD_HOLDER: ObjectId = ObjectId(0x5000_0F32);
const HELD_ITEM: ObjectId = ObjectId(0x5000_0F33);
const RIGHT_HAND: u32 = 1;
const RIGHT_HAND_COMBAT: u32 = 1;

fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

/// A non-solid point inside `cell`. The same search `rendering/indoor_depth_clear.rs` uses, and
/// for the same reason: a Holtburg house shares one frame origin across eighteen cells.
fn point_in(store: &RetailDatStore, cell: u32) -> Option<Vec3> {
    #[allow(clippy::cast_possible_truncation)] // a cell id's top 16 bits are its landblock
    let block = (cell >> 16) as u16;
    let d = EnvCellLoader::new()
        .load_block(store, block)
        .into_iter()
        .find(|d| d.id.0 == cell)?;
    let g = physics_geometry(&d);
    let bsp = g.cell_bsp.as_ref()?;
    for zi in -24i32..=24 {
        for i in -40i32..=40 {
            for j in -40i32..=40 {
                #[allow(clippy::cast_precision_loss)] // small loop counters
                let local = Vec3::new(i as f32 * 0.5, j as f32 * 0.5, zi as f32 * 0.5);
                if !bsp.point_inside_cell_bsp(local) {
                    continue;
                }
                if g.physics_bsp
                    .as_ref()
                    .is_some_and(|b| b.point_intersects_solid(local))
                {
                    continue;
                }
                return Some(dereth_physics::math::localtoglobal(&g.frame, local));
            }
        }
    }
    None
}

/// Normalize block-local metres into an outdoor cell and origin.
fn outdoor(x: f32, y: f32, z: f32) -> Position {
    let mut cell = LandblockId(HOLTBURG).cell(1);
    let mut o = Vec3::new(x, y, z);
    dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut o);
    Position::new(cell, Frame::new(o, Quat::IDENTITY))
}

struct Bench {
    gpu: Gpu,
    scene: WorldScene,
    store: Arc<RetailDatStore>,
    objects: ObjectStream,
    now: f64,
}

impl Bench {
    fn new(store: &Arc<RetailDatStore>, stamp: bool) -> Option<Self> {
        let mut gpu = crate::common::test_gpu(W, H);
        let region = dereth_client::world::load_region(store).expect("the region decodes");
        let cfg = SceneConfig {
            landblock: HOLTBURG,
            time_of_day: Some(0.35),
            particles: false,
            portal_depth_stamp: stamp,
            ..SceneConfig::default()
        };
        let mut scene = WorldScene::load(store, &mut gpu, cfg).expect("the scene loads");
        scene
            .attach_character(store, &region, &mut gpu)
            .expect("the body is created");
        Some(Self {
            gpu,
            scene,
            store: store.clone(),
            // Parent links validate holding locations against the retail setup. The assetless
            // constructor deliberately refuses them, which would never reach the held draw path.
            objects: ObjectStream::with_store(store.clone()),
            now: 1.0,
        })
    }

    fn stand(&mut self, cell: u32, p: Vec3) {
        self.scene
            .character
            .as_mut()
            .expect("a body")
            .teleport(Position::new(CellId(cell), Frame::new(p, Quat::IDENTITY)));
    }

    /// Create the positioned target through the application's own object stream.
    fn place(&mut self, at: Position) {
        let payload = dereth_protocol::objects::ObjectCreatePayload {
            id: TARGET,
            objdesc: dereth_protocol::types::ObjDesc::default(),
            physicsdesc: PhysicsDesc {
                bitfield: flags::POSITION | flags::SETUP,
                setup_id: Some(NEVER_DARK),
                position: Some(dereth_protocol::types::PositionWire {
                    objcell_id: at.cell.0,
                    frame: dereth_protocol::types::Frame {
                        origin: at.frame.origin.into(),
                        orientation: Quat::IDENTITY.into(),
                    },
                }),
                timestamps: PhysicsTimestamps {
                    instance: 1,
                    ..PhysicsTimestamps::default()
                },
                ..PhysicsDesc::default()
            },
            wdesc: PublicWeenieDesc::default(),
        };
        let body =
            dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(payload))
                .expect("encode");
        self.objects.apply_event(
            &SessionEvent::WorldObject {
                opcode: dereth_protocol::Opcode::ITEM_CREATE_OBJECT,
                body,
            },
            LocalTime(self.now),
        );
    }

    /// The recorded held-item server shape: a positioned human followed by a positionless
    /// weapon attached to his right hand. Both stand behind this room's outdoor doorway opening.
    fn place_held(&mut self, at: Position) {
        let holder = dereth_protocol::objects::ObjectCreatePayload {
            id: HELD_HOLDER,
            objdesc: dereth_protocol::types::ObjDesc::default(),
            physicsdesc: PhysicsDesc {
                bitfield: flags::POSITION | flags::SETUP | flags::ANIMFRAME,
                setup_id: Some(HOLDER_SETUP),
                position: Some(dereth_protocol::types::PositionWire {
                    objcell_id: at.cell.0,
                    frame: dereth_protocol::types::Frame {
                        origin: at.frame.origin.into(),
                        orientation: Quat::IDENTITY.into(),
                    },
                }),
                animframe_id: Some(101),
                timestamps: PhysicsTimestamps {
                    instance: 1,
                    ..PhysicsTimestamps::default()
                },
                ..PhysicsDesc::default()
            },
            wdesc: PublicWeenieDesc::default(),
        };
        let held = dereth_protocol::objects::ObjectCreatePayload {
            id: HELD_ITEM,
            objdesc: dereth_protocol::types::ObjDesc::default(),
            physicsdesc: PhysicsDesc {
                bitfield: flags::SETUP | flags::PARENT | flags::ANIMFRAME,
                setup_id: Some(HELD_SETUP),
                parent: Some((HELD_HOLDER, RIGHT_HAND)),
                animframe_id: Some(RIGHT_HAND_COMBAT),
                timestamps: PhysicsTimestamps {
                    instance: 1,
                    ..PhysicsTimestamps::default()
                },
                ..PhysicsDesc::default()
            },
            wdesc: PublicWeenieDesc::default(),
        };
        for payload in [holder, held] {
            let body =
                dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(payload))
                    .expect("encode");
            self.objects.apply_event(
                &SessionEvent::WorldObject {
                    opcode: dereth_protocol::Opcode::ITEM_CREATE_OBJECT,
                    body,
                },
                LocalTime(self.now),
            );
        }
    }

    /// The app's frame order — `sync_objects`, `update`, camera update, `stream`, `reserve_upload_arena`,
    /// `begin_frame`, `draw`, `end_frame` — so what is captured is the production path.
    fn draw(&mut self) -> Vec<u8> {
        self.now += 1.0;
        let Self {
            store,
            gpu,
            scene,
            objects,
            ..
        } = self;
        scene
            .sync_objects(store, gpu, objects)
            .expect("sync_objects");
        scene.update(
            dereth_client::camera::CameraInput::default(),
            CharacterInput::default(),
            LocalTime(self.now),
            1.0 / 30.0,
        );
        // **`App::frame`'s next stage.** The camera sphere sweep updates
        // `CameraControl::viewer_cell` and publishes the drawn camera into `scene.camera`.
        dereth_client::camera::update_viewer(
            scene,
            dereth_client::camera::CameraInput::default(),
            LocalTime(self.now),
            1.0 / 30.0,
        );
        scene.stream(store, gpu).expect("stream");
        scene.reserve_upload_arena(gpu).expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        scene.draw(gpu).expect("draw");
        gpu.end_frame().expect("end");
        gpu.capture().expect("capture").to_rgba()
    }

    fn target_parts(&self) -> usize {
        self.scene
            .drawn_part_order()
            .iter()
            .filter(|e| e.object == Some(TARGET))
            .count()
    }
}

/// Even-odd point-in-polygon, as `rendering/indoor_depth_clear.rs` has it.
fn inside(poly: &[(f32, f32)], px: f32, py: f32) -> bool {
    let mut c = false;
    for i in 0..poly.len() {
        let (x0, y0) = poly[i];
        let (x1, y1) = poly[(i + 1) % poly.len()];
        if (y0 > py) != (y1 > py) {
            let t = (py - y0) / (y1 - y0);
            if px < (x1 - x0).mul_add(t, x0) {
                c = !c;
            }
        }
    }
    c
}

fn opening_mask(polys: &[Vec<(f32, f32)>]) -> Vec<bool> {
    let mut mask = vec![false; (W * H) as usize];
    for poly in polys.iter().filter(|p| p.len() >= 3) {
        for y in 0..H {
            for x in 0..W {
                #[allow(clippy::cast_precision_loss)] // a window extent
                if inside(poly, x as f32 + 0.5, y as f32 + 0.5) {
                    mask[(y * W + x) as usize] = true;
                }
            }
        }
    }
    mask
}

/// Pixels that differ between the two frames, split by the opening mask.
fn changed(a: &[u8], b: &[u8], mask: &[bool]) -> (usize, usize) {
    let (mut din, mut dout) = (0, 0);
    for (i, (pa, pb)) in a.chunks_exact(4).zip(b.chunks_exact(4)).enumerate() {
        if pa[..3] != pb[..3] {
            if mask[i] {
                din += 1;
            } else {
                dout += 1;
            }
        }
    }
    (din, dout)
}

struct Arm {
    inside: usize,
    outside: usize,
    parts: usize,
    mask: usize,
}

fn arm(store: &Arc<RetailDatStore>, stamp: bool) -> Option<Arm> {
    let mut b = Bench::new(store, stamp)?;
    let p = point_in(store, STATION).expect("the station has a standable point");
    b.stand(STATION, p);
    for _ in 0..4 {
        b.draw();
    }
    let settled = b.scene.character.as_ref().expect("a body").position();
    assert!(
        !dereth_physics::landdefs::is_outdoors(settled.cell),
        "the viewer settled outdoors in {:#010X}; the indoor path would not run",
        settled.cell.0
    );
    let base = b.draw();
    let polys = b.scene.indoor_outdoor_portal_screen_polygons(W, H);
    let mask = opening_mask(&polys);
    let z = b
        .scene
        .character
        .as_ref()
        .expect("a body")
        .world
        .terrain_height_at(&outdoor(OBJECT_XY.0, OBJECT_XY.1, 0.0))
        .expect("the terrain has a height there");
    b.place(outdoor(OBJECT_XY.0, OBJECT_XY.1, z));
    let with = b.draw();
    let (din, dout) = changed(&base, &with, &mask);
    Some(Arm {
        inside: din,
        outside: dout,
        parts: b.target_parts(),
        mask: mask.iter().filter(|m| **m).count(),
    })
}

/// Behaviour: rendering.portals.an-object-outside-survives-the-portal-depth-stamp
/// **The rejecting test.** The object is drawn either way — the difference is entirely the depth
/// stamp, and retail put the object down before the stamp existed.
#[test]
fn an_object_outside_the_doorway_survives_the_portal_depth_stamp() {
    let store = store();
    let Some(control) = arm(&store, false) else {
        return;
    };
    let Some(shipped) = arm(&store, true) else {
        return;
    };
    eprintln!(
        "doorway {STATION:#010X}, opening mask {} px:
           no stamp (control): {} px of the object inside the opening, {} outside, {} parts drawn
           shipped:            {} px of the object inside the opening, {} outside, {} parts drawn",
        control.mask,
        control.inside,
        control.outside,
        control.parts,
        shipped.inside,
        shipped.outside,
        shipped.parts,
    );
    // The premise: there is an opening, and the object really is submitted to the device on both
    // arms. A build that culled it would fail here rather than in the pixel assertion below.
    assert!(
        control.mask > 500,
        "the station shows only {} px of opening",
        control.mask
    );
    assert!(
        control.parts > 0 && control.parts == shipped.parts,
        "the object was submitted {} times on the control arm and {} on the shipped arm -- this \
         station is measuring a cull, not the depth stamp",
        control.parts,
        shipped.parts
    );
    assert!(
        control.inside > 300,
        "the object paints only {} px inside the opening with nothing stamped over it, so this \
         station cannot show anything being stamped away",
        control.inside
    );
    // The stamp is a *depth* write inside the openings and nothing else, so the object's pixels
    // outside them cannot depend on it. This is what makes the assertion below about the opening.
    assert_eq!(
        control.outside, shipped.outside,
        "the two arms differ outside the openings ({} vs {}), so the stamp is not the only thing \
         that changed",
        control.outside, shipped.outside
    );
    // The defect: stamping always passes the depth test and writes the opening's own z/w.
    // A farther object drawn afterward is rejected; retail draws outdoor objects before the
    // depth clear (mask 4), which precedes those stamps.
    assert!(
        shipped.inside * 10 >= control.inside * 8,
        "the shipped build paints {} px of the object inside the opening where the control paints \
         {} -- {:.0}% of it is being stamped away by the portal depth stamp. Retail draws an \
         outdoor cell's objects inside the landscape draw, before the depth clear \
         and before the stamps.",
        shipped.inside,
        control.inside,
        100.0 - 100.0 * shipped.inside as f64 / control.inside as f64,
    );
}

/// Behaviour: rendering.portals.a-held-item-draws-in-its-holders-phase
/// **Rejecting.** A held item has no server position, but parenting immediately assigns the
/// holder's cell to the child, so both are drawn in that cell's pass. From this room, both must
/// use the outdoor pass through the opening rather than splitting the weapon into the later
/// interior pass.
#[test]
fn a_held_item_uses_its_holders_outdoor_draw_phase() {
    let store = store();
    let mut b = Bench::new(&store, true).expect("a real D3D12/WARP device");
    let p = point_in(&store, STATION).expect("the station has a standable point");
    b.stand(STATION, p);
    for _ in 0..4 {
        b.draw();
    }
    let mask = opening_mask(&b.scene.indoor_outdoor_portal_screen_polygons(W, H));
    assert!(
        mask.iter().filter(|m| **m).count() > 500,
        "the station has no outdoor opening"
    );

    let z = b
        .scene
        .character
        .as_ref()
        .expect("a body")
        .world
        .terrain_height_at(&outdoor(OBJECT_XY.0, OBJECT_XY.1, 0.0))
        .expect("the terrain has a height there");
    b.place_held(outdoor(OBJECT_XY.0, OBJECT_XY.1, z));
    b.draw();

    assert_eq!(
        b.scene.draw.stats.server_objects_held, 1,
        "the weapon reached the held draw path"
    );
    let trace = b.scene.drawn_part_order();
    let holder: Vec<_> = trace
        .iter()
        .filter(|d| d.object == Some(HELD_HOLDER))
        .collect();
    let held: Vec<_> = trace
        .iter()
        .filter(|d| d.object == Some(HELD_ITEM))
        .collect();
    assert!(!holder.is_empty(), "the outdoor holder was not submitted");
    assert!(!held.is_empty(), "the held weapon was not submitted");
    assert!(
        holder.iter().all(|d| d.outdoors),
        "the positioned holder missed the outdoor pass"
    );
    assert!(
        held.iter().all(|d| d.outdoors),
        "the positionless held weapon took the interior pass instead of its holder's outdoor pass"
    );
}
