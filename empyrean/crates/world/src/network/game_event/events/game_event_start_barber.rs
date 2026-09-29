// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventStartBarber.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventStartBarber.cs`.

use dereth_protocol::trade::{BarberSettings, CharacterStartBarber};
use empyrean_net::GameMessageGroup;
use empyrean_net::SessionId;

use crate::network::game_event::game_event_message::game_event_message;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::World;

// ACE: GameEventStartBarber.EmpyreanMaleMotionDID
pub const EMPYREAN_MALE_MOTION_DID: u32 = 0x0900_020E;
// ACE: GameEventStartBarber.EmpyreanFemaleMotionDID
pub const EMPYREAN_FEMALE_MOTION_DID: u32 = 0x0900_020D;

// ACE: GameEventStartBarber.GameEventStartBarber
/// The barber panel: the player's current and default appearance, its setup and the option flags.
///
/// # Panics
/// A session with no player, or a player with no `Character` (ACE's `NullReferenceException`).
#[must_use]
pub fn game_event_start_barber(w: &mut World, session: SessionId) -> GameMessage {
    let mut msg = game_event_message(
        GameEventType::StartBarber,
        GameMessageGroup::UIQueue,
        session_data(w, session),
    );

    let player = session_data(w, session)
        .player
        .expect("ACE: Session.Player is null (NullReferenceException)");
    let o = w
        .objects
        .get(player)
        .expect("ACE: Session.Player is null (NullReferenceException)");
    let character = o
        .player
        .as_ref()
        .and_then(|p| p.player.character.as_ref())
        .expect("ACE: Player.Character is null (NullReferenceException)");

    let settings = BarberSettings {
        base_palette: o.palette_base_did().unwrap_or(0),
        head_object: o.head_object_did().unwrap_or(0),
        head_texture: character.hair_texture,
        default_head_texture: character.default_hair_texture,

        eyes_texture: o.eyes_texture_did().unwrap_or(0),
        default_eyes_texture: o.default_eyes_texture_did().unwrap_or(0),

        nose_texture: o.nose_texture_did().unwrap_or(0),
        default_nose_texture: o.default_nose_texture_did().unwrap_or(0),

        mouth_texture: o.mouth_texture_did().unwrap_or(0),
        default_mouth_texture: o.default_mouth_texture_did().unwrap_or(0),

        skin_palette: o.skin_palette_did().unwrap_or(0),
        hair_palette: o.hair_palette_did().unwrap_or(0),
        eyes_palette: o.eyes_palette_did().unwrap_or(0),

        setup_id: o.setup_table_id(),

        // option1 - specifies the toggle option for some races, such as floating empyrean or flaming head on undead
        // 0 = using the "default" animation (normal running for most, float for empyrean)
        // 1 = using the "bound/running" animation for empyrean
        option1: i32::from(
            o.motion_table_id() == EMPYREAN_FEMALE_MOTION_DID
                || o.motion_table_id() == EMPYREAN_MALE_MOTION_DID,
        ),

        // option2 - seems to be unused
        option2: 0,
    };
    msg.write_proto(&CharacterStartBarber(settings));
    msg
}
