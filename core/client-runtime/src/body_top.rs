//! The top of a body as it is drawn this frame: the highest point of the meshes its parts draw,
//! where the parts stand. It is where an interface puts a name over a person or a creature.
//!
//! A setup's own measures are not that top. Its height and its selection sphere are what physics
//! and the selection marker read, and a body's meshes can stand well over or under both: a lugian's
//! head stands half a metre over its setup's height and its selection sphere, and a cow's selection
//! sphere reaches a metre over its back. The meshes are what the player sees, so the top is
//! measured on them, posed as they are drawn and scaled as the object is.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use dereth_animation::parts::PhysicsPart;
use dereth_assets::{Decode, GfxObj};
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::{DataId, Vec3};

/// The points each graphics object's drawn polygons are made of, read from the files the first
/// time a part draws the object and kept: the same few dozen objects make every body in view.
#[derive(Debug, Default)]
pub struct DrawnPoints {
    memo: Mutex<BTreeMap<u32, Option<Arc<[Vec3]>>>>,
}

impl DrawnPoints {
    /// The points of graphics object `id`'s drawn polygons, each once, in its own space. `None`
    /// for an object the files do not hold, or one that draws nothing.
    pub fn of(&self, store: &RetailDatStore, id: DataId) -> Option<Arc<[Vec3]>> {
        if let Ok(memo) = self.memo.lock() {
            if let Some(hit) = memo.get(&id.0) {
                return hit.clone();
            }
        }
        let points = store
            .read_typed(DbType::GfxObj, id)
            .ok()
            .and_then(|bytes| GfxObj::decode_payload_in(store.era_of(id), id, &bytes).ok())
            .map(|g| drawn_points(&g))
            .filter(|p| !p.is_empty())
            .map(Arc::from);
        if let Ok(mut memo) = self.memo.lock() {
            memo.insert(id.0, points.clone());
        }
        points
    }
}

/// The points `g`'s drawn polygons use, each once.
fn drawn_points(g: &GfxObj) -> Vec<Vec3> {
    let mut used: Vec<u16> = g
        .polygons
        .iter()
        .flat_map(|p| p.vertex_ids.iter().copied())
        .collect();
    used.sort_unstable();
    used.dedup();
    used.into_iter()
        .filter_map(|i| g.vertex_array.vertices.get(usize::from(i)))
        .map(|v| v.position)
        .collect()
}

/// The height of the highest point `parts` draw, each placed at its own frame and scaled by its
/// own scale, the points of each part's graphics object given by `points`. `None` when no part
/// draws anything `points` knows.
pub fn top_of_parts(
    parts: &[PhysicsPart],
    points: impl Fn(DataId) -> Option<Arc<[Vec3]>>,
) -> Option<f32> {
    let mut top: Option<f32> = None;
    for part in parts {
        if part.no_draw() || part.gfxobj_id.0 == 0 {
            continue;
        }
        let Some(points) = points(part.gfxobj_id) else {
            continue;
        };
        // The world height of a point in the part's own space is the part's height plus the
        // rotated, scaled point's: the rotation's third row against the scaled point.
        let m = dereth_primitives::frame::l2g(part.pos.rotation).0;
        let s = part.gfxobj_scale;
        let (zx, zy, zz) = (m[2] * s.x, m[5] * s.y, m[8] * s.z);
        let highest = points
            .iter()
            .map(|p| zx * p.x + zy * p.y + zz * p.z)
            .fold(f32::MIN, f32::max);
        let z = part.pos.origin.z + highest;
        top = Some(top.map_or(z, |t| t.max(z)));
    }
    top
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::{Frame, Quat};

    fn part(gfxobj: u32, at: Frame, scale: f32) -> PhysicsPart {
        let mut p = PhysicsPart::new(DataId(gfxobj));
        p.pos = at;
        p.gfxobj_scale = Vec3::new(scale, scale, scale);
        p
    }

    /// A unit cube's corners, centred on its own origin.
    fn cube(_: DataId) -> Option<Arc<[Vec3]>> {
        let mut v = Vec::new();
        for x in [-0.5, 0.5] {
            for y in [-0.5, 0.5] {
                for z in [-0.5, 0.5] {
                    v.push(Vec3::new(x, y, z));
                }
            }
        }
        Some(Arc::from(v))
    }

    #[test]
    fn the_top_is_the_highest_point_of_any_part_where_it_stands_scaled_and_turned() {
        let low = part(1, Frame::new(Vec3::new(0.0, 0.0, 1.0), Quat::IDENTITY), 1.0);
        let high = part(1, Frame::new(Vec3::new(3.0, 0.0, 2.0), Quat::IDENTITY), 2.0);
        // The cube's top at 2 + 0.5 * 2.
        assert_eq!(top_of_parts(&[low.clone(), high], cube), Some(3.0));
        // Turned an eighth of a turn about the east axis, a unit cube's highest corner stands
        // half its diagonal across the turn over its middle.
        let half = std::f32::consts::FRAC_PI_8;
        let r = Quat::new(
            dereth_primitives::num::math::cosf(half),
            dereth_primitives::num::math::sinf(half),
            0.0,
            0.0,
        );
        let turned = part(1, Frame::new(Vec3::new(0.0, 0.0, 1.0), r), 1.0);
        let top = top_of_parts(&[turned], cube).expect("a top");
        assert!(
            (top - (1.0 + std::f32::consts::FRAC_1_SQRT_2)).abs() < 1e-5,
            "{top}"
        );
        // A part not drawn, or with nothing to draw, is not measured.
        let mut hidden = low.clone();
        hidden.set_no_draw(true);
        let empty = part(0, Frame::new(Vec3::new(0.0, 0.0, 9.0), Quat::IDENTITY), 1.0);
        assert_eq!(top_of_parts(&[hidden, empty.clone()], cube), None);
        assert_eq!(top_of_parts(&[low, empty], cube), Some(1.5));
    }

    use crate::world_state::WorldState;
    use dereth_primitives::{LocalTime, ObjectId, Position};

    /// Holtburg with the player's body standing in it, stepped a frame at a time as the client
    /// steps the world: the server's objects, their bodies, then the world.
    struct Holtburg {
        store: Arc<RetailDatStore>,
        cfg: crate::scene::SceneConfig,
        ws: WorldState,
        residency: crate::world_build::BlockResidency,
        stream: crate::objects::ObjectStream,
        counters: crate::world_objects::ObjectCounters,
        stats: crate::world_step::StepCounters,
        now: f64,
        fps: f64,
    }

    /// A body the server makes: its setup, its motions and its scale, where it stands from the
    /// player in his own axes (right, ahead), and how it moves.
    struct Body {
        id: ObjectId,
        setup: u32,
        motions: u32,
        scale: Option<f32>,
        at: (f32, f32),
        run: bool,
    }

    const HUMAN: (u32, u32) = (0x0200_0001, 0x0900_0001);
    const LUGIAN: (u32, u32) = (0x0200_0A0B, 0x0900_0006);
    const COW: (u32, u32) = (0x0200_0006, 0x0900_000D);

    impl Holtburg {
        fn new(fps: f64, smooth: bool) -> Self {
            let store = Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
                panic!(
                    "the retail dats are this test's oracle and they are not under {}",
                    dereth_dat::testing::dat_dir().display()
                )
            }));
            let cfg = crate::scene::SceneConfig {
                landblock: 0xA9B4,
                character: true,
                ..crate::scene::SceneConfig::default()
            };
            let (mut ws, residency) =
                crate::world_build::load(&store, &cfg).expect("the world loads");
            ws.smooth_movement = smooth;
            let mut h = Self {
                store,
                cfg,
                ws,
                residency,
                stream: crate::objects::ObjectStream::new(),
                counters: crate::world_objects::ObjectCounters::default(),
                stats: crate::world_step::StepCounters::default(),
                now: 1.0,
                fps,
            };
            for _ in 0..30 {
                h.frame();
            }
            h
        }

        fn frame(&mut self) {
            self.now += 1.0 / self.fps;
            crate::world_objects::sync_objects(
                &mut self.ws,
                &self.store,
                &self.cfg,
                &mut self.stream,
                &mut self.counters,
                &mut crate::world_objects::NoAppearance,
            )
            .expect("the objects synchronise");
            if let Some(c) = self.ws.character.as_mut() {
                self.stream
                    .sync_physics_at(&self.store, &mut c.world, LocalTime(self.now));
            }
            #[allow(clippy::cast_possible_truncation)]
            crate::world_step::update(
                &mut self.ws,
                &mut self.residency,
                &self.cfg,
                crate::camera::CameraInput::default(),
                crate::character::CharacterInput::default(),
                LocalTime(self.now),
                (1.0 / self.fps) as f32,
                &mut self.stats,
            );
            self.residency.stream(&mut self.ws, &self.store, &self.cfg);
        }

        /// The server making `b` beside the player, facing his way, and it standing a second;
        /// then set running, if it runs.
        fn make(&mut self, bodies: &[Body]) {
            use dereth_protocol::types::physicsdesc::flags;
            let at = self.ws.character.as_ref().expect("a body").position();
            let axes = dereth_primitives::frame::l2g(at.frame.rotation);
            for b in bodies {
                let v = dereth_primitives::frame::localtoglobalvec(
                    axes,
                    Vec3::new(b.at.0, b.at.1, 0.0),
                );
                let o = at.frame.origin;
                let here = Position::new(
                    at.cell,
                    dereth_primitives::Frame::new(
                        Vec3::new(o.x + v.x, o.y + v.y, o.z),
                        at.frame.rotation,
                    ),
                );
                let mut bitfield = flags::POSITION | flags::SETUP | flags::MTABLE;
                if b.scale.is_some() {
                    bitfield |= flags::OBJSCALE;
                }
                let payload = dereth_protocol::objects::ObjectCreatePayload {
                    id: b.id,
                    physicsdesc: dereth_protocol::types::PhysicsDesc {
                        bitfield,
                        // Solid, and falling to the ground.
                        state: 0x0000_0408,
                        setup_id: Some(b.setup),
                        mtable_id: Some(b.motions),
                        object_scale: b.scale,
                        position: Some(dereth_protocol::types::PositionWire {
                            objcell_id: here.cell.0,
                            frame: dereth_protocol::types::Frame {
                                origin: here.frame.origin.into(),
                                orientation: here.frame.rotation.into(),
                            },
                        }),
                        timestamps: dereth_protocol::types::PhysicsTimestamps {
                            instance: 1,
                            ..dereth_protocol::types::PhysicsTimestamps::default()
                        },
                        ..dereth_protocol::types::PhysicsDesc::default()
                    },
                    ..Default::default()
                };
                let body = dereth_protocol::write_body(
                    &dereth_protocol::objects::ItemCreateObject(payload),
                )
                .expect("encode");
                self.stream.apply_event(
                    &dereth_client_net::client_session::SessionEvent::WorldObject {
                        opcode: dereth_protocol::Opcode::ITEM_CREATE_OBJECT,
                        body,
                    },
                    LocalTime(self.now),
                );
            }
            self.stream.world.update_visible_object_list();
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            for _ in 0..self.fps as usize {
                self.frame();
            }
            for b in bodies.iter().filter(|b| b.run) {
                let o = self.ws.objects.get(&b.id).expect("the body is made");
                o.sim.driver.borrow_mut().move_to_interpreted_state(
                    &dereth_animation::motion::interp::InterpretedMotionState {
                        forward_command: dereth_animation::MotionCommand::RUN_FORWARD,
                        forward_speed: 2.5,
                        ..Default::default()
                    },
                    false,
                );
            }
        }

        /// The highest point the parts of `id` draw, measured the long way: each drawn
        /// polygon's every point carried through its part's whole frame and scale.
        fn highest_drawn_point(&self, id: ObjectId) -> f32 {
            let o = self.ws.objects.get(&id).expect("the body");
            let driver = o.sim.driver.borrow();
            let mut top = f32::MIN;
            for part in driver.part_array.parts.iter().filter(|p| !p.no_draw()) {
                let g = part.gfxobj_id;
                let Some(g) = self
                    .store
                    .read_typed(DbType::GfxObj, g)
                    .ok()
                    .and_then(|b| GfxObj::decode_payload_in(self.store.era_of(g), g, &b).ok())
                else {
                    continue;
                };
                for p in &g.polygons {
                    for v in &p.vertex_ids {
                        let v = g.vertex_array.vertices[usize::from(*v)].position;
                        let s = part.gfxobj_scale;
                        let w = dereth_primitives::frame::localtoglobal(
                            &part.pos,
                            Vec3::new(v.x * s.x, v.y * s.y, v.z * s.z),
                        );
                        top = top.max(w.z);
                    }
                }
            }
            top
        }

        /// The top of `id`'s selection sphere, and its setup's height over its feet, as drawn.
        fn setup_measures(&self, id: ObjectId) -> (f32, f32) {
            let o = self.ws.objects.get(&id).expect("the body");
            let driver = o.sim.driver.borrow();
            let s = driver.part_array.selection_sphere();
            (
                dereth_primitives::frame::localtoglobal(&o.frame, s.center).z + s.radius,
                o.frame.origin.z + driver.part_array.height(),
            )
        }
    }

    const MAN: ObjectId = ObjectId(0x8300_0501);
    const BIG_LUGIAN: ObjectId = ObjectId(0x8300_0502);
    const LUGIAN_AT_ONE: ObjectId = ObjectId(0x8300_0503);
    const A_COW: ObjectId = ObjectId(0x8300_0504);

    fn body(
        id: ObjectId,
        (setup, motions): (u32, u32),
        scale: Option<f32>,
        at: (f32, f32),
    ) -> Body {
        Body {
            id,
            setup,
            motions,
            scale,
            at,
            run: false,
        }
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn a_bodys_top_is_the_highest_point_its_parts_draw_scaled_as_the_body_is() {
        let mut h = Holtburg::new(60.0, false);
        h.make(&[
            body(MAN, HUMAN, None, (-3.0, 6.0)),
            body(LUGIAN_AT_ONE, LUGIAN, None, (0.0, 6.0)),
            body(BIG_LUGIAN, LUGIAN, Some(1.3), (3.0, 6.0)),
            body(A_COW, COW, None, (7.0, 6.0)),
        ]);
        let height = |h: &Holtburg, id| {
            let top = h.ws.drawn_top(id).expect("drawn");
            let feet = h.ws.objects[&id].frame.origin.z;
            (top, feet)
        };
        for id in [MAN, LUGIAN_AT_ONE, BIG_LUGIAN, A_COW] {
            let (top, _) = height(&h, id);
            let drawn = h.ws.objects[&id].frame.origin;
            assert_eq!(
                (top.x, top.y),
                (drawn.x, drawn.y),
                "{id:?}: over its origin"
            );
            let highest = h.highest_drawn_point(id);
            assert!(
                (top.z - highest).abs() < 1e-3,
                "{id:?}: the top {} against the highest drawn point {highest}",
                top.z
            );
        }
        // A lugian's head stands well over its setup's height and its selection sphere, the
        // larger one's by as much more as it is larger.
        for id in [LUGIAN_AT_ONE, BIG_LUGIAN] {
            let (top, _) = height(&h, id);
            let (sphere, setup) = h.setup_measures(id);
            assert!(
                top.z > sphere + 0.4 && top.z > setup + 0.4,
                "{id:?}: its top {} against its selection sphere's {sphere} and its height {setup}",
                top.z
            );
        }
        let (small, small_feet) = height(&h, LUGIAN_AT_ONE);
        let (big, big_feet) = height(&h, BIG_LUGIAN);
        let ratio = (big.z - big_feet) / (small.z - small_feet);
        assert!(
            (ratio - 1.3).abs() < 0.03,
            "the lugian made larger stands {ratio} times as tall"
        );
        assert!(
            big.z - big_feet > 3.0,
            "a lugian made larger stands over three metres"
        );
        // A cow's selection sphere reaches far over its back.
        let (cow, _) = height(&h, A_COW);
        let (sphere, _) = h.setup_measures(A_COW);
        assert!(
            cow.z < sphere - 0.5,
            "the cow's top {} against its selection sphere's {sphere}",
            cow.z
        );
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn drawn_between_ticks_or_not_a_running_bodys_top_stands_over_where_it_is_drawn() {
        for smooth in [false, true] {
            let mut h = Holtburg::new(60.0, smooth);
            h.make(&[Body {
                run: true,
                ..body(MAN, HUMAN, None, (3.0, 2.0))
            }]);
            let mut off_physics = 0;
            for n in 0..60 {
                h.frame();
                let o = &h.ws.objects[&MAN];
                let drawn = o.frame.origin;
                let top = h.ws.drawn_top(MAN).expect("drawn");
                assert_eq!(
                    (top.x, top.y),
                    (drawn.x, drawn.y),
                    "smooth {smooth}, frame {n}: over where it is drawn"
                );
                let highest = h.highest_drawn_point(MAN);
                assert!(
                    (top.z - highest).abs() < 1e-3,
                    "smooth {smooth}, frame {n}: the top {} against the highest drawn point \
                     {highest}",
                    top.z
                );
                let physics = crate::world_step::render_frame_of(
                    &h.ws,
                    h.cfg.landblock,
                    o.sim.position.expect("placed"),
                )
                .origin;
                if (physics.x - drawn.x).abs() + (physics.y - drawn.y).abs() > 0.01 {
                    off_physics += 1;
                }
            }
            assert_eq!(
                off_physics > 0,
                smooth,
                "drawn between ticks, and only then, it is drawn off where physics has it"
            );
        }
    }
}
