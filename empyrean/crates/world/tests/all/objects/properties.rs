//! ACE: Source/ACE.Server/WorldObjects/WorldObject_Properties.cs::GetProperty
//! Get/Set/RemoveProperty with ephemeral overrides, ChangesDetected, GetAll* order, positions,
//! generated wrappers.
//! Fixture: explicit values and local in-memory state.

use empyrean_common::dotnet::{DotNetDict, DotNetHashSet};
use empyrean_entity::enums::{
    CoverageMask, HouseStatus, ItemType, PlayerKillerStatus, PortalBitmask, PositionType,
    PropertyBool, PropertyDataId, PropertyFloat, PropertyInstanceId, PropertyInt, PropertyInt64,
    PropertyString, Skill, SubscriptionStatus, TargetingTactic, Tolerance,
};
use empyrean_entity::Position;
use empyrean_world::world_objects::world_object::WorldObject;

fn wo() -> WorldObject {
    WorldObject::default()
}

fn changes(w: &WorldObject) -> bool {
    w.wo.world_object_database.changes_detected
}

fn clear_changes(w: &mut WorldObject) {
    w.wo.world_object_database.changes_detected = false;
}

fn pos(x: f32) -> Position {
    // an indoor cell (low word >= 0x100), so the constructor stores the coordinates as given
    Position::from_components(0xA9B4_0105, x, 20.0, 30.0, 0.0, 0.0, 0.0, 1.0, false)
}

fn keys<K: Copy + std::hash::Hash + Eq, V>(d: &DotNetDict<K, V>) -> Vec<K> {
    d.keys().copied().collect()
}

// ------------------------------------------------------------------------------------------
// Get / Set / Remove round trips, one per property type
// ------------------------------------------------------------------------------------------

#[test]
fn round_trip_every_property_type() {
    let mut w = wo();

    w.set_property(PropertyBool::Attackable, true);
    w.set_property(PropertyInt::Level, 42);
    w.set_property(PropertyInt64::TotalExperience, 1 << 40);
    w.set_property(PropertyFloat::DefaultScale, 1.25);
    w.set_property(PropertyString::Name, "Drudge".to_string());
    w.set_property(PropertyDataId::Setup, 0x0200_0001);
    w.set_property(PropertyInstanceId::Owner, 0x5000_0001);

    assert_eq!(w.get_property(PropertyBool::Attackable), Some(true));
    assert_eq!(w.get_property(PropertyInt::Level), Some(42));
    assert_eq!(
        w.get_property(PropertyInt64::TotalExperience),
        Some(1 << 40)
    );
    assert_eq!(w.get_property(PropertyFloat::DefaultScale), Some(1.25));
    assert_eq!(
        w.get_property(PropertyString::Name).as_deref(),
        Some("Drudge")
    );
    assert_eq!(w.get_property(PropertyDataId::Setup), Some(0x0200_0001));
    assert_eq!(w.get_property(PropertyInstanceId::Owner), Some(0x5000_0001));
    // non-ephemeral: all of it went to the biota
    assert_eq!(w.biota.get_property(PropertyInt::Level), Some(42));
    assert_eq!(
        w.biota.get_property(PropertyInstanceId::Owner),
        Some(0x5000_0001)
    );

    w.remove_property(PropertyBool::Attackable);
    w.remove_property(PropertyInt::Level);
    w.remove_property(PropertyInt64::TotalExperience);
    w.remove_property(PropertyFloat::DefaultScale);
    w.remove_property(PropertyString::Name);
    w.remove_property(PropertyDataId::Setup);
    w.remove_property(PropertyInstanceId::Owner);

    assert_eq!(w.get_property(PropertyBool::Attackable), None);
    assert_eq!(w.get_property(PropertyInt::Level), None);
    assert_eq!(w.get_property(PropertyInt64::TotalExperience), None);
    assert_eq!(w.get_property(PropertyFloat::DefaultScale), None);
    assert_eq!(w.get_property(PropertyString::Name), None);
    assert_eq!(w.get_property(PropertyDataId::Setup), None);
    assert_eq!(w.get_property(PropertyInstanceId::Owner), None);
}

#[test]
fn inc_property_starts_from_zero() {
    let mut w = wo();
    w.inc_property(PropertyFloat::HotspotCycleTime, 1.5);
    w.inc_property(PropertyFloat::HotspotCycleTime, 2.0);
    assert_eq!(w.get_property(PropertyFloat::HotspotCycleTime), Some(3.5));
}

#[test]
fn position_round_trip_and_remove() {
    let mut w = wo();
    w.set_position(PositionType::Location, Some(pos(10.0)));
    assert_eq!(
        w.get_position(PositionType::Location).map(|p| p.position_x),
        Some(10.0)
    );
    assert_eq!(
        w.biota
            .get_position(PositionType::Location)
            .map(|p| p.position_x),
        Some(10.0)
    );
    assert_eq!(w.location().map(|p| p.cell()), Some(0xA9B4_0105));

    // SetPosition(null) on a persisted type is RemovePosition
    w.set_location(None);
    assert!(w.get_position(PositionType::Location).is_none());
    assert!(w.biota.get_position(PositionType::Location).is_none());
}

// ------------------------------------------------------------------------------------------
// Ephemeral properties
// ------------------------------------------------------------------------------------------

#[test]
fn ephemeral_properties_never_touch_the_biota() {
    let mut w = wo();

    // [Ephemeral]: PropertyInt.CoinValue, PropertyBool.Open, PropertyFloat.ResetTimestamp,
    // PropertyString.Afk, PropertyInstanceId.Viewer, PositionType.Home
    w.set_property(PropertyInt::CoinValue, 500);
    w.set_property(PropertyBool::Open, true);
    w.set_property(PropertyFloat::ResetTimestamp, 9.0);
    w.set_property(PropertyString::Afk, "brb".to_string());
    w.set_property(PropertyInstanceId::Viewer, 7);
    w.set_position(PositionType::Home, Some(pos(1.0)));

    assert_eq!(w.get_property(PropertyInt::CoinValue), Some(500));
    assert_eq!(w.get_property(PropertyBool::Open), Some(true));
    assert_eq!(w.get_property(PropertyFloat::ResetTimestamp), Some(9.0));
    assert_eq!(w.get_property(PropertyString::Afk).as_deref(), Some("brb"));
    assert_eq!(w.get_property(PropertyInstanceId::Viewer), Some(7));
    assert_eq!(
        w.get_position(PositionType::Home).map(|p| p.position_x),
        Some(1.0)
    );

    assert!(w.biota.properties_int.is_none());
    assert!(w.biota.properties_bool.is_none());
    assert!(w.biota.properties_float.is_none());
    assert!(w.biota.properties_string.is_none());
    assert!(w.biota.properties_iid.is_none());
    assert!(w.biota.properties_position.is_none());
    assert!(!changes(&w), "ephemeral writes never set ChangesDetected");

    w.remove_property(PropertyInt::CoinValue);
    w.remove_position(PositionType::Home);
    assert_eq!(w.get_property(PropertyInt::CoinValue), None);
    assert!(w.get_position(PositionType::Home).is_none());
    assert!(w.biota.properties_int.is_none());
    assert!(!changes(&w));
}

#[test]
fn a_removed_ephemeral_hides_the_biota_value() {
    let mut w = wo();
    // a biota loaded with an ephemeral property (as a stale shard row would be)
    w.biota.set_property(PropertyInt::CoinValue, 100);
    assert_eq!(w.get_property(PropertyInt::CoinValue), Some(100));

    // RemoveProperty on an ephemeral only writes null if the dictionary exists: nothing yet
    w.remove_property(PropertyInt::CoinValue);
    assert_eq!(w.get_property(PropertyInt::CoinValue), Some(100));

    w.set_property(PropertyInt::CoinValue, 5);
    assert_eq!(w.get_property(PropertyInt::CoinValue), Some(5));
    // now the dictionary exists: the stored null hides the biota value
    w.remove_property(PropertyInt::CoinValue);
    assert_eq!(w.get_property(PropertyInt::CoinValue), None);
    assert_eq!(w.biota.get_property(PropertyInt::CoinValue), Some(100));
}

#[test]
fn property_int_already_in_the_ephemeral_dictionary_stays_ephemeral_on_set_only() {
    let mut w = wo();
    // a subclass stores EncumbranceVal ephemerally by writing ephemeralPropertyInts directly
    w.wo.world_object_properties.ephemeral_property_ints = Some(
        [(PropertyInt::EncumbranceVal, Some(10))]
            .into_iter()
            .collect(),
    );

    w.set_property(PropertyInt::EncumbranceVal, 20);
    assert_eq!(w.get_property(PropertyInt::EncumbranceVal), Some(20));
    assert!(
        w.biota.properties_int.is_none(),
        "SetProperty(PropertyInt) checks the dictionary too"
    );

    // ...but RemoveProperty(PropertyInt) only checks EphemeralProperties, so it goes to the biota
    w.biota.set_property(PropertyInt::EncumbranceVal, 99);
    w.remove_property(PropertyInt::EncumbranceVal);
    assert_eq!(w.biota.get_property(PropertyInt::EncumbranceVal), None);
    assert!(changes(&w));
    assert_eq!(w.get_property(PropertyInt::EncumbranceVal), Some(20));

    // other types have no such check: a float key present in the dictionary is not ephemeral
    // unless it is [Ephemeral]
    w.set_property(PropertyFloat::DefaultScale, 2.0);
    assert_eq!(w.biota.get_property(PropertyFloat::DefaultScale), Some(2.0));
}

// ------------------------------------------------------------------------------------------
// ChangesDetected
// ------------------------------------------------------------------------------------------

#[test]
fn changes_detected_only_on_real_biota_changes() {
    let mut w = wo();
    assert!(!changes(&w));

    w.set_property(PropertyInt::Level, 5);
    assert!(changes(&w), "new value");
    clear_changes(&mut w);

    w.set_property(PropertyInt::Level, 5);
    assert!(!changes(&w), "same value is not a change");

    w.set_property(PropertyInt::Level, 6);
    assert!(changes(&w), "different value");
    clear_changes(&mut w);

    w.remove_property(PropertyInt::Mass);
    assert!(!changes(&w), "removing an absent property");
    w.remove_property(PropertyInt::Level);
    assert!(changes(&w), "removing a present property");
    clear_changes(&mut w);

    w.set_property(PropertyFloat::DefaultScale, f64::NAN);
    clear_changes(&mut w);
    w.set_property(PropertyFloat::DefaultScale, f64::NAN);
    assert!(
        changes(&w),
        "NaN never equals itself, so it always counts as changed"
    );
    clear_changes(&mut w);

    // SetPosition always flags, even for the same position; RemovePosition only when present
    w.set_position(PositionType::Location, Some(pos(3.0)));
    clear_changes(&mut w);
    w.set_position(PositionType::Location, Some(pos(3.0)));
    assert!(changes(&w));
    clear_changes(&mut w);
    w.remove_position(PositionType::Instantiation);
    assert!(!changes(&w));
    w.remove_position(PositionType::Location);
    assert!(changes(&w));
}

// ------------------------------------------------------------------------------------------
// GetAll* enumeration order
// ------------------------------------------------------------------------------------------

#[test]
fn get_all_property_int_order_after_interleaved_sets_and_removes() {
    let mut w = wo();
    // biota bag: Level, Mass, Value -> remove Mass (frees slot 1) -> ItemType reuses slot 1
    w.set_property(PropertyInt::Level, 1);
    w.set_property(PropertyInt::Mass, 2);
    w.set_property(PropertyInt::Value, 3);
    w.remove_property(PropertyInt::Mass);
    w.set_property(PropertyInt::ItemType, 4);
    // a stale ephemeral row in the biota
    w.biota.set_property(PropertyInt::CoinValue, 50);
    assert_eq!(
        keys(w.biota.properties_int.as_ref().unwrap()),
        [
            PropertyInt::Level,
            PropertyInt::ItemType,
            PropertyInt::Value,
            PropertyInt::CoinValue
        ]
    );

    // ephemeral dictionary: RemainingLifespan, CoinValue(null), AppraisalPages
    w.set_property(PropertyInt::RemainingLifespan, 60);
    w.set_property(PropertyInt::CoinValue, 0);
    w.remove_property(PropertyInt::CoinValue);
    w.set_property(PropertyInt::AppraisalPages, 2);

    // results = biota order [Level, ItemType, Value, CoinValue]; then RemainingLifespan appends
    // (slot 4), CoinValue's null removes slot 3, AppraisalPages takes the freed slot 3.
    let all = w.get_all_property_int();
    assert_eq!(
        keys(&all),
        [
            PropertyInt::Level,
            PropertyInt::ItemType,
            PropertyInt::Value,
            PropertyInt::AppraisalPages,
            PropertyInt::RemainingLifespan
        ]
    );
    assert_eq!(all.get(&PropertyInt::AppraisalPages), Some(&2));

    let mut want = DotNetHashSet::new();
    want.insert(PropertyInt::Value.0);
    want.insert(PropertyInt::RemainingLifespan.0);
    want.insert(PropertyInt::CoinValue.0);
    let some = w.get_all_property_int_where(&want);
    assert_eq!(
        keys(&some),
        [PropertyInt::Value, PropertyInt::RemainingLifespan]
    );
}

#[test]
fn get_all_positions_rebuilds_from_the_biota_and_applies_ephemerals() {
    let mut w = wo();
    w.set_position(PositionType::Location, Some(pos(1.0)));
    w.set_position(PositionType::Instantiation, Some(pos(2.0)));
    w.set_position(PositionType::Home, Some(pos(3.0)));
    // in-place change through the cache is not in the biota, and GetAllPositions reads the biota
    w.get_position_mut(PositionType::Location)
        .unwrap()
        .position_x = 9.0;

    let all = w.get_all_positions();
    assert_eq!(
        keys(&all),
        [
            PositionType::Location,
            PositionType::Instantiation,
            PositionType::Home
        ]
    );
    assert_eq!(
        all.get(&PositionType::Location).map(|p| p.position_x),
        Some(1.0)
    );

    w.remove_position(PositionType::Home);
    assert_eq!(
        keys(&w.get_all_positions()),
        [PositionType::Location, PositionType::Instantiation]
    );
}

#[test]
#[should_panic(expected = "NullReferenceException")]
fn get_all_positions_without_a_position_bag_throws() {
    // ACE-BUG reproduced: Biota.PropertiesPosition is enumerated without a null check
    let _ = wo().get_all_positions();
}

#[test]
fn position_cache_is_the_live_object() {
    let mut w = wo();
    w.biota.set_position(PositionType::Location, &pos(4.0));

    w.get_position_mut(PositionType::Location)
        .unwrap()
        .position_x = 8.0;
    assert_eq!(
        w.location().map(|p| p.position_x),
        Some(8.0),
        "reads see the cached object"
    );
    assert_eq!(
        w.biota
            .get_position(PositionType::Location)
            .map(|p| p.position_x),
        Some(4.0)
    );
    assert!(!changes(&w), "an in-place change is not a SetPosition");

    // RemovePosition drops the cache entry
    w.remove_position(PositionType::Location);
    assert!(w.location().is_none());
}

// ------------------------------------------------------------------------------------------
// Generated wrappers, one or more per template form, against ACE's declarations
// ------------------------------------------------------------------------------------------

#[test]
fn generated_nullable_wrappers() {
    let mut w = wo();
    // int? Level: get => GetProperty; set { if (!value.HasValue) Remove; else Set(value.Value) }
    assert_eq!(w.level(), None);
    w.set_level(Some(12));
    assert_eq!(w.level(), Some(12));
    w.set_level(None);
    assert_eq!(w.biota.get_property(PropertyInt::Level), None);

    // long? AugmentationCost (AugmentationDevice.cs)
    w.set_augmentation_cost(Some(7_000_000_000));
    assert_eq!(
        w.get_property(PropertyInt64::AugmentationCost),
        Some(7_000_000_000)
    );

    // uint? LastUnlocker (Container_Properties.cs), an instance id
    w.set_last_unlocker(Some(0x5000_00AA));
    assert_eq!(w.last_unlocker(), Some(0x5000_00AA));

    // string AllegianceName: set { if (value == null) Remove; else Set(value) }
    w.set_allegiance_name(Some("Order".to_string()));
    assert_eq!(w.allegiance_name().as_deref(), Some("Order"));
    w.set_allegiance_name(None);
    assert_eq!(w.get_property(PropertyString::AllegianceName), None);

    // uint? PetDevice (Pet.cs): set { if (value.HasValue) Set; else Remove }
    w.set_pet_device(Some(3));
    assert_eq!(w.pet_device(), Some(3));
    w.set_pet_device(None);
    assert_eq!(w.pet_device(), None);

    // double? LifestoneProtectionTimestamp: get => GetProperty(..) ?? null
    assert_eq!(w.lifestone_protection_timestamp(), None);
    w.set_lifestone_protection_timestamp(Some(12.5));
    assert_eq!(w.lifestone_protection_timestamp(), Some(12.5));
}

#[test]
fn generated_cast_wrappers() {
    let mut w = wo();
    // float? ObjScale: (float?)GetProperty(PropertyFloat.DefaultScale); set stores the float
    w.set_property(PropertyFloat::DefaultScale, 0.1);
    assert_eq!(w.obj_scale(), Some(0.1_f32));
    w.set_obj_scale(Some(0.1_f32));
    assert_eq!(
        w.get_property(PropertyFloat::DefaultScale),
        Some(f64::from(0.1_f32))
    );

    // ushort? Structure: (ushort?) of an int truncates
    w.set_property(PropertyInt::Structure, 70_000);
    assert_eq!(w.structure(), Some(4464)); // (ushort)70000 == 70000 - 65536
    w.set_structure(Some(65_535));
    assert_eq!(w.get_property(PropertyInt::Structure), Some(65_535));

    // Skill? ItemSkillLimit: (Skill?)GetProperty(PropertyDataId..); set (uint)value
    w.set_item_skill_limit(Some(Skill::Axe));
    assert_eq!(
        w.get_property(PropertyDataId::ItemSkillLimit),
        Some(Skill::Axe.0 as u32)
    );
    assert_eq!(w.item_skill_limit(), Some(Skill::Axe));

    // SubscriptionStatus? Portal.AccountRequirements
    assert_eq!(w.account_requirements_portal(), None);
    w.set_account_requirements_portal(Some(SubscriptionStatus::AsheronsCall_Subscription));
    assert_eq!(
        w.account_requirements_portal(),
        Some(SubscriptionStatus::AsheronsCall_Subscription)
    );
}

#[test]
fn generated_default_wrappers() {
    let mut w = wo();
    // bool IsOpen: ?? false; set { if (!value) Remove; else Set(value) } (Open is [Ephemeral])
    assert!(!w.is_open());
    w.set_is_open(true);
    assert!(w.is_open());
    assert!(w.biota.properties_bool.is_none());

    // bool Stuck: ?? false
    w.set_stuck(false);
    assert!(!w.stuck());

    // uint MotionTableId: ?? 0; set => SetProperty (no remove on 0)
    assert_eq!(w.motion_table_id(), 0);
    w.set_motion_table_id(0);
    assert_eq!(w.get_property(PropertyDataId::MotionTable), Some(0));

    // int HouseMaxHooksUsable: ?? 25; set { if (value == 25) Remove; else Set }
    assert_eq!(w.house_max_hooks_usable(), 25);
    w.set_house_max_hooks_usable(30);
    assert_eq!(w.house_max_hooks_usable(), 30);
    w.set_house_max_hooks_usable(25);
    assert_eq!(w.get_property(PropertyInt::HouseMaxHooksUsable), None);

    // double? CycleTimeVariance: ?? 0 on a nullable -> never null
    assert_eq!(w.cycle_time_variance(), Some(0.0));

    // ulong AllegianceXPCached: (ulong)(GetProperty(Int64) ?? 0); set (long)value
    w.set_property(PropertyInt64::AllegianceXPCached, -1);
    assert_eq!(w.allegiance_xp_cached(), u64::MAX);
    w.set_allegiance_xp_cached(u64::MAX);
    assert_eq!(w.get_property(PropertyInt64::AllegianceXPCached), Some(-1));
    w.set_allegiance_xp_cached(0);
    assert_eq!(w.get_property(PropertyInt64::AllegianceXPCached), None);
}

#[test]
fn generated_enum_default_wrappers() {
    let mut w = wo();
    // Tolerance: (Tolerance)(GetProperty ?? 0); set { if (value == 0) Remove; else Set((int)value) }
    assert_eq!(w.tolerance(), Tolerance::None);
    w.set_tolerance(Tolerance::Appraise);
    assert_eq!(w.get_property(PropertyInt::Tolerance), Some(2));
    w.set_tolerance(Tolerance::None);
    assert_eq!(w.get_property(PropertyInt::Tolerance), None);

    // ItemType: set => SetProperty((int)value) stores 0 too
    w.set_item_type(ItemType(0));
    assert_eq!(w.get_property(PropertyInt::ItemType), Some(0));

    // HouseStatus: (HouseStatus?)GetProperty ?? HouseStatus.Active; Active removes
    assert_eq!(w.house_status(), HouseStatus::Active);
    w.set_house_status(HouseStatus(2));
    assert_eq!(w.house_status(), HouseStatus(2));
    w.set_house_status(HouseStatus::Active);
    assert_eq!(w.get_property(PropertyInt::HouseStatus), None);

    // PlayerKillerStatus: ?? NPK; setter always stores (renamed: Player.SetPlayerKillerStatus)
    assert_eq!(w.player_killer_status(), PlayerKillerStatus::NPK);
    w.set_player_killer_status_prop(PlayerKillerStatus::NPK);
    assert_eq!(
        w.get_property(PropertyInt::PlayerKillerStatus),
        Some(PlayerKillerStatus::NPK.0 as i32)
    );

    // PortalRestrictions: default Unrestricted, but the setter removes on Undef (ACE's asymmetry)
    assert_eq!(w.portal_restrictions(), PortalBitmask::Unrestricted);
    w.set_portal_restrictions(PortalBitmask::Undef);
    assert_eq!(w.portal_restrictions(), PortalBitmask::Unrestricted);
    w.set_portal_restrictions(PortalBitmask::Unrestricted);
    assert_eq!(w.get_property(PropertyInt::PortalBitmask), Some(1));

    // Player.AccountRequirements: ?? (int)AsheronsCall_Subscription
    assert_eq!(
        w.account_requirements_player(),
        SubscriptionStatus::AsheronsCall_Subscription
    );
}

#[test]
fn generated_int_backed_bools() {
    let mut w = wo();
    // bool Active: (GetProperty(Int) ?? 1) != 0; set { if (value) Remove; else Set(0) }
    assert!(w.active());
    w.set_active(false);
    assert_eq!(w.get_property(PropertyInt::Active), Some(0));
    assert!(!w.active());
    w.set_active(true);
    assert_eq!(w.get_property(PropertyInt::Active), None);

    // bool HearLocalSignals: (?? 0) != 0; set { if (!value) Remove; else Set(1) }
    assert!(!w.hear_local_signals());
    w.set_hear_local_signals(true);
    assert_eq!(w.get_property(PropertyInt::HearLocalSignals), Some(1));

    // bool OpenToEveryone (House.cs): (?? 0) == 1, so 2 reads false
    w.set_property(PropertyInt::OpenToEveryone, 2);
    assert!(!w.open_to_everyone());

    // bool IsCleaving: GetProperty(Cleaving) != null
    assert!(!w.is_cleaving());
    w.set_property(PropertyInt::Cleaving, 3);
    assert!(w.is_cleaving());
}

#[test]
fn merged_and_renamed_wrappers() {
    let mut w = wo();
    // Chest.LockCode and Door.LockCode are identical: one `lock_code`
    w.set_lock_code(Some("abc".to_string()));
    assert_eq!(w.lock_code().as_deref(), Some("abc"));
    // Player.IsAdmin is a property; `is_admin` stays the C# `is Admin` class check
    w.set_is_admin_prop(true);
    assert!(w.is_admin_prop());
    assert!(!w.is_admin());
}

// ------------------------------------------------------------------------------------------
// Hand-ported wrappers
// ------------------------------------------------------------------------------------------

#[test]
fn targeting_tactic_setter_stores_the_current_value() {
    let mut w = wo();
    // ACE-BUG: SetProperty(..., (int)TargetingTactic) uses the getter, not `value`
    w.set_targeting_tactic(TargetingTactic(4));
    assert_eq!(w.get_property(PropertyInt::TargetingTactic), Some(0));
    w.set_property(PropertyInt::TargetingTactic, 3);
    w.set_targeting_tactic(TargetingTactic(8));
    assert_eq!(w.targeting_tactic(), TargetingTactic(3));
    w.set_targeting_tactic(TargetingTactic(0));
    assert_eq!(w.get_property(PropertyInt::TargetingTactic), None);
}

#[test]
fn house_current_hooks_default_to_max_hooks() {
    let mut w = wo();
    assert_eq!(w.house_current_hooks_usable(), 25);
    w.set_house_max_hooks_usable(40);
    assert_eq!(w.house_current_hooks_usable(), 40);
    w.set_house_current_hooks_usable(40);
    assert_eq!(w.get_property(PropertyInt::HouseCurrentHooksUsable), None);
    w.set_house_current_hooks_usable(3);
    assert_eq!(w.house_current_hooks_usable(), 3);
    assert!(
        w.biota
            .get_property(PropertyInt::HouseCurrentHooksUsable)
            .is_none(),
        "[Ephemeral]"
    );
}

#[test]
fn cleave_targets() {
    let mut w = wo();
    assert_eq!(w.cleave_targets(), 0);
    w.set_property(PropertyInt::Cleaving, 3);
    assert_eq!(w.cleave_targets(), 2);
}

#[test]
fn visual_clothing_priority_falls_back_to_clothing_priority() {
    let mut w = wo();
    w.set_property(PropertyInt::ClothingPriority, 0x10);
    assert_eq!(w.visual_clothing_priority(), Some(CoverageMask(0x10)));
    w.set_visual_clothing_priority_prop(Some(CoverageMask(0x20)));
    assert_eq!(w.visual_clothing_priority(), Some(CoverageMask(0x20)));
    w.set_visual_clothing_priority_prop(None);
    assert_eq!(w.visual_clothing_priority(), Some(CoverageMask(0x10)));
}

#[test]
fn workmanship_recovers_an_old_encoding_and_rounds_half_to_even() {
    let mut w = wo();
    assert_eq!(w.workmanship(), None);

    w.set_item_workmanship(Some(15));
    w.set_property(PropertyInt::NumItemsInMaterial, 2);
    assert_eq!(w.workmanship(), Some(7.5));
    assert_eq!(w.item_workmanship(), Some(15), "in range: no write-back");

    // 65000 / 1 is out of range -> 65000 / 10000 / 1 = 6.5 -> ItemWorkmanship = Math.Round(6.5) = 6
    w.set_property(PropertyInt::NumItemsInMaterial, 1);
    w.set_item_workmanship(Some(65_000));
    assert_eq!(w.workmanship(), Some(6.5));
    assert_eq!(w.item_workmanship(), Some(6));

    // 200000 / 10000 / 2 (Structure) = 10.0 at the edge; 300000 -> 15 -> clamped to 10
    w.set_structure(Some(2));
    w.set_item_workmanship(Some(300_000));
    assert_eq!(w.workmanship(), Some(10.0));
    assert_eq!(w.item_workmanship(), Some(15));

    // setter: Math.Round(value * NumItemsInMaterial)
    w.set_property(PropertyInt::NumItemsInMaterial, 3);
    w.set_workmanship(Some(2.5));
    assert_eq!(w.item_workmanship(), Some(8)); // 7.5 -> 8 (half to even)
    w.set_workmanship(None);
    assert_eq!(w.item_workmanship(), None);
}

#[test]
fn physics_property_setters_without_a_physics_object() {
    let mut w = wo();
    w.set_ethereal(Some(true));
    assert_eq!(w.biota.get_property(PropertyBool::Ethereal), Some(true));
    w.set_ethereal(None);
    assert_eq!(w.ethereal(), None);
    assert!(
        !empyrean_common::not_ported::take_local().contains_key("ACE: WorldObject.SetPhysicsState"),
        "no PhysicsObj: SetPhysicsState does nothing"
    );
}
