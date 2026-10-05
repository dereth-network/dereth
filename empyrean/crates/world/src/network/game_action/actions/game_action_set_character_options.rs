// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionSetCharacterOptions.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionSetCharacterOptions.cs`.
//!
//! NOTE: The client does not send this packet if the only options that were changed are normally
//! called in the GameActionSetSingleCharacterOption packet (see ACE's note on the class).

use empyrean_common::dotnet::DotNetDict;
use empyrean_entity::enums::CharacterOptionDataFlag;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_character;
use crate::World;

/// `GenericQualitiesPackHeader` (`Network/Enum/GenericQualitiesPackHeader.cs`): the flags this
/// handler tests.
const PACKED_INT_STATS: u32 = 1;
const PACKED_BOOL_STATS: u32 = 2;
const PACKED_FLOAT_STATS: u32 = 4;
const PACKED_STRING_STATS: u32 = 8;

fn has(flags: u32, flag: CharacterOptionDataFlag) -> bool {
    (flags & flag.0) != 0
}

// ACE: GameActionSetCharacterOptions.Handle
/// The reads stay ACE's rather than `dereth-protocol`'s player-module record: ACE applies the first
/// options word before it reads the rest (a message that fails later still sets it), reads one
/// spell bar plus every bar group whose flag is set, skips the generic qualities (by the client's
/// entry layout), and keeps the gameplay options as raw bytes; the record takes the bar groups one of
/// three, reads the generic qualities by type, parses the gameplay options and ends aligned, so it
/// would read some messages differently and reject some this accepts.
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let player = session_player(w, session);
    if !w
        .objects
        .get(player)
        .expect("ACE: session.Player is null (NullReferenceException)")
        .first_enter_world_done()
    {
        // if a player is stuck in pink bubble state during login,
        // and they press the 'logout' button before first entering world,
        // their client will have not registered their character options from the server yet,
        // and their client will send the default character options upon clicking the logout button,
        // overwriting their custom options on the server with the defaults. this code avoids that situation
        let name = crate::dispatch::name::name(w, player).unwrap_or_default();
        log::warn!("{name} sent GameAction 0x1A1 - SetCharacterOptions before FirstEnterWorldDone, ignoring...");
        return Ok(());
    }

    let mut desired_components: DotNetDict<u32, i32> = DotNetDict::new();

    // Thanks to tfarley (aclogview) for guidance on how to parse some of these flags.  The protocol docs are incomplete.
    // Flags
    let flags = message.read_u32()?;

    let character_options1_flag = message.read_i32()?;
    player_character::set_character_options1(w, player, character_options1_flag);

    // TODO: Read shortcuts into object so it's available in the Handle method.
    if has(flags, CharacterOptionDataFlag::Shortcut) {
        let num_shortcuts = message.read_u32()?;
        for _ in 0..num_shortcuts {
            message.read_i32()?; // index
            message.read_u32()?; // objectId (guid?)
            message.read_u32()?; // spellId
        }
    }

    let num_tab1_spells = message.read_u32()?;
    if num_tab1_spells > 0 {
        for _ in 0..num_tab1_spells {
            message.read_u32()?; // SpellID
        }
    }

    // TODO: I think this has been replaced by the "SpellLists8" struct, but we need to verify
    if has(flags, CharacterOptionDataFlag::MultiSpellList) {
        // Reads in 4 tabs of spells?
        for _ in 0..4 {
            let count = message.read_u32()?;
            for _ in 0..count {
                message.read_u32()?; // spellId
            }
        }
    }

    // TODO: I think this has been replaced by the "SpellLists8" struct, but we need to verify
    if has(flags, CharacterOptionDataFlag::ExtendedMultiSpellLists) {
        // Reads in 6 tabs of spells?
        for _ in 0..6 {
            let count = message.read_u32()?;
            for _ in 0..count {
                message.read_u32()?; // spellId
            }
        }
    }

    // TODO: Read into an object so it's available to the Handle method
    if has(flags, CharacterOptionDataFlag::SpellLists8) {
        // Reads in 7 tabs of spells?
        for _ in 0..7 {
            let count = message.read_u32()?;
            for _ in 0..count {
                message.read_u32()?; // spellId
            }
        }
    }

    if has(flags, CharacterOptionDataFlag::DesiredComps) {
        let size_info = message.read_u32()?; // sizeInfo
        let num = size_info & 0xFFFF;
        for _ in 0..num {
            let key = message.read_u32()?;
            let value = message.read_i32()?;
            // `Dictionary.Add` throws on a duplicate key
            assert!(desired_components.get(&key).is_none(), "System.ArgumentException: An item with the same key has already been added. Key: {key}");
            desired_components.insert(key, value);
        }
    }

    // `spellbookFilters` is read and not used
    if has(flags, CharacterOptionDataFlag::SpellbookFilters) {
        message.read_u32()?;
    }

    if has(flags, CharacterOptionDataFlag::CharacterOptions2) {
        let character_options2_flag = message.read_i32()?;
        player_character::set_character_options2(w, player, character_options2_flag);
    }

    // TODO: Read into an object so it's available in the Handle method.
    if has(flags, CharacterOptionDataFlag::TimestampFormat) {
        message.read_string16l()?; // TODO: verify this is correct
    }

    // SquelchList doesn't get used by the client, so should never be set.
    // if ((flags & (uint)CharacterOptionDataFlag.SquelchList) != 0) { }

    // TODO: Read these properly and do something with the values
    // This functionality taken from aclogview.
    if has(flags, CharacterOptionDataFlag::GenericQualitiesData) {
        // the player's generic option qualities

        // We're not going to use these just yet...
        let generic_qualities_header = message.read_u32()?;
        if (generic_qualities_header & PACKED_INT_STATS) != 0 {
            let size_info = message.read_u32()?;
            let curr_num = size_info & 0xFFFF;
            for _ in 0..curr_num {
                message.skip(8); // 4 bytes for key, 4 for value
            }
        }
        if (generic_qualities_header & PACKED_BOOL_STATS) != 0 {
            let size_info = message.read_u32()?;
            let curr_num = size_info & 0xFFFF;
            for _ in 0..curr_num {
                message.skip(8); // 4 bytes for key, 4 for value
            }
        }
        if (generic_qualities_header & PACKED_FLOAT_STATS) != 0 {
            let size_info = message.read_u32()?;
            let curr_num = size_info & 0xFFFF;
            // Not ACE's (retail, V350): a float entry is a 4-byte key and a
            // 4-byte float, as the client writes it (ACE read a key and a string here, so a module
            // carrying float options lost options2 and the window layout).
            for _ in 0..curr_num {
                message.skip(8); // 4 bytes for key, 4 for value
            }
        }
        if (generic_qualities_header & PACKED_STRING_STATS) != 0 {
            let size_info = message.read_u32()?;
            let curr_num = size_info & 0xFFFF;
            // Not ACE's (retail, V350): a string entry is a 4-byte key and a
            // packed string, as the client writes it (ACE read a key and 4 bytes).
            for _ in 0..curr_num {
                message.skip(4); // 4 bytes for key
                message.read_string16l()?; // the packed string
            }
        }
    }

    // Window / UI Layout, Opacity, etc
    if has(flags, CharacterOptionDataFlag::GameplayOptions) {
        // This is the  last message... So it should be all that is left.
        let size = message.remaining();

        let gameplay_options = message.read_bytes(size);
        player_character::set_character_gameplay_options(w, player, gameplay_options);
    }
    Ok(())
}
