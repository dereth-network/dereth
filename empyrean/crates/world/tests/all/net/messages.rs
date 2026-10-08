//! Vectors: fixtures/vectors/messages/
//! GameMessage/GameEvent builders write ACE's bytes; SequenceManager; EnqueueSend.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state.

use empyrean_common::era::EraExt as _;
use std::collections::HashMap;
use std::time::Duration;

use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_common::dotnet::{DotNetDict, TimeSpan};
use empyrean_common::not_ported::take_local;
use empyrean_common::vectors::{self, f32_of, f64_of, i64_of, u64_of};
use empyrean_dat::{DatDatabaseType, FakeDats};
use empyrean_entity::enums::*;
use empyrean_entity::models::PropertiesBookPageData;
use empyrean_entity::{ObjectGuid, Position};
use empyrean_net::enums::CharacterError;
use empyrean_net::{GameMessageGroup, SessionId};
use empyrean_world::network::game_event::events::*;
use empyrean_world::network::game_event::game_event_message::game_event_message;
use empyrean_world::network::game_event::game_event_type::GameEventType;
use empyrean_world::network::game_messages::game_message::{self, GameMessage};
use empyrean_world::network::game_messages::game_message_opcode::GameMessageOpcode;
use empyrean_world::network::game_messages::messages::*;
use empyrean_world::network::sequence::byte_sequence::ByteSequence;
use empyrean_world::network::sequence::i_sequence::ISequence;
use empyrean_world::network::sequence::sequence_manager::{HasSequences, SequenceManager};
use empyrean_world::network::sequence::sequence_type::SequenceType;
use empyrean_world::network::sequence::u_int_sequence::UIntSequence;
use empyrean_world::network::sequence::u_long_sequence::ULongSequence;
use empyrean_world::network::sequence::u_short_sequence::UShortSequence;
use empyrean_world::sessions::{CharacterSummary, SessionData};
use empyrean_world::world_objects::entity::creature_attribute::CreatureAttribute;
use empyrean_world::world_objects::entity::creature_skill::CreatureSkill;
use empyrean_world::world_objects::entity::creature_vital::CreatureVital;
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::World;
use serde_json::Value;

// ---- helpers ------------------------------------------------------------------------------

/// A world object paired with its sequences, until `WorldObject.sequences` lands.
#[derive(Debug, Default)]
struct Obj {
    wo: WorldObject,
    seq: SequenceManager,
}

impl HasSequences for Obj {
    fn world_object(&self) -> &WorldObject {
        &self.wo
    }
    fn sequences(&mut self) -> &mut SequenceManager {
        &mut self.seq
    }
}

fn g(v: u32) -> ObjectGuid {
    ObjectGuid::new(v)
}

fn obj(guid: u32) -> Obj {
    Obj {
        wo: WorldObject {
            guid: g(guid),
            ..Default::default()
        },
        seq: SequenceManager::new(),
    }
}

/// An object whose biota holds the stat records the private update tests send: Strength
/// (ranks 10, starting 50, 999 XP), skill 6 (ranks 7, advancement class 2, 100 XP,
/// init 10, resistance 3, last used 1.5) and MaxHealth (ranks 1, starting 2, 3 XP, current 4).
fn stat_obj(guid: u32) -> Obj {
    use empyrean_entity::models::properties_attribute::PropertiesAttribute;
    use empyrean_entity::models::properties_attribute_2nd::PropertiesAttribute2nd;
    use empyrean_entity::models::properties_skill::PropertiesSkill;
    let mut o = obj(guid);
    let b = &mut o.wo.biota;
    let mut attributes = DotNetDict::new();
    attributes.insert(
        PropertyAttribute::Strength,
        PropertiesAttribute {
            init_level: 50,
            level_from_cp: 10,
            cp_spent: 999,
        },
    );
    b.properties_attribute = Some(attributes);
    let mut vitals = DotNetDict::new();
    let health = PropertiesAttribute2nd {
        init_level: 2,
        level_from_cp: 1,
        cp_spent: 3,
        current_level: 4,
    };
    vitals.insert(PropertyAttribute2nd::MaxHealth, health);
    b.properties_attribute_2nd = Some(vitals);
    let mut skills = DotNetDict::new();
    let skill = PropertiesSkill {
        level_from_pp: 7,
        sac: SkillAdvancementClass(2),
        pp: 100,
        init_level: 10,
        resistance_at_last_check: 3,
        last_used_time: 1.5,
    };
    skills.insert(Skill(6), skill);
    b.properties_skill = Some(skills);
    o
}

fn strength() -> CreatureAttribute {
    CreatureAttribute {
        attribute: PropertyAttribute::Strength,
    }
}

fn max_health(o: &mut Obj) -> CreatureVital {
    CreatureVital::new(&mut o.wo, PropertyAttribute2nd::MaxHealth)
}

fn player_obj(guid: u32) -> WorldObject {
    WorldObject {
        guid: g(guid),
        player: Some(Box::default()),
        creature: Some(Box::default()),
        ..Default::default()
    }
}

use crate::support::hex::hex;

fn world() -> World {
    let now = ClockSnapshot {
        portal_year_ticks: 0.0,
        unix_time: 1_000_000.0,
        utc: DotNetDateTime::new(2026, 1, 1),
        monotonic: Duration::ZERO,
    };
    World::new(now, FakeDats::new().build().expect("empty fake dats"))
}

const SESSION: SessionId = SessionId {
    client_id: 1,
    generation: 1,
};

/// A world with one session whose player (if any) is `player`, stored in the object store.
fn world_with_player(player: Option<WorldObject>, sequence: u32) -> World {
    let mut w = world();
    let guid = player.as_ref().map(|p| p.guid);
    if let Some(p) = player {
        w.objects.insert(p).expect("fresh guid");
    }
    w.sessions.insert(
        SESSION,
        SessionData {
            player: guid,
            game_event_sequence: sequence,
            ..Default::default()
        },
    );
    w
}

fn session(sequence: u32) -> SessionData {
    SessionData {
        game_event_sequence: sequence,
        ..Default::default()
    }
}

fn args(case: &vectors::Case) -> &Vec<Value> {
    case.input["args"].as_array().expect("args")
}
fn s(v: &Value) -> &str {
    v.as_str().expect("string")
}
fn os(v: &Value) -> Option<&str> {
    v.as_str()
}
fn u(v: &Value) -> u32 {
    u32::try_from(u64_of(v).expect("uint")).expect("u32")
}
fn i(v: &Value) -> i32 {
    i32::try_from(i64_of(v).expect("int")).expect("i32")
}
fn f(v: &Value) -> f32 {
    f32_of(v).expect("float")
}
fn b(v: &Value) -> bool {
    v.as_bool().expect("bool")
}

const SEQUENCE_TYPES: [SequenceType; 22] = [
    SequenceType::ObjectPosition,
    SequenceType::ObjectMovement,
    SequenceType::ObjectState,
    SequenceType::ObjectVector,
    SequenceType::ObjectTeleport,
    SequenceType::ObjectServerControl,
    SequenceType::ObjectForcePosition,
    SequenceType::ObjectVisualDesc,
    SequenceType::ObjectInstance,
    SequenceType::Motion,
    SequenceType::UpdatePropertyInt,
    SequenceType::UpdatePropertyInt64,
    SequenceType::UpdatePropertyBool,
    SequenceType::UpdatePropertyDouble,
    SequenceType::UpdatePropertyDataID,
    SequenceType::UpdatePropertyInstanceID,
    SequenceType::UpdatePropertyString,
    SequenceType::UpdateRestrictionDB,
    SequenceType::UpdateAttribute,
    SequenceType::UpdateAttribute2ndLevel,
    SequenceType::UpdatePosition,
    SequenceType::UpdateSkill,
];

fn advance(seq: &mut SequenceManager, ty: i32, property: i32, n: i32) {
    for _ in 0..n {
        let _ = seq.get_next_sequence_of(
            SEQUENCE_TYPES[usize::try_from(ty).unwrap()],
            u32::try_from(property).unwrap(),
        );
    }
}

fn character_error(v: u32) -> CharacterError {
    match v {
        0x1 => CharacterError::Logon,
        0x6 => CharacterError::Delete,
        0x17 => CharacterError::EnterGameCharacterLocked,
        _ => panic!("no CharacterError {v:#x} in the vectors"),
    }
}

fn assert_message(case: &vectors::Case, m: &GameMessage) {
    let out = &case.output;
    let ace = s(&out["hex"]);
    let class = case.input["class"].as_str().unwrap_or("");
    let bytes: Vec<u8> = (0..ace.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&ace[i..i + 2], 16).unwrap())
        .collect();
    let want = crate::proto_identity::retail_ruled(class, &bytes)
        .map_or_else(|| ace.to_owned(), |v| hex(&v));
    assert_eq!(hex(&m.data), want, "{}: bytes", case.input);
    assert_eq!(
        m.group as i64,
        i64_of(&out["group"]).unwrap(),
        "{}: group",
        case.input
    );
    assert_eq!(
        i64::from(m.opcode.0),
        i64_of(&out["opcode"]).unwrap(),
        "{}: opcode",
        case.input
    );
}

// ---- ACE vectors: plain messages -------------------------------------------------------------

fn build_message(class: &str, a: &[Value]) -> GameMessage {
    match class {
        "GameMessageAdminEnvirons" => game_message_admin_environs::game_message_admin_environs(EnvironChangeType(i(&a[0]))),
        "GameMessageBootAccount" => game_message_boot_account::game_message_boot_account(os(&a[0])),
        "GameMessageCharacterCreateResponse" => {
            game_message_character_create_response::game_message_character_create_response(empyrean_net::enums::CharacterGenerationVerificationResponse(u(&a[0])), g(u(&a[1])), s(&a[2]))
        }
        "GameMessageCharacterDelete" => game_message_character_delete::game_message_character_delete(),
        "GameMessageCharacterEnterWorldServerReady" => {
            game_message_character_enter_world_server_ready::game_message_character_enter_world_server_ready()
        }
        "GameMessageCharacterError" => game_message_character_error::game_message_character_error(character_error(u(&a[0]))),
        "GameMessageCharacterLogOff" => game_message_character_log_off::game_message_character_log_off(),
        "GameMessageCharacterRestore" => game_message_character_restore::game_message_character_restore(u(&a[0]), s(&a[1]), u(&a[2])),
        "GameMessageDDDEndDDD" => game_message_ddd_end_ddd::game_message_ddd_end_ddd(),
        "GameMessageDDDErrorMessage" => game_message_ddd_error_message::game_message_ddd_error_message(u(&a[0]), u(&a[1]), u(&a[2])),
        "GameMessageEmoteText" => game_message_emote_text::game_message_emote_text(u(&a[0]), s(&a[1]), s(&a[2])),
        "GameMessageHearRangedSpeech" => game_message_hear_ranged_speech::game_message_hear_ranged_speech(
            s(&a[0]),
            s(&a[1]),
            u(&a[2]),
            f(&a[3]),
            ChatMessageType(u(&a[4])),
        ),
        "GameMessageHearSpeech" => {
            game_message_hear_speech::game_message_hear_speech(s(&a[0]), s(&a[1]), u(&a[2]), ChatMessageType(u(&a[3])))
        }
        "GameMessagePlayerCreate" => game_message_player_create::game_message_player_create(g(u(&a[0]))),
        "GameMessagePlayerKilled" => game_message_player_killed::game_message_player_killed(s(&a[0]), g(u(&a[1])), g(u(&a[2]))),
        "GameMessageScript" => game_message_script::game_message_script(g(u(&a[0])), PlayScript(u(&a[1])), f(&a[2])),
        "GameMessageServerName" => game_message_server_name::game_message_server_name(s(&a[0]), i(&a[1]), i(&a[2])),
        "GameMessageSoulEmote" => game_message_soul_emote::game_message_soul_emote(u(&a[0]), s(&a[1]), s(&a[2])),
        "GameMessageSound" => game_message_sound::game_message_sound(g(u(&a[0])), Sound(u(&a[1])), f(&a[2])),
        "GameMessageSystemChat" => game_message_system_chat::game_message_system_chat(s(&a[0]), ChatMessageType(u(&a[1]))),
        "GameMessageTurbineChat" => game_message_turbine_chat::game_message_turbine_chat(
            ChatNetworkBlobType(i(&a[0])),
            ChatNetworkBlobDispatchType(i(&a[1])),
            u(&a[2]),
            s(&a[3]),
            s(&a[4]),
            u(&a[5]),
            ChatType(i(&a[6])),
        ),
        "GameMessageDDDBeginDDD" => {
            // The dictionary MessageVectors.cs builds, in its insertion order.
            let mut d: HashMap<DatDatabaseType, DotNetDict<u32, Vec<u32>>> = HashMap::new();
            let mut cell = DotNetDict::new();
            cell.add(982, vec![0x01D9_0108, 0x01D9_0109]);
            let mut portal = DotNetDict::new();
            portal.add(2072, vec![0x0E00_000E]);
            portal.add(2073, vec![]);
            let mut high_res = DotNetDict::new();
            high_res.add(5, vec![7, 8, 9]);
            let mut language = DotNetDict::new();
            language.add(3, vec![0x3100_0001]);
            d.insert(DatDatabaseType::Cell, cell);
            d.insert(DatDatabaseType::Portal, portal);
            d.insert(DatDatabaseType::HighRes, high_res);
            d.insert(DatDatabaseType::Language, language);
            game_message_ddd_begin_ddd::game_message_ddd_begin_ddd(u(&a[0]), u(&a[1]), &d)
        }
        other => panic!("no builder mapped for {other}"),
    }
}

#[test]
fn game_messages_match_aces_constructors() {
    let file = vectors::load_named("messages", "game_messages");
    assert!(file.cases.len() > 60, "vector file too small");
    for case in &file.cases {
        let class = s(&case.input["class"]);
        let m = build_message(class, args(case));
        assert_message(case, &m);
    }
}

// ---- ACE vectors: messages over a world object's guid and sequences --------------------------

fn build_world_object_message(class: &str, o: &mut Obj, a: &[Value]) -> GameMessage {
    use game_message_private_update_data_id as pdid;
    match class {
        "GameMessageDeleteObject" => game_message_delete_object::game_message_delete_object(o),
        "GameMessagePickupEvent" => game_message_pickup_event::game_message_pickup_event(o),
        "GameMessageSetState" => game_message_set_state::game_message_set_state(o, PhysicsState(i(&a[0]))),
        "GameMessageVectorUpdate" => game_message_vector_update::game_message_vector_update_of(o, empyrean_entity::Vector3::ZERO, empyrean_entity::Vector3::ZERO),
        "GameMessageInventoryRemoveObject" => game_message_inventory_remove_object::game_message_inventory_remove_object(&o.wo),
        "GameMessagePrivateUpdatePropertyInt" => game_message_private_update_property_int::game_message_private_update_property_int(
            o,
            PropertyInt(u16::try_from(i(&a[0])).unwrap()),
            i(&a[1]),
        ),
        "GameMessagePublicUpdatePropertyInt" => game_message_public_update_property_int::game_message_public_update_property_int(
            o,
            PropertyInt(u16::try_from(i(&a[0])).unwrap()),
            i(&a[1]),
        ),
        "GameMessagePrivateUpdatePropertyInt64" => {
            game_message_private_update_property_int64::game_message_private_update_property_int64(
                o,
                PropertyInt64(u16::try_from(i(&a[0])).unwrap()),
                i64_of(&a[1]).unwrap(),
            )
        }
        "GameMessagePublicUpdatePropertyInt64" => game_message_public_update_property_int64::game_message_public_update_property_int64(
            o,
            PropertyInt64(u16::try_from(i(&a[0])).unwrap()),
            i64_of(&a[1]).unwrap(),
        ),
        "GameMessagePrivateUpdatePropertyBool" => game_message_private_update_property_bool::game_message_private_update_property_bool(
            o,
            PropertyBool(u16::try_from(i(&a[0])).unwrap()),
            b(&a[1]),
        ),
        "GameMessagePublicUpdatePropertyBool" => game_message_public_update_property_bool::game_message_public_update_property_bool(
            o,
            PropertyBool(u16::try_from(i(&a[0])).unwrap()),
            b(&a[1]),
        ),
        "GameMessagePrivateUpdatePropertyFloat" => {
            game_message_private_update_property_float::game_message_private_update_property_float(
                o,
                PropertyFloat(u16::try_from(i(&a[0])).unwrap()),
                f64_of(&a[1]).unwrap(),
            )
        }
        "GameMessagePublicUpdatePropertyFloat" => game_message_public_update_property_float::game_message_public_update_property_float(
            o,
            PropertyFloat(u16::try_from(i(&a[0])).unwrap()),
            f64_of(&a[1]).unwrap(),
        ),
        "GameMessagePrivateUpdatePropertyString" => {
            game_message_private_update_property_string::game_message_private_update_property_string(
                o,
                PropertyString(u16::try_from(i(&a[0])).unwrap()),
                os(&a[1]),
            )
        }
        "GameMessagePublicUpdatePropertyString" => {
            game_message_public_update_property_string::game_message_public_update_property_string(
                o,
                PropertyString(u16::try_from(i(&a[0])).unwrap()),
                os(&a[1]),
            )
        }
        "GameMessagePrivateUpdateDataID" => {
            pdid::game_message_private_update_data_id(o, PropertyDataId(u16::try_from(i(&a[0])).unwrap()), u(&a[1]))
        }
        "GameMessagePublicUpdatePropertyDataID" => game_message_public_update_data_id::game_message_public_update_data_id(
            o,
            PropertyDataId(u16::try_from(i(&a[0])).unwrap()),
            u(&a[1]),
        ),
        "GameMessagePrivateUpdateInstanceID" => game_message_private_update_instance_id::game_message_private_update_instance_id(
            o,
            PropertyInstanceId(u16::try_from(i(&a[0])).unwrap()),
            u(&a[1]),
        ),
        "GameMessagePublicUpdateInstanceID" => game_message_public_update_instance_id::game_message_public_update_instance_id(
            o,
            PropertyInstanceId(u16::try_from(i(&a[0])).unwrap()),
            g(u(&a[1])),
        ),
        "GameMessagePrivateUpdateAttribute2ndLevel" => {
            game_message_private_update_attribute2nd_level::game_message_private_update_attribute2nd_level(
                o,
                Vital(u(&a[0])),
                u(&a[1]),
            )
        }
        "GameMessagePublicUpdateVital" => game_message_public_update_vital::game_message_public_update_vital(
            o,
            PropertyAttribute2nd(u16::try_from(i(&a[0])).unwrap()),
            u(&a[1]),
            u(&a[2]),
            u(&a[3]),
            u(&a[4]),
        ),
        "GameMessagePrivateUpdatePosition" => game_message_private_update_position::game_message_private_update_position(
            o,
            PositionType(u16::try_from(i(&a[0])).unwrap()),
            &vector_position(),
        ),
        "GameMessagePublicUpdatePosition" => game_message_public_update_position::game_message_public_update_position(
            o,
            PositionType(u16::try_from(i(&a[0])).unwrap()),
            &vector_position(),
        ),
        other => panic!("no builder mapped for {other}"),
    }
}

/// `new Position(0xA9B40019, 84.0f, 7.1f, 94.005f, 0f, 0f, -0.0795f, 0.9968f)`, as in
/// MessageVectors.cs.
fn vector_position() -> Position {
    Position::from_components(
        0xA9B4_0019,
        84.0,
        7.1,
        94.005,
        0.0,
        0.0,
        -0.0795,
        0.9968,
        false,
    )
}

#[test]
fn world_object_messages_match_aces_constructors_and_consume_the_same_sequences() {
    let file = vectors::load_named("messages", "world_object_messages");
    assert!(file.cases.len() > 40, "vector file too small");
    for case in &file.cases {
        let class = s(&case.input["class"]);
        let a = args(case);
        if class == "GameMessageParentEvent" {
            let adv = case.input["advance"].as_array().unwrap();
            let mut creature = obj(0x5000_0010);
            let mut item = obj(0x8000_0011);
            advance(&mut creature.seq, i(&adv[0]), i(&adv[1]), i(&adv[2]));
            advance(&mut item.seq, i(&adv[4]), i(&adv[5]), i(&adv[6]));
            let m = game_message_parent_event::game_message_parent_event(
                &mut creature,
                &mut item,
                Some(ParentLocation(i(&a[0]))),
                Some(Placement(u(&a[1]))),
            );
            assert_message(case, &m);
            continue;
        }
        let mut o = obj(u(&case.input["guid"]));
        if let Some(adv) = case.input["advance"].as_array() {
            advance(&mut o.seq, i(&adv[0]), i(&adv[1]), i(&adv[2]));
        }
        let m = build_world_object_message(class, &mut o, a);
        assert_message(case, &m);
        let after = &case.output["after"];
        for (key, ty) in [
            ("position", SequenceType::ObjectPosition),
            ("state", SequenceType::ObjectState),
            ("teleport", SequenceType::ObjectTeleport),
            ("vector", SequenceType::ObjectVector),
        ] {
            assert_eq!(
                hex(&o.seq.get_current_sequence(ty)),
                s(&after[key]),
                "{}: {key} after",
                case.input
            );
        }
    }
}

// ---- ACE vectors: game events ---------------------------------------------------------------

fn build_event(class: &str, ses: &mut SessionData, a: &[Value]) -> GameMessage {
    match class {
        "GameEventAcceptTrade" => game_event_accept_trade::game_event_accept_trade(ses, g(u(&a[0]))),
        "GameEventAddToTrade" => game_event_add_to_trade::game_event_add_to_trade(ses, u(&a[0]), TradeSide(i(&a[1]))),
        "GameEventAllegianceAllegianceUpdateDone" => {
            game_event_allegiance_allegiance_update_done::game_event_allegiance_allegiance_update_done(ses, WeenieError(i(&a[0])))
        }
        "GameEventAllegianceLoginNotification" => {
            game_event_allegiance_login_notification::game_event_allegiance_login_notification(ses, u(&a[0]), b(&a[1]))
        }
        "GameEventAttackDone" => game_event_attack_done::game_event_attack_done(ses, WeenieError(i(&a[0]))),
        "GameEventAttackerNotification" => game_event_attacker_notification::game_event_attacker_notification(
            ses,
            s(&a[0]),
            DamageType(i(&a[1])),
            f(&a[2]),
            u(&a[3]),
            b(&a[4]),
            AttackConditions(i(&a[5])),
        ),
        "GameEventBookAddPageResponse" => {
            game_event_book_add_page_response::game_event_book_add_page_response(ses, u(&a[0]), i(&a[1]), b(&a[2]))
        }
        "GameEventBookDeletePageResponse" => {
            game_event_book_delete_page_response::game_event_book_delete_page_response(ses, u(&a[0]), i(&a[1]), b(&a[2]))
        }
        "GameEventBookModifyPageResponse" => {
            game_event_book_modify_page_response::game_event_book_modify_page_response(ses, u(&a[0]), i(&a[1]), b(&a[2]))
        }
        "GameEventChannelBroadcast" => {
            game_event_channel_broadcast::game_event_channel_broadcast(ses, Channel(i(&a[0])), s(&a[1]), s(&a[2]))
        }
        "GameEventClearTradeAcceptance" => game_event_clear_trade_acceptance::game_event_clear_trade_acceptance(ses),
        "GameEventCloseGroundContainer" => game_event_close_ground_container::game_event_close_ground_container(ses, g(u(&a[0]))),
        "GameEventCloseTrade" => game_event_close_trade::game_event_close_trade(ses, EndTradeReason(i(&a[0]))),
        "GameEventCombatCommenceAttack" => game_event_combat_commence_attack::game_event_combat_commence_attack(ses),
        "GameEventCommunicationTransientString" => {
            game_event_communication_transient_string::game_event_communication_transient_string(ses, s(&a[0]))
        }
        "GameEventConfirmationDone" => {
            game_event_confirmation_done::game_event_confirmation_done(ses, ConfirmationType(u(&a[0])), u(&a[1]))
        }
        "GameEventConfirmationRequest" => {
            game_event_confirmation_request::game_event_confirmation_request(ses, ConfirmationType(u(&a[0])), u(&a[1]), s(&a[2]))
        }
        "GameEventDeclineTrade" => game_event_decline_trade::game_event_decline_trade(ses, g(u(&a[0]))),
        "GameEventDefenderNotification" => game_event_defender_notification::game_event_defender_notification(
            ses,
            s(&a[0]),
            DamageType(i(&a[1])),
            f(&a[2]),
            u(&a[3]),
            empyrean_net::enums::DamageLocation(u(&a[4])),
            b(&a[5]),
            AttackConditions(i(&a[6])),
        ),
        "GameEventEvasionAttackerNotification" => {
            game_event_evasion_attacker_notification::game_event_evasion_attacker_notification(ses, s(&a[0]))
        }
        "GameEventEvasionDefenderNotification" => {
            game_event_evasion_defender_notification::game_event_evasion_defender_notification(ses, s(&a[0]))
        }
        "GameEventFellowshipDisband" => game_event_fellowship_disband::game_event_fellowship_disband(ses),
        "GameEventFellowshipDismiss" => game_event_fellowship_dismiss::game_event_fellowship_dismiss(ses, g(u(&a[0]))),
        "GameEventFellowshipFellowUpdateDone" => {
            game_event_fellowship_fellow_update_done::game_event_fellowship_fellow_update_done(ses, WeenieError(i(&a[0])))
        }
        "GameEventFellowshipQuit" => game_event_fellowship_quit::game_event_fellowship_quit(ses, u(&a[0])),
        "GameEventGameOver" => game_event_game_over::game_event_game_over(ses, g(u(&a[0])), i(&a[1])),
        "GameEventHouseStatus" => game_event_house_status::game_event_house_status(ses, WeenieError(i(&a[0]))),
        "GameEventHouseTransaction" => game_event_house_transaction::game_event_house_transaction(ses),
        "GameEventHouseUpdateRentTime" => game_event_house_update_rent_time::game_event_house_update_rent_time(ses),
        "GameEventInventoryServerSaveFailed" => {
            game_event_inventory_server_save_failed::game_event_inventory_server_save_failed(ses, u(&a[0]), WeenieError(i(&a[1])))
        }
        "GameEventItemServerSaysMoveItem" => game_event_item_server_says_move_item::game_event_item_server_says_move_item(ses, g(u(&a[0]))),
        "GameEventJoinGameResponse" => game_event_join_game_response::game_event_join_game_response(ses, g(u(&a[0])), ChessColor(i(&a[1]))),
        "GameEventKillerNotification" => game_event_killer_notification::game_event_killer_notification(ses, s(&a[0])),
        "GameEventMagicDispelEnchantment" => {
            game_event_magic_dispel_enchantment::game_event_magic_dispel_enchantment(ses, u16v(&a[0]), u16v(&a[1]))
        }
        "GameEventMagicRemoveEnchantment" => {
            game_event_magic_remove_enchantment::game_event_magic_remove_enchantment(ses, u16v(&a[0]), u16v(&a[1]))
        }
        "GameEventMagicRemoveSpell" => game_event_magic_remove_spell::game_event_magic_remove_spell(ses, u16v(&a[0]), u16v(&a[1])),
        "GameEventMagicUpdateSpell" => game_event_magic_update_spell::game_event_magic_update_spell(ses, u16v(&a[0]), u16v(&a[1])),
        "GameEventMagicPurgeBadEnchantments" => game_event_magic_purge_bad_enchantments::game_event_magic_purge_bad_enchantments(ses),
        "GameEventMagicPurgeEnchantments" => game_event_magic_purge_enchantments::game_event_magic_purge_enchantments(ses),
        "GameEventMoveResponse" => game_event_move_response::game_event_move_response(ses, g(u(&a[0])), ChessMoveResult(i(&a[1]))),
        "GameEventOpponentStalemate" => {
            game_event_opponent_stalemate::game_event_opponent_stalemate(ses, g(u(&a[0])), ChessColor(i(&a[1])), b(&a[2]))
        }
        "GameEventPingResponse" => game_event_ping_response::game_event_ping_response(ses),
        "GameEventPopupString" => game_event_popup_string::game_event_popup_string(ses, s(&a[0])),
        "GameEventPortalStorm" => game_event_portal_storm::game_event_portal_storm(ses),
        "GameEventPortalStormBrewing" => game_event_portal_storm_brewing::game_event_portal_storm_brewing(ses, f(&a[0])),
        "GameEventPortalStormImminent" => game_event_portal_storm_imminent::game_event_portal_storm_imminent(ses, f(&a[0])),
        "GameEventPortalStormSubsided" => game_event_portal_storm_subsided::game_event_portal_storm_subsided(ses),
        "GameEventQueryAgeResponse" => game_event_query_age_response::game_event_query_age_response(ses, s(&a[0]), s(&a[1])),
        "GameEventQueryItemManaResponse" => {
            game_event_query_item_mana_response::game_event_query_item_mana_response(ses, u(&a[0]), f(&a[1]), u(&a[2]))
        }
        "GameEventRegisterTrade" => game_event_register_trade::game_event_register_trade(ses, g(u(&a[0])), g(u(&a[1]))),
        "GameEventResetTrade" => game_event_reset_trade::game_event_reset_trade(ses, g(u(&a[0]))),
        "GameEventSetTurbineChatChannels" => {
            game_event_set_turbine_chat_channels::game_event_set_turbine_chat_channels(ses, u(&a[0]), u(&a[1]))
        }
        "GameEventStartGame" => game_event_start_game::game_event_start_game(ses, g(u(&a[0])), ChessColor(i(&a[1]))),
        "GameEventTell" => game_event_tell::game_event_tell_from(ses, s(&a[0]), s(&a[1]), u(&a[2]), u(&a[3]), ChatMessageType(u(&a[4]))),
        "GameEventTradeFailure" => game_event_trade_failure::game_event_trade_failure(ses, u(&a[0]), WeenieError(i(&a[1]))),
        "GameEventUpdateHealth" => game_event_update_health::game_event_update_health(ses, u(&a[0]), f(&a[1])),
        "GameEventUpdateTitle" => game_event_update_title::game_event_update_title(ses, u(&a[0]), b(&a[1])),
        "GameEventUseDone" => game_event_use_done::game_event_use_done(ses, WeenieError(i(&a[0]))),
        "GameEventVictimNotification" => game_event_victim_notification::game_event_victim_notification(ses, s(&a[0])),
        "GameEventWeenieError" => game_event_weenie_error::game_event_weenie_error(ses, WeenieError(i(&a[0]))),
        "GameEventWeenieErrorWithString" => {
            game_event_weenie_error_with_string::game_event_weenie_error_with_string(ses, WeenieErrorWithString(i(&a[0])), s(&a[1]))
        }
        "GameEventWieldItem" => game_event_wield_item::game_event_wield_item(ses, u(&a[0]), EquipMask(u(&a[1]))),
        other => panic!("no builder mapped for {other}"),
    }
}

fn u16v(v: &Value) -> u16 {
    u16::try_from(u(v)).unwrap()
}

/// The ported builder's message for one case of the `game_messages`, `game_events` or
/// `world_object_messages` vector file, set up exactly as the three vector tests above set it up
/// (for `proto_identity`, which checks the same cases against dereth-protocol).
pub(crate) fn build_vector_case(file: &str, case: &vectors::Case) -> GameMessage {
    let class = s(&case.input["class"]);
    let a = args(case);
    match file {
        "game_messages" => build_message(class, a),
        "game_events" => build_event(class, &mut session(u(&case.input["seq"])), a),
        "world_object_messages" if class == "GameMessageParentEvent" => {
            let adv = case.input["advance"].as_array().unwrap();
            let mut creature = obj(0x5000_0010);
            let mut item = obj(0x8000_0011);
            advance(&mut creature.seq, i(&adv[0]), i(&adv[1]), i(&adv[2]));
            advance(&mut item.seq, i(&adv[4]), i(&adv[5]), i(&adv[6]));
            game_message_parent_event::game_message_parent_event(
                &mut creature,
                &mut item,
                Some(ParentLocation(i(&a[0]))),
                Some(Placement(u(&a[1]))),
            )
        }
        "world_object_messages" => {
            let mut o = obj(u(&case.input["guid"]));
            if let Some(adv) = case.input["advance"].as_array() {
                advance(&mut o.seq, i(&adv[0]), i(&adv[1]), i(&adv[2]));
            }
            build_world_object_message(class, &mut o, a)
        }
        other => panic!("no vector file {other}"),
    }
}

#[test]
fn game_events_match_aces_constructors_and_consume_one_sequence_each() {
    let file = vectors::load_named("messages", "game_events");
    assert!(file.cases.len() > 80, "vector file too small");
    for case in &file.cases {
        let class = s(&case.input["class"]);
        let mut ses = session(u(&case.input["seq"]));
        let m = build_event(class, &mut ses, args(case));
        assert_message(case, &m);
        assert_eq!(
            i64::from(ses.game_event_sequence),
            i64_of(&case.output["seq_after"]).unwrap(),
            "{}",
            case.input
        );
    }
}

#[test]
fn interleaved_events_number_in_construction_order_and_wrap() {
    let file = vectors::load_named("messages", "game_event_sequence");
    for case in &file.cases {
        let mut ses = session(u(&case.input["seq"]));
        let classes = case.input["events"].as_array().unwrap();
        let expected = case.output["hex"].as_array().unwrap();
        for (class, want) in classes.iter().zip(expected) {
            let m = match s(class) {
                "GameEventUseDone" => build_event("GameEventUseDone", &mut ses, &[Value::from(0)]),
                other => build_event(other, &mut ses, &[]),
            };
            assert_eq!(hex(&m.data), s(want), "{}", case.input);
        }
        assert_eq!(
            i64::from(ses.game_event_sequence),
            i64_of(&case.output["seq_after"]).unwrap()
        );
    }
}

#[test]
fn an_event_carries_the_session_players_guid() {
    let mut ses = SessionData {
        player: Some(g(0x5000_0ABC)),
        game_event_sequence: 7,
        ..Default::default()
    };
    let m = game_event_message(
        GameEventType::PingResponse,
        GameMessageGroup::UIQueue,
        &mut ses,
    );
    assert_eq!(hex(&m.data), "B0F70000BC0A005007000000EA010000");
    assert_eq!(ses.game_event_sequence, 8);
}

// ---- hand-derived goldens (constructors the harness cannot run) -------------------------------

#[test]
fn set_stack_size_reads_stack_size_and_value_and_shares_the_stack_size_int_sequence() {
    let mut o = obj(0x8000_1234);
    o.wo.set_stack_size_prop(Some(25));
    o.wo.set_value(Some(1000));
    let m = game_message_set_stack_size::game_message_set_stack_size(&mut o);
    // opcode 0x0197, the first NextValue of an unprimed byte sequence (0), guid, (uint)25, (uint)1000
    assert_eq!(
        hex(&m.data),
        "9701000000341200801900000".to_owned() + "0E8030000"
    );
    // The next PublicUpdatePropertyInt(StackSize) continues the same sequence.
    let n = game_message_public_update_property_int::game_message_public_update_property_int(
        &mut o,
        PropertyInt::StackSize,
        3,
    );
    assert_eq!(&hex(&n.data)[8..10], "01");
    // Absent properties are written as 0.
    let mut empty = obj(0x8000_0001);
    let m = game_message_set_stack_size::game_message_set_stack_size(&mut empty);
    assert_eq!(hex(&m.data), "9701000000010000800000000000000000");
}

#[test]
fn autonomous_position_is_empty_unless_the_object_is_a_player() {
    let mut o = obj(0x8000_0002);
    let m = game_message_autonomous_position::game_message_autonomous_position(&mut o);
    assert_eq!(hex(&m.data), "53F70000");
    assert_eq!(m.group, GameMessageGroup::SecureWeenieQueue);

    let mut p = Obj {
        wo: player_obj(0x5000_0003),
        seq: SequenceManager::new(),
    };
    p.wo.set_location(Some(Position::from_components(
        0x7D64_0024,
        1.0,
        2.0,
        3.0,
        0.0,
        0.0,
        0.0,
        1.0,
        false,
    )));
    let _ = p.seq.get_next_sequence(SequenceType::ObjectInstance);
    let m = game_message_autonomous_position::game_message_autonomous_position(&mut p);
    let mut want = String::from("53F7000003000050");
    // Position.Serialize(writer, writeQuaternion: true, writeLandblock: false): x y z w x y z
    for v in [1.0f32, 2.0, 3.0, 1.0, 0.0, 0.0, 0.0] {
        want += &hex(&v.to_le_bytes());
    }
    want += "0100" /* instance */;
    want += "0000" /* server control */;
    want += "0000" /* teleport */;
    want += "0000" /* force position */;
    want += "01000000" /* contact */;
    assert_eq!(hex(&m.data), want);
}

#[test]
fn private_update_attribute_skill_and_vital_follow_aces_field_order() {
    use game_message_private_update_attribute::game_message_private_update_attribute;
    use game_message_private_update_skill::game_message_private_update_skill;
    use game_message_private_update_vital::game_message_private_update_vital;
    let mut o = stat_obj(0x5000_0004);
    let m = game_message_private_update_attribute(&mut o, strength());
    assert_eq!(hex(&m.data), "E302000000010000000A00000032000000E7030000");

    let m = game_message_private_update_skill(&mut o, CreatureSkill::new(Skill(6)));
    // seq, (uint)skill, (ushort)ranks, (ushort)adjustPP = 1, (uint)advancement class, xp, init, resistance, (double)last used
    assert_eq!(
        hex(&m.data),
        "DD02000000060000000700010002000000640000000A00000003000000000000000000F83F"
    );

    let health = max_health(&mut o);
    let m = game_message_private_update_vital(&mut o, health);
    assert_eq!(
        hex(&m.data),
        "E70200000001000000010000000200000003000000".to_owned() + "04000000"
    );
    // Vital and Attribute2ndLevel share the UpdateAttribute2ndLevel sequence of the same key.
    let n = game_message_private_update_attribute2nd_level::game_message_private_update_attribute2nd_level(&mut o, Vital::MaxHealth, 9);
    assert_eq!(&hex(&n.data)[8..10], "01");
}

#[test]
fn character_list_serializes_deletion_times_and_server_options() {
    let w = world();
    let ses = SessionData {
        account: Some("acct".into()),
        ..Default::default()
    };
    let characters = vec![
        CharacterSummary {
            id: 0x5000_0001,
            name: "Aa".into(),
            is_deleted: false,
            delete_time: 0,
            ..Default::default()
        },
        CharacterSummary {
            id: 0x5000_0002,
            name: "Bbb".into(),
            is_deleted: false,
            delete_time: 1_003_600,
            ..Default::default()
        },
    ];
    let _ = take_local();
    let m = game_message_character_list::game_message_character_list(&w, &characters, &ses);
    let mut want = String::from("58F60000" /* opcode */);
    want += "00000000" /* 0u */;
    want += "02000000" /* count */;
    want += "01000050" /* id */;
    want += "02004161" /* "Aa": 2 + 2 = 4, no pad */;
    want += "00000000" /* not pending deletion */;
    want += "02000050";
    want += "0300426262" /* "Bbb" */;
    want += "000000" /* 2 + 3 = 5, pad 3 */;
    // V444: the seconds left, 1_003_600 - 1_000_000
    want += &hex(&3600_u32.to_le_bytes());
    want += "00000000" /* 0u */;
    want += "0B000000" /* slot count: max_chars_per_account, ACE's default 11 */;
    want += "0400616363740000" /* "acct" */;
    want += "01000000" /* use_turbine_chat: ACE's default true */;
    want += "01000000" /* hasThroneOfDestiny */;
    assert_eq!(hex(&m.data), want);
    assert!(take_local().is_empty(), "No unimplemented member is called");

    // OverrideCharacterPermissions (the accounts configuration, ACE's default true) and an access
    // level above Advocate: every name gets the `+`
    let admin = SessionData {
        account: Some("acct".into()),
        access_level: empyrean_entity::enums::AccessLevel::Sentinel,
        ..Default::default()
    };
    let m = game_message_character_list::game_message_character_list(&w, &characters, &admin);
    assert_eq!(
        &hex(&m.data)[32..48],
        "03002B4161000000",
        "+Aa, padded to 4"
    );
}

/// A character pending deletion is listed in `Login_LoginCharacterSet` (`0xF658`) with the
/// seconds left until it is deleted, at least 1 while the deletion is pending; a character not
/// pending deletion is listed with 0.
/// Divergence: V444
#[test]
fn character_list_sends_the_seconds_left_until_a_pending_deletion() {
    let w = world(); // unix time 1_000_000
    let ses = SessionData {
        account: Some("acct".into()),
        ..Default::default()
    };
    let listed = |delete_time: u64| -> u32 {
        let characters = vec![CharacterSummary {
            id: 0x5000_0001,
            name: "Aa".into(),
            delete_time,
            ..Default::default()
        }];
        let m = game_message_character_list::game_message_character_list(&w, &characters, &ses);
        let d = hex(&m.data);
        // opcode, 0u, count, id, "Aa" (4 bytes): the seconds follow at byte 20.
        u32::from_str_radix(&d[40..48], 16).unwrap().swap_bytes()
    };
    assert_eq!(listed(0), 0, "not pending deletion");
    assert_eq!(listed(1_003_600), 3600, "an hour left");
    assert_eq!(listed(1_000_090), 90, "a minute and a half left");
    assert_eq!(listed(1_000_000), 1, "due now, not yet purged");
    assert_eq!(listed(999_000), 1, "past due, not yet purged");
}

/// `Login_LoginCharacterSet` (`0xF658`) ends with the era's Throne of Destiny flag: 1 on the end
/// of retail, 0 on Infiltration (the client then shows at most level 126 and offers only the
/// original heritages).
/// Divergence: V389
#[test]
fn character_list_sends_the_eras_throne_of_destiny_flag() {
    let ses = SessionData {
        account: Some("acct".into()),
        ..Default::default()
    };
    let characters = vec![CharacterSummary {
        id: 0x5000_0001,
        name: "Aa".into(),
        ..Default::default()
    }];
    for (era, flag) in [
        (empyrean_common::era::EraId::Eor, "01000000"),
        (empyrean_common::era::EraId::Infiltration, "00000000"),
    ] {
        let mut w = world();
        w.era = era.rules();
        let _ = take_local();
        let m = game_message_character_list::game_message_character_list(&w, &characters, &ses);
        let data = hex(&m.data);
        assert!(data.starts_with("58F60000"), "{data}");
        assert_eq!(&data[data.len() - 8..], flag, "{era}");
        assert_eq!(
            &data[..data.len() - 8],
            {
                let mut eor = world();
                eor.era = empyrean_common::era::EraId::Eor.rules();
                let d = hex(&game_message_character_list::game_message_character_list(
                    &eor,
                    &characters,
                    &ses,
                )
                .data);
                d[..d.len() - 8].to_owned()
            },
            "only the flag differs"
        );
    }
}

#[test]
fn ddd_interrogation_and_data_messages_use_the_dat_headers() {
    let _ = take_local();
    let m = game_message_ddd_interrogation::game_message_ddd_interrogation(&world());
    assert_eq!(
        hex(&m.data),
        "E5F70000010000000100000001000000020000000000000001000000"
    );
    // A file the (empty) dat does not have: ACE's `null` branch leaves the bare opcode.
    let m = game_message_ddd_data_message::game_message_ddd_data_message(
        &mut world(),
        0x0600_0001,
        DatDatabaseType::Portal,
    );
    assert_eq!(hex(&m.data), "E2F70000");
}

#[test]
fn account_banned_counts_seconds_from_the_tick_clock() {
    let w = world();
    let expiry = w.now.utc + TimeSpan::from_seconds(3600.9);
    let m = game_message_account_banned::game_message_account_banned(&w, expiry, Some("x"));
    // (uint)3600.9 = 3600, then "x" (1 + 2 = 3, pad 1)
    assert_eq!(hex(&m.data), "C1F70000100E0000010078".to_owned() + "00");
    // V240: no reason is an empty string (length 0, then padding), never an absent one.
    let m = game_message_account_banned::game_message_account_banned(&w, expiry, None);
    assert_eq!(hex(&m.data), "C1F70000100E000000000000");
    let d = dereth_protocol::read_body::<dereth_protocol::login::LoginAccountBanned>(&m.data[4..])
        .expect("the client reads it");
    assert_eq!((d.expiry, d.reason.as_str()), (3600, ""));
}

#[test]
fn string_properties_align_before_the_string() {
    let mut o = obj(0x5000_0005);
    let m =
        game_message_private_update_property_string::game_message_private_update_property_string(
            &mut o,
            PropertyString::Name,
            Some("ab"),
        );
    // opcode, seq (1 byte), property -> 9 bytes, Align to 12, then String16L "ab" (2 + 2 = 4, no pad)
    assert_eq!(
        hex(&m.data),
        "D502000000010000000000000200".to_owned() + "6162"
    );
}

/// V241: a string of 65,535 or more UTF-16 units goes out in the client's long
/// form (0xFFFF, then a dword length), so the client reads it and the field after it. ACE wrote the
/// length's low 16 bits. A character Windows-1252 lacks is `?`, one per UTF-16 unit.
#[test]
fn a_long_string_is_written_in_the_clients_long_form() {
    for n in [65_534usize, 65_535, 70_000] {
        let text: String = "a".repeat(n - 1) + "\u{1F600}";
        let m = game_message_system_chat::game_message_system_chat(&text, ChatMessageType(3));
        let d = dereth_protocol::read_body::<dereth_protocol::comms::CommunicationTextboxString>(
            &m.data[4..],
        )
        .unwrap_or_else(|e| panic!("{n}: the client reads it: {e}"));
        assert_eq!(d.text, "a".repeat(n - 1) + "??", "{n}: text");
        assert_eq!(d.text_type, 3, "{n}: the field after the string");
        let long = n + 1 >= 0xFFFF;
        assert_eq!(m.data[4..6] == [0xFF, 0xFF], long, "{n}: long form");
    }
}

/// A known type data id the retail packer refuses is logged and sent as none.
/// V236, V237.
#[test]
fn a_known_type_data_id_the_retail_packer_refuses_is_logged_and_sent_as_none() {
    use dereth_protocol::types::{AnimPartChange, ObjDesc};
    use empyrean_world::network::game_messages::game_message::{known_type_did, write_record};

    let guid = g(0x8000_0001);
    for (value, base, sent, logged) in [
        (0x0400_1234u32, 0x0400_0000u32, 0x0400_1234u32, false),
        (0x4000_0001, 0x0400_0000, 0x4000_0001, false),
        (0x0400_7FFF, 0x0400_0000, 0x0400_7FFF, false),
        (0, 0x0400_0000, 0, false),
        (0x0600_0001, 0x0400_0000, 0x0600_0001, false),
        (0x0000_0042, 0x0400_0000, 0x0400_0042, false),
        (0x0000_0102, 0x0600_0000, 0x0600_0102, false),
        (0x0100_0010, 0x0500_0000, 0, true),
        (0x0500_4000, 0x0500_0000, 0x0500_4000, false),
        (0x0200_0102, 0x0600_0000, 0, true),
    ] {
        let (got, lines) =
            crate::log_capture::capture(|| known_type_did(value, base, guid, "icon"));
        assert_eq!(got, sent, "0x{value:08X}");
        if !logged {
            assert!(lines.is_empty(), "0x{value:08X}: {lines:?}");
        } else {
            assert_eq!(lines.len(), 1, "0x{value:08X}");
            let line = &lines[0];
            assert!(
                line.starts_with("content error")
                    && line.contains("80000001")
                    && line.contains("icon"),
                "{line}"
            );
            assert!(line.contains(&format!("0x{value:08X}")), "{line}");
        }
    }

    let record = ObjDesc {
        anim_part_changes: vec![
            AnimPartChange {
                part_index: 0,
                part_id: 0x0100_0001
            };
            257
        ],
        ..ObjDesc::default()
    };
    let (bytes, lines) = crate::log_capture::capture(|| {
        let mut bytes = Vec::new();
        write_record(&mut bytes, &[], |w| record.write(w));
        bytes
    });
    assert_eq!(&bytes[..4], &[0x11, 0, 0, 1], "ACE's (byte) count wraps");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with("content error: 1 count(s)"),
        "{}",
        lines[0]
    );
}

/// V236/V237 amended: a clothing sub-palette is held as the low 16 bits of the
/// palette set's id (0x12D7 for 0x040012D7). It is sent as the full id, with no content error, and
/// the client's reader gets the palette ACE's client got.
#[test]
fn a_clothing_sub_palette_held_as_an_offset_reaches_the_client_as_its_palette() {
    use dereth_protocol::types::{ObjDesc, Subpalette};
    use empyrean_world::network::game_messages::game_message::{known_type_did, write_record};

    let guid = g(0x8000_00B4);
    let (record, lines) = crate::log_capture::capture(|| ObjDesc {
        palette_id: known_type_did(0x0400_007E, 0x0400_0000, guid, "palette"),
        subpalettes: [0x12D7u32, 0x12D7, 0x0692]
            .iter()
            .map(|&id| Subpalette {
                sub_id: known_type_did(id, 0x0400_0000, guid, "sub-palette"),
                offset: 0,
                num_colors: 1,
            })
            .collect(),
        ..ObjDesc::default()
    });
    assert!(lines.is_empty(), "{lines:?}");
    let mut bytes = Vec::new();
    write_record(&mut bytes, &[], |w| record.write(w));
    let read = ObjDesc::read(&mut dereth_protocol::Reader::body(&bytes))
        .expect("the client's reader takes it");
    let ids: Vec<u32> = read.subpalettes.iter().map(|p| p.sub_id).collect();
    assert_eq!(ids, [0x0400_12D7, 0x0400_12D7, 0x0400_0692]);
    assert_eq!(read.palette_id, 0x0400_007E);
}

/// V238: exactly one list, the highest role's; an empty list for no role.
#[test]
fn channel_index_writes_exactly_one_list() {
    use dereth_protocol::comms::CommunicationChannelIndexRecv;
    let names = |p: WorldObject| -> Vec<String> {
        let mut w = world_with_player(Some(p), 1);
        let m = game_event_channel_index::game_event_channel_index(&mut w, SESSION);
        dereth_protocol::read_body::<CommunicationChannelIndexRecv>(&m.data[16..])
            .expect("one list, fully read")
            .names
    };
    let mut admin = player_obj(0x5000_0006);
    admin.set_is_admin_prop(true);
    assert_eq!(names(admin).len(), 8);

    // An admin who is also an advocate gets the admin list only (ACE sent both).
    let mut both = player_obj(0x5000_0007);
    both.set_is_admin_prop(true);
    both.set_is_advocate(true);
    assert_eq!(names(both).len(), 8);

    let mut advocate = player_obj(0x5000_0009);
    advocate.set_is_advocate(true);
    assert_eq!(names(advocate), ["Abuse", "Av1", "Av2", "Av3", "Help"]);

    // No role: an empty list (ACE sent no body at all).
    assert!(names(player_obj(0x5000_0008)).is_empty());
}

#[test]
fn book_responses_hide_the_author_account_from_non_admins() {
    let page = PropertiesBookPageData {
        author_id: 0x5000_0009,
        author_name: Some("Au".into()),
        author_account: Some("acc".into()),
        ignore_author: false,
        page_text: Some("T".into()),
    };
    let mut w = world_with_player(Some(player_obj(0x5000_0010)), 5);
    let m = game_event_book_page_data_response::game_event_book_page_data_response(
        &mut w,
        SESSION,
        0x8000_0001,
        2,
        &page,
    );
    // Header: the player's guid, sequence 5, event 0x00B8.
    assert_eq!(
        hex(&m.data[..16]),
        "B0F7000010000050".to_owned() + "05000000B8000000"
    );
    // Body: book, page, author id, "Au", "Password is cheese" (2 + 18 = 20, no pad), flags,
    // text included, ignore author, "T".
    let body = &m.data[16..];
    assert_eq!(&body[0..4], &0x8000_0001u32.to_le_bytes());
    assert_eq!(&body[4..8], &2i32.to_le_bytes());
    assert_eq!(&body[8..12], &0x5000_0009u32.to_le_bytes());
    assert_eq!(&body[12..16], b"\x02\x00Au");
    assert_eq!(&body[16..18], &18u16.to_le_bytes());
    assert_eq!(&body[18..36], b"Password is cheese");
    assert_eq!(&body[36..40], &0xFFFF_0002u32.to_le_bytes());
    assert_eq!(&body[40..44], &1i32.to_le_bytes());
    assert_eq!(&body[44..48], &0i32.to_le_bytes());
    assert_eq!(&body[48..], b"\x01\x00T\x00");
    assert_eq!(w.sessions.get(SESSION).unwrap().game_event_sequence, 6);

    let mut admin = player_obj(0x5000_0011);
    admin.set_is_admin_prop(true);
    let mut w = world_with_player(Some(admin), 1);
    let m = game_event_book_data_response::game_event_book_data_response(
        &mut w,
        SESSION,
        0x8000_0002,
        1000,
        2,
        std::slice::from_ref(&page),
        Some("ins"),
        0xFFFF_FFFF,
        None,
        true,
    );
    let body = &m.data[16..];
    let mut want = Vec::new();
    for v in [0x8000_0002u32, 2, 2, 1000, 1, 0x5000_0009] {
        want.extend_from_slice(&v.to_le_bytes());
    }
    want.extend_from_slice(b"\x02\x00Au");
    want.extend_from_slice(b"\x03\x00acc\x00\x00\x00");
    want.extend_from_slice(&0xFFFF_0002u32.to_le_bytes());
    want.extend_from_slice(&1i32.to_le_bytes());
    want.extend_from_slice(&1i32.to_le_bytes());
    want.extend_from_slice(b"\x01\x00T\x00");
    want.extend_from_slice(b"\x03\x00ins\x00\x00\x00");
    want.extend_from_slice(&0u32.to_le_bytes()); // authorId 0xFFFFFFFF -> 0
    want.extend_from_slice(b"\x00\x00\x00\x00"); // null author name
    assert_eq!(hex(body), hex(&want));
}

#[test]
fn inscription_response_and_contain_id_read_the_objects_properties() {
    let mut w = world_with_player(Some(player_obj(0x5000_0012)), 3);
    let mut item = WorldObject {
        guid: g(0x8000_0013),
        ..Default::default()
    };
    item.set_inscription(Some("hi".into()));
    item.set_scribe_name(Some("Sc".into()));
    let m =
        game_event_inscription_response::game_event_inscription_response(&mut w, SESSION, &item);
    let body = &m.data[16..];
    let mut want = Vec::new();
    // V382: retail's layout, the player guid before the inscription.
    want.extend_from_slice(&0x8000_0013u32.to_le_bytes());
    want.extend_from_slice(&0x5000_0012u32.to_le_bytes());
    want.extend_from_slice(b"\x02\x00hi");
    want.extend_from_slice(b"\x02\x00Sc");
    want.extend_from_slice(b"\x00\x00\x00\x00");
    assert_eq!(hex(body), hex(&want));

    let mut ses = session(1);
    item.set_placement_position(Some(4));
    item.set_requires_pack_slot(true);
    let m = game_event_item_server_says_contain_id::game_event_item_server_says_contain_id(
        &mut ses,
        &item,
        g(0x5000_0012),
    );
    assert_eq!(
        hex(&m.data[16..]),
        "130000801200005004000000".to_owned() + "02000000" /* Foci */
    );
    let bare = WorldObject {
        guid: g(0x8000_0014),
        ..Default::default()
    };
    let m = game_event_item_server_says_contain_id::game_event_item_server_says_contain_id(
        &mut ses,
        &bare,
        g(0x5000_0012),
    );
    assert_eq!(
        hex(&m.data[16..]),
        "140000801200005000000000".to_owned() + "00000000" /* NonContainer */
    );
}

#[test]
fn tell_from_a_world_object_marks_olthoi_names() {
    let mut w = world_with_player(Some(player_obj(0x5000_0015)), 1);
    let mut olthoi = WorldObject {
        guid: g(0x8000_0016),
        ..Default::default()
    };
    olthoi.set_creature_type(Some(CreatureType::Olthoi));
    w.objects.insert(olthoi).unwrap();
    let _ = take_local();
    let m = game_event_tell::game_event_tell(
        &mut w,
        g(0x8000_0016),
        "m",
        g(0x5000_0015),
        SESSION,
        ChatMessageType(3),
    );
    let body = &m.data[16..];
    // The object has no Name property, so the virtual `Name` is null and `null + "&"` is "&".
    assert_eq!(&body[..8], b"\x01\x00m\x00\x01\x00&\x00");
    assert_eq!(&body[8..12], &0x8000_0016u32.to_le_bytes());
    assert_eq!(&body[12..16], &0x5000_0015u32.to_le_bytes());
    assert_eq!(&body[16..], &[3, 0, 0, 0, 0, 0, 0, 0]);
    assert!(
        !take_local().contains_key("ACE: WorldObject.Name"),
        "The object name is read from its properties"
    );
}

#[test]
fn salvage_result_bonus_is_the_salvage_augmentation_times_25() {
    let mut p = player_obj(0x5000_0017);
    p.set_augmentation_bonus_salvage(2);
    let mut w = world_with_player(Some(p), 1);
    let m = game_event_salvage_operations_result::game_event_salvage_operations_result(
        &mut w,
        SESSION,
        Skill::Salvaging,
        &[],
    );
    assert_eq!(hex(&m.data[16..]), "28000000000000000000000032000000");
    let m = game_event_salvage_operations_result::game_event_salvage_operations_result(
        &mut w,
        SESSION,
        Skill(6),
        &[],
    );
    assert_eq!(hex(&m.data[16..]), "06000000000000000000000000000000");
}

#[test]
fn house_update_restrictions_consumes_the_restriction_db_byte_sequence() {
    let mut ses = session(1);
    let mut o = obj(0x7000_0018);
    let db = empyrean_world::network::structure::restriction_db::RestrictionDB::default();
    let _ = take_local();
    let m = game_event_house_update_restrictions::game_event_house_update_restrictions(
        &mut ses, &mut o, &db,
    );
    assert_eq!(
        hex(&m.data[16..]),
        "001800007002000010000000000000000000000003"
    );
    assert!(take_local().is_empty());
}

#[test]
fn start_barber_writes_the_appearance_and_consumes_its_sequence() {
    let mut o = player_obj(0x5000_0019);
    for (p, v) in [
        (PropertyDataId::PaletteBase, 0x0400_0001u32),
        (PropertyDataId::HeadObject, 0x0100_0002),
        (PropertyDataId::EyesTexture, 0x0500_0003),
        (PropertyDataId::DefaultEyesTexture, 0x0500_0004),
        (PropertyDataId::NoseTexture, 0x0500_0005),
        (PropertyDataId::DefaultNoseTexture, 0x0500_0006),
        (PropertyDataId::MouthTexture, 0x0500_0007),
        (PropertyDataId::DefaultMouthTexture, 0x0500_0008),
        (PropertyDataId::SkinPalette, 0x0400_0009),
        (PropertyDataId::HairPalette, 0x0400_000A),
        (PropertyDataId::Setup, 0x0200_000C),
        (PropertyDataId::MotionTable, 0x0900_020D),
    ] {
        o.set_property(p, v);
    }
    o.player.as_mut().unwrap().player.character = Some(empyrean_store::models::shard::Character {
        hair_texture: 0x0500_0010,
        default_hair_texture: 0x0500_0011,
        ..Default::default()
    });
    let mut w = world_with_player(Some(o), 10);
    let _ = take_local();
    let m = game_event_start_barber::game_event_start_barber(&mut w, SESSION);
    assert!(take_local().is_empty(), "no unimplemented member is called");
    assert_eq!(&m.data[4..8], &0x5000_0019u32.to_le_bytes());
    assert_eq!(&m.data[8..12], &10u32.to_le_bytes());
    assert_eq!(&m.data[12..16], &0x75u32.to_le_bytes());
    let body: Vec<u32> = m.data[16..]
        .chunks(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
        .collect();
    assert_eq!(
        body,
        [
            0x0400_0001,
            0x0100_0002,
            0x0500_0010,
            0x0500_0011,
            0x0500_0003,
            0x0500_0004,
            0x0500_0005,
            0x0500_0006,
            0x0500_0007,
            0x0500_0008,
            0x0400_0009,
            0x0400_000A,
            0,
            0x0200_000C,
            1,
            0
        ]
    );
    assert_eq!(w.sessions.get(SESSION).unwrap().game_event_sequence, 11);
}

#[test]
fn object_serialisation_messages_hand_off_to_the_2_2b_serialisers() {
    let mut w = world();
    let mut o = WorldObject {
        guid: g(0x8000_0020),
        ..Default::default()
    };
    o.set_position(
        PositionType::Location,
        Some(Position::from_components(
            0x0101_0001,
            1.0,
            2.0,
            3.0,
            1.0,
            0.0,
            0.0,
            0.0,
            false,
        )),
    );
    w.objects.insert(o).unwrap();
    let _ = take_local();
    let c = game_message_create_object::game_message_create_object(
        &mut w,
        g(0x8000_0020),
        false,
        false,
    );
    let u = game_message_update_object::game_message_update_object(
        &mut w,
        g(0x8000_0020),
        false,
        false,
    );
    let d = game_message_obj_desc_event::game_message_obj_desc_event(&mut w, g(0x8000_0020));
    let p =
        game_message_update_position::game_message_update_position(&mut w, g(0x8000_0020), false);
    assert_eq!(hex(&c.data[..8]), "45F7000020000080");
    assert_eq!(hex(&u.data[..8]), "DBF7000020000080");
    assert_eq!(hex(&d.data[..8]), "25F6000020000080");
    assert_eq!(hex(&p.data[..8]), "48F7000020000080");
    assert!(c.data.len() > 8 && u.data.len() > 8 && d.data.len() > 8 && p.data.len() > 8);
    assert_eq!(c.group, GameMessageGroup::SmartboxQueue);
    let hits = take_local();
    for name in [
        "ACE: WorldObject.SerializeCreateObject",
        "ACE: WorldObject.SerializeUpdateObject",
        "ACE: WorldObject.SerializeUpdateModelData",
        "ACE: PositionPack.PositionPack",
        "ACE: PositionPackExtensions.Write",
    ] {
        assert!(!hits.contains_key(name), "{name}: {hits:?}");
    }
}

// ---- SequenceManager ---------------------------------------------------------------------

#[test]
fn sequences_follow_aces_start_values_and_wrap_points() {
    let mut m = SequenceManager::new();
    // Object sequences: client-primed ushort, current 0, next 1.
    assert_eq!(
        m.get_current_sequence(SequenceType::ObjectPosition),
        vec![0, 0]
    );
    assert_eq!(
        m.get_next_sequence(SequenceType::ObjectPosition),
        vec![1, 0]
    );
    // Motion: starts at 1, wraps from 0x7FFF to 0.
    assert_eq!(m.get_current_sequence(SequenceType::Motion), vec![1, 0]);
    for _ in 0..0x7FFE {
        let _ = m.get_next_sequence(SequenceType::Motion);
    }
    assert_eq!(
        m.get_current_sequence(SequenceType::Motion),
        vec![0xFF, 0x7F]
    );
    assert_eq!(m.get_next_sequence(SequenceType::Motion), vec![0, 0]);
    // Everything else: an unprimed byte sequence, current 255, first next 0.
    assert_eq!(
        m.get_current_sequence_of(SequenceType::UpdatePropertyInt, PropertyInt::Level),
        vec![0xFF]
    );
    assert_eq!(
        m.get_next_sequence_of(SequenceType::UpdatePropertyInt, PropertyInt::Level),
        vec![0]
    );
    // Keys are per property.
    assert_eq!(
        m.get_next_sequence_of(SequenceType::UpdatePropertyInt, PropertyInt::Value),
        vec![0]
    );
    assert_eq!(
        m.get_next_sequence_of(SequenceType::UpdatePropertyInt, PropertyInt::Level),
        vec![1]
    );
    // And per type: (type << 16) | property.
    assert_eq!(
        m.get_next_sequence_of(SequenceType::UpdatePropertyBool, 25u32),
        vec![0]
    );
    // A ushort object sequence wraps from 65535 to 0.
    let mut u = UShortSequence::new(0xFFFE, u16::MAX);
    assert_eq!(u.next_bytes(), vec![0xFF, 0xFF]);
    assert_eq!(u.next_bytes(), vec![0, 0]);
    // SetSequence replaces the property-0 sequence.
    m.set_sequence(
        SequenceType::ObjectTeleport,
        Box::new(UShortSequence::new(41, u16::MAX)),
    );
    assert_eq!(
        m.get_next_sequence(SequenceType::ObjectTeleport),
        vec![42, 0]
    );
    // The other sequence widths.
    let mut b = ByteSequence::new_primed(true, 2);
    assert_eq!(
        (b.next_bytes(), b.next_bytes(), b.next_bytes()),
        (vec![1], vec![2], vec![0])
    );
    let mut ui = UIntSequence::new_primed(false, u32::MAX);
    assert_eq!(ui.current_bytes(), vec![0xFF; 4]);
    assert_eq!(ui.next_bytes(), vec![0; 4]);
    let mut ul = ULongSequence::new(5, 6);
    assert_eq!(
        (ul.next_bytes(), ul.next_bytes()),
        (6u64.to_le_bytes().to_vec(), vec![0; 8])
    );
    // Skill keys reinterpret the int enum.
    assert_eq!(
        m.get_next_sequence_of(SequenceType::UpdateSkill, Skill(-1)),
        vec![0]
    );
}

// ---- EnqueueSend -------------------------------------------------------------------------

#[test]
fn enqueue_send_keeps_call_order_per_session() {
    let mut w = world_with_player(None, 1);
    let other = SessionId {
        client_id: 2,
        generation: 1,
    };
    game_message::start_capture();
    game_message::enqueue_send(
        &mut w,
        SESSION,
        game_message_character_delete::game_message_character_delete(),
    );
    game_message::enqueue_send_many(
        &mut w,
        other,
        [
            game_message_server_name::game_message_server_name("a", 0, -1),
            game_message_sound::game_message_sound(g(1), Sound(2), 1.0),
        ],
    );
    game_message::enqueue_send(
        &mut w,
        SESSION,
        game_message_character_log_off::game_message_character_log_off(),
    );
    let sent = game_message::take_sent();
    let summary: Vec<(u16, GameMessageGroup, String)> = sent
        .iter()
        .map(|(s, grp, bytes)| (s.client_id, *grp, hex(&bytes[..4])))
        .collect();
    assert_eq!(
        summary,
        vec![
            (1, GameMessageGroup::UIQueue, "55F60000".to_owned()),
            (2, GameMessageGroup::UIQueue, "E1F70000".to_owned()),
            (2, GameMessageGroup::SmartboxQueue, "50F70000".to_owned()),
            (1, GameMessageGroup::UIQueue, "53F60000".to_owned()),
        ]
    );
    assert!(game_message::take_sent().is_empty());
}

#[test]
fn a_message_converts_to_the_transports_view() {
    let m = game_message_character_delete::game_message_character_delete();
    assert_eq!(m.opcode, GameMessageOpcode::CharacterDelete);
    let out = m.into_outbound();
    assert_eq!(out.opcode(), 0xF655);
    assert_eq!(out.group, GameMessageGroup::UIQueue);
}

// ---- Shared rules: decode our bytes with the client's dereth-protocol ------------------------------------
//
// Each check decodes the message with `dereth-protocol`'s decoder (tolerating at most three zero bytes of
// sender padding, as the retail archive does), re-encodes the decoded value and requires the same
// bytes back, then spot-checks the fields. Deliberate layout differences are documented in the
// server divergence register; these checks remain active.

use dereth_primitives::ObjectId;
use dereth_protocol::{self as dp, Message as ProtoMessage};

/// Decodes `m` as `M`: a plain message by its opcode, a game event (0xF7B0) by its event type
/// after the 12-byte ordered-event header.
fn decode_client_message<M: ProtoMessage + std::fmt::Debug>(m: &GameMessage) -> M {
    let data = &m.data;
    let (ty, body) = if m.opcode == GameMessageOpcode::GameEvent {
        let mut r = dp::Reader::new(data);
        dp::OrderedEventHeader::read(&mut r).expect("0xF7B0 header");
        (
            u32::from_le_bytes(data[12..16].try_into().unwrap()),
            &data[16..],
        )
    } else {
        (
            u32::from_le_bytes(data[..4].try_into().unwrap()),
            &data[4..],
        )
    };
    assert_eq!(ty, M::OPCODE.0, "opcode / event type");
    let decoded = dp::read_body_padded::<M>(body)
        .unwrap_or_else(|e| panic!("dereth-protocol rejects {}: {e}", hex(body)));
    let again = dp::write_body(&decoded).expect("re-encode");
    assert!(
        again.len() <= body.len(),
        "re-encoding is longer: {} vs {}",
        hex(&again),
        hex(body)
    );
    assert_eq!(
        hex(&body[..again.len()]),
        hex(&again),
        "fields do not round-trip"
    );
    assert!(
        body[again.len()..].iter().all(|b| *b == 0),
        "unread tail {}",
        hex(&body[again.len()..])
    );
    decoded
}

macro_rules! message_roundtrip_test {
    ($(#[$attr:meta])* $name:ident, $ty:ty, $build:expr, |$d:ident| $check:block) => {
        #[test]
        $(#[$attr])*
        fn $name() {
            let m: GameMessage = $build;
            let $d: $ty = decode_client_message(&m);
            $check
        }
    };
}

fn ses() -> SessionData {
    session(5)
}

// -- messages --

message_roundtrip_test!(
    set_stack_size_round_trips_through_the_client_protocol,
    dp::items::ItemUpdateStackSize,
    {
        let mut o = obj(0x8000_0030);
        o.wo.set_stack_size_prop(Some(7));
        o.wo.set_value(Some(70));
        game_message_set_stack_size::game_message_set_stack_size(&mut o)
    },
    |d| {
        assert_eq!(
            (d.item, d.amount, d.new_value),
            (ObjectId(0x8000_0030), 7, 70)
        );
    }
);

message_roundtrip_test!(
    player_killed_round_trips_through_the_client_protocol,
    dp::combat::CombatHandlePlayerDeathEvent,
    game_message_player_killed::game_message_player_killed(
        "You died",
        g(0x5000_0001),
        g(0x8000_0002)
    ),
    |d| {
        assert_eq!(
            (d.message.as_str(), d.killed, d.killer),
            ("You died", ObjectId(0x5000_0001), ObjectId(0x8000_0002))
        );
    }
);

message_roundtrip_test!(
    hear_speech_round_trips_through_the_client_protocol,
    dp::comms::CommunicationHearSpeech,
    game_message_hear_speech::game_message_hear_speech(
        "hello",
        "Bob",
        0x5000_0003,
        ChatMessageType(2)
    ),
    |d| {
        assert_eq!(
            (
                d.message.as_str(),
                d.sender_name.as_str(),
                d.sender_id,
                d.text_type
            ),
            ("hello", "Bob", ObjectId(0x5000_0003), 2)
        );
    }
);

message_roundtrip_test!(
    hear_ranged_speech_round_trips_through_the_client_protocol,
    dp::comms::CommunicationHearRangedSpeech,
    game_message_hear_ranged_speech::game_message_hear_ranged_speech(
        "yo",
        "Al",
        0x5000_0004,
        30.0,
        ChatMessageType(12)
    ),
    |d| {
        assert_eq!((d.message.as_str(), d.range, d.text_type), ("yo", 30.0, 12));
    }
);

message_roundtrip_test!(
    admin_environs_round_trips_through_the_client_protocol,
    dp::admin::AdminEnvirons,
    game_message_admin_environs::game_message_admin_environs(EnvironChangeType(2)),
    |d| {
        assert_eq!(d.environ_option, 2);
    }
);

message_roundtrip_test!(
    character_create_response_round_trips_through_the_client_protocol,
    dp::login::CharGenVerificationResponse,
    game_message_character_create_response::game_message_character_create_response(
        empyrean_net::enums::CharacterGenerationVerificationResponse::Ok,
        g(0x5000_0005),
        "Newbie"
    ),
    |d| {
        assert_eq!(
            (d.response_type, d.identity.gid, d.identity.name.as_str()),
            (1, ObjectId(0x5000_0005), "Newbie")
        );
    }
);

message_roundtrip_test!(
    character_create_response_failure_round_trips_through_the_client_protocol,
    dp::login::CharGenVerificationResponse,
    game_message_character_create_response::game_message_character_create_response(
        empyrean_net::enums::CharacterGenerationVerificationResponse::NameInUse,
        g(0x5000_0005),
        "Taken"
    ),
    |d| {
        assert_eq!(d.response_type, 3);
    }
);

message_roundtrip_test!(
    character_restore_round_trips_through_the_client_protocol,
    dp::login::CharGenVerificationResponse,
    game_message_character_restore::game_message_character_restore(0x5000_0006, "Back", 60),
    |d| {
        assert_eq!(
            (
                d.response_type,
                d.identity.name.as_str(),
                d.identity.seconds_greyed_out
            ),
            (1, "Back", 60)
        );
    }
);

message_roundtrip_test!(
    character_log_off_round_trips_through_the_client_protocol,
    dp::login::LoginExecuteLogOff,
    game_message_character_log_off::game_message_character_log_off(),
    |_d| {}
);
message_roundtrip_test!(
    character_delete_round_trips_through_the_client_protocol,
    dp::login::CharacterDeleteAck,
    game_message_character_delete::game_message_character_delete(),
    |_d| {}
);
message_roundtrip_test!(enter_world_server_ready_round_trips_through_the_client_protocol, dp::login::LoginEnterGameServerReady,
    game_message_character_enter_world_server_ready::game_message_character_enter_world_server_ready(), |_d| {});

message_roundtrip_test!(
    character_error_round_trips_through_the_client_protocol,
    dp::login::CharacterError,
    game_message_character_error::game_message_character_error(
        CharacterError::EnterGameCharacterLocked
    ),
    |d| {
        assert_eq!(d.char_error, 0x17);
    }
);

message_roundtrip_test!(
    character_list_round_trips_through_the_client_protocol,
    dp::login::LoginCharacterSet,
    {
        let w = world();
        let ses = SessionData {
            account: Some("acct".into()),
            ..Default::default()
        };
        let chars = vec![CharacterSummary {
            id: 0x5000_0007,
            name: "Aa".into(),
            is_deleted: false,
            delete_time: 0,
            ..Default::default()
        }];
        game_message_character_list::game_message_character_list(&w, &chars, &ses)
    },
    |d| {
        assert_eq!(d.characters.len(), 1);
        assert_eq!(
            (d.characters[0].gid, d.characters[0].name.as_str()),
            (ObjectId(0x5000_0007), "Aa")
        );
        assert_eq!((d.account.as_str(), d.has_throne_of_destiny), ("acct", 1));
    }
);

message_roundtrip_test!(
    player_create_round_trips_through_the_client_protocol,
    dp::objects::LoginCreatePlayer,
    game_message_player_create::game_message_player_create(g(0x5000_0008)),
    |d| {
        assert_eq!(d.player_id, ObjectId(0x5000_0008));
    }
);

message_roundtrip_test!(
    delete_object_round_trips_through_the_client_protocol,
    dp::objects::ItemDeleteObject,
    {
        let mut o = obj(0x8000_0009);
        advance(&mut o.seq, 8, 0, 3);
        game_message_delete_object::game_message_delete_object(&mut o)
    },
    |d| {
        assert_eq!((d.id, d.instance_sequence), (ObjectId(0x8000_0009), 3));
    }
);

message_roundtrip_test!(
    parent_event_round_trips_through_the_client_protocol,
    dp::objects::ItemParentEvent,
    {
        let mut c = obj(0x5000_000A);
        let mut i = obj(0x8000_000B);
        game_message_parent_event::game_message_parent_event(
            &mut c,
            &mut i,
            Some(ParentLocation::RightHand),
            Some(Placement::RightHandCombat),
        )
    },
    |d| {
        assert_eq!(
            (d.creature, d.item, d.location, d.placement_frame),
            (ObjectId(0x5000_000A), ObjectId(0x8000_000B), 1, 1)
        );
    }
);

message_roundtrip_test!(
    pickup_event_round_trips_through_the_client_protocol,
    dp::objects::InventoryPickupEvent,
    game_message_pickup_event::game_message_pickup_event(&mut obj(0x8000_000C)),
    |d| {
        assert_eq!(d.id, ObjectId(0x8000_000C));
    }
);

message_roundtrip_test!(
    set_state_round_trips_through_the_client_protocol,
    dp::objects::ItemSetState,
    game_message_set_state::game_message_set_state(&mut obj(0x8000_000D), PhysicsState(0x408)),
    |d| {
        assert_eq!((d.id, d.state), (ObjectId(0x8000_000D), 0x408));
    }
);

message_roundtrip_test!(
    vector_update_round_trips_through_the_client_protocol,
    dp::movement::MovementVectorUpdate,
    game_message_vector_update::game_message_vector_update_of(
        &mut obj(0x8000_000E),
        empyrean_entity::Vector3::new(1.0, 2.0, 3.0),
        empyrean_entity::Vector3::ZERO
    ),
    |d| {
        assert_eq!(d.id, ObjectId(0x8000_000E));
        assert_eq!((d.velocity.x, d.velocity.y, d.velocity.z), (1.0, 2.0, 3.0));
    }
);

message_roundtrip_test!(
    sound_round_trips_through_the_client_protocol,
    dp::objects::EffectsSoundEvent,
    game_message_sound::game_message_sound(g(0x8000_000F), Sound(0x77), 0.5),
    |d| {
        assert_eq!(
            (d.id, d.sound_type, d.volume),
            (ObjectId(0x8000_000F), 0x77, 0.5)
        );
    }
);

message_roundtrip_test!(
    player_teleport_round_trips_through_the_client_protocol,
    dp::objects::EffectsPlayerTeleport,
    game_message_player_teleport::game_message_player_teleport(&mut obj(0x5000_0010)),
    |d| {
        assert_eq!(d.teleport_sequence, 1);
    }
);

message_roundtrip_test!(
    autonomous_position_round_trips_through_the_client_protocol,
    dp::movement::MovementAutonomousPosition,
    {
        let mut p = Obj {
            wo: player_obj(0x5000_0011),
            seq: SequenceManager::new(),
        };
        p.wo.set_location(Some(Position::from_components(
            0x7D64_0024,
            1.0,
            2.0,
            3.0,
            0.0,
            0.0,
            0.0,
            1.0,
            false,
        )));
        game_message_autonomous_position::game_message_autonomous_position(&mut p)
    },
    |_d| {}
);

message_roundtrip_test!(
    script_round_trips_through_the_client_protocol,
    dp::objects::EffectsPlayScriptType,
    game_message_script::game_message_script(g(0x8000_0012), PlayScript(0x76), 1.0),
    |d| {
        assert_eq!(
            (d.id, d.script_type, d.intensity),
            (ObjectId(0x8000_0012), 0x76, 1.0)
        );
    }
);

message_roundtrip_test!(
    account_banned_round_trips_through_the_client_protocol,
    dp::login::LoginAccountBanned,
    {
        let w = world();
        game_message_account_banned::game_message_account_banned(
            &w,
            w.now.utc + TimeSpan::from_seconds(60.0),
            Some("bad"),
        )
    },
    |d| {
        assert_eq!((d.expiry, d.reason.as_str()), (60, "bad"));
    }
);

message_roundtrip_test!(
    boot_account_round_trips_through_the_client_protocol,
    dp::login::LoginAccountBooted,
    game_message_boot_account::game_message_boot_account(Some(" because")),
    |d| {
        assert_eq!(d.reason.as_deref(), Some(" because"));
    }
);

message_roundtrip_test!(
    boot_account_without_reason_round_trips_through_the_client_protocol,
    dp::login::LoginAccountBooted,
    game_message_boot_account::game_message_boot_account(None),
    |d| {
        assert_eq!(d.reason, None);
    }
);

message_roundtrip_test!(
    system_chat_round_trips_through_the_client_protocol,
    dp::comms::CommunicationTextboxString,
    game_message_system_chat::game_message_system_chat("Welcome", ChatMessageType(4)),
    |d| {
        assert_eq!((d.text.as_str(), d.text_type), ("Welcome", 4));
    }
);

message_roundtrip_test!(
    server_name_round_trips_through_the_client_protocol,
    dp::login::LoginWorldInfo,
    game_message_server_name::game_message_server_name("Dereth", 3, -1),
    |d| {
        assert_eq!(
            (d.connections, d.max_connections, d.world_name.as_str()),
            (3, -1, "Dereth")
        );
    }
);

message_roundtrip_test!(
    turbine_chat_round_trips_through_the_client_protocol,
    dp::comms::CommunicationTurbineChat,
    game_message_turbine_chat::game_message_turbine_chat(
        ChatNetworkBlobType::NETBLOB_EVENT_BINARY,
        ChatNetworkBlobDispatchType(1),
        0x000B_0001,
        "S",
        "hi",
        0x5000_0013,
        ChatType(3)
    ),
    |d| {
        assert!(!d.payload.is_empty());
    }
);

message_roundtrip_test!(
    ddd_error_round_trips_through_the_client_protocol,
    dp::admin::DddError,
    game_message_ddd_error_message::game_message_ddd_error_message(1, 2, 3),
    |d| {
        assert_eq!((d.resource_type, d.resource_id, d.error), (1, 2, 3));
    }
);

message_roundtrip_test!(
    ddd_interrogation_round_trips_through_the_client_protocol,
    dp::admin::DddInterrogation,
    game_message_ddd_interrogation::game_message_ddd_interrogation(&world()),
    |d| {
        assert_eq!(
            (
                d.servers_region,
                d.product_id,
                d.supported_languages.clone()
            ),
            (1, 1, vec![0, 1])
        );
    }
);

message_roundtrip_test!(
    ddd_begin_ddd_round_trips_through_the_client_protocol,
    dp::admin::DddBeginDdd,
    {
        let mut d: HashMap<DatDatabaseType, DotNetDict<u32, Vec<u32>>> = HashMap::new();
        let mut portal = DotNetDict::new();
        portal.add(2072, vec![0x0E00_000E]);
        let mut cell = DotNetDict::new();
        cell.add(982, vec![0x01D9_0108]);
        d.insert(DatDatabaseType::Portal, portal);
        d.insert(DatDatabaseType::Cell, cell);
        game_message_ddd_begin_ddd::game_message_ddd_begin_ddd(2, 1000, &d)
    },
    |d| {
        assert_eq!(d.revisions.len(), 2);
        assert_eq!(
            (
                d.revisions[0].dat_file_id,
                d.revisions[0].iteration,
                d.revisions[0].ids_to_download.clone()
            ),
            (1, 2072, vec![0x0E00_000E])
        );
        assert_eq!(
            (
                d.revisions[1].dat_file_id,
                d.revisions[1].ids_to_purge.clone()
            ),
            (2, vec![0x01D9_0108])
        );
    }
);

message_roundtrip_test!(
    ddd_end_round_trips_through_the_client_protocol,
    dp::admin::DddEndDdd,
    game_message_ddd_end_ddd::game_message_ddd_end_ddd(),
    |_d| {}
);

message_roundtrip_test!(
    private_update_int_round_trips_through_the_client_protocol,
    dp::qualities::QualitiesPrivateUpdateInt,
    game_message_private_update_property_int::game_message_private_update_property_int(
        &mut obj(1),
        PropertyInt::Level,
        42
    ),
    |d| {
        assert_eq!((d.0.sequence, d.0.property_id, d.0.value), (0, 25, 42));
    }
);

message_roundtrip_test!(
    public_update_int_round_trips_through_the_client_protocol,
    dp::qualities::QualitiesUpdateInt,
    game_message_public_update_property_int::game_message_public_update_property_int(
        &mut obj(0x5000_0014),
        PropertyInt::Level,
        42
    ),
    |d| {
        assert_eq!(
            (d.0.object, d.0.property_id, d.0.value),
            (ObjectId(0x5000_0014), 25, 42)
        );
    }
);

message_roundtrip_test!(
    private_update_int64_round_trips_through_the_client_protocol,
    dp::qualities::QualitiesPrivateUpdateInt64,
    game_message_private_update_property_int64::game_message_private_update_property_int64(
        &mut obj(1),
        PropertyInt64(1),
        -9
    ),
    |d| {
        assert_eq!(d.0.value, -9);
    }
);

message_roundtrip_test!(
    public_update_int64_round_trips_through_the_client_protocol,
    dp::qualities::QualitiesUpdateInt64,
    game_message_public_update_property_int64::game_message_public_update_property_int64(
        &mut obj(2),
        PropertyInt64(1),
        9
    ),
    |d| {
        assert_eq!((d.0.object, d.0.value), (ObjectId(2), 9));
    }
);

message_roundtrip_test!(
    private_update_bool_round_trips_through_the_client_protocol,
    dp::qualities::QualitiesPrivateUpdateBool,
    game_message_private_update_property_bool::game_message_private_update_property_bool(
        &mut obj(1),
        PropertyBool(4),
        true
    ),
    |d| {
        assert_eq!(d.0.value, 1);
    }
);

message_roundtrip_test!(
    public_update_bool_round_trips_through_the_client_protocol,
    dp::qualities::QualitiesUpdateBool,
    game_message_public_update_property_bool::game_message_public_update_property_bool(
        &mut obj(3),
        PropertyBool(4),
        true
    ),
    |d| {
        assert_eq!((d.0.object, d.0.value), (ObjectId(3), 1));
    }
);

message_roundtrip_test!(
    private_update_float_round_trips_through_the_client_protocol,
    dp::qualities::QualitiesPrivateUpdateFloat,
    game_message_private_update_property_float::game_message_private_update_property_float(
        &mut obj(1),
        PropertyFloat(5),
        0.25
    ),
    |d| {
        assert_eq!(d.0.value, 0.25);
    }
);

message_roundtrip_test!(
    public_update_float_round_trips_through_the_client_protocol,
    dp::qualities::QualitiesUpdateFloat,
    game_message_public_update_property_float::game_message_public_update_property_float(
        &mut obj(4),
        PropertyFloat(5),
        0.25
    ),
    |d| {
        assert_eq!((d.0.object, d.0.value), (ObjectId(4), 0.25));
    }
);

message_roundtrip_test!(
    private_update_string_round_trips_through_the_client_protocol,
    dp::qualities::QualitiesPrivateUpdateString,
    game_message_private_update_property_string::game_message_private_update_property_string(
        &mut obj(1),
        PropertyString::Name,
        Some("Bob")
    ),
    |d| {
        assert_eq!((d.0.property_id, d.0.value.0.as_str()), (1, "Bob"));
    }
);

message_roundtrip_test!(
    // V234: guid then property, as the client reads it.
    public_update_string_round_trips_through_the_client_protocol,
    dp::qualities::QualitiesUpdateString,
    game_message_public_update_property_string::game_message_public_update_property_string(
        &mut obj(0x5000_0015),
        PropertyString::Name,
        Some("Bob")
    ),
    |d| {
        assert_eq!(
            (d.0.object, d.0.property_id, d.0.value.0.as_str()),
            (ObjectId(0x5000_0015), 1, "Bob")
        );
    }
);

message_roundtrip_test!(
    private_update_data_id_round_trips_through_the_client_protocol,
    dp::qualities::QualitiesPrivateUpdateDataId,
    game_message_private_update_data_id::game_message_private_update_data_id(
        &mut obj(1),
        PropertyDataId(1),
        0x0200_0001
    ),
    |d| {
        assert_eq!(d.0.value, 0x0200_0001);
    }
);

message_roundtrip_test!(
    public_update_data_id_round_trips_through_the_client_protocol,
    dp::qualities::QualitiesUpdateDataId,
    game_message_public_update_data_id::game_message_public_update_data_id(
        &mut obj(5),
        PropertyDataId(1),
        0x0200_0001
    ),
    |d| {
        assert_eq!((d.0.object, d.0.value), (ObjectId(5), 0x0200_0001));
    }
);

message_roundtrip_test!(
    private_update_instance_id_round_trips_through_the_client_protocol,
    dp::qualities::QualitiesPrivateUpdateInstanceId,
    game_message_private_update_instance_id::game_message_private_update_instance_id(
        &mut obj(1),
        PropertyInstanceId(2),
        0x8000_0001
    ),
    |d| {
        assert_eq!(d.0.value, ObjectId(0x8000_0001));
    }
);

message_roundtrip_test!(
    public_update_instance_id_round_trips_through_the_client_protocol,
    dp::qualities::QualitiesUpdateInstanceId,
    game_message_public_update_instance_id::game_message_public_update_instance_id(
        &mut obj(6),
        PropertyInstanceId(2),
        g(0x8000_0001)
    ),
    |d| {
        assert_eq!(
            (d.0.object, d.0.value),
            (ObjectId(6), ObjectId(0x8000_0001))
        );
    }
);

message_roundtrip_test!(
    private_update_position_round_trips_through_the_client_protocol,
    dp::qualities::QualitiesPrivateUpdatePosition,
    game_message_private_update_position::game_message_private_update_position(
        &mut obj(1),
        PositionType(14),
        &vector_position()
    ),
    |d| {
        assert_eq!(d.0.property_id, 14);
    }
);

message_roundtrip_test!(
    public_update_position_round_trips_through_the_client_protocol,
    dp::qualities::QualitiesUpdatePosition,
    game_message_public_update_position::game_message_public_update_position(
        &mut obj(7),
        PositionType(14),
        &vector_position()
    ),
    |d| {
        assert_eq!((d.0.object, d.0.property_id), (ObjectId(7), 14));
    }
);

message_roundtrip_test!(
    private_update_skill_round_trips_through_the_client_protocol,
    dp::qualities::QualitiesPrivateUpdateSkill,
    game_message_private_update_skill::game_message_private_update_skill(
        &mut stat_obj(1),
        CreatureSkill::new(Skill(6))
    ),
    |d| {
        assert_eq!(d.0.property_id, 6);
    }
);

message_roundtrip_test!(
    private_update_attribute_round_trips_through_the_client_protocol,
    dp::qualities::QualitiesPrivateUpdateAttribute,
    game_message_private_update_attribute::game_message_private_update_attribute(
        &mut stat_obj(1),
        strength()
    ),
    |d| {
        assert_eq!(d.0.property_id, 1);
    }
);

message_roundtrip_test!(
    private_update_vital_round_trips_through_the_client_protocol,
    dp::qualities::QualitiesPrivateUpdateAttribute2nd,
    {
        let mut o = stat_obj(1);
        let health = max_health(&mut o);
        game_message_private_update_vital::game_message_private_update_vital(&mut o, health)
    },
    |d| {
        assert_eq!(d.0.property_id, 1);
    }
);

message_roundtrip_test!(
    public_update_vital_round_trips_through_the_client_protocol,
    dp::qualities::QualitiesUpdateAttribute2nd,
    game_message_public_update_vital::game_message_public_update_vital(
        &mut obj(8),
        PropertyAttribute2nd::MaxHealth,
        1,
        2,
        3,
        4
    ),
    |d| {
        assert_eq!((d.0.object, d.0.property_id), (ObjectId(8), 1));
    }
);

message_roundtrip_test!(
    private_update_attribute2nd_level_round_trips_through_the_client_protocol,
    dp::qualities::QualitiesPrivateUpdateAttribute2ndLevel,
    game_message_private_update_attribute2nd_level::game_message_private_update_attribute2nd_level(
        &mut obj(1),
        Vital::MaxHealth,
        77
    ),
    |d| {
        assert_eq!((d.0.property_id, d.0.value), (1, 77));
    }
);

// -- events --

message_roundtrip_test!(
    popup_string_round_trips_through_the_client_protocol,
    dp::comms::CommunicationPopUpString,
    game_event_popup_string::game_event_popup_string(&mut ses(), "Pop"),
    |_d| {}
);

message_roundtrip_test!(
    item_server_says_contain_id_round_trips_through_the_client_protocol,
    dp::objects::ItemServerSaysContainId,
    {
        let mut item = WorldObject {
            guid: g(0x8000_0040),
            ..Default::default()
        };
        item.set_placement_position(Some(3));
        game_event_item_server_says_contain_id::game_event_item_server_says_contain_id(
            &mut ses(),
            &item,
            g(0x5000_0041),
        )
    },
    |d| {
        assert_eq!(
            (d.item, d.container, d.slot, d.container_properties),
            (ObjectId(0x8000_0040), ObjectId(0x5000_0041), 3, 0)
        );
    }
);

message_roundtrip_test!(
    wield_item_round_trips_through_the_client_protocol,
    dp::objects::ItemWearItem,
    game_event_wield_item::game_event_wield_item(&mut ses(), 0x8000_0042, EquipMask(0x0020_0000)),
    |d| {
        assert_eq!((d.item, d.slot), (ObjectId(0x8000_0042), 0x0020_0000));
    }
);

message_roundtrip_test!(
    update_title_round_trips_through_the_client_protocol,
    dp::social::SocialAddOrSetCharacterTitle,
    game_event_update_title::game_event_update_title(&mut ses(), 12, true),
    |d| {
        assert_eq!((d.new_title, d.set_as_display_title), (12, 1));
    }
);

message_roundtrip_test!(
    inventory_server_save_failed_round_trips_through_the_client_protocol,
    dp::objects::CharacterServerSaysAttemptFailed,
    game_event_inventory_server_save_failed::game_event_inventory_server_save_failed(
        &mut ses(),
        0x8000_0043,
        WeenieError(0x36)
    ),
    |d| {
        assert_eq!((d.object, d.reason), (ObjectId(0x8000_0043), 0x36));
    }
);

message_roundtrip_test!(
    fellowship_quit_round_trips_through_the_client_protocol,
    dp::social::FellowshipQuitNotice,
    game_event_fellowship_quit::game_event_fellowship_quit(&mut ses(), 0x5000_0044),
    |_d| {}
);

message_roundtrip_test!(
    fellowship_dismiss_round_trips_through_the_client_protocol,
    dp::social::FellowshipDismiss,
    game_event_fellowship_dismiss::game_event_fellowship_dismiss(&mut ses(), g(0x5000_0045)),
    |d| {
        assert_eq!(d.target, ObjectId(0x5000_0045));
    }
);

message_roundtrip_test!(
    book_page_data_response_round_trips_through_the_client_protocol,
    dp::trade::BookPageDataResponse,
    {
        let page = PropertiesBookPageData {
            author_id: 1,
            author_name: Some("A".into()),
            author_account: None,
            ignore_author: false,
            page_text: Some("t".into()),
        };
        let mut w = world_with_player(Some(player_obj(0x5000_0046)), 1);
        game_event_book_page_data_response::game_event_book_page_data_response(
            &mut w,
            SESSION,
            0x8000_0047,
            0,
            &page,
        )
    },
    |d| {
        assert_eq!((d.object_id, d.page), (ObjectId(0x8000_0047), 0));
    }
);

message_roundtrip_test!(
    book_data_response_round_trips_through_the_client_protocol,
    dp::trade::WritingBookOpen,
    {
        let page = PropertiesBookPageData {
            author_id: 1,
            author_name: Some("A".into()),
            author_account: None,
            ignore_author: false,
            page_text: None,
        };
        let mut w = world_with_player(Some(player_obj(0x5000_0048)), 1);
        game_event_book_data_response::game_event_book_data_response(
            &mut w,
            SESSION,
            0x8000_0049,
            1000,
            2,
            &[page],
            Some("ins"),
            5,
            Some("Au"),
            false,
        )
    },
    |d| {
        assert_eq!(
            (
                d.book_id,
                d.max_num_pages,
                d.inscription.as_str(),
                d.scribe_name.as_str()
            ),
            (ObjectId(0x8000_0049), 2, "ins", "Au")
        );
    }
);

message_roundtrip_test!(
    // V382: retail's layout, the player guid in the unnamed dword.
    inscription_response_round_trips_through_the_client_protocol,
    dp::items::ItemGetInscriptionResponse,
    {
        let mut w = world_with_player(Some(player_obj(0x5000_004A)), 1);
        let mut item = WorldObject {
            guid: g(0x8000_004B),
            ..Default::default()
        };
        item.set_inscription(Some("hi".into()));
        game_event_inscription_response::game_event_inscription_response(&mut w, SESSION, &item)
    },
    |d| {
        assert_eq!(
            (d.object, d.unknown, d.inscription.as_str()),
            (ObjectId(0x8000_004B), 0x5000_004A, "hi")
        );
    }
);

message_roundtrip_test!(
    channel_broadcast_round_trips_through_the_client_protocol,
    dp::comms::CommunicationChannelBroadcastRecv,
    game_event_channel_broadcast::game_event_channel_broadcast(
        &mut ses(),
        Channel(0x800),
        "Bob",
        "yo"
    ),
    |d| {
        assert_eq!(
            (d.channel, d.sender_name.as_str(), d.message.as_str()),
            (0x800, "Bob", "yo")
        );
    }
);

message_roundtrip_test!(
    attack_done_round_trips_through_the_client_protocol,
    dp::combat::CombatHandleAttackDoneEvent,
    game_event_attack_done::game_event_attack_done(&mut ses(), WeenieError(0x36)),
    |d| {
        assert_eq!(d.error, 0x36);
    }
);

message_roundtrip_test!(
    // V235/V293: AttackConditions is one dword.
    attacker_notification_round_trips_through_the_client_protocol,
    dp::combat::AttackerNotification,
    game_event_attacker_notification::game_event_attacker_notification(
        &mut ses(),
        "Drudge",
        DamageType(4),
        0.25,
        12,
        true,
        AttackConditions(0)
    ),
    |d| {
        assert_eq!(
            (d.defender_name.as_str(), d.percent, d.damage, d.critical),
            ("Drudge", 0.25, 12, 1)
        );
    }
);

message_roundtrip_test!(
    // V235/V293: AttackConditions is one dword.
    defender_notification_round_trips_through_the_client_protocol,
    dp::combat::DefenderNotification,
    game_event_defender_notification::game_event_defender_notification(
        &mut ses(),
        "Drudge",
        DamageType(4),
        0.25,
        12,
        empyrean_net::enums::DamageLocation::Abdomen,
        false,
        AttackConditions(0)
    ),
    |d| {
        assert_eq!((d.attacker_name.as_str(), d.damage_location), ("Drudge", 2));
    }
);

message_roundtrip_test!(
    evasion_attacker_notification_round_trips_through_the_client_protocol,
    dp::combat::EvasionAttackerNotification,
    game_event_evasion_attacker_notification::game_event_evasion_attacker_notification(
        &mut ses(),
        "Mite"
    ),
    |d| {
        assert_eq!(d.defender_name.as_str(), "Mite");
    }
);

message_roundtrip_test!(
    evasion_defender_notification_round_trips_through_the_client_protocol,
    dp::combat::EvasionDefenderNotification,
    game_event_evasion_defender_notification::game_event_evasion_defender_notification(
        &mut ses(),
        "Mite"
    ),
    |_d| {}
);

message_roundtrip_test!(
    victim_notification_round_trips_through_the_client_protocol,
    dp::combat::VictimNotificationSelf,
    game_event_victim_notification::game_event_victim_notification(&mut ses(), "You died"),
    |d| {
        assert_eq!(d.message.as_str(), "You died");
    }
);

message_roundtrip_test!(
    killer_notification_round_trips_through_the_client_protocol,
    dp::combat::VictimNotificationOther,
    game_event_killer_notification::game_event_killer_notification(&mut ses(), "You killed it"),
    |_d| {}
);

message_roundtrip_test!(
    update_health_round_trips_through_the_client_protocol,
    dp::combat::CombatQueryHealthResponse,
    game_event_update_health::game_event_update_health(&mut ses(), 0x8000_004C, 0.5),
    |d| {
        assert_eq!((d.object, d.health), (ObjectId(0x8000_004C), 0.5));
    }
);

message_roundtrip_test!(
    query_age_response_round_trips_through_the_client_protocol,
    dp::admin::CharacterQueryAgeResponse,
    game_event_query_age_response::game_event_query_age_response(&mut ses(), "", "1d"),
    |d| {
        assert_eq!((d.target_name.as_str(), d.age.as_str()), ("", "1d"));
    }
);

message_roundtrip_test!(
    use_done_round_trips_through_the_client_protocol,
    dp::objects::ItemUseDone,
    game_event_use_done::game_event_use_done(&mut ses(), WeenieError(0)),
    |d| {
        assert_eq!(d.failure_type, 0);
    }
);

message_roundtrip_test!(
    fellowship_fellow_update_done_round_trips_through_the_client_protocol,
    dp::social::FellowshipFellowUpdateDone,
    game_event_fellowship_fellow_update_done::game_event_fellowship_fellow_update_done(
        &mut ses(),
        WeenieError(0)
    ),
    |_d| {}
);

message_roundtrip_test!(
    ping_response_round_trips_through_the_client_protocol,
    dp::admin::CharacterReturnPing,
    game_event_ping_response::game_event_ping_response(&mut ses()),
    |_d| {}
);

message_roundtrip_test!(
    register_trade_round_trips_through_the_client_protocol,
    dp::trade::TradeRegisterTrade,
    game_event_register_trade::game_event_register_trade(
        &mut ses(),
        g(0x5000_004D),
        g(0x5000_004E)
    ),
    |d| {
        assert_eq!(
            (d.initiator, d.partner, d.stamp),
            (ObjectId(0x5000_004D), ObjectId(0x5000_004E), 0.0)
        );
    }
);

message_roundtrip_test!(
    close_trade_round_trips_through_the_client_protocol,
    dp::trade::TradeCloseTrade,
    game_event_close_trade::game_event_close_trade(&mut ses(), EndTradeReason(2)),
    |_d| {}
);

message_roundtrip_test!(
    add_to_trade_round_trips_through_the_client_protocol,
    dp::trade::TradeAddToTradeRecv,
    game_event_add_to_trade::game_event_add_to_trade(&mut ses(), 0x8000_004F, TradeSide(2)),
    |d| {
        assert_eq!(
            (d.item, d.side, d.container_properties),
            (ObjectId(0x8000_004F), 2, 0)
        );
    }
);

message_roundtrip_test!(
    accept_trade_round_trips_through_the_client_protocol,
    dp::trade::TradeAcceptTradeRecv,
    game_event_accept_trade::game_event_accept_trade(&mut ses(), g(0x5000_0050)),
    |_d| {}
);

message_roundtrip_test!(
    decline_trade_round_trips_through_the_client_protocol,
    dp::trade::TradeDeclineTradeRecv,
    game_event_decline_trade::game_event_decline_trade(&mut ses(), g(0x5000_0051)),
    |_d| {}
);

message_roundtrip_test!(
    reset_trade_round_trips_through_the_client_protocol,
    dp::trade::TradeResetTradeRecv,
    game_event_reset_trade::game_event_reset_trade(&mut ses(), g(0x5000_0052)),
    |_d| {}
);

message_roundtrip_test!(
    trade_failure_round_trips_through_the_client_protocol,
    dp::trade::TradeTradeFailure,
    game_event_trade_failure::game_event_trade_failure(&mut ses(), 0x8000_0053, WeenieError(0x2A)),
    |d| {
        assert_eq!((d.item, d.reason), (ObjectId(0x8000_0053), 0x2A));
    }
);

message_roundtrip_test!(
    clear_trade_acceptance_round_trips_through_the_client_protocol,
    dp::trade::TradeClearTradeAcceptance,
    game_event_clear_trade_acceptance::game_event_clear_trade_acceptance(&mut ses()),
    |_d| {}
);

message_roundtrip_test!(
    house_status_round_trips_through_the_client_protocol,
    dp::trade::HouseHouseStatus,
    game_event_house_status::game_event_house_status(&mut ses(), WeenieError(4)),
    |_d| {}
);

message_roundtrip_test!(
    house_update_rent_time_round_trips_through_the_client_protocol,
    dp::trade::HouseUpdateRentTime,
    game_event_house_update_rent_time::game_event_house_update_rent_time(&mut ses()),
    |_d| {}
);

message_roundtrip_test!(
    house_transaction_round_trips_through_the_client_protocol,
    dp::trade::HouseHouseTransaction,
    game_event_house_transaction::game_event_house_transaction(&mut ses()),
    |_d| {}
);

message_roundtrip_test!(
    query_item_mana_response_round_trips_through_the_client_protocol,
    dp::items::ItemQueryItemManaResponse,
    game_event_query_item_mana_response::game_event_query_item_mana_response(
        &mut ses(),
        0x8000_0054,
        0.75,
        1
    ),
    |d| {
        assert_eq!(
            (d.object, d.mana, d.success),
            (ObjectId(0x8000_0054), 0.75, 1)
        );
    }
);

message_roundtrip_test!(
    confirmation_request_round_trips_through_the_client_protocol,
    dp::comms::CharacterConfirmationRequest,
    game_event_confirmation_request::game_event_confirmation_request(
        &mut ses(),
        ConfirmationType(5),
        9,
        "Sure?"
    ),
    |d| {
        assert_eq!(
            (d.confirmation_type, d.context_id, d.text.as_str()),
            (5, 9, "Sure?")
        );
    }
);

message_roundtrip_test!(
    confirmation_done_round_trips_through_the_client_protocol,
    dp::comms::CharacterConfirmationDone,
    game_event_confirmation_done::game_event_confirmation_done(&mut ses(), ConfirmationType(5), 9),
    |d| {
        assert_eq!((d.confirmation_type, d.context_id), (5, 9));
    }
);

message_roundtrip_test!(
    allegiance_login_notification_round_trips_through_the_client_protocol,
    dp::social::AllegianceLoginNotification,
    game_event_allegiance_login_notification::game_event_allegiance_login_notification(
        &mut ses(),
        0x5000_0055,
        true
    ),
    |d| {
        assert_eq!((d.member, d.now_logged_in), (ObjectId(0x5000_0055), 1));
    }
);

message_roundtrip_test!(
    opponent_stalemate_round_trips_through_the_client_protocol,
    dp::trade::GameOpponentStalemateState,
    game_event_opponent_stalemate::game_event_opponent_stalemate(
        &mut ses(),
        g(0x8000_0056),
        ChessColor(1),
        true
    ),
    |d| {
        assert_eq!((d.game_id, d.team, d.on), (0x8000_0056, 1, 1));
    }
);

message_roundtrip_test!(
    weenie_error_round_trips_through_the_client_protocol,
    dp::comms::CommunicationWeenieError,
    game_event_weenie_error::game_event_weenie_error(&mut ses(), WeenieError(0x1D)),
    |d| {
        assert_eq!(d.error_type, 0x1D);
    }
);

message_roundtrip_test!(
    weenie_error_with_string_round_trips_through_the_client_protocol,
    dp::comms::CommunicationWeenieErrorWithString,
    game_event_weenie_error_with_string::game_event_weenie_error_with_string(
        &mut ses(),
        WeenieErrorWithString(0x4E),
        "x"
    ),
    |d| {
        assert_eq!((d.error_type, d.text.as_str()), (0x4E, "x"));
    }
);

message_roundtrip_test!(
    set_turbine_chat_channels_round_trips_through_the_client_protocol,
    dp::comms::ChatRoomMembership,
    game_event_set_turbine_chat_channels::game_event_set_turbine_chat_channels(
        &mut ses(),
        0x000B_0001,
        6
    ),
    |d| {
        assert_eq!(
            (
                d.allegiance_room,
                d.general_room,
                d.olthoi_room,
                d.society_room,
                d.society_radblo_room
            ),
            (0x000B_0001, 2, 10, 6, 9)
        );
    }
);

message_roundtrip_test!(
    tell_round_trips_through_the_client_protocol,
    dp::comms::CommunicationHearDirectSpeech,
    game_event_tell::game_event_tell_from(
        &mut ses(),
        "psst",
        "Bob",
        0x5000_0057,
        0x5000_0058,
        ChatMessageType(3)
    ),
    |d| {
        assert_eq!(
            (d.message.as_str(), d.sender_id, d.target_id, d.secret_flags),
            ("psst", ObjectId(0x5000_0057), ObjectId(0x5000_0058), 0)
        );
    }
);

message_roundtrip_test!(
    fellowship_disband_round_trips_through_the_client_protocol,
    dp::social::FellowshipDisband,
    game_event_fellowship_disband::game_event_fellowship_disband(&mut ses()),
    |_d| {}
);

message_roundtrip_test!(
    magic_update_spell_round_trips_through_the_client_protocol,
    dp::qualities::MagicUpdateSpell,
    game_event_magic_update_spell::game_event_magic_update_spell(&mut ses(), 1234, 0),
    |_d| {}
);

message_roundtrip_test!(
    magic_remove_spell_round_trips_through_the_client_protocol,
    dp::qualities::MagicRemoveSpell,
    game_event_magic_remove_spell::game_event_magic_remove_spell(&mut ses(), 1234, 0),
    |_d| {}
);

message_roundtrip_test!(
    magic_remove_enchantment_round_trips_through_the_client_protocol,
    dp::qualities::MagicRemoveEnchantment,
    game_event_magic_remove_enchantment::game_event_magic_remove_enchantment(&mut ses(), 1234, 2),
    |_d| {}
);

message_roundtrip_test!(
    magic_dispel_enchantment_round_trips_through_the_client_protocol,
    dp::qualities::MagicDispelEnchantment,
    game_event_magic_dispel_enchantment::game_event_magic_dispel_enchantment(&mut ses(), 1234, 2),
    |_d| {}
);

message_roundtrip_test!(
    magic_purge_enchantments_round_trips_through_the_client_protocol,
    dp::qualities::MagicPurgeEnchantments,
    game_event_magic_purge_enchantments::game_event_magic_purge_enchantments(&mut ses()),
    |_d| {}
);

message_roundtrip_test!(
    magic_purge_bad_enchantments_round_trips_through_the_client_protocol,
    dp::qualities::MagicPurgeBadEnchantments,
    game_event_magic_purge_bad_enchantments::game_event_magic_purge_bad_enchantments(&mut ses()),
    |_d| {}
);

message_roundtrip_test!(
    portal_storm_round_trips_through_the_client_protocol,
    dp::trade::MiscPortalStorm,
    game_event_portal_storm::game_event_portal_storm(&mut ses()),
    |_d| {}
);

message_roundtrip_test!(
    portal_storm_subsided_round_trips_through_the_client_protocol,
    dp::trade::MiscPortalStormSubsided,
    game_event_portal_storm_subsided::game_event_portal_storm_subsided(&mut ses()),
    |_d| {}
);

message_roundtrip_test!(
    transient_string_round_trips_through_the_client_protocol,
    dp::comms::CommunicationTransientString,
    game_event_communication_transient_string::game_event_communication_transient_string(
        &mut ses(),
        "hey"
    ),
    |_d| {}
);
