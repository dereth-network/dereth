//! A storage chest, a corpse and a house hook stay exactly where the server put them through two
//! seconds of frames, with the state words ACE sends them (a chest and a corpse fall under gravity,
//! a hook does not): a hand's breadth above the ground, and a hand's breadth into it. The same
//! objects without what makes them a chest, a corpse or a hook, sent into the ground, are put back
//! on top of it at once.
//!
//! Fixture: the device-free application over the retail dats, in its default static scene with the
//! local body, the objects created through the object stream's own `0xF745 Item_CreateObject`
//! handling a few metres from the body, and the frames the application itself runs.

use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::{CellId, LocalTime, ObjectId, Position, Vec3};
use dereth_protocol::objects::{ItemCreateObject, ObjectCreatePayload};
use dereth_protocol::types::physicsdesc::flags;
use dereth_protocol::types::weeniedesc::header;
use dereth_protocol::types::{PhysicsDesc, PositionWire, PublicWeenieDesc};
use dereth_protocol::Message;

use crate::common::sim_app;

/// How far from the ground the server puts each object: above it, as the world database records a
/// storage chest above its floor, or into it.
const ABOVE: f32 = 0.1;
/// Two seconds of the application's frames.
const FRAMES: usize = 120;

/// One object: its setup, scale, state word and public description.
struct Kind {
    name: &'static str,
    setup: u32,
    scale: f32,
    state: u32,
    wcid: u32,
    bitfield: u32,
    hook: Option<(u16, u32)>,
}

/// The storage chest's weenie class, the portal dat's `STORAGE` entry.
const STORAGE_CLASS: u32 = 9687;

/// ACE's three kinds, with the setups, scales and state words it sends for them: a storage chest
/// (`GRAVITY | IGNORE_COLLISIONS | REPORT_COLLISIONS`), a decorative Lugian corpse
/// (`GRAVITY | IGNORE_COLLISIONS | ETHEREAL`) and an empty wall hook (`IGNORE_COLLISIONS |
/// ETHEREAL`, no gravity).
fn kinds() -> [Kind; 3] {
    [
        Kind {
            name: "a storage chest",
            setup: 0x0200_0A97,
            scale: 1.0,
            state: 0x418,
            wcid: STORAGE_CLASS,
            bitfield: 0,
            hook: None,
        },
        Kind {
            name: "a corpse",
            setup: 0x0200_0F9C,
            scale: 1.0,
            state: 0x414,
            wcid: 25457,
            bitfield: 0x2000,
            hook: None,
        },
        Kind {
            name: "a wall hook",
            setup: 0x0200_0A8E,
            scale: 0.5,
            state: 0x14,
            wcid: 9686,
            bitfield: 0,
            hook: Some((2, u32::MAX)),
        },
    ]
}

/// The same object without the one fact that makes it that kind: the class after the storage
/// chest's, no corpse bit, a hook that takes no item types.
fn ordinary(k: &Kind) -> Kind {
    Kind {
        name: k.name,
        setup: k.setup,
        scale: k.scale,
        state: k.state,
        wcid: if k.wcid == STORAGE_CLASS {
            STORAGE_CLASS + 1
        } else {
            k.wcid
        },
        bitfield: 0,
        hook: k.hook.map(|(t, _)| (t, 0)),
    }
}

fn create(id: ObjectId, at: Position, k: &Kind) -> SessionEvent {
    let payload = ObjectCreatePayload {
        id,
        objdesc: Default::default(),
        physicsdesc: PhysicsDesc {
            bitfield: flags::POSITION | flags::SETUP | flags::OBJSCALE,
            setup_id: Some(k.setup),
            object_scale: Some(k.scale),
            position: Some(PositionWire {
                objcell_id: at.cell.raw(),
                frame: dereth_protocol::types::Frame {
                    origin: at.frame.origin.into(),
                    orientation: at.frame.rotation.into(),
                },
            }),
            state: k.state,
            ..Default::default()
        },
        wdesc: PublicWeenieDesc {
            header: k
                .hook
                .map_or(0, |_| header::HOOK_TYPE | header::HOOK_ITEM_TYPES),
            name: k.name.to_owned(),
            wcid: k.wcid,
            bitfield: k.bitfield,
            hook_type: k.hook.map(|h| h.0),
            hook_item_types: k.hook.map(|h| h.1),
            ..Default::default()
        },
    };
    let body = dereth_protocol::write_body(&ItemCreateObject(payload)).expect("the create encodes");
    SessionEvent::WorldObject {
        opcode: ItemCreateObject::OPCODE,
        body,
    }
}

/// Where the object's collision body stands, if it is in a cell.
fn standing(app: &dereth_client::app::App, id: ObjectId) -> Option<Vec3> {
    let c = sim_app::body(app);
    let h = c.world.by_object_id(id)?;
    let o = c.world.get(h)?;
    o.cell.map(|_| o.position.frame.origin)
}

/// Behaviour: movement.remote-body.a-hook-a-storage-chest-or-a-corpse-stays-where-it-was-put-down
#[test]
fn a_chest_a_corpse_and_a_hook_stay_where_they_were_sent_frame_after_frame() {
    let mut app = sim_app::app_in_gameplay(60);
    sim_app::frames(&mut app, 30);
    let here = sim_app::position(&app);
    let block = here.cell.landblock();
    let land = std::sync::Arc::clone(sim_app::body(&app).land());

    // Spots three metres apart along x, clear of the body and of each other, `dz` from the ground.
    let spot = |i: usize, dz: f32| -> Position {
        #[allow(clippy::cast_precision_loss)]
        let x = here.frame.origin.x + 4.0 + 3.0 * i as f32;
        let y = here.frame.origin.y;
        assert!(
            (1.0..191.0).contains(&x),
            "spot {i} is off the body's landblock"
        );
        let ground = land
            .ground_height(block, x, y)
            .expect("the body's landblock is loaded");
        let mut cell: CellId = here.cell;
        let mut origin = Vec3::new(x, y, ground + dz);
        assert!(dereth_physics::landdefs::adjust_to_outside(
            &mut cell,
            &mut origin
        ));
        Position::new(
            cell,
            dereth_primitives::Frame::new(origin, here.frame.rotation),
        )
    };

    // Each kind above the ground and in it, and made ordinary in it.
    let mut placed = Vec::new();
    for (i, k) in kinds().iter().enumerate() {
        let ordinary = ordinary(k);
        for (j, (which, dz, as_sent)) in [
            (k, ABOVE, true),
            (k, -ABOVE, true),
            (&ordinary, -ABOVE, false),
        ]
        .into_iter()
        .enumerate()
        {
            let n = 3 * i + j;
            #[allow(clippy::cast_possible_truncation)]
            let id = ObjectId(0x7000_0100 + n as u32);
            let at = spot(n, dz);
            app.probe_mut()
                .objects_mut()
                .apply_event(&create(id, at, which), LocalTime(2.0));
            placed.push((k.name, as_sent, dz, id, at.frame.origin));
        }
    }

    // The worst distance each object stands from where it was sent, over every frame.
    let mut worst = vec![0.0_f32; placed.len()];
    let mut last = vec![Vec3::ZERO; placed.len()];
    for _ in 0..FRAMES {
        sim_app::frames(&mut app, 1);
        for (k, (_, _, _, id, sent)) in placed.iter().enumerate() {
            let now = standing(&app, *id).expect("every object is in a cell");
            let d =
                ((now.x - sent.x).powi(2) + (now.y - sent.y).powi(2) + (now.z - sent.z).powi(2))
                    .sqrt();
            worst[k] = worst[k].max(d);
            last[k] = now;
        }
    }
    for (k, (name, as_sent, dz, _, sent)) in placed.iter().enumerate() {
        let raised = last[k].z - sent.z;
        println!(
            "{name} ({}, sent {dz:+.1} m from the ground): at most {:.4} m from where it was sent \
             over {FRAMES} frames, {raised:+.4} m at the end",
            if *as_sent {
                "as itself"
            } else {
                "without what makes it one"
            },
            worst[k]
        );
        if *as_sent {
            assert!(
                worst[k] < 1e-4,
                "{name} moved {:.4} m from where it was sent",
                worst[k]
            );
        } else {
            assert!(
                raised > 0.05 && raised < 0.2,
                "{name}, made ordinary, ended {raised:+.4} m from where it was sent into the \
                 ground, not on top of it"
            );
        }
    }
}
