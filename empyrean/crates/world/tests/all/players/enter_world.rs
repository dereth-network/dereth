//! ACE: Source/ACE.Server/Managers/WorldManager.cs::DoPlayerEnterWorld
//! DoPlayerEnterWorld members (NoLogLandblock, login ctor, SetEphemeralValues, TrackObject) and
//! the whole enter-world flow match ACE.
//! Fixture: ACE vectors and explicit expected values, synthetic dats, isolated world state, retail dats or world.pack in the real-content tier.

use std::time::Duration;

use empyrean_common::clock::ClockSnapshot;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_dat::FakeDats;
use empyrean_entity::enums::{
    AccessLevel, EquipMask, PositionType, PropertyBool, PropertyDataId, PropertyInstanceId,
    PropertyInt, WeenieType,
};
use empyrean_entity::models::PropertiesPosition;
use empyrean_entity::{Biota, ObjectGuid};
use empyrean_net::SessionId;
use empyrean_store::models::shard::Character;
use empyrean_world::dispatch::Class;
use empyrean_world::network::game_messages::game_message;
use empyrean_world::sessions::SessionData;
use empyrean_world::world_objects::player::{
    player_ctor_load_possessions, player_from_biota_with_character,
};
use empyrean_world::world_objects::player_location::{handle_no_log_landblock, NO_LOG_LANDBLOCKS};
use empyrean_world::world_objects::player_tracking;
use empyrean_world::world_objects::world_object::{CtorEnv, WorldObject};
use empyrean_world::world_objects::{container, creature_equipment, player_inventory};
use empyrean_world::World;

const PLAYER: u32 = 0x5000_0001;
const S: SessionId = SessionId {
    client_id: 3,
    generation: 1,
};

/// Fills `Creature.EquippedObjects` of `this` directly, in the given order.
fn set_equipped_objects(w: &mut World, this: ObjectGuid, items: &[ObjectGuid]) {
    if items.is_empty() {
        return;
    }
    let equipped = &mut w
        .objects
        .get_mut(this)
        .expect("live")
        .creature
        .as_mut()
        .expect("a creature")
        .creature_equipment
        .equipped_objects;
    for g in items {
        equipped.insert(*g, ());
    }
}

/// Fills `Container.Inventory` of `this` directly, in the given order.
fn set_inventory(w: &mut World, this: ObjectGuid, items: &[ObjectGuid]) {
    if items.is_empty() {
        return;
    }
    let inventory = &mut w
        .objects
        .get_mut(this)
        .expect("live")
        .container
        .as_mut()
        .expect("a container")
        .container
        .inventory;
    for g in items {
        inventory.insert(*g, ());
    }
}

fn world() -> World {
    let now = ClockSnapshot {
        portal_year_ticks: 1000.0,
        unix_time: 1_767_225_600.0,
        utc: DotNetDateTime::new(2026, 1, 1),
        monotonic: Duration::ZERO,
    };
    World::new(
        now,
        empyrean_testkit::dats::with_stat_tables(FakeDats::new())
            .build()
            .expect("fake dats"),
    )
}

fn pos(cell: u32, x: f32) -> PropertiesPosition {
    PropertiesPosition {
        obj_cell_id: cell,
        position_x: x,
        position_y: 2.0,
        position_z: 3.0,
        rotation_w: 1.0,
        ..Default::default()
    }
}

fn player_biota(
    weenie_type: WeenieType,
    location: Option<PropertiesPosition>,
    sanctuary: Option<PropertiesPosition>,
) -> Biota {
    let mut b = Biota {
        id: PLAYER,
        weenie_class_id: 1,
        weenie_type,
        ..Default::default()
    };
    b.set_property(
        empyrean_entity::enums::PropertyDataId::CombatTable,
        0x3000_0000,
    );
    if let Some(l) = location {
        b.set_property_position(PositionType::Location, l);
    }
    if let Some(s) = sanctuary {
        b.set_property_position(PositionType::Sanctuary, s);
    }
    b
}

// ---- Player.HandleNoLogLandblock --------------------------------------------------------------

#[test]
fn no_log_landblocks_move_a_player_to_its_lifestone() {
    // Every listed landblock, and only those: the Location takes the Sanctuary's cell, origin and
    // rotation.
    for &lb in &NO_LOG_LANDBLOCKS {
        let mut b = player_biota(
            WeenieType::Creature,
            Some(pos(u32::from(lb) << 16 | 0x0105, 1.0)),
            Some(pos(0xA9B4_0019, 84.0)),
        );
        assert!(handle_no_log_landblock(&mut b), "{lb:04X}");
        let l = b
            .properties_position
            .as_ref()
            .unwrap()
            .get(&PositionType::Location)
            .expect("location");
        assert_eq!((l.obj_cell_id, l.position_x), (0xA9B4_0019, 84.0));
    }
    let mut b = player_biota(
        WeenieType::Creature,
        Some(pos(0xA9B4_0019, 1.0)),
        Some(pos(0x0007_0105, 84.0)),
    );
    assert!(!handle_no_log_landblock(&mut b), "Holtburg allows login");
    assert_eq!(NO_LOG_LANDBLOCKS.len(), 37);
}

#[test]
fn staff_and_players_without_a_lifestone_stay_on_a_no_log_landblock() {
    for wt in [WeenieType::Admin, WeenieType::Sentinel] {
        let mut b = player_biota(
            wt,
            Some(pos(0x0007_0105, 1.0)),
            Some(pos(0xA9B4_0019, 84.0)),
        );
        assert!(!handle_no_log_landblock(&mut b));
        assert_eq!(
            b.properties_position
                .as_ref()
                .unwrap()
                .get(&PositionType::Location)
                .map(|l| l.obj_cell_id),
            Some(0x0007_0105)
        );
    }
    let mut b = player_biota(WeenieType::Creature, Some(pos(0x0007_0105, 1.0)), None);
    assert!(!handle_no_log_landblock(&mut b));
    let mut b = player_biota(WeenieType::Creature, None, Some(pos(0xA9B4_0019, 84.0)));
    assert!(!handle_no_log_landblock(&mut b));
}

// ---- the login constructor ----------------------------------------------------------------------

/// A shard-model biota (what `GetPossessedBiotasInParallel` returns) for an item.
fn shard_item(
    guid: u32,
    weenie_type: WeenieType,
    container: Option<u32>,
    placement: Option<i32>,
    burden: i32,
) -> empyrean_store::models::shard::Biota {
    let mut b = Biota {
        id: guid,
        weenie_class_id: 0,
        weenie_type,
        ..Default::default()
    };
    b.set_property(PropertyInt::EncumbranceVal, burden);
    b.set_property(PropertyInt::Value, 5);
    if let Some(c) = container {
        b.set_property(PropertyInstanceId::Container, c);
    }
    if let Some(p) = placement {
        b.set_property(PropertyInt::PlacementPosition, p);
    }
    empyrean_store::adapter::BiotaConverter::convert_from_entity_biota(&b, false)
}

#[test]
fn the_login_constructor_sorts_possessions_as_aces_container_does() {
    let mut w = world();
    let account_id = w
        .auth
        .lock()
        .create_account(
            "acct",
            "pw",
            AccessLevel::Player,
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        )
        .expect("created")
        .account_id;
    w.sessions.insert(
        S,
        SessionData {
            access_level: AccessLevel::Player,
            ..Default::default()
        },
    );

    // Main pack items at placements 5, none, 2; a foci (side slot) at 9; a pack with two items.
    let inventory = vec![
        shard_item(0x8000_0001, WeenieType::Generic, Some(PLAYER), Some(5), 3),
        shard_item(0x8000_0002, WeenieType::Generic, Some(PLAYER), None, 4),
        shard_item(0x8000_0003, WeenieType::Container, Some(PLAYER), Some(1), 1),
        shard_item(
            0x8000_0004,
            WeenieType::Generic,
            Some(0x8000_0003),
            Some(7),
            6,
        ),
        shard_item(
            0x8000_0005,
            WeenieType::Generic,
            Some(0x8000_0003),
            Some(3),
            8,
        ),
        shard_item(0x8000_0006, WeenieType::Generic, Some(PLAYER), Some(2), 10),
        shard_item(
            0x8000_0007,
            WeenieType::Generic,
            Some(0x8000_0099),
            None,
            100,
        ), // no container: logged, dropped
    ];
    let mut foci = shard_item(0x8000_0008, WeenieType::Generic, Some(PLAYER), Some(9), 20);
    let mut e = empyrean_store::adapter::BiotaConverter::convert_to_entity_biota(&foci, false);
    e.set_property(PropertyBool::RequiresBackpackSlot, true);
    foci = empyrean_store::adapter::BiotaConverter::convert_from_entity_biota(&e, false);
    let mut inventory = inventory;
    inventory.insert(3, foci);
    let mut worn = Biota {
        id: 0x8000_0010,
        weenie_type: WeenieType::Clothing,
        ..Default::default()
    };
    worn.set_property(PropertyInt::EncumbranceVal, 30);
    worn.set_property(
        PropertyInt::CurrentWieldedLocation,
        EquipMask::ChestWear.0.cast_signed(),
    );
    let wielded =
        vec![empyrean_store::adapter::BiotaConverter::convert_from_entity_biota(&worn, false)];

    let character = Character {
        id: PLAYER,
        account_id,
        name: "Aldric".to_owned(),
        ..Default::default()
    };
    let mut biota = player_biota(WeenieType::Creature, None, None);
    biota.set_property(
        PropertyInt::HeritageGroup,
        empyrean_entity::enums::HeritageGroup::Gearknight.0,
    );
    let p = CtorEnv::with_world(&w, |env| {
        player_from_biota_with_character(
            env,
            Class::Player,
            biota,
            inventory,
            wielded,
            character,
            Some(S),
        )
    });
    // The constructor's tail runs once the player is in the store (as DoPlayerEnterWorld does).
    let guid = p.guid;
    w.objects.insert(p).expect("fresh");
    player_ctor_load_possessions(&mut w, guid);
    let p = w.objects.get(guid).expect("inserted");

    let g = |v: u32| ObjectGuid::new(v);
    // Walked from the end: 6, foci 8, pack 3, 2, 1 (the orphan is skipped).
    assert_eq!(
        container::inventory_values(&w, guid),
        vec![
            g(0x8000_0006),
            g(0x8000_0008),
            g(0x8000_0003),
            g(0x8000_0002),
            g(0x8000_0001)
        ]
    );
    assert_eq!(
        creature_equipment::equipped_objects_values(&w, guid),
        vec![g(0x8000_0010)]
    );
    let pd = &p.player.as_ref().unwrap().player;
    let by = |v: u32| w.objects.get(g(v)).expect("built");
    assert_eq!(
        container::inventory_values(&w, g(0x8000_0003)),
        vec![g(0x8000_0005), g(0x8000_0004)]
    );
    // Placements renumbered per group, ordered by placement with null first: main 2, 6, 1; side
    // slots pack 3, foci 8; inside the pack 5, 4.
    let placements: Vec<Option<i32>> = [2, 6, 1, 3, 8, 5, 4]
        .iter()
        .map(|&v| by(0x8000_0000 + v).placement_position())
        .collect();
    assert_eq!(
        placements,
        [
            Some(0),
            Some(1),
            Some(2),
            Some(0),
            Some(1),
            Some(0),
            Some(1)
        ]
    );
    // Burden: 3 + 4 + 10 + foci 20 + the pack (its weenie has no burden: 0, then 6 + 8) + worn 30.
    assert_eq!(p.encumbrance_val(), Some(3 + 4 + 10 + 20 + 14 + 30));
    // A creature has no Value: `null += x` stays null.
    assert_eq!(p.value(), None);
    assert_eq!(player_inventory::get_all_possessions(&w, guid).len(), 8);
    assert!(
        w.objects.get(g(0x8000_0007)).is_none(),
        "the orphan is not kept"
    );
    assert!(pd.login_possessions.is_none());

    // SetEphemeralValues: the Player flag, ListeningRadius, NonCombat, container capacity, the
    // gear knight flag (gearknight_core_plating defaults true), the account.
    assert!(
        p.player
            .as_ref()
            .unwrap()
            .player_properties
            .is_gear_knight_player
    );
    assert!(
        !p.player
            .as_ref()
            .unwrap()
            .player_properties
            .is_olthoi_player
    );
    assert_eq!(pd.account.as_ref().map(|a| a.account_id), Some(account_id));
    assert_eq!(p.container_capacity(), Some(7));
    assert_eq!(p.player_kills_pk(), Some(0));
    assert!(
        empyrean_world::world_objects::world_object_networking::shims::current_motion_state(p)
            .is_some()
    );
}

#[test]
fn set_ephemeral_values_grants_the_sessions_staff_flags_when_permissions_are_overridden() {
    let mut w = world();
    let account_id = w
        .auth
        .lock()
        .create_account(
            "acct",
            "pw",
            AccessLevel::Envoy,
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        )
        .expect("created")
        .account_id;
    let override_on = w
        .auth
        .lock()
        .accounts_config()
        .override_character_permissions;
    for (level, admin, arch, sentinel, envoy, advocate) in [
        (AccessLevel::Admin, true, false, false, false, false),
        (AccessLevel::Developer, false, true, false, false, false),
        (AccessLevel::Sentinel, false, false, true, false, false),
        (AccessLevel::Envoy, false, false, true, true, false),
        (AccessLevel::Advocate, false, false, false, false, true),
        (AccessLevel::Player, false, false, false, false, false),
    ] {
        w.sessions.insert(
            S,
            SessionData {
                access_level: level,
                ..Default::default()
            },
        );
        let character = Character {
            id: PLAYER,
            account_id,
            ..Default::default()
        };
        let b = player_biota(WeenieType::Creature, None, None);
        let p = CtorEnv::with_world(&w, |env| {
            player_from_biota_with_character(
                env,
                Class::Player,
                b,
                Vec::new(),
                Vec::new(),
                character,
                Some(S),
            )
        });
        let flags = (
            p.is_admin_prop(),
            p.is_arch(),
            p.is_sentinel_prop(),
            p.is_envoy(),
            p.is_advocate(),
        );
        if override_on {
            assert_eq!(flags, (admin, arch, sentinel, envoy, advocate), "{level:?}");
        } else {
            assert_eq!(flags, (false, false, false, false, false), "{level:?}");
        }
    }
}

/// An advocate flag left from an earlier Advocate account level is cleared at login unless the
/// character earned it by the advocate quest (ACE e0f9ce83; before it, ACE kept it).
#[test]
fn set_ephemeral_values_clears_an_advocate_flag_the_account_no_longer_grants() {
    let mut w = world();
    let account_id = w
        .auth
        .lock()
        .create_account(
            "acct",
            "pw",
            AccessLevel::Player,
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        )
        .expect("created")
        .account_id;
    assert!(
        w.auth
            .lock()
            .accounts_config()
            .override_character_permissions,
        "the default"
    );
    for (level, quest, advocate) in [
        (AccessLevel::Player, false, false),
        (AccessLevel::Player, true, true),
        (AccessLevel::Advocate, false, true),
    ] {
        w.sessions.insert(
            S,
            SessionData {
                access_level: level,
                ..Default::default()
            },
        );
        let character = Character {
            id: PLAYER,
            account_id,
            ..Default::default()
        };
        let mut b = player_biota(WeenieType::Creature, None, None);
        b.set_property(empyrean_entity::enums::PropertyBool::IsAdvocate, true);
        if quest {
            b.set_property(empyrean_entity::enums::PropertyBool::AdvocateQuest, true);
        }
        let p = CtorEnv::with_world(&w, |env| {
            player_from_biota_with_character(
                env,
                Class::Player,
                b,
                Vec::new(),
                Vec::new(),
                character,
                Some(S),
            )
        });
        assert_eq!(p.is_advocate(), advocate, "{level:?}, quest {quest}");
    }
}

// ---- Player.TrackObject -------------------------------------------------------------------------

#[test]
fn track_object_sends_a_create_unless_the_object_is_server_only_or_the_player_itself() {
    let mut w = world();
    let mut p = WorldObject::allocate(Class::Player);
    p.guid = ObjectGuid::new(PLAYER);
    w.objects.insert(p).expect("fresh");
    w.sessions.insert(
        S,
        SessionData {
            player: Some(ObjectGuid::new(PLAYER)),
            ..Default::default()
        },
    );
    let mut add = |guid: u32, class: Class, hidden: bool| {
        let mut o = WorldObject::allocate(class);
        o.guid = ObjectGuid::new(guid);
        if hidden {
            o.set_property(PropertyBool::Visibility, true);
        }
        o.set_property(PropertyDataId::Setup, 0x0200_0001);
        w.objects.insert(o).expect("fresh");
    };
    add(0x8000_0001, Class::GenericObject, false);
    add(0x8000_0002, Class::GenericObject, true);
    add(0x8000_0003, Class::Creature, false);
    add(0x8000_0004, Class::Clothing, false); // the creature's wielded sword slot
    add(0x8000_0005, Class::Clothing, false); // its worn shirt
    add(0x8000_0006, Class::GenericObject, false); // arrows, with no missile weapon wielded
    for (item, loc) in [
        (0x8000_0004u32, EquipMask::MeleeWeapon),
        (0x8000_0005, EquipMask::ChestWear),
        (0x8000_0006, EquipMask::MissileAmmo),
    ] {
        w.objects
            .get_mut(ObjectGuid::new(item))
            .unwrap()
            .set_property(PropertyInt::CurrentWieldedLocation, loc.0.cast_signed());
    }
    set_equipped_objects(
        &mut w,
        ObjectGuid::new(0x8000_0003),
        &[
            ObjectGuid::new(0x8000_0004),
            ObjectGuid::new(0x8000_0005),
            ObjectGuid::new(0x8000_0006),
        ],
    );

    game_message::start_capture();
    for g in [PLAYER, 0x8000_0001, 0x8000_0002, 0x8000_0003, 0x8000_0099] {
        player_tracking::track_object(&mut w, ObjectGuid::new(PLAYER), ObjectGuid::new(g), false);
    }
    let sent: Vec<(u32, u32)> = game_message::take_sent()
        .iter()
        .map(|(_, _, b)| {
            (
                u32::from_le_bytes(b[0..4].try_into().unwrap()),
                u32::from_le_bytes(b[4..8].try_into().unwrap()),
            )
        })
        .collect();
    // Itself, the server-only object and a missing one: nothing. The creature: its create, then
    // its wielded item in a child location (Selectable); not the worn shirt, and not ammunition
    // while no missile weapon is wielded (IsInChildLocation).
    assert_eq!(
        sent,
        [
            (0xF745, 0x8000_0001),
            (0xF745, 0x8000_0003),
            (0xF745, 0x8000_0004)
        ]
    );
}

// ---- the login events' bodies (hand-derived from ACE) ------------------------------------------

/// A player with a Character holding two titles and two friends (one online), a session at
/// sequence 5.
fn event_world() -> (World, ObjectGuid) {
    let mut w = world();
    let guid = ObjectGuid::new(PLAYER);
    let mut p = WorldObject::allocate(Class::Player);
    p.guid = guid;
    p.biota.id = PLAYER;
    p.set_property(PropertyInt::CharacterTitleId, 7);
    let ch = Character {
        id: PLAYER,
        character_properties_title_book: vec![
            empyrean_store::models::shard::CharacterPropertiesTitleBook {
                character_id: PLAYER,
                title_id: 7,
            },
            empyrean_store::models::shard::CharacterPropertiesTitleBook {
                character_id: PLAYER,
                title_id: 3,
            },
        ],
        character_properties_friend_list: vec![
            empyrean_store::models::shard::CharacterPropertiesFriendList {
                character_id: PLAYER,
                friend_id: 0x5000_0002,
            },
            empyrean_store::models::shard::CharacterPropertiesFriendList {
                character_id: PLAYER,
                friend_id: 0x5000_0003,
            },
        ],
        ..Default::default()
    };
    p.player.as_mut().unwrap().player.character = Some(ch);
    w.objects.insert(p).expect("fresh");
    // Friend 2 is online (in the world with its name); friend 3 is unknown.
    let mut f = WorldObject::allocate(Class::Player);
    f.guid = ObjectGuid::new(0x5000_0002);
    f.biota.id = 0x5000_0002;
    f.set_property(
        empyrean_entity::enums::PropertyString::Name,
        "Bo".to_owned(),
    );
    f.player.as_mut().unwrap().player.character = Some(Character {
        id: 0x5000_0002,
        ..Default::default()
    });
    w.objects.insert(f).expect("fresh");
    w.player_manager.online_players.try_add(
        0x5000_0002,
        empyrean_world::managers::player_manager::OnlinePlayer {
            guid: ObjectGuid::new(0x5000_0002),
            account: None,
        },
    );
    w.sessions.insert(
        S,
        SessionData {
            player: Some(guid),
            game_event_sequence: 5,
            ..Default::default()
        },
    );
    (w, guid)
}

use crate::support::hex::hex;

#[test]
fn character_title_writes_the_title_book_in_its_order() {
    use empyrean_world::network::game_event::events::game_event_character_title::game_event_character_title;
    let (mut w, guid) = event_world();
    let m = game_event_character_title(&mut w, S);
    // header: GameEvent, player, sequence 5, CharacterTitle; then version 1, title 7, 2 titles.
    let e = [
        "B0F70000", "01000050", "05000000", "29000000", "01000000", "07000000", "02000000",
        "07000000", "03000000",
    ]
    .concat();
    assert_eq!(hex(&m.data), e);
    assert_eq!(
        w.objects.get(guid).unwrap().num_character_titles(),
        Some(2),
        "NumCharacterTitles is set"
    );
}

#[test]
fn friends_list_update_writes_each_friend_with_its_online_state_and_name() {
    use empyrean_world::network::game_event::events::game_event_friends_list_update::game_event_friends_list_update;
    let (mut w, _) = event_world();
    let m = game_event_friends_list_update(&mut w, S);
    let e = [
        "B0F70000", "01000050", "05000000", "21000000", // header
        "02000000", // two friends
        "02000050", "01000000", "00000000", "0200426F", "00000000",
        "00000000", // online, "Bo", no friends' friends, no inverse
        "03000050", "00000000", "00000000", "00000000", "00000000",
        "00000000", // offline and unknown: "" (length 0, 2 bytes padding)
        "00000000", // FullList
    ]
    .concat();
    assert_eq!(hex(&m.data), e);
}

#[test]
fn view_contents_lists_the_items_by_placement_with_their_container_type() {
    use empyrean_world::network::game_event::events::game_event_view_contents::game_event_view_contents;
    let (mut w, _) = event_world();
    let pack = ObjectGuid::new(0x8000_0100);
    let mut o = WorldObject::allocate(Class::Container);
    o.guid = pack;
    w.objects.insert(o).expect("fresh");
    set_inventory(
        &mut w,
        pack,
        &[
            ObjectGuid::new(0x8000_0101),
            ObjectGuid::new(0x8000_0102),
            ObjectGuid::new(0x8000_0103),
        ],
    );
    for (g, placement, class, foci) in [
        (0x8000_0101u32, Some(2), Class::GenericObject, false),
        (0x8000_0102, None, Class::Container, false),
        (0x8000_0103, Some(1), Class::GenericObject, true),
    ] {
        let mut i = WorldObject::allocate(class);
        i.guid = ObjectGuid::new(g);
        i.biota.weenie_type = if class == Class::Container {
            WeenieType::Container
        } else {
            WeenieType::Generic
        };
        if let Some(p) = placement {
            i.set_property(PropertyInt::PlacementPosition, p);
        }
        if foci {
            i.set_property(PropertyBool::RequiresBackpackSlot, true);
        }
        w.objects.insert(i).expect("fresh");
    }
    let m = game_event_view_contents(&mut w, S, pack);
    // By placement, null first: the pack (Container 1), the foci (2), the plain item (0).
    let e = [
        "B0F70000", "01000050", "05000000", "96010000", "00010080", "03000000", "02010080",
        "01000000", "03010080", "02000000", "01010080", "00000000",
    ]
    .concat();
    assert_eq!(hex(&m.data), e);
}

mod object_description_vectors {
    // Vector fields are C# values of the width the structure declares: narrowing them is intended.
    #![allow(clippy::cast_possible_truncation)]

    use empyrean_common::dotnet::DotNetDict;
    use empyrean_common::vectors::{self, f32_of, f64_of, i64_of, u64_of};
    use empyrean_dat::{DatDatabaseType, FakeDats};
    use empyrean_entity::enums::{
        ObjectDescriptionFlag, PositionType, PropertyAttribute, PropertyAttribute2nd, PropertyBool,
        PropertyDataId, PropertyFloat, PropertyInstanceId, PropertyInt, PropertyInt64,
        PropertyString, Skill, SkillAdvancementClass, WeenieType,
    };
    use empyrean_entity::models::{
        PropertiesAttribute, PropertiesAttribute2nd, PropertiesPosition, PropertiesSkill,
    };
    use empyrean_entity::ObjectGuid;
    use empyrean_store::models::shard::{
        CharacterPropertiesFillCompBook, CharacterPropertiesShortcutBar,
        CharacterPropertiesSpellBar,
    };
    use empyrean_world::network::game_event::events::game_event_player_description::game_event_player_description;
    use empyrean_world::network::game_messages::messages::{
        game_message_create_object, game_message_obj_desc_event, game_message_update_object,
    };
    use empyrean_world::network::sequence::sequence_type::SequenceType;
    use serde_json::Value;

    use super::*;

    use crate::support::hex::unhex;
    fn u(v: &Value) -> u32 {
        u32::try_from(u64_of(v).unwrap_or_else(|| panic!("uint: {v}"))).expect("u32")
    }
    fn i(v: &Value) -> i32 {
        i32::try_from(i64_of(v).unwrap_or_else(|| panic!("int: {v}"))).expect("i32")
    }
    fn f(v: &Value) -> f32 {
        f32_of(v).unwrap_or_else(|| panic!("float: {v}"))
    }
    fn arr(v: &Value) -> &Vec<Value> {
        v.as_array().unwrap_or_else(|| panic!("array: {v}"))
    }

    fn class_of(name: &str) -> Class {
        match name {
            "Player" => Class::Player,
            "GenericObject" => Class::GenericObject,
            "Container" => Class::Container,
            "Clothing" => Class::Clothing,
            c => panic!("no class {c}"),
        }
    }

    /// Builds the vector's object (and its equipped items and inventory) into `w`, as the harness
    /// built ACE's: the biota, the object description flags, and the containers' and creatures'
    /// collections.
    fn build(w: &mut World, spec: &Value) -> ObjectGuid {
        let guid = ObjectGuid::new(u(&spec["guid"]));
        let mut o = WorldObject::allocate(class_of(spec["class"].as_str().expect("class")));
        o.guid = guid;
        let bi = &mut o.biota;
        bi.id = guid.full();
        bi.weenie_class_id = u(&spec["wcid"]);
        bi.weenie_type = WeenieType(u(&spec["weenie_type"]));
        let pairs = |k: &str| {
            arr(&spec[k])
                .iter()
                .map(|p| arr(p).clone())
                .collect::<Vec<_>>()
        };
        bi.properties_int = Some(
            pairs("int")
                .iter()
                .map(|p| (PropertyInt(u(&p[0]) as u16), i(&p[1])))
                .collect(),
        );
        bi.properties_int64 = Some(
            pairs("int64")
                .iter()
                .map(|p| (PropertyInt64(u(&p[0]) as u16), i64_of(&p[1]).expect("long")))
                .collect(),
        );
        bi.properties_bool = Some(
            pairs("bool")
                .iter()
                .map(|p| (PropertyBool(u(&p[0]) as u16), p[1].as_bool().expect("bool")))
                .collect(),
        );
        bi.properties_float = Some(
            pairs("float")
                .iter()
                .map(|p| {
                    (
                        PropertyFloat(u(&p[0]) as u16),
                        f64_of(&p[1]).expect("double"),
                    )
                })
                .collect(),
        );
        bi.properties_string = Some(
            pairs("string")
                .iter()
                .map(|p| {
                    (
                        PropertyString(u(&p[0]) as u16),
                        p[1].as_str().expect("s").to_owned(),
                    )
                })
                .collect(),
        );
        bi.properties_did = Some(
            pairs("did")
                .iter()
                .map(|p| (PropertyDataId(u(&p[0]) as u16), u(&p[1])))
                .collect(),
        );
        bi.properties_iid = Some(
            pairs("iid")
                .iter()
                .map(|p| (PropertyInstanceId(u(&p[0]) as u16), u(&p[1])))
                .collect(),
        );
        bi.properties_position = Some(
            pairs("positions")
                .iter()
                .map(|p| {
                    let pos = PropertiesPosition {
                        obj_cell_id: u(&p[1]),
                        position_x: f(&p[2]),
                        position_y: f(&p[3]),
                        position_z: f(&p[4]),
                        rotation_w: f(&p[5]),
                        rotation_x: f(&p[6]),
                        rotation_y: f(&p[7]),
                        rotation_z: f(&p[8]),
                    };
                    (PositionType(u(&p[0]) as u16), pos)
                })
                .collect(),
        );
        let spells = pairs("spell_book");
        if !spells.is_empty() {
            bi.properties_spell_book = Some(spells.iter().map(|p| (i(&p[0]), f(&p[1]))).collect());
        }
        bi.properties_attribute = Some(DotNetDict::new());
        bi.properties_attribute_2nd = Some(DotNetDict::new());
        bi.properties_skill = Some(DotNetDict::new());
        for a in pairs("attributes") {
            let id = u(&a[0]);
            if id < 100 {
                let v = PropertiesAttribute {
                    init_level: u(&a[1]),
                    level_from_cp: u(&a[2]),
                    cp_spent: u(&a[3]),
                };
                bi.properties_attribute
                    .as_mut()
                    .expect("set")
                    .insert(PropertyAttribute(id as u16), v);
            } else {
                let v = PropertiesAttribute2nd {
                    init_level: u(&a[1]),
                    level_from_cp: u(&a[2]),
                    cp_spent: u(&a[3]),
                    current_level: u(&a[4]),
                };
                bi.properties_attribute_2nd
                    .as_mut()
                    .expect("set")
                    .insert(PropertyAttribute2nd((id - 100) as u16), v);
            }
        }
        for s in pairs("skills") {
            let v = PropertiesSkill {
                level_from_pp: u(&s[1]) as u16,
                sac: SkillAdvancementClass(u(&s[2])),
                pp: u(&s[3]),
                init_level: u(&s[4]),
                ..Default::default()
            };
            bi.properties_skill
                .as_mut()
                .expect("set")
                .insert(Skill(i(&s[0])), v);
        }
        o.wo.world_object.object_description_flags = ObjectDescriptionFlag(i(&spec["odf"]));
        w.objects.insert(o).expect("fresh guid");

        let equipped: Vec<ObjectGuid> =
            arr(&spec["equipped"]).iter().map(|e| build(w, e)).collect();
        let inventory: Vec<ObjectGuid> = arr(&spec["inventory"])
            .iter()
            .map(|e| build(w, e))
            .collect();
        super::set_equipped_objects(w, guid, &equipped);
        super::set_inventory(w, guid, &inventory);
        guid
    }

    /// The player's shard `Character` from the vector's description.
    fn character(guid: ObjectGuid, c: &Value) -> Character {
        let id = guid.full();
        Character {
            id,
            character_options_1: i(&c["options1"]),
            character_options_2: i(&c["options2"]),
            default_hair_texture: u(&c["default_hair"]),
            hair_texture: u(&c["hair"]),
            is_plussed: c["plussed"].as_bool().expect("plussed"),
            spellbook_filters: u(&c["spellbook_filters"]),
            gameplay_options: c["gameplay_options"].as_str().map(unhex),
            character_properties_shortcut_bar: arr(&c["shortcuts"])
                .iter()
                .map(|s| CharacterPropertiesShortcutBar {
                    character_id: id,
                    shortcut_bar_index: u(&s[0]),
                    shortcut_object_id: u(&s[1]),
                })
                .collect(),
            character_properties_spell_bar: arr(&c["spell_bars"])
                .iter()
                .map(|s| CharacterPropertiesSpellBar {
                    character_id: id,
                    spell_bar_number: u(&s[0]),
                    spell_bar_index: u(&s[1]),
                    spell_id: u(&s[2]),
                })
                .collect(),
            character_properties_fill_comp_book: arr(&c["fill_comps"])
                .iter()
                .map(|s| CharacterPropertiesFillCompBook {
                    character_id: id,
                    spell_component_id: i(&s[0]),
                    quantity_to_rebuy: i(&s[1]),
                })
                .collect(),
            ..Default::default()
        }
    }

    fn world_for(case: &vectors::Case) -> World {
        let mut dats = FakeDats::new();
        if let Some(files) = case.input["dats"].as_array() {
            for e in files {
                let e = arr(e);
                let bytes = unhex(e[1].as_str().expect("hex"));
                if !bytes.is_empty() {
                    dats = dats.with_raw(DatDatabaseType::Portal, u(&e[0]), bytes);
                }
            }
        }
        let now = ClockSnapshot {
            portal_year_ticks: 0.0,
            unix_time: 1_000_000.0,
            utc: DotNetDateTime::new(2026, 1, 1),
            monotonic: Duration::ZERO,
        };
        World::new(now, dats.build().expect("fake dats"))
    }

    /// Builds the case's player with its Character and a session (no player yet) whose next event
    /// is `seq`.
    fn player_for(case: &vectors::Case, seq: u32) -> (World, ObjectGuid) {
        let mut w = world_for(case);
        let guid = build(&mut w, &case.input["object"]);
        let ch = character(guid, &case.input["character"]);
        w.objects
            .get_mut(guid)
            .expect("built")
            .player
            .as_mut()
            .expect("a player")
            .player
            .character = Some(ch);
        w.sessions.insert(
            S,
            SessionData {
                player: Some(guid),
                game_event_sequence: seq,
                ..Default::default()
            },
        );
        (w, guid)
    }

    #[test]
    fn player_descriptions_match_aces_writer() {
        let file = vectors::load_named("players", "player_description");
        assert!(file.cases.len() >= 20);
        for (n, case) in file.cases.iter().enumerate() {
            let (mut w, _) = player_for(case, u(&case.input["seq"]));
            let m = game_event_player_description(&mut w, S);
            assert_eq!(
                hex(&m.data),
                case.output["hex"].as_str().expect("hex"),
                "case {n}"
            );
            assert_eq!(
                w.sessions.get(S).expect("session").game_event_sequence,
                u(&case.output["seq_after"]),
                "case {n}"
            );
        }
    }

    const ALL_SEQUENCES: [SequenceType; 10] = [
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
    ];

    #[test]
    fn player_object_descriptions_match_aces_writers() {
        let file = vectors::load_named("players", "player_objects");
        assert!(file.cases.len() >= 4);
        for (n, case) in file.cases.iter().enumerate() {
            let (mut w, guid) = player_for(case, 1);
            let create =
                game_message_create_object::game_message_create_object(&mut w, guid, false, false);
            assert_eq!(
                hex(&create.data),
                case.output["create"].as_str().expect("create"),
                "case {n}: create"
            );
            let admin =
                game_message_create_object::game_message_create_object(&mut w, guid, true, true);
            assert_eq!(
                hex(&admin.data),
                case.output["create_admin"].as_str().expect("admin"),
                "case {n}: create adminvision"
            );
            let update =
                game_message_update_object::game_message_update_object(&mut w, guid, false, false);
            assert_eq!(
                hex(&update.data),
                case.output["update"].as_str().expect("update"),
                "case {n}: update"
            );
            let obj_desc = game_message_obj_desc_event::game_message_obj_desc_event(&mut w, guid);
            assert_eq!(
                hex(&obj_desc.data),
                case.output["obj_desc"].as_str().expect("obj desc"),
                "case {n}: obj desc"
            );
            let seqs: Vec<String> = ALL_SEQUENCES
                .iter()
                .map(|t| {
                    hex(&w
                        .objects
                        .get_mut(guid)
                        .expect("built")
                        .sequences
                        .get_current_sequence(*t))
                })
                .collect();
            let expected: Vec<String> = arr(&case.output["seq"])
                .iter()
                .map(|s| s.as_str().expect("seq").to_owned())
                .collect();
            assert_eq!(seqs, expected, "case {n}: sequences");
        }
    }
}

// ---- real content -------------------------------------------------------------------------------

/// The real-content tier (`--features real-content`): the retail dats under `DERETH_TEST_DAT_DIR`,
/// `world.pack` under `EMPYREAN_TEST_WORLD_PACK` (default
/// `world.pack` in the repository) and `fixtures/packet-captures/early-inventory-and-casting.jsonl`. Nothing from the
/// capture is written anywhere: it is decoded at run time and compared as opcodes and structure.
#[cfg(feature = "real-content")]
mod real_content {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use dereth_primitives::ObjectId;
    use dereth_primitives::{IncomingMessage, NetQueue};
    use dereth_protocol as proto;
    use dereth_protocol::login::{
        CharGenVerificationResponse, CharacterSendCharGenResult, LoginEnterGameServerReady,
        LoginSendEnterWorld, LoginSendEnterWorldRequest,
    };
    use empyrean_content::PackContent;
    use empyrean_dat::{DatManager, RealDats};
    use empyrean_testkit::{ClientId, TestServer};
    use empyrean_world::managers::guid_manager::{self, ShardGuidQueries};
    use empyrean_world::managers::world_manager::WorldStatusState;
    use empyrean_world::physics::phys_ext;

    use super::*;

    fn dats() -> Arc<DatManager> {
        let dir = dereth_dat::testing::dat_dir();
        let source = RealDats::open(&dir).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs the retail dats under {} (set DERETH_TEST_DAT_DIR): {e}",
                dir.display()
            )
        });
        DatManager::initialize(Arc::new(source)).expect("retail dats")
    }

    fn pack() -> PackContent {
        let path = empyrean_common::test_paths::world_pack();
        PackContent::open(&path).unwrap_or_else(|e| {
            panic!(
                "the real-content tier needs world.pack at {} (set EMPYREAN_TEST_WORLD_PACK): {e}",
                path.display()
            )
        })
    }

    struct EmptyShard;

    impl ShardGuidQueries for EmptyShard {
        fn get_max_guid_found_in_range(&mut self, _min: u32, _max: u32) -> u32 {
            u32::MAX
        }
        fn get_sequence_gaps(&mut self, _min: u32, _limit: u32) -> Vec<(u32, u32)> {
            Vec::new()
        }
    }

    /// One message of a recorded session: direction, queue, blob sequence and payload (opcode
    /// included).
    struct Recorded {
        c2s: bool,
        seq: u32,
        payload: Vec<u8>,
    }

    /// The recording's messages, reassembled from their fragments, in arrival order.
    fn recording(name: &str) -> Vec<Recorded> {
        use dereth_client_net::client_session::testing::{Corpus, Direction};
        Corpus::shared(name)
            .blobs
            .iter()
            .map(|blob| Recorded {
                c2s: blob.dir == Direction::ClientToServer,
                seq: u32::try_from(blob.blob_id & u64::from(u32::MAX)).expect("low sequence word"),
                payload: blob.payload.clone(),
            })
            .collect()
    }

    /// The recorded session's first world entry: the server's messages after the client's
    /// `Login_SendEnterWorld` and before the client's first game action, in the server's send
    /// order; and the recorded `Character_SendCharGenResult` before it.
    fn recorded_entry(name: &str) -> (CharacterSendCharGenResult, Vec<Recorded>) {
        let all = recording(name);
        let chargen = all
            .iter()
            .find(|m| m.c2s && m.payload.starts_with(&0xF656u32.to_le_bytes()))
            .expect("a character creation");
        let chargen: CharacterSendCharGenResult =
            proto::read_body_padded(&chargen.payload[4..]).expect("decodes");
        let start = all
            .iter()
            .position(|m| m.c2s && m.payload.starts_with(&0xF657u32.to_le_bytes()))
            .expect("an enter-world");
        let mut entry: Vec<Recorded> = Vec::new();
        for m in all.into_iter().skip(start + 1) {
            if m.c2s {
                if m.payload.starts_with(&0xF7B1u32.to_le_bytes()) {
                    break;
                }
                continue;
            }
            entry.push(m);
        }
        entry.sort_by_key(|m| m.seq);
        (chargen, entry)
    }

    fn opcode_label(payload: &[u8]) -> String {
        let op = u32::from_le_bytes(payload[0..4].try_into().expect("opcode"));
        if op == 0xF7B0 {
            format!(
                "ev {:04X}",
                u32::from_le_bytes(payload[12..16].try_into().expect("event"))
            )
        } else {
            format!("{op:04X}")
        }
    }

    /// The structural token of a message: its label, with a create split into the player's own,
    /// a possession (in the player's inventory, a pack of it, or wielded by it), a static object
    /// (a landblock instance guid, below the dynamic range) or a spawned one.
    fn token(payload: &[u8], player: u32, possessions: &mut BTreeSet<u32>) -> String {
        let label = opcode_label(payload);
        if label != "F745" {
            return label;
        }
        let create: dereth_protocol::objects::ItemCreateObject =
            proto::read_body_padded(&payload[4..]).expect("create decodes");
        let guid = create.0.id.0;
        let owner = create
            .0
            .wdesc
            .container_id
            .or(create.0.wdesc.wielder_id)
            .map(|o| o.0);
        if guid == player {
            "F745 self".to_owned()
        } else if owner.is_some_and(|o| o == player || possessions.contains(&o)) {
            possessions.insert(guid);
            "F745 possession".to_owned()
        } else if guid < 0x8000_0000 {
            "F745 static".to_owned()
        } else {
            "F745 spawn".to_owned()
        }
    }

    fn payload_of(m: &IncomingMessage) -> Vec<u8> {
        let mut v = m.opcode.to_le_bytes().to_vec();
        v.extend_from_slice(&m.body);
        v
    }

    /// Decodes a message (opcode included) with dereth-protocol, by opcode or event type; panics on
    /// any it does not know or that does not decode.
    fn decode(payload: &[u8]) {
        use dereth_protocol::{admin, comms, login, objects, qualities, social, Message};
        fn dec<M: Message + std::fmt::Debug>(label: &str, body: &[u8]) {
            proto::read_body_padded::<M>(body)
                .unwrap_or_else(|e| panic!("{label} does not decode: {e:?}"));
        }
        let label = opcode_label(payload);
        let (bare, event) = (&payload[4..], &payload[payload.len().min(16)..]);
        match label.as_str() {
            "F746" => dec::<objects::LoginCreatePlayer>(&label, bare),
            "F745" => dec::<objects::ItemCreateObject>(&label, bare),
            "F755" => dec::<objects::EffectsPlayScriptType>(&label, bare),
            "F7E0" => dec::<comms::CommunicationTextboxString>(&label, bare),
            "02CD" => dec::<qualities::QualitiesPrivateUpdateInt>(&label, bare),
            "EA60" => dec::<admin::AdminEnvirons>(&label, bare),
            "ev 0013" => dec::<login::LoginPlayerDescription>(&label, event),
            "ev 0029" => dec::<social::CharacterTitlesMessage>(&label, event),
            "ev 0021" => dec::<social::SocialFriendsUpdate>(&label, event),
            "ev 0196" => dec::<objects::ItemOnViewContents>(&label, event),
            "ev 028A" => dec::<comms::CommunicationWeenieError>(&label, event),
            "ev 028B" => dec::<comms::CommunicationWeenieErrorWithString>(&label, event),
            "ev 0295" => dec::<comms::ChatRoomMembership>(&label, event),
            "ev 0004" => dec::<comms::CommunicationPopUpString>(&label, event),
            other => panic!("no decoder for {other}"),
        }
    }

    fn guid_in(payload: &[u8]) -> u32 {
        u32::from_le_bytes(payload[4..8].try_into().expect("guid"))
    }

    /// The recorded account's level: its PlayerDescription carries `IsAdmin` and WeenieType Admin,
    /// which `DoPlayerEnterWorld` gives an Admin-level session's character (with the default
    /// `OverrideCharacterPermissions`).
    fn access_level() -> AccessLevel {
        AccessLevel::Admin
    }

    pub(super) fn recorded_chargen(name: &str) -> CharacterSendCharGenResult {
        recorded_entry(name).0
    }

    pub(super) fn created_for_tests(
        chargen: &CharacterSendCharGenResult,
    ) -> (TestServer, ClientId, u32) {
        created(chargen)
    }

    /// A server on the retail dats and pack, the recorded character created over the wire through
    /// a bot logged in as the recorded account (its name read at run time, never written).
    fn created(chargen: &CharacterSendCharGenResult) -> (TestServer, ClientId, u32) {
        let mut ts = TestServer::with_dats(dats());
        ts.world.content = Arc::new(pack());
        ts.world.world_manager.world_status = WorldStatusState::Open;
        guid_manager::initialize(&mut ts.world, &mut EmptyShard);
        ts.auth()
            .create_account(
                &chargen.account,
                "pw",
                access_level(),
                std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            )
            .expect("created");
        let id = ts.connect(&chargen.account, "pw");
        assert!(ts.run_until(1.0, |ts| ts
            .world
            .sessions
            .iter()
            .any(|(_, s)| s.state == empyrean_net::SessionState::AuthConnected)));
        ts.send_message(id, NetQueue::Logon, chargen);
        ts.advance(0.5);
        let r = ts.received::<CharGenVerificationResponse>(id);
        assert_eq!(
            r.iter().map(|r| r.response_type).collect::<Vec<_>>(),
            [1],
            "created"
        );
        let guid = r[0].identity.gid.0;
        (ts, id, guid)
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn the_recorded_character_enters_the_world_as_the_recorded_ace_session_did() {
        use dereth_protocol::login::LoginPlayerDescription;
        use dereth_protocol::objects::ItemCreateObject;

        let (chargen, recorded) = recorded_entry("early-inventory-and-casting");
        let (mut ts, id, guid) = created(&chargen);

        ts.send_message(id, NetQueue::Logon, &LoginSendEnterWorldRequest);
        ts.advance(0.1);
        assert_eq!(ts.received::<LoginEnterGameServerReady>(id).len(), 1);
        let from = ts.received_raw(id).len();
        ts.send_message(
            id,
            NetQueue::Logon,
            &LoginSendEnterWorld {
                character: ObjectId(guid),
                account: chargen.account.clone(),
            },
        );
        ts.advance(0.5);

        // Our messages in the server's send order (the blob sequence), as the recording's are.
        let mut ours: Vec<&IncomingMessage> = ts.received_raw(id)[from..].iter().collect();
        ours.sort_by_key(|m| m.blob_id.low32());
        let ours: Vec<Vec<u8>> = ours.into_iter().map(payload_of).collect();
        let theirs: Vec<Vec<u8>> = recorded.iter().map(|m| m.payload.clone()).collect();

        // Every message we sent decodes as the client reads it.
        for m in &ours {
            decode(m);
        }

        let recorded_player = theirs
            .iter()
            .find(|m| opcode_label(m) == "F746")
            .map(|m| guid_in(m))
            .expect("PlayerCreate");
        let tokens = |v: &[Vec<u8>], player: u32| {
            let mut possessions = BTreeSet::new();
            v.iter()
                .map(|m| token(m, player, &mut possessions))
                .collect::<Vec<String>>()
        };
        let theirs_tokens = tokens(&theirs, recorded_player);
        let ours_tokens = tokens(&ours, guid);

        // Three ruled changes to ACE's recorded sequence (the rest stays ACE's record):
        // - (V280): the Age update (0x02CD) is no longer sent at login, right after the
        //   welcome text, but by the player's first heartbeat 0-5 s in; it is dropped from ACE's
        //   record, and ours may carry it only once, where a heartbeat fell.
        // - V260/V278/V286/V287/V308 stage 2c (V286): a PlayScript (0xF755) goes to every player in the object's
        //   landblock and its adjacents, known or not, so ours carries the PlayScript.Create of
        //   spawns in reach that are outside the player's create set (V260/V278/V286/V287/V308 stage 2b, V287) and so
        //   were never created for it. Each is checked to be one, then set aside.
        assert!(
            theirs_tokens
                .iter()
                .filter(|t| t.as_str() == "02CD")
                .count()
                == 1,
            "ACE sent the Age at login"
        );
        assert!(
            ours_tokens.iter().filter(|t| t.as_str() == "02CD").count() <= 1,
            "at most one heartbeat's Age"
        );
        let created_for_us: BTreeSet<u32> = ours
            .iter()
            .zip(&ours_tokens)
            .filter(|(_, t)| t.starts_with("F745"))
            .map(|(m, _)| guid_in(m))
            .collect();
        let player_phys = ts
            .world
            .objects
            .get(ObjectGuid::new(guid))
            .and_then(|p| p.phys)
            .expect("the player's body");
        let mut unsent_scripts = 0;
        let mut kept = Vec::new();
        for (m, t) in ours.iter().zip(&ours_tokens) {
            if t == "02CD" {
                continue;
            }
            if t == "F755" && !created_for_us.contains(&guid_in(m)) {
                let obj = ObjectGuid::new(guid_in(m));
                let o = ts.world.objects.get(obj).unwrap_or_else(|| {
                    panic!("PlayScript about {:08X}, not in the world", obj.full())
                });
                assert!(
                    empyrean_world::world_objects::world_object_networking::reach_players(
                        &ts.world, obj
                    )
                    .contains(&ObjectGuid::new(guid)),
                    "PlayScript about unsent {:08X}: the player is in its reach",
                    obj.full()
                );
                let h = o.phys.expect("a spawn's body");
                assert!(
                    !empyrean_world::physics::object_maint::in_create_set(
                        &ts.world,
                        player_phys,
                        h
                    ),
                    "PlayScript about unsent {:08X}: outside the player's create set",
                    obj.full()
                );
                unsent_scripts += 1;
                continue;
            }
            kept.push(t.clone());
        }
        println!("PlayScripts about spawns in reach but not created: {unsent_scripts}");
        let ours_tokens_ruled = kept;
        let theirs_tokens_ruled: Vec<String> = theirs_tokens
            .iter()
            .filter(|t| t.as_str() != "02CD")
            .cloned()
            .collect();
        let wcid = |m: &Vec<u8>| {
            proto::read_body_padded::<ItemCreateObject>(&m[4..])
                .expect("create")
                .0
                .wdesc
                .wcid
        };
        let of_kind = |v: &[Vec<u8>], t: &[String], kind: &str| -> Vec<Vec<u8>> {
            v.iter()
                .zip(t)
                .filter(|(_, k)| k.as_str() == kind)
                .map(|(m, _)| m.clone())
                .collect()
        };

        let spawns = of_kind(&theirs, &theirs_tokens, "F745 spawn");
        assert!(!spawns.is_empty());
        let spawn_at: Vec<usize> = theirs_tokens
            .iter()
            .enumerate()
            .filter(|(_, t)| t.as_str() == "F745 spawn")
            .map(|(i, _)| i)
            .collect();
        for &i in &spawn_at {
            assert_eq!(
                theirs_tokens.get(i + 1).map(String::as_str),
                Some("F755"),
                "a spawn's PlayScript.Create follows its create"
            );
        }
        let mut link_children = BTreeSet::new();
        for lb in empyrean_world::managers::landblock_manager::get_loaded_landblocks(&ts.world) {
            let instances = ts.world.content.get_cached_instances_by_landblock(
                u16::try_from(lb.raw() >> 16).expect("landblock"),
            );
            let children: BTreeSet<u32> = instances
                .iter()
                .flat_map(|i| i.landblock_instance_link.iter().map(|l| l.child_guid))
                .collect();
            link_children.extend(
                instances
                    .iter()
                    .filter(|i| children.contains(&i.guid))
                    .map(|i| i.weenie_class_id),
            );
        }
        for s in &spawns {
            assert!(
                link_children.contains(&wcid(s)),
                "spawn wcid {} is a linked child's weenie",
                wcid(s)
            );
        }
        let mut theirs_spawns: Vec<u32> = spawns.iter().map(wcid).collect();
        let mut ours_spawns: Vec<u32> = of_kind(&ours, &ours_tokens, "F745 spawn")
            .iter()
            .map(wcid)
            .collect();
        theirs_spawns.sort_unstable();
        ours_spawns.sort_unstable();
        assert_eq!(ours_spawns, theirs_spawns, "the same linked spawns");

        let mut theirs_items: Vec<u32> = of_kind(
            &theirs,
            &tokens(&theirs, recorded_player),
            "F745 possession",
        )
        .iter()
        .map(wcid)
        .collect();
        let mut ours_items: Vec<u32> = of_kind(&ours, &ours_tokens, "F745 possession")
            .iter()
            .map(wcid)
            .collect();
        let gift = u32::from(
            empyrean_entity::enums::WeenieClassName::W_GEMACTDPURCHASEREWARDARMOR_CLASS.0,
        );
        assert!(ours_items.contains(&gift), "our pre-order gift");
        theirs_items.sort_unstable();
        ours_items.sort_unstable();
        assert_eq!(ours_items, theirs_items, "the same possessions");

        // The ordered opcodes are the same: the UI queue (PlayerDescription,
        // title, friends, ViewContents, the chat channels, the popup, the welcome text),
        // then the Smartbox queue (PlayerCreate, the player, its possessions, the static objects),
        // with the three ruled changes above applied.
        assert_eq!(ours_tokens_ruled, theirs_tokens_ruled);

        // The static objects are the same world-DB instances in the same order (ObjectMaint's).
        let statics = |v: &[Vec<u8>], t: &[String]| {
            of_kind(v, t, "F745 static")
                .iter()
                .map(|m| guid_in(m))
                .collect::<Vec<u32>>()
        };
        let theirs_statics = statics(&theirs, &tokens(&theirs, recorded_player));
        assert!(!theirs_statics.is_empty());
        assert_eq!(statics(&ours, &ours_tokens), theirs_statics);

        let pd = |v: &[Vec<u8>]| -> LoginPlayerDescription {
            let m = v
                .iter()
                .find(|m| opcode_label(m) == "ev 0013")
                .expect("PlayerDescription");
            proto::read_body_padded(&m[16..]).expect("decodes")
        };
        let (a, b) = (pd(&theirs), pd(&ours));
        let keys = |q: &LoginPlayerDescription| {
            let t = &q.qualities.base.tables;
            let k =
                |h: Option<Vec<u32>>| h.unwrap_or_default().into_iter().collect::<BTreeSet<u32>>();
            [
                k(t.ints
                    .as_ref()
                    .map(|h| h.entries.iter().map(|e| e.0).collect())),
                k(t.int64s
                    .as_ref()
                    .map(|h| h.entries.iter().map(|e| e.0).collect())),
                k(t.bools
                    .as_ref()
                    .map(|h| h.entries.iter().map(|e| e.0).collect())),
                k(t.floats
                    .as_ref()
                    .map(|h| h.entries.iter().map(|e| e.0).collect())),
                k(t.strings
                    .as_ref()
                    .map(|h| h.entries.iter().map(|e| e.0).collect())),
                k(t.dids
                    .as_ref()
                    .map(|h| h.entries.iter().map(|e| e.0).collect())),
                k(t.iids
                    .as_ref()
                    .map(|h| h.entries.iter().map(|e| e.0).collect())),
                k(t.positions
                    .as_ref()
                    .map(|h| h.entries.iter().map(|e| e.0).collect())),
            ]
        };
        assert_eq!(keys(&b), keys(&a));
        let coin_value = |q: &LoginPlayerDescription| {
            q.qualities
                .base
                .tables
                .ints
                .as_ref()
                .and_then(|h| h.entries.iter().find(|e| e.0 == 20).map(|e| e.1))
        };
        assert_eq!(coin_value(&b), coin_value(&a), "CoinValue");
        assert_eq!(
            (
                b.qualities.base.weenie_type,
                b.qualities.flags,
                b.qualities.base.flags
            ),
            (
                a.qualities.base.weenie_type,
                a.qualities.flags,
                a.qualities.base.flags
            )
        );
        let n = |q: &LoginPlayerDescription| {
            (
                q.qualities.skills.as_ref().map(|s| s.entries.len()),
                q.qualities.spell_book.as_ref().map(|s| s.entries.len()),
            )
        };
        assert_eq!(n(&b), n(&a));
        assert_eq!(
            (b.content_profiles.len(), b.inventory_placements.len()),
            (a.content_profiles.len(), a.inventory_placements.len())
        );
        assert_eq!(
            (
                b.player_module.options,
                b.player_module.options2,
                b.player_module.option_flags
            ),
            (
                a.player_module.options,
                a.player_module.options2,
                a.player_module.option_flags
            )
        );
        assert_eq!(
            (a.player_module.spell_filters, b.player_module.spell_filters),
            (16383, 16383)
        );

        // The player's own create: the same weenie header flags and the login physics state.
        let own = |v: &[Vec<u8>], player: u32| {
            let m = v
                .iter()
                .find(|m| opcode_label(m) == "F745" && guid_in(m) == player)
                .expect("self");
            let c: ItemCreateObject = proto::read_body_padded(&m[4..]).expect("create");
            (c.0.wdesc.bitfield, c.0.wdesc.header, c.0.physicsdesc.state)
        };
        assert_eq!(own(&ours, guid), own(&theirs, recorded_player));

        // The v1 lesson: the body is placed at the chargen start, in a cell the client can place.
        let p = ts
            .world
            .objects
            .get(ObjectGuid::new(guid))
            .expect("in the world");
        let h = p.phys.expect("a body");
        let cell = phys_ext::cur_cell(&ts.world, h).expect("the body is placed in a cell");
        assert_eq!(Some(cell.0), p.location().map(|l| l.cell()));
    }
}

#[cfg(feature = "real-content")]
mod real_content_placement {
    use dereth_primitives::NetQueue;
    use dereth_primitives::ObjectId;
    use dereth_protocol::login::{LoginSendEnterWorld, LoginSendEnterWorldRequest};
    use empyrean_world::physics::phys_ext;

    use super::real_content::{created_for_tests, recorded_chargen};
    use super::*;

    /// Enters the recorded character after moving its saved Location to `cell` (x, y, z); returns
    /// the cell its body was placed in and its Location.
    fn enter_at(
        cell: u32,
        x: f32,
        y: f32,
        z: f32,
    ) -> (Option<u32>, Option<u32>, Option<empyrean_entity::Position>) {
        let chargen = recorded_chargen("early-inventory-and-casting");
        let (mut ts, id, guid) = created_for_tests(&chargen);
        let sanctuary = {
            let o = ts
                .world
                .player_manager
                .offline_players
                .get_mut(&guid)
                .expect("offline");
            let s = o
                .biota
                .properties_position
                .as_ref()
                .and_then(|p| p.get(&PositionType::Sanctuary).cloned());
            o.biota.set_property_position(
                PositionType::Location,
                PropertiesPosition {
                    obj_cell_id: cell,
                    position_x: x,
                    position_y: y,
                    position_z: z,
                    rotation_w: 1.0,
                    ..Default::default()
                },
            );
            s.map(|s| s.obj_cell_id)
        };
        ts.send_message(id, NetQueue::Logon, &LoginSendEnterWorldRequest);
        ts.advance(0.1);
        ts.send_message(
            id,
            NetQueue::Logon,
            &LoginSendEnterWorld {
                character: ObjectId(guid),
                account: chargen.account.clone(),
            },
        );
        ts.advance(0.5);
        let p = ts
            .world
            .objects
            .get(ObjectGuid::new(guid))
            .expect("in the world");
        let placed = p
            .phys
            .and_then(|h| phys_ext::cur_cell(&ts.world, h))
            .map(|c| c.0);
        (placed, sanctuary, p.location())
    }

    /// The v1 lesson on real data: the client refuses to place a body in deep sea (landblock
    /// `0x5368` is all `WaterDeepSea`), and so does the retail-faithful physics, so
    /// `LandblockManager.AddObject` fails and ACE's relocation moves the player to its lifestone
    /// (the chargen start). A cell that does not exist is caught the same way; an ordinary outdoor
    /// cell is kept.
    #[test]
    fn a_saved_position_in_deep_sea_or_a_missing_cell_is_moved_to_the_lifestone() {
        for (cell, z) in [
            (0x5368_0001u32, 0.0f32),
            (0x5368_0001, 200.0),
            (0x5368_FFFF, 0.0),
        ] {
            let (placed, sanctuary, location) = enter_at(cell, 12.0, 12.0, z);
            assert!(sanctuary.is_some());
            assert_eq!(
                placed, sanctuary,
                "{cell:08X} at z {z}: relocated to the lifestone"
            );
            assert_eq!(location.map(|l| l.cell()), sanctuary);
        }
        let (placed, sanctuary, _) = enter_at(0xA9B4_0019, 84.0, 7.1, 94.005);
        assert_eq!(placed, Some(0xA9B4_0019), "Holtburg is kept");
        assert_ne!(sanctuary, Some(0xA9B4_0019));
    }
}

mod login_completion {
    use crate::support::navigation_world::*;

    /// `GameActionLoginComplete`: `OnTeleportComplete` takes the player out of portal space; the first
    /// time, `FirstEnterWorldDone` is set and the property updates and overrides are sent (only then).
    #[test]
    fn login_complete_ends_portal_space_and_sends_the_first_property_updates_once() {
        let mut h = H::new();
        empyrean_world::managers::property_manager::modify_bool(&h.w, "require_spell_comps", false);
        lm::get_landblock(&mut h.w, lb_id(), false, false);
        h.player(100.0, 110.0);
        h.run(0.5);
        let dest = h.pos(PLAYER);
        player_location::teleport(&mut h.w, PLAYER, &dest, false);
        assert!(h.o(PLAYER).wo.world_object.teleporting);
        assert!(!h.o(PLAYER).first_enter_world_done());

        start_capture();
        action(&mut h.w, &CharacterLoginCompleteNotification);
        h.run(0.5);
        let first = take_sent().len();
        assert!(
            !h.o(PLAYER).wo.world_object.teleporting,
            "OnTeleportComplete"
        );
        assert!(h.o(PLAYER).first_enter_world_done());

        start_capture();
        action(&mut h.w, &CharacterLoginCompleteNotification);
        h.run(0.5);
        let second = take_sent().len();
        assert!(
            second < first,
            "the property updates only the first time: {first} then {second}"
        );
    }
}

mod tracking {
    use crate::support::quest_world::*;

    /// `Player.AddTrackedObject` (reached from `NotifyPlayers`): an object the player already knows is
    /// skipped; an unknown one becomes known and visible and is sent (`TrackObject`: its CreateObject).
    #[test]
    fn add_tracked_object_skips_known_objects_and_tracks_new_ones() {
        let mut h = Arena::new();
        lm::get_landblock(&mut h.w, lb_id(), false, false);
        h.player(100.0, 110.0);
        h.monster(100.0, 100.0);
        let me = phys_ext::physics_obj(&h.w, ARENA_PLAYER).unwrap();
        let it = phys_ext::physics_obj(&h.w, MONSTER).unwrap();
        assert!(
            object_maint::known_objects_contains_value(&h.w, me, it),
            "the physics already made it known"
        );

        start_capture();
        assert!(!player_tracking::add_tracked_object(
            &mut h.w,
            ARENA_PLAYER,
            MONSTER
        ));
        assert!(take_sent().is_empty());

        object_maint::remove_known_object(&mut h.w, me, it, false);
        object_maint::remove_visible_object(&mut h.w, me, it, false);
        start_capture();
        assert!(player_tracking::add_tracked_object(
            &mut h.w,
            ARENA_PLAYER,
            MONSTER
        ));
        let sent: Vec<u32> = take_sent().iter().map(|(_, _, b)| u32_at(b, 0)).collect();
        assert_eq!(sent.first(), Some(&CREATE_OBJECT), "{sent:04X?}");
        assert!(object_maint::known_objects_contains_value(&h.w, me, it));
        assert!(object_maint::visible_objects_contains_key(
            &h.w,
            me,
            MONSTER.full()
        ));
    }
}
