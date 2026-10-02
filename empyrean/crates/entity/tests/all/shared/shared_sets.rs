// @generated from the mapping of ACE's enums to the shared crates' constant sets; do not edit by hand

//! Vectors: enum_shared_map.toml, ACE enums and their shared constant sets
//! The value-equality tests between ACE's enums and the constant sets the shared crates
//! define, generated from a declarative mapping: which ACE enum pairs with which shared
//! constants, and exactly how the two differ (each difference with its reason). Any new drift
//! on either side fails here; a genuine difference is added to the mapping, not to this file.
//! The helpers are hand-written in `shared_equality.rs`.

use empyrean_entity::enums::*;

use super::shared_equality::{ace_opcodes, assert_enum, assert_sets, k, opcode_table, set, Known};

/// The values ACE has and the shared set lacks (GameMessageOpcode | GameActionType | GameEventType): value, name, reason.
pub(crate) const KNOWN_OPCODES_ACE_ONLY: &[Known] = &[
    (0, "GameMessageOpcode.None", "ACE's placeholder zero; not a message"),
    (0xF7B0, "GameMessageOpcode.GameEvent", "the ordered-event header; the table lists its sub-types"),
    (0xF7B1, "GameMessageOpcode.GameAction", "the ordered-action header; the table lists its sub-types"),
    (0xF7E8, "GameMessageOpcode.DDD_BeginPullDDD", "DDD pull, no client codec (plan §2.4)"),
    (0xF7E9, "GameMessageOpcode.DDD_IterationData", "DDD iteration, no client codec (plan §2.4)"),
    (0x00B5, "GameEventType.BookModifyPageResponse", "ACE-only event id (plan §2.4)"),
    (0x01C8, "GameEventType.AllegianceAllegianceUpdateDone", "ACE-only event id (plan §2.4)"),
];

/// The values the shared set has and ACE lacks (GameMessageOpcode | GameActionType | GameEventType): value, name, reason.
pub(crate) const KNOWN_OPCODES_SHARED_ONLY: &[Known] = &[
    (0x004B, "Magic_TestSpellFormula", "an early clients' request ACE never had; Empyrean answers it"),
    (0x01D1, "Qualities_PrivateRemoveIntEvent", "ACE never removes a quality by message"),
    (0x01D2, "Qualities_RemoveIntEvent", "ACE never removes a quality by message"),
    (0x01D3, "Qualities_PrivateRemoveBoolEvent", "ACE never removes a quality by message"),
    (0x01D4, "Qualities_RemoveBoolEvent", "ACE never removes a quality by message"),
    (0x01D5, "Qualities_PrivateRemoveFloatEvent", "ACE never removes a quality by message"),
    (0x01D6, "Qualities_RemoveFloatEvent", "ACE never removes a quality by message"),
    (0x01D7, "Qualities_PrivateRemoveStringEvent", "ACE never removes a quality by message"),
    (0x01D8, "Qualities_RemoveStringEvent", "ACE never removes a quality by message"),
    (0x01D9, "Qualities_PrivateRemoveDataIDEvent", "ACE never removes a quality by message"),
    (0x01DA, "Qualities_RemoveDataIDEvent", "ACE never removes a quality by message"),
    (0x01DB, "Qualities_PrivateRemoveInstanceIDEvent", "ACE never removes a quality by message"),
    (0x01DC, "Qualities_RemoveInstanceIDEvent", "ACE never removes a quality by message"),
    (0x01DD, "Qualities_PrivateRemovePositionEvent", "ACE never removes a quality by message"),
    (0x01DE, "Qualities_RemovePositionEvent", "ACE never removes a quality by message"),
    (0x0220, "Character_RemovePlayerPermission_UnusedCatalogueOpcode", "catalogue-only; client uses 0x021A"),
    (0x02B8, "Qualities_PrivateRemoveInt64Event", "ACE never removes a quality by message"),
    (0x02B9, "Qualities_RemoveInt64Event", "ACE never removes a quality by message"),
    (0x02E1, "Qualities_PrivateUpdateSkillAC", "not sent by ACE"),
    (0x02E2, "Qualities_UpdateSkillAC", "not sent by ACE"),
    (0x02E5, "Qualities_PrivateUpdateAttributeLevel", "not sent by ACE"),
    (0x02E6, "Qualities_UpdateAttributeLevel", "not sent by ACE"),
    (0x02EA, "Qualities_UpdateAttribute2ndLevel", "not sent by ACE"),
    (0xF630, "Character_SetPlayerVisualDesc", "not sent by ACE"),
    (0xF651, "Login_AwaitingSubscriptionExpiration", "not sent by ACE"),
    (0xF7CA, "Admin_ReceiveAccountData", "not sent by ACE"),
    (0xF7CB, "Admin_ReceivePlayerData", "not sent by ACE"),
    (0xF7EB, "DDD_EndDDDMessage", "not sent by ACE"),
];

/// The values ACE has and the shared set lacks (MotionCommand): value, name, reason.
pub(crate) const KNOWN_MOTION_ACE_ONLY: &[Known] = &[
    (0, "Invalid", "ACE's Invalid is 0; the client's is 0x80000000"),
];

/// The values the shared set has and ACE lacks (MotionCommand): value, name, reason.
pub(crate) const KNOWN_MOTION_SHARED_ONLY: &[Known] = &[
    (0x8000_0000, "Invalid", "the client's Invalid; ACE's is 0"),
];

/// The values ACE has and the shared set lacks (WeenieHeaderFlag): value, name, reason.
pub(crate) const KNOWN_WEENIE_HEADER_FLAG_ACE_ONLY: &[Known] = &[
    (0, "None", "ACE's empty-mask member"),
];

/// The values ACE has and the shared set lacks (WeenieHeaderFlag2): value, name, reason.
pub(crate) const KNOWN_WEENIE_HEADER_FLAG2_ACE_ONLY: &[Known] = &[
    (0, "None", "ACE's empty-mask member"),
];

/// The values ACE has and the shared set lacks (PhysicsDescriptionFlag): value, name, reason.
pub(crate) const KNOWN_PHYSICS_DESCRIPTION_FLAG_ACE_ONLY: &[Known] = &[
    (0, "None", "ACE's empty-mask member"),
];

/// The values ACE has and the shared set lacks (PositionFlags): value, name, reason.
pub(crate) const KNOWN_POSITION_FLAGS_ACE_ONLY: &[Known] = &[
    (0, "None", "ACE's empty-mask member"),
];

/// The values ACE has and the shared set lacks (AttackConditions): value, name, reason.
pub(crate) const KNOWN_ATTACK_CONDITIONS_ACE_ONLY: &[Known] = &[
    (0, "None", "ACE's empty-mask member"),
];

/// The values ACE has and the shared set lacks (IdentifyResponseFlags): value, name, reason.
pub(crate) const KNOWN_IDENTIFY_RESPONSE_FLAGS_ACE_ONLY: &[Known] = &[
    (0, "None", "ACE's empty-mask member"),
];

/// The values ACE has and the shared set lacks (MovementType): value, name, reason.
pub(crate) const KNOWN_MOVEMENT_TYPE_ACE_ONLY: &[Known] = &[
    (1, "RawCommand", "no constant in dereth-protocol; the codec's default branch"),
    (2, "InterpretedCommand", "no constant in dereth-protocol; the codec's default branch"),
    (3, "StopRawCommand", "no constant in dereth-protocol; the codec's default branch"),
    (4, "StopInterpretedCommand", "no constant in dereth-protocol; the codec's default branch"),
    (5, "StopCompletely", "no constant in dereth-protocol; the codec's default branch"),
];

/// The values ACE has and the shared set lacks (MotionFlags): value, name, reason.
pub(crate) const KNOWN_MOTION_FLAGS_ACE_ONLY: &[Known] = &[
    (0, "None", "ACE's empty-mask member"),
];

/// The values ACE has and the shared set lacks (Quadrant): value, name, reason.
pub(crate) const KNOWN_QUADRANT_ACE_ONLY: &[Known] = &[
    (0, "None", "ACE's empty-mask member"),
];

/// The values ACE has and the shared set lacks (StatType): value, name, reason.
pub(crate) const KNOWN_STAT_TYPE_ACE_ONLY: &[Known] = &[
    (0, "Undef", "ACE's zero member"),
];

/// The values ACE has and the shared set lacks (AttributeCache): value, name, reason.
pub(crate) const KNOWN_ATTRIBUTE_CACHE_ACE_ONLY: &[Known] = &[
    (0, "Undef", "ACE's empty-mask member"),
];

/// The values ACE has and the shared set lacks (CharacterOptionDataFlag): value, name, reason.
pub(crate) const KNOWN_CHARACTER_OPTION_DATA_FLAG_ACE_ONLY: &[Known] = &[
    (2, "SquelchList", "the client's module unpacker has no branch for it"),
];

/// The values ACE has and the shared set lacks (EquipMask): value, name, reason.
pub(crate) const KNOWN_EQUIP_MASK_ACE_ONLY: &[Known] = &[
    (0x0121, "Extremity", "ACE's own composite; the client has none"),
    (0x7F00, "Armor", "ACE's composite adds FootWear; the client's ARMOR is ACE's ArmorExclusive"),
    (0x0370_0000, "Selectable", "ACE's own composite; the client has none"),
    (0x2500_0000, "Weapon", "ACE's composite is the client's value shifted one hex digit up"),
    (0x3500_0000, "WeaponReadySlot", "ACE's composite is the client's value shifted one hex digit up"),
    (0x3F00_0000, "ReadySlot", "ACE's composite is the client's value shifted one hex digit up"),
    (0x8000_01FF, "Clothing", "ACE's composite has bit 31 where the client's 0x080001FF has the cloak bit"),
];

/// The values the shared set has and ACE lacks (EquipMask): value, name, reason.
pub(crate) const KNOWN_EQUIP_MASK_SHARED_ONLY: &[Known] = &[
    (0x0250_0000, "WEAPON", "the client's composite; ACE's is this shifted one hex digit up"),
    (0x0350_0000, "WEAPON_READY_SLOT", "the client's composite; ACE's is this shifted one hex digit up"),
    (0x0800_01FF, "CLOTHING", "the client's clothing composite, with the cloak bit instead of bit 31"),
    (0x0800_7FFF, "WEARABLE", "named only by the client"),
    (0x7EFF_8000, "WIELDABLE", "named only by the client"),
];

/// ACE's three opcode enums together against dereth-protocol's master opcode table.
#[test]
fn ace_opcodes_match_the_master_opcode_table() {
    let shared: Vec<u64> = opcode_table().into_iter().collect();
    assert_sets(
        "GameMessageOpcode | GameActionType | GameEventType",
        &ace_opcodes(),
        &set(shared.iter().copied()),
        KNOWN_OPCODES_ACE_ONLY,
        KNOWN_OPCODES_SHARED_ONLY,
    );
}

/// The motion-command sets use the final retail numbering.
/// ACE retains the earlier UI target-selection block; V331 maps that block to the client values.
/// The invalid sentinel remains the explicit set difference.
#[test]
fn motion_command_matches_the_client_table() {
    let shared: Vec<u64> = dereth_animation::command::all().map(|(_, c, _)| k(c.0)).collect();
    assert_eq!(shared.len(), 412);
    assert_enum::<MotionCommand>(&shared, KNOWN_MOTION_ACE_ONLY, KNOWN_MOTION_SHARED_ONLY);
}

#[test]
fn physics_state_matches_dereth_physics() {
    use dereth_physics::obj::PhysicsState as P;
    let shared = [
        P::STATIC_PS,
        P::UNUSED1_PS,
        P::ETHEREAL_PS,
        P::REPORT_COLLISIONS_PS,
        P::IGNORE_COLLISIONS_PS,
        P::NODRAW_PS,
        P::MISSILE_PS,
        P::PUSHABLE_PS,
        P::ALIGNPATH_PS,
        P::PATHCLIPPED_PS,
        P::GRAVITY_PS,
        P::LIGHTING_ON_PS,
        P::PARTICLE_EMITTER_PS,
        P::UNNUSED2_PS,
        P::HIDDEN_PS,
        P::SCRIPTED_COLLISION_PS,
        P::HAS_PHYSICS_BSP_PS,
        P::INELASTIC_PS,
        P::HAS_DEFAULT_ANIM_PS,
        P::HAS_DEFAULT_SCRIPT_PS,
        P::CLOAKED_PS,
        P::REPORT_COLLISIONS_AS_ENVIRONMENT_PS,
        P::EDGE_SLIDE_PS,
        P::SLEDDING_PS,
        P::FROZEN_PS,
    ]
    .map(k);
    assert_enum::<PhysicsState>(&shared, &[], &[]);
}

#[test]
fn weenie_header_flags_match_the_public_weenie_desc() {
    {
        use dereth_protocol::types::weeniedesc::header as h;
        let shared = [
            h::PLURAL_NAME,
            h::ITEMS_CAPACITY,
            h::CONTAINERS_CAPACITY,
            h::VALUE,
            h::USEABILITY,
            h::USE_RADIUS,
            h::MONARCH,
            h::EFFECTS,
            h::AMMO_TYPE,
            h::COMBAT_USE,
            h::STRUCTURE,
            h::MAX_STRUCTURE,
            h::STACK_SIZE,
            h::MAX_STACK_SIZE,
            h::CONTAINER_ID,
            h::WIELDER_ID,
            h::VALID_LOCATIONS,
            h::LOCATION,
            h::PRIORITY,
            h::TARGET_TYPE,
            h::BLIP_COLOR,
            h::BURDEN,
            h::SPELL_ID,
            h::RADAR_ENUM,
            h::WORKMANSHIP,
            h::HOUSE_OWNER,
            h::RESTRICTIONS,
            h::PSCRIPT,
            h::HOOK_TYPE,
            h::HOOK_ITEM_TYPES,
            h::ICON_OVERLAY,
            h::MATERIAL_TYPE,
        ]
        .map(k);
        assert_enum::<WeenieHeaderFlag>(&shared, KNOWN_WEENIE_HEADER_FLAG_ACE_ONLY, &[]);
    }
    {
        use dereth_protocol::types::weeniedesc::header2 as h2;
        let shared = [
            h2::ICON_UNDERLAY,
            h2::COOLDOWN_ID,
            h2::COOLDOWN_DURATION,
            h2::PET_OWNER,
        ]
        .map(k);
        assert_enum::<WeenieHeaderFlag2>(&shared, KNOWN_WEENIE_HEADER_FLAG2_ACE_ONLY, &[]);
    }
}

#[test]
fn physics_description_flags_match_the_physics_desc() {
    use dereth_protocol::types::physicsdesc::flags as f;
    let shared = [
        f::SETUP,
        f::MTABLE,
        f::VELOCITY,
        f::ACCELERATION,
        f::OMEGA,
        f::PARENT,
        f::CHILDREN,
        f::OBJSCALE,
        f::FRICTION,
        f::ELASTICITY,
        f::TIMESTAMPS,
        f::STABLE,
        f::PETABLE,
        f::DEFAULT_SCRIPT,
        f::DEFAULT_SCRIPT_INTENSITY,
        f::POSITION,
        f::MOVEMENT,
        f::ANIMFRAME,
        f::TRANSLUCENCY,
    ]
    .map(k);
    assert_enum::<PhysicsDescriptionFlag>(&shared, KNOWN_PHYSICS_DESCRIPTION_FLAG_ACE_ONLY, &[]);
}

#[test]
fn position_flags_match_the_position_pack() {
    use dereth_protocol::movement::position_flags as f;
    let shared = [
        f::HAS_VELOCITY,
        f::HAS_PLACEMENT_ID,
        f::IS_GROUNDED,
        f::ORIENTATION_HAS_NO_W,
        f::ORIENTATION_HAS_NO_X,
        f::ORIENTATION_HAS_NO_Y,
        f::ORIENTATION_HAS_NO_Z,
    ]
    .map(k);
    assert_enum::<PositionFlags>(&shared, KNOWN_POSITION_FLAGS_ACE_ONLY, &[]);
}

#[test]
fn attack_conditions_match_the_combat_messages() {
    use dereth_protocol::combat::attack_conditions as f;
    let shared = [
        f::CRITICAL_PROTECTION_AUGMENTATION,
        f::RECKLESSNESS,
        f::SNEAK_ATTACK,
        f::OVERPOWER,
    ]
    .map(k);
    assert_enum::<AttackConditions>(&shared, KNOWN_ATTACK_CONDITIONS_ACE_ONLY, &[]);
}

#[test]
fn identify_response_flags_match_the_appraisal_profile() {
    use dereth_protocol::types::appraisal::flags as f;
    let shared = [
        f::INT,
        f::BOOL,
        f::FLOAT,
        f::STRING,
        f::SPELL_BOOK,
        f::WEAPON_PROFILE,
        f::HOOK_PROFILE,
        f::ARMOR_PROFILE,
        f::CREATURE_PROFILE,
        f::ARMOR_ENCHANTMENT,
        f::RESIST_ENCHANTMENT,
        f::WEAPON_ENCHANTMENT,
        f::DID,
        f::INT64,
        f::BASE_ARMOR,
    ]
    .map(k);
    assert_enum::<IdentifyResponseFlags>(&shared, KNOWN_IDENTIFY_RESPONSE_FLAGS_ACE_ONLY, &[]);
}

#[test]
fn movement_params_match_the_motion_flags() {
    use dereth_animation::motion::flags as f;
    let shared = [
        f::CAN_WALK,
        f::CAN_RUN,
        f::CAN_SIDESTEP,
        f::CAN_WALK_BACKWARDS,
        f::CAN_CHARGE,
        f::FAIL_WALK,
        f::USE_FINAL_HEADING,
        f::STICKY,
        f::MOVE_AWAY,
        f::MOVE_TOWARDS,
        f::USE_SPHERES,
        f::SET_HOLD_KEY,
        f::AUTONOMOUS,
        f::MODIFY_RAW_STATE,
        f::MODIFY_INTERPRETED_STATE,
        f::CANCEL_MOVETO,
        f::STOP_COMPLETELY,
        f::DISABLE_JUMP_DURING_LINK,
    ]
    .map(k);
    assert_enum::<MovementParams>(&shared, &[], &[]);
}

/// The shared movement-type module names only the types the movement-body codec branches on.
#[test]
fn movement_type_and_motion_flags_match_the_movement_body() {
    {
        use dereth_protocol::movement::movement_type as mt;
        let shared = [
            mt::INVALID,
            mt::MOVE_TO_OBJECT,
            mt::MOVE_TO_POSITION,
            mt::TURN_TO_OBJECT,
            mt::TURN_TO_HEADING,
        ]
        .map(k);
        assert_enum::<MovementType>(&shared, KNOWN_MOVEMENT_TYPE_ACE_ONLY, &[]);
    }
    {
        use dereth_protocol::movement::motion_flags as mf;
        let shared = [
            mf::STICK_TO_OBJECT,
            mf::STANDING_LONG_JUMP,
        ]
        .map(k);
        assert_enum::<MotionFlags>(&shared, KNOWN_MOTION_FLAGS_ACE_ONLY, &[]);
    }
}

#[test]
fn trade_side_matches() {
    use dereth_protocol::trade::trade_side as f;
    let shared = [
        f::SELF,
        f::PARTNER,
    ]
    .map(k);
    assert_enum::<TradeSide>(&shared, &[], &[]);
}

#[test]
fn quadrant_matches_the_hit_location_bits() {
    use dereth_physics::detect::hit_location as f;
    let shared = [
        f::HIGH,
        f::MEDIUM,
        f::LOW,
        f::LEFT,
        f::RIGHT,
        f::FRONT,
        f::BACK,
    ]
    .map(k);
    assert_enum::<Quadrant>(&shared, KNOWN_QUADRANT_ACE_ONLY, &[]);
}

#[test]
fn stat_type_matches_the_quality_stat_types() {
    use dereth_protocol::qualities::stat_type as f;
    let shared = [
        f::INT,
        f::FLOAT,
        f::POSITION,
        f::SKILL,
        f::STRING,
        f::DID,
        f::IID,
        f::ATTRIBUTE,
        f::ATTRIBUTE_2ND,
        f::BODY_DAMAGE_VALUE,
        f::BODY_DAMAGE_VARIANCE,
        f::BODY_ARMOR_VALUE,
        f::BOOL,
        f::INT64,
        f::NUM_STAT_TYPES,
    ]
    .map(k);
    assert_enum::<StatType>(&shared, KNOWN_STAT_TYPE_ACE_ONLY, &[]);
}

#[test]
fn attribute_cache_matches_the_attribute_cache_mask() {
    use dereth_protocol::types::qualities::attribute_cache_mask as f;
    let shared = [
        f::STRENGTH,
        f::ENDURANCE,
        f::QUICKNESS,
        f::COORDINATION,
        f::FOCUS,
        f::SELF,
        f::HEALTH,
        f::STAMINA,
        f::MANA,
        f::FULL,
    ]
    .map(k);
    assert_enum::<AttributeCache>(&shared, KNOWN_ATTRIBUTE_CACHE_ACE_ONLY, &[]);
}

#[test]
fn character_option_data_flags_match_the_player_module_flags() {
    use dereth_protocol::login::player_module_flags as f;
    let shared = [
        f::SHORTCUT,
        f::MULTI_SPELL_LIST,
        f::DESIRED_COMPS,
        f::EXTENDED_MULTI_SPELL_LISTS,
        f::SPELLBOOK_FILTERS,
        f::CHARACTER_OPTIONS_2,
        f::TIMESTAMP_FORMAT,
        f::GENERIC_QUALITIES_DATA,
        f::GAMEPLAY_OPTIONS,
        f::SPELL_LISTS_8,
    ]
    .map(k);
    assert_enum::<CharacterOptionDataFlag>(&shared, KNOWN_CHARACTER_OPTION_DATA_FLAG_ACE_ONLY, &[]);
}

#[test]
fn animation_hook_dir_matches_dereth_animation() {
    use dereth_animation::hooks as f;
    let shared = [
        f::BOTH_ANIMHOOK,
        f::FORWARD_ANIMHOOK,
        f::BACKWARD_ANIMHOOK,
        f::UNKNOWN_ANIMHOOK,
    ]
    .map(k);
    assert_enum::<AnimationHookDir>(&shared, &[], &[]);
}

/// `ItemType` as a value set: the rules name the retail client's members, composites included.
#[test]
fn item_type_matches_the_rules_item_type() {
    use dereth_rules::weenie::item_type as t;
    let shared = [
        t::UNDEF,
        t::MELEE_WEAPON,
        t::ARMOR,
        t::CLOTHING,
        t::VESTEMENTS,
        t::JEWELRY,
        t::CREATURE,
        t::FOOD,
        t::MONEY,
        t::MISC,
        t::MISSILE_WEAPON,
        t::WEAPON,
        t::CONTAINER,
        t::LOCKABLE_MAGIC_TARGET,
        t::USELESS,
        t::GEM,
        t::SPELL_COMPONENTS,
        t::WRITABLE,
        t::KEY,
        t::CASTER,
        t::WEAPON_OR_CASTER,
        t::REDIRECTABLE_ITEM_ENCHANTMENT_TARGET,
        t::PORTAL,
        t::LOCKABLE,
        t::PROMISSORY_NOTE,
        t::MANASTONE,
        t::ITEM_ENCHANTABLE_TARGET,
        t::SERVICE,
        t::MAGIC_WIELDABLE,
        t::ITEM,
        t::CRAFT_COOKING_BASE,
        t::VENDOR_GROCER,
        t::CRAFT_ALCHEMY_BASE,
        t::CRAFT_FLETCHING_BASE,
        t::CRAFT_ALCHEMY_INTERMEDIATE,
        t::CRAFT_FLETCHING_INTERMEDIATE,
        t::LIFESTONE,
        t::PORTAL_MAGIC_TARGET,
        t::TINKERING_TOOL,
        t::TINKERING_MATERIAL,
        t::VENDOR_SHOPKEEP,
        t::GAMEBOARD,
    ]
    .map(k);
    assert_enum::<ItemType>(&shared, &[], &[]);
}

/// `EquipMask` as a value set. ACE's composites differ from the client's
/// (V229); the client's
/// READY_SLOT is ACE's `SelectablePlusAmmo` and its ARMOR is ACE's `ArmorExclusive`.
#[test]
fn equip_mask_matches_the_rules_inventory_loc() {
    use dereth_rules::slots::loc as l;
    let shared = [
        l::NONE,
        l::HEAD_WEAR,
        l::CHEST_WEAR,
        l::ABDOMEN_WEAR,
        l::UPPER_ARM_WEAR,
        l::LOWER_ARM_WEAR,
        l::HAND_WEAR,
        l::UPPER_LEG_WEAR,
        l::LOWER_LEG_WEAR,
        l::FOOT_WEAR,
        l::CHEST_ARMOR,
        l::ABDOMEN_ARMOR,
        l::UPPER_ARM_ARMOR,
        l::LOWER_ARM_ARMOR,
        l::UPPER_LEG_ARMOR,
        l::LOWER_LEG_ARMOR,
        l::ARMOR,
        l::NECK_WEAR,
        l::WRIST_WEAR_LEFT,
        l::WRIST_WEAR_RIGHT,
        l::WRIST_WEAR,
        l::FINGER_WEAR_LEFT,
        l::FINGER_WEAR_RIGHT,
        l::FINGER_WEAR,
        l::MELEE_WEAPON,
        l::SHIELD,
        l::MISSILE_WEAPON,
        l::MISSILE_AMMO,
        l::HELD,
        l::TWO_HANDED,
        l::WEAPON,
        l::WEAPON_READY_SLOT,
        l::READY_SLOT,
        l::TRINKET_ONE,
        l::CLOAK,
        l::CLOTHING,
        l::SIGIL_ONE,
        l::SIGIL_TWO,
        l::SIGIL_THREE,
        l::SIGIL,
        l::JEWELRY,
        l::ALL,
        l::WEARABLE,
        l::WIELDABLE,
    ]
    .map(k);
    assert_enum::<EquipMask>(&shared, KNOWN_EQUIP_MASK_ACE_ONLY, KNOWN_EQUIP_MASK_SHARED_ONLY);
}

/// The skill, attribute, vital and augmentation ids pair by name.
#[test]
fn skill_attribute_vital_and_augmentation_ids_match_the_rules() {
    {
        use dereth_rules::skills::skill;
        for (ace, shared) in [
            (Skill::Jump, k(skill::JUMP)),
            (Skill::Run, k(skill::RUN)),
            (Skill::CreatureEnchantment, k(skill::CREATURE_ENCHANTMENT)),
            (Skill::ItemEnchantment, k(skill::ITEM_ENCHANTMENT)),
            (Skill::LifeMagic, k(skill::LIFE_MAGIC)),
            (Skill::WarMagic, k(skill::WAR_MAGIC)),
            (Skill::TwoHandedCombat, k(skill::TWO_HANDED_COMBAT)),
            (Skill::VoidMagic, k(skill::VOID_MAGIC)),
            (Skill::HeavyWeapons, k(skill::HEAVY_WEAPONS)),
            (Skill::LightWeapons, k(skill::LIGHT_WEAPONS)),
            (Skill::FinesseWeapons, k(skill::FINESSE_WEAPONS)),
            (Skill::MissileWeapons, k(skill::MISSILE_WEAPONS)),
            (Skill::DualWield, k(skill::DUAL_WIELD)),
            (Skill::Recklessness, k(skill::RECKLESSNESS)),
        ] {
            assert_eq!(ace.key(), shared, "{ace:?}");
        }
    }
    {
        use dereth_rules::skills::aug;
        for (ace, shared) in [
            (PropertyInt::AugmentationSkilledMelee, k(aug::SKILLED_MELEE)),
            (PropertyInt::AugmentationSkilledMissile, k(aug::SKILLED_MISSILE)),
            (PropertyInt::AugmentationSkilledMagic, k(aug::SKILLED_MAGIC)),
            (PropertyInt::AugmentationJackOfAllTrades, k(aug::JACK_OF_ALL_TRADES)),
            (PropertyInt::LumAugSkilledSpec, k(aug::LUM_SKILLED_SPEC)),
            (PropertyInt::LumAugAllSkills, k(aug::LUM_ALL_SKILLS)),
            (PropertyInt::AugmentationIncreasedCarryingCapacity, k(aug::INCREASED_CARRYING_CAPACITY)),
            (PropertyInt::Enlightenment, k(aug::ENLIGHTENMENT)),
        ] {
            assert_eq!(ace.key(), shared, "{ace:?}");
        }
    }
    {
        use dereth_rules::attributes::attribute as a;
        for (ace, shared) in [
            (PropertyAttribute::Strength, k(a::STRENGTH)),
            (PropertyAttribute::Endurance, k(a::ENDURANCE)),
            (PropertyAttribute::Quickness, k(a::QUICKNESS)),
            (PropertyAttribute::Coordination, k(a::COORDINATION)),
            (PropertyAttribute::Focus, k(a::FOCUS)),
            (PropertyAttribute::Self_, k(a::SELF)),
        ] {
            assert_eq!(ace.key(), shared, "{ace:?}");
        }
    }
    {
        use dereth_rules::attributes::vital as v;
        for (ace, shared) in [
            (PropertyAttribute2nd::MaxHealth, k(v::MAX_HEALTH)),
            (PropertyAttribute2nd::Health, k(v::HEALTH)),
            (PropertyAttribute2nd::MaxStamina, k(v::MAX_STAMINA)),
            (PropertyAttribute2nd::Stamina, k(v::STAMINA)),
            (PropertyAttribute2nd::MaxMana, k(v::MAX_MANA)),
            (PropertyAttribute2nd::Mana, k(v::MANA)),
        ] {
            assert_eq!(ace.key(), shared, "{ace:?}");
        }
    }
}

/// The public-description bits in `dereth_rules::weenie::bitfield` match ACE's
/// `ObjectDescriptionFlag` by value. The rules keep the retail
/// client's names; ACE's differ for four of them (impenetrable is ACE's `FreePkStatus`, cannot be
/// salvaged is `Retained`, has restrictions is `IncludesSecondHeader`, cell barrier immune is
/// `ImmuneCellRestrictions`).
#[test]
fn the_bitfield_bits_are_aces_object_description_flags() {
    use dereth_rules::weenie::bitfield as b;
    for (ace, shared) in [
        (ObjectDescriptionFlag::Openable, k(b::OPENABLE)),
        (ObjectDescriptionFlag::Inscribable, k(b::INSCRIBABLE)),
        (ObjectDescriptionFlag::Stuck, k(b::STUCK)),
        (ObjectDescriptionFlag::Player, k(b::PLAYER)),
        (ObjectDescriptionFlag::Attackable, k(b::ATTACKABLE)),
        (ObjectDescriptionFlag::PlayerKiller, k(b::PLAYER_KILLER)),
        (ObjectDescriptionFlag::HiddenAdmin, k(b::HIDDEN_ADMIN)),
        (ObjectDescriptionFlag::UiHidden, k(b::UI_HIDDEN)),
        (ObjectDescriptionFlag::Vendor, k(b::VENDOR)),
        (ObjectDescriptionFlag::Corpse, k(b::CORPSE)),
        (ObjectDescriptionFlag::Healer, k(b::HEALER)),
        (ObjectDescriptionFlag::Lockpick, k(b::LOCKPICK)),
        (ObjectDescriptionFlag::FreePkStatus, k(b::IMPENETRABLE)),
        (ObjectDescriptionFlag::Admin, k(b::ADMIN)),
        (ObjectDescriptionFlag::ImmuneCellRestrictions, k(b::CELL_BARRIER_IMMUNE)),
        (ObjectDescriptionFlag::Retained, k(b::CANNOT_BE_SALVAGED)),
        (ObjectDescriptionFlag::PkLiteStatus, k(b::PK_LITE)),
        (ObjectDescriptionFlag::IncludesSecondHeader, k(b::HAS_RESTRICTIONS)),
    ] {
        assert_eq!(ace.key(), shared, "{ace:?}");
    }
}
