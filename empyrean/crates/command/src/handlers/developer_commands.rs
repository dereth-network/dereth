// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Command/Handlers/DeveloperCommands.cs
//! Port of `Source/ACE.Server/Command/Handlers/DeveloperCommands.cs`.
//!
//! - **Selection.** Several commands act on "the selected object": `HealthQueryTarget`, else
//!   `ManaQueryTarget`, else `CurrentAppraisalTarget` ([`selected_object_id`]); most act on the
//!   last appraised object (`CommandHandlerHelper.GetLastAppraisedObject`).
//! - **Exceptions.** A C# exception inside a handler is a panic here; the command manager catches
//!   and logs it where ACE does. A `try`/`catch` inside a handler is ported as the explicit
//!   failure path of the call it wraps (a parse that fails, an index past the parameters).
//! - **`Convert`.** `Convert.ToInt32(string)` and friends, and the `fromBase` overloads
//!   (`ParseNumbers.StringToInt`/`StringToLong`), are ported in [`convert`]; the `commands`
//!   vectors (`dev_convert`) check them against .NET.
//! - **Local time.** DIVERGE: ACE's `ToLocalTime()` is the host's local time; the world has only
//!   its UTC clock, so those read as UTC (as in AdminCommands).
//! - **Databases.** `teledungeon`, `dungeonname` and `tiermobs` run LINQ queries over ACE's world
//!   database (`WorldDbContext`); here they are pointers to the world content queries.
//!
//! Callees that are not ported yet are private pointer functions at the bottom of this file, each
//! a `not_ported!` named after its ACE member.

use std::collections::HashSet;
use std::sync::Mutex;

use empyrean_common::dotnet::numerics::{Quaternion, Vector3};
use empyrean_common::dotnet::{format, to_string, CsCast};
use empyrean_common::extensions::date_time_extensions::to_common_string;
use empyrean_common::extensions::string_extensions;
use empyrean_entity::enums::*;
use empyrean_entity::{LandblockId, ObjectGuid, Position};
use empyrean_net::SessionId;
use empyrean_world::dispatch;
use empyrean_world::entity::landblock;
use empyrean_world::managers::{guid_manager, landblock_manager, player_manager};
use empyrean_world::network::game_event::game_event_message::session_data;
use empyrean_world::network::game_messages::game_message::enqueue_send;
use empyrean_world::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use empyrean_world::network::game_messages::messages::game_message_update_motion::game_message_update_motion;
use empyrean_world::network::motion::movement_data::{Motion, MovementData};
use empyrean_world::world_objects::player_inventory::{self, SearchLocations};
use empyrean_world::world_objects::world_object::{self, CtorEnv, WorldObject};
use empyrean_world::world_objects::{
    player_character, player_location, player_xp, world_object_networking,
};
use empyrean_world::World;

use crate::command_handler_attribute::CommandHandlerAttribute;
use crate::command_handler_flag::CommandHandlerFlag;
use crate::command_handler_info::{CommandHandlerInfo, NamedHandler};
use crate::command_manager::console_write_line;
use crate::command_parameter_helpers::dotnet_parse;
use crate::handler;
use crate::handlers::admin_commands::{self, enum_try_parse};
use crate::handlers::command_handler_helper::{self, send_server_message};

// ---------------------------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------------------------

/// `session` for a handler ACE only reaches with a session (RequiresWorld, or a dereference).
fn require(session: Option<SessionId>) -> SessionId {
    session.expect("NullReferenceException: session")
}

/// `session.Player`.
fn session_player(w: &World, session: SessionId) -> ObjectGuid {
    w.sessions
        .player(session)
        .expect("NullReferenceException: session.Player")
}

/// `wo.Name`.
fn name_of(w: &World, wo: ObjectGuid) -> String {
    w.objects
        .get(wo)
        .and_then(|o| o.get_property(PropertyString::Name))
        .unwrap_or_default()
}

/// The object `g`, which ACE dereferences.
fn obj(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects
        .get(g)
        .unwrap_or_else(|| panic!("NullReferenceException: object 0x{:08X}", g.full()))
}

/// The object `g`, mutably.
fn obj_mut(w: &mut World, g: ObjectGuid) -> &mut WorldObject {
    w.objects
        .get_mut(g)
        .unwrap_or_else(|| panic!("NullReferenceException: object 0x{:08X}", g.full()))
}

/// `session.Network.EnqueueSend(new GameMessageSystemChat(message, type))`.
fn system_chat(
    w: &mut World,
    session: SessionId,
    message: &str,
    chat_message_type: ChatMessageType,
) {
    enqueue_send(
        w,
        session,
        game_message_system_chat(message, chat_message_type),
    );
}

/// `player.SendMessage(msg)` (`ChatMessageType.Broadcast`).
fn player_send_message(w: &mut World, player: ObjectGuid, msg: &str) {
    empyrean_world::world_objects::player::send_message(w, player, msg, ChatMessageType::Broadcast);
}

/// `CommandHandlerHelper.WriteOutputInfo(session, output, type)`.
fn write_output_info(
    w: &mut World,
    session: Option<SessionId>,
    output: &str,
    chat_message_type: ChatMessageType,
) {
    command_handler_helper::write_output_info(w, session, output, chat_message_type);
}

/// `CommandHandlerHelper.GetLastAppraisedObject(session)`.
fn last_appraised(w: &mut World, session: Option<SessionId>) -> Option<ObjectGuid> {
    command_handler_helper::get_last_appraised_object(w, require(session))
}

/// `HealthQueryTarget`, else `ManaQueryTarget`, else `CurrentAppraisalTarget`, else
/// `new ObjectGuid()` (0).
fn selected_object_id(w: &World, player: ObjectGuid) -> ObjectGuid {
    let p = obj(w, player);
    if let Some(t) = p.health_query_target() {
        ObjectGuid::new(t)
    } else if let Some(t) = p.mana_query_target() {
        ObjectGuid::new(t)
    } else if let Some(t) = p.current_appraisal_target() {
        ObjectGuid::new(t)
    } else {
        ObjectGuid::new(0)
    }
}

/// `HealthQueryTarget.HasValue || ManaQueryTarget.HasValue || CurrentAppraisalTarget.HasValue`.
fn has_selection(w: &World, player: ObjectGuid) -> bool {
    let p = obj(w, player);
    p.health_query_target().is_some()
        || p.mana_query_target().is_some()
        || p.current_appraisal_target().is_some()
}

/// `session.Player.CurrentLandblock?.GetObject(objectId)`.
fn current_landblock_get_object(
    w: &World,
    player: ObjectGuid,
    object_id: ObjectGuid,
) -> Option<ObjectGuid> {
    let lb = obj(w, player).current_landblock?;
    landblock::get_object(w, lb, object_id, true)
}

/// `wo is Creature`.
fn is_creature(w: &World, wo: ObjectGuid) -> bool {
    w.objects.get(wo).is_some_and(|o| o.creature.is_some())
}

/// `wo is Player`.
fn is_player(w: &World, wo: ObjectGuid) -> bool {
    w.objects.get(wo).is_some_and(|o| o.player.is_some())
}

/// `wo.Location`, which ACE dereferences.
fn location_of(w: &World, wo: ObjectGuid) -> Position {
    obj(w, wo)
        .location()
        .expect("NullReferenceException: Location")
}

/// `bool.ToString()`.
fn bool_string(b: bool) -> &'static str {
    if b {
        "True"
    } else {
        "False"
    }
}

/// `Vector3.ToString()` in en-US: `<x, y, z>`.
fn vec3_string(v: Vector3) -> String {
    format!(
        "<{}, {}, {}>",
        to_string(v.x),
        to_string(v.y),
        to_string(v.z)
    )
}

/// `Vector3.Distance(a, b)`.
fn vec3_distance(a: Vector3, b: Vector3) -> f32 {
    (a - b).length()
}

/// `Vector2.Distance(new Vector2(a.X, a.Y), new Vector2(b.X, b.Y))`.
fn vec2_distance(a: Vector3, b: Vector3) -> f32 {
    let (dx, dy) = (a.x - b.x, a.y - b.y);
    (dx * dx + dy * dy).sqrt()
}

/// `Position.ToGlobal()` (ACE.Server's `PositionExtensions`).
fn to_global(p: &Position) -> Vector3 {
    empyrean_world::entity::position_extensions::to_global(p, false)
}

/// `s.Length` (UTF-16 units).
fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// `s.Substring(0, n)` over UTF-16 units.
fn substring_to(s: &str, n: usize) -> String {
    let units: Vec<u16> = s.encode_utf16().take(n).collect();
    String::from_utf16_lossy(&units)
}

/// `s.Substring(start)` over UTF-16 units.
fn substring_from(s: &str, start: usize) -> String {
    let units: Vec<u16> = s.encode_utf16().skip(start).collect();
    String::from_utf16_lossy(&units)
}

/// `a.Equals(b, StringComparison.OrdinalIgnoreCase)` (one character at a time, simple case
/// mapping).
fn eq_ignore_case(a: &str, b: &str) -> bool {
    a.chars().count() == b.chars().count()
        && a.chars()
            .zip(b.chars())
            .all(|(x, y)| x == y || x.to_uppercase().eq(y.to_uppercase()))
}

/// `string.Contains(value, StringComparison.OrdinalIgnoreCase)`.
fn contains_ignore_case(haystack: &str, needle: &str) -> bool {
    haystack.to_uppercase().contains(&needle.to_uppercase())
}

/// A new object from `WorldObjectFactory.CreateNewWorldObject(weenie)`: a new dynamic guid,
/// construction, and the object in `World.objects` (where the C# object is held by its
/// reference). `None` when construction fails; the guid is recycled.
fn create_new_world_object(
    w: &mut World,
    weenie: std::sync::Arc<empyrean_entity::Weenie>,
) -> Option<ObjectGuid> {
    let guid = guid_manager::new_dynamic_guid(w);
    let Some(o) = CtorEnv::with_world(w, |env| {
        empyrean_world::factories::world_object_factory::create_world_object(
            env,
            Some(weenie),
            guid,
        )
    }) else {
        guid_manager::recycle_dynamic_guid(w, guid);
        return None;
    };
    Some(insert_object(w, o))
}

/// `WorldObjectFactory.CreateNewWorldObject(uint weenieClassId)`: `None` when the weenie is not in
/// the world database.
fn create_new_world_object_by_wcid(w: &mut World, wcid: u32) -> Option<ObjectGuid> {
    let weenie = w.content.get_cached_weenie(wcid)?;
    create_new_world_object(w, weenie)
}

/// `WorldObjectFactory.CreateNewWorldObject(string weenieClassName)`.
fn create_new_world_object_by_name(w: &mut World, weenie_class_name: &str) -> Option<ObjectGuid> {
    let weenie = w
        .content
        .get_cached_weenie_by_class_name(weenie_class_name)?;
    create_new_world_object(w, weenie)
}

/// An object that entered neither the world nor an inventory: ACE leaves it to the garbage
/// collector; here it leaves `World.objects`.
fn drop_unplaced(w: &mut World, wo: ObjectGuid) {
    let placed = w.objects.get(wo).is_some_and(|o| {
        o.current_landblock.is_some() || o.container_id().is_some() || o.wielder_id().is_some()
    });
    if !placed {
        w.objects.remove(wo);
    }
}

/// A `WorldObject` built outside the store (the loot factories): into `World.objects`.
fn insert_object(w: &mut World, o: WorldObject) -> ObjectGuid {
    let guid = o.guid;
    assert!(w.objects.insert(o).is_ok(), "fresh dynamic guid");
    guid
}

/// `player.TryCreateInInventoryWithNetworking(item)`; an item that found no room is dropped (ACE's
/// garbage collector).
fn try_create_in_inventory(w: &mut World, player: ObjectGuid, item: ObjectGuid) -> bool {
    let ok = player_inventory::try_create_in_inventory_with_networking(w, player, item).is_some();
    if !ok {
        drop_unplaced(w, item);
    }
    ok
}

// ---------------------------------------------------------------------------------------------
// Enum.TryParse
// ---------------------------------------------------------------------------------------------

/// `Enum.TryParse` over an enum with an unsigned 32-bit underlying type.
fn try_parse_u32<E: AceEnum>(s: &str, ignore_case: bool) -> Option<u32> {
    enum_try_parse::<E>(s, ignore_case, 0, i128::from(u32::MAX)).map(CsCast::cs_cast)
}

/// `Enum.TryParse` over an enum with a signed 32-bit underlying type.
fn try_parse_i32<E: AceEnum>(s: &str, ignore_case: bool) -> Option<i32> {
    enum_try_parse::<E>(s, ignore_case, i128::from(i32::MIN), i128::from(i32::MAX)).map(|v| {
        #[allow(clippy::cast_possible_truncation)]
        let low = v as u32;
        low.cast_signed()
    })
}

/// `Enum.TryParse` over an enum with an unsigned 16-bit underlying type.
fn try_parse_u16<E: AceEnum>(s: &str, ignore_case: bool) -> Option<u16> {
    enum_try_parse::<E>(s, ignore_case, 0, i128::from(u16::MAX)).map(CsCast::cs_cast)
}

// ---------------------------------------------------------------------------------------------
// Convert
// ---------------------------------------------------------------------------------------------

/// The `System.Convert` string overloads these handlers call. `Err` is the .NET exception type
/// (every caller either catches it or lets it reach the command manager).
pub mod convert {
    /// `NumberStyles.Integer` (`int.Parse`): white, one sign, ASCII digits, white, then trailing
    /// `\0`s. The sign and the magnitude (saturated).
    fn integer_parts(s: &str) -> Result<(bool, u128), &'static str> {
        let white = |c: char| matches!(c, '\u{9}'..='\u{D}' | ' ');
        let s = s.trim_end_matches('\0').trim_matches(white);
        let (negative, digits) = match s.as_bytes().first() {
            Some(b'-') => (true, &s[1..]),
            Some(b'+') => (false, &s[1..]),
            _ => (false, s),
        };
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err("System.FormatException");
        }
        let mut value: u128 = 0;
        for b in digits.bytes() {
            value = value
                .saturating_mul(10)
                .saturating_add(u128::from(b - b'0'));
        }
        Ok((negative, value))
    }

    fn integer(s: &str, min: i128, max: i128) -> Result<i128, &'static str> {
        let (negative, v) = integer_parts(s)?;
        let v = i128::try_from(v).map_err(|_| "System.OverflowException")?;
        let v = if negative { -v } else { v };
        if v < min || v > max {
            return Err("System.OverflowException");
        }
        Ok(v)
    }

    /// `Convert.ToInt16(string)`.
    ///
    /// # Errors
    /// `FormatException` or `OverflowException`.
    pub fn to_int16(s: &str) -> Result<i16, &'static str> {
        integer(s, i128::from(i16::MIN), i128::from(i16::MAX))
            .map(|v| i16::try_from(v).unwrap_or_default())
    }

    /// `Convert.ToUInt16(string)`.
    ///
    /// # Errors
    /// `FormatException` or `OverflowException`.
    pub fn to_uint16(s: &str) -> Result<u16, &'static str> {
        integer(s, 0, i128::from(u16::MAX)).map(|v| u16::try_from(v).unwrap_or_default())
    }

    /// `Convert.ToInt32(string)`.
    ///
    /// # Errors
    /// `FormatException` or `OverflowException`.
    pub fn to_int32(s: &str) -> Result<i32, &'static str> {
        integer(s, i128::from(i32::MIN), i128::from(i32::MAX))
            .map(|v| i32::try_from(v).unwrap_or_default())
    }

    /// `Convert.ToUInt32(string)`.
    ///
    /// # Errors
    /// `FormatException` or `OverflowException`.
    pub fn to_uint32(s: &str) -> Result<u32, &'static str> {
        integer(s, 0, i128::from(u32::MAX)).map(|v| u32::try_from(v).unwrap_or_default())
    }

    /// `Convert.ToInt64(string)`.
    ///
    /// # Errors
    /// `FormatException` or `OverflowException`.
    pub fn to_int64(s: &str) -> Result<i64, &'static str> {
        integer(s, i128::from(i64::MIN), i128::from(i64::MAX))
            .map(|v| i64::try_from(v).unwrap_or_default())
    }

    /// `Convert.ToBoolean(string)` (`bool.Parse`).
    ///
    /// # Errors
    /// `FormatException`.
    pub fn to_boolean(s: &str) -> Result<bool, &'static str> {
        crate::handlers::admin_commands::dotnet_bool_parse(s).map_err(|_| "System.FormatException")
    }

    /// `Convert.ToDouble(string)` (`double.Parse`, en-US).
    ///
    /// # Errors
    /// `FormatException`.
    pub fn to_double(s: &str) -> Result<f64, &'static str> {
        crate::command_parameter_helpers::dotnet_parse::double_try_parse(s)
            .ok_or("System.FormatException")
    }

    /// `ParseNumbers.StringToInt`/`StringToLong(s, radix, IsTight | [TreatAsUnsigned])` for
    /// `bits` 32 or 64: the bodies of `Convert.ToInt32/ToUInt32/ToInt64(string, fromBase)` (radix
    /// 10 or 16), as the raw two's-complement bits.
    fn string_to_long(s: &str, radix: u32, unsigned: bool, bits: u32) -> Result<u64, &'static str> {
        let c: Vec<char> = s.chars().collect();
        if c.is_empty() {
            return Err("System.ArgumentOutOfRangeException");
        }
        let mut i = 0;
        let mut negative = false;
        if c[i] == '-' {
            if radix != 10 {
                return Err("System.ArgumentException");
            }
            if unsigned {
                return Err("System.OverflowException");
            }
            negative = true;
            i += 1;
        } else if c[i] == '+' {
            i += 1;
        }
        if radix == 16 && i + 1 < c.len() && c[i] == '0' && (c[i + 1] == 'x' || c[i + 1] == 'X') {
            i += 2;
        }
        let start = i;
        let digit = |ch: char| {
            if ch.is_ascii() {
                ch.to_digit(radix)
            } else {
                None
            }
        };
        let mask: u64 = if bits == 32 {
            u64::from(u32::MAX)
        } else {
            u64::MAX
        };
        let sign_bit: u64 = 1 << (bits - 1);
        let mut result: u64 = 0;
        if radix == 10 && !unsigned {
            // GrabInts / GrabLongs, signed decimal
            let max_val = (sign_bit - 1) / 10;
            while i < c.len() {
                let Some(v) = digit(c[i]) else { break };
                if result > max_val || result & sign_bit != 0 {
                    return Err("System.OverflowException");
                }
                result = (result * 10 + u64::from(v)) & mask;
                i += 1;
            }
            if result & sign_bit != 0 && result != sign_bit {
                return Err("System.OverflowException");
            }
        } else {
            let max_val = mask / u64::from(radix);
            while i < c.len() {
                let Some(v) = digit(c[i]) else { break };
                if result > max_val {
                    return Err("System.OverflowException");
                }
                let temp = (result * u64::from(radix) + u64::from(v)) & mask;
                if temp < result {
                    return Err("System.OverflowException");
                }
                result = temp;
                i += 1;
            }
        }
        if i == start {
            return Err("System.FormatException");
        }
        if i < c.len() {
            return Err("System.FormatException");
        }
        if result == sign_bit && !negative && radix == 10 && !unsigned {
            return Err("System.OverflowException");
        }
        if radix == 10 && negative {
            result = result.wrapping_neg() & mask;
        }
        Ok(result)
    }

    /// `Convert.ToInt32(string, fromBase)` (10 or 16).
    ///
    /// # Errors
    /// The .NET exception type.
    pub fn to_int32_base(s: &str, from_base: u32) -> Result<i32, &'static str> {
        #[allow(clippy::cast_possible_truncation)]
        string_to_long(s, from_base, false, 32).map(|v| (v as u32).cast_signed())
    }

    /// `Convert.ToUInt32(string, fromBase)` (10 or 16).
    ///
    /// # Errors
    /// The .NET exception type.
    pub fn to_uint32_base(s: &str, from_base: u32) -> Result<u32, &'static str> {
        #[allow(clippy::cast_possible_truncation)]
        string_to_long(s, from_base, true, 32).map(|v| v as u32)
    }

    /// `Convert.ToInt64(string, fromBase)` (10 or 16).
    ///
    /// # Errors
    /// The .NET exception type.
    pub fn to_int64_base(s: &str, from_base: u32) -> Result<i64, &'static str> {
        string_to_long(s, from_base, false, 64).map(u64::cast_signed)
    }
}

// ---------------------------------------------------------------------------------------------
// Enum names
// ---------------------------------------------------------------------------------------------

/// .NET `Enum.ToString()`: empyrean-entity's [`AceEnum::to_dotnet_string`], which follows .NET's
/// `GetValues` order among aliases and its `FindDefinedIndex` / flags lookups.
trait NetString: AceEnum {
    fn net_string(self) -> String {
        self.to_dotnet_string()
    }
}

impl<E: AceEnum> NetString for E {}
/// `wo.WeenieClassName`: the cached weenie's class name.
fn weenie_class_name(w: &World, wo: ObjectGuid) -> String {
    let wcid = obj(w, wo).biota.weenie_class_id;
    empyrean_world::world_objects::world_object_networking::shims::weenie_class_name(w, wcid)
}

// ---------------------------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------------------------

// ACE: DeveloperCommands.HandleNudge
/// `nudge`: correct player position cell ID after teleporting into black space.
pub fn handle_nudge(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let s = require(session);
    let player = session_player(w, s);
    // `GetPosition(PositionType.Location)` is the player's cached Location object: ACE adjusts it
    // in place.
    let mut pos = obj(w, player)
        .get_position(PositionType::Location)
        .expect("NullReferenceException: pos");
    let before = pos;
    let adjusted = world_object::adjust_dungeon_cells(w, &mut pos);
    if !pos.equals(&before) || pos.cell() != before.cell() {
        obj_mut(w, player).set_location(Some(pos));
    }
    if adjusted {
        pos.position_z += 0.005_f32;
        obj_mut(w, player).set_location(Some(pos));
        let pos_readable = postion_as_landblocks_google_spreadsheet_format(&pos);
        let split: Vec<String> = pos_readable.split(' ').map(str::to_owned).collect();
        admin_commands::handle_teleport_loc(w, session, &split);
        system_chat(
            w,
            s,
            &format!("Nudge player to {pos_readable}"),
            ChatMessageType::Broadcast,
        );
    }
}

// ACE: DeveloperCommands.HandleFixBusy
/// `fixbusy`: attempts to remove the hourglass / fix the busy state for the player.
pub fn handle_fix_busy(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let player = session_player(w, require(session));
    player_send_use_done_event(w, player);
}

// ACE: DeveloperCommands.PostionAsLandblocksGoogleSpreadsheetFormat
/// `0x{Cell:X} {X} {Y} {Z} {W} {qX} {qY} {qZ}`.
#[must_use]
pub fn postion_as_landblocks_google_spreadsheet_format(pos: &Position) -> String {
    let p = pos.pos();
    let r = pos.rotation();
    format!(
        "0x{} {} {} {} {} {} {} {}",
        format(pos.cell(), "X"),
        to_string(p.x),
        to_string(p.y),
        to_string(p.z),
        to_string(r.w),
        to_string(r.x),
        to_string(r.y),
        to_string(r.z)
    )
}

// ACE: DeveloperCommands.EquipTest
/// `equiptest (hex)clothingTableId [palette_index] [shade]`: its parses only (ACE's
/// `TestWieldItem` call is commented out).
pub fn equip_test(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    if parameters.is_empty() {
        send_server_message(w, session, "Usage: @equiptest (hex)clothingTableId [palette_index] [shade].\neg '@equiptest 0x100005fd'", ChatMessageType::Broadcast);
        return;
    }

    // `UInt32.Parse(.., HexNumber)`, `Int32.Parse`, `Single.Parse`: any failure is the catch
    #[allow(clippy::manual_clamp)] // ACE's two ifs (a NaN shade stays NaN)
    let parsed = (|| {
        let _model_id = if parameters[0].starts_with("0x") {
            let strippedmodelid = substring_from(&parameters[0], 2);
            dotnet_parse::uint_try_parse_hex(&strippedmodelid)?
        } else {
            dotnet_parse::uint_try_parse_hex(&parameters[0])?
        };

        let mut _pal_option = -1;
        if parameters.len() > 1 {
            _pal_option = convert::to_int32(&parameters[1]).ok()?;
        }
        let mut shade = 0f32;
        if parameters.len() > 2 {
            shade = dotnet_parse::float_try_parse(&parameters[2])?;
        }
        if shade < 0.0 {
            shade = 0.0;
        }
        if shade > 1.0 {
            shade = 1.0;
        }
        let _ = shade;

        //if ((modelId >= 0x10000001) && (modelId <= 0x1000086B))
        //    session.Player.TestWieldItem(session, modelId, palOption, shade);
        //else
        //    ChatPacket.SendServerMessage(session, "Please enter a value greater than 0x10000000 and less than 0x1000086C",
        //        ChatMessageType.Broadcast);
        Some(())
    })();
    if parsed.is_none() {
        send_server_message(
            w,
            session,
            "Please enter a value greater than 0x10000000 and less than 0x1000086C",
            ChatMessageType::Broadcast,
        );
    }
}

// ACE: DeveloperCommands.HandleNetStats
/// `netstats`: view network statistics.
pub fn handle_net_stats(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let summary = w.net.statistics().summary();
    write_output_info(w, session, &summary, ChatMessageType::Broadcast);
}

// ACE: DeveloperCommands.HandleShowCompatibleClothingBases
/// `listcb <setup>` (console): lists the clothing tables compatible with a setup.
pub fn handle_show_compatible_clothing_bases(
    w: &mut World,
    _session: Option<SessionId>,
    parameters: &[String],
) {
    let setup_id = dotnet_parse::uint_try_parse(&parameters[0]).unwrap_or(0);

    let cb_start: u32 = 0x1000_0001;
    let cb_end: u32 = 0x1000_086c;

    let mut compatible_cbs: Vec<u32> = Vec::new();

    for i in cb_start..cb_end {
        // (an absent file is ACE's empty `new ClothingTable()`)
        let cb_to_test = w
            .dats
            .portal_dat()
            .read_from_dat::<empyrean_dat::file_types::ClothingTable>(i);

        if cb_to_test.is_some_and(|t| t.clothing_bases.keys().any(|k| k.0 == setup_id)) {
            compatible_cbs.push(i);
        }
    }

    console_write_line(&format!(
        "There are {} compatible clothingbase tables for setup {setup_id}",
        compatible_cbs.len()
    ));
    console_write_line("");
    console_write_line(
        &compatible_cbs
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    );
}

// ==================================
// Client Testing
// ==================================

// ACE: DeveloperCommands.HandleDebugEcho
/// `echo "text to send back to yourself" [ChatMessageType]`.
pub fn handle_debug_echo(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // `parameters[1]` past the end throws into the catch, which reads `parameters[0]`
    let Some(p1) = parameters.get(1) else {
        send_server_message(w, session, &parameters[0], ChatMessageType::Broadcast);
        return;
    };
    if let Some(cmt) = try_parse_u32::<ChatMessageType>(p1, true) {
        let cmt = ChatMessageType(cmt);
        if cmt.is_defined() {
            send_server_message(w, session, &parameters[0], cmt);
        } else {
            send_server_message(w, session, &parameters[0], ChatMessageType::Broadcast);
        }
    }
}

// ACE: DeveloperCommands.HandlePlaySound
/// `playsound sound (volume) (guid)`: plays a sound.
pub fn handle_play_sound(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let player = session_player(w, s);
    let mut volume = 1f32;

    if parameters.len() > 1 {
        // `float.TryParse(parameters[1], out volume)`: a failed parse leaves 0
        volume = dotnet_parse::float_try_parse(&parameters[1]).unwrap_or(0.0);
    }

    let mut guid = player.full();

    if parameters.len() > 2 {
        // `uint.TryParse(p.TrimStart("0x"), HexNumber, InvariantCulture, out guid)`: 0 on failure
        guid =
            dotnet_parse::uint_try_parse_hex(&string_extensions::trim_start(&parameters[2], "0x"))
                .unwrap_or(0);
    }

    let mut message = format!("Unable to find a sound called {} to play.", parameters[0]);

    if let Some(sound) = try_parse_u32::<Sound>(&parameters[0], true) {
        let sound = Sound(sound);
        if sound.is_defined() {
            message = format!("Playing sound {}", sound.net_string());
            // add the sound to the player queue for everyone to hear
            // player action queue items will execute on the landblock
            // player.playsound will play a sound on only the client session that called the function
            world_object::play_sound_effect(w, player, sound, ObjectGuid::new(guid), volume);
        }
    }

    system_chat(w, s, &message, ChatMessageType::Broadcast);
}

// ACE: DeveloperCommands.HandlePlayEffect
/// `effect effect (scale)`: plays an effect.
pub fn handle_play_effect(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let player = session_player(w, s);
    // (ACE builds a `GameMessageScript(Guid, PlayScript.Invalid)` it never sends)

    if parameters.len() > 1 && !parameters[1].is_empty() {
        // `float.Parse` throws into the catch, which does nothing
        let Some(_scale) = dotnet_parse::float_try_parse(&parameters[1]) else {
            return;
        };
    }

    let mut message = format!("Unable to find a effect called {} to play.", parameters[0]);

    if let Some(effect) = try_parse_u32::<PlayScript>(&parameters[0], true) {
        let effect = PlayScript(effect);
        if effect.is_defined() {
            message = format!("Playing effect {}", effect.net_string());
            world_object::apply_visual_effects(w, player, effect, 1.0);
        }
    }

    system_chat(w, s, &message, ChatMessageType::Broadcast);
}

// ACE: DeveloperCommands.ChatDump
/// `chatdump`: spews 1000 lines of text to you.
pub fn chat_dump(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    for i in 0..1000 {
        send_server_message(
            w,
            session,
            &format!("Test Message {i}"),
            ChatMessageType::Broadcast,
        );
    }
}

// ACE: DeveloperCommands.Animation
/// `animation MotionCommand (optional target guid)`: plays an animation on the current player, or
/// optionally another object.
pub fn animation(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let Some(motion_command) =
        try_parse_u32::<MotionCommand>(&parameters[0], false).map(MotionCommand)
    else {
        send_server_message(
            w,
            session,
            &format!("MotionCommand: {} not found", parameters[0]),
            ChatMessageType::Broadcast,
        );
        return;
    };
    let me = session_player(w, require(session));
    let mut o = me;

    if parameters.len() > 1 {
        let Some(guid) =
            dotnet_parse::uint_try_parse_hex(&string_extensions::trim_start(&parameters[1], "0x"))
        else {
            send_server_message(
                w,
                session,
                &format!("Invalid guid: {}", parameters[1]),
                ChatMessageType::Broadcast,
            );
            return;
        };
        let Some(found) = player_inventory::find_object(
            w,
            me,
            ObjectGuid::new(guid),
            SearchLocations::Everywhere,
        )
        .result
        else {
            send_server_message(
                w,
                session,
                &format!("Couldn't find guid: {}", parameters[1]),
                ChatMessageType::Broadcast,
            );
            return;
        };
        o = found;
        if current_motion_state(w, o).is_none() {
            let text = format!("{} ({}) has no CurrentMotionState", name_of(w, o), o);
            send_server_message(w, session, &text, ChatMessageType::Broadcast);
            return;
        }
    }
    let stance = current_motion_state(w, o)
        .expect("NullReferenceException: CurrentMotionState")
        .stance;

    let mut suffix = String::new();
    if o != me {
        suffix = format!(" on {} ({})", name_of(w, o), o);
    }

    let text = format!(
        "Playing animation {}.{}{suffix}",
        stance.net_string(),
        motion_command.net_string()
    );
    send_server_message(w, session, &text, ChatMessageType::Broadcast);

    let motion = Motion::new(stance, motion_command, 1.0);
    world_object_networking::enqueue_broadcast_motion(w, o, &motion, None, None);
}

/// `wo.CurrentMotionState`.
fn current_motion_state(w: &World, wo: ObjectGuid) -> Option<Motion> {
    obj(w, wo)
        .wo
        .world_object_properties
        .current_motion_state
        .clone()
}

/// `session.Network.EnqueueSend(new GameMessageUpdateMotion(player, motion))`.
fn send_update_motion(w: &mut World, session: SessionId, player: ObjectGuid, motion: &Motion) {
    let movement_data = MovementData::from_motion(player, motion);
    let numbering = w.dats.portal_dat().command_numbering();
    let msg = game_message_update_motion(obj_mut(w, player), &movement_data, numbering);
    enqueue_send(w, session, msg);
}

// ACE: DeveloperCommands.Movement
/// `movement <MotionCommand as short>`: movement testing command.
pub fn movement(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let player = session_player(w, s);
    // `(MotionCommand)Convert.ToInt16(parameters[0])`: the short sign-extends
    let forward = convert::to_int16(&parameters[0])
        .unwrap_or_else(|e| panic!("{e}: Convert.ToInt16({})", parameters[0]));
    let forward_command = MotionCommand(i32::from(forward).cast_unsigned());

    let movement = Motion::from_world_object(w, player, forward_command, 1.0);
    send_update_motion(w, s, player, &movement);

    let movement = Motion::from_world_object(w, player, MotionCommand::Ready, 1.0);
    send_update_motion(w, s, player, &movement);
}

// ACE: DeveloperCommands.MoveTo
/// `MoveTo (distance)`: spawns a training wand in front of you and then moves to that object.
pub fn move_to(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let player = session_player(w, s);
    let training_wand_target: u16 = 12748;

    if !parameters.is_empty() {
        // `distance = Convert.ToInt16(parameters[0])`; the distance is never read
        let _distance = convert::to_int16(&parameters[0])
            .unwrap_or_else(|e| panic!("{e}: Convert.ToInt16({})", parameters[0]));
    }

    let loot = create_new_world_object_by_wcid(w, u32::from(training_wand_target))
        .expect("NullReferenceException: loot");
    let use_radius = obj(w, loot).use_radius();
    let distance = if use_radius.unwrap_or(2.0) > 2.0 {
        use_radius.unwrap_or_default()
    } else {
        2.0
    };
    let mut location = location_of(w, player).in_front_of(f64::from(distance), false);
    location.set_landblock_id(LandblockId::new(
        empyrean_world::entity::position_extensions::get_cell(w, &location),
    ));
    obj_mut(w, loot).set_location(Some(location));

    dispatch::enter_world::enter_world(w, loot);

    player_inventory::handle_action_put_item_in_container(w, player, loot.full(), player.full(), 0);
}

// ACE: DeveloperCommands.BarberShop
/// `barbershop`: displays the barber ui.
pub fn barber_shop(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let s = require(session);
    let player = session_player(w, s);
    obj_mut(w, player).set_barber_active(true);
    let msg = empyrean_world::network::game_event::events::game_event_start_barber::game_event_start_barber(w, s);
    enqueue_send(w, s, msg);
}

// ==================================
// Server
// ==================================

// ACE: DeveloperCommands.HandleListPlayers
/// `listplayers [accesslevel]`: displays all of the active players connected to the server.
pub fn handle_list_players(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let mut message = String::new();
    let mut player_counter: u32 = 0;

    let mut target_access_level: Option<AccessLevel> = None;
    if !parameters.is_empty() {
        if let Some(parsed) = try_parse_i32::<AccessLevel>(&parameters[0], true) {
            target_access_level = Some(AccessLevel(parsed));
        } else {
            match convert::to_uint16(&parameters[0]) {
                Ok(access_level) => {
                    target_access_level = Some(AccessLevel(i32::from(access_level)))
                }
                Err(_) => {
                    write_output_info(
                        w,
                        session,
                        "Invalid AccessLevel value",
                        ChatMessageType::Broadcast,
                    );
                    return;
                }
            }
        }
    }

    if let Some(t) = target_access_level {
        message += &format!("Listing only {}s:\n", t.net_string());
    }

    for player in player_manager::get_all_online(w) {
        let account = obj(w, player)
            .player
            .as_ref()
            .and_then(|p| p.player.account.clone())
            .expect("NullReferenceException: Player.Account");
        if target_access_level.is_some_and(|t| account.access_level != t.0.cast_unsigned()) {
            continue;
        }
        let account_id = player_manager::player_session(w, player)
            .and_then(|s| w.sessions.get(s))
            .map(|s| s.account_id)
            .expect("NullReferenceException: player.Session");
        message += &format!("{} : {account_id}\n", name_of(w, player));
        player_counter += 1;
    }

    message += &format!("Total connected Players: {player_counter}\n");

    write_output_info(w, session, &message, ChatMessageType::Broadcast);
}

// ACE: DeveloperCommands.HandleSaveNow
/// `save-now`: saves your session.
pub fn handle_save_now(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let player = session_player(w, require(session));
    empyrean_world::world_objects::player_database::save_player_to_database(w, player);
}

// ACE: DeveloperCommands.HandleLoadAllLandblocks
/// `loadalllandblocks`: loads all landblocks. This is VERY crude.
///
/// DIVERGE (arch): ACE loads them on a thread-pool task (`Task.Run`) while the world runs on;
/// here the loop runs on the world thread, where every landblock call must run.
pub fn handle_load_all_landblocks(
    w: &mut World,
    session: Option<SessionId>,
    _parameters: &[String],
) {
    write_output_info(
        w,
        session,
        "Loading landblocks. This will likely crash the server. Landblock resources will be loaded async and will continue to do work even after all landblocks have been loaded.",
        ChatMessageType::Broadcast,
    );

    for x in 0..=0xFE_u8 {
        write_output_info(
            w,
            session,
            &format!("Loading landblocks, x = 0x{} of 0xFE....", format(x, "X2")),
            ChatMessageType::Broadcast,
        );

        for y in 0..=0xFE_u8 {
            let blockid = LandblockId::from_xy(x, y);
            landblock_manager::get_landblock(w, blockid, false, false);
        }
    }

    write_output_info(
        w,
        session,
        "Loading landblocks completed. Async landblock resources are likely still loading...",
        ChatMessageType::Broadcast,
    );
}

// ==================================
// World Object Properties
// ==================================

// ACE: DeveloperCommands.HandlePropertyDump
/// `propertydump`: lists all properties for the last world object you examined.
pub fn handle_property_dump(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let Some(target) = last_appraised(w, session) else {
        return;
    };
    let text = format!("\n{}", world_object::debug_output_string(w, target, target));
    system_chat(w, require(session), &text, ChatMessageType::System);
}

// ==================================
// Player Properties
// ==================================

// ACE: DeveloperCommands.HandleWhoAmI
/// `whoami`: shows you your GUIDs.
pub fn handle_who_am_i(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let g = session_player(w, require(session));
    let text = format!(
        "GUID: {} (0x{g}) | ID(low): {} High:{}",
        g.full(),
        g.low(),
        g.high()
    );
    send_server_message(w, session, &text, ChatMessageType::Broadcast);
}

/// `$"{flag.GetType().Name} = {flag.ToString()}" + " (" + (uint)flag + ")"`.
fn flag_line<E: AceEnum>(value: E, raw: String) -> String {
    format!("{} = {} ({raw})", E::TYPE_NAME, value.net_string())
}

/// What `echoflags` writes for `parameters` (exactly two of them): `None` for any other count.
/// The body of the `try`; a `Convert` failure is the catch's text.
#[must_use]
pub fn echo_flags_output(parameters: &[String]) -> Option<String> {
    if parameters.len() != 2 {
        return None;
    }
    let u32_of = || convert::to_uint32(&parameters[1]);
    let body = || -> Result<String, &'static str> {
        #[allow(clippy::cast_possible_truncation)]
        Ok(match parameters[0].to_lowercase().as_str() {
            "descriptionflags" => {
                let v = u32_of()?;
                flag_line(ObjectDescriptionFlag(v.cast_signed()), v.to_string())
            }
            "weenieflags" => {
                let v = u32_of()?;
                flag_line(WeenieHeaderFlag(v), v.to_string())
            }
            "weenieflags2" => {
                let v = u32_of()?;
                flag_line(WeenieHeaderFlag2(v), v.to_string())
            }
            "positionflag" => {
                let v = u32_of()?;
                flag_line(PositionFlags(v), v.to_string())
            }
            "type" => {
                let v = u32_of()?;
                flag_line(ItemType(v), v.to_string())
            }
            "containertype" => {
                let v = u32_of()?;
                flag_line(ContainerType(v.cast_signed()), v.to_string())
            }
            "usable" => {
                // `(Usable)Convert.ToInt64(..)` keeps the low 32 bits; `(Int64)usableType` is
                // their unsigned value
                let v = convert::to_int64(&parameters[1])? as u32;
                flag_line(Usable(v), i64::from(v).to_string())
            }
            "radarbehavior" => {
                let v = u32_of()? as u8;
                flag_line(RadarBehavior(v), v.to_string())
            }
            "physicsdescriptionflags" => {
                let v = u32_of()?;
                flag_line(PhysicsDescriptionFlag(v.cast_signed()), v.to_string())
            }
            "physicsstate" => {
                let v = u32_of()?;
                flag_line(PhysicsState(v.cast_signed()), v.to_string())
            }
            "validlocations" | "currentwieldedlocation" => {
                let v = u32_of()?;
                flag_line(EquipMask(v), v.to_string())
            }
            "priority" => {
                let v = u32_of()?;
                flag_line(CoverageMask(v), v.to_string())
            }
            "radarcolor" => {
                let v = u32_of()? as u8;
                flag_line(RadarColor(v), v.to_string())
            }
            _ => "No valid type to test".to_owned(),
        })
    };
    Some(body().unwrap_or_else(|_| "Exception Error, check input and try again".to_owned()))
}

// ACE: DeveloperCommands.HandleDebugEchoFlags
/// `echoflags [type to test] [int]`: echo flags back to you.
pub fn handle_debug_echo_flags(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    if let Some(debug_output) = echo_flags_output(parameters) {
        write_output_info(w, session, &debug_output, ChatMessageType::Broadcast);
    }
}

// ACE: DeveloperCommands.HandleSetCoin
/// `setcoin <number>`: set coin display, debug only usage.
pub fn handle_set_coin(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let Ok(coins) = convert::to_int32(&parameters[0]) else {
        send_server_message(
            w,
            session,
            "Not a valid number - must be a number between 0 - 2,147,483,647",
            ChatMessageType::Broadcast,
        );
        return;
    };

    let player = session_player(w, s);
    obj_mut(w, player).set_coin_value(Some(coins));
    let msg = empyrean_world::network::game_messages::messages::game_message_private_update_property_int::game_message_private_update_property_int(
        obj_mut(w, player),
        PropertyInt::CoinValue,
        coins,
    );
    enqueue_send(w, s, msg);
}

// ==================================
// Teleport + Positions/Locations
// ==================================

// ACE: DeveloperCommands.HandleDebugTeleportXYZ
/// `telexyz cell x y z qx qy qz qw`: teleport to a location.
pub fn handle_debug_teleport_xyz(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let Some(cell) = dotnet_parse::uint_try_parse(&parameters[0]) else {
        return;
    };

    let mut position_data = [0f32; 7];

    for i in 0..7usize {
        let Some(position) = dotnet_parse::float_try_parse(&parameters[i + 1]) else {
            return;
        };

        position_data[i] = position;
    }

    let player = session_player(w, require(session));
    let pd = position_data;
    let position =
        Position::from_components(cell, pd[0], pd[1], pd[2], pd[3], pd[4], pd[5], pd[6], false);
    player_location::teleport(w, player, &position, false);
}

// ACE: DeveloperCommands.HandleTeleType
// Not ACE's (a fix): the reply prints the saved position the player is
// teleporting to; ACE printed the player's Location right after the teleport started, which is
// still where the player was.
/// `teletype <PositionType>`: teleport to a saved character position.
pub fn handle_tele_type(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    if parameters.is_empty() {
        return;
    }
    let s = require(session);
    let parse_position_string = if utf16_len(&parameters[0]) > 3 {
        substring_to(&parameters[0], 3)
    } else {
        parameters[0].clone()
    };

    if let Some(position_type) =
        try_parse_u16::<PositionType>(&parse_position_string, true).map(PositionType)
    {
        let player = session_player(w, s);
        let saved = obj(w, player).get_position(position_type);
        if player_location::tele_to_position(w, player, position_type) {
            let saved = saved.expect("the position just teleported to");
            let text = format!("{} {saved}", PositionType::Location.net_string());
            system_chat(w, s, &text, ChatMessageType::Broadcast);
        } else {
            system_chat(
                w,
                s,
                &format!(
                    "Error finding saved character position: {}",
                    position_type.net_string()
                ),
                ChatMessageType::Broadcast,
            );
        }
    }
}

// ACE: DeveloperCommands.HandleListPositions
/// `listpositions`: displays all available saved character positions from the database.
pub fn handle_list_positions(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let s = require(session);
    let player = session_player(w, s);
    let pos_dict = obj(w, player).get_all_positions();
    let mut message = "Saved character positions:\n".to_owned();

    for (key, value) in pos_dict.iter() {
        message += &format!("ID: {} Loc: {value}\n", key.0);
    }

    message += &format!("Total positions: {}\n", pos_dict.len());
    system_chat(w, s, &message, ChatMessageType::Broadcast);
}

// ACE: DeveloperCommands.HandleSetPosition
/// `setposition <PositionType>`: saves the supplied character position type to the database.
pub fn handle_set_position(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    if parameters.len() == 1 {
        let parse_position_string = if utf16_len(&parameters[0]) > 19 {
            substring_to(&parameters[0], 19)
        } else {
            parameters[0].clone()
        };

        // The enum labels max character length has been observered as length 19
        // int value can be: 0-27

        if let Some(position_type) =
            try_parse_u16::<PositionType>(&parse_position_string, true).map(PositionType)
        {
            if position_type != PositionType::Undef {
                // Create a new position from the current player location
                let player = session_player(w, s);
                let player_position = Position::from_position(&location_of(w, player));

                // Save the position
                obj_mut(w, player).set_position(position_type, Some(player_position));

                // Report changes to client
                let text = format!(
                    "Set: {} to Loc: {player_position}",
                    position_type.net_string()
                );
                system_chat(w, s, &text, ChatMessageType::Broadcast);
                return;
            }
        }
    }

    system_chat(
        w,
        s,
        "Could not determine the correct position type.\nPlease supply a single integer value from within the range of 1 through 27.",
        ChatMessageType::Broadcast,
    );
}

// ACE: DeveloperCommands.HandleDebugGPS
/// `gps`: display location.
pub fn handle_debug_gps(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let player = session_player(w, require(session));
    let position = location_of(w, player);
    let landblock: u16 = position.landblock_id().landblock();
    let text = format!(
        "Position: [Cell: 0x{} | Offset: {}, {}, {} | Facing: {}, {}, {}, {}]",
        format(landblock, "X4"),
        to_string(position.position_x),
        to_string(position.position_y),
        to_string(position.position_z),
        to_string(position.rotation_x),
        to_string(position.rotation_y),
        to_string(position.rotation_z),
        to_string(position.rotation_w)
    );
    send_server_message(w, session, &text, ChatMessageType::Broadcast);
}

// ==================================
// Titles
// ==================================

// ACE: DeveloperCommands.HandleAddTitle
/// `addtitle [titleid]`: add title to yourself.
pub fn handle_add_title(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    if let Some(title_id) = dotnet_parse::uint_try_parse(&parameters[0]) {
        let player = session_player(w, require(session));
        player_character::add_title(w, player, title_id, false);
    }
}

// ACE: DeveloperCommands.HandleAddAllTitles
/// `addalltitles`: add all titles to yourself.
pub fn handle_add_all_titles(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let player = session_player(w, require(session));
    for title in CharacterTitle::MEMBERS {
        player_character::add_title(w, player, title.0, false);
    }
}

// ==================================
// Experience
// ==================================

/// `[OnlinePlayerNameOrIid (default session.Player), PositiveLong required]` resolved; the player
/// and the amount.
fn resolve_player_and_amount(
    w: &mut World,
    session: Option<SessionId>,
    parameters: &[String],
    error: &str,
) -> Option<(ObjectGuid, i64)> {
    use crate::command_parameter_helpers::{
        resolve_ace_parameters, ACECommandParameter, ACECommandParameterType, AceParamValue,
    };

    let me = session_player(w, require(session));
    let mut ace_params = vec![
        ACECommandParameter {
            r#type: ACECommandParameterType::OnlinePlayerNameOrIid,
            required: false,
            default_value: Some(AceParamValue::Player(me)),
            ..ACECommandParameter::default()
        },
        ACECommandParameter {
            r#type: ACECommandParameterType::PositiveLong,
            required: true,
            error_message: Some(error.to_owned()),
            ..ACECommandParameter::default()
        },
    ];
    if !resolve_ace_parameters(w, session, parameters, &mut ace_params, false) {
        return None;
    }
    let player = ace_params[0]
        .as_player()
        .expect("NullReferenceException: aceParams[0].AsPlayer");
    Some((player, ace_params[1].as_long()))
}

// ACE: DeveloperCommands.HandleGrantXp
/// `grantxp [name] <amount>`: give XP to yourself (or the specified character).
pub fn handle_grant_xp(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    if !parameters.is_empty() {
        if let Some((player, amount)) =
            resolve_player_and_amount(w, session, parameters, "You must specify the amount of xp.")
        {
            player_xp::grant_xp(w, player, amount, XpType::Admin, ShareType::None);

            system_chat(
                w,
                require(session),
                &format!("{} experience granted.", format(amount, "N0")),
                ChatMessageType::Advancement,
            );

            let me = session_player(w, require(session));
            let text = format!(
                "{} granted {} experience to {}.",
                name_of(w, me),
                format(amount, "N0"),
                name_of(w, player)
            );
            player_manager::broadcast_to_audit_channel(w, Some(me), &text);

            return;
        }
    }

    send_server_message(
        w,
        session,
        "Usage: /grantxp [name] 1234 (max 999999999999)",
        ChatMessageType::Broadcast,
    );
}

// ACE: DeveloperCommands.HandleGrantLuminance
/// `grantluminance [name] <amount>`: give luminance to yourself (or the specified character).
pub fn handle_grant_luminance(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    if !parameters.is_empty() {
        if let Some((player, amount)) = resolve_player_and_amount(
            w,
            session,
            parameters,
            "You must specify the amount of luminance.",
        ) {
            player_grant_luminance(w, player, amount, XpType::Admin, ShareType::None);

            system_chat(
                w,
                require(session),
                &format!("{} luminance granted.", format(amount, "N0")),
                ChatMessageType::Advancement,
            );

            let me = session_player(w, require(session));
            let text = format!(
                "{} granted {} luminance to {}.",
                name_of(w, me),
                format(amount, "N0"),
                name_of(w, player)
            );
            player_manager::broadcast_to_audit_channel(w, Some(me), &text);

            return;
        }
    }

    send_server_message(
        w,
        session,
        "Usage: /grantluminance [name] 1234 (max 999999999999)",
        ChatMessageType::Broadcast,
    );
}

// ACE: DeveloperCommands.HandleGrantItemXp
/// `grantitemxp <amount>`: give item XP to the last appraised item.
pub fn handle_grant_item_xp(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let Some(amount) = dotnet_parse::long_try_parse(&parameters[0]) else {
        system_chat(
            w,
            s,
            &format!("Invalid amount {}", parameters[0]),
            ChatMessageType::Broadcast,
        );
        return;
    };

    let Some(item) = last_appraised(w, session) else {
        return;
    };

    if is_player(w, item) {
        player_xp::grant_item_xp(w, item, amount);

        for i in empyrean_world::world_objects::creature_equipment::equipped_objects_values(w, item)
        {
            if world_object_has_item_level(w, i) {
                system_chat(
                    w,
                    s,
                    &format!(
                        "{} experience granted to {}.",
                        format(amount, "N0"),
                        name_of(w, i)
                    ),
                    ChatMessageType::Broadcast,
                );
            }
        }
    } else if world_object_has_item_level(w, item) {
        let me = session_player(w, s);
        player_xp::grant_item_xp_to(w, me, item, amount);

        system_chat(
            w,
            s,
            &format!(
                "{} experience granted to {}.",
                format(amount, "N0"),
                name_of(w, item)
            ),
            ChatMessageType::Broadcast,
        );
    } else {
        system_chat(
            w,
            s,
            &format!("{} is not a levelable item.", name_of(w, item)),
            ChatMessageType::Broadcast,
        );
    }
}

// ACE: DeveloperCommands.HandleSpendAllXp
/// `spendallxp`: spend all available XP on attributes, vitals and skills.
pub fn handle_spend_all_xp(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let player = session_player(w, require(session));
    player_xp::spend_all_xp(w, player, true);

    send_server_message(
        w,
        session,
        "All available xp has been spent. You must now log out for the updated values to take effect.",
        ChatMessageType::Broadcast,
    );
}

// ==================================
// Vitals
// ==================================

// ACE: DeveloperCommands.SetVital
/// `setvital <vital> <value>`: sets the specified vital to a specified value (or by a relative
/// `+`/`-` value).
pub fn set_vital(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let param_vital = parameters[0].to_lowercase();
    let param_value = &parameters[1];

    let first = *param_value
        .encode_utf16()
        .collect::<Vec<u16>>()
        .first()
        .expect("IndexOutOfRangeException: paramValue[0]");
    let rel_value = first == u16::from(b'+') || first == u16::from(b'-');

    let Some(value) = dotnet_parse::int_try_parse(param_value) else {
        send_server_message(
            w,
            session,
            "setvital Error: Invalid set value",
            ChatMessageType::Broadcast,
        );
        return;
    };

    // Parse args...
    let player = session_player(w, require(session));
    let vital = match param_vital.as_str() {
        "health" | "hp" => obj(w, player).health(),
        "stamina" | "stam" | "sp" => obj(w, player).stamina(),
        "mana" | "mp" => obj(w, player).mana(),
        _ => {
            send_server_message(
                w,
                session,
                "setvital Error: Invalid vital",
                ChatMessageType::Broadcast,
            );
            return;
        }
    };

    let delta = if rel_value {
        empyrean_world::world_objects::creature_vitals::update_vital_delta(w, player, vital, value)
    } else {
        dispatch::update_vital::update_vital_uint(w, player, vital, value.cast_unsigned())
    };

    if vital.vital == obj(w, player).health().vital {
        if delta > 0 {
            empyrean_world::entity::damage_history::on_heal(w, player, delta.cast_unsigned());
        } else {
            empyrean_world::entity::damage_history::add(
                w,
                player,
                player,
                DamageType::Health,
                delta.wrapping_neg().cast_unsigned(),
            );
        }
    }
}

// ACE: DeveloperCommands.HandleSetHealth
/// `sethealth <ushort>`: sets your current health to a specific value.
pub fn handle_set_health(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    if !parameters.is_empty() {
        if let Some(health) =
            dotnet_parse::uint_try_parse(&parameters[0]).and_then(|v| u16::try_from(v).ok())
        {
            let player = session_player(w, s);
            let vital = obj(w, player).health();
            vital.set_current(obj_mut(w, player), u32::from(health));
            let current = vital.current(obj(w, player));
            let update_players_health = empyrean_world::network::game_messages::messages::game_message_private_update_attribute2nd_level::game_message_private_update_attribute2nd_level(
                obj_mut(w, player),
                Vital::Health,
                current,
            );
            let message = game_message_system_chat(
                &format!("Attempting to set health to {health}..."),
                ChatMessageType::Broadcast,
            );
            enqueue_send(w, s, update_players_health);
            enqueue_send(w, s, message);
            return;
        }
    }

    send_server_message(
        w,
        session,
        "Usage: /sethealth 200 (max Max Health)",
        ChatMessageType::Broadcast,
    );
}

// ACE: DeveloperCommands.HarmSelf
/// `harmself`: sets all player vitals to 1.
pub fn harm_self(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let player = session_player(w, require(session));
    let health = obj(w, player).health();
    dispatch::update_vital::update_vital(w, player, health, 1);
    let stamina = obj(w, player).stamina();
    dispatch::update_vital::update_vital(w, player, stamina, 1);
    let mana = obj(w, player).mana();
    dispatch::update_vital::update_vital(w, player, mana, 1);
}

// ==================================
// Create Objects in Player Inventory
// ==================================

/// `new HashSet<uint> { ... }` enumerated: first occurrences, in order (no removals).
fn hash_set(ids: &[u32]) -> Vec<u32> {
    let mut seen = HashSet::new();
    ids.iter().copied().filter(|id| seen.insert(*id)).collect()
}

// ACE: DeveloperCommands.AddWeeniesToInventory
fn add_weenies_to_inventory(
    w: &mut World,
    session: Option<SessionId>,
    weenie_ids: &[u32],
    stack_size: Option<u16>,
) {
    let player = session_player(w, require(session));
    for weenie_id in hash_set(weenie_ids) {
        let Some(loot) = create_new_world_object_by_wcid(w, weenie_id) else {
            // weenie doesn't exist
            continue;
        };

        let stack_size_for_this_weenie_id = stack_size.or_else(|| obj(w, loot).max_stack_size());

        if let Some(size) = stack_size_for_this_weenie_id.filter(|&s| s > 1) {
            obj_mut(w, loot).set_stack_size(Some(i32::from(size)));
        }

        try_create_in_inventory(w, player, loot);
    }
}

// ACE: DeveloperCommands.HandleWeapons
/// `weapons`: creates testing items in your inventory.
pub fn handle_weapons(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let weenie_ids = [
        93, 148, 300, 307, 311, 326, 338, 348, 350, 7765, 12748, 12463, 31812,
    ];

    add_weenies_to_inventory(w, session, &weenie_ids, None);
}

// ACE: DeveloperCommands.HandleInv
/// `inv`: creates sample items, foci and containers in your inventory.
pub fn handle_inv(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let weenie_ids = [44, 45, 46, 136, 5893, 15268, 15269, 15270, 15271, 12748];

    add_weenies_to_inventory(w, session, &weenie_ids, None);
}

// ACE: DeveloperCommands.HandleSplits
/// `splits`: creates some stackable items in your inventory for testing.
pub fn handle_splits(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let weenie_ids = [300, 690, 20630, 20631, 31198, 37155];

    add_weenies_to_inventory(w, session, &weenie_ids, None);
}

// ACE: DeveloperCommands.HandleComps
/// `comps`: creates spell component items in your inventory for testing.
pub fn handle_comps(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let weenie_ids = [
        686, 687, 688, 689, 690, 691, 740, 741, 742, 743, 744, 745, 746, 747, 748, 749, 750, 751,
        752, 753, 754, 755, 756, 757, 758, 759, 760, 761, 762, 763, 764, 765, 766, 767, 768, 769,
        770, 771, 772, 773, 774, 775, 776, 777, 778, 779, 780, 781, 782, 783, 784, 785, 786, 787,
        788, 789, 790, 791, 792, 1643, 1644, 1645, 1646, 1647, 1648, 1649, 1650, 1651, 1652, 1653,
        1654, 7299, 7581, 8897, 20631,
    ];

    add_weenies_to_inventory(w, session, &weenie_ids, Some(1));
}

// ACE: DeveloperCommands.HandleFood
/// `food`: creates some food items in your inventory for testing.
pub fn handle_food(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let weenie_ids = [259, 259, 260, 377, 378, 379];

    add_weenies_to_inventory(w, session, &weenie_ids, None);
}

// ACE: DeveloperCommands.HandleCurrency
/// `currency`: creates some currency items in your inventory for testing.
pub fn handle_currency(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let weenie_ids = [273, 20630];

    add_weenies_to_inventory(w, session, &weenie_ids, None);
}

// ACE: DeveloperCommands.HandleCIRandom
/// `cirand <type> (num)`: creates random objects in your inventory.
pub fn handle_ci_random(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let weenie_type = try_parse_u32::<WeenieType>(&parameters[0], true)
        .map(WeenieType)
        .filter(|t| t.is_defined());
    let Some(weenie_type) = weenie_type else {
        send_server_message(
            w,
            session,
            &format!("{} is not a valid WeenieType", parameters[0]),
            ChatMessageType::Broadcast,
        );
        return;
    };

    if !admin_commands::verify_create_weenie_type(weenie_type) {
        send_server_message(
            w,
            session,
            &format!(
                "{} is not a valid WeenieType for create commands",
                weenie_type.net_string()
            ),
            ChatMessageType::Broadcast,
        );
        return;
    }

    let mut num_items = 10;

    if parameters.len() > 1 {
        match dotnet_parse::int_try_parse(&parameters[1]) {
            Some(n) if (1..=50).contains(&n) => num_items = n,
            _ => {
                send_server_message(
                    w,
                    session,
                    "<num to create> must be a number between 1 - 50",
                    ChatMessageType::Broadcast,
                );
                return;
            }
        }
    }

    let items = empyrean_world::factories::loot_generation_factory::create_random_objects_of_type(
        w,
        weenie_type,
        num_items,
    );

    let player = session_player(w, s);
    let mut stuck = Vec::new();

    for item in items {
        let item = item.expect("NullReferenceException: item");
        let is_stuck = item.stuck();
        let item = insert_object(w, item);
        if is_stuck {
            stuck.push(item);
        } else {
            try_create_in_inventory(w, player, item);
        }
    }

    if !stuck.is_empty() {
        let names: Vec<String> = stuck.iter().map(|&i| weenie_class_name(w, i)).collect();
        for i in stuck {
            w.objects.remove(i);
        }
        system_chat(
            w,
            s,
            &format!(
                "You cannot spawn {} in your inventory because it cannot be picked up",
                names.join(", ")
            ),
            ChatMessageType::Broadcast,
        );
    }
}
// ==================================
// Spells
// ==================================

// ACE: DeveloperCommands.HandleAddAllSpells
/// `addallspells`: adds all known spells to your own spellbook.
pub fn handle_add_all_spells(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let player = session_player(w, require(session));
    for spell_level in 1..=8u32 {
        empyrean_world::world_objects::player_spells::learn_spells_in_bulk(
            w,
            player,
            MagicSchool::CreatureEnchantment,
            spell_level,
            true,
        );
        empyrean_world::world_objects::player_spells::learn_spells_in_bulk(
            w,
            player,
            MagicSchool::ItemEnchantment,
            spell_level,
            true,
        );
        empyrean_world::world_objects::player_spells::learn_spells_in_bulk(
            w,
            player,
            MagicSchool::LifeMagic,
            spell_level,
            true,
        );
        empyrean_world::world_objects::player_spells::learn_spells_in_bulk(
            w,
            player,
            MagicSchool::VoidMagic,
            spell_level,
            true,
        );
        empyrean_world::world_objects::player_spells::learn_spells_in_bulk(
            w,
            player,
            MagicSchool::WarMagic,
            spell_level,
            true,
        );
    }
}

// ACE: DeveloperCommands.GetSpellFormula
/// `getspellformula <accountname> <spellid>` (console): tests spell formula calculation.
pub fn get_spell_formula(w: &mut World, _session: Option<SessionId>, parameters: &[String]) {
    use empyrean_dat::file_types::spell_table::SpellBaseExt;

    if parameters.len() != 2 {
        console_write_line("getspellformula <accountname> <spellid>");
        return;
    }

    let Some(spellid) = dotnet_parse::uint_try_parse(&parameters[1]) else {
        console_write_line("getspellformula <accountname> <spellid>");
        return;
    };

    let dats = std::sync::Arc::clone(&w.dats);
    let spell_table = dats.portal_dat().spell_table();
    let comps = dats.portal_dat().spell_components_table();

    let spell = spell_table
        .spells
        .get(&spellid)
        .unwrap_or_else(|| panic!("KeyNotFoundException: SpellTable.Spells[{spellid}]"));
    console_write_line(&format!("Formula for {}", spell.name));
    let words = spell
        .get_spell_words(comps)
        .unwrap_or_else(|e| panic!("{e:?}"));
    console_write_line(&format!("Spell Words: {words}"));
    console_write_line(&spell.description);

    let formula = empyrean_dat::file_types::spell_table::get_spell_formula(
        spell_table,
        spellid,
        &parameters[0],
    )
    .unwrap_or_else(|e| panic!("{e:?}"));

    for (i, f) in formula.iter().enumerate() {
        if let Some(c) = comps.components.get(f) {
            console_write_line(&format!("Comp {i}: {}", c.name));
        } else {
            console_write_line(&format!("Comp {i} : Unknown Component {f}"));
        }
    }

    console_write_line("");
}

// ACE: DeveloperCommands.GetAllSpellFormula
/// `getallspellformula <accountname>` (console): tests spell formula calculation over every
/// spell.
///
/// DIVERGE (forced): ACE enumerates the dat's `Dictionary` (the file's order); the shared decoder
/// keeps the spells in id order.
pub fn get_all_spell_formula(w: &mut World, _session: Option<SessionId>, parameters: &[String]) {
    if parameters.len() != 1 {
        console_write_line("getallspellformula <accountname>");
        return;
    }

    let dats = std::sync::Arc::clone(&w.dats);
    let spell_table = dats.portal_dat().spell_table();
    let comps = dats.portal_dat().spell_components_table();

    for (&spellid, spell) in &spell_table.spells {
        console_write_line(&format!("Formula for {} ({spellid})", spell.name));

        let formula = empyrean_dat::file_types::spell_table::get_spell_formula(
            spell_table,
            spellid,
            &parameters[0],
        )
        .unwrap_or_else(|e| panic!("{e:?}"));

        for (i, f) in formula.iter().enumerate() {
            let c = comps
                .components
                .get(f)
                .unwrap_or_else(|| panic!("KeyNotFoundException: SpellComponents[{f}]"));
            console_write_line(&format!("Comp {i}: {}", c.name));
        }

        console_write_line("");
    }
}

// ACE: DeveloperCommands.ReadDat
/// `readdat` (console): reads the skill table and returns (the rest of ACE's body is commented
/// out).
pub fn read_dat(w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    //int total = 0;
    //uint min = 0x0E010000;
    //uint max = 0x0E01FFFF;

    let _test = w.dats.portal_dat().skill_table();
}

// ==================================
// Quests/Contracts
// ==================================

/// `ContractStage.ToString()`.
fn contract_stage_string(stage: i32) -> String {
    use empyrean_world::network::structure::contract_tracker::ContractStage;
    match stage {
        ContractStage::Available => "Available".to_owned(),
        ContractStage::InProgress => "InProgress".to_owned(),
        ContractStage::DoneOrPendingRepeat => "DoneOrPendingRepeat".to_owned(),
        ContractStage::ProgressCounter => "ProgressCounter".to_owned(),
        other => other.to_string(),
    }
}

/// `$"In {t:%d} days, {t:%h} hours, {t:%m} minutes and, {t:%s} seconds. ({(DateTime.UtcNow + t).ToLocalTime().ToCommonString()})"`.
fn contract_time_line(w: &World, t: empyrean_common::dotnet::TimeSpan) -> String {
    format!(
        "In {} days, {} hours, {} minutes and, {} seconds. ({})",
        t.format("%d"),
        t.format("%h"),
        t.format("%m"),
        t.format("%s"),
        to_common_string(w.now.utc.add_ticks(t.ticks()))
    )
}

/// `contractTracker.Contract.ContractName` (a contract not in the dat is ACE's
/// `NullReferenceException`).
fn contract_name(w: &World, contract_id: u32) -> String {
    empyrean_world::world_objects::managers::contract_manager::get_contract_from_dat(w, contract_id)
        .map(|c| c.strings[0].clone())
        .expect("NullReferenceException: ContractTracker.Contract")
}

// ACE: DeveloperCommands.HandleContract
/// `contract [list | bestow | erase]`: query, stamp, and erase contracts on the targeted player.
pub fn handle_contract(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    use empyrean_world::network::structure::contract_tracker::ContractStage;
    use empyrean_world::world_objects::managers::contract_manager;

    if parameters.is_empty() {
        // todo: display help screen
        return;
    }

    let me = session_player(w, require(session));
    let object_id = selected_object_id(w, me);

    let wo = current_landblock_get_object(w, me, object_id);

    if let Some(player) = wo.filter(|&g| is_player(w, g)) {
        if parameters[0] == "list" {
            let count =
                empyrean_world::world_objects::world_object_networking::shims::player_character(
                    obj(w, player),
                )
                .expect("NullReferenceException: Player.Character")
                .get_contracts_count();
            let mut contracts_hdr = format!(
                "Contract Registry for {} (0x{player}):\n",
                name_of(w, player)
            );
            contracts_hdr += "================================================\n";
            contracts_hdr += &format!("Contracts.Count: {count}\n");
            contracts_hdr += "================================================\n";
            let mut contracts = String::new();
            for (_, contract_tracker) in contract_manager::contract_tracker_table(w, player) {
                contracts += &format!(
                    "Contract Id: {} | Contract Name: {}\nStage: {}\n",
                    contract_tracker.contract_id,
                    contract_name(w, contract_tracker.contract_id),
                    contract_stage_string(contract_tracker.stage)
                );

                if contract_tracker.stage == ContractStage::InProgress {
                    let time_when_done = empyrean_common::dotnet::TimeSpan::from_seconds(
                        f64::from(CsCast::<i32>::cs_cast(contract_tracker.time_when_done)),
                    );

                    // (`TimeSpan.MinValue`/`MaxValue` cannot come from an int of seconds)
                    if time_when_done.total_seconds() == 0.0 {
                        contracts += &format!(
                            "TimeWhenDone: Expired ({})\n",
                            to_string(contract_tracker.time_when_done)
                        );
                    } else {
                        contracts +=
                            &format!("TimeWhenDone: {}\n", contract_time_line(w, time_when_done));
                    }
                }

                if contract_tracker.stage == ContractStage::DoneOrPendingRepeat {
                    let time_when_repeats = empyrean_common::dotnet::TimeSpan::from_seconds(
                        f64::from(CsCast::<i32>::cs_cast(contract_tracker.time_when_repeats)),
                    );

                    // Not ACE's (a fix): the Available line prints
                    // TimeWhenRepeats, the value it describes; ACE printed TimeWhenDone.
                    if time_when_repeats.total_seconds() == 0.0 {
                        contracts += &format!(
                            "TimeWhenRepeats: Available ({})\n",
                            to_string(contract_tracker.time_when_repeats)
                        );
                    } else {
                        contracts += &format!(
                            "TimeWhenRepeats: {}\n",
                            contract_time_line(w, time_when_repeats)
                        );
                    }
                }

                contracts += "--====--\n";
            }

            let text = format!(
                "{contracts_hdr}{}",
                if contracts.is_empty() {
                    "No contracts found."
                } else {
                    contracts.as_str()
                }
            );
            player_send_message(w, me, &text);
            return;
        }

        if parameters[0] == "bestow" {
            if parameters.len() < 2 {
                // delete all contracts?
                // seems unsafe, maybe a confirmation?
                return;
            }

            let Some(contract_id) = dotnet_parse::uint_try_parse(&parameters[1]) else {
                return;
            };

            let Some(dat_contract_name) = contract_manager::get_contract_from_dat(w, contract_id)
                .map(|c| c.strings[0].clone())
            else {
                player_send_message(
                    w,
                    me,
                    &format!("Unable to find contract for id {contract_id} in dat file."),
                );
                return;
            };

            if contract_manager::has_contract(w, player, contract_id) {
                let text = format!(
                    "{} already has the contract for \"{dat_contract_name}\" ({contract_id})",
                    name_of(w, player)
                );
                player_send_message(w, me, &text);
                return;
            }

            let has_contract = contract_manager::has_contract(w, player, contract_id);
            if has_contract {
                let text = format!("Couldn't bestow {contract_id} on {}", name_of(w, player));
                player_send_message(w, me, &text);
            } else {
                contract_manager::add(w, player, contract_id);
                let text = format!(
                    "Contract for \"{dat_contract_name}\" ({contract_id}) bestowed on {}",
                    name_of(w, player)
                );
                player_send_message(w, me, &text);
            }
            return;
        }

        if parameters[0] == "erase" {
            if parameters.len() < 2 {
                // delete all contracts?
                // seems unsafe, maybe a confirmation?
                player_send_message(w, me, "You must specify a contract to delete, if you want to delete all contracts use the following command: /contract delete *");
                return;
            }

            if parameters[1] == "*" {
                contract_manager::erase_all(w, player);
                let text = format!("All contracts deleted for {}.", name_of(w, player));
                player_send_message(w, me, &text);
                return;
            }

            let Some(contract_id) = dotnet_parse::uint_try_parse(&parameters[1]) else {
                return;
            };

            let Some(dat_contract_name) = contract_manager::get_contract_from_dat(w, contract_id)
                .map(|c| c.strings[0].clone())
            else {
                player_send_message(
                    w,
                    me,
                    &format!("Unable to find contract for id {contract_id} in dat file."),
                );
                return;
            };

            if !contract_manager::has_contract(w, player, contract_id) {
                let text = format!(
                    "{dat_contract_name} ({contract_id}) not found in {}'s registry.",
                    name_of(w, player)
                );
                player_send_message(w, me, &text);
                return;
            }
            contract_manager::erase(w, player, contract_id);
            let text = format!(
                "{dat_contract_name} ({contract_id}) deleted for {}.",
                name_of(w, player)
            );
            player_send_message(w, me, &text);
        }
    } else if let Some(wo) = wo {
        let text = format!(
            "Selected object {} (0x{object_id}) is not a player.",
            name_of(w, wo)
        );
        player_send_message(w, me, &text);
    } else {
        player_send_message(
            w,
            me,
            &format!("Selected object (0x{object_id}) not found."),
        );
    }
}

// ==================================
// Monster movement
// ==================================

/// `session.Player.CurrentAppraisalTarget`, then the object on the player's landblock; the
/// console lines ACE writes when either is missing.
fn appraisal_target_on_landblock(w: &World, player: ObjectGuid) -> Option<ObjectGuid> {
    let Some(target_id) = obj(w, player).current_appraisal_target() else {
        console_write_line("ERROR: no appraisal target");
        return None;
    };
    let target_guid = ObjectGuid::new(target_id);
    let target = current_landblock_get_object(w, player, target_guid);
    if target.is_none() {
        console_write_line(&format!("Couldn't find {target_guid}"));
    }
    target
}

// ACE: DeveloperCommands.HandleRequestTurnTo
/// `turnto`: turns the last appraised object to the player.
pub fn handle_request_turn_to(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    // get the last appraised object
    let me = session_player(w, require(session));
    let Some(target) = appraisal_target_on_landblock(w, me) else {
        return;
    };
    if !is_creature(w, target) {
        console_write_line(&format!(
            "{} is not a creature / monster",
            name_of(w, target)
        ));
        return;
    }
    empyrean_world::world_objects::creature_navigation::turn_to_target(w, target, me, true);
}

// ACE: DeveloperCommands.ToggleMovementDebug
/// `debugmove <on/off>`: toggles movement debugging for the last appraised monster.
pub fn toggle_movement_debug(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // get the last appraised object
    let Some(creature) = last_appraised(w, session).filter(|&g| is_creature(w, g)) else {
        return;
    };

    let mut enabled = true;
    if !parameters.is_empty() && parameters[0] == "off" {
        enabled = false;
    }

    obj_mut(w, creature)
        .creature
        .as_mut()
        .expect("a creature")
        .monster_navigation
        .debug_move = enabled;
}

// ACE: DeveloperCommands.HandleVisible
/// `lostest`: tests for direct visibility with the latest appraised object.
pub fn handle_visible(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    // get the last appraised object
    let me = session_player(w, require(session));
    let Some(target) = appraisal_target_on_landblock(w, me) else {
        return;
    };

    let visible = world_object::is_direct_visible(w, me, target);
    console_write_line(&format!("Visible: {}", bool_string(visible)));
}

// ACE: DeveloperCommands.HandleShowStats
/// `showstats`: shows a list of a creature's current attribute/skill levels.
pub fn handle_show_stats(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    use empyrean_world::world_objects::entity::creature_attribute::StatCtx;

    let me = session_player(w, require(session));
    // get the last appraised object
    let Some(creature) = last_appraised(w, session).filter(|&g| is_creature(w, g)) else {
        player_send_message(
            w,
            me,
            "ERROR: You must appraise a creature or player to use this function.",
        );
        return;
    };

    let attr = |w: &mut World, a: PropertyAttribute| -> u32 {
        let attribute = *obj(w, creature)
            .attributes()
            .get(&a)
            .unwrap_or_else(|| panic!("KeyNotFoundException: Attributes[{}]", a.net_string()));
        attribute.current(&mut StatCtx::in_world(w, creature))
    };
    let mut output = format!("Strength: {}", attr(w, PropertyAttribute::Strength));
    output += &format!("\nEndurance: {}", attr(w, PropertyAttribute::Endurance));
    output += &format!(
        "\nCoordination: {}",
        attr(w, PropertyAttribute::Coordination)
    );
    output += &format!("\nQuickness: {}", attr(w, PropertyAttribute::Quickness));
    output += &format!("\nFocus: {}", attr(w, PropertyAttribute::Focus));
    output += &format!("\nSelf: {}", attr(w, PropertyAttribute::Self_));

    let vital = |w: &mut World,
                 v: fn(
        &WorldObject,
    )
        -> empyrean_world::world_objects::entity::creature_vital::CreatureVital|
     -> (u32, u32) {
        let vital = v(obj(w, creature));
        let current = vital.current(obj(w, creature));
        (
            current,
            vital.max_value(&mut StatCtx::in_world(w, creature)),
        )
    };
    let (c, m) = vital(w, WorldObject::health);
    output += &format!("\n\nHealth: {c}/{m}");
    let (c, m) = vital(w, WorldObject::stamina);
    output += &format!("\nStamina: {c}/{m}");
    let (c, m) = vital(w, WorldObject::mana);
    output += &format!("\nMana: {c}/{m}");

    // `Skills.Values.Where(..).OrderBy(s => s.Skill.ToString())`: a stable sort under the culture
    // comparer
    let skills: Vec<_> = obj(w, creature).skills().values().copied().collect();
    let mut classes: [Vec<(String, _)>; 4] = Default::default();
    for s in skills {
        let o = obj(w, creature);
        let class = s.advancement_class(o);
        let group = if class == SkillAdvancementClass::Specialized {
            0
        } else if class == SkillAdvancementClass::Trained {
            1
        } else if class == SkillAdvancementClass::Untrained && s.is_usable(w, o) {
            2
        } else if class == SkillAdvancementClass::Untrained {
            3
        } else {
            continue;
        };
        classes[group].push((s.skill.net_string(), s));
    }
    for group in &mut classes {
        group.sort_by(|a, b| admin_commands::culture_compare(&a.0, &b.0));
    }

    for (group, title) in classes.into_iter().zip([
        "\n\n== Specialized ==",
        "\n\n== Trained ==",
        "\n\n== Untrained ==",
        "\n\n== Unusable ==",
    ]) {
        if !group.is_empty() {
            output += title;
            for (name, skill) in group {
                let current = skill.current(w, creature);
                output += &format!("\n{name}: {current}");
            }
        }
    }

    player_send_message(w, me, &output);
}

// ACE: DeveloperCommands.HandleGiveMana
/// `givemana <amount>`: gives mana to the last appraised object.
pub fn handle_give_mana(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    if parameters.is_empty() {
        return;
    }
    let mut amount = convert::to_int32(&parameters[0])
        .unwrap_or_else(|e| panic!("{e}: Int32.Parse({})", parameters[0]));

    let Some(o) = last_appraised(w, session) else {
        return;
    };

    let (max, cur) = (obj(w, o).item_max_mana(), obj(w, o).item_cur_mana());
    amount = amount.min(max.unwrap_or(0).wrapping_sub(cur.unwrap_or(0)));
    // `ItemCurMana += amount`: a null ItemCurMana stays null
    let new_cur = cur.map(|c| c.wrapping_add(amount));
    obj_mut(w, o).set_item_cur_mana(new_cur);
    let text = format!("You give {amount} points of mana to the {}.", name_of(w, o));
    system_chat(w, require(session), &text, ChatMessageType::Magic);
}

// ACE: DeveloperCommands.HandleDist
/// `dist`: returns the distance to the last appraised object.
pub fn handle_dist(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let s = require(session);
    let Some(o) = last_appraised(w, session).filter(|&g| obj(w, g).phys.is_some()) else {
        return;
    };

    let me = session_player(w, s);
    let source_pos = to_global(&location_of(w, me));
    let target_pos = to_global(&location_of(w, o));

    let dist = vec3_distance(source_pos, target_pos);
    let dist2d = vec2_distance(source_pos, target_pos);

    let cyl_dist =
        empyrean_world::world_objects::monster_navigation::get_distance_to_object(w, me, o, true);

    system_chat(
        w,
        s,
        &format!("Dist: {}", to_string(dist)),
        ChatMessageType::Broadcast,
    );
    system_chat(
        w,
        s,
        &format!("2D Dist: {}", to_string(dist2d)),
        ChatMessageType::Broadcast,
    );

    system_chat(
        w,
        s,
        &format!("CylDist: {}", to_string(cyl_dist)),
        ChatMessageType::Broadcast,
    );
}

// ACE: DeveloperCommands.HandleTeleportDist
/// `teledist <distance>`: teleports some distance ahead of the last object spawned.
pub fn handle_teleport_dist(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    if parameters.is_empty() {
        return;
    }

    let last_spawn_pos = admin_commands::LAST_SPAWN_POS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .expect("NullReferenceException: AdminCommands.LastSpawnPos");

    let distance = dotnet_parse::float_try_parse(&parameters[0])
        .unwrap_or_else(|| panic!("FormatException: float.Parse({})", parameters[0]));

    let me = session_player(w, require(session));
    let mut new_pos = Position::new();
    new_pos.set_landblock_id(LandblockId::new(last_spawn_pos.landblock_id().raw()));
    new_pos.set_pos(last_spawn_pos.pos());
    new_pos.set_rotation(location_of(w, me).rotation());

    let dir = Vector3::normalize(Vector3::transform(
        Vector3::new(0.0, 1.0, 0.0),
        new_pos.rotation(),
    ));
    let offset = dir * distance;

    new_pos.set_position(new_pos.pos() + offset);

    player_location::teleport(w, me, &new_pos, false);

    let glob_last_spawn_pos = to_global(&last_spawn_pos);
    let glob_new_pos = to_global(&new_pos);

    let total_dist = vec3_distance(glob_last_spawn_pos, glob_new_pos);

    let total_dist2d = vec2_distance(glob_last_spawn_pos, glob_new_pos);

    let text = format!(
        "Teleporting player to {} @ {}",
        format(new_pos.cell(), "X8"),
        vec3_string(new_pos.pos())
    );
    send_server_message(w, session, &text, ChatMessageType::System);

    send_server_message(
        w,
        session,
        &format!("2D Distance: {}", to_string(total_dist2d)),
        ChatMessageType::System,
    );
    send_server_message(
        w,
        session,
        &format!("3D Distance: {}", to_string(total_dist)),
        ChatMessageType::System,
    );
}

// ACE: DeveloperCommands.GetObjectMaintTarget
/// The target of the object-maintenance dumps: the player, the last appraised object
/// (`target`), or the world object of a physics object by hex guid.
pub fn get_object_maint_target(
    w: &mut World,
    session: Option<SessionId>,
    parameters: &[String],
) -> Option<ObjectGuid> {
    let s = require(session);
    let mut target = Some(session_player(w, s));

    if !parameters.is_empty() {
        let target_type = &parameters[0];

        if eq_ignore_case(target_type, "target") {
            target = last_appraised(w, session);
        } else if let Some(target_guid) = dotnet_parse::uint_try_parse_hex(target_type) {
            if let Some(physics_obj) =
                empyrean_world::physics::server_object_manager::get_object_a(w, target_guid)
            {
                target =
                    empyrean_world::physics::phys_ext::weenie_obj(w, physics_obj).world_object(w);
            }
        }
    }
    if target.is_none() {
        let param = if parameters.is_empty() {
            String::new()
        } else {
            format!(" {}", parameters[0])
        };
        system_chat(
            w,
            s,
            &format!("Couldn't find target{param}"),
            ChatMessageType::Broadcast,
        );
    }
    target
}

/// `target.PhysicsObj`, which ACE dereferences.
fn physics_obj(w: &World, target: ObjectGuid) -> dereth_physics::PhysHandle {
    obj(w, target)
        .phys
        .expect("NullReferenceException: target.PhysicsObj")
}

/// `PhysicsObj.Name`: its world object's name, or "NULL".
fn phys_name(w: &World, h: dereth_physics::PhysHandle) -> String {
    match empyrean_world::physics::phys_ext::weenie_obj(w, h).world_object(w) {
        Some(wo) if w.objects.get(wo).is_some() => name_of(w, wo),
        _ => "NULL".to_owned(),
    }
}

/// `$"{obj.Name} ({obj.ID:X8})"`.
fn phys_line(w: &World, h: dereth_physics::PhysHandle) -> String {
    format!(
        "{} ({})",
        phys_name(w, h),
        format(
            empyrean_world::physics::phys_ext::id(w, h).unwrap_or_default(),
            "X8"
        )
    )
}

/// The dump the object-maintenance commands write to the console: a header, then a line per
/// object.
fn object_maint_dump(w: &World, header: &str, count: usize, objs: &[dereth_physics::PhysHandle]) {
    console_write_line(&format!("\n{header}: {count}"));

    for &o in objs {
        console_write_line(&phys_line(w, o));
    }
}

// ACE: DeveloperCommands.HandleKnownObjs
/// `knownobjs [guid|target]`: shows the list of objects currently known to an object.
pub fn handle_known_objs(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    use empyrean_world::physics::object_maint;
    let Some(target) = get_object_maint_target(w, session, parameters) else {
        return;
    };
    let h = physics_obj(w, target);
    let header = format!("Known objects to {}", name_of(w, target));
    object_maint_dump(
        w,
        &header,
        object_maint::get_known_objects_count(w, h),
        &object_maint::get_known_objects_values(w, h),
    );
}

// ACE: DeveloperCommands.HandleVisibleObjs
/// `visibleobjs [guid|target]`: shows the list of objects currently visible to an object.
pub fn handle_visible_objs(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    use empyrean_world::physics::object_maint;
    let Some(target) = get_object_maint_target(w, session, parameters) else {
        return;
    };
    let h = physics_obj(w, target);
    let header = format!("Visible objects to {}", name_of(w, target));
    object_maint_dump(
        w,
        &header,
        object_maint::get_visible_objects_count(w, h),
        &object_maint::get_visible_objects_values(w, h),
    );
}

// ACE: DeveloperCommands.HandleKnownPlayers
/// `knownplayers [guid|target]`: shows the list of players known to an object.
pub fn handle_known_players(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    use empyrean_world::physics::object_maint;
    let Some(target) = get_object_maint_target(w, session, parameters) else {
        return;
    };
    let h = physics_obj(w, target);
    let header = format!("Known players to {}", name_of(w, target));
    object_maint_dump(
        w,
        &header,
        object_maint::get_known_players_count(w, h),
        &object_maint::get_known_players_values(w, h),
    );
}

// ACE: DeveloperCommands.HandleVisiblePlayers
/// `visibleplayers [guid|target]`: shows the list of players visible to a player.
pub fn handle_visible_players(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    use empyrean_world::physics::{object_maint, phys_ext};
    let Some(target) = get_object_maint_target(w, session, parameters) else {
        return;
    };
    let h = physics_obj(w, target);
    let header = format!("Visible players to {}", name_of(w, target));
    let count =
        object_maint::get_visible_objects_values_where(w, h, |o| phys_ext::is_player(w, o)).len();
    let objs = object_maint::get_visible_objects_values_where(w, h, |o| phys_ext::is_player(w, o));
    object_maint_dump(w, &header, count, &objs);
}

// ACE: DeveloperCommands.HandleVisibleTargets
/// `visibletargets [guid|target]`: shows the list of targets currently visible to a monster.
pub fn handle_visible_targets(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    use empyrean_world::physics::object_maint;
    let Some(target) = get_object_maint_target(w, session, parameters) else {
        return;
    };
    let h = physics_obj(w, target);
    let header = format!("Visible targets to {}", name_of(w, target));
    object_maint_dump(
        w,
        &header,
        object_maint::get_visible_targets_count(w, h),
        &object_maint::get_visible_targets_values(w, h),
    );
}

// ACE: DeveloperCommands.HandleRetaliateTargets
/// `retaliatetargets [guid|target]`: shows the list of retaliate targets for a monster.
pub fn handle_retaliate_targets(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    use empyrean_world::physics::object_maint;
    let Some(target) = get_object_maint_target(w, session, parameters) else {
        return;
    };
    let h = physics_obj(w, target);
    let header = format!("Retaliate targets to {}", name_of(w, target));
    object_maint_dump(
        w,
        &header,
        object_maint::get_retaliate_targets_count(w, h),
        &object_maint::get_retaliate_targets_values(w, h),
    );
}

// ACE: DeveloperCommands.HandleDestructionQueue
/// `destructionqueue [guid|target]`: shows the list of previously visible objects queued for
/// destruction for a player.
pub fn handle_destruction_queue(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    use empyrean_world::physics::{object_maint, phys_ext};
    let Some(target) = get_object_maint_target(w, session, parameters) else {
        return;
    };
    let h = physics_obj(w, target);

    console_write_line(&format!(
        "\nDestruction queue for {}: {}",
        name_of(w, target),
        object_maint::get_destruction_queue_count(w, h)
    ));

    let current_time = phys_ext::physics_timer_current_time(w);

    for (o, t) in object_maint::get_destruction_queue_copy(w, h) {
        console_write_line(&format!(
            "{}: {}",
            phys_line(w, o),
            to_string(t - current_time)
        ));
    }
}

// ACE: DeveloperCommands.HandleDebugEmote
/// `debugemote`: enables emote debugging for the last appraised object.
pub fn handle_debug_emote(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    if let Some(o) = last_appraised(w, session) {
        console_write_line(&format!("Showing emotes for {}", name_of(w, o)));
        obj_mut(w, o).wo.world_object.emote_manager.debug = true;
    }
}

/// `PhysicsObj.Position.ToString()`: `0x{ObjCellID:X8} [x y z] w x y z`.
fn physics_position_string(w: &World, h: dereth_physics::PhysHandle) -> String {
    let p = empyrean_world::physics::phys_ext::position(w, h)
        .expect("NullReferenceException: PhysicsObj.Position");
    let (o, r) = (p.frame.origin, p.frame.rotation);
    format!(
        "0x{} [{} {} {}] {} {} {} {}",
        format(p.cell.0, "X8"),
        to_string(o.x),
        to_string(o.y),
        to_string(o.z),
        to_string(r.w),
        to_string(r.x),
        to_string(r.y),
        to_string(r.z)
    )
}

// ACE: DeveloperCommands.HandleMyLoc
/// `myloc`: shows the current player location, from the server perspective.
pub fn handle_my_loc(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let s = require(session);
    let me = session_player(w, s);
    let lb = obj(w, me)
        .current_landblock
        .expect("NullReferenceException: CurrentLandblock");
    system_chat(
        w,
        s,
        &format!("CurrentLandblock: {}", format(lb.landblock(), "X4")),
        ChatMessageType::Broadcast,
    );
    system_chat(
        w,
        s,
        &format!("Location: {}", location_of(w, me).to_loc_string()),
        ChatMessageType::Broadcast,
    );
    let physics = physics_position_string(w, physics_obj(w, me));
    system_chat(
        w,
        s,
        &format!("Physics : {physics}"),
        ChatMessageType::Broadcast,
    );
}
// ==================================
// Properties
// ==================================

/// The seven property types `getproperty`/`setproperty` accept, by `propType` (ordinal ignoring
/// case).
#[derive(Clone, Copy, PartialEq, Eq)]
enum PropType {
    Int,
    Int64,
    Bool,
    Float,
    String,
    InstanceId,
    DataId,
}

fn prop_type_of(prop_type: &str) -> Option<PropType> {
    [
        ("PropertyInt", PropType::Int),
        ("PropertyInt64", PropType::Int64),
        ("PropertyBool", PropType::Bool),
        ("PropertyFloat", PropType::Float),
        ("PropertyString", PropType::String),
        ("PropertyInstanceId", PropType::InstanceId),
        ("PropertyDataId", PropType::DataId),
    ]
    .into_iter()
    .find(|(name, _)| eq_ignore_case(prop_type, name))
    .map(|(_, t)| t)
}

/// `Enum.TryParse(pType, propName, true, out var result)`: the property's value.
fn prop_try_parse(t: PropType, prop_name: &str) -> Option<u16> {
    match t {
        PropType::Int => try_parse_u16::<PropertyInt>(prop_name, true),
        PropType::Int64 => try_parse_u16::<PropertyInt64>(prop_name, true),
        PropType::Bool => try_parse_u16::<PropertyBool>(prop_name, true),
        PropType::Float => try_parse_u16::<PropertyFloat>(prop_name, true),
        PropType::String => try_parse_u16::<PropertyString>(prop_name, true),
        PropType::InstanceId => try_parse_u16::<PropertyInstanceId>(prop_name, true),
        PropType::DataId => try_parse_u16::<PropertyDataId>(prop_name, true),
    }
}

/// `prop.Split('.')` into `(propType, propName)`, and both resolved; the chat line ACE sends when
/// one step fails.
fn resolve_property(prop: &str) -> Result<(PropType, u16), String> {
    let props: Vec<&str> = prop.split('.').collect();
    if props.len() != 2 {
        return Err(format!("Unknown {prop}"));
    }

    let (prop_type, prop_name) = (props[0], props[1]);

    let Some(t) = prop_type_of(prop_type) else {
        return Err(format!("Unknown property type: {prop_type}"));
    };

    let Some(result) = prop_try_parse(t, prop_name) else {
        return Err(format!("Couldn't find {prop}"));
    };
    Ok((t, result))
}

// ACE: DeveloperCommands.HandleGetProperty
/// `getproperty <PropertyType.Name>`: gets a property for the last appraised object.
pub fn handle_get_property(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let Some(o) = last_appraised(w, session) else {
        return;
    };

    if parameters.is_empty() {
        return;
    }

    let prop = &parameters[0];

    let (t, result) = match resolve_property(prop) {
        Ok(r) => r,
        Err(text) => {
            system_chat(w, s, &text, ChatMessageType::Broadcast);
            return;
        }
    };

    // `Convert.ToString(nullable)`: null is ""
    let wo = obj(w, o);
    let value = match t {
        PropType::Int => wo.get_property(PropertyInt(result)).map(|v| v.to_string()),
        PropType::Int64 => wo
            .get_property(PropertyInt64(result))
            .map(|v| v.to_string()),
        PropType::Bool => wo
            .get_property(PropertyBool(result))
            .map(|v| bool_string(v).to_owned()),
        PropType::Float => wo.get_property(PropertyFloat(result)).map(to_string),
        PropType::String => wo.get_property(PropertyString(result)),
        PropType::InstanceId => wo
            .get_property(PropertyInstanceId(result))
            .map(|v| v.to_string()),
        PropType::DataId => wo
            .get_property(PropertyDataId(result))
            .map(|v| v.to_string()),
    }
    .unwrap_or_default();

    let text = format!("{} ({o}): {prop} = {value}", name_of(w, o));
    system_chat(w, s, &text, ChatMessageType::Broadcast);
}

/// `value.StartsWith("0x", StringComparison.OrdinalIgnoreCase) ? 16 : 10`.
fn from_base(value: &str) -> u32 {
    let mut c = value.chars();
    if c.next() == Some('0') && matches!(c.next(), Some('x' | 'X')) {
        16
    } else {
        10
    }
}

// ACE: DeveloperCommands.HandleSetProperty
/// `setproperty <PropertyType.Name> <value>`: sets a property for the last appraised object
/// (`null` removes it).
pub fn handle_set_property(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let Some(o) = last_appraised(w, session) else {
        return;
    };

    if parameters.len() < 2 {
        return;
    }

    let prop = &parameters[0];
    let value = &parameters[1];

    let (t, result) = match resolve_property(prop) {
        Ok(r) => r,
        Err(text) => {
            system_chat(w, s, &text, ChatMessageType::Broadcast);
            return;
        }
    };

    let me = session_player(w, s);
    if value == "null" {
        let wo = obj_mut(w, o);
        match t {
            PropType::Int => wo.remove_property(PropertyInt(result)),
            PropType::Int64 => wo.remove_property(PropertyInt64(result)),
            PropType::Bool => wo.remove_property(PropertyBool(result)),
            PropType::Float => wo.remove_property(PropertyFloat(result)),
            PropType::String => wo.remove_property(PropertyString(result)),
            PropType::InstanceId => wo.remove_property(PropertyInstanceId(result)),
            PropType::DataId => wo.remove_property(PropertyDataId(result)),
        }
    } else {
        let converted: Result<(), &'static str> = match t {
            PropType::Int => convert::to_int32_base(value, from_base(value))
                .map(|v| player_update_property_int(w, me, o, PropertyInt(result), Some(v), true)),
            PropType::Int64 => convert::to_int64_base(value, from_base(value)).map(|v| {
                player_update_property_int64(w, me, o, PropertyInt64(result), Some(v), true)
            }),
            PropType::Bool => convert::to_boolean(value).map(|v| {
                player_update_property_bool(w, me, o, PropertyBool(result), Some(v), true)
            }),
            PropType::Float => convert::to_double(value).map(|v| {
                player_update_property_float(w, me, o, PropertyFloat(result), Some(v), true)
            }),
            PropType::String => {
                player_update_property_string(
                    w,
                    me,
                    o,
                    PropertyString(result),
                    Some(value.clone()),
                    true,
                );
                Ok(())
            }
            PropType::InstanceId => convert::to_uint32_base(value, from_base(value)).map(|v| {
                player_update_property_instance_id(
                    w,
                    me,
                    o,
                    PropertyInstanceId(result),
                    Some(v),
                    true,
                )
            }),
            PropType::DataId => convert::to_uint32_base(value, from_base(value)).map(|v| {
                player_update_property_data_id(w, me, o, PropertyDataId(result), Some(v), true)
            }),
        };
        if let Err(e) = converted {
            // `Console.WriteLine(e)`: the exception's type (the .NET text also has a message and a
            // stack trace)
            console_write_line(e);
            return;
        }
    }
    let text = format!("{} ({o}): {prop} = {value}", name_of(w, o));
    system_chat(w, s, &text, ChatMessageType::Broadcast);
    let text = format!(
        "{} changed a property for {} ({o}): {prop} = {value}",
        name_of(w, me),
        name_of(w, o)
    );
    player_manager::broadcast_to_audit_channel(w, Some(me), &text);
}

/// `DateTimeOffset.FromUnixTimeSeconds(seconds).UtcDateTime`.
fn from_unix_time_seconds(seconds: i64) -> empyrean_common::dotnet::DotNetDateTime {
    empyrean_common::dotnet::DotNetDateTime::UNIX_EPOCH
        .add_ticks(seconds.wrapping_mul(empyrean_common::dotnet::datetime::TICKS_PER_SECOND))
}

// ACE: DeveloperCommands.HandleSetPurchaseTime
/// `setpurchasetime`: sets the house purchase time for this player (30 days ago, plus a second).
pub fn handle_set_purchase_time(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    use empyrean_common::time::Time;

    let me = session_player(w, require(session));
    let current_time = w.now.utc;
    console_write_line(&format!("Current time: {}", to_common_string(current_time)));
    // subtract 30 days
    let mut purchase_time =
        current_time.add_ticks(-empyrean_common::dotnet::TimeSpan::from_days(30.0).ticks());
    // add buffer
    purchase_time =
        purchase_time.add_ticks(empyrean_common::dotnet::TimeSpan::from_seconds(1.0).ticks());
    //purchaseTime += TimeSpan.FromMinutes(2);
    let house = player_house(w, me).expect("NullReferenceException: session.Player.House");
    let purchase_unix: u32 = Time::get_unix_time_at(purchase_time).cs_cast();
    let rent_due = from_unix_time_seconds(i64::from(house_get_rent_due(w, house, purchase_unix)));

    let prev = obj(w, me).house_purchase_timestamp().unwrap_or(0);
    let prev_purchase_time = from_unix_time_seconds(i64::from(prev));
    let prev_rent_due = from_unix_time_seconds(i64::from(house_get_rent_due(
        w,
        house,
        prev.cast_unsigned(),
    )));

    console_write_line(&format!(
        "Previous purchase time: {}",
        to_common_string(prev_purchase_time)
    ));
    console_write_line(&format!(
        "New purchase time: {}",
        to_common_string(purchase_time)
    ));

    console_write_line(&format!(
        "Previous rent time: {}",
        to_common_string(prev_rent_due)
    ));
    console_write_line(&format!("New rent time: {}", to_common_string(rent_due)));

    let purchase_timestamp: i32 = Time::get_unix_time_at(purchase_time).cs_cast();
    obj_mut(w, me).set_house_purchase_timestamp(Some(purchase_timestamp));
    let rent_timestamp = house_get_rent_due(w, house, purchase_unix).cast_signed();
    obj_mut(w, me).set_house_rent_timestamp(Some(rent_timestamp));

    house_manager_build_rent_queue(w);
}

/// `Creature.DebugDamageType.ToString()`.
fn debug_damage_string(
    t: empyrean_world::world_objects::world_object_magic::DebugDamageType,
) -> String {
    use empyrean_world::world_objects::world_object_magic::DebugDamageType as D;
    match t {
        D::None => "None".to_owned(),
        D::Attacker => "Attacker".to_owned(),
        D::Defender => "Defender".to_owned(),
        D::All => "All".to_owned(),
        other => other.0.to_string(),
    }
}

// ACE: DeveloperCommands.HandleDebugDamage
/// `debugdamage <attack|defense|all|on|off>`: toggles the display for player damage info.
pub fn handle_debug_damage(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    use empyrean_world::world_objects::world_object_magic::DebugDamageType;

    let s = require(session);
    // get last appraisal creature target
    let Some(target_creature) = last_appraised(w, session).filter(|&g| is_creature(w, g)) else {
        return;
    };

    let get = |w: &World| {
        obj(w, target_creature)
            .creature
            .as_ref()
            .expect("a creature")
            .creature_combat
            .debug_damage
    };
    let set = |w: &mut World, v: DebugDamageType| {
        obj_mut(w, target_creature)
            .creature
            .as_mut()
            .expect("a creature")
            .creature_combat
            .debug_damage = v
    };

    if parameters.is_empty() {
        // toggle
        if get(w) == DebugDamageType::None {
            set(w, DebugDamageType::All);
        } else {
            set(w, DebugDamageType::None);
        }
    } else {
        let param = parameters[0].to_lowercase();
        if param == "on" || param == "all" {
            set(w, DebugDamageType::All);
        } else if param == "off" {
            set(w, DebugDamageType::None);
        } else if param.starts_with("attack") {
            set(w, DebugDamageType::Attacker);
        } else if param.starts_with("defen") {
            set(w, DebugDamageType::Defender);
        } else {
            let text = format!(
                "DebugDamage: - unknown {param} ({})",
                name_of(w, target_creature)
            );
            system_chat(w, s, &text, ChatMessageType::Broadcast);
            return;
        }
    }
    let text = format!(
        "DebugDamage: - {} ({})",
        debug_damage_string(get(w)),
        name_of(w, target_creature)
    );
    system_chat(w, s, &text, ChatMessageType::Broadcast);
    let me = session_player(w, s);
    obj_mut(w, target_creature)
        .creature
        .as_mut()
        .expect("a creature")
        .creature_combat
        .debug_damage_target = me;
}

// ACE: DeveloperCommands.HandleEnableAetheria
/// `enable-aetheria [flags]`: enables the aetheria slots for the player.
pub fn handle_enable_aetheria(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let mut flags = AetheriaBitfield::All.0;

    if !parameters.is_empty() {
        // `int.TryParse(parameters[0], out flags)`: a failed parse leaves 0
        flags = dotnet_parse::int_try_parse(&parameters[0]).unwrap_or(0);
    }

    let me = session_player(w, require(session));
    player_update_property_int(w, me, me, PropertyInt::AetheriaBitfield, Some(flags), false);
}

// ACE: DeveloperCommands.HandleDebugChess
/// `debugchess`: shows the chess move history for a player.
pub fn handle_debug_chess(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let me = session_player(w, require(session));
    chess_match_debug_move(w, me);
}

// ACE: DeveloperCommands.HandleDebugBoard
/// `debugboard`: shows the current chess board state.
pub fn handle_debug_board(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let me = session_player(w, require(session));
    chess_logic_debug_board(w, me);
}

// ACE: DeveloperCommands.HandleTeleDungeon
/// `teledungeon <dungeon name or landblock>`: teleports directly to a dungeon by name or landblock.
pub fn handle_tele_dungeon(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let mut is_block = true;
    let param = &parameters[0];
    if parameters.len() > 1 {
        is_block = false;
    }

    let mut landblock = 0u32;
    if is_block {
        match convert::to_uint32_base(param, 16) {
            Ok(v) => {
                landblock = v;

                if landblock >= 0xFFFF {
                    landblock >>= 16;
                }
            }
            Err(_) => is_block = false,
        }
    }

    // teleport to dungeon landblock
    if is_block {
        handle_tele_dungeon_block(w, session, landblock);
    }
    // teleport to dungeon by name
    else {
        handle_tele_dungeon_name(w, session, parameters);
    }
}

/// `new Position(dest.ObjCellId, dest.OriginX, .. , dest.AnglesW)` of a destination row.
fn destination_position(
    dest: &empyrean_content::models::world::WeeniePropertiesPosition,
) -> Position {
    Position::from_components(
        dest.obj_cell_id,
        dest.origin_x,
        dest.origin_y,
        dest.origin_z,
        dest.angles_x,
        dest.angles_y,
        dest.angles_z,
        dest.angles_w,
        false,
    )
}

/// The world database's portal weenies (`weenie.Type == Portal`), each with its `Destination`
/// position rows, in the query's row order.
///
/// DIVERGE (forced): ACE's LINQ query returns MariaDB's join order; here the rows come in the
/// world content's weenie order, then each weenie's position rows. Reading every weenie also warms
/// the content's weenie cache, which ACE's direct `WorldDbContext` query does not.
fn portal_weenies(w: &World) -> Vec<empyrean_content::models::world::Weenie> {
    w.content
        .get_all_weenies()
        .into_iter()
        .filter(|weenie| weenie.r#type == WeenieType::Portal.0.cast_signed())
        .collect()
}

// ACE: DeveloperCommands.HandleTeleDungeonBlock
/// Teleports to the destination of the first portal leading into `landblock`.
pub fn handle_tele_dungeon_block(w: &mut World, session: Option<SessionId>, landblock: u32) {
    let s = require(session);
    let mut dest = None;
    'rows: for weenie in portal_weenies(w) {
        for wpos in &weenie.weenie_properties_position {
            if wpos.position_type == PositionType::Destination.0
                && wpos.obj_cell_id >> 16 == landblock
            {
                dest = Some(wpos.clone());
                break 'rows;
            }
        }
    }

    let Some(dest) = dest else {
        system_chat(
            w,
            s,
            &format!("Couldn't find dungeon {}", format(landblock, "X4")),
            ChatMessageType::Broadcast,
        );
        return;
    };

    let mut pos = destination_position(&dest);
    world_object::adjust_dungeon(w, &mut pos);

    let me = session_player(w, s);
    player_location::teleport(w, me, &pos, false);
}

// ACE: DeveloperCommands.HandleTeleDungeonName
/// Teleports to the destination of the first portal whose name contains the parameters.
pub fn handle_tele_dungeon_name(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let search_name = parameters.join(" ");

    let mut dest = None;
    'rows: for weenie in portal_weenies(w) {
        for wstr in weenie
            .weenie_properties_string
            .iter()
            .filter(|r| r.r#type == PropertyString::Name.0)
        {
            for wpos in weenie
                .weenie_properties_position
                .iter()
                .filter(|r| r.position_type == PositionType::Destination.0)
            {
                if contains_ignore_case(&wstr.value, &search_name) {
                    dest = Some(wpos.clone());
                    break 'rows;
                }
            }
        }
    }

    let Some(dest) = dest else {
        system_chat(
            w,
            s,
            &format!("Couldn't find dungeon name {search_name}"),
            ChatMessageType::Broadcast,
        );
        return;
    };

    let mut pos = destination_position(&dest);
    world_object::adjust_dungeon(w, &mut pos);

    let me = session_player(w, s);
    player_location::teleport(w, me, &pos, false);
}

// ACE: DeveloperCommands.HandleDungeonName
/// `dungeonname`: shows the dungeon name for the current landblock.
pub fn handle_dungeon_name(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let s = require(session);
    let me = session_player(w, s);
    let landblock = location_of(w, me).landblock();

    let block_start = landblock << 16;
    let block_end = block_start | 0xFFFF;

    let mut results = Vec::new();
    for weenie in portal_weenies(w) {
        for wstr in weenie
            .weenie_properties_string
            .iter()
            .filter(|r| r.r#type == PropertyString::Name.0)
        {
            for wpos in &weenie.weenie_properties_position {
                if wpos.position_type == PositionType::Destination.0
                    && wpos.obj_cell_id >= block_start
                    && wpos.obj_cell_id <= block_end
                {
                    results.push(wstr.value.clone());
                }
            }
        }
    }

    if results.is_empty() {
        system_chat(
            w,
            s,
            &format!("Couldn't find dungeon {}", format(landblock, "X4")),
            ChatMessageType::Broadcast,
        );
        return;
    }

    for result in results {
        let name = string_extensions::trim_end(
            &string_extensions::trim_start(&result, "Portal to "),
            " Portal",
        );

        system_chat(w, s, &name, ChatMessageType::Broadcast);
    }
}

// ACE: DeveloperCommands.HandleClearPhysicsCaches
/// `clearphysicscaches`: clears the physics object caches.
pub fn handle_clear_physics_caches(
    w: &mut World,
    session: Option<SessionId>,
    _parameters: &[String],
) {
    bsp_cache_clear(w);
    gfx_obj_cache_clear(w);
    polygon_cache_clear(w);
    vertex_cache_clear(w);

    write_output_info(
        w,
        session,
        "Physics caches cleared",
        ChatMessageType::Broadcast,
    );
}

// ACE: DeveloperCommands.HandleForceGC
/// `forcegc`: forces .NET garbage collection.
///
/// DIVERGE (forced): there is no garbage collector to run; the command answers as ACE does.
pub fn handle_force_gc(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    write_output_info(
        w,
        session,
        ".NET Garbage Collection forced",
        ChatMessageType::Broadcast,
    );
}

// ACE: DeveloperCommands.HandleForceGC2
/// `forcegc2`: forces .NET garbage collection with a large object heap compaction.
///
/// DIVERGE (forced): there is no garbage collector to run; the command answers as ACE does.
pub fn handle_force_gc2(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    // https://learn.microsoft.com/en-us/dotnet/standard/garbage-collection/fundamentals
    // https://learn.microsoft.com/en-us/dotnet/api/system.runtime.gcsettings.largeobjectheapcompactionmode
    write_output_info(
        w,
        session,
        ".NET Garbage Collection forced with LOH Compact",
        ChatMessageType::Broadcast,
    );
}

/// `obj?.WeenieObj?.WorldObject?.IsDestroyed` as the debug line prints it (null is "").
fn phys_is_destroyed(w: &World, h: dereth_physics::PhysHandle) -> String {
    empyrean_world::physics::phys_ext::weenie_obj(w, h)
        .world_object(w)
        .and_then(|g| w.objects.get(g))
        .map(|o| bool_string(o.wo.world_object.is_destroyed).to_owned())
        .unwrap_or_default()
}

/// The `AuditObjectMaint removed ..` debug line.
fn audit_line(
    w: &World,
    removed: dereth_physics::PhysHandle,
    from: dereth_physics::PhysHandle,
    table: &str,
) -> String {
    let pos = |h| {
        empyrean_world::physics::phys_ext::position(w, h)
            .map(|_| physics_position_string(w, h))
            .unwrap_or_default()
    };
    format!(
        "AuditObjectMaint removed 0x{}:{} (IsDestroyed:{}, Position:{}) from 0x{}:{} (IsDestroyed:{}, Position:{}) [{table}]",
        format(empyrean_world::physics::phys_ext::id(w, removed).unwrap_or_default(), "X8"),
        phys_name(w, removed),
        phys_is_destroyed(w, removed),
        pos(removed),
        format(empyrean_world::physics::phys_ext::id(w, from).unwrap_or_default(), "X8"),
        phys_name(w, from),
        phys_is_destroyed(w, from),
        pos(from)
    )
}

// ACE: DeveloperCommands.HandleAuditObjectMaint
/// `auditobjectmaint`: iterates over physics objects to find leaks.
pub fn handle_audit_object_maint(
    w: &mut World,
    session: Option<SessionId>,
    _parameters: &[String],
) {
    use empyrean_world::physics::object_maint;

    let server_objects: HashSet<u32> = w
        .server_object_manager
        .server_objects
        .keys()
        .copied()
        .collect();

    let mut object_table_errors = 0;
    let mut visible_object_table_errors = 0;
    let mut voyeur_table_errors = 0;

    let values: Vec<_> = w
        .server_object_manager
        .server_objects
        .values()
        .copied()
        .collect();
    for value in values {
        {
            let kvps = object_maint::get_known_objects_where(w, value, |k, _| {
                !server_objects.contains(&k)
            });
            for (_, v) in kvps {
                if object_maint::remove_known_object(w, value, v, false) {
                    log::debug!("{}", audit_line(w, v, value, "ObjectTable"));
                    object_table_errors += 1;
                }
            }
        }

        {
            let kvps = object_maint::get_visible_objects_where(w, value, |k, _| {
                !server_objects.contains(&k)
            });
            for (_, v) in kvps {
                if object_maint::remove_visible_object(w, value, v, false) {
                    log::debug!("{}", audit_line(w, v, value, "VisibleObjectTable"));
                    visible_object_table_errors += 1;
                }
            }
        }

        {
            let kvps = object_maint::get_known_players_where(w, value, |k, _| {
                !server_objects.contains(&k)
            });
            for (_, v) in kvps {
                if object_maint::remove_known_player(w, value, v) {
                    log::debug!("{}", audit_line(w, v, value, "VoyeurTable"));
                    voyeur_table_errors += 1;
                }
            }
        }
    }

    let text = format!(
        "Physics ObjMaint Audit Completed. Errors - objectTable: {object_table_errors}, visibleObjectTable: {visible_object_table_errors}, voyeurTable: {voyeur_table_errors}"
    );
    if session.is_some() {
        write_output_info(w, session, &text, ChatMessageType::Broadcast);
    }
    log::info!("{text}");
}

// ACE: DeveloperCommands.HandleLootGen
/// `lootgen <wcid or classname> <tier>`: generate a piece of loot from the LootGenerationFactory.
pub fn handle_loot_gen(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    // create base item
    let wo = match dotnet_parse::uint_try_parse(&parameters[0]) {
        Some(wcid) => create_new_world_object_by_wcid(w, wcid),
        None => create_new_world_object_by_name(w, &parameters[0]),
    };

    let Some(wo) = wo else {
        system_chat(
            w,
            s,
            &format!("Couldn't find {}", parameters[0]),
            ChatMessageType::Broadcast,
        );
        return;
    };

    let mut tier = 1;
    if parameters.len() > 1 {
        // `int.TryParse(parameters[1], out tier)`: a failed parse leaves 0
        tier = dotnet_parse::int_try_parse(&parameters[1]).unwrap_or(0);
    }

    if !(1..=8).contains(&tier) {
        system_chat(
            w,
            s,
            "Loot Tier must be a number between 1 and 8",
            ChatMessageType::Broadcast,
        );
        drop_unplaced(w, wo);
        return;
    }

    let o = obj(w, wo);
    let is_pet_device =
        empyrean_world::dispatch::class_of(w, wo) == empyrean_world::dispatch::Class::PetDevice;
    if o.tsys_mutation_data().is_none()
        && !empyrean_world::entity::aetheria::is_aetheria(o.biota.weenie_class_id)
        && !is_pet_device
    {
        let text = format!(
            "{} ({}) missing PropertyInt.TsysMutationData",
            name_of(w, wo),
            o.biota.weenie_class_id
        );
        system_chat(w, s, &text, ChatMessageType::Broadcast);
        drop_unplaced(w, wo);
        return;
    }

    let profile = empyrean_content::models::world::TreasureDeath {
        tier,
        loot_quality_mod: 0.0,
        ..Default::default()
    };

    let mut item = *w.objects.remove(wo).expect("the new object");
    let _success = empyrean_world::factories::loot_generation_factory::mutate_item(
        w, &mut item, &profile, true,
    );
    insert_object(w, item);

    let me = session_player(w, s);
    try_create_in_inventory(w, me, wo);
}

// ACE: DeveloperCommands.HandleCILoot
/// `ciloot <tier> (# items)`: generates randomized loot in the player's inventory.
pub fn handle_ci_loot(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // `int.TryParse(.., out tier)` leaves 0 on failure; the clamp makes it 1
    let tier = dotnet_parse::int_try_parse(&parameters[0])
        .unwrap_or(0)
        .clamp(1, 8);

    let mut num_items = 1;
    if parameters.len() > 1 {
        num_items = dotnet_parse::int_try_parse(&parameters[1]).unwrap_or(0);
    }

    // Create a dummy treasure profile for passing in tier value
    let profile = empyrean_content::models::world::TreasureDeath {
        tier,
        loot_quality_mod: 0.0,
        magic_item_treasure_type_selection_chances: 9, // 8 or 9?
        ..Default::default()
    };

    let me = session_player(w, require(session));
    for _ in 0..num_items {
        //var wo = LootGenerationFactory.CreateRandomLootObjects(profile, true);
        let wo = empyrean_world::factories::loot_generation_factory::create_random_loot_objects_of_category(
            w,
            &profile,
            empyrean_tables::enums::TreasureItemCategory::MagicItem,
            empyrean_tables::enums::TreasureItemType::Undef,
        );
        if let Some(wo) = wo {
            let wo = insert_object(w, wo);
            try_create_in_inventory(w, me, wo);
        } else {
            log::error!("{}.HandleCILoot: LootGenerationFactory.CreateRandomLootObjects({tier}) returned null", name_of(w, me));
        }
    }
}

// ACE: DeveloperCommands.HandleMakeIOU
/// `makeiou <wcid>`: make an IOU and put it in your inventory.
pub fn handle_make_iou(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let weenie_class_description = &parameters[0];
    let Some(weenie_class_id) = dotnet_parse::uint_try_parse(weenie_class_description) else {
        system_chat(
            w,
            s,
            "WCID must be a valid weenie id",
            ChatMessageType::Broadcast,
        );
        return;
    };

    if let Some(iou) = empyrean_world::factories::player_factory::create_iou(w, weenie_class_id) {
        let iou = insert_object(w, iou);
        let me = session_player(w, s);
        try_create_in_inventory(w, me, iou);
    }
}

// ACE: DeveloperCommands.HandleTestDeathItems
/// `testdeathitems [name]`: test death item selection.
pub fn handle_test_death_items(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let mut target = session_player(w, s);
    if !parameters.is_empty() {
        let Some(t) = player_manager::get_online_player_by_name(w, &parameters[0]) else {
            system_chat(
                w,
                s,
                &format!("Couldn't find {}", parameters[0]),
                ChatMessageType::Broadcast,
            );
            return;
        };
        target = t;
    }

    let inventory = player_inventory::get_all_possessions(w, target);
    let sorted = empyrean_world::entity::death_item::DeathItems::new(w, &inventory);

    let mut i = 0;
    for item in &sorted.inventory {
        let bonded = obj(w, item.world_object)
            .bonded()
            .unwrap_or(BondedStatus::Normal);

        if bonded != BondedStatus::Normal {
            continue;
        }

        i += 1;
        let text = format!(
            "{i}. {} ({:?}, AdjustedValue: {})",
            item.name.clone().unwrap_or_default(),
            item.category,
            item.adjusted_value
        );
        system_chat(w, s, &text, ChatMessageType::Broadcast);
    }
}
/// The player's `Player` partial fields, which ACE reaches through the object.
fn player_data(w: &World, player: ObjectGuid) -> &empyrean_world::world_objects::kinds::PlayerData {
    obj(w, player).player.as_deref().expect("a player")
}

/// The player's `Player` partial fields, mutably.
fn player_data_mut(
    w: &mut World,
    player: ObjectGuid,
) -> &mut empyrean_world::world_objects::kinds::PlayerData {
    obj_mut(w, player).player.as_deref_mut().expect("a player")
}

/// `player.Session`: the world's session for an online player.
fn player_session(w: &World, player: ObjectGuid) -> Option<SessionId> {
    player_manager::player_session(w, player)
}

// ACE: DeveloperCommands.HandleForceLogout
/// `forcelogout [name]`: force log off of the specified character or the last appraised one.
pub fn handle_force_logout(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    handle_force_logoff(w, session, parameters);
}

// ACE: DeveloperCommands.HandleForceLogoff
/// `forcelogoff [name]`: force log off of the specified character or the last appraised one.
pub fn handle_force_logoff(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    use empyrean_net::enums::SessionTerminationReason;
    use empyrean_world::network::game_messages::messages::game_message_boot_account::game_message_boot_account;

    let mut player_name = String::new();
    if !parameters.is_empty() {
        player_name = parameters.join(" ");
    }

    let target: Option<ObjectGuid>;

    if player_name.is_empty() {
        target = last_appraised(w, session);
    } else {
        let (plr, _) = player_manager::find_by_name(w, &player_name);
        let Some(plr) = plr else {
            write_output_info(
                w,
                session,
                &format!("Unable to force log off for {player_name}: Player not found in manager."),
                ChatMessageType::Broadcast,
            );
            return;
        };
        target = player_manager::get_online_player(w, plr.guid().full());

        if target.is_none() {
            let name = empyrean_world::entity::i_player::name(w, plr).unwrap_or_default();
            write_output_info(
                w,
                session,
                &format!("Unable to force log off for {name}: Player is not online."),
                ChatMessageType::Broadcast,
            );
            return;
        }
    }

    let Some(target) = target else { return };
    if !is_player(w, target) {
        let text = format!(
            "Unable to force log off for {}: Target is not a player.",
            name_of(w, target)
        );
        write_output_info(w, session, &text, ChatMessageType::Broadcast);
        //else
        //    CommandHandlerHelper.WriteOutputInfo(session, $"Unable to force log off for {playerName}: Player not found in manager.");
        return;
    }
    let player = target;

    //if (player.Session != null)
    //    player.Session.LogOffPlayer(true);
    //else
    //    player.LogOut();

    let player_session_id = player_session(w, player);
    let mut msg = format!(
        "Player {} (0x{player}) found in PlayerManager.onlinePlayers.\n",
        name_of(w, player)
    );
    let session_text = match player_session_id.and_then(|s| w.net.session(s)) {
        Some(ns) => format!(
            "C2S: {} | S2C: {}",
            ns.core.end_point_c2s,
            ns.core
                .end_point_s2c
                .map(|e| e.to_string())
                .unwrap_or_default()
        ),
        None => "NULL".to_owned(),
    };
    msg += &format!("------- Session: {session_text}\n");
    let current_landblock = obj(w, player).current_landblock;
    msg += &format!(
        "------- CurrentLandblock: {}\n",
        current_landblock.map_or_else(|| "NULL".to_owned(), |lb| format!("0x{lb}"))
    );
    msg += &format!(
        "------- Location: {}\n",
        obj(w, player)
            .location()
            .map_or_else(|| "NULL".to_owned(), |l| l.to_loc_string())
    );
    msg += &format!(
        "------- IsLoggingOut: {}\n",
        bool_string(player_data(w, player).player.is_logging_out)
    );
    msg += &format!(
        "------- IsInDeathProcess: {}\n",
        bool_string(player_data(w, player).player_death.is_in_death_process)
    );
    let mut found_on_landblock = false;
    if let Some(lb) = current_landblock {
        let lb = landblock_manager::get_landblock(w, lb, false, false);
        found_on_landblock = landblock::get_object(w, lb, player, true).is_some();
    }
    msg += &format!(
        "------- FoundOnLandblock: {}\n",
        bool_string(found_on_landblock)
    );
    let player_forced_log_off_requested = player_data(w, player).player.forced_log_off_requested;
    msg += &format!(
        "------- ForcedLogOffRequested: {}\n",
        bool_string(player_forced_log_off_requested)
    );

    msg += "Log off path taken: ";
    let boot = || {
        game_message_boot_account(Some(
            " because the character was forced to log off by an admin",
        ))
        .into_outbound()
    };
    if player_forced_log_off_requested {
        if let Some(ps) = player_session_id {
            let now = w.now;
            w.net.terminate(
                ps,
                SessionTerminationReason::ForcedLogOffRequested,
                Some(boot()),
                String::new(),
                now,
            );
        }
        empyrean_world::world_objects::player::force_logoff(w, player);
        msg += "player.Session?.Terminate() | player.ForceLogoff()";
    } else if let Some(ps) = player_session_id {
        player_data_mut(w, player).player.forced_log_off_requested = true;
        let now = w.now;
        w.net.terminate(
            ps,
            SessionTerminationReason::ForcedLogOffRequested,
            Some(boot()),
            String::new(),
            now,
        );
        msg += "player.ForcedLogOffRequested = true | player.Session.Terminate()";
    } else if current_landblock.is_some() && found_on_landblock {
        player_data_mut(w, player).player.forced_log_off_requested = true;
        empyrean_world::world_objects::player::log_out(w, player, false, false);
        msg += "player.ForcedLogOffRequested = true | player.LogOut()";
    } else if player_data(w, player).player_death.is_in_death_process {
        player_data_mut(w, player).player.forced_log_off_requested = true;
        player_data_mut(w, player).player_death.is_in_death_process = false;
        empyrean_world::world_objects::player::log_out_inner(w, player, true);
        msg += "player.ForcedLogOffRequested = true | player.IsInDeathProcess = false | player.LogOut_Inner(true)";
    } else {
        player_data_mut(w, player).player.forced_log_off_requested = true;
        msg += "player.ForcedLogOffRequested = true";
    }

    if player_forced_log_off_requested {
        // DIVERGE: points at Empyrean's issue tracker where ACE's names the ACEmulator team's Discord (brand).
        msg += &format!(
            "\nPlease report the above at {}.",
            empyrean_common::brand::ISSUES_URL
        );
    } else {
        msg += "\nUse this command again if this player does not properly log off within the next minute.";
    }

    write_output_info(w, session, &msg, ChatMessageType::Broadcast);

    let issuer = session.and_then(|s| w.sessions.player(s));
    let text = format!("Forcing Log Off of {}...", name_of(w, player));
    player_manager::broadcast_to_audit_channel(w, issuer, &text);
}

// ACE: DeveloperCommands.HandleShowSession
/// `showsession`: shows the IP and ID of the network session of the last appraised character.
pub fn handle_show_session(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let s = require(session);
    let Some(player) = last_appraised(w, session).filter(|&g| is_player(w, g)) else {
        return;
    };

    match player_session(w, player).and_then(|ps| w.net.session(ps)) {
        Some(ns) => {
            let account = obj(w, player)
                .player
                .as_ref()
                .and_then(|p| p.player.account.clone())
                .expect("NullReferenceException: Player.Account");
            let text = format!(
                "Session IP: {} | C2S Port: {} | S2C Port: {} | ClientId: {} is connected to Character: {} (0x{}), Account: {} ({})",
                ns.core.end_point_c2s.ip(),
                ns.core.end_point_c2s.port(),
                ns.core.end_point_s2c.map(|e| e.port().to_string()).unwrap_or_default(),
                ns.network.client_id,
                name_of(w, player),
                format(player.full(), "X8"),
                account.account_name,
                account.account_id
            );
            system_chat(w, s, &text, ChatMessageType::Broadcast);
        }
        None => {
            let text = format!(
                "Session is null for {} which shouldn't occur.",
                name_of(w, player)
            );
            system_chat(w, s, &text, ChatMessageType::Broadcast);
        }
    }
}

// ACE: DeveloperCommands.HandleRequireComps
/// `requirecomps [ on | off ]`: sets whether spell components are required to cast spells.
pub fn handle_require_comps(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    use empyrean_world::network::game_messages::messages::game_message_public_update_property_bool::game_message_public_update_property_bool;

    let s = require(session);
    let me = session_player(w, s);
    let param = &parameters[0];

    let (required, text) = if param == "off" {
        (false, "You can now cast spells without components.")
    } else {
        (true, "You can no longer cast spells without components.")
    };
    obj_mut(w, me).set_spell_components_required(required);
    let value = obj(w, me).spell_components_required();
    let msg = game_message_public_update_property_bool(
        obj_mut(w, me),
        PropertyBool::SpellComponentsRequired,
        value,
    );
    world_object_networking::enqueue_broadcast(w, me, true, &[msg]);
    system_chat(w, s, text, ChatMessageType::Broadcast);
}

// ACE: DeveloperCommands.HandleSafeComps
/// `safecomps <on/off>`: enables / disables spell component burning.
pub fn handle_safe_comps(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let mut safe_comps = true;
    if !parameters.is_empty() && parameters[0].to_lowercase() == "off" {
        safe_comps = false;
    }

    let me = session_player(w, s);
    obj_mut(w, me).set_safe_spell_components(safe_comps);

    if safe_comps {
        system_chat(
            w,
            s,
            "Your spell components are now safe, and will not be consumed when casting spells.",
            ChatMessageType::Broadcast,
        );
    } else {
        system_chat(
            w,
            s,
            "Your spell components will now be consumed when casting spells.",
            ChatMessageType::Broadcast,
        );
    }
}

/// `Enum.TryParse(parameters[0], true, out SpellId spellId)`, then `new Spell(spellId)` unless it
/// is not found; the chat lines ACE sends on either failure.
fn parse_item_spell(
    w: &mut World,
    s: SessionId,
    parameter: &str,
) -> Option<(SpellId, empyrean_world::entity::spell::Spell)> {
    let Some(spell_id) = admin_commands::try_parse_spell_id(parameter, true) else {
        system_chat(
            w,
            s,
            &format!("{parameter} is not a valid spell id"),
            ChatMessageType::Broadcast,
        );
        return None;
    };

    // ensure valid spell id
    let spell = empyrean_world::entity::spell::Spell::new(w, spell_id.0, true);

    if spell.not_found() {
        system_chat(w, s, "SpellID is not found", ChatMessageType::Broadcast);
        return None;
    }
    Some((spell_id, spell))
}

// ACE: DeveloperCommands.HandleAddItemSpell
/// `additemspell <spell id>`: adds a spell to the last appraised item's spellbook.
pub fn handle_add_item_spell(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let Some(o) = last_appraised(w, session) else {
        return;
    };

    let Some((spell_id, spell)) = parse_item_spell(w, s, &parameters[0]) else {
        return;
    };

    let (_, spell_added) = obj_mut(w, o)
        .biota
        .get_or_add_known_spell(spell_id.0.cast_signed(), 2.0);

    let msg = if spell_added {
        "added to"
    } else {
        "already on"
    };

    let text = format!("{} ({}) {msg} {}", spell.name(), spell.id(), name_of(w, o));
    system_chat(w, s, &text, ChatMessageType::Broadcast);
}

// ACE: DeveloperCommands.HandleRemoveItemSpell
/// `removeitemspell <spell id>`: removes a spell from the last appraised item's spellbook.
pub fn handle_remove_item_spell(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let Some(o) = last_appraised(w, session) else {
        return;
    };

    let Some((spell_id, spell)) = parse_item_spell(w, s, &parameters[0]) else {
        return;
    };

    let spell_removed = obj_mut(w, o)
        .biota
        .try_remove_known_spell(spell_id.0.cast_signed());

    let msg = if spell_removed {
        "removed from"
    } else {
        "not found on"
    };

    let text = format!("{} ({}) {msg} {}", spell.name(), spell.id(), name_of(w, o));
    system_chat(w, s, &text, ChatMessageType::Broadcast);
}

// ACE: DeveloperCommands.HandlePKTimer
/// `pktimer`: sets your PK timer to the current time.
pub fn handle_pk_timer(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let s = require(session);
    let me = session_player(w, s);
    empyrean_world::world_objects::player_combat::update_pk_timer(w, me);

    system_chat(w, s, "Updated PK timer", ChatMessageType::Broadcast);
}

// ACE: DeveloperCommands.HandleFellowInfo
/// `fellow-info`: shows debug info for fellowships.
pub fn handle_fellow_info(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    use empyrean_world::entity::fellowship;

    let s = require(session);
    let player = last_appraised(w, session)
        .filter(|&g| is_player(w, g))
        .unwrap_or_else(|| session_player(w, s));

    let Some(fellowship_ref) =
        empyrean_world::world_objects::player_fellowship::fellowship(w, player)
    else {
        system_chat(
            w,
            s,
            "Player target must be in a fellowship to use this command.",
            ChatMessageType::Broadcast,
        );
        return;
    };

    let fellows: Vec<ObjectGuid> = fellowship::get_fellowship_members(w, &fellowship_ref)
        .values()
        .copied()
        .collect();

    let level = |w: &World, f: ObjectGuid| obj(w, f).level();
    let xp_to_next = |w: &World, f: ObjectGuid| {
        player_xp::get_xp_to_next_level(
            w,
            level(w, f).expect("InvalidOperationException: Level.Value"),
        )
    };

    //var levelSum = fellows.Values.Select(f => f.Level.Value).Sum();
    let level_xp_sum: u64 = fellows
        .iter()
        .map(|&f| xp_to_next(w, f))
        .fold(0u64, u64::wrapping_add);

    // this should match up with the client
    let mut by_level = fellows.clone();
    // `OrderBy(f => f.Level)`: stable; a null level sorts first
    by_level.sort_by_key(|&f| level(w, f));
    for fellow in by_level {
        //var levelScale = (double)fellow.Level.Value / levelSum;
        #[allow(clippy::cast_precision_loss)]
        let level_xp_scale = xp_to_next(w, fellow) as f64 / level_xp_sum as f64;

        let pct = empyrean_common::dotnet::math::round_digits(level_xp_scale * 100.0, 2);
        system_chat(
            w,
            s,
            &format!("{}: {}%", name_of(w, fellow), to_string(pct)),
            ChatMessageType::Broadcast,
        );
    }

    system_chat(w, s, "----------", ChatMessageType::Broadcast);

    let (share_xp, even_share) = {
        let f = fellowship_ref.get(w);
        (f.share_xp, f.even_share)
    };
    system_chat(
        w,
        s,
        &format!("ShareXP: {}", bool_string(share_xp)),
        ChatMessageType::Broadcast,
    );
    system_chat(
        w,
        s,
        &format!("EvenShare: {}", bool_string(even_share)),
        ChatMessageType::Broadcast,
    );

    system_chat(w, s, "Distance scale:", ChatMessageType::Broadcast);

    for fellow in fellows {
        let dist = location_of(w, player).distance_2d(&location_of(w, fellow));

        let distance_scalar =
            fellowship::get_distance_scalar(w, Some(player), Some(fellow), XpType::Kill);

        let text = format!(
            "{}: {} ({}) - {}",
            name_of(w, fellow),
            format(empyrean_common::dotnet::math::round(f64::from(dist)), "N0"),
            format(distance_scalar, "F2"),
            location_of(w, fellow)
        );
        system_chat(w, s, &text, ChatMessageType::Broadcast);
    }
}

// ACE: DeveloperCommands.HandleFellowDist
/// `fellow-dist`: shows the distance to each fellowship member.
pub fn handle_fellow_dist(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    use empyrean_world::entity::fellowship;

    let s = require(session);
    let player = session_player(w, s);

    let Some(fellowship_ref) =
        empyrean_world::world_objects::player_fellowship::fellowship(w, player)
    else {
        system_chat(
            w,
            s,
            "You must be in a fellowship to use this command.",
            ChatMessageType::Broadcast,
        );
        return;
    };

    let fellows: Vec<ObjectGuid> = fellowship::get_fellowship_members(w, &fellowship_ref)
        .values()
        .copied()
        .collect();

    for fellow in fellows {
        let dist2d = location_of(w, player).distance_2d(&location_of(w, fellow));
        let dist3d = location_of(w, player).distance_to(&location_of(w, fellow));

        let scalar = fellowship::get_distance_scalar(w, Some(player), Some(fellow), XpType::Kill);

        let text = format!(
            "{} | 2d: {} | 3d: {} | Scalar: {}",
            name_of(w, fellow),
            format(dist2d, "N0"),
            format(dist3d, "N0"),
            format(scalar, "N0")
        );
        system_chat(w, s, &text, ChatMessageType::Broadcast);
    }
}

/// `Time.GetDateTimeFromTimestamp(ts).ToLocalTime().ToCommonString()` (local time is UTC here).
fn timestamp_common_string(ts: f64) -> String {
    to_common_string(empyrean_common::time::Time::get_date_time_from_timestamp(
        ts,
    ))
}

/// `float?`/`int?`/`uint?` interpolated: null is "".
fn opt_string<T: ToString>(v: Option<T>) -> String {
    v.map(|v| v.to_string()).unwrap_or_default()
}

// ACE: DeveloperCommands.HandleGeneratorDump
/// `generatordump`: lists all properties for the last generator you examined.
#[allow(clippy::too_many_lines)]
pub fn handle_generator_dump(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    // TODO: output

    let s = require(session);
    let me = session_player(w, s);
    if !has_selection(w, me) {
        return;
    }
    let object_id = selected_object_id(w, me);

    let Some(wo) = current_landblock_get_object(w, me, object_id) else {
        return;
    };

    if object_id.is_player() {
        return;
    }

    let utc_now = w.now.utc;
    let o = obj(w, wo);
    let msg = if o.is_generator() {
        let mut msg = format!("Generator Dump for {} (0x{})\n", name_of(w, wo), o.guid);
        msg += &format!("Generator WCID: {}\n", o.biota.weenie_class_id);
        msg += &format!("Generator WeenieClassName: {}\n", weenie_class_name(w, wo));
        msg += &format!(
            "Generator WeenieType: {}\n",
            o.biota.weenie_type.net_string()
        );
        msg += &format!(
            "Generator Status: {}\n",
            if o.generator_disabled() {
                "Disabled"
            } else {
                "Enabled"
            }
        );
        msg += &format!("GeneratorType: {}\n", o.generator_type().net_string());
        msg += &format!(
            "GeneratorTimeType: {}\n",
            o.generator_time_type().net_string()
        );
        if o.generator_time_type() == GeneratorTimeType::Event {
            let event = o
                .generator_event()
                .filter(|e| !e.trim().is_empty())
                .unwrap_or_else(|| "Undef".to_owned());
            msg += &format!("GeneratorEvent: {event}\n");
        }
        if o.generator_time_type() == GeneratorTimeType::RealTime {
            let start = o.generator_start_time();
            let end = o.generator_end_time();
            msg += &format!(
                "GeneratorStartTime: {start} ({})\n",
                timestamp_common_string(f64::from(start))
            );
            msg += &format!(
                "GeneratorEndTime: {end} ({})\n",
                timestamp_common_string(f64::from(end))
            );
        }
        msg += &format!(
            "GeneratorEndDestructionType: {}\n",
            o.generator_end_destruction_type().net_string()
        );
        msg += &format!(
            "GeneratorDestructionType: {}\n",
            o.generator_destruction_type().net_string()
        );
        msg += &format!(
            "GeneratorRadius: {}\n",
            to_string(
                o.get_property(PropertyFloat::GeneratorRadius)
                    .unwrap_or(0.0)
            )
        );
        msg += &format!("InitGeneratedObjects: {}\n", o.init_generated_objects());
        msg += &format!("MaxGeneratedObjects: {}\n", o.max_generated_objects());
        msg += &format!(
            "GeneratorInitialDelay: {}\n",
            to_string(o.generator_initial_delay())
        );
        msg += &format!(
            "RegenerationInterval: {}\n",
            to_string(o.regeneration_interval())
        );
        let update = o.generator_update_timestamp();
        msg += &format!(
            "GeneratorUpdateTimestamp: {} ({})\n",
            to_string(update),
            timestamp_common_string(update)
        );
        let next_update =
            empyrean_world::world_objects::world_object_tick::next_generator_update_time(o);
        let next_update_text = if next_update == f64::MAX {
            "Disabled".to_owned()
        } else {
            timestamp_common_string(next_update)
        };
        msg += &format!(
            "NextGeneratorUpdateTime: {} ({next_update_text})\n",
            to_string(next_update)
        );
        let regen = o.regeneration_timestamp();
        msg += &format!(
            "RegenerationTimestamp: {} ({})\n",
            to_string(regen),
            timestamp_common_string(regen)
        );
        let next_regen =
            empyrean_world::world_objects::world_object_tick::next_generator_regeneration_time(o);
        let next_regen_text = if next_regen == f64::MAX {
            "On Demand".to_owned()
        } else {
            timestamp_common_string(next_regen)
        };
        msg += &format!(
            "NextGeneratorRegenerationTime: {} ({next_regen_text})\n",
            to_string(next_regen)
        );

        let profiles = &o.wo.world_object_generators.generator_profiles;
        let active = o.generator_active_profiles(utc_now);
        msg += &format!(
            "GeneratorProfiles.Count: {}\n",
            profiles.iter().filter(|g| !g.is_placeholder()).count()
        );
        msg += &format!("GeneratorActiveProfiles.Count: {}\n", active.len());
        msg += &format!("CurrentCreate: {}\n", o.current_create());

        msg += "===============================================\n";
        for active_profile in active {
            let profile = &profiles[usize::try_from(active_profile).expect("a list index")];

            msg += &format!(
                "Active GeneratorProfile id: {active_profile} | LinkId: {}\n",
                profile.link_id()
            );

            let b = &profile.biota;
            msg += &format!(
                "Probability: {} | WCID: {} | Delay: {} | Init: {} | Max: {}\n",
                to_string(b.probability),
                b.weenie_class_id,
                b.delay.map(to_string).unwrap_or_default(),
                b.init_create,
                b.max_create
            );
            msg += &format!(
                "WhenCreate: {} | WhereCreate: {}\n",
                b.when_create.net_string(),
                b.where_create.net_string()
            );
            msg += &format!(
                "StackSize: {} | PaletteId: {} | Shade: {}\n",
                opt_string(b.stack_size),
                opt_string(b.palette_id),
                b.shade.map(to_string).unwrap_or_default()
            );
            msg += &format!(
                "CurrentCreate: {} | Spawned.Count: {} | SpawnQueue.Count: {}\n",
                profile.current_create(),
                profile.spawned.len(),
                profile.spawn_queue.len()
            );
            msg += &format!(
                "GeneratedTreasureItem: {}\n",
                bool_string(profile.generated_treasure_item)
            );
            msg += &format!("IsMaxed: {}\n", bool_string(profile.is_maxed()));
            if !profile.is_maxed() {
                let available = profile.is_available(utc_now);
                let next = if available {
                    String::new()
                } else {
                    format!(
                        ", NextAvailable: {}",
                        to_common_string(profile.next_available)
                    )
                };
                msg += &format!("IsAvailable: {}{next}\n", bool_string(available));
            }
            msg += "--====--\n";
            if !profile.spawned.is_empty() {
                msg += "Spawned Objects:\n";
                for spawn in profile.spawned.values() {
                    msg += &format!(
                        "0x{}: {} - {} - {}\n",
                        spawn.guid,
                        spawn.name.clone().unwrap_or_default(),
                        spawn.weenie_class_id,
                        spawn.weenie_type.net_string()
                    );
                    if let Some(spawn_wo) = spawn.try_get_world_object(w) {
                        let so = obj(w, spawn_wo);
                        if let Some(l) = so.location() {
                            msg += &format!(" LOC: {}\n", l.to_loc_string());
                        } else if so.container_id() == Some(wo.full()) {
                            msg += " Contained by Generator\n";
                        } else if so.wielder_id() == Some(wo.full()) {
                            msg += " Wielded by Generator\n";
                        } else {
                            msg += " Location Unknown\n";
                        }
                    } else {
                        msg += " LOC: Unknown, WorldObject could not be found\n";
                    }
                }
                msg += "--====--\n";
            }

            if !profile.spawn_queue.is_empty() {
                msg += "Pending Spawn Times:\n";
                for spawn in &profile.spawn_queue {
                    msg += &format!("{}\n", to_common_string(*spawn));
                }
                msg += "--====--\n";
            }

            msg += "===============================================\n";
        }
        msg
    } else {
        format!("{} (0x{}) is not a generator.", name_of(w, wo), o.guid)
    };

    system_chat(w, s, &msg, ChatMessageType::System);
}

// ACE: DeveloperCommands.HandlePurchaseHouse
/// `purchase-house`: instantly purchase the house for the last appraised covenant crystal.
pub fn handle_purchase_house(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let s = require(session);
    let slumlord = last_appraised(w, session).filter(|&g| {
        empyrean_world::dispatch::class_of(w, g) == empyrean_world::dispatch::Class::SlumLord
    });

    let Some(slumlord) = slumlord else {
        system_chat(w, s, "Couldn't find slumlord", ChatMessageType::Broadcast);
        return;
    };
    let me = session_player(w, s);
    player_set_house_owner(w, me, slumlord);
    player_give_deed(w, me, slumlord);
}

// ACE: DeveloperCommands.HandleBarrierTest
/// `barrier-test`: shows debug information for house barriers.
pub fn handle_barrier_test(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let me = session_player(w, require(session));
    let location = location_of(w, me);
    let mut cell = location.cell();
    console_write_line(&format!("CurCell: {}", format(cell, "X8")));

    let lb = obj(w, me)
        .current_landblock
        .expect("NullReferenceException: CurrentLandblock");
    let is_dungeon = w
        .landblock_manager
        .landblocks
        .get_mut(lb)
        .expect("NullReferenceException: CurrentLandblock")
        .is_dungeon();
    if is_dungeon {
        console_write_line("Dungeon landblock");

        if !house_manager_apartment_blocks_contains_key(w, location.landblock()) {
            return;
        }
    } else {
        cell = empyrean_world::entity::position_extensions::get_outdoor_cell(&location);
        console_write_line(&format!("OutdoorCell: {}", format(cell, "X8")));
    }

    let barrier = house_cell_house_cells_contains_key(w, cell);
    console_write_line(&format!("Barrier: {}", bool_string(barrier)));
}

// ACE: DeveloperCommands.HandleTargetLoc
/// `targetloc [guid]`: shows the location of the last appraised object (or of a guid).
pub fn handle_target_loc(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let wo = if parameters.is_empty() {
        let Some(wo) = last_appraised(w, session) else {
            return;
        };
        wo
    } else {
        // `parameters[0] = parameters[0].Substring(2)` rewrites the caller's array
        let p0 = if parameters[0].starts_with("0x") {
            substring_from(&parameters[0], 2)
        } else {
            parameters[0].clone()
        };

        let Some(guid) = dotnet_parse::uint_try_parse_hex(&p0) else {
            system_chat(
                w,
                s,
                &format!("{p0} is not a valid guid"),
                ChatMessageType::Broadcast,
            );
            return;
        };

        let me = session_player(w, s);
        let mut wo = current_landblock_get_object(w, me, ObjectGuid::new(guid));

        if wo.is_none() {
            wo = empyrean_world::physics::server_object_manager::get_object_a(w, guid)
                .and_then(|h| empyrean_world::physics::phys_ext::weenie_obj(w, h).world_object(w))
                .filter(|&g| w.objects.get(g).is_some());
        }

        let Some(wo) = wo else {
            system_chat(
                w,
                s,
                &format!("Couldn't find {p0}"),
                ChatMessageType::Broadcast,
            );
            return;
        };
        wo
    };

    let o = obj(w, wo);
    let lb = o
        .current_landblock
        .map(|lb| format(lb.landblock(), "X4"))
        .unwrap_or_default();
    let loc = o.location().map(|l| l.to_loc_string()).unwrap_or_default();
    let phys = o.phys;
    system_chat(
        w,
        s,
        &format!("CurrentLandblock: 0x{lb}"),
        ChatMessageType::Broadcast,
    );
    system_chat(
        w,
        s,
        &format!("Location: {loc}"),
        ChatMessageType::Broadcast,
    );
    let physics = phys
        .map(|h| physics_position_string(w, h))
        .unwrap_or_default();
    system_chat(
        w,
        s,
        &format!("Physics : {physics}"),
        ChatMessageType::Broadcast,
    );
    let cur_cell = phys
        .and_then(|h| empyrean_world::physics::phys_ext::cur_cell(w, h))
        .map(|c| format(c.0, "X8"))
        .unwrap_or_default();
    system_chat(
        w,
        s,
        &format!("CurCell: 0x{cur_cell}"),
        ChatMessageType::Broadcast,
    );
}

// ACE: DeveloperCommands.HandleDamageHistory
/// `damagehistory`: shows your damage history.
pub fn handle_damage_history(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let s = require(session);
    let me = session_player(w, s);
    let text = empyrean_world::entity::damage_history::of(w, me).to_string();
    system_chat(w, s, &text, ChatMessageType::Broadcast);
}

// ACE: DeveloperCommands.HandleRemoveVitae
/// `remove-vitae`: removes vitae from the last appraised player (or yourself).
pub fn handle_remove_vitae(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let s = require(session);
    let me = session_player(w, s);
    let player = last_appraised(w, session)
        .filter(|&g| is_player(w, g))
        .unwrap_or(me);

    empyrean_world::world_objects::managers::enchantment_manager::remove_vitae(w, player);

    if player != me {
        system_chat(
            w,
            s,
            &format!("Removed vitae for {}", name_of(w, player)),
            ChatMessageType::Broadcast,
        );
    }
}

/// `session.Player.CreateEnchantment(session.Player, session.Player, null, new Spell(spellId))`
/// for each spell.
fn self_enchantments(w: &mut World, session: Option<SessionId>, spells: &[SpellId]) {
    let me = session_player(w, require(session));
    for &spell_id in spells {
        let spell = empyrean_world::entity::spell::Spell::from_spell_id(w, spell_id, true);
        empyrean_world::world_objects::world_object_magic::create_enchantment(
            w, me, me, me, None, &spell, false, false, false,
        );
    }
}

// ACE: DeveloperCommands.HandleFast
/// `fast`: Quickness, Sprint and Strength Self VIII on yourself.
pub fn handle_fast(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    self_enchantments(
        w,
        session,
        &[
            SpellId::QuicknessSelf8,
            SpellId::SprintSelf8,
            SpellId::StrengthSelf8,
        ],
    );
}

// ACE: DeveloperCommands.HandleSlow
/// `slow`: Slowness, Leaden Feet and Weakness Self VIII on yourself.
pub fn handle_slow(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    self_enchantments(
        w,
        session,
        &[
            SpellId::SlownessSelf8,
            SpellId::LeadenFeetSelf8,
            SpellId::WeaknessSelf8,
        ],
    );
}

// ACE: DeveloperCommands.HandleRip
/// `rip`: insta-death, without the confirmation dialog from /die.
pub fn handle_rip(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    // insta-death, without the confirmation dialog from /die
    // useful during developer testing
    let me = session_player(w, require(session));
    let health = obj(w, me).health();
    let current = health.current(obj(w, me));
    #[allow(clippy::cast_precision_loss)]
    dispatch::take_damage::take_damage(w, me, me, DamageType::Bludgeon, current as f32, false);
}

// ACE: DeveloperCommands.ResistProperties
/// The resistances `resist-info` shows.
pub const RESIST_PROPERTIES: [PropertyFloat; 8] = [
    PropertyFloat::ResistSlash,
    PropertyFloat::ResistPierce,
    PropertyFloat::ResistBludgeon,
    PropertyFloat::ResistFire,
    PropertyFloat::ResistCold,
    PropertyFloat::ResistAcid,
    PropertyFloat::ResistElectric,
    PropertyFloat::ResistNether,
];

// ACE: DeveloperCommands.HandleResistInfo
/// `resist-info`: shows the resistance info for the last appraised creature.
pub fn handle_resist_info(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let s = require(session);
    let Some(creature) = last_appraised(w, session).filter(|&g| is_creature(w, g)) else {
        system_chat(
            w,
            s,
            "You must appraise a creature to use this command.",
            ChatMessageType::Broadcast,
        );
        return;
    };

    let text = format!("{} ({creature}):", name_of(w, creature));
    system_chat(w, s, &text, ChatMessageType::Broadcast);

    let mut resist_info: Vec<(PropertyFloat, Option<f64>)> = RESIST_PROPERTIES
        .iter()
        .map(|&p| (p, obj(w, creature).get_property(p)))
        .collect();

    // `OrderByDescending(i => i.Value)`: stable, null (the smallest double?) last
    resist_info.sort_by(|a, b| match (b.1, a.1) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    for (key, value) in resist_info {
        let text = format!(
            "{} - {}",
            key.net_string(),
            value.map(to_string).unwrap_or_default()
        );
        system_chat(w, s, &text, ChatMessageType::Broadcast);
    }
}

// ACE: DeveloperCommands.HandleDebugSpell
/// `debugspell [on]`: toggles spell projectile debugging info.
pub fn handle_debug_spell(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let me = session_player(w, s);
    let magic = &mut player_data_mut(w, me).player_magic;
    if parameters.is_empty() {
        magic.debug_spell = !magic.debug_spell;
    } else {
        magic.debug_spell = eq_ignore_case(&parameters[0], "on");
    }
    let text = format!(
        "Spell projectile debugging is {}",
        if player_data(w, me).player_magic.debug_spell {
            "enabled"
        } else {
            "disabled"
        }
    );
    system_chat(w, s, &text, ChatMessageType::Broadcast);
}

// ACE: DeveloperCommands.HandleRecordCast
/// `recordcast [on]`: records spell casting keypresses to the server for debugging.
pub fn handle_record_cast(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let me = session_player(w, s);
    let record_cast = &mut player_data_mut(w, me).player_magic.record_cast;
    if parameters.is_empty() {
        record_cast.enabled = !record_cast.enabled;
    } else {
        record_cast.enabled = eq_ignore_case(&parameters[0], "on");
    }
    let text = format!(
        "Record cast {}",
        if player_data(w, me).player_magic.record_cast.enabled {
            "enabled"
        } else {
            "disabled"
        }
    );
    system_chat(w, s, &text, ChatMessageType::Broadcast);
}

// ACE: DeveloperCommands.HandlePScript
/// `pscript <PlayScript>`: plays a script on the last appraised object.
pub fn handle_p_script(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let Some(wo) = last_appraised(w, session) else {
        return;
    };

    let Some(pscript) = try_parse_u32::<PlayScript>(&parameters[0], true).map(PlayScript) else {
        system_chat(
            w,
            s,
            &format!("Couldn't find PlayScript.{}", parameters[0]),
            ChatMessageType::Broadcast,
        );
        return;
    };
    let msg =
        empyrean_world::network::game_messages::messages::game_message_script::game_message_script(
            wo, pscript, 1.0,
        );
    world_object_networking::enqueue_broadcast(w, wo, true, &[msg]);
}

// ACE: DeveloperCommands.HandleGetInfo
/// `getinfo`: shows basic info for the last appraised object.
pub fn handle_get_info(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let s = require(session);
    if let Some(wo) = last_appraised(w, session) {
        let text = format!(
            "GUID: {wo}\nWeenieClassId: {}\nWeenieClassName: {}",
            obj(w, wo).biota.weenie_class_id,
            weenie_class_name(w, wo)
        );
        system_chat(w, s, &text, ChatMessageType::Broadcast);
    }
}

// ACE: DeveloperCommands.LastTestAim
/// The arrow the last `testaim` spawned.
pub static LAST_TEST_AIM: Mutex<Option<ObjectGuid>> = Mutex::new(None);

/// `float.ToRadians()` (ACE.Server's physics extension): `(float)(Math.PI / 180.0f * angle)`.
fn to_radians(angle: f32) -> f32 {
    CsCast::<f32>::cs_cast(std::f64::consts::PI / f64::from(180.0f32) * f64::from(angle))
}

// ACE: DeveloperCommands.HandleTestAim
/// `testaim <Aim motion>`: tests the aim high/low motions, and the projectile spawn position.
pub fn handle_test_aim(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let motion_str = &parameters[0];

    let aim = motion_str
        .get(..3)
        .is_some_and(|p| p.eq_ignore_ascii_case("Aim"));
    if !aim {
        system_chat(
            w,
            s,
            "Motion must start with Aim!",
            ChatMessageType::Broadcast,
        );
        return;
    }

    let Some(motion_command) = try_parse_u32::<MotionCommand>(motion_str, true).map(MotionCommand)
    else {
        system_chat(
            w,
            s,
            &format!("Couldn't find MotionCommand {motion_str}"),
            ChatMessageType::Broadcast,
        );
        return;
    };

    let _positive = motion_command.0 >= MotionCommand::AimHigh15.0
        && motion_command.0 <= MotionCommand::AimHigh90.0;

    let last = *LAST_TEST_AIM
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(last) = last {
        world_object::destroy(w, last, true, false);
    }

    let me = session_player(w, s);
    let motion = Motion::from_world_object(w, me, motion_command, 1.0);

    world_object_networking::enqueue_broadcast_motion(w, me, &motion, None, None);

    // spawn ethereal arrow w/ no velocity or gravity
    let local_origin = empyrean_world::world_objects::creature_missile::get_projectile_spawn_origin(
        w,
        me,
        300,
        motion_command,
    );

    let location = location_of(w, me);
    let global_origin = location.pos() + Vector3::transform(local_origin, location.rotation());

    let wo = create_new_world_object_by_wcid(w, 300).expect("NullReferenceException: wo");
    obj_mut(w, wo).set_ethereal(Some(true));
    obj_mut(w, wo).set_gravity_status(Some(false));

    let angle = to_radians(motion_command.get_aim_angle());
    let z_rotation = Quaternion::create_from_axis_angle(Vector3::new(1.0, 0.0, 0.0), angle);

    let mut wo_location = Position::from_position(&location);
    wo_location.set_pos(global_origin);
    wo_location.set_rotation(wo_location.rotation() * z_rotation);
    obj_mut(w, wo).set_location(Some(wo_location));

    let lb = obj(w, me)
        .current_landblock
        .expect("NullReferenceException: CurrentLandblock");
    landblock::add_world_object(w, lb, wo);

    *LAST_TEST_AIM
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(wo);
}
// ACE: DeveloperCommands.HandleReloadLandblocks
/// `reload-landblock`: reloads the current landblock.
pub fn handle_reload_landblocks(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    use empyrean_world::entity::actions::action_chain::ActionChain;
    use empyrean_world::entity::actions::i_actor::Actor;

    let s = require(session);
    let me = session_player(w, s);
    let lb = obj(w, me)
        .current_landblock
        .expect("NullReferenceException: CurrentLandblock");

    let landblock_id = lb.raw() | 0xFFFF;

    system_chat(
        w,
        s,
        &format!("Reloading 0x{}", format(landblock_id, "X8")),
        ChatMessageType::Broadcast,
    );

    // destroy all non-player server objects
    landblock::destroy_all_non_player_objects(w, lb);

    // clear landblock cache
    w.content
        .clear_cached_instances_by_landblock(lb.landblock());

    // reload landblock
    let mut action_chain = ActionChain::new();
    action_chain.add_delay_for_one_tick(w);
    action_chain.add_action(Actor::Object(me), move |w| {
        landblock::init(w, lb, true);
    });
    action_chain.enqueue_chain(w);
}

// ACE: DeveloperCommands.HandleShowVelocity
/// `showvelocity`: shows the velocity of the last appraised object.
pub fn handle_show_velocity(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    use empyrean_world::physics::phys_ext;

    let s = require(session);
    let Some(o) = last_appraised(w, session) else {
        return;
    };
    let Some(h) = obj(w, o).phys else { return };

    let velocity = phys_ext::velocity(w, h);
    system_chat(
        w,
        s,
        &format!("Velocity: {}", vec3_string(velocity)),
        ChatMessageType::Broadcast,
    );
    system_chat(
        w,
        s,
        &format!("Physics.Velocity: {}", vec3_string(velocity)),
        ChatMessageType::Broadcast,
    );
    let cached = w
        .physics
        .get(h)
        .map(|p| empyrean_entity::shared_types::vector3_of_data(p.cached_velocity))
        .unwrap_or_default();
    system_chat(
        w,
        s,
        &format!("CachedVelocity: {}", vec3_string(cached)),
        ChatMessageType::Broadcast,
    );
}

// ACE: DeveloperCommands.HandleBumpVelocity
/// `bumpvelocity`: bumps the velocity of the last appraised object.
pub fn handle_bump_velocity(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let s = require(session);
    let Some(o) = last_appraised(w, session) else {
        return;
    };
    let Some(h) = obj(w, o).phys else { return };

    let velocity = Vector3::new(0.0, 0.0, 0.5);

    empyrean_world::physics::phys_ext::set_velocity_field(w, h, velocity);

    let msg = empyrean_world::network::game_messages::messages::game_message_vector_update::game_message_vector_update(w, o);
    enqueue_send(w, s, msg);
}

// ACE: DeveloperCommands.HandleCheckEthereal
/// `check-collision`: checks if the player is currently colliding with any other objects.
pub fn handle_check_ethereal(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let s = require(session);
    let me = session_player(w, s);
    let colliding = physics_obj_ethereal_check_for_collisions(w, me);

    system_chat(
        w,
        s,
        &format!("IsColliding: {}", bool_string(colliding)),
        ChatMessageType::Broadcast,
    );
}

/// `session.Player.Society`: `Faction1Bits ?? FactionBits.None`.
fn society(w: &World, player: ObjectGuid) -> FactionBits {
    obj(w, player).faction1_bits().unwrap_or(FactionBits::None)
}

/// The society rank a `faction` rank parameter sets, and its title.
fn faction_rank(rank: i32) -> Option<(i32, &'static str)> {
    match rank {
        1 => Some((1, "Initiate")),
        2 => Some((101, "Adept")),
        3 => Some((301, "Knight")),
        4 => Some((601, "Lord")),
        5 => Some((1001, "Master")),
        _ => None,
    }
}

// ACE: DeveloperCommands.HandleFaction
/// `faction < none / ch / ew / rb > (rank)`: sets your own faction state.
#[allow(clippy::too_many_lines)]
pub fn handle_faction(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    use empyrean_world::managers::quest_manager::{self, QuestOwner};
    use empyrean_world::network::game_messages::messages::game_message_private_update_property_int::game_message_private_update_property_int;

    let me = session_player(w, require(session));
    let mut rank_str = "Initiate";
    if parameters.is_empty() {
        let message = format!(
            "Your current Faction state is: {}\nYou can change it to the following:\nNONE      = No Faction\nCH        = Celestial Hand\nEW        = Eldrytch Web\nRB        = Radiant Blood\nOptionally you can also include a rank, otherwise rank will be set to Initiate\n1 = Initiate | 2 = Adept | 3 = Knight | 4 = Lord | 5 = Master",
            society(w, me).to_sentence()
        );
        write_output_info(w, session, &message, ChatMessageType::Broadcast);
        return;
    }

    let owner = QuestOwner::Creature(me);
    let mut owner = owner;
    // the rank parameter: `parameters.Length == 2 && int.TryParse(parameters[1], out var rank)`
    let rank = if parameters.len() == 2 {
        dotnet_parse::int_try_parse(&parameters[1])
    } else {
        None
    };
    let set_ranks = |w: &mut World, ch: Option<i32>, ew: Option<i32>, rb: Option<i32>| {
        let o = obj_mut(w, me);
        o.set_society_rank_celhan(ch);
        o.set_society_rank_eldweb(ew);
        o.set_society_rank_radblo(rb);
    };
    let ch_ew = FactionBits(FactionBits::EldrytchWeb.0 | FactionBits::RadiantBlood.0);
    let ew_rb = FactionBits(FactionBits::CelestialHand.0 | FactionBits::RadiantBlood.0);
    let ch_rb = FactionBits(FactionBits::CelestialHand.0 | FactionBits::EldrytchWeb.0);
    match parameters[0].to_lowercase().as_str() {
        "none" => {
            obj_mut(w, me).set_faction1_bits(None);
            set_ranks(w, None, None, None);
            quest_manager::erase(w, &mut owner, "SocietyMember");
            quest_manager::erase(w, &mut owner, "SocietyFlag");
            quest_manager::erase(w, &mut owner, "CelestialHandMember");
            quest_manager::erase(w, &mut owner, "EldrytchWebMember");
            quest_manager::erase(w, &mut owner, "RadiantBloodMember");
        }
        "ch" => {
            obj_mut(w, me).set_faction1_bits(Some(FactionBits::CelestialHand));
            set_ranks(w, Some(1), None, None);
            quest_manager::set_quest_bits(
                w,
                &mut owner,
                "SocietyMember",
                FactionBits::CelestialHand.0,
                true,
            );
            quest_manager::set_quest_bits(
                w,
                &mut owner,
                "SocietyFlag",
                FactionBits::CelestialHand.0,
                true,
            );
            quest_manager::set_quest_bits(w, &mut owner, "SocietyMember", ch_ew.0, false);
            quest_manager::set_quest_bits(w, &mut owner, "SocietyFlag", ch_ew.0, false);
            quest_manager::stamp(w, &mut owner, "CelestialHandMember");
            quest_manager::erase(w, &mut owner, "EldrytchWebMember");
            quest_manager::erase(w, &mut owner, "RadiantBloodMember");
            if let Some((value, title)) = rank.and_then(faction_rank) {
                obj_mut(w, me).set_society_rank_celhan(Some(value));
                rank_str = title;
            }
        }
        "ew" => {
            obj_mut(w, me).set_faction1_bits(Some(FactionBits::EldrytchWeb));
            set_ranks(w, None, Some(1), None);
            quest_manager::set_quest_bits(
                w,
                &mut owner,
                "SocietyMember",
                FactionBits::EldrytchWeb.0,
                true,
            );
            quest_manager::set_quest_bits(
                w,
                &mut owner,
                "SocietyFlag",
                FactionBits::EldrytchWeb.0,
                true,
            );
            quest_manager::set_quest_bits(w, &mut owner, "SocietyMember", ew_rb.0, false);
            quest_manager::set_quest_bits(w, &mut owner, "SocietyFlag", ew_rb.0, false);
            quest_manager::erase(w, &mut owner, "CelestialHandMember");
            quest_manager::stamp(w, &mut owner, "EldrytchWebMember");
            quest_manager::erase(w, &mut owner, "RadiantBloodMember");
            if let Some((value, title)) = rank.and_then(faction_rank) {
                obj_mut(w, me).set_society_rank_eldweb(Some(value));
                rank_str = title;
            }
        }
        "rb" => {
            obj_mut(w, me).set_faction1_bits(Some(FactionBits::RadiantBlood));
            set_ranks(w, None, None, Some(1));
            quest_manager::set_quest_bits(
                w,
                &mut owner,
                "SocietyMember",
                FactionBits::RadiantBlood.0,
                true,
            );
            quest_manager::set_quest_bits(
                w,
                &mut owner,
                "SocietyFlag",
                FactionBits::RadiantBlood.0,
                true,
            );
            quest_manager::set_quest_bits(w, &mut owner, "SocietyMember", ch_rb.0, false);
            quest_manager::set_quest_bits(w, &mut owner, "SocietyFlag", ch_rb.0, false);
            quest_manager::erase(w, &mut owner, "CelestialHandMember");
            quest_manager::erase(w, &mut owner, "EldrytchWebMember");
            quest_manager::stamp(w, &mut owner, "RadiantBloodMember");
            if let Some((value, title)) = rank.and_then(faction_rank) {
                obj_mut(w, me).set_society_rank_radblo(Some(value));
                rank_str = title;
            }
        }
        _ => {}
    }
    // Not ACE's (a fix): these are private (self-only) property updates,
    // so they go to this player alone; ACE broadcast them to every player who knew this one, whose
    // clients applied them to themselves.
    let o = obj(w, me);
    let values = [
        (
            PropertyInt::Faction1Bits,
            o.faction1_bits().map_or(0, |f| f.0),
        ),
        (
            PropertyInt::SocietyRankCelhan,
            o.society_rank_celhan().unwrap_or(0),
        ),
        (
            PropertyInt::SocietyRankEldweb,
            o.society_rank_eldweb().unwrap_or(0),
        ),
        (
            PropertyInt::SocietyRankRadblo,
            o.society_rank_radblo().unwrap_or(0),
        ),
    ];
    for (prop, value) in values {
        let msg = game_message_private_update_property_int(obj_mut(w, me), prop, value);
        enqueue_send(w, require(session), msg);
    }
    empyrean_world::world_objects::player_networking::send_turbine_chat_channels(w, me, false);
    let society = society(w, me);
    let rank_text = if society == FactionBits::None {
        String::new()
    } else {
        format!(" with a rank of {rank_str}")
    };
    write_output_info(
        w,
        session,
        &format!(
            "Your current Faction state is now set to: {}{rank_text}",
            society.to_sentence()
        ),
        ChatMessageType::Broadcast,
    );

    let text = format!(
        "{} changed their Faction state to {}{rank_text}.",
        name_of(w, me),
        society.to_sentence()
    );
    player_manager::broadcast_to_audit_channel(w, Some(me), &text);
}

// ACE: DeveloperCommands.HandleShowTier
/// `showtier`: shows the DeathTreasure tier for the last appraised monster.
pub fn handle_show_tier(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let Some(creature) = last_appraised(w, session).filter(|&g| is_creature(w, g)) else {
        return;
    };

    let msg = match empyrean_world::world_objects::creature_death::death_treasure(w, creature) {
        Some(dt) => format!("DeathTreasure - Tier: {}", dt.tier),
        None => "doesn't have PropertyDataId.DeathTreasureType".to_owned(),
    };

    write_output_info(
        w,
        session,
        &format!("{} ({creature}) {msg}", name_of(w, creature)),
        ChatMessageType::Broadcast,
    );
}

// ACE: DeveloperCommands.HandleTierMobs
/// `tiermobs <tier>`: shows a list of monsters for a particular tier.
///
/// DIVERGE (forced): as `teledungeon`, the rows come in the world content's order.
pub fn handle_tier_mobs(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let Some(tier) = dotnet_parse::uint_try_parse(&parameters[0]) else {
        write_output_info(
            w,
            session,
            &format!("Invalid tier {}", parameters[0]),
            ChatMessageType::Broadcast,
        );
        return;
    };
    if !(1..=8).contains(&tier) {
        write_output_info(
            w,
            session,
            "Please enter a tier between 1-8",
            ChatMessageType::Broadcast,
        );
        return;
    }
    let treasure_deaths: Vec<empyrean_content::models::world::TreasureDeath> = w
        .content
        .get_all_treasure_death()
        .values()
        .cloned()
        .collect();
    let mut results = Vec::new();
    for weenie in w.content.get_all_weenies() {
        if weenie.r#type != WeenieType::Creature.0.cast_signed() {
            continue;
        }
        for death_treasure in weenie
            .weenie_properties_did
            .iter()
            .filter(|d| d.r#type == PropertyDataId::DeathTreasureType.0)
        {
            for treasure_death in &treasure_deaths {
                if death_treasure.value == treasure_death.treasure_type
                    && i64::from(treasure_death.tier) == i64::from(tier)
                {
                    results.push(weenie.class_name.clone());
                }
            }
        }
    }

    write_output_info(
        w,
        session,
        &format!("Found {} monsters for tier {tier}", results.len()),
        ChatMessageType::Broadcast,
    );

    for result in results {
        write_output_info(w, session, &result, ChatMessageType::Broadcast);
    }
}

// ACE: DeveloperCommands.HandleDelevel
/// `delevel <new level>`: attempts to delevel the current player. Requires enough unassigned xp
/// and unspent skill credits. The confirmation's callback is the `confirmed` overload.
pub fn handle_delevel(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    handle_delevel_confirmed(w, session, false, parameters);
}

/// `long?`/`int?` with `:N0`: null is "".
fn opt_n0<T: Into<empyrean_common::dotnet::format::Num>>(v: Option<T>) -> String {
    v.map(|v| format(v, "N0")).unwrap_or_default()
}

// ACE: DeveloperCommands.HandleDelevel
/// `HandleDelevel(session, confirmed, parameters)`.
pub fn handle_delevel_confirmed(
    w: &mut World,
    session: Option<SessionId>,
    confirmed: bool,
    parameters: &[String],
) {
    let s = require(session);
    let me = session_player(w, s);
    let Some(delevel) = dotnet_parse::int_try_parse(&parameters[0]) else {
        system_chat(
            w,
            s,
            &format!("Invalid level {}", parameters[0]),
            ChatMessageType::Broadcast,
        );
        return;
    };
    if delevel < 1 || i64::from(delevel) > i64::from(player_xp::get_max_level(w)) {
        system_chat(
            w,
            s,
            &format!("Invalid level {delevel}"),
            ChatMessageType::Broadcast,
        );
        return;
    }
    let level = obj(w, me).level();
    if level.is_some_and(|l| delevel > l) {
        system_chat(
            w,
            s,
            &format!(
                "Delevel # must be less than current level {}",
                opt_string(level)
            ),
            ChatMessageType::Broadcast,
        );
        return;
    }

    // get amount of unassigned xp required
    let current_level = level.expect("InvalidOperationException: Level.Value");
    let xp_between_levels: i64 =
        player_xp::get_xp_between_levels(w, delevel, current_level).cs_cast();
    let level_xp = w.dats.portal_dat().xp_table().level_xp
        [usize::try_from(current_level).expect("IndexOutOfRangeException")];
    let total_experience = obj(w, me).total_experience();
    // `TotalExperience - (long)..`: a null TotalExperience stays null
    let xp_into_current_level = total_experience.map(|t| t.wrapping_sub(level_xp.cs_cast()));
    let unassigned_xp_required = xp_into_current_level.map(|x| xp_between_levels.wrapping_add(x));

    system_chat(
        w,
        s,
        &format!("Unassigned XP required: {}", opt_n0(unassigned_xp_required)),
        ChatMessageType::Broadcast,
    );

    let available_experience = obj(w, me).available_experience();
    if let (Some(a), Some(u)) = (available_experience, unassigned_xp_required) {
        if a < u {
            system_chat(
                w,
                s,
                &format!(
                    "You only have {} unassigned XP -- delevel failed",
                    format(a, "N0")
                ),
                ChatMessageType::Broadcast,
            );
            return;
        }
    }

    // get # of available skill credits required
    let mut skill_credits_required: i32 = 0;
    for i in (delevel + 1)..=current_level {
        let credits = w.dats.portal_dat().xp_table().level_credits
            [usize::try_from(i).expect("IndexOutOfRangeException")];
        skill_credits_required = skill_credits_required.wrapping_add(credits.cast_signed());
    }

    system_chat(
        w,
        s,
        &format!(
            "Skill credits required: {}",
            format(skill_credits_required, "N0")
        ),
        ChatMessageType::Broadcast,
    );

    let available_skill_credits = obj(w, me).available_skill_credits();
    if available_skill_credits.is_some_and(|a| a < skill_credits_required) {
        let text = format!(
            "You only have {} available skill credits -- delevel failed",
            opt_n0(available_skill_credits)
        );
        system_chat(w, s, &text, ChatMessageType::Broadcast);
        return;
    }

    if !confirmed {
        let msg = format!(
            "Are you sure you want to delevel {} to level {delevel}?",
            name_of(w, me)
        );
        if !confirmation_manager_enqueue_send_delevel(w, me, s, parameters, &msg) {
            empyrean_world::world_objects::player_networking::send_weenie_error(
                w,
                me,
                empyrean_entity::enums::WeenieError::ConfirmationInProgress,
            );
        }
        return;
    }

    system_chat(
        w,
        s,
        &format!("Deleveling {} to level {delevel}", name_of(w, me)),
        ChatMessageType::Broadcast,
    );

    let sub = |a: Option<i64>, b: Option<i64>| a.zip(b).map(|(a, b)| a.wrapping_sub(b));
    let new_available_experience = sub(available_experience, unassigned_xp_required);
    let new_total_experience = sub(total_experience, unassigned_xp_required);

    let new_available_skill_credits =
        available_skill_credits.map(|a| a.wrapping_sub(skill_credits_required));
    let new_total_skill_credits = obj(w, me)
        .total_skill_credits()
        .map(|t| t.wrapping_sub(skill_credits_required));

    player_update_property_int64(
        w,
        me,
        me,
        PropertyInt64::AvailableExperience,
        new_available_experience,
        false,
    );
    player_update_property_int64(
        w,
        me,
        me,
        PropertyInt64::TotalExperience,
        new_total_experience,
        false,
    );

    player_update_property_int(
        w,
        me,
        me,
        PropertyInt::AvailableSkillCredits,
        new_available_skill_credits,
        false,
    );
    player_update_property_int(
        w,
        me,
        me,
        PropertyInt::TotalSkillCredits,
        new_total_skill_credits,
        false,
    );

    player_update_property_int(w, me, me, PropertyInt::Level, Some(delevel), false);

    let text = format!(
        "{} has deleveled themselves from {current_level} to {} - unassignedXPRequired: {} | skillCreditsRequired: {}",
        name_of(w, me),
        opt_string(obj(w, me).level()),
        opt_n0(unassigned_xp_required),
        format(skill_credits_required, "N0")
    );
    player_manager::broadcast_to_audit_channel(w, Some(me), &text);
}

// ACE: DeveloperCommands.HandleMonsterProj
// Not ACE's (a fix): "Invalid SpellId" names the text typed; ACE
// printed the failed parse's result, which is always Undef.
/// `monsterspell <SpellId> (target guid)`: the last appraised creature casts a spell; targeted
/// spells default to the current player.
pub fn handle_monster_proj(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let parsed = admin_commands::try_parse_spell_id(&parameters[0], false);
    let Some(spell_id) = parsed else {
        write_output_info(
            w,
            session,
            &format!("Invalid SpellId {}", parameters[0]),
            ChatMessageType::Broadcast,
        );
        return;
    };
    let spell = empyrean_world::entity::spell::Spell::from_spell_id(w, spell_id, true);
    if spell.not_found() {
        write_output_info(
            w,
            session,
            &format!("Couldn't find SpellId {}", spell_id.net_string()),
            ChatMessageType::Broadcast,
        );
        return;
    }

    let me = session_player(w, require(session));
    let mut attack_target = Some(me);

    if parameters.len() > 1 {
        let Some(target_guid) = dotnet_parse::uint_try_parse_hex(&parameters[1]) else {
            write_output_info(
                w,
                session,
                &format!("Invalid target guid: {}", parameters[1]),
                ChatMessageType::Broadcast,
            );
            return;
        };

        attack_target = player_inventory::find_object(
            w,
            me,
            ObjectGuid::new(target_guid),
            SearchLocations::Landblock,
        )
        .result
        .filter(|&g| is_creature(w, g));

        if attack_target.is_none() {
            write_output_info(
                w,
                session,
                &format!("Couldn't find attack target {}", format(target_guid, "X8")),
                ChatMessageType::Broadcast,
            );
            return;
        }
    }
    let Some(monster) = last_appraised(w, session).filter(|&g| is_creature(w, g)) else {
        return;
    };

    let prev_attack_target =
        empyrean_world::world_objects::monster_combat::attack_target(w, monster);
    empyrean_world::world_objects::monster_combat::set_attack_target(w, monster, attack_target);

    empyrean_world::world_objects::monster_magic::cast_spell(w, monster, &spell);

    empyrean_world::world_objects::monster_combat::set_attack_target(
        w,
        monster,
        prev_attack_target,
    );
}

// ACE: DeveloperCommands.HandleDebugSpellbook
/// `debugspellbook`: shows the spellbook for the last appraised object.
pub fn handle_debug_spellbook(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let Some(creature) = last_appraised(w, session).filter(|&g| is_creature(w, g)) else {
        return;
    };
    let Some(book) = obj(w, creature).biota.properties_spell_book.as_ref() else {
        return;
    };

    let lines: Vec<String> = book
        .iter()
        .map(|(&k, &v)| {
            format!(
                "{} - {}",
                SpellId(k.cast_unsigned()).net_string(),
                to_string(v)
            )
        })
        .collect();

    write_output_info(w, session, &lines.join("\n"), ChatMessageType::Broadcast);
}

// ACE: DeveloperCommands.HandleTryWield
/// `trywield <item guid> <EquipMask>`: tries to wield an item from your inventory.
pub fn handle_try_wield(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let Some(item_guid) = dotnet_parse::uint_try_parse_hex(&parameters[0]) else {
        write_output_info(
            w,
            session,
            &format!("Invalid item guid {}", parameters[0]),
            ChatMessageType::Broadcast,
        );
        return;
    };

    let me = session_player(w, require(session));
    let item = player_inventory::find_object(
        w,
        me,
        ObjectGuid::new(item_guid),
        SearchLocations::MyInventory,
    )
    .result;

    if item.is_none() {
        write_output_info(
            w,
            session,
            &format!("Couldn't find item guid {}", parameters[0]),
            ChatMessageType::Broadcast,
        );
        return;
    }

    let Some(equip_mask) = try_parse_u32::<EquipMask>(&parameters[1], false).map(EquipMask) else {
        write_output_info(
            w,
            session,
            &format!("Invalid EquipMask {}", parameters[1]),
            ChatMessageType::Broadcast,
        );
        return;
    };

    player_inventory::handle_action_get_and_wield_item(w, me, item_guid, equip_mask);
}

use empyrean_world::entity::treasure_wielded_set::TreasureWieldedSet as WieldedTreasureSet;

// ACE: DeveloperCommands.HandleShowWieldedTreasure
/// `show-wielded-treasure <wcid>`: shows the WieldedTreasure table for a creature.
pub fn handle_show_wielded_treasure(
    w: &mut World,
    session: Option<SessionId>,
    parameters: &[String],
) {
    let Some(wcid) = dotnet_parse::uint_try_parse(&parameters[0]) else {
        write_output_info(
            w,
            session,
            &format!("Invalid wcid {}", parameters[0]),
            ChatMessageType::Broadcast,
        );
        return;
    };
    let created = create_new_world_object_by_wcid(w, wcid);
    let creature = created.filter(|&g| is_creature(w, g));

    let Some(creature) = creature else {
        if let Some(g) = created {
            drop_unplaced(w, g);
        }
        write_output_info(
            w,
            session,
            &format!("Couldn't find weenie {wcid}"),
            ChatMessageType::Broadcast,
        );
        return;
    };

    let wielded_treasure =
        empyrean_world::world_objects::creature_equipment::wielded_treasure(w, creature);
    let Some(wielded_treasure) = wielded_treasure else {
        let text = format!(
            "{} ({}) missing WieldedTreasure",
            name_of(w, creature),
            obj(w, creature).biota.weenie_class_id
        );
        drop_unplaced(w, creature);
        write_output_info(w, session, &text, ChatMessageType::Broadcast);
        return;
    };
    // (ACE leaves the creature to the garbage collector)
    drop_unplaced(w, creature);

    let table = treasure_wielded_table_new(w, &wielded_treasure);

    for set in &table {
        output_wielded_treasure_set(w, session, set, 0);
    }
}

// ACE: DeveloperCommands.OutputWieldedTreasureSet
fn output_wielded_treasure_set(
    w: &mut World,
    session: Option<SessionId>,
    set: &WieldedTreasureSet,
    depth: usize,
) {
    let prefix = " ".repeat(depth * 2);

    let mut total_probability = 0.0f32;
    let mut spacer = false;

    for item in &set.items {
        if total_probability >= 1.0 {
            total_probability = 0.0;
            //spacer = true;
        }
        total_probability += item.item.probability;

        let wo = create_new_world_object_by_wcid(w, item.item.weenie_class_id);

        let item_name = wo.map_or_else(|| "Unknown".to_owned(), |g| name_of(w, g));
        if let Some(g) = wo {
            drop_unplaced(w, g);
        }

        if spacer {
            write_output_info(w, session, "", ChatMessageType::Broadcast);
            spacer = false;
        }
        let text = format!(
            "{prefix}- {} - {item_name} ({}%)",
            item.item.weenie_class_id,
            to_string(item.item.probability * 100.0)
        );
        write_output_info(w, session, &text, ChatMessageType::Broadcast);

        if let Some(subset) = &item.subset {
            output_wielded_treasure_set(w, session, subset, depth + 1);
            //spacer = true;
        }
    }
}

/// `Enum.TryParse` over ACE.Server's small `int` enums declared in `Entity/Aetheria.cs`
/// (`AetheriaColor`, `Sigil`, `Surge`): the enum's members are `0..names.len()`.
macro_rules! aetheria_enum {
    ($ty:ident, $name:literal, [$($member:literal),*]) => {
        /// ACE.Server's `$name` (`Entity/Aetheria.cs`), for `Enum.TryParse`.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct $ty(pub i32);

        impl AceEnum for $ty {
            const TYPE_NAME: &'static str = $name;
            const IS_FLAGS: bool = false;
            const MEMBERS: &'static [Self] = &aetheria_enum!(@members $ty, 0, [$($member),*]);
            const MEMBER_NAMES: &'static [&'static str] = &[$($member),*];

            fn key(self) -> u64 {
                i64::from(self.0).cast_unsigned()
            }

            fn numeric_string(self) -> String {
                self.0.to_string()
            }

            fn name_index(name: &str) -> Option<usize> {
                Self::MEMBER_NAMES.iter().position(|n| *n == name)
            }
        }
    };
    (@members $ty:ident, $i:expr, []) => { [] };
    (@members $ty:ident, $i:expr, [$a:literal]) => { [$ty($i)] };
    (@members $ty:ident, $i:expr, [$a:literal, $b:literal]) => { [$ty($i), $ty($i + 1)] };
    (@members $ty:ident, $i:expr, [$a:literal, $b:literal, $c:literal]) => { [$ty($i), $ty($i + 1), $ty($i + 2)] };
    (@members $ty:ident, $i:expr, [$a:literal, $b:literal, $c:literal, $d:literal, $e:literal]) => {
        [$ty($i), $ty($i + 1), $ty($i + 2), $ty($i + 3), $ty($i + 4)]
    };
}

aetheria_enum!(
    AetheriaColorEnum,
    "AetheriaColor",
    ["Blue", "Yellow", "Red"]
);
aetheria_enum!(
    SigilEnum,
    "Sigil",
    ["Defense", "Destruction", "Fury", "Growth", "Vigor"]
);
aetheria_enum!(
    SurgeEnum,
    "Surge",
    [
        "Destruction",
        "Protection",
        "Regeneration",
        "Affliction",
        "Festering"
    ]
);

// ACE: DeveloperCommands.AetheriaWcids
/// `AetheriaWcids[color]`: the coalesced aetheria weenie of each colour.
fn aetheria_wcids(color: i32) -> Option<u32> {
    match color {
        0 => Some(empyrean_world::entity::aetheria::AETHERIA_BLUE),
        1 => Some(empyrean_world::entity::aetheria::AETHERIA_YELLOW),
        2 => Some(empyrean_world::entity::aetheria::AETHERIA_RED),
        _ => None,
    }
}

// ACE: DeveloperCommands.SurgeSpells
/// `SurgeSpells[surge]`: the proc spell of each surge.
fn surge_spells(surge: i32) -> Option<SpellId> {
    match surge {
        0 => Some(SpellId::AetheriaProcDamageBoost),
        1 => Some(SpellId::AetheriaProcDamageReduction),
        2 => Some(SpellId::AetheriaProcHealthOverTime),
        3 => Some(SpellId::AetheriaProcDamageOverTime),
        4 => Some(SpellId::AetheriaProcHealDebuff),
        _ => None,
    }
}

// ACE: DeveloperCommands.HandleCIAetheria
/// `ciaetheria [color] [set] [surge] [level]`: spawns an Aetheria in the player's inventory.
pub fn handle_ci_aetheria(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let Some(color) = try_parse_i32::<AetheriaColorEnum>(&parameters[0], true) else {
        write_output_info(
            w,
            session,
            &format!("Invalid color: {}", parameters[0]),
            ChatMessageType::Broadcast,
        );
        write_output_info(
            w,
            session,
            "Available colors: Blue, Yellow, Red",
            ChatMessageType::Broadcast,
        );
        return;
    };

    let Some(set) = try_parse_i32::<SigilEnum>(&parameters[1], true) else {
        write_output_info(
            w,
            session,
            &format!("Invalid set: {}", parameters[1]),
            ChatMessageType::Broadcast,
        );
        write_output_info(
            w,
            session,
            "Available sets: Defense, Destruction, Fury, Growth, Vigor",
            ChatMessageType::Broadcast,
        );
        return;
    };

    let Some(surge_spell) = try_parse_i32::<SurgeEnum>(&parameters[2], true) else {
        write_output_info(
            w,
            session,
            &format!("Invalid surge spell: {}", parameters[2]),
            ChatMessageType::Broadcast,
        );
        write_output_info(
            w,
            session,
            "Available surge spells: Destruction, Protection, Regeneration, Affliction, Festering",
            ChatMessageType::Broadcast,
        );
        return;
    };

    let max_level = dotnet_parse::int_try_parse(&parameters[3]).filter(|l| (1..=5).contains(l));
    let Some(max_level) = max_level else {
        write_output_info(
            w,
            session,
            &format!("Invalid level: {}", parameters[3]),
            ChatMessageType::Broadcast,
        );
        write_output_info(
            w,
            session,
            "Available levels: 1 - 5",
            ChatMessageType::Broadcast,
        );
        return;
    };

    let wcid = aetheria_wcids(color)
        .unwrap_or_else(|| panic!("KeyNotFoundException: AetheriaWcids[{color}]"));

    let created = create_new_world_object_by_wcid(w, wcid);
    let wo = created.filter(|&g| {
        empyrean_world::dispatch::class_of(w, g) == empyrean_world::dispatch::Class::Gem
    });

    let Some(wo) = wo else {
        if let Some(g) = created {
            drop_unplaced(w, g);
        }
        write_output_info(
            w,
            session,
            "Failed to create Aetheria wcid",
            ChatMessageType::Broadcast,
        );
        return;
    };

    obj_mut(w, wo).set_property(PropertyInt::ItemMaxLevel, max_level);
    let icon_overlay =
        empyrean_world::factories::loot_generation_factory_aetheria::ICON_OVERLAY_ITEM_MAX_LEVEL
            [usize::try_from(max_level - 1).expect("IndexOutOfRangeException")];
    obj_mut(w, wo).set_property(PropertyDataId::IconOverlay, icon_overlay);

    let equipment_set = aetheria_sigil_to_equipment_set(w, set);
    obj_mut(w, wo).set_property(PropertyInt::EquipmentSetId, equipment_set);

    let icon = aetheria_icons(w, color, set);
    obj_mut(w, wo).set_property(PropertyDataId::Icon, icon);

    let proc_spell = surge_spells(surge_spell)
        .unwrap_or_else(|| panic!("KeyNotFoundException: SurgeSpells[{surge_spell}]"));

    obj_mut(w, wo).set_property(PropertyDataId::ProcSpell, proc_spell.0);

    if aetheria_surge_target_self(w, proc_spell) {
        obj_mut(w, wo).set_property(PropertyBool::ProcSpellSelfTargeted, true);
    }

    let valid_locations = aetheria_color_to_mask(color);
    obj_mut(w, wo).set_property(PropertyInt::ValidLocations, valid_locations.0.cast_signed());

    let o = obj(w, wo);
    let base_xp: u64 = o
        .get_property(PropertyInt64::ItemBaseXp)
        .unwrap_or_default()
        .cs_cast();
    let xp_style = o
        .get_property(PropertyInt::ItemXpStyle)
        .map(empyrean_entity::enums::ItemXpStyle)
        .expect("InvalidOperationException: ItemXpStyle.Value");
    let total: i64 = empyrean_world::entity::experience_system::item_level_to_total_xp(
        max_level, base_xp, max_level, xp_style,
    )
    .cs_cast();
    obj_mut(w, wo).set_property(PropertyInt64::ItemTotalXp, total);

    let me = session_player(w, require(session));
    if !try_create_in_inventory(w, me, wo) {
        write_output_info(
            w,
            session,
            "Failed to add Aetheria item to player inventory",
            ChatMessageType::Broadcast,
        );
    }
}

/// `weenie.GetName()`/`GetPluralName()` by count: the currency line's unit.
///
/// Not ACE's (a fix): with the currency weenie missing the unit is
/// `"WCID <n>"`, so the dump is still sent; ACE's summary reported the missing weenie and then
/// dereferenced it here, throwing instead of sending the dump.
fn currency_unit(
    currency_weenie: Option<&empyrean_entity::Weenie>,
    currency_wcid: u32,
    amount: i64,
) -> String {
    let Some(weenie) = currency_weenie else {
        return format!("WCID {currency_wcid}");
    };
    if amount == 1 {
        weenie.get_name().unwrap_or_default()
    } else {
        weenie.get_plural_name()
    }
}

/// `{value}` of a nullable, or `NULL`.
fn or_null<T: ToString>(v: Option<T>) -> String {
    v.map_or_else(|| "NULL".to_owned(), |v| v.to_string())
}

// ACE: DeveloperCommands.HandleVendorDump
// Not ACE's (a fix): with the currency weenie missing the dump is still
// sent (see `currency_unit`); ACE threw on MoneyOutflow's unit name.
/// `vendordump [all|summary|createList|uniques]`: lists all properties for the last vendor you
/// examined.
#[allow(clippy::too_many_lines)]
pub fn handle_vendor_dump(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    use empyrean_world::world_objects::vendor;

    let s = require(session);
    let me = session_player(w, s);
    if !has_selection(w, me) {
        return;
    }
    let object_id = selected_object_id(w, me);

    let Some(wo) = current_landblock_get_object(w, me, object_id) else {
        system_chat(
            w,
            s,
            &format!("Unable to find 0x{}", format(object_id.full(), "X8")),
            ChatMessageType::System,
        );
        return;
    };

    if object_id.is_player() {
        system_chat(
            w,
            s,
            &format!("{} (0x{wo}) is not a vendor.", name_of(w, wo)),
            ChatMessageType::System,
        );
        return;
    }

    let mut all = false;
    let mut summary = false;
    let mut create_list = false;
    let mut uniques = false;

    if parameters.is_empty() {
        all = true;
    } else {
        let args = parameters.join(" ");

        if contains_ignore_case(&args, "all") {
            all = true;
        }
        if contains_ignore_case(&args, "summary") {
            summary = true;
        }
        if contains_ignore_case(&args, "createList") {
            create_list = true;
        }
        if contains_ignore_case(&args, "uniques") {
            uniques = true;
        }

        if !all && !summary && !create_list && !uniques {
            all = true;
        }
    }

    let msg = if empyrean_world::dispatch::class_of(w, wo)
        == empyrean_world::dispatch::Class::Vendor
    {
        let v = obj(w, wo);
        let currency_wcid = v.alternate_currency().unwrap_or(u32::from(
            empyrean_entity::enums::WeenieClassName::W_COINSTACK_CLASS.0,
        ));
        let currency_weenie = w.content.get_cached_weenie(currency_wcid);

        let mut msg = format!("Vendor Dump for {} (0x{wo})\n", name_of(w, wo));

        if all || summary {
            msg += &format!("Vendor WCID: {}\n", v.biota.weenie_class_id);
            msg += &format!("Vendor WeenieClassName: {}\n", weenie_class_name(w, wo));
            msg += &format!("Vendor WeenieType: {}\n", v.biota.weenie_type.net_string());
            msg += &format!("OpenForBusiness: {}\n", bool_string(v.open_for_business()));

            msg += "Currency: ";
            let alternate = if v.alternate_currency().is_some() {
                " | AlternateCurrency"
            } else {
                ""
            };
            if let Some(cw) = &currency_weenie {
                msg += &format!(
                    "{} (WCID: {currency_wcid}){alternate}\n",
                    cw.get_plural_name()
                );
            } else {
                let from = if v.alternate_currency().is_some() {
                    ", which comes from PropertyDataId.AlternateCurrency,"
                } else {
                    ""
                };
                msg += &format!("WCID {currency_wcid}{from} is not found in the database, Vendor has been disabled as a result!\n");
            }
            msg += &format!(
                "BuyPrice: {}\n",
                v.buy_price()
                    .map_or_else(|| "NULL".to_owned(), |p| format(p, "F"))
            );
            msg += &format!(
                "SellPrice: {}\n",
                v.sell_price()
                    .map_or_else(|| "NULL".to_owned(), |p| format(p, "F"))
            );

            msg += &format!(
                "DealMagicalItems: {}\n",
                v.deal_magical_items()
                    .map_or_else(|| "NULL".to_owned(), |b| bool_string(b).to_owned())
            );
            msg += &format!(
                "VendorService: {}\n",
                v.vendor_service()
                    .map_or_else(|| "NULL".to_owned(), |b| bool_string(b).to_owned())
            );

            let merchandise = v.merchandise_item_types().map_or_else(
                || "NULL".to_owned(),
                |t| format!("{} ({t})", ItemType(t.cast_unsigned()).net_string()),
            );
            msg += &format!("MerchandiseItemTypes: {merchandise}\n");
            msg += &format!(
                "MerchandiseMinValue: {}\n",
                or_null(v.merchandise_min_value())
            );
            msg += &format!(
                "MerchandiseMaxValue: {}\n",
                or_null(v.merchandise_max_value())
            );

            msg += &format!("VendorHappyMean: {}\n", or_null(v.vendor_happy_mean()));
            msg += &format!(
                "VendorHappyVariance: {}\n",
                or_null(v.vendor_happy_variance())
            );
            msg += &format!(
                "VendorHappyMaxItems: {}\n",
                or_null(v.vendor_happy_max_items())
            );

            let cw = currency_weenie.as_deref();
            msg += &format!(
                "MoneyOutflow: {} {}\n",
                format(v.money_outflow(), "N0"),
                currency_unit(cw, currency_wcid, i64::from(v.money_outflow()))
            );
            msg += &format!("NumItemsBought: {}\n", format(v.num_items_bought(), "N0"));
            msg += &format!("NumItemsSold: {}\n", format(v.num_items_sold(), "N0"));
            msg += &format!("NumServicesSold: {}\n", format(v.num_services_sold(), "N0"));
            msg += &format!(
                "MoneyIncome: {} {}\n",
                format(v.money_income(), "N0"),
                currency_unit(cw, currency_wcid, i64::from(v.money_income()))
            );
        }

        if all || create_list {
            let create_list_shop: Vec<
                empyrean_entity::models::properties_create_list::PropertiesCreateList,
            > = v
                .biota
                .properties_create_list
                .iter()
                .flat_map(|l| l.iter())
                .filter(|x| x.destination_type == DestinationType::Shop)
                .cloned()
                .collect();

            msg += &format!("createListShop.Count: {}\n", create_list_shop.len());
            msg += "===============================================\n";
            for shop_item in create_list_shop {
                match w.content.get_cached_weenie(shop_item.weenie_class_id) {
                    None => {
                        msg += &format!("{} is not in the database, which will be skipped on load, and will not be sold by this vendor.\n", shop_item.weenie_class_id);
                    }
                    Some(item_weenie) => {
                        msg += &format!(
                            "{} ({} | {} | {})\n",
                            item_weenie.get_name().unwrap_or_default(),
                            item_weenie.weenie_class_id,
                            item_weenie.class_name.clone().unwrap_or_default(),
                            item_weenie.weenie_type.net_string()
                        );
                        if item_weenie.is_vendor_service() {
                            let service_spell = item_weenie.get_property(PropertyDataId::Spell);
                            let spell_text = match service_spell {
                                Some(id) => {
                                    let spell =
                                        empyrean_world::entity::spell::Spell::new(w, id, true);
                                    format!(
                                        "{} ({}): {}",
                                        spell.name(),
                                        spell.id(),
                                        spell.description()
                                    )
                                }
                                None => "NULL SPELL".to_owned(),
                            };
                            msg += &format!("This is a vendor service which casts the following spell on purchaser: {spell_text}\n");
                        } else {
                            let unlimited = if shop_item.stack_size == -1 {
                                " (Unlimited)"
                            } else {
                                " (per single transction)"
                            };
                            msg += &format!(
                                "StackSize: {}{unlimited} | PaletteTemplate: {} ({}) | Shade: {}\n",
                                shop_item.stack_size,
                                empyrean_entity::enums::PaletteTemplate(i32::from(
                                    shop_item.palette
                                ))
                                .net_string(),
                                shop_item.palette,
                                to_string(shop_item.shade)
                            );
                        }

                        let cost = vendor::get_sell_cost_weenie(w, wo, &item_weenie);
                        msg += &format!(
                            "Cost: {} {}\n",
                            format(cost, "N0"),
                            currency_unit(
                                currency_weenie.as_deref(),
                                currency_wcid,
                                i64::from(cost)
                            )
                        );
                    }
                }

                msg += "===============================================\n";
            }
        }

        if all || uniques {
            let unique_items = vendor::unique_items_for_sale(w, wo);
            msg += &format!("UniqueItemsForSale.Count: {}\n", unique_items.len());
            msg += "===============================================\n";
            for shop_item in unique_items {
                let si = obj(w, shop_item);
                msg += &format!(
                    "{} (0x{shop_item} | {} | {} | {})\n",
                    name_of(w, shop_item),
                    si.biota.weenie_class_id,
                    weenie_class_name(w, shop_item),
                    si.biota.weenie_type.net_string()
                );
                let palette = si.palette_template();
                msg += &format!(
                    "StackSize: {} | PaletteTemplate: {} ({}) | Shade: {}\n",
                    si.stack_size().unwrap_or(1),
                    palette
                        .map(|p| empyrean_entity::enums::PaletteTemplate(p).net_string())
                        .unwrap_or_default(),
                    opt_string(palette),
                    si.shade().map(|v| format(v, "F3")).unwrap_or_default()
                );
                let sold = si.sold_timestamp();
                let sold_timestamp =
                    empyrean_common::time::Time::get_date_time_from_timestamp(sold.unwrap_or(0.0));
                msg += &format!(
                    "SoldTimestamp: {} ({})\n",
                    to_common_string(sold_timestamp),
                    sold.map_or_else(|| "NULL".to_owned(), to_string)
                );
                let rot = empyrean_world::managers::property_manager::get_double(
                    w,
                    "vendor_unique_rot_time",
                    0.0,
                    true,
                )
                .item;
                let rot_time = sold_timestamp.add_seconds(rot);
                msg += &format!("RotTimestamp: {}\n", to_common_string(rot_time));
                let payout = vendor::get_buy_cost(w, wo, shop_item);
                msg += &format!(
                    "Paid: {} {}\n",
                    format(payout, "N0"),
                    currency_unit(currency_weenie.as_deref(), currency_wcid, i64::from(payout))
                );
                let cost = vendor::get_sell_cost(w, wo, shop_item);
                msg += &format!(
                    "Cost: {} {}\n",
                    format(cost, "N0"),
                    currency_unit(currency_weenie.as_deref(), currency_wcid, i64::from(cost))
                );

                msg += "===============================================\n";
            }
        }
        msg
    } else {
        format!("{} (0x{wo}) is not a vendor.", name_of(w, wo))
    };

    system_chat(w, s, &msg, ChatMessageType::System);
}

// ACE: DeveloperCommands.HandleCastSpell
/// `castspell <spell id>`: casts a spell on the last appraised object.
pub fn handle_cast_spell(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let Some(spell_id) = dotnet_parse::uint_try_parse(&parameters[0]) else {
        write_output_info(
            w,
            session,
            &format!("Invalid spell id {}", parameters[0]),
            ChatMessageType::Broadcast,
        );
        return;
    };

    let spell = empyrean_world::entity::spell::Spell::new(w, spell_id, true);

    if spell.not_found() {
        write_output_info(
            w,
            session,
            &format!("Spell {spell_id} not found"),
            ChatMessageType::Broadcast,
        );
        return;
    }

    let mut target = None;

    if spell.non_component_target_type() != ItemType::None {
        let Some(t) = last_appraised(w, session) else {
            return;
        };
        target = Some(t);
    }

    let me = session_player(w, require(session));
    empyrean_world::world_objects::world_object_magic::try_cast_spell(
        w, me, &spell, target, None, None, false, false, false,
    );
}

// ACE: DeveloperCommands.HandleUseWithTarget
/// `usewith <guid>`: uses the specified object on the last appraised object.
pub fn handle_use_with_target(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let guid = if parameters[0].starts_with("0x") {
        let hex = substring_from(&parameters[0], 2);
        dotnet_parse::uint_try_parse_hex(&hex)
    } else {
        dotnet_parse::uint_try_parse(&parameters[0])
    };
    let Some(guid) = guid else {
        write_output_info(
            w,
            session,
            &format!("Invalid guid {}", parameters[0]),
            ChatMessageType::Broadcast,
        );
        return;
    };

    //var spell = new Spell(spellId);

    //if (spell.NotFound)
    //{
    //    CommandHandlerHelper.WriteOutputInfo(session, $"Spell {spellId} not found");
    //    return;
    //}

    let Some(target) = last_appraised(w, session) else {
        return;
    };

    //session.Player.TryCastSpell(spell, target, tryResist: false);

    let me = session_player(w, require(session));
    empyrean_world::world_objects::player_use::handle_action_use_with_target(
        w,
        me,
        guid,
        target.full(),
    );
}

// ACE: DeveloperCommands.HandlePortalStorm
/// `portalstorm <0-3>`: tests starting a portal storm on yourself.
pub fn handle_portal_storm(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    use empyrean_world::network::game_event::events::{
        game_event_portal_storm::game_event_portal_storm,
        game_event_portal_storm_brewing::game_event_portal_storm_brewing,
        game_event_portal_storm_imminent::game_event_portal_storm_imminent,
        game_event_portal_storm_subsided::game_event_portal_storm_subsided,
    };

    let Some(mut storm_level) = dotnet_parse::uint_try_parse(&parameters[0]) else {
        write_output_info(
            w,
            session,
            &format!("Invalid storm level {}", parameters[0]),
            ChatMessageType::Broadcast,
        );
        return;
    };
    if storm_level > 3 {
        storm_level = 3;
    }

    let s = require(session);
    match storm_level {
        0 => {
            // (the event's `extent` defaults to 0.4)
            let msg = game_event_portal_storm_brewing(session_data(w, s), 0.4);
            enqueue_send(w, s, msg);
        }
        1 => {
            // (the event's `extent` defaults to 0.6)
            let msg = game_event_portal_storm_imminent(session_data(w, s), 0.6);
            enqueue_send(w, s, msg);
        }
        2 => {
            // Portal Storm Event comes immediatley before the teleport
            let msg = game_event_portal_storm(session_data(w, s));
            enqueue_send(w, s, msg);

            // We're going to move the player to 0,0
            let new_pos =
                Position::from_components(0x7F7F_001C, 84.0, 84.0, 80.0, 0.0, 0.0, 0.0, 1.0, false);
            let me = session_player(w, s);
            player_location::teleport(w, me, &new_pos, false);
        }
        _ => {
            let msg = game_event_portal_storm_subsided(session_data(w, s));
            enqueue_send(w, s, msg);
        }
    }
}
// ---------------------------------------------------------------------------------------------
// Pointers to members that are not ported yet (their owners swap these for the real calls).
// ---------------------------------------------------------------------------------------------

/// `player.SendUseDoneEvent()`.
fn player_send_use_done_event(w: &mut World, player: ObjectGuid) {
    empyrean_world::world_objects::player_use::send_use_done_event(
        w,
        player,
        empyrean_entity::enums::WeenieError::None,
    );
}

/// `player.GrantLuminance(amount, xpType, shareType)`.
fn player_grant_luminance(
    w: &mut World,
    player: ObjectGuid,
    amount: i64,
    xp_type: XpType,
    share_type: ShareType,
) {
    empyrean_world::world_objects::player_luminance::grant_luminance(
        w, player, amount, xp_type, share_type,
    );
}

/// `item.HasItemLevel`.
fn world_object_has_item_level(w: &World, item: ObjectGuid) -> bool {
    w.objects
        .get(item)
        .expect("ACE: item is null (NullReferenceException)")
        .has_item_level()
}

/// `session.Player.UpdateProperty(obj, PropertyInt prop, value, broadcast)`.
fn player_update_property_int(
    w: &mut World,
    player: ObjectGuid,
    obj: ObjectGuid,
    prop: PropertyInt,
    value: Option<i32>,
    broadcast: bool,
) {
    empyrean_world::world_objects::player_properties::update_property_int(
        w, player, obj, prop, value, broadcast,
    );
}

/// `session.Player.UpdateProperty(obj, PropertyInt64 prop, value, broadcast)`.
fn player_update_property_int64(
    w: &mut World,
    player: ObjectGuid,
    obj: ObjectGuid,
    prop: PropertyInt64,
    value: Option<i64>,
    broadcast: bool,
) {
    empyrean_world::world_objects::player_properties::update_property_int64(
        w, player, obj, prop, value, broadcast,
    );
}

/// `session.Player.UpdateProperty(obj, PropertyBool prop, value, broadcast)`.
fn player_update_property_bool(
    w: &mut World,
    player: ObjectGuid,
    obj: ObjectGuid,
    prop: PropertyBool,
    value: Option<bool>,
    broadcast: bool,
) {
    empyrean_world::world_objects::player_properties::update_property_bool(
        w, player, obj, prop, value, broadcast,
    );
}

/// `session.Player.UpdateProperty(obj, PropertyFloat prop, value, broadcast)`.
fn player_update_property_float(
    w: &mut World,
    player: ObjectGuid,
    obj: ObjectGuid,
    prop: PropertyFloat,
    value: Option<f64>,
    broadcast: bool,
) {
    empyrean_world::world_objects::player_properties::update_property_float(
        w, player, obj, prop, value, broadcast,
    );
}

/// `session.Player.UpdateProperty(obj, PropertyString prop, value, broadcast)`.
fn player_update_property_string(
    w: &mut World,
    player: ObjectGuid,
    obj: ObjectGuid,
    prop: PropertyString,
    value: Option<String>,
    broadcast: bool,
) {
    empyrean_world::world_objects::player_properties::update_property_string(
        w,
        player,
        obj,
        prop,
        value.as_deref(),
        broadcast,
    );
}

/// `session.Player.UpdateProperty(obj, PropertyInstanceId prop, value, broadcast)`.
fn player_update_property_instance_id(
    w: &mut World,
    player: ObjectGuid,
    obj: ObjectGuid,
    prop: PropertyInstanceId,
    value: Option<u32>,
    broadcast: bool,
) {
    empyrean_world::world_objects::player_properties::update_property_instance_id(
        w, player, obj, prop, value, broadcast,
    );
}

/// `session.Player.UpdateProperty(obj, PropertyDataId prop, value, broadcast)`.
fn player_update_property_data_id(
    w: &mut World,
    player: ObjectGuid,
    obj: ObjectGuid,
    prop: PropertyDataId,
    value: Option<u32>,
    broadcast: bool,
) {
    empyrean_world::world_objects::player_properties::update_property_data_id(
        w, player, obj, prop, value, broadcast,
    );
}

/// `player.House`.
fn player_house(w: &World, player: ObjectGuid) -> Option<ObjectGuid> {
    empyrean_world::world_objects::player_house::house(w, player)
}

/// `house.GetRentDue(purchaseTime)`.
fn house_get_rent_due(w: &World, house: ObjectGuid, purchase_time: u32) -> u32 {
    empyrean_world::world_objects::house::get_rent_due(w, house, purchase_time)
}

/// `HouseManager.BuildRentQueue()`.
fn house_manager_build_rent_queue(w: &mut World) {
    empyrean_world::managers::house_manager::build_rent_queue(w);
}

/// `session.Player.ChessMatch?.DebugMove()`: ACE writes the move log to the console.
fn chess_match_debug_move(w: &mut World, player: ObjectGuid) {
    if let Some(chess_match) = empyrean_world::world_objects::player_chess::chess_match(w, player) {
        empyrean_common::console_write!("{}", chess_match.get(w).debug_move());
    }
}

/// `session.Player.ChessMatch?.Logic?.DebugBoard()`: ACE writes the board to the console.
fn chess_logic_debug_board(w: &mut World, player: ObjectGuid) {
    if let Some(chess_match) = empyrean_world::world_objects::player_chess::chess_match(w, player) {
        empyrean_common::console_write!("{}", chess_match.get(w).logic.debug_board());
    }
}

/// `BSPCache.Clear()`. Not ported: the shared physics has no such cache (see
/// `admin_stat_commands`' cache counts), so there is nothing to clear.
// ACE: BSPCache.Clear
fn bsp_cache_clear(_w: &mut World) {}

/// `GfxObjCache.Clear()`. Not ported, as [`bsp_cache_clear`].
// ACE: GfxObjCache.Clear
fn gfx_obj_cache_clear(_w: &mut World) {}

/// `PolygonCache.Clear()`. Not ported, as [`bsp_cache_clear`].
// ACE: PolygonCache.Clear
fn polygon_cache_clear(_w: &mut World) {}

/// `VertexCache.Clear()`. Not ported, as [`bsp_cache_clear`].
// ACE: VertexCache.Clear
fn vertex_cache_clear(_w: &mut World) {}

/// `session.Player.SetHouseOwner(slumlord)`.
fn player_set_house_owner(w: &mut World, player: ObjectGuid, slumlord: ObjectGuid) {
    empyrean_world::world_objects::player_house::set_house_owner(w, player, slumlord);
}

/// `session.Player.GiveDeed(slumlord)`.
fn player_give_deed(w: &mut World, player: ObjectGuid, slumlord: ObjectGuid) {
    empyrean_world::world_objects::player_house::give_deed(w, player, slumlord);
}

/// `HouseManager.ApartmentBlocks.ContainsKey(landblock)`.
fn house_manager_apartment_blocks_contains_key(_w: &World, landblock: u32) -> bool {
    empyrean_world::managers::house_manager::apartment_block(landblock).is_some()
}

/// `HouseCell.HouseCells.ContainsKey(cell)`.
fn house_cell_house_cells_contains_key(_w: &World, cell: u32) -> bool {
    empyrean_tables::house_cell::HOUSE_CELLS.contains_key(&cell)
}

/// `session.Player.PhysicsObj.ethereal_check_for_collisions()`: whether any other unparented
/// body sharing one of the player's cells overlaps it where it stands (false without a body).
fn physics_obj_ethereal_check_for_collisions(w: &mut World, player: ObjectGuid) -> bool {
    let Some(h) = obj(w, player).phys else {
        return false;
    };
    w.physics.ethereal_check_for_collisions(h)
}

/// `session.Player.ConfirmationManager.EnqueueSend(new Confirmation_Custom(session.Player.Guid,
/// () => HandleDelevel(session, true, parameters)), msg)`.
fn confirmation_manager_enqueue_send_delevel(
    w: &mut World,
    player: ObjectGuid,
    session: SessionId,
    parameters: &[String],
    msg: &str,
) -> bool {
    let parameters = parameters.to_vec();
    let action: empyrean_world::entity::confirmation::CustomAction =
        Box::new(move |w: &mut World| {
            handle_delevel_confirmed(w, Some(session), true, &parameters)
        });
    let confirmation = empyrean_world::entity::confirmation::Confirmation::custom(player, action);
    empyrean_world::world_objects::managers::confirmation_manager::enqueue_send(
        w,
        player,
        confirmation,
        msg,
    )
}

/// `new TreasureWieldedTable(creature.WieldedTreasure).Sets`.
fn treasure_wielded_table_new(
    _w: &World,
    wielded_treasure: &[empyrean_content::models::world::treasure_wielded::TreasureWielded],
) -> Vec<WieldedTreasureSet> {
    empyrean_world::entity::treasure_wielded_table::TreasureWieldedTable::new(wielded_treasure).sets
}

/// `(int)Aetheria.SigilToEquipmentSet[(Sigil)set]` (a set outside the enum:
/// `KeyNotFoundException`).
fn aetheria_sigil_to_equipment_set(_w: &World, set: i32) -> i32 {
    use empyrean_world::entity::aetheria;
    let sigil = aetheria::Sigil::from_i32(set)
        .unwrap_or_else(|| panic!("KeyNotFoundException: SigilToEquipmentSet[{set}]"));
    aetheria::sigil_to_equipment_set(sigil).0
}

/// `Aetheria.Icons[(AetheriaColor)color][(Sigil)set]` (filled by Aetheria's static constructor).
fn aetheria_icons(_w: &World, color: i32, set: i32) -> u32 {
    use empyrean_world::entity::aetheria;
    let color = aetheria::AetheriaColor::from_i32(color)
        .unwrap_or_else(|| panic!("KeyNotFoundException: Icons[{color}]"));
    let sigil = aetheria::Sigil::from_i32(set)
        .unwrap_or_else(|| panic!("KeyNotFoundException: Icons[..][{set}]"));
    aetheria::icons(color, sigil)
}

/// `Aetheria.SurgeTargetSelf[procSpell]`.
fn aetheria_surge_target_self(_w: &World, proc_spell: SpellId) -> bool {
    empyrean_world::entity::aetheria::surge_target_self(proc_spell)
        .unwrap_or_else(|| panic!("KeyNotFoundException: SurgeTargetSelf[{proc_spell:?}]"))
}

/// `Aetheria.ColorToMask[color]`: the sigil slot an aetheria of `color` equips in (a color outside
/// the enum: `KeyNotFoundException`).
///
/// # Panics
/// For a color outside the enum, where ACE throws.
#[must_use]
pub fn aetheria_color_to_mask(color: i32) -> EquipMask {
    use empyrean_world::entity::aetheria;
    let color = aetheria::AetheriaColor::from_i32(color)
        .unwrap_or_else(|| panic!("KeyNotFoundException: ColorToMask[{color}]"));
    aetheria::color_to_mask(color)
}

/// This file's `[CommandHandler]` decorations, in declaration order. `knownobjs` replaces
/// AdminCommands' entry in its slot; `nudge` is replaced by DeveloperContentCommands' own once that
/// file is ported (it registers later).
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn command_handlers() -> Vec<CommandHandlerInfo> {
    let none = CommandHandlerFlag::None;
    let console = CommandHandlerFlag::ConsoleInvoke;
    let world = CommandHandlerFlag::RequiresWorld;
    let rows: Vec<(CommandHandlerAttribute, NamedHandler)> = vec![
        (CommandHandlerAttribute::new("nudge", AccessLevel::Developer, world, false, 0, "Correct player position cell ID after teleporting into black space.", ""), handler!(handle_nudge)),
        (CommandHandlerAttribute::new("fixbusy", AccessLevel::Player, world, false, 0, "Attempts to remove the hourglass / fix the busy state for the player", ""), handler!(handle_fix_busy)),
        (CommandHandlerAttribute::new("equiptest", AccessLevel::Developer, world, false, -1, "Simulates equipping a new item to your character, replacing all other items.", ""), handler!(equip_test)),
        (CommandHandlerAttribute::new("netstats", AccessLevel::Developer, none, false, -1, "View network statistics", ""), handler!(handle_net_stats)),
        (CommandHandlerAttribute::new("listcb", AccessLevel::Developer, console, false, -1, "List Clothing Tables available", ""), handler!(handle_show_compatible_clothing_bases)),
        (CommandHandlerAttribute::new("echo", AccessLevel::Developer, world, false, -1, "Send text back to yourself.", "\"text to send back to yourself\" [ChatMessageType]\nChatMessageType can be a uint or enum name"), handler!(handle_debug_echo)),
        (CommandHandlerAttribute::new("playsound", AccessLevel::Developer, world, false, 1, "Plays a sound.", "sound (volume) (guid)\nSound can be uint or enum name\nVolume and source guid are optional"), handler!(handle_play_sound)),
        (CommandHandlerAttribute::new("effect", AccessLevel::Developer, world, false, 1, "Plays an effect.", "effect (float)\nEffect can be uint or enum namefloat is scale level"), handler!(handle_play_effect)),
        (CommandHandlerAttribute::new("chatdump", AccessLevel::Developer, world, false, 0, "Spews 1000 lines of text to you.", ""), handler!(chat_dump)),
        (CommandHandlerAttribute::new("animation", AccessLevel::Developer, world, false, 1, "Plays an animation on the current player, or optionally another object", "MotionCommand (optional target guid)\n"), handler!(animation)),
        (CommandHandlerAttribute::new("movement", AccessLevel::Developer, world, false, 1, "Movement testing command, to be removed soon", ""), handler!(movement)),
        (CommandHandlerAttribute::new("MoveTo", AccessLevel::Developer, world, false, 0, "Used to test the MoveToObject message.   It will spawn a training wand in front of you and then move to that object.", "moveto\noptional parameter distance if omitted 10f"), handler!(move_to)),
        (CommandHandlerAttribute::new("barbershop", AccessLevel::Developer, world, false, -1, "Displays the barber ui", ""), handler!(barber_shop)),
        (CommandHandlerAttribute::new("listplayers", AccessLevel::Developer, none, false, 0, "Displays all of the active players connected too the server.", ""), handler!(handle_list_players)),
        (CommandHandlerAttribute::new("save-now", AccessLevel::Developer, world, false, -1, "Saves your session.", ""), handler!(handle_save_now)),
        (CommandHandlerAttribute::new("loadalllandblocks", AccessLevel::Developer, none, false, -1, "Loads all Landblocks. This is VERY crude. Do NOT use it on a live server!!! It will likely crash the server.  Landblock resources will be loaded async and will continue to do work even after all landblocks have been loaded.", ""), handler!(handle_load_all_landblocks)),
        (CommandHandlerAttribute::new("propertydump", AccessLevel::Developer, world, false, -1, "Lists all properties for the last world object you examined.", ""), handler!(handle_property_dump)),
        (CommandHandlerAttribute::new("whoami", AccessLevel::Developer, world, false, -1, "Shows you your GUIDs.", ""), handler!(handle_who_am_i)),
        (CommandHandlerAttribute::new("echoflags", AccessLevel::Developer, none, false, 2, "Echo flags back to you", "[type to test] [int]\n"), handler!(handle_debug_echo_flags)),
        (CommandHandlerAttribute::new("setcoin", AccessLevel::Developer, world, false, 1, "Set Coin display debug only usage", ""), handler!(handle_set_coin)),
        (CommandHandlerAttribute::new("telexyz", AccessLevel::Developer, world, false, 8, "Teleport to a location.", "cell x y z qx qy qz qw\nall parameters must be specified and cell must be in decimal form"), handler!(handle_debug_teleport_xyz)),
        (CommandHandlerAttribute::new("teletype", AccessLevel::Developer, world, false, 1, "Teleport to a saved character position.", "uint 0-22\n@teletype 1"), handler!(handle_tele_type)),
        (CommandHandlerAttribute::new("listpositions", AccessLevel::Developer, world, false, 0, "Displays all available saved character positions from the database.", ""), handler!(handle_list_positions)),
        (CommandHandlerAttribute::new("setposition", AccessLevel::Developer, world, false, 1, "Saves the supplied character position type to the database.", "uint 1-27\n@setposition 1"), handler!(handle_set_position)),
        (CommandHandlerAttribute::new("gps", AccessLevel::Developer, world, false, -1, "Display location.", ""), handler!(handle_debug_gps)),
        (CommandHandlerAttribute::new("addtitle", AccessLevel::Developer, world, false, 1, "Add title to yourself", "[titleid]"), handler!(handle_add_title)),
        (CommandHandlerAttribute::new("addalltitles", AccessLevel::Developer, world, false, -1, "Add all titles to yourself", ""), handler!(handle_add_all_titles)),
        (CommandHandlerAttribute::new("grantxp", AccessLevel::Developer, world, false, 1, "Give XP to yourself (or the specified character).", "ulong\n@grantxp [name] 191226310247 is max level 275"), handler!(handle_grant_xp)),
        (CommandHandlerAttribute::new("grantluminance", AccessLevel::Developer, world, false, 1, "Give luminance to yourself (or the specified character).", "ulong\n@grantluminance [name] 1500000 is max luminance"), handler!(handle_grant_luminance)),
        (CommandHandlerAttribute::new("grantitemxp", AccessLevel::Developer, world, false, 1, "Give item XP to the last appraised item.", ""), handler!(handle_grant_item_xp)),
        (CommandHandlerAttribute::new("spendallxp", AccessLevel::Developer, world, false, 0, "Spend all available XP on Attributes, Vitals and Skills.", ""), handler!(handle_spend_all_xp)),
        (CommandHandlerAttribute::new("setvital", AccessLevel::Developer, world, false, 2, "Sets the specified vital to a specified value", "Usage: @setvital <vital> <value>\n<vital> is one of the following strings:\n    health, hp\n    stamina, stam, sp\n    mana, mp\n<value> is an integral value [0-9]+, or a relative value [-+][0-9]+"), handler!(set_vital)),
        (CommandHandlerAttribute::new("sethealth", AccessLevel::Developer, world, false, 1, "sets your current health to a specific value.", "ushort"), handler!(handle_set_health)),
        (CommandHandlerAttribute::new("harmself", AccessLevel::Developer, world, false, -1, "Sets all player vitals to 1", ""), handler!(harm_self)),
        (CommandHandlerAttribute::new("weapons", AccessLevel::Developer, world, false, 0, "Creates testing items in your inventory.", ""), handler!(handle_weapons)),
        (CommandHandlerAttribute::new("inv", AccessLevel::Developer, world, false, 0, "Creates sample items, foci and containers in your inventory.", ""), handler!(handle_inv)),
        (CommandHandlerAttribute::new("splits", AccessLevel::Developer, world, false, 0, "Creates some stackable items in your inventory for testing.", ""), handler!(handle_splits)),
        (CommandHandlerAttribute::new("comps", AccessLevel::Developer, world, false, 0, "Creates spell component items in your inventory for testing.", ""), handler!(handle_comps)),
        (CommandHandlerAttribute::new("food", AccessLevel::Developer, world, false, 0, "Creates some food items in your inventory for testing.", ""), handler!(handle_food)),
        (CommandHandlerAttribute::new("currency", AccessLevel::Developer, world, false, 0, "Creates some currency items in your inventory for testing.", ""), handler!(handle_currency)),
        (CommandHandlerAttribute::new("cirand", AccessLevel::Developer, world, false, 1, "Creates random objects in your inventory.", "type (string or number) <num to create> defaults to 10 if omitted, max 50"), handler!(handle_ci_random)),
        (CommandHandlerAttribute::new("addallspells", AccessLevel::Developer, world, false, 0, "Adds all known spells to your own spellbook.", ""), handler!(handle_add_all_spells)),
        (CommandHandlerAttribute::new("getspellformula", AccessLevel::Developer, console, false, 0, "Tests spell formula calculation", ""), handler!(get_spell_formula)),
        (CommandHandlerAttribute::new("getallspellformula", AccessLevel::Developer, console, false, 0, "Tests spell formula calculation", ""), handler!(get_all_spell_formula)),
        (CommandHandlerAttribute::new("readdat", AccessLevel::Developer, console, false, 0, "Tests reading the client_portal.dat", ""), handler!(read_dat)),
        (CommandHandlerAttribute::new("contract", AccessLevel::Developer, world, false, 1, "Query, stamp, and erase contracts on the targeted player", "[list | bestow | erase]\ncontract list - List the contracts for the targeted player\ncontract bestow - Stamps the specific contract on the targeted player. If this fails, it's probably because the contract is invalid.\ncontract erase - Erase the specific contract from the targeted player. If no quest flag is given, it erases the entire contract table for the targeted player.\n"), handler!(handle_contract)),
        (CommandHandlerAttribute::new("turnto", AccessLevel::Developer, world, false, 0, "Turns the last appraised object to the player", "turnto"), handler!(handle_request_turn_to)),
        (CommandHandlerAttribute::new("debugmove", AccessLevel::Developer, world, false, 0, "Toggles movement debugging for the last appraised monster", "<on/off>"), handler!(toggle_movement_debug)),
        (CommandHandlerAttribute::new("lostest", AccessLevel::Developer, world, false, -1, "Tests for direct visibilty with latest appraised object", ""), handler!(handle_visible)),
        (CommandHandlerAttribute::new("showstats", AccessLevel::Developer, world, false, 0, "Shows a list of a creature's current attribute/skill levels", "showstats"), handler!(handle_show_stats)),
        (CommandHandlerAttribute::new("givemana", AccessLevel::Developer, world, false, 0, "Gives mana to the last appraised object", "<amount>"), handler!(handle_give_mana)),
        (CommandHandlerAttribute::new("dist", AccessLevel::Developer, world, false, 0, "Returns the distance to the last appraised object", ""), handler!(handle_dist)),
        (CommandHandlerAttribute::new("teledist", AccessLevel::Developer, world, false, 1, "Teleports a some distance ahead of the last object spawned", "<distance>"), handler!(handle_teleport_dist)),
        (CommandHandlerAttribute::new("knownobjs", AccessLevel::Developer, world, false, 0, "Shows the list of objects currently known to an object", "<optional guid, or optional 'target' for last appraisal target>"), handler!(handle_known_objs)),
        (CommandHandlerAttribute::new("visibleobjs", AccessLevel::Developer, world, false, 0, "Shows the list of objects currently visible to an object", "<optional guid, or optional 'target' for last appraisal target>"), handler!(handle_visible_objs)),
        (CommandHandlerAttribute::new("knownplayers", AccessLevel::Developer, world, false, 0, "Shows the list of players known to an object", "<optional guid, or optional 'target' for last appraisal target>"), handler!(handle_known_players)),
        (CommandHandlerAttribute::new("visibleplayers", AccessLevel::Developer, world, false, 0, "Shows the list of players visible to a player", "<optional guid, or optional 'target' for last appraisal target>"), handler!(handle_visible_players)),
        (CommandHandlerAttribute::new("visibletargets", AccessLevel::Developer, world, false, 0, "Shows the list of targets currently visible to a monster", "<optional guid, or optional 'target' for last appraisal target>"), handler!(handle_visible_targets)),
        (CommandHandlerAttribute::new("retaliatetargets", AccessLevel::Developer, world, false, 0, "Shows the list of retaliate targets for a monster", "<optional guid, or optional 'target' for last appraisal target>"), handler!(handle_retaliate_targets)),
        (CommandHandlerAttribute::new("destructionqueue", AccessLevel::Developer, world, false, 0, "Shows the list of previously visible objects queued for destruction for a player", "<optional guid, or optional 'target' for last appraisal target>"), handler!(handle_destruction_queue)),
        (CommandHandlerAttribute::new("debugemote", AccessLevel::Developer, world, false, 0, "Enables emote debugging for the last appraised object", ""), handler!(handle_debug_emote)),
        (CommandHandlerAttribute::new("myloc", AccessLevel::Developer, world, false, 0, "Shows the current player location, from the server perspective", ""), handler!(handle_my_loc)),
        (CommandHandlerAttribute::new("getproperty", AccessLevel::Developer, world, false, 1, "Gets a property for the last appraised object", "<property>"), handler!(handle_get_property)),
        (CommandHandlerAttribute::new("setproperty", AccessLevel::Developer, world, false, 2, "Sets a property for the last appraised object", "<property> <value>"), handler!(handle_set_property)),
        (CommandHandlerAttribute::new("setpurchasetime", AccessLevel::Developer, world, false, 0, "Sets the house purchase time for this player", ""), handler!(handle_set_purchase_time)),
        (CommandHandlerAttribute::new("debugdamage", AccessLevel::Developer, world, false, 0, "Toggles the display for player damage info", "<attack|defense|all|on|off>"), handler!(handle_debug_damage)),
        (CommandHandlerAttribute::new("enable-aetheria", AccessLevel::Developer, world, false, 0, "Enables the aetheria slots for the player", ""), handler!(handle_enable_aetheria)),
        (CommandHandlerAttribute::new("debugchess", AccessLevel::Developer, world, false, -1, "Shows the chess move history for a player", ""), handler!(handle_debug_chess)),
        (CommandHandlerAttribute::new("debugboard", AccessLevel::Developer, world, false, -1, "Shows the current chess board state", ""), handler!(handle_debug_board)),
        (CommandHandlerAttribute::new("teledungeon", AccessLevel::Developer, world, false, 1, "Teleport to a dungeon", "<dungeon name or landblock>"), handler!(handle_tele_dungeon)),
        (CommandHandlerAttribute::new("dungeonname", AccessLevel::Developer, world, false, -1, "Shows the dungeon name for the current landblock", ""), handler!(handle_dungeon_name)),
        (CommandHandlerAttribute::new("clearphysicscaches", AccessLevel::Developer, none, false, 0, "Clears Physics Object Caches", ""), handler!(handle_clear_physics_caches)),
        (CommandHandlerAttribute::new("forcegc", AccessLevel::Developer, none, false, 0, "Forces .NET Garbage Collection", ""), handler!(handle_force_gc)),
        (CommandHandlerAttribute::new("forcegc2", AccessLevel::Developer, none, false, 0, "Forces .NET Garbage Collection with LOH Compact", ""), handler!(handle_force_gc2)),
        (CommandHandlerAttribute::new("auditobjectmaint", AccessLevel::Developer, none, false, 0, "Iterates over physics objects to find leaks", ""), handler!(handle_audit_object_maint)),
        (CommandHandlerAttribute::new("lootgen", AccessLevel::Developer, world, false, 1, "Generate a piece of loot from the LootGenerationFactory.", "<wcid or classname> <tier>"), handler!(handle_loot_gen)),
        (CommandHandlerAttribute::new("ciloot", AccessLevel::Developer, world, false, 1, "Generates randomized loot in player's inventory", "<tier> optional: <# items>"), handler!(handle_ci_loot)),
        (CommandHandlerAttribute::new("makeiou", AccessLevel::Developer, world, false, 1, "Make an IOU and put it in your inventory", "<wcid>"), handler!(handle_make_iou)),
        (CommandHandlerAttribute::new("testdeathitems", AccessLevel::Developer, world, false, 0, "Test death item selection", ""), handler!(handle_test_death_items)),
        (CommandHandlerAttribute::new("forcelogout", AccessLevel::Developer, world, false, -1, "Force log off of specified character or last appraised character", ""), handler!(handle_force_logout)),
        (CommandHandlerAttribute::new("forcelogoff", AccessLevel::Developer, world, false, -1, "Force log off of specified character or last appraised character", ""), handler!(handle_force_logoff)),
        (CommandHandlerAttribute::new("showsession", AccessLevel::Developer, world, false, -1, "Show IP and ID for network session of last appraised character", ""), handler!(handle_show_session)),
        (CommandHandlerAttribute::new("requirecomps", AccessLevel::Developer, world, false, 1, "Sets whether spell components are required to cast spells.", "[ on | off ]\nThis command sets whether spell components are required to cast spells..\n When turned on, spell components are required.\n When turned off, spell components are ignored."), handler!(handle_require_comps)),
        (CommandHandlerAttribute::new("safecomps", AccessLevel::Developer, world, false, 0, "Enables / disables spell component burning", "<on/off>"), handler!(handle_safe_comps)),
        (CommandHandlerAttribute::new("additemspell", AccessLevel::Developer, world, false, 1, "Adds a spell to the last appraised item's spellbook.", "<spell id>"), handler!(handle_add_item_spell)),
        (CommandHandlerAttribute::new("removeitemspell", AccessLevel::Developer, world, false, 1, "Removes a spell to the last appraised item's spellbook.", "<spell id>"), handler!(handle_remove_item_spell)),
        (CommandHandlerAttribute::new("pktimer", AccessLevel::Developer, world, false, -1, "Sets your PK timer to the current time", ""), handler!(handle_pk_timer)),
        (CommandHandlerAttribute::new("fellow-info", AccessLevel::Developer, world, false, -1, "Shows debug info for fellowships.", ""), handler!(handle_fellow_info)),
        (CommandHandlerAttribute::new("fellow-dist", AccessLevel::Developer, world, false, -1, "Shows distance to each fellowship member", ""), handler!(handle_fellow_dist)),
        (CommandHandlerAttribute::new("generatordump", AccessLevel::Developer, world, false, 0, "Lists all properties for the last generator you examined.", ""), handler!(handle_generator_dump)),
        (CommandHandlerAttribute::new("purchase-house", AccessLevel::Developer, world, false, -1, "Instantly purchase the house for the last appraised covenant crystal.", ""), handler!(handle_purchase_house)),
        (CommandHandlerAttribute::new("barrier-test", AccessLevel::Developer, world, false, -1, "Shows debug information for house barriers", ""), handler!(handle_barrier_test)),
        (CommandHandlerAttribute::new("targetloc", AccessLevel::Developer, world, false, -1, "Shows the location of the last appraised object", ""), handler!(handle_target_loc)),
        (CommandHandlerAttribute::new("damagehistory", AccessLevel::Developer, world, false, -1, "", ""), handler!(handle_damage_history)),
        (CommandHandlerAttribute::new("remove-vitae", AccessLevel::Developer, world, false, -1, "Removes vitae from last appraised player", ""), handler!(handle_remove_vitae)),
        (CommandHandlerAttribute::new("fast", AccessLevel::Developer, world, false, -1, "", ""), handler!(handle_fast)),
        (CommandHandlerAttribute::new("slow", AccessLevel::Developer, world, false, -1, "", ""), handler!(handle_slow)),
        (CommandHandlerAttribute::new("rip", AccessLevel::Developer, world, false, -1, "", ""), handler!(handle_rip)),
        (CommandHandlerAttribute::new("resist-info", AccessLevel::Developer, world, false, -1, "Shows the resistance info for the last appraised creature.", ""), handler!(handle_resist_info)),
        (CommandHandlerAttribute::new("debugspell", AccessLevel::Developer, world, false, -1, "Toggles spell projectile debugging info", ""), handler!(handle_debug_spell)),
        (CommandHandlerAttribute::new("recordcast", AccessLevel::Developer, world, false, -1, "Records spell casting keypresses to server for debugging", ""), handler!(handle_record_cast)),
        (CommandHandlerAttribute::new("pscript", AccessLevel::Developer, world, false, 1, "", ""), handler!(handle_p_script)),
        (CommandHandlerAttribute::new("getinfo", AccessLevel::Developer, world, false, -1, "Shows basic info for the last appraised object.", ""), handler!(handle_get_info)),
        (CommandHandlerAttribute::new("testaim", AccessLevel::Developer, world, false, 1, "Tests the aim high/low motions, and projectile spawn position", ""), handler!(handle_test_aim)),
        (CommandHandlerAttribute::new("reload-landblock", AccessLevel::Developer, world, false, -1, "Reloads the current landblock.", ""), handler!(handle_reload_landblocks)),
        (CommandHandlerAttribute::new("showvelocity", AccessLevel::Developer, world, false, -1, "Shows the velocity of the last appraised object.", ""), handler!(handle_show_velocity)),
        (CommandHandlerAttribute::new("bumpvelocity", AccessLevel::Developer, world, false, -1, "Bumps the velocity of the last appraised object.", ""), handler!(handle_bump_velocity)),
        (CommandHandlerAttribute::new("check-collision", AccessLevel::Developer, world, false, -1, "Checks if the player is currently colliding with any other objects.", ""), handler!(handle_check_ethereal)),
        (CommandHandlerAttribute::new("faction", AccessLevel::Developer, world, false, 0, "sets your own faction state.", "< none / ch / ew / rb > (rank)\nThis command sets your current faction state\n< none > No Faction\n< ch > Celestial Hand\n< ew > Eldrytch Web\n< rb > Radiant Blood\n(rank) 1 = Initiate | 2 = Adept | 3 = Knight | 4 = Lord | 5 = Master"), handler!(handle_faction)),
        (CommandHandlerAttribute::new("showtier", AccessLevel::Developer, world, false, -1, "Shows the DeathTreasure tier for the last appraised monster", ""), handler!(handle_show_tier)),
        (CommandHandlerAttribute::new("tiermobs", AccessLevel::Developer, world, false, 1, "Shows a list of monsters for a particular tier #", "tier"), handler!(handle_tier_mobs)),
        (CommandHandlerAttribute::new("delevel", AccessLevel::Developer, world, false, 1, "Attempts to delevel the current player. Requires enough unassigned xp and unspent skill credits.", "new level"), handler!(handle_delevel)),
        (CommandHandlerAttribute::new("monsterspell", AccessLevel::Developer, world, false, 1, "The last appraised creature casts a spell. For targeted spells, defaults to the current player.", "optional target guid"), handler!(handle_monster_proj)),
        (CommandHandlerAttribute::new("debugspellbook", AccessLevel::Developer, world, false, -1, "Shows the spellbook for the last appraised object", ""), handler!(handle_debug_spellbook)),
        (CommandHandlerAttribute::new("trywield", AccessLevel::Developer, world, false, 2, "", ""), handler!(handle_try_wield)),
        (CommandHandlerAttribute::new("show-wielded-treasure", AccessLevel::Developer, none, false, 1, "Shows the WieldedTreasure table for a Creature", "wcid"), handler!(handle_show_wielded_treasure)),
        (CommandHandlerAttribute::new("ciaetheria", AccessLevel::Developer, world, false, 4, "Spawns an Aetheria in the player's inventory", "[color] [set] [surge] [level]\nColor: Blue, Yellow, Red\nSet: Defense, Destruction, Fury, Growth, Vigor\nSurge: Destruction, Protection, Regeneration, Affliction, Festering\nLevel: 1 - 5"), handler!(handle_ci_aetheria)),
        (CommandHandlerAttribute::new("vendordump", AccessLevel::Developer, world, false, 0, "Lists all properties for the last vendor you examined.", ""), handler!(handle_vendor_dump)),
        (CommandHandlerAttribute::new("castspell", AccessLevel::Developer, world, false, 1, "Casts a spell on the last appraised object", "spell id"), handler!(handle_cast_spell)),
        (CommandHandlerAttribute::new("usewith", AccessLevel::Developer, world, false, 1, "Uses specified object on last appraised object", "guid"), handler!(handle_use_with_target)),
        (CommandHandlerAttribute::new("portalstorm", AccessLevel::Developer, world, false, 1, "Tests starting a portal storm on yourself", "storm_level [0=Brewing, 1=Imminent, 2=Stormed, 3=Subsided]"), handler!(handle_portal_storm)),
    ];
    rows.into_iter()
        .map(|(attribute, (handler, handler_name))| CommandHandlerInfo {
            handler,
            handler_name,
            attribute,
        })
        .collect()
}
