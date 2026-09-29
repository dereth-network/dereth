//! Persisted characters for commands entered through chat.
use empyrean_common::dotnet::DotNetDict;
use empyrean_entity::enums::{
    AccessLevel, PositionType, PropertyAttribute, PropertyAttribute2nd, PropertyBool,
    PropertyDataId, PropertyInt, PropertyInt64, PropertyString, WeenieType,
};
use empyrean_entity::models::{PropertiesAttribute, PropertiesAttribute2nd, PropertiesPosition};
use empyrean_entity::Position;
use empyrean_store::models::shard::Character;
use empyrean_testkit::{land, TestServer};
use std::net::{IpAddr, Ipv4Addr};

/// A character in the shard: 100 in every attribute and max vital, a 102-item pack. Answers the
/// account id.
pub(crate) fn seed(
    ts: &TestServer,
    account: &str,
    guid: u32,
    name: &str,
    at: Position,
    progression: bool,
) -> u32 {
    let account_id = ts
        .auth()
        .create_account(
            account,
            "pw",
            AccessLevel::Player,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
        )
        .expect("created")
        .account_id;
    let mut biota = empyrean_entity::Biota {
        id: guid,
        weenie_class_id: 1,
        weenie_type: WeenieType::Creature,
        ..Default::default()
    };
    biota.set_property(
        empyrean_entity::enums::PropertyDataId::CombatTable,
        0x3000_0000,
    );
    biota.set_property(PropertyString::Name, name.to_owned());
    biota.set_property(PropertyDataId::Setup, land::TEST_SETUP);
    biota.set_property(PropertyBool::ReportCollisions, true);
    biota.set_property(PropertyBool::IgnoreCollisions, false);
    biota.set_property(PropertyInt::ItemsCapacity, 102);
    biota.set_property(PropertyInt::ContainersCapacity, 7);
    if progression {
        biota.set_property(PropertyInt::Level, 1);
        biota.set_property(PropertyInt64::TotalExperience, 0);
        biota.set_property(PropertyInt64::AvailableExperience, 0);
    }
    let attributes = biota
        .properties_attribute
        .get_or_insert_with(DotNetDict::new);
    for a in &PropertyAttribute::ALL[1..] {
        attributes.insert(
            *a,
            PropertiesAttribute {
                init_level: 100,
                ..PropertiesAttribute::default()
            },
        );
    }
    let vitals = biota
        .properties_attribute_2nd
        .get_or_insert_with(DotNetDict::new);
    for v in [
        PropertyAttribute2nd::MaxHealth,
        PropertyAttribute2nd::MaxStamina,
        PropertyAttribute2nd::MaxMana,
    ] {
        vitals.insert(
            v,
            PropertiesAttribute2nd {
                init_level: 100,
                current_level: 100,
                ..PropertiesAttribute2nd::default()
            },
        );
    }
    let position = PropertiesPosition {
        obj_cell_id: at.cell(),
        position_x: at.position_x,
        position_y: at.position_y,
        position_z: at.position_z,
        rotation_w: at.rotation_w,
        rotation_x: at.rotation_x,
        rotation_y: at.rotation_y,
        rotation_z: at.rotation_z,
    };
    biota
        .properties_position
        .get_or_insert_with(DotNetDict::new)
        .insert(PositionType::Location, position);
    let character = Character {
        id: guid,
        account_id,
        name: name.to_owned(),
        ..Character::default()
    };
    assert!(ts
        .shard()
        .add_character_in_parallel(&mut biota, &mut [], &character));
    account_id
}
