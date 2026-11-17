//! A body taken through sub-steps the client recorded, each from where the client's body began
//! it, with the client's own animation offset and time step, ending each where the client's
//! body ended it. Numbers are `f32` bit patterns, so "the same" means the same to the last bit.

use std::sync::Arc;

use dereth_physics::geom::Sphere;
use dereth_physics::land::VERTEX_COUNT;
use dereth_physics::{
    LandblockCollision, PhysHandle, PhysicsState, PhysicsWorld, Plane, ScriptedMotion,
    SetupGeometry, StaticLandSource, TransientState,
};
use dereth_primitives::{CellId, Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};

/// A vector as the bit patterns of its three components.
pub type Bits3 = [u32; 3];

pub fn v3(b: Bits3) -> Vec3 {
    Vec3::new(
        f32::from_bits(b[0]),
        f32::from_bits(b[1]),
        f32::from_bits(b[2]),
    )
}

pub fn bits(v: Vec3) -> Bits3 {
    [v.x.to_bits(), v.y.to_bits(), v.z.to_bits()]
}

/// The client's human body: two spheres stacked, stepping up 0.6 m and down 1.5 m.
pub fn human() -> Arc<SetupGeometry> {
    Arc::new(SetupGeometry {
        spheres: vec![
            Sphere::new(Vec3::new(0.0, 0.0, 0.475), 0.48),
            Sphere::new(Vec3::new(0.0, 0.0, 1.35), 0.48),
        ],
        sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, 0.9125), 0.9175),
        selection_sphere: Sphere::new(Vec3::new(0.0, 0.0, 0.9175), 0.9175),
        step_up_height: 0.6,
        step_down_height: 1.5,
        radius: 0.678_822_5,
        height: 1.835,
        allow_free_heading: true,
        ..SetupGeometry::default()
    })
}

/// A world of the given landblocks, each by its 81 height-table indices (`x * 9 + y`), on the
/// linear `2 * i` table.
pub fn land(blocks: &[(LandblockId, [u8; VERTEX_COUNT])]) -> PhysicsWorld {
    let mut land = StaticLandSource::linear();
    for (id, heights) in blocks {
        let table = *dereth_physics::LandSource::height_table(&land);
        let block = LandblockCollision::build(
            *id,
            Box::new(*heights),
            Box::new([0; VERTEX_COUNT]),
            false,
            8,
            &table,
        )
        .expect("a full-detail block");
        land.add_block(block);
    }
    PhysicsWorld::new(Arc::new(land))
}

/// Where and how the client's body began the first recorded sub-step.
pub struct Start {
    pub cell: u32,
    pub origin: Bits3,
    pub rotation: [u32; 4],
    pub velocity: Bits3,
    pub acceleration: Bits3,
    pub contact_plane: [u32; 4],
    pub sliding_normal: Bits3,
    pub state: u32,
    pub transient_state: u32,
}

/// One recorded sub-step: its time step and animation offset, and where the client's body ended
/// it.
pub struct SubStep {
    pub quantum: u32,
    pub offset: Bits3,
    pub origin: Bits3,
    pub velocity: Bits3,
    pub cached_velocity: Bits3,
}

/// The human body placed in `world` as the client's began.
pub fn place(world: &mut PhysicsWorld, start: &Start, offsets: Vec<Frame>) -> PhysHandle {
    let cell = CellId(start.cell);
    let h = world.create(ObjectId(0x5000_0001), human(), true);
    world.enter_cell(h, cell);
    let q = start.rotation.map(f32::from_bits);
    let p = start.contact_plane.map(f32::from_bits);
    {
        let o = world.get_mut(h).expect("live");
        o.position = Position::new(
            cell,
            Frame::new(v3(start.origin), Quat::new(q[0], q[1], q[2], q[3])),
        );
        o.state = PhysicsState(start.state);
        o.transient_state = TransientState(start.transient_state);
        o.velocity_vector = v3(start.velocity);
        o.acceleration_vector = v3(start.acceleration);
        o.contact_plane = Plane {
            normal: Vec3::new(p[0], p[1], p[2]),
            d: p[3],
        };
        o.contact_plane_cell_id = cell;
        o.sliding_normal = v3(start.sliding_normal);
        o.elasticity = 0.05;
        o.friction = 0.95;
        o.set_motion(Box::new(ScriptedMotion::new(offsets, true)));
    }
    world.calc_cross_cells(h, false);
    world.set_player(h);
    h
}

/// Take the body through `steps` from `start` and assert each ends where the client's did.
pub fn assert_follows_the_client(world: &mut PhysicsWorld, start: &Start, steps: &[SubStep]) {
    let offsets = steps
        .iter()
        .map(|s| Frame::new(v3(s.offset), Quat::IDENTITY))
        .collect();
    let h = place(world, start, offsets);
    for (i, s) in steps.iter().enumerate() {
        world.get_mut(h).expect("live").update_time = 0.0;
        world.update_object(h, LocalTime(f64::from(f32::from_bits(s.quantum))));
        let o = world.get(h).expect("live");
        assert_eq!(
            (
                bits(o.position.frame.origin),
                bits(o.velocity_vector),
                bits(o.cached_velocity)
            ),
            (s.origin, s.velocity, s.cached_velocity),
            "sub-step {i}: origin {:?}, velocity {:?}, cached velocity {:?}",
            o.position.frame.origin,
            o.velocity_vector,
            o.cached_velocity
        );
    }
}
