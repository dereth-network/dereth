// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageCharacterList.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageCharacterList.cs`.

use dereth_primitives::ObjectId;
use dereth_protocol::login as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::AccessLevel;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::sessions::CharacterSummary;
use crate::sessions::SessionData;
use crate::World;

/// `ConfigManager.Config.Server.Accounts.OverrideCharacterPermissions`: the accounts
/// configuration the authentication store was opened with (as `Player.IsPlussed` and the
/// character restore read it).
fn override_character_permissions(w: &World) -> bool {
    w.auth
        .lock()
        .accounts_config()
        .override_character_permissions
}

/// `PropertyManager.GetLong(key).Item`.
fn property_manager_get_long(w: &World, key: &str) -> i64 {
    crate::managers::property_manager::get_long(w, key, 0, true).item
}

/// `PropertyManager.GetBool(key).Item`.
fn property_manager_get_bool(w: &World, key: &str) -> bool {
    crate::managers::property_manager::get_bool(w, key, false, true).item
}

/// `character.IsPlussed`.
fn character_is_plussed(character: &CharacterSummary) -> bool {
    character.is_plussed
}

// ACE: GameMessageCharacterList.GameMessageCharacterList
/// `Time.GetUnixTime()` is `w.now.unix_time`. The configuration and property reads are pointers
/// (see above) because ACE makes them inside the constructor.
#[must_use]
#[allow(clippy::if_same_then_else)] // ACE's two `+` branches, kept as written
pub fn game_message_character_list(
    w: &World,
    characters: &[CharacterSummary],
    session: &SessionData,
) -> GameMessage {
    let characters = characters
        .iter()
        .map(|character| {
            let name = if override_character_permissions(w)
                && session.access_level > AccessLevel::Advocate
            {
                format!("+{}", character.name)
            } else if !override_character_permissions(w) && character_is_plussed(character) {
                format!("+{}", character.name)
            } else {
                character.name.clone()
            };

            // Not ACE's (a fix, V444): a character pending deletion is sent with the seconds left
            // until it is deleted, at least 1 while the deletion is pending (one past its time
            // but not yet purged is still pending). ACE subtracted the other way round,
            // `(uint)Math.Max(1, Time.GetUnixTime() - character.DeleteTime)`, and since the
            // deletion time is in the future every pending deletion went out as 1. The ulong
            // converts to double, `Math.Max(double, double)`, then `(uint)` of the double, as ACE's.
            let delete = if character.delete_time != 0 {
                #[allow(clippy::cast_precision_loss)]
                let remaining = character.delete_time as f64 - w.now.unix_time;
                let seconds: u32 = empyrean_common::dotnet::math::max(1.0, remaining).cs_cast();
                seconds
            } else {
                0
            };
            (character.id, name, delete)
        })
        .collect::<Vec<_>>();

    let slot_count: u32 = property_manager_get_long(w, "max_chars_per_account").cs_cast();
    let use_turbine_chat = u32::from(property_manager_get_bool(w, "use_turbine_chat"));

    let body = proto::LoginCharacterSet {
        status: 0,
        characters: characters
            .iter()
            .map(|(id, name, delete)| proto::CharacterIdentity {
                gid: ObjectId(*id),
                name: ace_str(name.as_str()),
                seconds_greyed_out: *delete,
            })
            .collect(),
        deleted: Vec::new(),
        num_allowed_characters: slot_count.cast_signed(),
        account: ace_str(session.account.as_deref()),
        use_turbine_chat,
        // DIVERGE: the era's Throne of Destiny flag (`EraRules.account_has_tod`); ACE always sends 1.
        has_throne_of_destiny: u32::from(w.era.account_has_tod), /*hasThroneOfDestiny*/
    };
    let mut strings: Vec<&str> = characters
        .iter()
        .map(|(_, name, _)| name.as_str())
        .collect();
    strings.push(session.account.as_deref().unwrap_or(""));
    let mut msg = GameMessage::new(GameMessageOpcode::CharacterList, GameMessageGroup::UIQueue);
    msg.write_proto_strings(&body, &strings);
    msg
}
