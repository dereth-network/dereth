use dereth_client::character::{Character, CharacterInput, MovementCommands};
use dereth_client_net::client_session::testing::MockTransport;
use dereth_client_net::client_session::{PositionReporter, Session, SessionEvent};
use dereth_physics::math::V3 as _;
use dereth_primitives::{CellId, Frame, LocalTime, ObjectId, Position, Quat, Vec3};
use dereth_protocol::actions::unpack_action;
use dereth_protocol::movement::{MoveTimestamps, MovementAutonomousPosition};
use dereth_protocol::types::PositionWire;
use dereth_protocol::Message;

use super::support::{create_event, store, PLAYER};

pub const LOCAL: ObjectId = ObjectId(0x8000_114F);
pub const REMOTE: ObjectId = ObjectId(0x8000_1150);

/// The reported destination, from the evidence behind this claim's handle.
const OWNER_CELL: u32 = 0x01F9_01E7;
const OWNER_ORIGIN: [f32; 3] = [55.310642, -59.855103, 12.004999];
const OWNER_ROTATION: [f32; 4] = [-0.961667, 0.0, 0.0, 0.274220];

/// What stands on (or beside) the destination, and what the local body's description says.
#[derive(Debug, Clone, Copy)]
pub struct Scene {
    /// The local player's own description bits.
    pub local: u32,
    /// A body standing **exactly** on the destination, with these description bits.
    pub on_the_spot: Option<u32>,
    /// A body this far along the x axis from the destination instead.
    pub beside: Option<(f32, u32)>,
}

impl Scene {
    pub const fn npk_onto(remote: u32) -> Self {
        Self {
            local: PLAYER,
            on_the_spot: Some(remote),
            beside: None,
        }
    }
}

/// What one measured arrival produced.
#[derive(Debug, Clone, Copy)]
pub struct Arrival {
    pub landed: Position,
    pub offset: f32,
    /// What the next outbound position report actually carried.
    pub reported: PositionWire,
}

fn owner_position() -> Position {
    Position::new(
        CellId(OWNER_CELL),
        Frame::new(
            Vec3::new(OWNER_ORIGIN[0], OWNER_ORIGIN[1], OWNER_ORIGIN[2]),
            Quat {
                w: OWNER_ROTATION[0],
                x: OWNER_ROTATION[1],
                y: OWNER_ROTATION[2],
                z: OWNER_ROTATION[3],
            },
        ),
    )
}

/// A settled local body standing on the reported spot, so the destination below is a real
/// ground position and no measurement is absorbing a drop.
fn settled_on_the_owner_spot(
    store: &std::sync::Arc<dereth_dat::RetailDatStore>,
) -> (Character, Position) {
    let region = dereth_client::world::load_region(store).expect("the region decodes");
    let mut c = Character::new(
        store,
        &region,
        dereth_client::world::DEFAULT_LANDBLOCK,
        (96.0, 96.0),
    )
    .expect("the ordinary local body is created");
    let there = owner_position();
    c.land().load_block_cells(there.cell.landblock());
    c.teleport(there);
    for i in 0..=60 {
        c.update(LocalTime(f64::from(i) / 30.0));
    }
    assert!(
        c.on_ground(),
        "the reported destination cell must support a body: {:?}",
        c.position()
    );
    let settled = c.position();
    assert_eq!(
        settled.cell, there.cell,
        "the body left the reported destination cell while settling"
    );
    assert!(
        settled.frame.origin.sub(there.frame.origin).mag2().sqrt() < 1e-3,
        "those coordinates are not a settled standing position: {:?}",
        settled.frame.origin
    );
    (c, settled)
}

/// The whole measurement, through the production path end to end.
pub fn teleport_onto(scene: Scene) -> (Arrival, Position) {
    let store = store();
    let (mut c, destination) = settled_on_the_owner_spot(&store);

    // Stand the local body somewhere else first, so the remote body is placed on an empty
    // destination and the arrival is the only thing being measured.
    let mut away = destination;
    away.frame.origin.x -= 12.0;
    c.teleport(away);
    assert!(
        c.position()
            .frame
            .origin
            .sub(destination.frame.origin)
            .mag2()
            .sqrt()
            > 8.0,
        "the body did not actually leave the destination"
    );

    // The local half, as the connected client gets it.
    let mut stream = dereth_client::objects::ObjectStream::new();
    stream.apply_event(&SessionEvent::PlayerCreated(LOCAL), LocalTime(1.0));
    stream.apply_event(
        &create_event(LOCAL, None, scene.local, "Me"),
        LocalTime(1.0),
    );
    c.adopt_server_id(stream.player());
    stream.sync_physics(&store, &mut c.world);
    let local_weenie = c
        .world
        .get(c.handle)
        .and_then(|o| o.weenie.clone())
        .expect("the local body's weenie half is pushed by the object stream");
    assert_eq!(
        local_weenie.is_player,
        scene.local & PLAYER != 0,
        "the local body's own description is not the one its create carried"
    );

    // The remote half, an ordinary create for a body standing where the other player stood.
    let remote = scene
        .on_the_spot
        .map(|bits| (destination, bits))
        .or_else(|| {
            scene.beside.map(|(d, bits)| {
                let mut at = destination;
                at.frame.origin.x += d;
                (at, bits)
            })
        });
    if let Some((at, bits)) = remote {
        stream.apply_event(
            &create_event(REMOTE, Some(at), bits, "Them"),
            LocalTime(9.0),
        );
        stream.sync_physics(&store, &mut c.world);
        let h = c
            .world
            .by_object_id(REMOTE)
            .expect("the remote body exists");
        let b = c.world.get(h).expect("the remote body");
        assert!(b.cell.is_some(), "the remote body is in no cell");
        let w = b.weenie.as_ref().expect("the remote body's weenie half");
        assert_eq!(
            w.is_player,
            bits & PLAYER != 0,
            "the remote description did not arrive"
        );
        assert!(
            b.position.frame.origin.sub(at.frame.origin).mag2().sqrt() < 1e-3,
            "the remote body was not placed where the shard put it: {:?}",
            b.position.frame.origin
        );
    }

    // The arrival: the production call an accepted teleport reaches.
    let mut movement = MovementCommands::default();
    let mut input = CharacterInput::default();
    dereth_client::app::complete_player_teleport_at(
        destination,
        &mut c,
        &mut movement,
        &mut input,
        |_| {},
    );
    let landed = c.position();
    let offset = landed
        .frame
        .origin
        .sub(destination.frame.origin)
        .mag2()
        .sqrt();

    // And what the client then tells the shard, through the real producer.
    let mut reporter = PositionReporter::new(0.0);
    reporter.active = true;
    let mut session = Session::new(MockTransport::new());
    let motion = dereth_client::app::body_motion(&c, MoveTimestamps::default());
    assert!(
        motion.contact,
        "a settled arrival must be in contact or nothing is reported"
    );
    assert!(
        reporter.should_send_position_event(2.0, &motion),
        "the arrival changed the position, so the schedule must want a report"
    );
    reporter.send_position_event(2.0, &motion, &mut session);
    let blob = session
        .transport
        .sent
        .first()
        .expect("a report was emitted")
        .payload
        .clone();
    let mut action = unpack_action(&blob).expect("the producer emits a game action");
    let sent = MovementAutonomousPosition::read(&mut action.body).expect("the body decodes");
    action
        .body
        .expect_exhausted()
        .expect("the body is fully consumed");

    (
        Arrival {
            landed,
            offset,
            reported: sent.0.position,
        },
        destination,
    )
}

/// Bit for bit on the requested position, and the report says so too.
pub fn landed_exactly(a: &Arrival, destination: Position) -> bool {
    a.landed.cell == destination.cell
        && a.landed.frame.origin.x.to_bits() == destination.frame.origin.x.to_bits()
        && a.landed.frame.origin.y.to_bits() == destination.frame.origin.y.to_bits()
        && a.landed.frame.origin.z.to_bits() == destination.frame.origin.z.to_bits()
        && a.reported.objcell_id == destination.cell.0
        && a.reported.frame.origin.x.to_bits() == destination.frame.origin.x.to_bits()
        && a.reported.frame.origin.y.to_bits() == destination.frame.origin.y.to_bits()
        && a.reported.frame.origin.z.to_bits() == destination.frame.origin.z.to_bits()
}

/// Pushed aside, and the client reports where it really is rather than where it was sent.
pub fn pushed_aside(a: &Arrival, destination: Position) -> bool {
    a.offset > 0.4
        && (a.landed.frame.origin.x - destination.frame.origin.x).abs() < 1e-4
        && (a.reported.frame.origin.y - a.landed.frame.origin.y).abs() < 1e-6
}
