//! ACE: Source/ACE.Server/WorldObjects/Creature_Navigation.cs::Rotate
//! Rotate/TurnToObject/TurnTo(Position) broadcast turn motions and face after delay; PositionPack
//! carries velocity and ground contact; heartbeat emotes skip awake creatures; corrected emote
//! motion is broadcast.
//! Fixture: An isolated world with synthetic terrain and decoded position broadcasts.

use std::sync::Arc;

use dereth_protocol::movement::{
    movement_type, position_flags, MoveToArm, MovementParameters, MovementSetObjectMovement,
};
use empyrean_entity::enums::{EmoteCategory, EmoteType, MotionCommand, PropertyBool};
use empyrean_entity::models::properties_emote::PropertiesEmote;
use empyrean_entity::models::properties_emote_action::PropertiesEmoteAction;
use empyrean_entity::ObjectGuid;
use empyrean_world::managers::landblock_manager as lm;
use empyrean_world::network::game_messages::game_message::{start_capture, take_sent};
use empyrean_world::network::structure::position_pack::position_pack_new;
use empyrean_world::physics::phys_ext;
use empyrean_world::world_objects::managers::emote_manager;
use empyrean_world::world_objects::{creature_navigation as cn, monster_awareness};

use super::monster_ai::{lb_id, H, MONSTER, PLAYER};

const UPDATE_MOTION: u32 = 0xF74C;
/// `MovementParams.StopCompletely`.
const STOP_COMPLETELY: u32 = 0x0001_0000;

/// A monster at (100, 100) facing north (identity rotation) and the player 10 m east of it.
fn monster_and_player_east() -> H {
    let mut h = H::new();
    lm::get_landblock(&mut h.w, lb_id(), false, false);
    h.monster(100.0, 100.0);
    // not attackable: nobody wakes it (`AlertMonster`), so it stands as placed
    h.w.objects
        .get_mut(MONSTER)
        .unwrap()
        .set_property(PropertyBool::Attackable, false);
    h.player(110.0, 100.0);
    h.run(1.0);
    assert!(!monster_awareness::is_awake(&h.w, MONSTER));
    h
}

/// The UpdateMotions about `about` the capture holds, decoded.
fn motions_about(about: ObjectGuid) -> Vec<MovementSetObjectMovement> {
    take_sent()
        .into_iter()
        .filter(|(_, _, b)| b[..4] == UPDATE_MOTION.to_le_bytes())
        .map(|(_, _, b)| {
            dereth_protocol::read_body_padded::<MovementSetObjectMovement>(&b[4..])
                .expect("UpdateMotion decodes")
        })
        .filter(|m| m.id.0 == about.full())
        .collect()
}

/// The heading (degrees, 0 north, clockwise) the object's `Location` faces.
fn facing(h: &H, g: ObjectGuid) -> f32 {
    cn::a_frame_get_heading(h.pos(g).rotation())
}

/// `Creature.Rotate`: the TurnToObject broadcast (with StopCompletely), the rotate delay for the
/// angle to the target, and `Location` turned to face the target only once that delay has passed.
#[test]
fn rotate_broadcasts_a_turn_to_object_and_faces_the_target_after_the_delay() {
    let mut h = monster_and_player_east();
    let angle = cn::get_angle(&h.w, MONSTER, PLAYER);
    assert!(
        (angle - 90.0).abs() < 1e-3,
        "the player is 90 degrees off: {angle}"
    );

    start_capture();
    let delay = cn::creature_rotate(&mut h.w, MONSTER, PLAYER);
    assert_eq!(
        delay,
        cn::creature_get_rotate_delay(&h.w, MONSTER, angle),
        "GetRotateDelay(angle)"
    );
    assert!(
        (delay - 1.0).abs() < 1e-3,
        "90 degrees at the table's turn speed (pi/2 per second): {delay}"
    );

    let sent = motions_about(MONSTER);
    assert_eq!(sent.len(), 1, "one TurnToObject broadcast");
    let body = sent[0].decoded_movement().unwrap().body;
    assert_eq!(body.movement_type, movement_type::TURN_TO_OBJECT);
    let Some(MoveToArm::TurnToObject {
        target,
        params: MovementParameters::TurnTo { bitfield, .. },
        ..
    }) = body.decode_move_to().unwrap()
    else {
        panic!("a TurnToObject arm")
    };
    assert_eq!(target.0, PLAYER.full());
    assert_ne!(bitfield & STOP_COMPLETELY, 0, "stopCompletely: true");

    assert!(
        facing(&h, MONSTER).abs() < 1e-3,
        "not turned before the delay"
    );
    h.run(f64::from(delay) - 0.1);
    assert!(
        facing(&h, MONSTER).abs() < 1e-3,
        "not turned before the delay"
    );
    h.run(0.2);
    assert!(
        (facing(&h, MONSTER) - 90.0).abs() < 0.01,
        "faces the player (east): {}",
        facing(&h, MONSTER)
    );
}

/// `Creature.Rotate` with a target that has left the world (ACE: `target == null`): 0 s, no
/// broadcast.
#[test]
fn rotate_to_a_target_gone_does_nothing() {
    let mut h = monster_and_player_east();
    start_capture();
    assert_eq!(
        cn::creature_rotate(&mut h.w, MONSTER, ObjectGuid::new(0x8000_0999)),
        0.0
    );
    assert!(motions_about(MONSTER).is_empty());
}

/// `Creature.TurnToObject(target, stopCompletely: false)` clears StopCompletely.
#[test]
fn turn_to_object_without_stop_completely_clears_the_flag() {
    let mut h = monster_and_player_east();
    start_capture();
    cn::turn_to_object(&mut h.w, MONSTER, PLAYER, false);
    let sent = motions_about(MONSTER);
    let Some(MoveToArm::TurnToObject {
        params: MovementParameters::TurnTo { bitfield, .. },
        ..
    }) = sent[0]
        .decoded_movement()
        .unwrap()
        .body
        .decode_move_to()
        .unwrap()
    else {
        panic!("a TurnToObject arm")
    };
    assert_eq!(bitfield & STOP_COMPLETELY, 0);
}

/// `Creature.TurnTo(Position)` (the emote system's stored headings): a TurnToHeading broadcast
/// whose heading is the position's, the delay for the angle between the two headings, then
/// `Location` and the body both face that heading.
#[test]
fn turn_to_position_broadcasts_a_turn_to_heading_and_turns_location_and_body() {
    let mut h = monster_and_player_east();
    let mut target = h.pos(MONSTER);
    target.rotate(empyrean_common::dotnet::Vector3::new(-1.0, 0.0, 0.0)); // face west
    let heading = cn::a_frame_get_heading(target.rotation());
    assert!((heading - 270.0).abs() < 0.01, "{heading}");
    let angle = cn::get_angle_position(&h.w, MONSTER, &target);
    assert!((angle - 90.0).abs() < 1e-3, "{angle}");

    start_capture();
    let delay = cn::turn_to_position(&mut h.w, MONSTER, &target);
    assert_eq!(delay, cn::creature_get_rotate_delay(&h.w, MONSTER, angle));
    let sent = motions_about(MONSTER);
    assert_eq!(sent.len(), 1);
    let body = sent[0].decoded_movement().unwrap().body;
    assert_eq!(body.movement_type, movement_type::TURN_TO_HEADING);
    let Some(MoveToArm::TurnToHeading {
        params: MovementParameters::TurnTo {
            desired_heading, ..
        },
    }) = body.decode_move_to().unwrap()
    else {
        panic!("a TurnToHeading arm")
    };
    assert!(
        (desired_heading - heading).abs() < 0.01,
        "{desired_heading}"
    );

    h.run(f64::from(delay) + 0.1);
    assert!(
        (facing(&h, MONSTER) - 270.0).abs() < 0.01,
        "Location faces west: {}",
        facing(&h, MONSTER)
    );
    let body_pos = phys_ext::position(&h.w, phys_ext::physics_obj(&h.w, MONSTER).unwrap()).unwrap();
    let r = h.pos(MONSTER).rotation();
    let q = body_pos.frame.rotation;
    assert_eq!(
        (q.w, q.x, q.y, q.z),
        (r.w, r.x, r.y, r.z),
        "PhysicsObj.Position.Frame.Orientation = Location.Rotation"
    );
}

/// `PositionPack`: the body's velocity (`PhysicsObj.Velocity`) and ground contact
/// (`TransientState.OnWalkable`) set HasVelocity and IsGrounded.
#[test]
fn position_pack_carries_the_body_velocity_and_ground_contact() {
    let mut h = monster_and_player_east();
    let body = phys_ext::physics_obj(&h.w, MONSTER).unwrap();
    let p = position_pack_new(&mut h.w, MONSTER, false);
    assert_ne!(
        p.flags.0 & position_flags::IS_GROUNDED,
        0,
        "a creature standing on the ground"
    );
    assert_eq!(p.flags.0 & position_flags::HAS_VELOCITY, 0, "at rest");

    let v = empyrean_common::dotnet::Vector3::new(1.0, 2.0, 0.0);
    phys_ext::set_velocity_field(&mut h.w, body, v);
    let p = position_pack_new(&mut h.w, MONSTER, false);
    assert_ne!(p.flags.0 & position_flags::HAS_VELOCITY, 0);
    assert_eq!(p.velocity, v);
}

/// `EmoteManager.HeartBeat`: an awake creature does no idle emotes (`creature.IsAwake`); asleep,
/// its HeartBeat set's Motion is broadcast (`ExecuteMotion`: an UpdateMotion to the player in
/// range), and the starting motion follows after the animation.
#[test]
fn heartbeat_emotes_skip_an_awake_creature() {
    let mut h = monster_and_player_east();
    let action = PropertiesEmoteAction {
        r#type: EmoteType::Motion.0.cast_unsigned(),
        motion: Some(MotionCommand::Ready),
        extent: 1.0,
        ..PropertiesEmoteAction::default()
    };
    let set = PropertiesEmote {
        category: EmoteCategory::HeartBeat,
        probability: 1.0,
        properties_emote_action: vec![action],
        ..PropertiesEmote::default()
    };
    h.w.objects.get_mut(MONSTER).unwrap().biota.properties_emote = Some(Arc::new(vec![set]));

    // (awake, its monster tick would put it back to sleep: the check is on the call itself)
    start_capture();
    monster_awareness::fields_mut(&mut h.w, MONSTER).is_awake = true;
    emote_manager::heart_beat(&mut h.w, MONSTER);
    assert!(motions_about(MONSTER).is_empty(), "awake: no idle emote");
    monster_awareness::fields_mut(&mut h.w, MONSTER).is_awake = false;

    emote_manager::heart_beat(&mut h.w, MONSTER);
    h.run(1.5);
    let sent = motions_about(MONSTER);
    assert!(
        !sent.is_empty(),
        "asleep: the HeartBeat set's motion is broadcast"
    );
    assert!(sent
        .iter()
        .all(|m| m.decoded_movement().unwrap().body.movement_type == movement_type::INVALID));
}

/// V337/V338 (V338): a HeartBeat motion the world data stores three below retail (0x13000116, a
/// value no command has) is read as retail's WarmHands, and that is the command the idle emote
/// broadcasts (its 16-bit index, 0x119).
#[test]
fn a_corrected_emote_motion_is_the_command_broadcast() {
    use empyrean_content::models::world::{
        Weenie, WeeniePropertiesEmote, WeeniePropertiesEmoteAction,
    };
    use empyrean_content::{MemContent, WorldDatabase};
    use empyrean_entity::enums::WeenieType;

    let stored = WeeniePropertiesEmote {
        id: 49419,
        object_id: 25682,
        category: EmoteCategory::HeartBeat.0.cast_unsigned(),
        probability: 1.0,
        weenie_properties_emote_action: vec![WeeniePropertiesEmoteAction {
            emote_id: 49419,
            r#type: EmoteType::Motion.0.cast_unsigned(),
            extent: 1.0,
            motion: Some(0x1300_0116),
            ..WeeniePropertiesEmoteAction::default()
        }],
        ..WeeniePropertiesEmote::default()
    };
    let mut guard = Weenie::new(25682, "guarddeepplaces", WeenieType::Creature);
    guard.weenie_properties_emote.push(stored);
    let content = MemContent::new().weenie(guard);
    let emotes = content
        .get_cached_weenie(25682)
        .expect("the weenie")
        .properties_emote
        .clone();
    assert_eq!(
        emotes.as_ref().unwrap()[0].properties_emote_action[0].motion,
        Some(MotionCommand::WarmHands)
    );

    let mut h = monster_and_player_east();
    h.w.objects.get_mut(MONSTER).unwrap().biota.properties_emote = emotes;
    start_capture();
    emote_manager::heart_beat(&mut h.w, MONSTER);
    h.run(1.5);
    let commands: Vec<u16> = motions_about(MONSTER)
        .iter()
        .filter_map(|m| m.decoded_movement().unwrap().body.interpreted)
        .flat_map(|s| {
            s.forward_command
                .into_iter()
                .chain(s.actions.into_iter().map(|a| a.command_index))
        })
        .collect();
    assert!(
        commands.contains(&0x119),
        "WarmHands is broadcast: {commands:#X?}"
    );
    assert!(!commands.contains(&0x116), "{commands:#X?}");
}
