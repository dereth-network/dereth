use super::*;
use crate::{RecordingRequests, RecordingSink};
use dereth_primitives::ServerTime;
use dereth_protocol::types::PublicWeenieDesc;

/// The three send sites report level zero, level zero again when the build starts, and then
/// the requested level. A snapshot with equal
/// endpoints cannot distinguish this sequence from no transition.
#[test]
fn power_bar_journal_preserves_same_frame_edges_and_retail_stale_cache() {
    let mut c = CombatState::begin();
    c.begin_power_bar(PowerBarMode::Combat, false, 0);
    c.start_power_bar_build(LocalTime(1.0));
    c.set_power_bar_level(0.75);
    let initial = c.take_power_bar_notices();
    assert_eq!(
        initial,
        vec![
            PowerBarNotice::SetLevel {
                mode: PowerBarMode::Combat,
                level: 0.0
            },
            PowerBarNotice::SetLevel {
                mode: PowerBarMode::Combat,
                level: 0.0
            },
            PowerBarNotice::SetLevel {
                mode: PowerBarMode::Combat,
                level: 0.75
            },
        ]
    );
    c.hide_power_bar();
    assert_eq!(c.latest_power_bar_level, 0.75);
    assert_eq!(c.power_bar_mode, PowerBarMode::Undef);
    c.begin_power_bar(PowerBarMode::Combat, false, 0);
    c.set_power_bar_level(0.75);
    assert_eq!(c.take_power_bar_notices(), initial);
    assert!(c.take_power_bar_notices().is_empty());
}

/// Finishing a jump only sends when `jump_pending` is set, and retains the original
/// mode argument. Neither `finish_jump` nor `hide_power_bar` changes `latest_power_bar_level`.
#[test]
fn finish_jump_journals_original_mode_once_without_zeroing_the_retail_cache() {
    for mode in [
        PowerBarMode::Combat,
        PowerBarMode::AdvancedCombat,
        PowerBarMode::Jump,
    ] {
        let mut c = CombatState::begin();
        c.begin_power_bar(mode, true, 2);
        c.set_power_bar_level(0.625);
        let _ = c.take_power_bar_notices();
        c.finish_jump();
        assert!(
            c.take_power_bar_notices().is_empty(),
            "no jump pending: no Finish notice"
        );
        c.jump_pending = true;
        c.finish_jump();
        assert_eq!(c.latest_power_bar_level, 0.625);
        assert_eq!(c.power_bar_mode, PowerBarMode::Undef);
        assert!(!c.jump_pending);
        assert_eq!(
            c.take_power_bar_notices(),
            vec![if mode == PowerBarMode::Combat {
                PowerBarNotice::SetLevel { mode, level: 0.0 }
            } else {
                PowerBarNotice::Finish { mode }
            }]
        );
        c.finish_jump();
        assert!(c.take_power_bar_notices().is_empty());
    }
}

/// Begin's synchronous consumer sees the mode and skill at emission, even when
/// the host delivers after a later mode/quality update in the same batch.
#[test]
fn attack_begin_notice_retains_its_original_caption_and_skill_context() {
    let mut w = world();
    let player = w.player.unwrap();
    w.tables.weenies.get_mut(player).unwrap().qualities = Some(crate::Qualities::new());
    w.player_qualities_mut().unwrap().set_skill(
        0x32,
        dereth_protocol::types::Skill {
            sac: 2,
            ..dereth_protocol::types::Skill::default()
        },
    );
    w.combat.advanced_combat_mode = true;
    w.attempt_start_building_attack(true, LocalTime(2.0));
    w.combat.combat_mode = CombatMode::Missile;
    w.player_qualities_mut()
        .unwrap()
        .set_skill_advancement_class(0x32, 1);
    assert_eq!(
        w.combat.take_power_bar_notices(),
        vec![
            PowerBarNotice::Begin {
                mode: PowerBarMode::AdvancedCombat,
                melee: true,
                recklessness_sac: 2,
            },
            PowerBarNotice::SetLevel {
                mode: PowerBarMode::AdvancedCombat,
                level: 0.0
            },
        ]
    );
}

fn world() -> World {
    let mut w = World::new();
    w.set_player(ObjectId(1));
    let mut p = crate::weenie::Weenie::new(ObjectId(1));
    p.pwd = PublicWeenieDesc {
        name: "Lark".into(),
        ..PublicWeenieDesc::default()
    };
    w.tables.weenies.insert(ObjectId(1), p);
    let mut m = crate::weenie::Weenie::new(ObjectId(2));
    m.pwd = PublicWeenieDesc {
        name: "Drudge".into(),
        obj_type: item_type::CREATURE,
        bitfield: dereth_rules::weenie::bitfield::ATTACKABLE,
        ..PublicWeenieDesc::default()
    };
    w.tables.weenies.insert(ObjectId(2), m);
    w.tables.inventories.insert(
        ObjectId(1),
        crate::objects::ObjectInventory::new(ObjectId(1)),
    );
    w.combat.combat_mode = CombatMode::Melee;
    let mut out = RecordingSink::default();
    w.set_selected_object(Some(ObjectId(2)), false, &mut out);
    w
}

/// Give the player the `CombatTable` DataID quality retail characters are born with, through
/// the same table `Qualities_PrivateUpdateDataID` writes.
///
/// `0x30000000` is the first of the 71 shipped `.wct` files
/// and is the default character combat-table entry in the data-id mapper, so it is an id a real character carries rather
/// than an arbitrary non-zero.
fn give_combat_table(w: &mut World) {
    let p = w.player.expect("the fixture has a player");
    let we = w.tables.weenies.get_mut(p).expect("and a weenie for it");
    we.qualities.get_or_insert_with(crate::Qualities::new).set(
        crate::qualities::StatKey::new(crate::qualities::StatType::Did, COMBAT_TABLE_DID),
        crate::qualities::StatValue::Did(dereth_primitives::DataId(0x3000_0000)),
    );
    assert_eq!(
        w.combat_table_did(),
        Some(dereth_primitives::DataId(0x3000_0000)),
        "the fixture must actually reach the DataID-4 lookup, or every melee arm below is \
             measuring the absence it was meant to remove"
    );
}

/// **No physics object means not ready, in every mode and both
/// flavours.** The one arm `lenient` cannot rescue.
#[test]
fn a_null_physics_object_answers_false_in_every_mode_and_both_flavours() {
    let mut w = world();
    give_combat_table(&mut w);
    w.combat.current_style = MISSILE_READY_STYLES[0];
    for mode in [
        CombatMode::NonCombat,
        CombatMode::Melee,
        CombatMode::Missile,
        CombatMode::Magic,
    ] {
        w.combat.combat_mode = mode;
        for lenient in [false, true] {
            assert!(
                !w.player_in_ready_position(lenient, None),
                "{mode:?} lenient={lenient}: the player has no physics body"
            );
        }
        // The same world with a body answers true: the absent body caused the false
        // result above, and this fixture can reach the ready state.
        assert!(
            w.player_in_ready_position(true, Some(false)),
            "{mode:?}: the identical world with a settled body IS ready"
        );
    }
}

/// **The shared non-combat/magic arm.** `NONCOMBAT` and `MAGIC` reach
/// `motions_pending` **without reading `lenient` at all**, so the two flavours are the same answer in those two modes.
#[test]
fn noncombat_and_magic_ignore_param_1_and_answer_the_motion_queue() {
    let mut w = world();
    // The table is present so that a melee-shaped answer cannot be mistaken for this one.
    give_combat_table(&mut w);
    for mode in [CombatMode::NonCombat, CombatMode::Magic] {
        w.combat.combat_mode = mode;
        for lenient in [false, true] {
            assert!(
                w.player_in_ready_position(lenient, Some(false)),
                "{mode:?} idle"
            );
            assert!(
                !w.player_in_ready_position(lenient, Some(true)),
                "{mode:?} lenient={lenient}: a pending motion refuses even the lenient \
                     flavour, because this mode does not consult the lenient flag"
            );
        }
    }
}

/// **The measurement the whole flavour split rests on.** One world, one body with a motion
/// outstanding, melee mode, a combat table present: `lenient == true` answers **true** and
/// `lenient == false` answers **false**.
///
/// The attack flavor returns true without testing the motion queue.
/// A build that used one value for both flavors would be more
/// restrictive than retail in exactly this state, and this is the state that says so.
#[test]
fn the_melee_arm_is_where_the_two_flavours_come_apart() {
    let mut w = world();
    w.combat.combat_mode = CombatMode::Melee;

    // No `CombatTable` DataID: the DataID lookup for key 4 leaves the invalid id in place and
    // falls to the release-and-return-false block. `lenient` cannot rescue that either.
    assert_eq!(w.combat_table_did(), None);
    assert!(
        !w.player_in_ready_position(true, Some(false)),
        "no table, lenient"
    );
    assert!(
        !w.player_in_ready_position(false, Some(false)),
        "no table, strict"
    );

    // An explicit `INVALID_DID` is the same answer as an absent property, which is what
    // the invalid-id comparison means and is the whole reason the quality lookup's callers
    // pre-seed the out parameter. A quality that is *present* and zero must still refuse.
    let p = w.player.expect("player");
    w.tables
        .weenies
        .get_mut(p)
        .expect("weenie")
        .qualities
        .get_or_insert_with(crate::Qualities::new)
        .set(
            crate::qualities::StatKey::new(crate::qualities::StatType::Did, COMBAT_TABLE_DID),
            crate::qualities::StatValue::Did(dereth_primitives::DataId(0)),
        );
    assert_eq!(
        w.combat_table_did(),
        None,
        "an explicit INVALID_DID reads as no table"
    );
    assert!(
        !w.player_in_ready_position(true, Some(false)),
        "INVALID_DID, lenient"
    );

    give_combat_table(&mut w);
    assert!(
        w.player_in_ready_position(true, Some(false)),
        "table + idle, lenient"
    );
    assert!(
        w.player_in_ready_position(false, Some(false)),
        "table + idle, strict"
    );

    // The discriminating station.
    assert!(
        w.player_in_ready_position(true, Some(true)),
        "lenient readiness returns without consulting `motions_pending`"
    );
    assert!(
        !w.player_in_ready_position(false, Some(true)),
        "strict readiness checks `motions_pending` and refuses"
    );
}

/// **missile is the strict arm.** The stance must already be one of the six and
/// `forward_command` must be `Ready`; only then does `lenient` matter.
#[test]
fn the_missile_arm_needs_one_of_the_six_styles_and_a_ready_forward_command() {
    let mut w = world();
    give_combat_table(&mut w); // present, and must not help: missile never reads it
    w.combat.combat_mode = CombatMode::Missile;

    w.combat.current_style = 0x8000_003D; // NonCombat -- the stance is not up
    assert!(
        !w.player_in_ready_position(true, Some(false)),
        "NonCombat stance, lenient"
    );

    // The neighbours of the accepted styles, which is what says the case table is a table and
    // not a range: every one of these is a `1` byte, and `0x80000046` is
    // `DualWieldCombat`, which the power bar reads and this arm must not accept.
    for style in [
        0x8000_0040_u32,
        0x8000_0042,
        0x8000_0044,
        0x8000_0045,
        0x8000_0046,
    ] {
        w.combat.current_style = style;
        assert!(
            !w.player_in_ready_position(true, Some(false)),
            "{style:#010X} is a `1` byte in the case table and must refuse"
        );
    }

    for style in [0x8000_0138_u32, 0x8000_0139] {
        w.combat.current_style = style;
        w.combat.forward_command = MOTION_READY;
        assert!(
            !w.player_in_ready_position(true, Some(false)),
            "{style:#010X} is the 2013 table's numbering, not a stance of the final client's"
        );
    }

    for style in MISSILE_READY_STYLES {
        w.combat.current_style = style;
        w.combat.forward_command = MOTION_READY;
        assert!(
            w.player_in_ready_position(true, Some(false)),
            "{style:#010X} idle, lenient"
        );
        assert!(
            w.player_in_ready_position(true, Some(true)),
            "{style:#010X}: lenient readiness skips `motions_pending`"
        );
        assert!(
            !w.player_in_ready_position(false, Some(true)),
            "{style:#010X}: strict readiness refuses pending motion"
        );
        // Only motion-ready (`0x41000003`) passes; every other forward command fails.
        w.combat.forward_command = 0x4000_0004; // WalkForward
        assert!(
            !w.player_in_ready_position(true, Some(false)),
            "{style:#010X}: the stance is up but the body is not in `Ready`"
        );
    }
}

/// Undefined is mode 0, so `mode - 1` is `0xFFFFFFFF` unsigned and fails the range check.
#[test]
fn an_undefined_combat_mode_answers_false() {
    let mut w = world();
    give_combat_table(&mut w);
    w.combat.current_style = MISSILE_READY_STYLES[0];
    w.combat.combat_mode = CombatMode::Undef;
    for lenient in [false, true] {
        assert!(
            !w.player_in_ready_position(lenient, Some(false)),
            "lenient={lenient}"
        );
    }
}

/// Pin literal values so that the six styles, Ready and the
/// DataID key cannot be moved by an edit that keeps every other test symmetric.
///
/// Every number here is retail's: the six styles (offsets 0, 2, 4, 8, 249 and 250 from
/// `0x8000003F`), `Ready`'s `0x41000003`, and the DataID key 4.
#[test]
fn the_ready_position_constants_are_the_ones_in_the_shipped_image() {
    assert_eq!(
        MISSILE_READY_STYLES,
        [
            0x8000_003F,
            0x8000_0041,
            0x8000_0043,
            0x8000_0047,
            0x8000_013B,
            0x8000_013C
        ]
    );
    assert_eq!(MOTION_READY, 0x4100_0003);
    assert_eq!(COMBAT_TABLE_DID, 4);
    // The six are `BowCombat`, `CrossbowCombat`, `SlingCombat`, `ThrownWeaponCombat`,
    // `AtlatlCombat` and `ThrownShieldCombat` -- indices 63, 65, 67, 71, 315 and 316 of the
    // final client's `MotionCommand` name table.
    assert!(!MISSILE_READY_STYLES.contains(&DUAL_WIELD_COMBAT_STYLE));
}

fn enter_advanced_combat(w: &mut World) {
    w.player_system
        .options
        .set(crate::player::options::option::ADVANCED_COMBAT_UI, true);
    assert!(
        !w.combat.advanced_combat_mode,
        "the option alone changes nothing"
    );
    let mut req = RecordingRequests::default();
    let mut sink = RecordingSink::default();
    // `sendToServer = false` is the quality-changed handler's arm, which re-reads the option too.
    w.set_combat_mode(
        &mut req,
        &mut sink,
        CombatMode::NonCombat,
        false,
        true,
        false,
    )
    .unwrap();
    w.set_combat_mode(&mut req, &mut sink, CombatMode::Melee, false, true, false)
        .unwrap();
    assert!(
        w.combat.advanced_combat_mode,
        "mode change re-read the advanced-combat option"
    );
}

/// In classic UI, releasing above the slider cap fires once at released power and once
/// more clamped to the cap.
#[test]
fn end_attack_request_fires_execute_attack_twice_in_the_classic_ui() {
    let mut w = world();
    let mut req = RecordingRequests::default();
    w.combat.ui_requested_power = 0.5;
    w.start_attack_request(true, LocalTime(0.0)).unwrap();
    // Release after a full second: level = 1.0, above the 0.5 cap.
    w.end_attack_request(&mut req, AttackHeight::Medium, None, true, LocalTime(1.0));

    let attacks: Vec<_> = req
        .0
        .iter()
        .filter_map(|r| match r {
            Request::TargetedMeleeAttack(a) => Some(a.power_level),
            _ => None,
        })
        .collect();
    assert_eq!(
        attacks.len(),
        2,
        "two attack messages per request; servers accept it"
    );
    assert_eq!(attacks[0], 1.0, "the first is at the released power");
    assert_eq!(attacks[1], 0.5, "the second is clamped to the slider cap");
}

/// The same release in **advanced** mode fires once: the second `execute_attack` is gated on
/// advanced combat mode being off.
#[test]
fn advanced_combat_mode_fires_once() {
    let mut w = world();
    let mut req = RecordingRequests::default();
    enter_advanced_combat(&mut w);
    w.combat.ui_requested_power = 0.5;
    w.start_attack_request(true, LocalTime(0.0)).unwrap();
    w.end_attack_request(&mut req, AttackHeight::Medium, None, true, LocalTime(1.0));
    assert_eq!(
        req.0
            .iter()
            .filter(|r| matches!(r, Request::TargetedMeleeAttack(_)))
            .count(),
        1
    );
}

/// The power-bar duration is 1.000 seconds, or 0.800 seconds while dual-wielding, and
/// the result is clamped to `[0, 1]`.
#[test]
fn the_power_bar_takes_one_second_or_point_eight_dual_wielding() {
    let mut c = CombatState::begin();
    assert_eq!(c.power_bar_level(LocalTime(5.0)), 0.0, "no build, no level");
    c.start_power_bar_build(LocalTime(10.0));
    assert_eq!(c.power_bar_level(LocalTime(10.0)), 0.0);
    assert_eq!(c.power_bar_level(LocalTime(10.5)), 0.5);
    assert_eq!(c.power_bar_level(LocalTime(11.0)), 1.0);
    assert_eq!(c.power_bar_level(LocalTime(99.0)), 1.0, "clamped");
    c.current_style = DUAL_WIELD_COMBAT_STYLE;
    assert_eq!(c.power_bar_level(LocalTime(10.8)), 1.0);
    assert!((c.power_bar_level(LocalTime(10.4)) - 0.5).abs() < 1e-6);
}

/// Jump power floors at `MIN_JUMP_EXTENT` only while a jump is pending.
#[test]
fn the_jump_bar_floors_at_the_minimum_extent() {
    let mut c = CombatState::begin();
    c.start_power_bar_build(LocalTime(0.0));
    assert_eq!(c.jump_power_level(LocalTime(0.0)), 0.0, "not jumping");
    c.jump_pending = true;
    assert_eq!(c.jump_power_level(LocalTime(0.0)), MIN_JUMP_EXTENT);
}

/// Combat compatibility, including the melee rule's
/// three-way condition.
#[test]
fn combat_mode_compatibility_follows_the_inventory_mask() {
    let mut w = world();
    assert!(w.compatible_combat_mode(CombatMode::NonCombat));
    assert!(!w.compatible_combat_mode(CombatMode::Missile));
    w.inventory_mask = loc::MISSILE_WEAPON;
    assert!(w.compatible_combat_mode(CombatMode::Missile));
    assert!(!w.compatible_combat_mode(CombatMode::Magic));
    w.inventory_mask |= loc::HELD;
    assert!(w.compatible_combat_mode(CombatMode::Magic));

    // Melee: a bow in the ready slot with no melee or two-handed weapon refuses.
    w.inventory_mask = loc::MISSILE_WEAPON; // MISSILE_WEAPON is inside WEAPON_READY_SLOT
    assert!(!w.compatible_combat_mode(CombatMode::Melee));
    w.inventory_mask |= loc::MELEE_WEAPON;
    assert!(w.compatible_combat_mode(CombatMode::Melee));
    // Nothing wielded at all: melee is fine (unarmed).
    w.inventory_mask = 0;
    assert!(w.compatible_combat_mode(CombatMode::Melee));
}

/// The attack target is the selection, unless it is owned by the
/// player.
#[test]
fn the_attack_target_excludes_the_players_own_things() {
    let mut w = world();
    assert_eq!(w.get_attack_target(), Some(ObjectId(2)));
    w.weenie_mut(ObjectId(2)).unwrap().pwd.container_id = Some(ObjectId(1));
    assert_eq!(
        w.get_attack_target(),
        None,
        "an owned object is never an attack target"
    );
}

/// Classic UI refuses to start a swing with no valid target; advanced UI
/// skips the check entirely.
#[test]
fn the_target_check_is_skipped_in_advanced_mode() {
    let mut w = world();
    let mut out = RecordingSink::default();
    w.set_selected_object(None, false, &mut out);
    assert_eq!(
        w.start_attack_request(true, LocalTime(0.0)),
        Err("You must select a valid combat target before attacking")
    );
    enter_advanced_combat(&mut w);
    assert_eq!(w.start_attack_request(true, LocalTime(0.0)), Ok(()));
}

/// The hit adjective table boundaries belong to the lower bucket.
#[test]
fn the_hit_adjective_table_boundaries_belong_to_the_lower_bucket() {
    assert_eq!(
        combat_hit_adjectives(damage_type::SLASH, 0.09),
        ("scratch", "scratches")
    );
    assert_eq!(
        combat_hit_adjectives(damage_type::SLASH, 0.10),
        ("scratch", "scratches"),
        "exactly 0.10 is still the first bucket"
    );
    assert_eq!(
        combat_hit_adjectives(damage_type::SLASH, 0.1001),
        ("cut", "cuts")
    );
    assert_eq!(
        combat_hit_adjectives(damage_type::SLASH, 0.25),
        ("cut", "cuts"),
        "exactly 0.25 falls in the SECOND bucket, not the third"
    );
    assert_eq!(
        combat_hit_adjectives(damage_type::SLASH, 0.2501),
        ("slash", "slashes")
    );
    assert_eq!(
        combat_hit_adjectives(damage_type::SLASH, 0.50),
        ("slash", "slashes"),
        "and exactly 0.50 in the third, not the fourth"
    );
    assert_eq!(
        combat_hit_adjectives(damage_type::SLASH, 0.5001),
        ("mangle", "mangles")
    );
    assert_eq!(
        combat_hit_adjectives(damage_type::SLASH, -0.1),
        ("hit", "hits")
    );
    assert_eq!(combat_hit_adjectives(0x1234, 0.9), ("hit", "hits"));
    assert_eq!(
        combat_hit_adjectives(damage_type::SLASH | damage_type::PIERCE, 0.9),
        ("hit", "hits"),
        "the switch sends 3 to the default: no lowest-bit decode"
    );
    assert_eq!(
        combat_hit_adjectives(damage_type::NETHER, 0.9),
        ("eradicate", "eradicates")
    );
    assert_eq!(
        combat_hit_adjectives(damage_type::HEALTH, 0.0),
        ("drain", "drains")
    );
}

/// Oracle: `"Critical hit!  "` has **two** trailing spaces in the attacker
/// message and one in the defender message.
#[test]
fn the_critical_hit_prefix_has_two_spaces_for_the_attacker_and_one_for_the_defender() {
    let a = attacker_notification_line("Drudge", damage_type::SLASH, 0.6, 42, true, 0);
    assert!(a.starts_with("Critical hit!  You mangle Drudge"), "{a}");
    let d = defender_notification_line("Drudge", damage_type::SLASH, 0.6, 42, 0, true, 0);
    assert!(
        d.starts_with("Critical hit! Drudge mangles your head"),
        "{d}"
    );
    assert_ne!(&a[..15], &d[..15]);
}

/// Both attack-notification message formats, field by field.
#[test]
fn the_notification_lines_match_the_documented_format() {
    assert_eq!(
        attacker_notification_line("Drudge", damage_type::FIRE, 0.3, 1, false, 0),
        "You burn Drudge for 1 point of fire damage!\n"
    );
    assert_eq!(
        attacker_notification_line("Drudge", damage_type::HEALTH, 0.3, 7, false, 0),
        "You siphon Drudge for 7 points of damage!\n",
        "HEALTH has an adjective row and no name, so the word is empty -- and the \
             space that used to be emitted for it was a double space"
    );
    assert_eq!(
        attacker_notification_line("Drudge", 0, 0.3, 7, false, 0),
        "You hit Drudge for 7 points of damage!\n",
        "an undefined damage type contributes no type word at all"
    );
    assert_eq!(
        attacker_notification_line(
            "Drudge",
            damage_type::SLASH,
            0.6,
            9,
            true,
            attack_conditions::SNEAK_ATTACK
                | attack_conditions::RECKLESSNESS
                | attack_conditions::CRITICAL_PROTECTION
        ),
        "Critical hit!  Sneak Attack! Recklessness! You mangle Drudge for 9 points of slashing \
             damage! Your target's Critical Protection augmentation allows them to avoid your \
             critical hit!\n"
    );
    assert_eq!(
        defender_notification_line("Drudge", damage_type::PIERCE, 0.05, 3, 3, false, 0),
        "Drudge nicks your upper arm for 3 points of piercing damage!\n"
    );
    assert_eq!(
        evasion_attacker_line("Drudge"),
        "Drudge evaded your attack.\n"
    );
    assert_eq!(evasion_defender_line("Drudge"), "You evaded Drudge!\n");
}

/// Condition bit `0x8` says "Overpower! " on both lines, after the critical prefix and before
/// the sneak-attack one; alone it is the only prefix.
#[test]
fn an_overpowering_hit_says_so_on_both_lines_in_its_place() {
    let all = attack_conditions::OVERPOWER
        | attack_conditions::SNEAK_ATTACK
        | attack_conditions::RECKLESSNESS;
    assert_eq!(
        attacker_notification_line("Drudge", damage_type::SLASH, 0.6, 9, true, all),
        "Critical hit!  Overpower! Sneak Attack! Recklessness! You mangle Drudge for 9 points \
             of slashing damage!\n"
    );
    assert_eq!(
        defender_notification_line("Drudge", damage_type::PIERCE, 0.05, 3, 3, true, all),
        "Critical hit! Overpower! Sneak Attack! Reckless! Drudge nicks your upper arm for 3 \
             points of piercing damage!\n"
    );
    assert_eq!(
        attacker_notification_line(
            "Drudge",
            damage_type::FIRE,
            0.3,
            1,
            false,
            attack_conditions::OVERPOWER
        ),
        "Overpower! You burn Drudge for 1 point of fire damage!\n"
    );
    assert_eq!(
        attack_conditions::OVERPOWER,
        dereth_protocol::combat::attack_conditions::OVERPOWER
    );
}

/// Damage-type and body-part display text.
#[test]
fn damage_types_join_with_slashes_and_body_parts_lower_case() {
    assert_eq!(damage_type_to_string(damage_type::SLASH), "Slashing");
    assert_eq!(
        damage_type_to_string(damage_type::SLASH | damage_type::FIRE | damage_type::NETHER),
        "Slashing/Fire/Nether"
    );
    assert_eq!(damage_type_to_string(damage_type::BASE), "Prismatic");
    assert_eq!(damage_type_to_string(0), "");
    assert_eq!(body_part_to_string(0x18), "upper tentacle");
    assert_eq!(body_part_to_string(11), "unknown", "11 and 14 are gaps");
    assert_eq!(body_part_to_string(14), "unknown");
    assert_eq!(body_part_to_string(0x1b), "num");
    assert_eq!(
        body_part_to_string(0x1c),
        "unknown",
        "past the last entry, 0x1b"
    );
    assert_eq!(body_part_to_string(u32::MAX), "undefined");
    // The 64-byte buffer both notification handlers pass to `damage_type_to_string`.
    let eight = damage_type::SLASH
        | damage_type::PIERCE
        | damage_type::BLUDGEON
        | damage_type::COLD
        | damage_type::FIRE
        | damage_type::ACID
        | damage_type::ELECTRIC
        | damage_type::NETHER;
    assert_eq!(damage_type_to_string(eight).len() + 1, 63);
    assert_eq!(
        notification_damage_word(eight),
        "slashing/piercing/bludgeoning/cold/fire/acid/electrical/nether "
    );
    assert_eq!(
        damage_type_to_string(eight | damage_type::BASE).len() + 1,
        73
    );
    assert_eq!(
        notification_damage_word(eight | damage_type::BASE),
        "",
        "73 > 0x40, so the retail client writes only the terminator"
    );
    assert_eq!(notification_damage_word(damage_type::STAMINA), "");
    assert_eq!(notification_damage_word(damage_type::MANA), "");
}

/// Weapon-speed descriptions across four deliberately asymmetric ranges.
#[test]
fn weapon_time_adjectives_use_the_documented_boundaries() {
    assert_eq!(weapon_time_to_string(10), "Very Fast");
    assert_eq!(weapon_time_to_string(11), "Fast");
    assert_eq!(weapon_time_to_string(30), "Fast");
    assert_eq!(weapon_time_to_string(31), "Average");
    assert_eq!(weapon_time_to_string(49), "Average");
    assert_eq!(weapon_time_to_string(50), "Slow");
    assert_eq!(weapon_time_to_string(79), "Slow");
    assert_eq!(weapon_time_to_string(80), "Very Slow");
}

/// Attack commencement and completion update the request guard:
/// `attack_in_progress` blocks every inventory request while it is set.
#[test]
fn an_attack_in_progress_blocks_inventory_requests() {
    let mut w = world();
    let mut req = RecordingRequests::default();
    assert_eq!(w.ready_for_inventory_request(), Ok(()));
    w.handle_commence_attack();
    assert_eq!(
        w.ready_for_inventory_request(),
        Err("You cannot move or use an item while attacking")
    );
    w.handle_attack_done(&mut req, 0, true, LocalTime(1.0));
    assert_eq!(w.ready_for_inventory_request(), Ok(()));
    let _ = ServerTime(0.0);
}

/// Oracle: the commence-attack handler raises the busy count on every commence; the
/// attack-done handler lowers it only when a swing is in progress.
///
/// **The busy cursor is up from the commenced swing to its end**, and an attack-done with no
/// swing in progress takes nothing down.
#[test]
fn a_commenced_swing_holds_the_busy_count_until_the_attack_is_done() {
    let mut w = world();
    let mut req = RecordingRequests::default();
    w.handle_attack_done(&mut req, 0, true, LocalTime(0.5));
    assert_eq!(w.magic.busy_count, 0, "no swing, nothing to take down");
    w.handle_commence_attack();
    assert_eq!(w.magic.busy_count, 1, "the swing is outstanding");
    w.handle_attack_done(&mut req, 0, true, LocalTime(1.0));
    assert_eq!(w.magic.busy_count, 0, "the swing is over");
    w.handle_attack_done(&mut req, 0, true, LocalTime(1.5));
    assert_eq!(
        w.magic.busy_count, 0,
        "a second attack-done is not a second swing"
    );
}

/// Begin resets to the documented values.
#[test]
fn begin_resets_to_the_documented_values() {
    let c = CombatState::begin();
    assert_eq!(c.ui_requested_power, 0.5);
    assert_eq!(c.combat_mode, CombatMode::NonCombat);
    assert_eq!(c.pending_combat_mode, CombatMode::Undef);
    assert_eq!(
        c.pending_combat_mode.raw(),
        0,
        "UNDEF_COMBAT_MODE is 0, as a literal"
    );
    assert_eq!(c.requested_attack_height, AttackHeight::Medium);
    assert_eq!(c.power_bar_mode, PowerBarMode::Undef);
    assert!(!c.target_willingly_lost);
}

/// Combat enum values.
#[test]
fn the_enums_have_the_documented_values() {
    assert_eq!(CombatMode::NonCombat.raw(), 1);
    assert_eq!(CombatMode::Melee.raw(), 2);
    assert_eq!(CombatMode::Missile.raw(), 4);
    assert_eq!(CombatMode::Magic.raw(), 8);
    assert_eq!(
        CombatMode::COMBAT,
        CombatMode::Melee.raw() | CombatMode::Missile.raw() | CombatMode::Magic.raw()
    );
    assert_eq!(CombatMode::VALID, 15);
    assert_eq!(CombatMode::Undef.raw(), 0);
    assert_eq!(AttackHeight::Low as u32, 3);
    assert_eq!(PowerBarMode::Ddd as u32, 4);
    assert_eq!(damage_type::BASE, 0x1000_0000);
    assert_eq!(combat_use::TWO_HANDED, 5);
    assert_eq!(CombatMode::Melee.name(), "melee");
    assert_eq!(CombatMode::Undef.name(), "unknown");
}

/// Every `Combat_ChangeCombatMode` in the sink, as its raw mode word.
fn modes_sent(req: &RecordingRequests) -> Vec<u32> {
    req.0
        .iter()
        .filter_map(|r| match r {
            Request::ChangeCombatMode(m) => Some(m.combat_mode),
            _ => None,
        })
        .collect()
}

/// A combat mode toggle pressed before the body is ready is retried not dropped.
#[test]
fn a_combat_mode_toggle_pressed_before_the_body_is_ready_is_retried_not_dropped() {
    let mut w = world(); // starts in melee
    let mut req = RecordingRequests::default();
    let mut sink = crate::RecordingSink::default();

    // The player presses the toggle while a motion is pending: peace is queued, nothing sent.
    let (mode, refusal) = w.toggle_combat_mode_target(false);
    assert_eq!(mode, CombatMode::NonCombat);
    assert!(refusal.is_none());
    assert_eq!(
        w.set_combat_mode(&mut req, &mut sink, mode, true, false, false),
        Ok(())
    );
    assert_eq!(
        w.combat.pending_combat_mode,
        CombatMode::NonCombat,
        "queued"
    );
    assert_eq!(
        w.combat.combat_mode,
        CombatMode::Melee,
        "and not applied locally"
    );
    assert_eq!(
        modes_sent(&req),
        Vec::<u32>::new(),
        "0 of 1 sent while not ready"
    );

    // Frames keep running while the body is still busy: still queued, still nothing sent.
    for _ in 0..10 {
        assert_eq!(w.combat_use_time(&mut req, &mut sink, false), None);
    }
    assert_eq!(
        w.combat.pending_combat_mode,
        CombatMode::NonCombat,
        "still queued after 10"
    );
    assert_eq!(
        modes_sent(&req),
        Vec::<u32>::new(),
        "0 of 1 sent across 10 not-ready frames"
    );

    // The first ready frame retries it.
    assert_eq!(w.combat_use_time(&mut req, &mut sink, true), None);
    assert_eq!(w.combat.combat_mode, CombatMode::NonCombat);
    assert_eq!(
        w.combat.pending_combat_mode,
        CombatMode::Undef,
        "cleared by UseTime"
    );
    assert_eq!(
        modes_sent(&req),
        vec![1],
        "1 of 1 sent; NONCOMBAT_COMBAT_MODE is 1"
    );

    // And it is one-shot: later frames send nothing more.
    for _ in 0..10 {
        assert_eq!(w.combat_use_time(&mut req, &mut sink, true), None);
    }
    assert_eq!(
        modes_sent(&req),
        vec![1],
        "still exactly 1 after 10 more ready frames"
    );
}

/// The pending mode is a single field, not a queue. Each request overwrites it, so two
/// requests before the body is ready produce one
/// message, carrying the **second** mode.
#[test]
fn two_combat_mode_requests_before_ready_collapse_to_the_second_one() {
    let mut w = world();
    w.inventory_mask = loc::MISSILE_WEAPON | loc::HELD | loc::MELEE_WEAPON;
    let mut req = RecordingRequests::default();
    let mut sink = crate::RecordingSink::default();

    assert_eq!(
        w.set_combat_mode(&mut req, &mut sink, CombatMode::Missile, true, false, false),
        Ok(())
    );
    assert_eq!(w.combat.pending_combat_mode, CombatMode::Missile);
    assert_eq!(
        w.set_combat_mode(&mut req, &mut sink, CombatMode::Magic, true, false, false),
        Ok(())
    );
    assert_eq!(
        w.combat.pending_combat_mode,
        CombatMode::Magic,
        "overwritten, not queued"
    );

    assert_eq!(w.combat_use_time(&mut req, &mut sink, true), None);
    assert_eq!(
        modes_sent(&req),
        vec![8],
        "one message, MAGIC_COMBAT_MODE = 8"
    );
    assert_eq!(w.combat.combat_mode, CombatMode::Magic);
}

/// Oracle: `UseTime`'s clear is **outside** the `SetCombatMode` call, so a queued mode that has
/// become incompatible is refused once and then dropped rather than retried on every frame.
#[test]
fn a_pending_mode_that_became_incompatible_is_refused_once_and_then_dropped() {
    let mut w = world();
    w.inventory_mask = loc::MISSILE_WEAPON;
    let mut req = RecordingRequests::default();
    let mut sink = crate::RecordingSink::default();
    assert_eq!(
        w.set_combat_mode(&mut req, &mut sink, CombatMode::Missile, true, false, false),
        Ok(())
    );

    // The bow is stowed before the body comes to rest.
    w.inventory_mask = 0;
    assert_eq!(
        w.combat_use_time(&mut req, &mut sink, true),
        Some("You can't enter missile mode".to_string()),
        "the combat-mode check refuses, and the client puts the text on the scroll"
    );
    assert_eq!(
        w.combat.pending_combat_mode,
        CombatMode::Undef,
        "dropped, not re-armed"
    );
    assert_eq!(
        w.combat.combat_mode,
        CombatMode::Melee,
        "and the mode did not change"
    );
    assert_eq!(modes_sent(&req), Vec::<u32>::new());
    assert_eq!(
        w.combat_use_time(&mut req, &mut sink, true),
        None,
        "and never refuses twice"
    );
}

/// The retry runs the whole mode-change tail before clearing the pending mode.
/// A callback observing or changing that field distinguishes
/// an after-tail clear from an early one. The reentrant request is an ordering probe,
/// not a claim that the application's currently deferred notice bus runs synchronously.
#[test]
fn a_pending_retry_finishes_its_mode_change_tail_before_clearing_pending() {
    let mut w = world();
    let mut req = RecordingRequests::default();
    let mut sink = RecordingSink::default();
    w.set_combat_mode(
        &mut req,
        &mut sink,
        CombatMode::NonCombat,
        true,
        false,
        false,
    )
    .unwrap();
    let mut callbacks = 0;
    assert_eq!(
        w.combat_use_time_with_mode_change(&mut req, &mut sink, true, |w, out| {
            callbacks += 1;
            assert_eq!(w.combat.combat_mode, CombatMode::NonCombat);
            assert_eq!(w.combat.pending_combat_mode, CombatMode::NonCombat);
            // A selection effect in the mode-change tail can notify other subscribers.
            w.set_selected_object(None, false, out);
            // Model a subscriber queuing a new request while the tail still owns control.
            w.set_combat_mode(
                &mut crate::NullRequests,
                out,
                CombatMode::Melee,
                true,
                false,
                false,
            )
            .unwrap();
            assert_eq!(w.combat.pending_combat_mode, CombatMode::Melee);
        }),
        None,
    );
    assert_eq!(callbacks, 1);
    assert_eq!(w.selected, None);
    assert!(sink
        .0
        .iter()
        .any(|n| matches!(n, crate::Notice::SelectionChanged { .. })));
    assert_eq!(
        w.combat.pending_combat_mode,
        CombatMode::Undef,
        "UseTime's final clear wins"
    );
    assert_eq!(modes_sent(&req), vec![1]);
}

/// Oracle: the client's pending/ready guards and the client's equality
/// and compatibility returns all precede the mode-change tail.
#[test]
fn a_pending_retry_does_not_call_the_tail_without_a_mode_change() {
    let mut w = world();
    let mut req = RecordingRequests::default();
    let mut sink = RecordingSink::default();
    for (pending, ready, clears, refuses) in [
        (CombatMode::Undef, true, true, false),
        (CombatMode::NonCombat, false, false, false),
        (CombatMode::Melee, true, true, false),
        (CombatMode::Missile, true, true, true),
    ] {
        w.combat.pending_combat_mode = pending;
        let refusal = w.combat_use_time_with_mode_change(&mut req, &mut sink, ready, |_, _| {
            panic!("{pending:?}, ready={ready}: no mode change may reach the tail")
        });
        assert_eq!(refusal.is_some(), refuses);
        assert_eq!(w.combat.combat_mode, CombatMode::Melee);
        assert_eq!(
            w.combat.pending_combat_mode,
            if clears { CombatMode::Undef } else { pending },
        );
    }
    assert!(modes_sent(&req).is_empty());
}

/// A server driven mode change does not discard the players queued request.
#[test]
fn a_server_driven_mode_change_does_not_discard_the_players_queued_request() {
    let mut w = world();
    w.inventory_mask = loc::MISSILE_WEAPON | loc::MELEE_WEAPON;
    let mut req = RecordingRequests::default();
    let mut sink = crate::RecordingSink::default();
    assert_eq!(
        w.set_combat_mode(&mut req, &mut sink, CombatMode::Missile, true, false, false),
        Ok(())
    );
    assert_eq!(w.combat.pending_combat_mode, CombatMode::Missile);

    // The server says the character is in peace mode. `sendToServer = false`, so no message and
    // no ready check — the quality is authoritative.
    assert_eq!(
        w.set_combat_mode(
            &mut req,
            &mut sink,
            CombatMode::NonCombat,
            false,
            false,
            false
        ),
        Ok(())
    );
    assert_eq!(w.combat.combat_mode, CombatMode::NonCombat);
    assert_eq!(modes_sent(&req), Vec::<u32>::new(), "nothing is sent back");
    assert_eq!(
        w.combat.pending_combat_mode,
        CombatMode::Missile,
        "the player's own request survives the server's update"
    );

    assert_eq!(w.combat_use_time(&mut req, &mut sink, true), None);
    assert_eq!(modes_sent(&req), vec![4], "MISSILE_COMBAT_MODE = 4");
}

/// Setting combat mode reads the advanced-combat option and stores its bit. The option getter
/// reads bit 12 from the option word.
///
/// **`CombatState::advanced_combat_mode` had no production writer**: its only two writers in
/// the workspace were assignments in this file's tests standing in for a missing producer.
/// Both now use [`enter_advanced_combat`],
/// which drives this.
///
/// Asserted at two stations *in each direction*, because a producer that only ever sets and a
/// producer that only ever clears both pass a one-station harness, and because the field's six
/// readers all branch on it. The literal `0x1000` is pinned here so a wrong bit cannot hide
/// behind the symbol.
#[test]
fn set_combat_mode_re_reads_the_advanced_combat_ui_option_in_both_directions() {
    use crate::player::options::{option, PLAYER_OPTIONS};
    // The bit tests, as a literal.
    assert_eq!(option::ADVANCED_COMBAT_UI, 12);
    assert_eq!(PLAYER_OPTIONS[12].0, "AdvancedCombatUI");
    assert_eq!(
        PLAYER_OPTIONS[12].2, 0x0000_1000,
        "1 << 12, bit 12 of the first option word"
    );

    let mut w = world();
    let mut req = RecordingRequests::default();
    let mut sink = RecordingSink::default();

    // Station A: the option is off and the field is false after a mode change.
    assert!(!w.player_system.options.advanced_combat_ui());
    w.set_combat_mode(
        &mut req,
        &mut sink,
        CombatMode::NonCombat,
        false,
        true,
        false,
    )
    .unwrap();
    assert!(!w.combat.advanced_combat_mode, "A: off");

    // Station B: turn the option on. **Nothing happens until a mode change** — the client
    // re-reads it inside `SetCombatMode` and nowhere else.
    w.player_system
        .options
        .set(option::ADVANCED_COMBAT_UI, true);
    assert!(
        !w.combat.advanced_combat_mode,
        "B: the option alone does not move the field"
    );
    w.set_combat_mode(&mut req, &mut sink, CombatMode::Missile, false, true, false)
        .unwrap();
    assert!(
        w.combat.advanced_combat_mode,
        "B: and the mode change picks it up"
    );

    // Station C: and back down again, on the `sendToServer = false` arm, which is the one
    // the quality-changed handler uses — the re-read is outside the `sendToServer` test.
    w.player_system
        .options
        .set(option::ADVANCED_COMBAT_UI, false);
    w.set_combat_mode(
        &mut req,
        &mut sink,
        CombatMode::NonCombat,
        false,
        true,
        false,
    )
    .unwrap();
    assert!(
        !w.combat.advanced_combat_mode,
        "C: cleared, and by the server-driven arm"
    );
}

/// Selection fixup after a combat-mode change.
///
/// Four cases, and the two that must *not* change anything are the point: a fixup that always
/// cleared the selection would satisfy "an unattackable selection is cleared" on its own.
#[test]
fn entering_combat_clears_a_selection_that_is_not_attackable_and_leaves_the_rest_alone() {
    // (a) an attackable creature survives the change.
    let mut w = world(); // ObjectId(2) is an attackable drudge, and is selected
    let mut req = RecordingRequests::default();
    let mut sink = RecordingSink::default();
    w.combat.combat_mode = CombatMode::NonCombat;
    w.set_combat_mode(&mut req, &mut sink, CombatMode::Melee, false, true, false)
        .unwrap();
    assert_eq!(
        w.selected,
        Some(ObjectId(2)),
        "a: an attackable target is kept"
    );
    assert!(
        !sink
            .0
            .iter()
            .any(|n| matches!(n, crate::Notice::SelectionChanged { .. })),
        "a: and `force = 0` means no notice for an unchanged selection"
    );

    // (b) a creature that is not attackable is dropped. Same object, `BF_ATTACKABLE` cleared,
    // which is the difference between a drudge and a townsfolk.
    let mut w = world();
    let mut req = RecordingRequests::default();
    let mut sink = RecordingSink::default();
    w.weenie_mut(ObjectId(2)).unwrap().pwd.bitfield = 0;
    w.combat.combat_mode = CombatMode::NonCombat;
    w.set_combat_mode(&mut req, &mut sink, CombatMode::Missile, false, true, false)
        .unwrap();
    assert_eq!(w.selected, None, "b: an unattackable selection is cleared");
    assert_eq!(
        sink.0
            .iter()
            .filter(|n| matches!(n, crate::Notice::SelectionChanged { .. }))
            .count(),
        1,
        "b: exactly one SelectionChanged"
    );

    // (c) **the same object, leaving combat**: the fixup is gated on the new mode, so peace
    // and magic must leave even an unattackable selection alone. This is the second station,
    // and without it (b) would also pass against a fixup that ran in every mode.
    let mut w = world();
    let mut req = RecordingRequests::default();
    let mut sink = RecordingSink::default();
    w.weenie_mut(ObjectId(2)).unwrap().pwd.bitfield = 0;
    w.combat.combat_mode = CombatMode::Missile;
    w.set_combat_mode(
        &mut req,
        &mut sink,
        CombatMode::NonCombat,
        false,
        true,
        false,
    )
    .unwrap();
    assert_eq!(
        w.selected,
        Some(ObjectId(2)),
        "c: peace does not touch the selection"
    );
    w.set_combat_mode(&mut req, &mut sink, CombatMode::Magic, false, true, false)
        .unwrap();
    assert_eq!(w.selected, Some(ObjectId(2)), "c: nor does magic");

    // (d) the player's own selection survives, because the guard is `selected != player`
    // and `get_attack_target` would answer `None` for it — the fixup would otherwise clear it.
    let mut w = world();
    let mut req = RecordingRequests::default();
    let mut sink = RecordingSink::default();
    w.set_selected_object(Some(ObjectId(1)), false, &mut sink);
    w.combat.combat_mode = CombatMode::NonCombat;
    w.set_combat_mode(&mut req, &mut sink, CombatMode::Melee, false, true, false)
        .unwrap();
    assert_eq!(
        w.selected,
        Some(ObjectId(1)),
        "d: the player keeps himself selected"
    );
}

/// Run one combat-update frame at `t`.
fn frame(w: &mut World, req: &mut RecordingRequests, t: f64) {
    w.combat_power_bar_use_time(req, true, LocalTime(t));
}

fn attacks(req: &RecordingRequests) -> Vec<f32> {
    req.0
        .iter()
        .filter_map(|r| match r {
            Request::TargetedMeleeAttack(a) => Some(a.power_level),
            _ => None,
        })
        .collect()
}

fn cancels(req: &RecordingRequests) -> usize {
    req.0
        .iter()
        .filter(|r| matches!(r, Request::CancelAttack(_)))
        .count()
}

/// **A single click starts the charge, and the swing happens frames later, on arrival.**
///
/// Press and release inside one frame, then wait for the bar to reach the requested level.
/// The attack must fire without another input edge.
///
/// The gauge is left at its default of **0.5**, so the arrival is at the
/// halfway point of a 1.000 s charge and the frame it lands on is a fact about the clock
/// rather than about the frame rate.
#[test]
fn a_click_starts_the_charge_and_the_attack_fires_when_the_bar_reaches_the_requested_level() {
    let mut w = world();
    let mut req = RecordingRequests::default();

    assert!(
        (w.combat.ui_requested_power - 0.5).abs() < 1e-9,
        "the shipped gauge default"
    );

    // The click. `0x1C` / the key press -> `set_requested_attack_height` -> `start_attack_request`;
    // the completed click -> `end_attack_request(height, -1.0)`. Both inside one frame, which is
    // what "a single click" means.
    w.set_requested_attack_height(AttackHeight::High, true, LocalTime(10.0))
        .unwrap();
    assert!(w.combat.build_in_progress, "the press started the bar");
    assert!(w.combat.attack_request_in_progress);
    w.end_attack_request(&mut req, AttackHeight::High, None, true, LocalTime(10.0));

    // **The release fired nothing.** This is the assertion the held reading fails.
    assert!(
        attacks(&req).is_empty(),
        "a click does not swing on the release edge"
    );
    assert!(
        !w.combat.attack_request_in_progress,
        "but the request did end"
    );
    assert!(w.combat.build_in_progress, "and the bar is still charging");
    assert!(
        (w.combat.requested_attack_power - 0.5).abs() < 1e-9,
        "ending the attack request took max(level, cap) and level was 0"
    );

    // Now the frames. Each one is a separate observation, and the silence before arrival is
    // asserted per frame rather than once at the end: a swing that fired early and a swing
    // that fired late both pass an "eventually one attack" test.
    for i in 1..=4 {
        frame(&mut w, &mut req, 10.0 + f64::from(i) * 0.1);
        assert!(
            attacks(&req).is_empty(),
            "frame {i}: still charging, nothing sent"
        );
        assert!(
            w.combat.build_in_progress,
            "frame {i}: and the build is still live"
        );
        let expected = f64::from(i) * 0.1;
        #[allow(clippy::cast_possible_truncation)]
        let expected = expected as f32;
        assert!(
            (w.combat.latest_power_bar_level - expected).abs() < 1e-5,
            "frame {i}: the bar follows the clock, not the cap"
        );
    }

    // t = 10.5: level == 0.5 == the cap. Retail tests `level < requested`, so
    // equality is **arrival**, not "still charging". The boundary is the whole behaviour.
    frame(&mut w, &mut req, 10.5);
    assert_eq!(
        attacks(&req),
        vec![0.5],
        "one attack, at the requested level"
    );
    assert!(
        !w.combat.build_in_progress,
        "executing the attack ends the build"
    );
    assert!(
        (w.combat.latest_power_bar_level - 0.5).abs() < 1e-9,
        "the bar is pinned at the cap, not at the clock"
    );

    // And it does not fire again on the next frame.
    frame(&mut w, &mut req, 10.6);
    assert_eq!(attacks(&req).len(), 1, "exactly one swing per click");
}

/// **A frame that lands *past* the arrival still swings at the requested level, not at the
/// clock's.** `min(requested_attack_power, level)`.
///
/// **Added because a mutation survived.** The test above steps the clock so that the arrival
/// frame has `level == requested` exactly, and at equality `min` is the identity — so dropping
/// it changed nothing there. A real frame almost never lands on the boundary: the arrival is a
/// clock time and the frames are wherever the frame rate puts them, so the overshoot is the
/// *normal* case and the equality is the special one. Reading 1 of the four:
/// the test was weak, and this is the assertion that would have caught it.
#[test]
fn a_frame_that_overshoots_the_arrival_still_pins_the_bar_at_the_requested_level() {
    let mut w = world();
    let mut req = RecordingRequests::default();
    w.set_requested_attack_height(AttackHeight::Medium, true, LocalTime(0.0))
        .unwrap();
    w.end_attack_request(&mut req, AttackHeight::Medium, None, true, LocalTime(0.0));
    assert!((w.combat.requested_attack_power - 0.5).abs() < 1e-9);

    // The premise, read *before* the frame consumes the build: the clock really has overshot,
    // so `level` and `requested` are different numbers and `min` is not the identity here.
    // Asserted first because `execute_attack` clears `build_in_progress` and `power_bar_level`
    // then answers 0.0 — the same shape as every other "assert the premise" in this project.
    let level_at_the_frame = w.combat.power_bar_level(LocalTime(0.7));
    assert!(
        level_at_the_frame > 0.69 && level_at_the_frame > w.combat.requested_attack_power,
        "the frame lands past the arrival: level {level_at_the_frame}, requested 0.5"
    );

    // One long frame: the bar is at 0.700 when it is looked at, and the arrival was at 0.500.
    frame(&mut w, &mut req, 0.7);
    assert_eq!(
        attacks(&req),
        vec![0.5],
        "the swing is at the requested level"
    );
    assert!(
        (w.combat.latest_power_bar_level - 0.5).abs() < 1e-9,
        "and so is the bar: min(requested, level), not level"
    );
}

/// **The charge is not restarted by a click that arrives while one is already running**, and a
/// tap below the cap is still one swing. The complement of the test above: it asserts the
/// arrival, this asserts that nothing else can produce one.
#[test]
fn a_second_click_mid_charge_does_not_produce_a_second_swing() {
    let mut w = world();
    let mut req = RecordingRequests::default();
    w.set_requested_attack_height(AttackHeight::Medium, true, LocalTime(0.0))
        .unwrap();
    w.end_attack_request(&mut req, AttackHeight::Medium, None, true, LocalTime(0.0));
    frame(&mut w, &mut req, 0.2);
    let start = w.combat.build_start_time;

    // The same height again. `set_requested_attack_height`'s guard is
    // `old != new || !attack_request_in_progress`, so `start_attack_request` *does* run — and
    // attack-build startup returns immediately when a build is in progress, so the
    // clock is not reset. That distinction is the reason a fast clicker does not stall.
    w.set_requested_attack_height(AttackHeight::Medium, true, LocalTime(0.2))
        .unwrap();
    assert!(
        (w.combat.build_start_time - start).abs() < 1e-9,
        "the charge already running is not restarted"
    );
    w.end_attack_request(&mut req, AttackHeight::Medium, None, true, LocalTime(0.2));
    assert!(attacks(&req).is_empty());

    frame(&mut w, &mut req, 0.5);
    assert_eq!(attacks(&req), vec![0.5], "still exactly one swing");
}

/// **`AutoRepeatAttack`, both ways** — the checkbox the owner named, and the reason it ships
/// default-on.
///
/// With automatic repeat enabled, the server's *attack done* restarts
/// the bar (`start_power_bar_build`, `current_build_is_automatic = true`); with it off,
/// `repeat_attacking` is cleared and the bar is hidden. **Both arms are asserted, because a
/// repeat that never stops and one that never starts both pass a single-shot test.**
///
/// The automatic build's own arrival is asserted too: the frame update's
/// `current_build_is_automatic` arm **stops** the build instead of swinging, because the swing
/// is the server's while the sustained attack runs. Without that arm the client would swing
/// once per second for ever off one click.
#[test]
fn auto_repeat_attack_decides_whether_the_charge_restarts_by_itself() {
    for on in [true, false] {
        let mut w = world();
        w.player_system
            .options
            .set(crate::player::options::option::AUTO_REPEAT_ATTACK, on);
        let mut req = RecordingRequests::default();

        w.set_requested_attack_height(AttackHeight::Medium, true, LocalTime(0.0))
            .unwrap();
        w.end_attack_request(&mut req, AttackHeight::Medium, None, true, LocalTime(0.0));
        frame(&mut w, &mut req, 0.5);
        assert_eq!(
            attacks(&req),
            vec![0.5],
            "{on}: the first swing is the same either way"
        );
        assert_eq!(
            w.combat.repeat_attacking, on,
            "{on}: executing the attack sets the repeat flag only when the box is on"
        );

        // The server answers. `result == 0` is a clean finish.
        w.handle_attack_done(&mut req, 0, true, LocalTime(1.0));

        assert_eq!(
            w.combat.build_in_progress, on,
            "{on}: the charge restarts by itself only with the box on"
        );
        if on {
            assert!(
                w.combat.current_build_is_automatic,
                "and the restart is the automatic one"
            );
            assert_eq!(
                w.combat.power_bar_mode,
                PowerBarMode::Combat,
                "the bar stays up"
            );
            assert!(
                (w.combat.build_start_time - 1.0).abs() < 1e-9,
                "restarted at *now*"
            );

            // The automatic build arrives and **does not swing**: the sustained attack is the
            // server's until `Request::CancelAttack`.
            for t in [1.1, 1.3, 1.49] {
                frame(&mut w, &mut req, t);
                assert_eq!(
                    attacks(&req).len(),
                    1,
                    "automatic build at {t}: no client swing"
                );
            }
            frame(&mut w, &mut req, 1.5);
            assert_eq!(
                attacks(&req).len(),
                1,
                "arrival of an automatic build never swings"
            );
            assert!(!w.combat.build_in_progress, "it stops the build instead");
        } else {
            assert!(
                !w.combat.repeat_attacking,
                "off: the repeat flag is cleared"
            );
            assert_eq!(
                w.combat.power_bar_mode,
                PowerBarMode::Undef,
                "off: the bar is hidden"
            );
            // And no number of frames produces a second swing without another click.
            for t in [1.1, 1.5, 2.0, 5.0] {
                frame(&mut w, &mut req, t);
            }
            assert_eq!(attacks(&req).len(), 1, "off: you must click again");
        }
    }
}

/// The not-ready arm of the same function, asserted because it is the only producer of a
/// `CancelAttack` from the frame loop and it is gated on the **option** rather than on
/// `repeat_attacking` — an asymmetry that reads like a bug and is the client's.
#[test]
fn losing_the_ready_position_mid_charge_cancels_and_hides_the_bar() {
    let mut w = world();
    let mut req = RecordingRequests::default();
    w.set_requested_attack_height(AttackHeight::Medium, true, LocalTime(0.0))
        .unwrap();
    w.end_attack_request(&mut req, AttackHeight::Medium, None, true, LocalTime(0.0));
    assert!(w.combat.build_in_progress);
    assert!(!w.combat.repeat_attacking, "nothing has swung yet");

    w.combat_power_bar_use_time(&mut req, false, LocalTime(0.2));
    assert_eq!(
        cancels(&req),
        1,
        "the cancel is sent on the option, not on the repeat flag"
    );
    assert!(!w.combat.build_in_progress);
    assert_eq!(
        w.combat.power_bar_mode,
        PowerBarMode::Undef,
        "the bar is hidden, not a stall"
    );
    assert!(attacks(&req).is_empty(), "and nothing swung");
}

/// **The re-arm, and it took a surviving mutation to find the case that needs
/// it.**
///
/// `if (attack_request_in_progress && !build_in_progress && !attack_server_response_pending)
///  attempt_start_building_attack();` is the last thing the frame update does before the mode
/// retry.
/// Deleting it survived every test this unit had written, because none of them reached the one
/// state that can satisfy all three conditions at once: **the control is still held and the
/// build has been torn down under it.**
///
/// The producer of that state is the not-ready arm two tests up — its
/// `attack_request_in_progress` branch clears `build_in_progress` and zeroes the clock but
/// deliberately does **not** call `hide_power_bar`, so the bar stays up with nothing charging
/// it.
/// Without this re-arm a player who is interrupted mid-swing keeps holding the key over a dead
/// bar until they let go. Reading 1 of the four: the code is right, the test was weak.
#[test]
fn a_held_request_re_arms_its_build_once_the_ready_position_comes_back() {
    let mut w = world();
    let mut req = RecordingRequests::default();
    // Held, not clicked: no `end_attack_request`, so the request stays open.
    w.set_requested_attack_height(AttackHeight::Medium, true, LocalTime(0.0))
        .unwrap();
    assert!(w.combat.attack_request_in_progress && w.combat.build_in_progress);

    // Interrupted. The bar is torn down but the request survives, which is the asymmetry.
    w.combat_power_bar_use_time(&mut req, false, LocalTime(0.2));
    assert!(
        w.combat.attack_request_in_progress,
        "the control is still down"
    );
    assert!(!w.combat.build_in_progress, "and the build is gone");
    assert_eq!(
        w.combat.power_bar_mode,
        PowerBarMode::Combat,
        "the bar was NOT hidden"
    );
    assert!(
        !w.combat.attack_server_response_pending,
        "the third condition holds too"
    );

    // Ready again: the next frame re-arms it, at *now* rather than at the original press.
    frame(&mut w, &mut req, 0.6);
    assert!(
        w.combat.build_in_progress,
        "the held request re-arms its own build"
    );
    assert!(
        (w.combat.build_start_time - 0.6).abs() < 1e-9,
        "restarted at this frame"
    );
    assert!(attacks(&req).is_empty(), "and re-arming is not a swing");

    // And it charges from there, so the interruption cost the player the charge and not the
    // swing: releasing after a full second still swings.
    frame(&mut w, &mut req, 1.2);
    assert!(attacks(&req).is_empty(), "still held, still charging");
    w.end_attack_request(&mut req, AttackHeight::Medium, None, true, LocalTime(1.6));
    assert!(
        !attacks(&req).is_empty(),
        "the release swings off the re-armed build"
    );
}

/// The keyboard gauge steps one seventh at a time and saturates.
#[test]
fn the_keyboard_gauge_steps_one_seventh_at_a_time_and_saturates() {
    let mut c = CombatState::begin();
    // The seven notches, walked up from the bottom.
    c.ui_requested_power = 0.0;
    let up: Vec<f32> = (0..8).map(|_| c.adjust_ui_requested_power(true)).collect();
    let sixth = 1.0_f32 / 6.0;
    for (i, v) in up.iter().enumerate().take(6) {
        let want = sixth * (i + 1) as f32;
        assert!((v - want).abs() < 1e-6, "step {i}: {v} != {want}");
    }
    assert!(
        (up[6] - 1.0).abs() < 1e-9,
        "the sixth step is the top notch"
    );
    assert!(
        (up[7] - 1.0).abs() < 1e-9,
        "and stepping up at the top saturates"
    );

    // Down again, ending at 0 and staying there. Both directions, because one step expression
    // produces `+1` or `-1` and a sign error is invisible from one side.
    let down: Vec<f32> = (0..8).map(|_| c.adjust_ui_requested_power(false)).collect();
    assert!(
        (down[5] - 0.0).abs() < 1e-9,
        "six steps down from the top is the bottom notch"
    );
    assert!(
        (down[6] - 0.0).abs() < 1e-9,
        "and stepping down at the bottom saturates"
    );

    // **Round-half-up onto the nearest notch, which is what the `+0.5/6` before the truncation
    // is for.** From a value *between* notches the key does not step by a sixth from where it
    // was; it snaps to the neighbouring notch. This is the assertion that fails if the offset
    // is dropped, and it is only reachable because the scrollbar can put it here.
    c.ui_requested_power = 0.30;
    assert!(
        (c.adjust_ui_requested_power(true) - sixth * 3.0).abs() < 1e-6,
        "0.30 rounds to notch 2 and steps to notch 3"
    );
    c.ui_requested_power = 0.30;
    assert!(
        (c.adjust_ui_requested_power(false) - sixth).abs() < 1e-6,
        "and downwards to notch 1"
    );

    // The seven notches are the *whole* reachable set from this arm — the count as a count.
    let mut reachable = std::collections::BTreeSet::new();
    for start in 0..=100_u8 {
        let mut c = CombatState::begin();
        c.ui_requested_power = f32::from(start) / 100.0;
        reachable.insert(dereth_primitives::num::to_i32(
            c.adjust_ui_requested_power(true) * 1e6,
        ));
        let mut c = CombatState::begin();
        c.ui_requested_power = f32::from(start) / 100.0;
        reachable.insert(dereth_primitives::num::to_i32(
            c.adjust_ui_requested_power(false) * 1e6,
        ));
    }
    assert_eq!(
        reachable.len(),
        7,
        "seven notches and no others: {reachable:?}"
    );
}

/// **The scrollbar arm is continuous, and nobody has to choose between the two readings.**
///
/// The power scrollbar's element-message `0x0A` arm computes
/// `requested_power = clamp(p1 * 0.001, 0, 1)` from an unsigned position. Its store is
/// a plain 32-bit move with **no quantisation**. The owner thought a drag could land between
/// the notches and asked to be reminded to check; it can, and the client says so.
///
/// The load-bearing assertion is the last one: a drag reaches values the keyboard arm above
/// **cannot produce at all**. Asserting the two paths together would let a single wrong
/// implementation — quantising the drag — pass both.
#[test]
fn the_scrollbar_gauge_is_continuous_and_reaches_values_the_keyboard_cannot() {
    let mut c = CombatState::begin();
    assert!((c.set_ui_requested_power_from_scrollbar(0) - 0.0).abs() < 1e-9);
    assert!((c.set_ui_requested_power_from_scrollbar(1000) - 1.0).abs() < 1e-9);
    assert!(
        (c.set_ui_requested_power_from_scrollbar(1) - 0.001).abs() < 1e-7,
        "one thousandth"
    );
    assert!(
        (c.set_ui_requested_power_from_scrollbar(2500) - 1.0).abs() < 1e-9,
        "clamped above"
    );
    // The `jge`: `dwParam1` is read back as the `ulong` the message carries,
    // so a position with the top bit set is a huge positive and clamps, never a negative.
    assert!(
        (c.set_ui_requested_power_from_scrollbar(0x8000_0000) - 1.0).abs() < 1e-9,
        "the unsigned fixup, not a wrap to 0"
    );

    // How many of the 1,001 drag positions land *between* notches. **This is a count produced
    // by a threshold, so the threshold is stated and two brackets are given rather than one
    // number** — the first draft of this test asserted a single figure that
    // was wrong by six, and the six were entirely the epsilon.
    let notches: Vec<f32> = (0..=6_u8).map(|i| f32::from(i) / 6.0).collect();
    let mut between = |tol: f32| {
        (0..=1000_u32)
            .filter(|&pos| {
                let v = c.set_ui_requested_power_from_scrollbar(pos);
                notches.iter().all(|n| (v - n).abs() > tol)
            })
            .count()
    };
    let (wide, tight) = (between(1e-2), between(1e-3));
    assert!(
        (875..=885).contains(&wide),
        "at 0.010: {wide} of 1,001 between notches"
    );
    assert!(
        (983..=993).contains(&tight),
        "at 0.001: {tight} of 1,001 between notches"
    );
    assert!(
        tight > wide,
        "a tighter tolerance can only find more of them"
    );

    // And the two arms disagree on the same request, which is the point of filing them apart:
    // the drag sets 0.30 exactly, the key would have snapped it to a sixth.
    assert!((c.set_ui_requested_power_from_scrollbar(300) - 0.3).abs() < 1e-6);
    assert!(
        notches.iter().all(|n| (0.3 - n).abs() > 1e-3),
        "0.300 is not a notch, and the keyboard has no way to produce it"
    );
}
