//! The camera's own object: a small body the client makes once and never shows. Outdoors, where the
//! lines of sight leave the camera it is put in the world there and moved to the nearest place it
//! has room, then taken out again, and the camera stands where it ended up. So the camera keeps its
//! sphere off the land and off whatever it does not pass through. A place asked for again gets the
//! same answer without asking the world.

use auberean_physics::body::{setup_spheres, Body, BodyConfig};
use auberean_physics::collision::CollisionSettings;
use auberean_physics::land::Land;
use auberean_physics::motion::values::default_values;
use auberean_physics::world::World;
use auberean_primitives::DataId;
use auberean_world::records::WorldRecords;
use dereth_primitives::Position;

/// The enumeration and the mapping that name the camera object's record.
const CAMERA_OBJECT_RECORD: (u32, u32) = (0x4000_002A, 0x4000_0002);

/// The camera's own object.
pub(crate) struct CameraObject {
    body: Body,
    /// The place last asked for, and where that left the camera.
    asked: Option<(Position, Position)>,
}

impl CameraObject {
    /// Made from the record the data names for it, under the temporary id `id`; `None` with no
    /// such record.
    // D2-10667
    pub(crate) fn new(records: &WorldRecords, id: u64) -> Option<Self> {
        let record = records.data_id_by_enum(CAMERA_OBJECT_RECORD.0, CAMERA_OBJECT_RECORD.1);
        let setup = records.loaded_setup(DataId(record))?;
        let still = default_values();
        let mut body = Body::new(
            BodyConfig {
                spheres: setup_spheres(&setup.get()),
                step_up: 0.0,
                step_down: 0.0,
                scale: 1.0,
                ground: still,
                air: still,
            },
            Position::new(
                dereth_primitives::CellId(0),
                dereth_primitives::Frame::new(
                    dereth_primitives::Vec3::ZERO,
                    dereth_primitives::Quat::IDENTITY,
                ),
            ),
            0.0,
        );
        body.id = id;
        body.is_player = false;
        body.in_world = false;
        Some(Self { body, asked: None })
    }

    /// Where the camera stands, asked to stand at `at` outdoors: where the object ends up put in
    /// the world there, sliding to room; `at` while it has never been placed.
    // D2-10662, D2-06369
    pub(crate) fn stand(&mut self, land: &Land, world: &mut World, at: Position) -> Position {
        if let Some((asked, stands)) = self.asked {
            if asked == at {
                return stands;
            }
        }
        let key = match self.body.key().filter(|k| world.position(*k).is_some()) {
            Some(k) => k,
            None => {
                let passes = world.camera_ethereal();
                let k = world.insert_body(land, self.body.id, &mut self.body);
                let mut solid = auberean_physics::ethereal::ObjectEthereality::default();
                solid.set_to_solid();
                let mut collision = CollisionSettings::default();
                collision.set_physics_placement_type(passes);
                collision.set_weenie_placement_type(&solid);
                collision.set_physics_movement_type(passes);
                collision.set_weenie_movement_type(&solid);
                world.set_collision_state(k, &collision);
                k
            }
        };
        let _ = self.body.set_position_simple(land, world, at, true);
        // Out of the world again: the camera's lines of sight do not meet its own object.
        world.remove(key);
        let stands = if self.body.position.cell.0 == 0 {
            at
        } else {
            self.body.position
        };
        self.asked = Some((at, stands));
        stands
    }
}
