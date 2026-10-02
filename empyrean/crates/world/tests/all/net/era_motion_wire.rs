//! Divergence: V391
//! A world on older data files puts its motion commands on the wire in its files' numbering, as
//! the client of their day numbered them, and reads the client's in it: the stance, the
//! interpreted commands and the queued actions go out as the older indices, a queued action the
//! older numbering lacks is left out, and a client's raw motion state and actions are read back
//! into the final numbering. Under the final numbering every byte is as before.
//! Fixture: explicit expected values.

use dereth_world_data::command_numbering::CommandNumbering;
use empyrean_common::dotnet::binary_reader::BinaryReader;
use empyrean_entity::enums::{MotionCommand, MotionStance};
use empyrean_entity::ObjectGuid;
use empyrean_world::network::motion::motion_item::MotionItem;
use empyrean_world::network::motion::movement_data::{self, Motion, MovementData};
use empyrean_world::network::motion::raw_motion_state::RawMotionState;
use empyrean_world::network::sequence::sequence_manager::SequenceManager;

const WO: ObjectGuid = ObjectGuid::new(0x5000_0001);

/// The bytes of a motion with `forward` as its forward command and `actions` queued, in `stance`,
/// written in `numbering` with no header.
fn written(
    stance: MotionStance,
    forward: MotionCommand,
    actions: &[MotionCommand],
    numbering: CommandNumbering,
) -> Vec<u8> {
    let mut motion = Motion::new(stance, forward, 1.0);
    for a in actions {
        motion.motion_state.add_command(WO, *a, 1.0);
    }
    let data = MovementData::from_motion(WO, &motion);
    let mut out = Vec::new();
    movement_data::write(
        &mut out,
        &data,
        false,
        &mut SequenceManager::new(),
        numbering,
    );
    out
}

/// The `u16` at `at`.
fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

/// The motion's layout: movement type, flags, the stance (2 bytes), then the interpreted
/// state's flags dword (commands count in bits 7 on), its stance and forward command, then each
/// queued command (index, sequence, speed).
#[test]
fn an_older_world_sends_the_logout_and_an_atlatl_stance_as_its_own_indices() {
    let now = written(
        MotionStance::AtlatlCombat,
        MotionCommand::Ready,
        &[MotionCommand::LogOut],
        CommandNumbering::Final,
    );
    let then = written(
        MotionStance::AtlatlCombat,
        MotionCommand::Ready,
        &[MotionCommand::LogOut],
        CommandNumbering::Before2015,
    );
    assert_eq!(now.len(), then.len());
    assert_eq!(u16_at(&now, 2), 0x13B, "the final AtlatlCombat");
    assert_eq!(u16_at(&then, 2), 0x138, "the older AtlatlCombat");
    let flags = u32::from_le_bytes([then[4], then[5], then[6], then[7]]);
    assert_eq!(flags >> 7, 1, "one queued command");
    assert_eq!(u16_at(&then, 8), 0x138, "the interpreted stance");
    assert_eq!(u16_at(&then, 10), 0x003, "Ready keeps its index");
    assert_eq!(u16_at(&now, 12), 0x11E, "the final LogOut");
    assert_eq!(u16_at(&then, 12), 0x11B, "the older LogOut");
}

#[test]
fn a_queued_command_the_older_numbering_lacks_is_left_out() {
    let then = written(
        MotionStance::NonCombat,
        MotionCommand::Ready,
        &[MotionCommand::CombatEat, MotionCommand::Wave],
        CommandNumbering::Before2015,
    );
    let flags = u32::from_le_bytes([then[4], then[5], then[6], then[7]]);
    assert_eq!(flags >> 7, 1, "CombatEat is the final client's alone");
    let first = then.len() - 8;
    assert_eq!(u16_at(&then, first), 0x87, "Wave");
}

#[test]
fn an_older_clients_motion_state_and_actions_are_read_into_the_final_numbering() {
    // Flags: style and forward command, one action. SitState and AtlatlCombat in the older
    // numbering, then a queued older LogOut.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(0x2_u32 | 0x4 | 1 << 11).to_le_bytes());
    bytes.extend_from_slice(&0x8000_0138_u32.to_le_bytes());
    bytes.extend_from_slice(&0x4300_013A_u32.to_le_bytes());
    bytes.extend_from_slice(&0x011B_u16.to_le_bytes());
    bytes.extend_from_slice(&[1, 0, 0, 0, 0x80, 0x3F]);
    let s = RawMotionState::read(
        WO,
        &mut BinaryReader::new(&bytes),
        CommandNumbering::Before2015,
    )
    .expect("reads");
    assert_eq!(s.current_style, MotionStance::AtlatlCombat);
    assert_eq!(s.forward_command, MotionCommand::SitState);

    let item = MotionItem::read(
        WO,
        &mut BinaryReader::new(&bytes[12..]),
        CommandNumbering::Before2015,
    )
    .expect("reads");
    assert_eq!(item.motion_command, MotionCommand::LogOut);
    let same = MotionItem::read(
        WO,
        &mut BinaryReader::new(&bytes[12..]),
        CommandNumbering::Final,
    )
    .expect("reads");
    assert_eq!(
        same.motion_command,
        MotionCommand::AFKState,
        "the final numbering reads 0x11B as itself"
    );
}
