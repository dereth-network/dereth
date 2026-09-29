//! Message structure: opcodes, labels, and where a message names its object. Layouts are the ones
//! ACE's `GameMessage*` writers produce (`Source/ACE.Server/Network/GameMessages/Messages`).

/// A little-endian `u32` at `at`, if the payload is long enough.
#[must_use]
pub fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// A little-endian `u16` at `at`, if the payload is long enough.
#[must_use]
pub fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    b.get(at..at + 2).map(|s| u16::from_le_bytes([s[0], s[1]]))
}

/// A little-endian `f32` at `at`.
#[must_use]
pub fn f32_at(b: &[u8], at: usize) -> Option<f32> {
    u32_at(b, at).map(f32::from_bits)
}

pub const GAME_EVENT: u32 = 0xF7B0;
pub const GAME_ACTION: u32 = 0xF7B1;
pub const CREATE_OBJECT: u32 = 0xF745;
pub const PLAYER_CREATE: u32 = 0xF746;
pub const DELETE_OBJECT: u32 = 0xF747;
pub const UPDATE_POSITION: u32 = 0xF748;
pub const PARENT_EVENT: u32 = 0xF749;
pub const SET_STATE: u32 = 0xF74B;
pub const UPDATE_MOTION: u32 = 0xF74C;
pub const VECTOR_UPDATE: u32 = 0xF74E;
pub const PLAYER_TELEPORT: u32 = 0xF751;
pub const UPDATE_OBJECT: u32 = 0xF7DB;
pub const LOGOFF: u32 = 0xF653;
pub const CHARACTER_LIST: u32 = 0xF658;
pub const CHAR_GEN: u32 = 0xF656;
pub const CHAR_GEN_RESPONSE: u32 = 0xF643;
pub const ENTER_WORLD: u32 = 0xF657;
pub const ENTER_WORLD_REQUEST: u32 = 0xF7C8;
pub const SERVER_READY: u32 = 0xF7DF;

pub const EV_PLAYER_DESCRIPTION: u32 = 0x0013;
pub const EV_USE_DONE: u32 = 0x01C7;
pub const EV_UPDATE_HEALTH: u32 = 0x01C0;

pub const ACT_MOVE_TO_STATE: u32 = 0xF61C;
pub const ACT_AUTONOMOUS_POSITION: u32 = 0xF753;
pub const ACT_JUMP: u32 = 0xF61B;
pub const ACT_LOGIN_COMPLETE: u32 = 0x00A1;
pub const ACT_CONFIRMATION_RESPONSE: u32 = 0x0275;

/// The game-action opcodes whose handlers can end with `UseDone` (`Player.SendUseDoneEvent`):
/// Use, UseWithTarget, the two casts, and the vendor Buy and Sell.
pub const USE_DONE_REQUESTS: [u32; 6] = [0x0036, 0x0035, 0x0048, 0x004A, 0x005F, 0x0060];

/// The server's private property updates: sequence byte at 4, key at 5 (`GameMessagePrivateUpdate*`).
pub const PRIVATE_UPDATES: [u32; 12] = [
    0x02CD, 0x02CF, 0x02D1, 0x02D3, 0x02D5, 0x02D7, 0x02D9, 0x02DB, 0x02DD, 0x02E3, 0x02E7, 0x02E9,
];

/// The game-event type of a `0xF7B0` payload.
#[must_use]
pub fn event_type(p: &[u8]) -> Option<u32> {
    (u32_at(p, 0) == Some(GAME_EVENT))
        .then(|| u32_at(p, 12))
        .flatten()
}

/// The game-action type of a `0xF7B1` payload.
#[must_use]
pub fn action_type(p: &[u8]) -> Option<u32> {
    (u32_at(p, 0) == Some(GAME_ACTION))
        .then(|| u32_at(p, 8))
        .flatten()
}

/// A client movement action (MoveToState, AutonomousPosition, Jump).
#[must_use]
pub fn is_movement(p: &[u8]) -> bool {
    action_type(p)
        .is_some_and(|a| [ACT_MOVE_TO_STATE, ACT_AUTONOMOUS_POSITION, ACT_JUMP].contains(&a))
}

/// The structural label of a message: `ev XXXX` for a game event, `act XXXX` for a game action,
/// the opcode otherwise.
#[must_use]
pub fn label(p: &[u8]) -> String {
    if let Some(e) = event_type(p) {
        format!("ev {e:04X}")
    } else if let Some(a) = action_type(p) {
        format!("act {a:04X}")
    } else {
        format!("{:04X}", u32_at(p, 0).unwrap_or(0))
    }
}

/// The object a server message is about (its subject), where the layout names one at a fixed
/// offset. Game events are about the player they are sent to (their target, at 4).
#[must_use]
pub fn subject(p: &[u8]) -> Option<u32> {
    let op = u32_at(p, 0)?;
    let at = match op {
        CREATE_OBJECT | PLAYER_CREATE | DELETE_OBJECT | UPDATE_POSITION | SET_STATE
        | UPDATE_MOTION | VECTOR_UPDATE | UPDATE_OBJECT | 0xF74A | 0xF750 | 0xF755 | 0xF625
        | 0x0024 | GAME_EVENT => 4,
        // ParentEvent: parent, then child; the child is the object that moves.
        PARENT_EVENT => 8,
        // public updates: sequence byte, then the guid (the string update writes its key first)
        0x02CE | 0x02D0 | 0x02D2 | 0x02D4 | 0x02D8 | 0x02DA | 0x02DC | 0x02E8 | 0x0197 => 5,
        0x02D6 => 9,
        _ => return None,
    };
    u32_at(p, at)
}

/// The objects a server message names besides its subject, which a client must know: a
/// ParentEvent's parent.
#[must_use]
pub fn other_named(p: &[u8]) -> Option<u32> {
    (u32_at(p, 0)? == PARENT_EVENT)
        .then(|| u32_at(p, 4))
        .flatten()
}

/// The messages whose subject the client must already know (all of [`subject`]'s except the
/// creates, the delete and the game events, which are checked separately).
#[must_use]
pub fn is_object_update(op: u32) -> bool {
    !matches!(
        op,
        CREATE_OBJECT | PLAYER_CREATE | UPDATE_OBJECT | DELETE_OBJECT | GAME_EVENT
    )
}

/// A player guid (ACE's `ObjectGuid` player range).
#[must_use]
pub const fn is_player_guid(g: u32) -> bool {
    g >= 0x5000_0001 && g <= 0x6FFF_FFFF
}

/// A dynamic guid (ACE's `ObjectGuid` dynamic range).
#[must_use]
pub const fn is_dynamic_guid(g: u32) -> bool {
    g >= 0x8000_0000 && g <= 0xFFFF_FFFE
}

/// A u16 sequence `a` newer than `b` (ACE's `SequenceManager`/the client's wrap-around compare).
#[must_use]
pub const fn newer16(a: u16, b: u16) -> bool {
    a != b && a.wrapping_sub(b) < 0x8000
}

/// A u8 sequence `a` newer than `b`.
#[must_use]
pub const fn newer8(a: u8, b: u8) -> bool {
    a != b && a.wrapping_sub(b) < 0x80
}
