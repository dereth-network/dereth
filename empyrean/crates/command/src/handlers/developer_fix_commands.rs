// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Command/Handlers/DeveloperFixCommands.cs
//! Port of `Source/ACE.Server/Command/Handlers/DeveloperFixCommands.cs`.
//!
//! The `verify-*` console commands check, and with `fix` repair, player and item data.
//!
//! - **Offline players.** `PlayerManager.GetAllOffline()` is the player manager's offline entries;
//!   a fix edits the entry's biota and `SaveBiotaToDatabase` hands a snapshot to `w.shard`.
//! - **The shard.** ACE's `ShardDbContext` and `WorldDbContext` queries read through empyrean-store and
//!   the world content instead (`w.shard.base_database()`, `w.content.get_all_weenies()`): every
//!   biota or character by id, then ACE's filters and joins in code
//!   (`empyrean_store::shard_database_offline_tools`). A `SaveChanges` is one batch transaction; ACE's
//!   raw SQL statements are still built as ACE builds them and their effect is applied through the
//!   store. DIVERGE (arch): the order of rows a MySQL join returns without `ORDER BY` is
//!   unspecified; here such rows come by object id.
//! - **Hashes.** `CalculateEmoteHash` uses .NET `GetHashCode` (vectored against .NET); DIVERGE
//!   (forced): `string.GetHashCode()` is randomized per .NET process, so the quest name's hash is a
//!   fixed FNV-1a here. Only equality within one run matters.
//! - `Environment.NewLine` is `"\n"` (ACE on Linux).

use std::collections::HashSet;
use std::sync::LazyLock;

use empyrean_common::dotnet::{format, CsCast, DotNetDict, DotNetHashSet};
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::{
    AccessLevel, CombatUse, EnchantmentTypeFlags, EquipMask, HeritageGroup, HookType, MaterialType,
    PositionType, PropertyAttribute, PropertyAttribute2nd, PropertyBool, PropertyInstanceId,
    PropertyInt, PropertyInt64, PropertyString, Skill, SkillAdvancementClass, WeenieType,
    WieldRequirement,
};
use empyrean_entity::{Biota as EntityBiota, ObjectGuid, Position};
use empyrean_net::SessionId;
use empyrean_store::models::shard::{
    Biota, BiotaPropertiesInt, BiotaPropertiesPosition, Character,
};
use empyrean_store::shard_database_offline_tools::{all_biotas, all_characters, save_changes};
use empyrean_store::ShardDatabase;
use empyrean_world::entity::spell::Spell;
use empyrean_world::entity::tinker_log::TinkerLog;
use empyrean_world::entity::verify_xp_result::VerifyXpResult;
use empyrean_world::factories::loot_tables::CANTRIP_SETS;
use empyrean_world::factories::world_object_factory;
use empyrean_world::managers::player_manager;
use empyrean_world::world_objects::{player_attributes, player_skills, player_vitals};
use empyrean_world::World;

use crate::command_handler_attribute::CommandHandlerAttribute;
use crate::command_handler_flag::CommandHandlerFlag;
use crate::command_handler_info::{CommandHandlerInfo, NamedHandler};
use crate::command_manager::console_write_line;
use crate::handler;
use crate::handlers::admin_commands::culture_compare;
use crate::handlers::command_handler_helper;

/// This file's `[CommandHandler]` decorations, in declaration order.
#[must_use]
pub fn command_handlers() -> Vec<CommandHandlerInfo> {
    let admin = AccessLevel::Admin;
    let console = CommandHandlerFlag::ConsoleInvoke;
    let d = |command: &str, description: &str| {
        CommandHandlerAttribute::with_description(command, admin, console, description, "")
    };
    let rows: [(CommandHandlerAttribute, NamedHandler); 15] = [
        (d("verify-player-data", "Verifies and optionally fixes any bugs with player data. Runs all of the verify* commands."), handler!(handle_verify_all)),
        (d("verify-attributes", "Verifies and optionally fixes any bugs with player attribute data"), handler!(handle_verify_attributes)),
        (d("verify-vitals", "Verifies and optionally fixes any bugs with player vitals data"), handler!(handle_verify_vitals)),
        (d("verify-skills", "Verifies and optionally fixes any bugs with player skill data"), handler!(handle_verify_skills)),
        (d("verify-skill-credits", "Verifies and optionally fixes any bugs with player skill credits"), handler!(handle_verify_skill_credits)),
        (d("verify-heritage-augs", "Verifies all players have their heritage augs."), handler!(handle_verify_heritage_augs)),
        (d("verify-max-augs", "Verifies and optionally fixes any bugs with the # of augs each player has"), handler!(handle_verify_max_augs)),
        (d("verify-xp", "Verifies and optionally fixes any bugs with player xp"), handler!(handle_verify_experience)),
        (
            CommandHandlerAttribute::with_count("fix-biota-emote-delay", admin, console, 0, "Fixes biota emotes with incorrect default delays", ""),
            handler!(handle_fix_biota_emote_delay),
        ),
        (d("verify-armor-levels", "Verifies and optionally fixes any existing armor levels above AL cap"), handler!(handle_fix_armor_level)),
        (d("verify-clothing-wield-level", "Verifies and optionally fixes any t7/t8 clothing that is missing a wield level requirement"), handler!(handle_verify_clothing_wield_level)),
        (
            d("verify-legendary-wield-level", "Verifies and optionally fixes any items with legendary cantrips that have less than 180 wield level requirement"),
            handler!(handle_verify_legendary_wield_level),
        ),
        (d("verify-shield-rating", "Verifies and optionally fixes any lootgen shields with incorrectly assigned CD/CDR"), handler!(handle_remove_shield_ratings)),
        (d("verify-melee-rares", "Verifies and optionally fixes any melee rares to EoR wcids"), handler!(handle_fix_melee_rares)),
        (
            d("verify-beneficial-enchantments", "Verifies enchantment registry has correct StatModType for Beneficial spells and optionally fixes"),
            handler!(handle_enchantments),
        ),
    ];
    rows.into_iter()
        .map(|(attribute, (handler, handler_name))| CommandHandlerInfo {
            handler,
            handler_name,
            attribute,
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Offline players
// ---------------------------------------------------------------------------------------------

/// `fix = parameters.Length > 0 && parameters[0].Equals("fix")` and its `fixStr`.
fn fix_of(parameters: &[String]) -> (bool, &'static str) {
    let fix = parameters.first().is_some_and(|p| p == "fix");
    (fix, if fix { " -- fixed" } else { "" })
}

/// `player.Name` (null prints as nothing).
fn name(w: &World, player: ObjectGuid) -> String {
    offline(w, player).name().unwrap_or_default()
}

fn offline(
    w: &World,
    player: ObjectGuid,
) -> &empyrean_world::entity::offline_player::OfflinePlayer {
    player_manager::get_offline_player(w, player.full())
        .expect("NullReferenceException: OfflinePlayer")
}

fn offline_mut(
    w: &mut World,
    player: ObjectGuid,
) -> &mut empyrean_world::entity::offline_player::OfflinePlayer {
    w.player_manager
        .offline_players
        .get_mut(&player.full())
        .expect("NullReferenceException: OfflinePlayer")
}

fn biota_mut(w: &mut World, player: ObjectGuid) -> &mut EntityBiota {
    &mut offline_mut(w, player).biota
}

/// `player.SaveBiotaToDatabase()`: the snapshot goes to `DatabaseManager.Shard.SaveBiota`.
fn save_biota_to_database(w: &mut World, player: ObjectGuid) {
    let now = w.now.utc;
    if let Some(biota) = offline_mut(w, player).save_biota_to_database(now, true) {
        w.shard.save_biota(biota, None);
    }
}

/// `player.Account.AccessLevel`.
///
/// # Panics
/// A player without an account (ACE's `NullReferenceException`).
fn account_access_level(w: &World, player: ObjectGuid) -> u32 {
    offline(w, player)
        .account
        .as_ref()
        .expect("NullReferenceException: player.Account")
        .access_level
}

/// `list[list.Count - 1]`.
///
/// # Panics
/// An empty list (ACE's `ArgumentOutOfRangeException`).
fn last<T: Copy>(list: &[T]) -> T {
    *list.last().expect("ArgumentOutOfRangeException: index -1")
}

// ACE: DeveloperFixCommands.HandleVerifyAll
/// `verify-player-data (fix)`: runs every `verify-*` command for player data.
pub fn handle_verify_all(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    handle_verify_attributes(w, session, parameters);
    handle_verify_vitals(w, session, parameters);

    handle_verify_skills(w, session, parameters);

    handle_verify_skill_credits(w, session, parameters);

    handle_verify_heritage_augs(w, session, parameters);
    handle_verify_max_augs(w, session, parameters);

    handle_verify_experience(w, session, parameters);
}

// ACE: DeveloperFixCommands.HandleVerifyAttributes
/// `verify-attributes (fix)`: attribute ranks, XP caps, and innate levels augmented above 100.
pub fn handle_verify_attributes(w: &mut World, _session: Option<SessionId>, parameters: &[String]) {
    let players = player_manager::get_all_offline(w);

    let (fix, fix_str) = fix_of(parameters);
    let mut found_issues = false;
    for &player in &players {
        let mut updated = false;
        // Not ACE's (a fix): set per player, so only a player whose
        // points were redistributed gets FreeAttributeResetRenewed; ACE never reset it between
        // players, so every later player fixed for another attribute issue got it too.
        let mut reset_free_attribute_redistribution_timer = false;
        let player_name = name(w, player);

        let keys: Vec<PropertyAttribute> = offline(w, player)
            .biota
            .properties_attribute
            .as_ref()
            .expect("ArgumentNullException: PropertiesAttribute")
            .keys()
            .copied()
            .collect();

        for key in keys {
            let value = |w: &World| {
                offline(w, player)
                    .biota
                    .properties_attribute
                    .as_ref()
                    .and_then(|d| d.get(&key))
                    .cloned()
                    .expect("attribute")
            };

            // ensure this is a valid attribute
            if key < PropertyAttribute::Strength || key > PropertyAttribute::Self_ {
                console_write_line(&format!(
                    "{player_name} has unknown attribute {}{fix_str}",
                    key.to_dotnet_string()
                ));
                found_issues = true;

                if fix {
                    // i have found no instances of this situation being run into,
                    // but if it does happen, verify-xp will refund the player xp properly

                    if let Some(d) = biota_mut(w, player).properties_attribute.as_mut() {
                        d.remove(&key);
                    }
                    updated = true;
                }
                continue;
            }

            let rank = value(w).level_from_cp;

            // verify attribute rank
            let correct_rank = player_attributes::calc_attribute_rank(w, value(w).cp_spent);
            if i64::from(rank) != i64::from(correct_rank) {
                console_write_line(&format!(
                    "{player_name}'s {} rank is {rank}, should be {correct_rank}{fix_str}",
                    key.to_dotnet_string()
                ));
                found_issues = true;

                if fix {
                    let level: u16 = correct_rank.cs_cast();
                    if let Some(a) = biota_mut(w, player)
                        .properties_attribute
                        .as_mut()
                        .and_then(|d| d.get_mut(&key))
                    {
                        a.level_from_cp = u32::from(level);
                    }
                    updated = true;
                }
            }

            // verify attribute xp is within bounds
            let max_attribute_xp = last(&w.dats.portal_dat().xp_table().attribute_xp);

            if value(w).cp_spent > max_attribute_xp {
                console_write_line(&format!(
                    "{player_name}'s {} attribute total xp is {}, should be capped at {}{fix_str}",
                    key.to_dotnet_string(),
                    format(value(w).cp_spent, "N0"),
                    format(max_attribute_xp, "N0")
                ));
                found_issues = true;

                if fix {
                    // again i have found no instances of this situation being run into,
                    // but if it does happen, verify-xp will refund the player xp properly

                    if let Some(a) = biota_mut(w, player)
                        .properties_attribute
                        .as_mut()
                        .and_then(|d| d.get_mut(&key))
                    {
                        a.cp_spent = max_attribute_xp;
                    }
                    updated = true;
                }
            }

            // Verify that an attribute has not been augmented above 100
            // only do this if server operators have opted into this functionality
            let init_level = value(w).init_level;
            if init_level > 100
                && init_level <= 104
                && account_access_level(w, player) == AccessLevel::Player.0.cast_unsigned()
                && player_manager::property_manager_get_bool(w, "attribute_augmentation_safety_cap")
            {
                let mut augmentation_exploit_message_builder = String::new();
                found_issues = true;
                augmentation_exploit_message_builder += &format!(
                    "{player_name}'s {} is currently {init_level}, augmented above 100.\n",
                    key.to_dotnet_string()
                );

                // only search strength, endurance, coordination, quicknesss, focus, and self
                let valid_attributes: Vec<(PropertyAttribute, u32)> = offline(w, player)
                    .biota
                    .properties_attribute
                    .as_ref()
                    .map(|d| {
                        d.iter()
                            .filter(|(k, _)| {
                                **k >= PropertyAttribute::Strength
                                    && **k <= PropertyAttribute::Self_
                            })
                            .map(|(k, v)| (*k, v.init_level))
                            .collect()
                    })
                    .unwrap_or_default();
                // find the lowest value of an attribute to distribute points to
                let lowest_init_attribute_level = valid_attributes
                    .iter()
                    .map(|(_, l)| *l)
                    .min()
                    .expect("InvalidOperationException: Sequence contains no elements");

                // find the lowest attribute to distribute the extra points to so they're not lost
                let target_attribute = valid_attributes
                    .iter()
                    .find(|(_, l)| *l == lowest_init_attribute_level)
                    .map(|(k, _)| *k);
                augmentation_exploit_message_builder += "5 points will be redistributed to lowest eligible innate attribute to fix this issue.\n";

                console_write_line(&augmentation_exploit_message_builder);
                if lowest_init_attribute_level < 96 && fix {
                    let d = biota_mut(w, player)
                        .properties_attribute
                        .as_mut()
                        .expect("attributes");
                    if let Some(a) = d.get_mut(&key) {
                        a.init_level = a.init_level.wrapping_sub(5);
                    }
                    let target = target_attribute.expect("NullReferenceException: targetAttribute");
                    if let Some(t) = d.get_mut(&target) {
                        t.init_level = t.init_level.wrapping_add(5);
                    }
                    updated = true;
                    reset_free_attribute_redistribution_timer = true;
                }
            }
        }
        if fix && updated {
            // if we've redistributed augmented attribute points, give people the opportunity
            // to redistribute them legitimately as they please
            if reset_free_attribute_redistribution_timer {
                offline_mut(w, player).set_property(PropertyBool::FreeAttributeResetRenewed, true);
            }
            save_biota_to_database(w, player);
        }
    }

    if !fix && found_issues {
        console_write_line("Dry run completed. Type 'verify-attributes fix' to fix any issues.");
    }

    if !found_issues {
        console_write_line(&format!(
            "Verified attributes for {} players",
            format(players.len(), "N0")
        ));
    }
}

// ACE: DeveloperFixCommands.HandleVerifyVitals
/// `verify-vitals (fix)`: vital ranks and XP caps.
pub fn handle_verify_vitals(w: &mut World, _session: Option<SessionId>, parameters: &[String]) {
    let players = player_manager::get_all_offline(w);

    let (fix, fix_str) = fix_of(parameters);
    let mut found_issues = false;

    for &player in &players {
        let mut updated = false;
        let player_name = name(w, player);

        let keys: Vec<PropertyAttribute2nd> = offline(w, player)
            .biota
            .properties_attribute_2nd
            .as_ref()
            .expect("ArgumentNullException: PropertiesAttribute2nd")
            .keys()
            .copied()
            .collect();

        for key in keys {
            let value = |w: &World| {
                offline(w, player)
                    .biota
                    .properties_attribute_2nd
                    .as_ref()
                    .and_then(|d| d.get(&key))
                    .cloned()
                    .expect("vital")
            };

            // ensure this is a valid MaxVital
            if key != PropertyAttribute2nd::MaxHealth
                && key != PropertyAttribute2nd::MaxStamina
                && key != PropertyAttribute2nd::MaxMana
            {
                console_write_line(&format!(
                    "{player_name} has unknown vital {}{fix_str}",
                    key.to_dotnet_string()
                ));
                found_issues = true;

                if fix {
                    // i have found no instances of this situation being run into,
                    // but if it does happen, verify-xp will refund the player xp properly

                    if let Some(d) = biota_mut(w, player).properties_attribute_2nd.as_mut() {
                        d.remove(&key);
                    }
                    updated = true;
                }
                continue;
            }

            let rank = value(w).level_from_cp;

            // verify vital rank
            let correct_rank = player_vitals::calc_vital_rank(w, value(w).cp_spent);
            if i64::from(rank) != i64::from(correct_rank) {
                console_write_line(&format!(
                    "{player_name}'s {} rank is {rank}, should be {correct_rank}{fix_str}",
                    key.to_dotnet_string()
                ));
                found_issues = true;

                if fix {
                    let level: u16 = correct_rank.cs_cast();
                    if let Some(v) = biota_mut(w, player)
                        .properties_attribute_2nd
                        .as_mut()
                        .and_then(|d| d.get_mut(&key))
                    {
                        v.level_from_cp = u32::from(level);
                    }
                    updated = true;
                }
            }

            // verify vital xp is within bounds
            let max_vital_xp = last(&w.dats.portal_dat().xp_table().vital_xp);

            if value(w).cp_spent > max_vital_xp {
                console_write_line(&format!(
                    "{player_name}'s {} vital total xp is {}, should be capped at {}{fix_str}",
                    key.to_dotnet_string(),
                    format(value(w).cp_spent, "N0"),
                    format(max_vital_xp, "N0")
                ));
                found_issues = true;

                if fix {
                    // again i have found no instances of this situation being run into,
                    // but if it does happen, verify-xp will refund the player xp properly

                    if let Some(v) = biota_mut(w, player)
                        .properties_attribute_2nd
                        .as_mut()
                        .and_then(|d| d.get_mut(&key))
                    {
                        v.cp_spent = max_vital_xp;
                    }
                    updated = true;
                }
            }
        }

        if updated {
            save_biota_to_database(w, player);
        }
    }

    if !fix && found_issues {
        console_write_line("Dry run completed. Type 'verify-vitals fix' to fix any issues.");
    }

    if !found_issues {
        console_write_line(&format!(
            "Verified vitals for {} players",
            format(players.len(), "N0")
        ));
    }
}

// ACE: DeveloperFixCommands.HandleVerifySkills
/// `verify-skills (fix)`: unknown skills, XP in untrained skills, init levels, ranks and XP caps.
pub fn handle_verify_skills(w: &mut World, _session: Option<SessionId>, parameters: &[String]) {
    let players = player_manager::get_all_offline(w);

    let (fix, fix_str) = fix_of(parameters);
    let mut found_issues = false;

    for &player in &players {
        let mut updated = false;
        let player_name = name(w, player);

        let keys: Vec<Skill> = offline(w, player)
            .biota
            .properties_skill
            .as_ref()
            .expect("ArgumentNullException: PropertiesSkill")
            .keys()
            .copied()
            .collect();

        for key in keys {
            let value = |w: &World| {
                offline(w, player)
                    .biota
                    .properties_skill
                    .as_ref()
                    .and_then(|d| d.get(&key))
                    .cloned()
                    .expect("skill")
            };
            let skill_name = key.to_dotnet_string();

            // ensure this is a valid player skill
            if !player_skills::PLAYER_SKILLS.contains(&key) {
                console_write_line(&format!(
                    "{player_name} has unknown skill {skill_name}{fix_str}"
                ));
                found_issues = true;
                if fix {
                    // i have found no instances of these skills ever having xp put into them,
                    // but if there were, verify-xp will fix that
                    if let Some(d) = biota_mut(w, player).properties_skill.as_mut() {
                        d.remove(&key);
                    }
                    updated = true;
                }
                continue;
            }

            let rank = value(w).level_from_pp;

            let sac = value(w).sac;
            if sac < SkillAdvancementClass::Trained {
                let v = value(w);
                if v.pp > 0 || v.level_from_pp > 0 {
                    console_write_line(&format!(
                        "{player_name} has {} skill {skill_name} with {} xp (rank {}){fix_str}",
                        sac.to_dotnet_string(),
                        format(v.pp, "N0"),
                        v.level_from_pp
                    ));
                    found_issues = true;

                    if fix {
                        // i have found no instances of this situation being run into,
                        // but if it does happen, verify-xp will refund the player xp properly
                        if let Some(s) = biota_mut(w, player)
                            .properties_skill
                            .as_mut()
                            .and_then(|d| d.get_mut(&key))
                        {
                            s.pp = 0;
                            s.level_from_pp = 0;
                        }

                        updated = true;
                    }
                }
                continue;
            }

            let target_init_level = if sac == SkillAdvancementClass::Specialized {
                10
            } else {
                0
            };
            let init_level = value(w).init_level;
            let wrong = if sac == SkillAdvancementClass::Specialized {
                init_level != 10
            } else {
                init_level > 0
            };
            if wrong {
                console_write_line(&format!(
                    "{player_name} has {} skill {skill_name} with {} InitLevel{fix_str}",
                    sac.to_dotnet_string(),
                    format(init_level, "N0")
                ));
                found_issues = true;

                if fix {
                    if let Some(s) = biota_mut(w, player)
                        .properties_skill
                        .as_mut()
                        .and_then(|d| d.get_mut(&key))
                    {
                        s.init_level = target_init_level;
                    }

                    updated = true;
                }
            }

            // verify skill rank
            let correct_rank = player_skills::calc_skill_rank(w, sac, value(w).pp);
            if i32::from(rank) != correct_rank {
                console_write_line(&format!("{player_name}'s {skill_name} rank is {rank}, should be {correct_rank}{fix_str}"));
                found_issues = true;

                if fix {
                    if let Some(s) = biota_mut(w, player)
                        .properties_skill
                        .as_mut()
                        .and_then(|d| d.get_mut(&key))
                    {
                        s.level_from_pp = correct_rank.cs_cast();
                    }
                    updated = true;
                }
            }

            // verify skill xp is within bounds

            // in retail, if a player had a trained skill maxed out, and then they speced it in spec temple,
            // they would sort of temporarily 'lose' that ~103m xp, unless they reset the trained skill, and then speced it

            // so the data can be in a legit situation here where a character has a skill speced,
            // but their xp is beyond the spec xp cap (4,100,490,438) and <= the trained xp cap (4,203,819,496)

            //var skillXPTable = Player.GetSkillXPTable(sac);
            let skill_xp_table =
                player_skills::get_skill_xp_table(w, SkillAdvancementClass::Trained)
                    .expect("NullReferenceException: GetSkillXPTable");
            let max_skill_xp = last(skill_xp_table);

            let pp = value(w).pp;
            if pp > max_skill_xp {
                console_write_line(&format!(
                    "{player_name}'s {} {skill_name} skill total xp is {}, should be capped at {}{fix_str}",
                    sac.to_dotnet_string(),
                    format(pp, "N0"),
                    format(max_skill_xp, "N0")
                ));
                found_issues = true;
                if fix {
                    // again i have found no instances of this situation being run into,
                    // but if it does happen, verify-xp will refund the player xp properly
                    if let Some(s) = biota_mut(w, player)
                        .properties_skill
                        .as_mut()
                        .and_then(|d| d.get_mut(&key))
                    {
                        s.pp = max_skill_xp;
                    }
                    updated = true;
                }
            }
        }

        if fix && updated {
            save_biota_to_database(w, player);
        }
    }

    if !fix && found_issues {
        console_write_line("Dry run completed. Type 'verify-skills fix' to fix any issues.");
    }

    if !found_issues {
        console_write_line(&format!(
            "Verified skills for {} players",
            format(players.len(), "N0")
        ));
    }
}

/// `ctx.CharacterPropertiesQuestRegistry.Where(i => i.QuestName.Equals(name))` (MySQL's
/// `quest_Name` collation compares): `(character, NumTimesCompleted)` in character order.
fn quest_registry_rows(characters: &[Character], quest_name: &str) -> Vec<(u32, i32)> {
    characters
        .iter()
        .flat_map(|c| c.character_properties_quest_registry.iter())
        .filter(|q| empyrean_store::collation::eq(&q.quest_name, quest_name))
        .map(|q| (q.character_id, q.num_times_completed))
        .collect()
}

// ACE: DeveloperFixCommands.HandleVerifySkillCredits
/// `verify-skill-credits (fix)`: available and total skill credits against heritage, level and
/// quest credits, with partial resets when a player has spent too many.
pub fn handle_verify_skill_credits(
    w: &mut World,
    _session: Option<SessionId>,
    parameters: &[String],
) {
    let players = player_manager::get_all_offline(w);

    let (fix, fix_str) = fix_of(parameters);
    let mut found_issues = false;

    let (oswald_skill_credit, ralirea_skill_credit, lum_aug_skill_credits) = {
        // 4 possible skill credits from quests
        // - OswaldManualCompleted
        // - ArantahKill1 (no 'turned in' stamp, only if given figurine?)
        // - LumAugSkillQuest (stamped either 1 or 2 times)
        let characters = all_characters(&mut **w.shard.base_database());

        let oswald: HashSet<u32> = quest_registry_rows(&characters, "OswaldManualCompleted")
            .into_iter()
            .map(|(c, _)| c)
            .collect();
        let ralirea: HashSet<u32> = quest_registry_rows(&characters, "ArantahKill1")
            .into_iter()
            .map(|(c, _)| c)
            .collect();
        let mut lum: DotNetDict<u32, i32> = DotNetDict::new();
        for (character_id, num_times_completed) in
            quest_registry_rows(&characters, "LumAugSkillQuest")
        {
            lum.add(character_id, num_times_completed);
        }
        (oswald, ralirea, lum)
    };

    for &player in &players {
        // skip admins
        match offline(w, player).account.as_ref() {
            None => continue,
            Some(a) if a.access_level == AccessLevel::Admin.0.cast_unsigned() => continue,
            Some(_) => {}
        }

        let player_name = name(w, player);

        let Some(heritage) = offline(w, player).heritage() else {
            console_write_line(&format!(
                "{player_name} (0x{player}) does not have a Heritage, skipping!"
            ));
            continue;
        };

        let heritage: u32 = heritage.cs_cast();
        let heritage_group = w
            .dats
            .portal_dat()
            .char_gen()
            .heritage_groups
            .get(&heritage)
            .cloned()
            .unwrap_or_else(|| panic!("KeyNotFoundException: {heritage}"));
        let mut adjusted_skill_costs: DotNetDict<Skill, (i32, i32)> = DotNetDict::new();
        for &(skill_num, normal_cost, primary_cost) in &heritage_group.skills {
            adjusted_skill_costs.add(Skill(skill_num.cs_cast()), (normal_cost, primary_cost));
        }

        let start_credits: i32 = heritage_group.skill_credits.cs_cast();

        let level_credits = get_additional_credits(offline(w, player).level().unwrap_or(1));

        let mut quest_credits = 0;

        // 4 possible skill credits from quests

        // - OswaldManualCompleted
        if oswald_skill_credit.contains(&player.full()) {
            quest_credits += 1;
        }

        // - ArantahKill1 (no 'turned in' stamp, only if given figurine?)
        if ralirea_skill_credit.contains(&player.full()) {
            quest_credits += 1;
        }

        // - LumAugSkillQuest (stamped either 1 or 2 times)
        if let Some(lum_skill_credits) = lum_aug_skill_credits.get(&player.full()) {
            quest_credits += lum_skill_credits;
        }

        let total_credits = start_credits + level_credits + quest_credits;

        //Console.WriteLine($"{player.Name} (0x{player.Guid}) Heritage: {heritage}, Level: {player.Level}, Base Credits: {startCredits}, Additional Level Credits: {levelCredits}, Quest Credits: {questCredits}, Total Skill Credits: {totalCredits}");

        let mut used = 0;

        let mut spec_credits_spent = 0;

        let skills: Vec<(Skill, SkillAdvancementClass)> = offline(w, player)
            .biota
            .properties_skill
            .as_ref()
            .expect("ArgumentNullException: PropertiesSkill")
            .iter()
            .map(|(k, v)| (*k, v.sac))
            .collect();
        for (skill, sac) in skills {
            if sac < SkillAdvancementClass::Trained {
                continue;
            }

            let key: u32 = skill.0.cs_cast();
            let Some(skill_info) = w.dats.portal_dat().skill_table().skills.get(&key).cloned()
            else {
                console_write_line(&format!(
                    "{player_name}:0x{player}.HandleVerifySkillCredits({}): unknown skill",
                    skill.to_dotnet_string()
                ));
                continue;
            };

            let adjusted_cost = adjusted_skill_costs.get(&skill).copied();

            let trained_cost = adjusted_cost.map_or(skill_info.trained_cost, |c| c.0);
            let specialized_cost = adjusted_cost.map_or(skill_info.specialized_cost, |c| c.1);

            //Console.WriteLine($"{(Skill)skill.Type} trained cost: {skillInfo.TrainedCost}, spec cost: {skillInfo.SpecializedCost}, adjusted trained cost: {trainedCost}, adjusted spec cost: {specializedCost}");

            used += trained_cost;

            if sac == SkillAdvancementClass::Specialized {
                // these can only be speced through augs, they have >= 999 in the spec data
                if matches!(
                    skill,
                    Skill::ArmorTinkering
                        | Skill::ItemTinkering
                        | Skill::MagicItemTinkering
                        | Skill::WeaponTinkering
                        | Skill::Salvaging
                ) {
                    continue;
                }

                used += specialized_cost - trained_cost;

                spec_credits_spent += specialized_cost;
            }
        }

        let target_credits = total_credits - used;
        let target_msg = format!(
            "{player_name} (0x{player}) should have {target_credits} available skill credits"
        );

        if target_credits < 0 {
            // if the player has already spent more skill credits than they should have,
            // unfortunately this situation requires a partial reset..

            console_write_line(&format!("{target_msg}. To fix this situation, trained skill reset will need to be applied{fix_str}"));
            found_issues = true;

            if fix {
                untrain_skills(w, player, target_credits);
            }

            continue;
        }

        if spec_credits_spent > 70 {
            // if the player has already spent more skill credits than they should have,
            // unfortunately this situation requires a partial reset..

            console_write_line(&format!(
                "{player_name} (0x{player}) has spent {spec_credits_spent} skill credits on specialization, {} over the limit of 70. To fix this situation, specialized skill reset will need to be applied{fix_str}",
                spec_credits_spent - 70
            ));
            found_issues = true;

            if fix {
                unspecialize_skills(w, player);
            }

            continue;
        }

        let available_credits = offline(w, player)
            .get_property(PropertyInt::AvailableSkillCredits)
            .unwrap_or(0);

        if available_credits != target_credits {
            console_write_line(&format!(
                "{target_msg}, but they have {available_credits}{fix_str}"
            ));
            found_issues = true;

            if fix {
                offline_mut(w, player)
                    .set_property(PropertyInt::AvailableSkillCredits, target_credits);
                save_biota_to_database(w, player);
            }
        }

        let total_skill_credits = offline(w, player)
            .get_property(PropertyInt::TotalSkillCredits)
            .unwrap_or(0);

        if total_skill_credits != total_credits {
            console_write_line(&format!("{player_name} (0x{player}) should have {total_credits} total skill credits, but they have {total_skill_credits}{fix_str}"));
            found_issues = true;

            if fix {
                offline_mut(w, player).set_property(PropertyInt::TotalSkillCredits, total_credits);
                save_biota_to_database(w, player);
            }
        }
    }

    if !fix && found_issues {
        console_write_line("Dry run completed. Type 'verify-skill-credits fix' to fix any issues.");
    }

    if !found_issues {
        console_write_line(&format!(
            "Verified skill credits for {} players",
            format(players.len(), "N0")
        ));
    }
}

// ACE: DeveloperFixCommands.GetAdditionalCredits
/// The skill credits earned by reaching `level` (the last `AdditionalCredits` entry at or below it).
#[must_use]
pub fn get_additional_credits(level: i32) -> i32 {
    for &(key, value) in ADDITIONAL_CREDITS.iter().rev() {
        if level >= key {
            return value;
        }
    }

    0
}

/// `AdditionalCredits`: level => total additional credits (a static `SortedDictionary`).
pub const ADDITIONAL_CREDITS: [(i32, i32); 46] = [
    (2, 1),
    (3, 2),
    (4, 3),
    (5, 4),
    (6, 5),
    (7, 6),
    (8, 7),
    (9, 8),
    (10, 9),
    (12, 10),
    (14, 11),
    (16, 12),
    (18, 13),
    (20, 14),
    (23, 15),
    (26, 16),
    (29, 17),
    (32, 18),
    (35, 19),
    (40, 20),
    (45, 21),
    (50, 22),
    (55, 23),
    (60, 24),
    (65, 25),
    (70, 26),
    (75, 27),
    (80, 28),
    (85, 29),
    (90, 30),
    (95, 31),
    (100, 32),
    (105, 33),
    (110, 34),
    (115, 35),
    (120, 36),
    (125, 37),
    (130, 38),
    (140, 39),
    (150, 40),
    (160, 41),
    (180, 42),
    (200, 43),
    (225, 44),
    (250, 45),
    (275, 46),
];

// ACE: DeveloperFixCommands.UntrainSkills
/// This method is only required in the rare situation when the amount of available skill credits
/// a player should have is negative.
fn untrain_skills(w: &mut World, player: ObjectGuid, mut target_credits: i32) {
    let mut refund_xp: i64 = 0;
    let player_name = name(w, player);

    let skills: Vec<Skill> = offline(w, player)
        .biota
        .properties_skill
        .as_ref()
        .map(|d| d.keys().copied().collect())
        .unwrap_or_default();
    for skill in skills {
        let key: u32 = skill.0.cs_cast();
        let Some(skill_base) = w.dats.portal_dat().skill_table().skills.get(&key).cloned() else {
            console_write_line(&format!(
                "{player_name}.UntrainSkills({}) - unknown skill",
                skill.to_dotnet_string()
            ));
            continue;
        };

        let Some(value) = biota_mut(w, player)
            .properties_skill
            .as_mut()
            .and_then(|d| d.get_mut(&skill))
        else {
            continue;
        };
        let sac = value.sac;

        if sac != SkillAdvancementClass::Trained || !player_skills::is_skill_untrainable(skill) {
            continue;
        }

        refund_xp += i64::from(value.pp);

        value.sac = SkillAdvancementClass::Untrained;
        value.init_level = 0;
        value.pp = 0;
        value.level_from_pp = 0;

        target_credits += skill_base.trained_cost;
    }

    let p = offline_mut(w, player);
    let available_experience = p
        .get_property(PropertyInt64::AvailableExperience)
        .unwrap_or(0);

    p.set_property(
        PropertyInt64::AvailableExperience,
        available_experience + refund_xp,
    );

    p.set_property(PropertyInt::AvailableSkillCredits, target_credits);

    p.set_property(PropertyBool::UntrainedSkills, true);

    p.set_property(PropertyBool::FreeSkillResetRenewed, true);
    p.set_property(PropertyBool::SkillTemplesTimerReset, true);

    save_biota_to_database(w, player);
}

// ACE: DeveloperFixCommands.UnspecializeSkills
/// This method is only required if the player is found to be over the spec skill limit of 70
/// credits.
fn unspecialize_skills(w: &mut World, player: ObjectGuid) {
    let mut refund_xp: i64 = 0;

    let mut refunded_credits = 0;
    let player_name = name(w, player);

    let skills: Vec<Skill> = offline(w, player)
        .biota
        .properties_skill
        .as_ref()
        .map(|d| d.keys().copied().collect())
        .unwrap_or_default();
    for skill in skills {
        let key: u32 = skill.0.cs_cast();
        let Some(skill_base) = w.dats.portal_dat().skill_table().skills.get(&key).cloned() else {
            console_write_line(&format!(
                "{player_name}.UntrainSkills({}) - unknown skill",
                skill.to_dotnet_string()
            ));
            continue;
        };

        let Some(value) = biota_mut(w, player)
            .properties_skill
            .as_mut()
            .and_then(|d| d.get_mut(&skill))
        else {
            continue;
        };
        let sac = value.sac;

        if sac != SkillAdvancementClass::Specialized
            || player_skills::AUG_SPEC_SKILLS.contains(&skill)
        {
            continue;
        }

        refund_xp += i64::from(value.pp);

        value.sac = SkillAdvancementClass::Trained;
        value.init_level = 0;
        value.pp = 0;
        value.level_from_pp = 0;

        // SkillBase.UpgradeCostFromTrainedToSpecialized
        refunded_credits += skill_base.specialized_cost - skill_base.trained_cost;
    }

    let p = offline_mut(w, player);
    let available_experience = p
        .get_property(PropertyInt64::AvailableExperience)
        .unwrap_or(0);

    p.set_property(
        PropertyInt64::AvailableExperience,
        available_experience + refund_xp,
    );

    let available_skill_credits = p
        .get_property(PropertyInt::AvailableSkillCredits)
        .unwrap_or(0);

    p.set_property(
        PropertyInt::AvailableSkillCredits,
        available_skill_credits + refunded_credits,
    );

    p.set_property(PropertyBool::UnspecializedSkills, true);

    p.set_property(PropertyBool::FreeSkillResetRenewed, true);
    p.set_property(PropertyBool::SkillTemplesTimerReset, true);

    save_biota_to_database(w, player);
}

// ACE: DeveloperFixCommands.HandleVerifyHeritageAugs
/// `verify-heritage-augs (fix)`: every player has their heritage's free augmentation.
pub fn handle_verify_heritage_augs(
    w: &mut World,
    _session: Option<SessionId>,
    parameters: &[String],
) {
    let players = player_manager::get_all_offline(w);

    let (fix, fix_str) = fix_of(parameters);
    let mut found_issues = false;

    for &player in &players {
        let player_name = name(w, player);
        let Some(heritage) = offline(w, player)
            .get_property(PropertyInt::HeritageGroup)
            .map(HeritageGroup)
        else {
            console_write_line(&format!("Couldn't find heritage for {player_name}"));
            continue;
        };

        let Some(heritage_aug) = get_heritage_aug(heritage) else {
            console_write_line(&format!(
                "Couldn't find heritage aug for {} player {player_name}",
                heritage.to_dotnet_string()
            ));
            continue;
        };

        let num_augs = offline(w, player).get_property(heritage_aug).unwrap_or(0);
        if num_augs < 1 {
            console_write_line(&format!(
                "{}={num_augs} for {} player {player_name}{fix_str}",
                heritage_aug.to_dotnet_string(),
                heritage.to_dotnet_string()
            ));
            found_issues = true;

            if fix {
                offline_mut(w, player).set_property(heritage_aug, 1);
                save_biota_to_database(w, player);
            }
        }
    }
    if !fix && found_issues {
        console_write_line("Dry run completed. Type 'verify-heritage-augs fix' to fix any issues.");
    }

    if !found_issues {
        console_write_line(&format!(
            "Verified heritage augs for {} players",
            format(players.len(), "N0")
        ));
    }
}

// ACE: DeveloperFixCommands.GetHeritageAug
/// The augmentation each heritage starts with.
#[must_use]
pub fn get_heritage_aug(heritage: HeritageGroup) -> Option<PropertyInt> {
    match heritage {
        HeritageGroup::Aluvian
        | HeritageGroup::Gharundim
        | HeritageGroup::Sho
        | HeritageGroup::Viamontian => Some(PropertyInt::AugmentationJackOfAllTrades),

        HeritageGroup::Shadowbound | HeritageGroup::Penumbraen => {
            Some(PropertyInt::AugmentationCriticalExpertise)
        }

        HeritageGroup::Gearknight => Some(PropertyInt::AugmentationDamageReduction),

        HeritageGroup::Undead => Some(PropertyInt::AugmentationCriticalDefense),

        HeritageGroup::Empyrean => Some(PropertyInt::AugmentationInfusedLifeMagic),

        HeritageGroup::Tumerok => Some(PropertyInt::AugmentationCriticalPower),

        HeritageGroup::Lugian => Some(PropertyInt::AugmentationIncreasedCarryingCapacity),

        _ => None,
    }
}

// ACE: DeveloperFixCommands.HandleVerifyMaxAugs
/// `verify-max-augs (fix)`: each augmentation count within `0..=MaxAugs`.
pub fn handle_verify_max_augs(w: &mut World, _session: Option<SessionId>, parameters: &[String]) {
    let players = player_manager::get_all_offline(w);

    let (fix, fix_str) = fix_of(parameters);
    let mut found_issues = false;

    for &player in &players {
        for &(aug_type, prop) in empyrean_world::world_objects::augmentation_device::AUG_PROPS {
            let max = empyrean_world::world_objects::augmentation_device::max_augs(aug_type);
            let aug_prop = offline(w, player).get_property(prop).unwrap_or(0);

            if aug_prop >= 0 && aug_prop <= max {
                continue;
            }

            let mut msg = format!(
                "{} has {aug_prop} {}",
                name(w, player),
                prop.to_dotnet_string()
            );

            if aug_prop < 0 {
                msg += &format!(", min should be 0{fix_str}");
            } else {
                msg += &format!(", max should be {max}{fix_str}");
            }

            console_write_line(&msg);

            found_issues = true;

            if fix {
                if aug_prop < 0 {
                    offline_mut(w, player).set_property(prop, 0);
                } else {
                    offline_mut(w, player).set_property(prop, max);
                }

                save_biota_to_database(w, player);
            }
        }
    }
    if !fix && found_issues {
        console_write_line("Dry run completed. Type 'verify-max-augs fix' to fix any issues.");
    }

    if !found_issues {
        console_write_line(&format!(
            "Verified max augs for {} players",
            format(players.len(), "N0")
        ));
    }
}

/// `AugmentationDevices`: each augmentation property and the class name of its augmentation gem
/// (a static `Dictionary`, in insertion order).
pub static AUGMENTATION_DEVICES: LazyLock<DotNetDict<PropertyInt, &'static str>> =
    LazyLock::new(|| {
        let mut d = DotNetDict::new();
        for (k, v) in [
            (
                PropertyInt::AugmentationBonusImbueChance,
                "gemaugmentationluckonimbues",
            ),
            (
                PropertyInt::AugmentationBonusSalvage,
                "gemaugmentationbonussalvage",
            ),
            (PropertyInt::AugmentationBonusXp, "gemaugmentationbonusxp"),
            (
                PropertyInt::AugmentationCriticalDefense,
                "gemaugmentationcriticaldefense",
            ),
            (
                PropertyInt::AugmentationCriticalExpertise,
                "ace41482-eyeoftheremorseless",
            ),
            (
                PropertyInt::AugmentationCriticalPower,
                "ace41481-handoftheremorseless",
            ),
            (
                PropertyInt::AugmentationDamageBonus,
                "ace41478-frenzyoftheslayer",
            ),
            (
                PropertyInt::AugmentationDamageReduction,
                "ace41480-ironskinoftheinvincible",
            ),
            (
                PropertyInt::AugmentationExtraPackSlot,
                "gemaugmentationpackslot",
            ),
            (
                PropertyInt::AugmentationFasterRegen,
                "gemaugmentationfastregen",
            ),
            (
                PropertyInt::AugmentationIncreasedCarryingCapacity,
                "gemaugmentationcarryingcapacityi",
            ),
            (
                PropertyInt::AugmentationIncreasedSpellDuration,
                "gemaugmentationspellduration",
            ),
            (
                PropertyInt::AugmentationInfusedCreatureMagic,
                "ace41472-infusedcreaturemagic",
            ),
            (
                PropertyInt::AugmentationInfusedItemMagic,
                "ace41473-infuseditemmagic",
            ),
            (
                PropertyInt::AugmentationInfusedLifeMagic,
                "ace41474-infusedlifemagic",
            ),
            (
                PropertyInt::AugmentationInfusedVoidMagic,
                "ace41479-infusedvoidmagic",
            ),
            (
                PropertyInt::AugmentationInfusedWarMagic,
                "ace41475-infusedwarmagic",
            ),
            (
                PropertyInt::AugmentationInnateCoordination,
                "gemaugmentationattcoordination",
            ),
            (
                PropertyInt::AugmentationInnateEndurance,
                "gemaugmentationattendurance",
            ),
            (
                PropertyInt::AugmentationInnateFocus,
                "gemaugmentationattfocus",
            ),
            (
                PropertyInt::AugmentationInnateQuickness,
                "gemaugmentationattquickness",
            ),
            (
                PropertyInt::AugmentationInnateSelf,
                "gemaugmentationattself",
            ),
            (
                PropertyInt::AugmentationInnateStrength,
                "gemaugmentationattstrength",
            ),
            (
                PropertyInt::AugmentationJackOfAllTrades,
                "ace43167-jackofalltrades",
            ),
            (
                PropertyInt::AugmentationLessDeathItemLoss,
                "gemaugmentationdeathreduceditems",
            ),
            (
                PropertyInt::AugmentationResistanceAcid,
                "gemaugmentationnaturalresistanceacid",
            ),
            (
                PropertyInt::AugmentationResistanceBlunt,
                "gemaugmentationnaturalresistancebludg",
            ),
            (
                PropertyInt::AugmentationResistanceFire,
                "gemaugmentationnaturalresistancefire",
            ),
            (
                PropertyInt::AugmentationResistanceFrost,
                "gemaugmentationnaturalresistancefrost",
            ),
            (
                PropertyInt::AugmentationResistanceLightning,
                "gemaugmentationnaturalresistanceelectric",
            ),
            (
                PropertyInt::AugmentationResistancePierce,
                "gemaugmentationnaturalresistancepierc",
            ),
            (
                PropertyInt::AugmentationResistanceSlash,
                "gemaugmentationnaturalresistanceslash",
            ),
            (
                PropertyInt::AugmentationSkilledMagic,
                "ace41476-masterofthefivefoldpath",
            ),
            (
                PropertyInt::AugmentationSkilledMelee,
                "ace41477-masterofthesteelcircle",
            ),
            (
                PropertyInt::AugmentationSkilledMissile,
                "ace41490-masterofthefocusedeye",
            ),
            (
                PropertyInt::AugmentationSpecializeArmorTinkering,
                "gemaugmentationtinkeringspecarmor",
            ),
            (
                PropertyInt::AugmentationSpecializeItemTinkering,
                "gemaugmentationtinkeringspecitem",
            ),
            (
                PropertyInt::AugmentationSpecializeMagicItemTinkering,
                "gemaugmentationtinkeringspecmagic",
            ),
            (
                PropertyInt::AugmentationSpecializeSalvaging,
                "gemaugmentationtinkeringspecsalv",
            ),
            (
                PropertyInt::AugmentationSpecializeWeaponTinkering,
                "gemaugmentationtinkeringspecweap",
            ),
            (
                PropertyInt::AugmentationSpellsRemainPastDeath,
                "gemaugmentationdeathspellsremain",
            ),
        ] {
            d.add(k, v);
        }
        d
    });

// ACE: DeveloperFixCommands.HandleVerifyExperience
/// `verify-xp (fix)`: total XP against the XP spent on attributes, vitals, skills and
/// augmentations.
pub fn handle_verify_experience(w: &mut World, _session: Option<SessionId>, parameters: &[String]) {
    let players = player_manager::get_all_offline(w);

    let (fix, fix_str) = fix_of(parameters);
    let mut found_issues = false;

    let mut results: Vec<VerifyXpResult> = Vec::new();

    // Asheron's Lesser Benediction augmentation operates differently than all other augs
    let lesser_benediction: HashSet<u32> = {
        let characters = all_characters(&mut **w.shard.base_database());
        quest_registry_rows(&characters, "LesserBenedictionAug")
            .into_iter()
            .map(|(c, _)| c)
            .collect()
    };

    for &player in &players {
        let p = offline(w, player);
        let total_xp = p.get_property(PropertyInt64::TotalExperience).unwrap_or(0);
        let unassigned_xp = p
            .get_property(PropertyInt64::AvailableExperience)
            .unwrap_or(0);

        // loop through all attributes/vitals/skills, add up assigned xp
        let mut attribute_xp: i64 = 0;
        let mut vital_xp: i64 = 0;
        let mut skill_xp: i64 = 0;
        let mut aug_xp: i64 = 0;

        let diff_xp: i64 = 0.min(p.get_property(PropertyInt64::VerifyXp).unwrap_or(0));

        for attribute in p
            .biota
            .properties_attribute
            .as_ref()
            .expect("NullReferenceException: PropertiesAttribute")
            .values()
        {
            attribute_xp += i64::from(attribute.cp_spent);
        }

        for vital in p
            .biota
            .properties_attribute_2nd
            .as_ref()
            .expect("NullReferenceException: PropertiesAttribute2nd")
            .values()
        {
            vital_xp += i64::from(vital.cp_spent);
        }

        for skill in p
            .biota
            .properties_skill
            .as_ref()
            .expect("NullReferenceException: PropertiesSkill")
            .values()
        {
            skill_xp += i64::from(skill.pp);
        }

        // find any xp spent on augs
        let Some(heritage) = p
            .get_property(PropertyInt::HeritageGroup)
            .map(HeritageGroup)
        else {
            continue; // ignore admins who have morphed into asheron / bael'zharon
        };

        let heritage_aug = get_heritage_aug(heritage);

        for (&aug_property, &class_name) in AUGMENTATION_DEVICES.iter() {
            let mut num_augs = offline(w, player).get_property(aug_property).unwrap_or(0);
            if Some(aug_property) == heritage_aug {
                num_augs -= 1;
            }

            if num_augs <= 0 {
                continue;
            }

            let aug = w
                .content
                .get_cached_weenie_by_class_name(class_name)
                .expect("NullReferenceException: GetCachedWeenie");
            let cost_per = aug
                .properties_int64
                .as_ref()
                .expect("NullReferenceException: PropertiesInt64")
                .get(&PropertyInt64::AugmentationCost)
                .copied()
                .unwrap_or(0);

            aug_xp += cost_per.wrapping_mul(i64::from(num_augs));
        }

        if lesser_benediction.contains(&player.full()) {
            aug_xp += 2_000_000_000;
        }

        let calculated_spent = attribute_xp + vital_xp + skill_xp + aug_xp + diff_xp;

        let current_spent = total_xp - unassigned_xp;

        let bonus_xp = (current_spent - calculated_spent) % 526;

        if calculated_spent != current_spent && bonus_xp != 0 {
            // the results for this data set can be large,
            // especially due to an earlier ace bug where it wasn't calculating the Proficiency Points correctly

            // instead of displaying the results in random order,
            // we going to sort them all by diff

            found_issues = true;
            results.push(VerifyXpResult::new(player, calculated_spent, current_spent));
        }
    }

    let max_total_xp: i64 = last(&w.dats.portal_dat().xp_table().level_xp).cs_cast();

    // results.OrderBy(i => i.Player.Name).OrderBy(i => i.Diff): two stable sorts
    let mut ordered = results.clone();
    ordered.sort_by(|a, b| culture_compare(&name(w, a.player), &name(w, b.player)));
    ordered.sort_by_key(VerifyXpResult::diff);

    for result in &ordered {
        let player = result.player;
        let diff = result.diff();

        console_write_line(&format!(
            "{} is calculated to have spent {} experience, which currently differs by {}{fix_str}",
            name(w, player),
            format(result.calculated, "N0"),
            format(diff, "N0")
        ));

        if !fix {
            continue;
        }

        let p = offline_mut(w, player);
        if diff > 0 {
            // add to unassigned xp
            let unassigned_xp = p
                .get_property(PropertyInt64::AvailableExperience)
                .unwrap_or(0);
            p.set_property(PropertyInt64::AvailableExperience, unassigned_xp + diff);
            p.set_property(PropertyInt64::VerifyXp, diff);
        } else {
            let total_xp = p.get_property(PropertyInt64::TotalExperience).unwrap_or(0);
            if total_xp - diff > max_total_xp {
                let unassigned_xp = p
                    .get_property(PropertyInt64::AvailableExperience)
                    .unwrap_or(0);
                if unassigned_xp + diff >= 0 {
                    // this is the only (rare) case where subtracting from AvailableExperience is required
                    p.set_property(PropertyInt64::AvailableExperience, unassigned_xp + diff);
                } else {
                    console_write_line("ERROR: couldn't fix, xp exceeds all bounds");
                }
            } else {
                // setting the diff property below, which will be handled on player login
                // to properly handle possibly leveling up / skill credits
                p.set_property(PropertyInt64::VerifyXp, diff);
            }
        }
        save_biota_to_database(w, player);
    }

    if found_issues {
        console_write_line(&format!(
            "{} issues for {} players",
            if fix { "Fixed" } else { "Found" },
            format(results.len(), "N0")
        ));

        if !fix {
            console_write_line("Dry run completed. Type 'verify-xp fix' to fix any issues.");
        }
    } else {
        console_write_line(&format!(
            "Verified XP for {} players",
            format(players.len(), "N0")
        ));
    }
}

/// `(WeenieClassName)wcid` of `ACE.Entity.Enum` (a `ushort` enum: the id is truncated).
fn entity_weenie_class_name(wcid: u32) -> String {
    let truncated: u16 = wcid.cs_cast();
    empyrean_entity::enums::WeenieClassName(truncated).to_dotnet_string()
}

// ACE: DeveloperFixCommands.HandleFixBiotaEmoteDelay
/// `fix-biota-emote-delay (fix)`: biota emote actions whose delay is 1 where the weenie's is 0.
pub fn handle_fix_biota_emote_delay(
    w: &mut World,
    session: Option<SessionId>,
    parameters: &[String],
) {
    let info = |w: &mut World, output: &str| {
        command_handler_helper::write_output_info(
            w,
            session,
            output,
            empyrean_entity::enums::ChatMessageType::Broadcast,
        )
    };

    if parameters.is_empty() {
        info(w, "This command is intended to be run while the world is in offline mode, or there are 0 players connected.");
        info(w, "To run this fix, type fix-biota-emote-delay fix");
        return;
    }

    let fix = player_manager::equals_ordinal_ignore_case(&parameters[0], "fix");

    info(w, "Building weenie emote cache");

    let weenie_emote_cache = build_weenie_emote_cache(w);

    info(
        w,
        &format!(
            "Found {} weenie templates w/ emote actions with delay 0",
            format(weenie_emote_cache.len(), "N0")
        ),
    );

    info(w, "Finding biotas for these wcids");

    let mut biotas: Vec<Biota> = all_biotas(&mut **w.shard.base_database())
        .into_iter()
        .filter(|i| weenie_emote_cache.contains_key(&i.weenie_class_id))
        .collect();

    let mut distinct: Vec<u32> = Vec::new();
    let mut counts: DotNetDict<u32, u32> = DotNetDict::new();
    for biota in &biotas {
        if !distinct.contains(&biota.weenie_class_id) {
            distinct.push(biota.weenie_class_id);
        }
        match counts.get_mut(&biota.weenie_class_id) {
            None => {
                counts.insert(biota.weenie_class_id, 1);
            }
            Some(c) => *c += 1,
        }
    }

    info(
        w,
        &format!(
            "Found {} biotas matching {} distinct wcids",
            biotas.len(),
            distinct.len()
        ),
    );

    let mut ordered: Vec<(u32, u32)> = counts.iter().map(|(k, v)| (*k, *v)).collect();
    ordered.sort_by_key(|(k, _)| *k);
    for (key, value) in ordered {
        info(
            w,
            &format!("{key} - {} ({value})", entity_weenie_class_name(key)),
        );
    }

    if !fix {
        info(w, "Dry run completed");
        return;
    }

    let mut total_updated = 0;
    let mut changed: Vec<Biota> = Vec::new();

    for biota in &mut biotas {
        let mut updated = false;

        let weenie_emotes = weenie_emote_cache
            .get(&biota.weenie_class_id)
            .expect("KeyNotFoundException");

        for emote in &mut biota.biota_properties_emote {
            // ensure this delay 1 should be delay 0
            let hash = calculate_emote_hash(
                emote.category,
                emote.probability,
                emote.weenie_class_id,
                emote.style,
                emote.substyle,
                emote.quest.as_deref(),
                emote.vendor_type,
            );
            for action in &mut emote.biota_properties_emote_action {
                if action.delay != 1.0 {
                    continue;
                }

                let Some(list) = weenie_emotes.get(&hash) else {
                    //CommandHandlerHelper.WriteOutputInfo(session, $"Skipping emote for {biota.WeenieClassId} not found in hash list");
                    continue;
                };
                if !list.contains(&action.order) {
                    //CommandHandlerHelper.WriteOutputInfo(session, $"Skipping emote for {biota.WeenieClassId} not found in action list");
                    continue;
                }

                // confirmed match, update delay 1 -> 0
                action.delay = 0.0;
                updated = true;
            }
        }

        if updated {
            let text = format!(
                "Fixed shard object {:08X} of type {} - {}",
                biota.id,
                biota.weenie_class_id,
                entity_weenie_class_name(biota.weenie_class_id)
            );
            info(w, &text);
            total_updated += 1;
            changed.push(biota.clone());
        }
    }
    save_changes(&mut **w.shard.base_database(), |db| {
        for mut b in changed {
            db.write_biota(&mut b)?;
        }
        Ok(())
    })
    .unwrap_or_else(|e| panic!("{e}"));

    info(
        w,
        &format!("Completed successfully, fixed {total_updated} shard items"),
    );
}

// ACE: DeveloperFixCommands.BuildWeenieEmoteCache
/// wcid => emote hash => the order ids of its actions with delay 0.
fn build_weenie_emote_cache(w: &World) -> DotNetDict<u32, DotNetDict<i32, DotNetHashSet<u32>>> {
    let mut emote_cache: DotNetDict<u32, DotNetDict<i32, DotNetHashSet<u32>>> = DotNetDict::new();

    for weenie in w.content.get_all_weenies() {
        for emote in &weenie.weenie_properties_emote {
            for action in emote
                .weenie_properties_emote_action
                .iter()
                .filter(|a| a.delay == 0.0)
            {
                let wcid = emote.object_id;

                if !emote_cache.contains_key(&wcid) {
                    emote_cache.add(wcid, DotNetDict::new());
                }
                let hash_table = emote_cache.get_mut(&wcid).expect("added");

                let emote_hash = calculate_emote_hash(
                    emote.category,
                    emote.probability,
                    emote.weenie_class_id,
                    emote.style,
                    emote.substyle,
                    emote.quest.as_deref(),
                    emote.vendor_type,
                );

                if !hash_table.contains_key(&emote_hash) {
                    hash_table.add(emote_hash, DotNetHashSet::new());
                }

                hash_table
                    .get_mut(&emote_hash)
                    .expect("added")
                    .insert(action.order);
            }
        }
    }

    emote_cache
}

/// .NET `float.GetHashCode()`: the bits, with both zeros (and every NaN) folded.
fn float_hash_code(value: f32) -> i32 {
    let mut bits = value.to_bits().cast_signed();
    if (bits.wrapping_sub(1) & 0x7FFF_FFFF) >= 0x7F80_0000 {
        bits &= 0x7F80_0000;
    }
    bits
}

/// A stand-in for `string.GetHashCode()` (see the module docs): FNV-1a over the UTF-16 units.
fn string_hash_code(value: &str) -> i32 {
    let mut hash: u32 = 0x811C_9DC5;
    for unit in value.encode_utf16() {
        hash ^= u32::from(unit);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash.cast_signed()
}

// ACE: DeveloperFixCommands.CalculateEmoteHash
/// The emote's hash (all three ACE overloads).
#[must_use]
pub fn calculate_emote_hash(
    category: u32,
    probability: f32,
    wcid: Option<u32>,
    style: Option<u32>,
    substyle: Option<u32>,
    quest: Option<&str>,
    vendor_type: Option<i32>,
) -> i32 {
    // no illegitimate collisions found that require doing exact equality comparison as of 11/23/2019 emote data

    let mut hash: i32 = 0;

    hash = hash.wrapping_mul(397) ^ category.cast_signed();
    hash = hash.wrapping_mul(397) ^ float_hash_code(probability);

    if let Some(wcid) = wcid {
        hash = hash.wrapping_mul(397) ^ wcid.cast_signed();
    }

    if let Some(style) = style {
        hash = hash.wrapping_mul(397) ^ style.cast_signed();
    }

    if let Some(substyle) = substyle {
        hash = hash.wrapping_mul(397) ^ substyle.cast_signed();
    }

    if let Some(quest) = quest {
        hash = hash.wrapping_mul(397) ^ string_hash_code(quest);
    }

    if let Some(vendor_type) = vendor_type {
        hash = hash.wrapping_mul(397) ^ vendor_type;
    }

    hash
}

/// The biota's int property row of `type`.
fn int_of(biota: &Biota, r#type: PropertyInt) -> Option<i32> {
    biota
        .biota_properties_int
        .iter()
        .find(|r| r.r#type == r#type.0)
        .map(|r| r.value)
}

/// The biota's string property row of `type`.
fn string_of(biota: &Biota, r#type: PropertyString) -> Option<&str> {
    biota
        .biota_properties_string
        .iter()
        .find(|r| r.r#type == r#type.0)
        .map(|r| r.value.as_str())
}

/// `ctx.BiotaPropertiesInt.Where(i => i.Type == type).ToDictionary(i => i.ObjectId, i => i.Value)`.
fn int_dictionary(biotas: &[Biota], r#type: PropertyInt) -> DotNetDict<u32, i32> {
    let mut d = DotNetDict::new();
    for b in biotas {
        if let Some(v) = int_of(b, r#type) {
            d.add(b.id, v);
        }
    }
    d
}

// ACE: DeveloperFixCommands.HandleFixArmorLevel
/// `verify-armor-levels (fix)`: loot-generated armor above the armor level cap.
pub fn handle_fix_armor_level(w: &mut World, _session: Option<SessionId>, parameters: &[String]) {
    console_write_line("Fetching shard armors (this may take awhile on large servers) ...");

    let mut biotas = all_biotas(&mut **w.shard.base_database());

    let resist_magic = get_resist_magic(&biotas);
    let tinker_logs = get_tinker_logs(&biotas);
    let num_times_tinkered = get_num_times_tinkered(&biotas);
    let imbued_effects = get_imbued_effect(&biotas);

    let (fix, fix_str) = fix_of(parameters);

    // get all loot-generated items on server with armor level
    // (armor join workmanship join name join validLocations, orderby armor.Value descending)
    let mut armor_items: Vec<(u32, i32, String, i32)> = biotas
        .iter()
        .filter_map(|b| {
            let armor_level = int_of(b, PropertyInt::ArmorLevel)?;
            int_of(b, PropertyInt::ItemWorkmanship)?;
            let name = string_of(b, PropertyString::Name)?.to_owned();
            let valid_locs = int_of(b, PropertyInt::ValidLocations)?;
            Some((b.id, armor_level, name, valid_locs))
        })
        .collect();
    armor_items.sort_by_key(|a| std::cmp::Reverse(a.1));

    let mut adjusted = 0;
    let mut changed: Vec<(u32, i32)> = Vec::new();

    for (guid, armor_level, name, valid_locs) in &armor_items {
        // ignore unenchantable
        if resist_magic.get(guid).is_some_and(|&resist| resist == 9999) {
            continue;
        }

        let tinker_log = tinker_logs.get(guid).map(|t| TinkerLog::new(Some(t)));

        let num_tinkers = num_times_tinkered.get(guid).copied().unwrap_or(0);
        let imbued_effect = imbued_effects.get(guid).copied().unwrap_or(0);

        let num_armor_tinkers = tinker_log
            .as_ref()
            .map_or(0, |t| t.num_tinkers(MaterialType::Steel));

        let num_armor_tinker_str = if num_armor_tinkers > 0 {
            format!(" ({num_armor_tinkers})")
        } else {
            String::new()
        };

        let equip_mask = EquipMask(valid_locs.cast_unsigned());

        let new_armor_level = get_armor_level(
            *armor_level,
            equip_mask,
            tinker_log.as_ref(),
            num_tinkers,
            imbued_effect,
        );

        if new_armor_level != *armor_level {
            if fix {
                changed.push((*guid, new_armor_level));
            }

            console_write_line(&format!(
                "{name}, {armor_level}{num_armor_tinker_str} => {new_armor_level}{fix_str}"
            ));

            adjusted += 1;
        }
    }
    if fix {
        for b in &mut biotas {
            if let Some((_, value)) = changed.iter().find(|(g, _)| *g == b.id) {
                for r in b
                    .biota_properties_int
                    .iter_mut()
                    .filter(|r| r.r#type == PropertyInt::ArmorLevel.0)
                {
                    r.value = *value;
                }
            }
        }
        write_biotas(
            w,
            biotas
                .iter()
                .filter(|b| changed.iter().any(|(g, _)| *g == b.id)),
        );
    }

    let will_be = if fix { " " } else { " will be " };

    if adjusted > 0 {
        console_write_line(&format!(
            "Found {} armors, {}{will_be}adjusted",
            format(armor_items.len(), "N0"),
            format(adjusted, "N0")
        ));

        if !fix {
            console_write_line(
                "Dry run completed. Type 'verify-armor-levels fix' to fix any issues.",
            );
        }
    } else {
        console_write_line(&format!(
            "Verified {} armors.",
            format(armor_items.len(), "N0")
        ));
    }
}

/// `ctx.SaveChanges()` over the changed biotas.
fn write_biotas<'a>(w: &World, biotas: impl Iterator<Item = &'a Biota>) {
    let changed: Vec<Biota> = biotas.cloned().collect();
    save_changes(&mut **w.shard.base_database(), |db| {
        for mut b in changed {
            empyrean_store::shard_database::set_biota_populated_collections(&mut b);
            db.write_biota(&mut b)?;
        }
        Ok(())
    })
    .unwrap_or_else(|e| panic!("{e}"));
}

// ACE: DeveloperFixCommands.GetResistMagic
#[must_use]
pub fn get_resist_magic(biotas: &[Biota]) -> DotNetDict<u32, i32> {
    int_dictionary(biotas, PropertyInt::ResistMagic)
}

// ACE: DeveloperFixCommands.GetTinkerLogs
#[must_use]
pub fn get_tinker_logs(biotas: &[Biota]) -> DotNetDict<u32, String> {
    let mut d = DotNetDict::new();
    for b in biotas {
        if let Some(v) = string_of(b, PropertyString::TinkerLog) {
            d.add(b.id, v.to_owned());
        }
    }
    d
}

// ACE: DeveloperFixCommands.GetNumTimesTinkered
#[must_use]
pub fn get_num_times_tinkered(biotas: &[Biota]) -> DotNetDict<u32, i32> {
    int_dictionary(biotas, PropertyInt::NumTimesTinkered)
}

// ACE: DeveloperFixCommands.GetImbuedEffect
#[must_use]
pub fn get_imbued_effect(biotas: &[Biota]) -> DotNetDict<u32, i32> {
    int_dictionary(biotas, PropertyInt::ImbuedEffect)
}

/// head / hands / feet (`MaxArmorLevel_Extremity`)
pub const MAX_ARMOR_LEVEL_EXTREMITY: i32 = 345;

/// everything else (`MaxArmorLevel_NonExtremity`)
pub const MAX_ARMOR_LEVEL_NON_EXTREMITY: i32 = 315;

// ACE: DeveloperFixCommands.GetArmorLevel
/// The armor level capped at the extremity or non-extremity cap plus 20 per steel tinker (from a
/// full tinker log, or a random count up to the unlogged tinkers).
#[must_use]
pub fn get_armor_level(
    armor_level: i32,
    equip_mask: EquipMask,
    tinker_log: Option<&TinkerLog>,
    num_tinkers: i32,
    imbued_effect: i32,
) -> i32 {
    let mut max_armor_level = if (equip_mask & EquipMask::Extremity) != EquipMask::None {
        MAX_ARMOR_LEVEL_EXTREMITY
    } else {
        MAX_ARMOR_LEVEL_NON_EXTREMITY
    };

    if let Some(tinker_log) =
        tinker_log.filter(|t| i32::try_from(t.tinkers.len()).unwrap_or(i32::MAX) == num_tinkers)
    {
        // full tinkering log available
        max_armor_level += tinker_log.num_tinkers(MaterialType::Steel) * 20;
    } else if num_tinkers > 0 {
        // partial or no tinkering log available
        let mut rng_max = num_tinkers;
        if imbued_effect != 0 {
            rng_max -= 1;
        }

        if rng_max > 0 {
            // prevent further iterations on multiple re-runs
            if armor_level <= max_armor_level + rng_max * 20 {
                return armor_level;
            }

            let rng = ThreadSafeRandom::next(0, rng_max);
            max_armor_level += rng * 20;
        }
    }
    armor_level.min(max_armor_level)
}

/// Inserts an int property row, as `insert into biota_properties_int set object_Id=.., type=..,
/// value=..` does.
///
/// # Panics
/// When the biota does not exist (MySQL's foreign key error, which ACE does not catch).
///
/// Not ACE's (a fix): a row that already exists takes the value; ACE's
/// plain insert threw a duplicate key error for it, which ended the command part way.
fn insert_int_row(w: &World, object_id: u32, r#type: PropertyInt, value: i32) {
    let mut db = w.shard.base_database();
    let mut biota = empyrean_store::shard_database_offline_tools::load_biota(&mut **db, object_id)
        .expect("MySqlException: a foreign key constraint fails");
    biota.biota_properties_int.retain(|r| r.r#type != r#type.0);
    biota.biota_properties_int.push(BiotaPropertiesInt {
        object_id,
        r#type: r#type.0,
        value,
    });
    biota.biota_properties_int.sort_by_key(|r| r.r#type);
    empyrean_store::shard_database::set_biota_populated_collections(&mut biota);
    db.write_biota(&mut biota).unwrap_or_else(|e| panic!("{e}"));
}

// ACE: DeveloperFixCommands.HandleVerifyClothingWieldLevel
/// `verify-clothing-wield-level (fix)`: t7/t8 clothing (epic or legendary cantrips) with no wield
/// level requirement.
pub fn handle_verify_clothing_wield_level(
    w: &mut World,
    _session: Option<SessionId>,
    parameters: &[String],
) {
    let (fix, fix_str) = fix_of(parameters);
    let mut found_issues = false;

    let biotas = all_biotas(&mut **w.shard.base_database());

    // get all shard clothing
    let clothing_biotas: Vec<&Biota> = biotas
        .iter()
        .filter(|i| i.weenie_type == WeenieType::Clothing.0.cast_signed())
        .collect();

    // get all shard armor levels
    let armor_levels = int_dictionary(&biotas, PropertyInt::ArmorLevel);

    // filter clothing to actual clothing
    let mut clothing: DotNetDict<u32, &Biota> = DotNetDict::new();
    for item in clothing_biotas {
        if armor_levels
            .get(&item.id)
            .is_none_or(|&armor_level| armor_level == 0)
        {
            clothing.add(item.id, item);
            //Console.WriteLine($"{item.Id:X8} - {(Factories.Enum.WeenieClassName)item.WeenieClassId}");
        }
    }

    // get shard spells
    let spells: Vec<(u32, i32)> = biotas
        .iter()
        .filter(|b| clothing.contains_key(&b.id))
        .flat_map(|b| {
            b.biota_properties_spell_book
                .iter()
                .map(move |s| (b.id, s.spell))
        })
        .collect();

    // filter clothing to those with epics/legendaries
    let mut high_tier_clothing: DotNetDict<u32, i32> = DotNetDict::new();

    for (object_id, spell) in spells {
        let mut cantrip_level = 0;

        if CANTRIP_SETS.epic_cantrips.contains(&spell) {
            cantrip_level = 3;
        } else if CANTRIP_SETS.legendary_cantrips.contains(&spell) {
            cantrip_level = 4;
        }

        if cantrip_level == 0 {
            continue;
        }

        if let Some(level) = high_tier_clothing.get_mut(&object_id) {
            *level = (*level).max(cantrip_level);
        } else {
            high_tier_clothing.insert(object_id, cantrip_level);
        }
    }

    // get wield level for these items
    let wield_levels: HashSet<u32> = biotas
        .iter()
        .filter(|b| {
            high_tier_clothing.contains_key(&b.id)
                && int_of(b, PropertyInt::WieldDifficulty).is_some()
        })
        .map(|b| b.id)
        .collect();

    for (&object_id, &max_cantrip_level) in high_tier_clothing.iter() {
        if wield_levels.contains(&object_id) {
            continue;
        }

        if !found_issues {
            console_write_line("Missing wield difficulty:");
            found_issues = true;
        }

        if fix {
            let mut wield_level = 150;

            if max_cantrip_level > 3 {
                let rng = ThreadSafeRandom::next_float(0.0, 1.0);
                if rng < 0.9 {
                    wield_level = 180;
                }
            }

            // Not ACE's (a fix): an item with a WieldRequirements (or
            // WieldSkillType) row but no WieldDifficulty has those rows set (`insert_int_row`);
            // ACE's inserts threw a duplicate key error for it, ending the command part way.
            insert_int_row(
                w,
                object_id,
                PropertyInt::WieldRequirements,
                WieldRequirement::Level.0,
            );
            insert_int_row(w, object_id, PropertyInt::WieldSkillType, 1);
            insert_int_row(w, object_id, PropertyInt::WieldDifficulty, wield_level);
        }

        let item = *clothing.get(&object_id).expect("KeyNotFoundException");

        console_write_line(&format!(
            "{:08X} - {} - {max_cantrip_level}{fix_str}",
            item.id,
            factories_weenie_class_name(item.weenie_class_id)
        ));
    }

    if !fix && found_issues {
        console_write_line(
            "Dry run completed. Type 'verify-clothing-wield-level fix' to fix any issues.",
        );
    }

    if !found_issues {
        console_write_line(&format!(
            "Verified wield levels for {} pieces of t7 / t8 clothing",
            format(high_tier_clothing.len(), "N0")
        ));
    }
}

/// `(Factories.Enum.WeenieClassName)wcid` (an `int` enum).
fn factories_weenie_class_name(wcid: u32) -> String {
    empyrean_tables::enums::WeenieClassName(wcid.cast_signed()).to_dotnet_string()
}

// ACE: DeveloperFixCommands.HandleVerifyLegendaryWieldLevel
/// `verify-legendary-wield-level (fix)`: items with legendary cantrips whose level requirement is
/// below 180.
pub fn handle_verify_legendary_wield_level(
    w: &mut World,
    _session: Option<SessionId>,
    parameters: &[String],
) {
    let (fix, _fix_str) = fix_of(parameters);
    let mut found_issues = false;

    let biotas = all_biotas(&mut **w.shard.base_database());

    // get all biota spellbooks
    let spellbook: Vec<(u32, i32)> = biotas
        .iter()
        .flat_map(|b| {
            b.biota_properties_spell_book
                .iter()
                .filter(|s| s.probability == 2.0)
                .map(move |s| (b.id, s.spell))
        })
        .collect();

    let mut legendary_items: DotNetHashSet<u32> = DotNetHashSet::new();

    for (object_id, spell) in spellbook {
        if CANTRIP_SETS.legendary_cantrips.contains(&spell) {
            legendary_items.insert(object_id);
        }
    }

    // get wield requirements for these items
    let wield_req = |req_type: PropertyInt, diff_type: PropertyInt| -> Vec<(u32, i32)> {
        biotas
            .iter()
            .filter(|b| {
                legendary_items.contains(&b.id)
                    && int_of(b, req_type) == Some(WieldRequirement::Level.0)
            })
            .filter_map(|b| int_of(b, diff_type).map(|d| (b.id, d)))
            .collect()
    };
    let wield_req1 = wield_req(PropertyInt::WieldRequirements, PropertyInt::WieldDifficulty);

    let wield_req2 = wield_req(
        PropertyInt::WieldRequirements2,
        PropertyInt::WieldDifficulty2,
    );

    let mut verified: HashSet<u32> = HashSet::new();

    let mut has_level_req1: HashSet<u32> = HashSet::new();
    let mut has_level_req2: HashSet<u32> = HashSet::new();

    let mut updates: Vec<String> = Vec::new();
    let mut apply: Vec<(u32, PropertyInt)> = Vec::new();

    for (object_id, wield_diff) in wield_req1 {
        has_level_req1.insert(object_id);

        if wield_diff < 180 {
            found_issues = true;
            updates.push(format!("UPDATE biota_properties_int SET value=180 WHERE object_Id=0x{object_id:08X} AND type={};", PropertyInt::WieldDifficulty.0));
            apply.push((object_id, PropertyInt::WieldDifficulty));
        } else {
            verified.insert(object_id);
        }
    }

    for (object_id, wield_diff) in wield_req2 {
        has_level_req2.insert(object_id);

        if wield_diff < 180 {
            found_issues = true;
            updates.push(format!("UPDATE biota_properties_int SET value=180 WHERE object_Id=0x{object_id:08X} AND type={};", PropertyInt::WieldDifficulty2.0));
            apply.push((object_id, PropertyInt::WieldDifficulty2));
        } else {
            verified.insert(object_id);
        }
    }

    /*var hasLevelReq = hasLevelReq1.Union(hasLevelReq2).ToList(); ... (commented out in ACE) */

    let num_issues = i32::try_from(legendary_items.len()).unwrap_or(i32::MAX)
        - i32::try_from(verified.len()).unwrap_or(i32::MAX);

    if num_issues > 0 {
        console_write_line(&format!(
            "Found issues for {} of {} legendary items",
            format(num_issues, "N0"),
            format(legendary_items.len(), "N0")
        ));
    }

    if !fix && found_issues {
        console_write_line(
            "Dry run completed. Type 'verify-legendary-wield-level fix' to fix any issues.",
        );
    }

    if fix {
        for (update, (object_id, r#type)) in updates.iter().zip(&apply) {
            console_write_line(update);
            let mut db = w.shard.base_database();
            if let Some(mut biota) =
                empyrean_store::shard_database_offline_tools::load_biota(&mut **db, *object_id)
            {
                for r in biota
                    .biota_properties_int
                    .iter_mut()
                    .filter(|r| r.r#type == r#type.0)
                {
                    r.value = 180;
                }
                db.write_biota(&mut biota).unwrap_or_else(|e| panic!("{e}"));
            }
        }
    }

    if !found_issues {
        console_write_line(&format!(
            "Verified wield levels for {} legendary items",
            format(legendary_items.len(), "N0")
        ));
    }
}

// ACE: DeveloperFixCommands.HandleRemoveShieldRatings
/// `verify-shield-rating (fix)`: loot-generated shields with crit damage or crit damage resist
/// ratings.
pub fn handle_remove_shield_ratings(
    w: &mut World,
    _session: Option<SessionId>,
    parameters: &[String],
) {
    let (fix, fix_str) = fix_of(parameters);
    let mut found_issues = false;

    let biotas = all_biotas(&mut **w.shard.base_database());

    // get items with GearCritDamage
    let crit_damage = int_dictionary(&biotas, PropertyInt::GearCritDamage);

    // get items with GearCritDamageResist
    let crit_damage_resist = int_dictionary(&biotas, PropertyInt::GearCritDamageResist);

    // get lootgen shields
    let mut results: DotNetDict<u32, (u32, String)> = DotNetDict::new();
    for b in &biotas {
        if int_of(b, PropertyInt::ItemWorkmanship).is_none()
            || int_of(b, PropertyInt::CombatUse) != Some(i32::from(CombatUse::Shield.0))
        {
            continue;
        }
        let Some(name) = string_of(b, PropertyString::Name) else {
            continue;
        };
        results.add(b.id, (b.id, name.to_owned()));
    }

    // generate list of remove fields
    let mut crit_damage_remove: DotNetDict<u32, i32> = DotNetDict::new();
    let mut crit_damage_resist_remove: DotNetDict<u32, i32> = DotNetDict::new();

    for &result in results.keys() {
        if let Some(&crit_damage_result) = crit_damage.get(&result) {
            crit_damage_remove.add(result, crit_damage_result);
        }

        if let Some(&crit_damage_resist_result) = crit_damage_resist.get(&result) {
            crit_damage_resist_remove.add(result, crit_damage_resist_result);
        }
    }

    let num_issues = crit_damage_remove.len() + crit_damage_resist_remove.len();

    let mut sql_lines: Vec<String> = Vec::new();
    let mut removes: Vec<(u32, PropertyInt)> = Vec::new();

    if num_issues > 0 {
        found_issues = true;

        console_write_line(&format!(
            "Found {} bugged shields:",
            format(num_issues, "N0")
        ));

        for (key, value) in crit_damage_remove.iter() {
            let shield = results.get(key).expect("KeyNotFoundException");

            console_write_line(&format!(
                "{:08X} - {} (CD: {value}){fix_str}",
                shield.0, shield.1
            ));

            sql_lines.push(format!(
                "delete from biota_properties_int where object_Id=0x{:08X} and `type`={};",
                shield.0,
                PropertyInt::GearCritDamage.0
            ));
            removes.push((shield.0, PropertyInt::GearCritDamage));
        }

        for (key, value) in crit_damage_resist_remove.iter() {
            let shield = results.get(key).expect("KeyNotFoundException");

            console_write_line(&format!(
                "{:08X} - {} (CDR: {value}){fix_str}",
                shield.0, shield.1
            ));

            sql_lines.push(format!(
                "delete from biota_properties_int where object_Id=0x{:08X} and `type`={};",
                shield.0,
                PropertyInt::GearCritDamageResist.0
            ));
            removes.push((shield.0, PropertyInt::GearCritDamageResist));
        }
    }

    if !fix && found_issues {
        console_write_line("Dry run completed. Type 'verify-shield-rating fix' to fix any issues.");
    }

    if fix {
        // foreach (var sqlLine in sqlLines) ctx.Database.ExecuteSqlRaw(sqlLine): see the module docs
        let _ = sql_lines;
        let mut db = w.shard.base_database();
        for (object_id, r#type) in removes {
            if let Some(mut biota) =
                empyrean_store::shard_database_offline_tools::load_biota(&mut **db, object_id)
            {
                biota.biota_properties_int.retain(|r| r.r#type != r#type.0);
                empyrean_store::shard_database::set_biota_populated_collections(&mut biota);
                db.write_biota(&mut biota).unwrap_or_else(|e| panic!("{e}"));
            }
        }
    }

    if !found_issues {
        console_write_line(&format!("Verified {} shields", format(results.len(), "N0")));
    }
}

/// `PreToPostMeleeRareConversions`: each pre-MoA melee rare's EoR wcid (a static `Dictionary`, in
/// insertion order).
pub const PRE_TO_POST_MELEE_RARE_CONVERSIONS: [(u32, u32); 35] = [
    /* Ridgeback Dagger */ (30310, 45444),
    /* Zharalim Crookblade */ (30311, 45445),
    /* Baton of Tirethas */ (30312, 45446),
    /* Dripping Death */ (30313, 45447),
    /* Star of Tukal */ (30314, 45448),
    /* Subjugator */ (30315, 45449),
    /* Black Thistle */ (30316, 45441),
    /* Moriharu's Kitchen Knife */ (30317, 45442),
    /* Pitfighter's Edge */ (30318, 45443),
    /* Champion's Demise */ (30319, 45451),
    /* Pillar of Fearlessness */ (30320, 45452),
    /* Squire's Glaive */ (30321, 45453),
    /* Star of Gharu'n */ (30322, 45454),
    /* Tri-Blade Spear */ (30323, 45455),
    /* Staff of All Aspects */ (30324, 45456),
    /* Death's Grip Staff */ (30325, 45457),
    /* Staff of Fettered Souls */ (30326, 45458),
    /* Spirit Shifting Staff */ (30327, 45459),
    /* Staff of Tendrils */ (30328, 45460),
    /* Brador's Frozen Eye */ (30329, 45461),
    /* Defiler of Milantos */ (30330, 45462),
    /* Desert Wyrm */ (30331, 45463),
    /* Guardian of Pwyll */ (30332, 45464),
    /* Morrigan's Vanity */ (30333, 45465),
    /* Fist of Three Principles */ (30334, 45466),
    /* Hevelio's Half-Moon */ (30335, 45467),
    /* Malachite Slasher */ (30336, 45468),
    /* Skullpuncher */ (30337, 45469),
    /* Steel Butterfly */ (30338, 45470),
    /* Thunderhead */ (30339, 45450),
    /* Bearded Axe of Souia-Vey */ (30340, 45436),
    /* Canfield Cleaver */ (30341, 45437),
    /* Count Renari's Equalizer */ (30342, 45438),
    /* Smite */ (30343, 45439),
    /* Tusked Axe of Ayan Baqur */ (30344, 45440),
];

/// One row of the rare queries: the biota and its left-joined link rows.
struct RareRow {
    biota: Biota,
    name: String,
    version: Option<i32>,
    container: Option<u32>,
    placement_position: Option<i32>,
    wielder: Option<u32>,
    current_wielded_location: Option<i32>,
    location: Option<BiotaPropertiesPosition>,
    shortcut: bool,
}

fn rare_row(biota: &Biota, characters: &[Character]) -> Option<RareRow> {
    let name = string_of(biota, PropertyString::Name)?.to_owned();
    let iid = |t: PropertyInstanceId| {
        biota
            .biota_properties_iid
            .iter()
            .find(|r| r.r#type == t.0)
            .map(|r| r.value)
    };
    Some(RareRow {
        name,
        version: int_of(biota, PropertyInt::Version),
        container: iid(PropertyInstanceId::Container),
        placement_position: int_of(biota, PropertyInt::PlacementPosition),
        wielder: iid(PropertyInstanceId::Wielder),
        current_wielded_location: int_of(biota, PropertyInt::CurrentWieldedLocation),
        location: biota
            .biota_properties_position
            .iter()
            .find(|p| p.position_type == PositionType::Location.0)
            .cloned(),
        shortcut: characters.iter().any(|c| {
            c.character_properties_shortcut_bar
                .iter()
                .any(|s| s.shortcut_object_id == biota.id)
        }),
        biota: biota.clone(),
    })
}

/// `0x{cell:X8} [{x:F6} {y:F6} {z:F6}] {w:F6} {x:F6} {y:F6} {z:F6}`.
fn location_text(l: &BiotaPropertiesPosition) -> String {
    format!(
        "0x{:08X} [{} {} {}] {} {} {} {}",
        l.obj_cell_id,
        format(l.origin_x, "F6"),
        format(l.origin_y, "F6"),
        format(l.origin_z, "F6"),
        format(l.angles_w, "F6"),
        format(l.angles_x, "F6"),
        format(l.angles_y, "F6"),
        format(l.angles_z, "F6")
    )
}

/// The "contained by" / "wielded by" / "located at" part of a rare's log line.
fn describe_links(biotas: &[Biota], row: &RareRow, with_wielder: bool) -> String {
    let find = |id: u32| {
        biotas
            .iter()
            .find(|b| b.id == id && string_of(b, PropertyString::Name).is_some())
    };
    let placement = row.placement_position.unwrap_or(0);
    let mut logline = String::new();

    if let Some(container_id) = row.container {
        logline += " contained by: ";

        let Some(container) = find(container_id) else {
            logline += &format!("\n Unable to find 0x{container_id} in database. Orphaned object?");
            return logline;
        };
        let container_name = string_of(container, PropertyString::Name).unwrap_or_default();
        let container_weenie_type = WeenieType(container.weenie_type.cast_unsigned());

        if container_weenie_type == WeenieType::Container {
            let parent_container_id = container
                .biota_properties_iid
                .iter()
                .find(|r| r.r#type == PropertyInstanceId::Container.0)
                .map(|r| r.value)
                .expect("InvalidOperationException: Nullable object must have a value.");
            let parent_container =
                find(parent_container_id).expect("NullReferenceException: parentContainer");
            let slot = int_of(container, PropertyInt::PlacementPosition).unwrap_or(0);

            logline += &format!(
                "\n 0x{:08X} {}{} Sub Pack 0x{:08X} {container_name} (Slot {slot}) at placement position {placement}",
                parent_container.id,
                string_of(parent_container, PropertyString::Name).unwrap_or_default(),
                if WeenieType(parent_container.weenie_type.cast_unsigned()) == WeenieType::Storage { "" } else { "'s" },
                container.id
            );
        } else if container_weenie_type == WeenieType::Hook {
            let hook_type = int_of(container, PropertyInt::HookType)
                .expect("NullReferenceException: hook.HookType");
            let location = container
                .biota_properties_position
                .iter()
                .find(|p| p.position_type == PositionType::Location.0)
                .expect("NullReferenceException: hook.Location");

            logline += &format!(
                "\n 0x{:08X} {} Hook located at:\n {}",
                container.id,
                HookType(hook_type).to_dotnet_string(),
                location_text(location)
            );
        } else if container_weenie_type == WeenieType::Storage
            || container_weenie_type == WeenieType::Chest
            || container_weenie_type == WeenieType::SlumLord
        {
            let location = container
                .biota_properties_position
                .iter()
                .find(|p| p.position_type == PositionType::Location.0)
                .expect("NullReferenceException: storage.Location");

            logline += &format!("\n 0x{:08X} {container_name} Main Pack at placement position {placement} and located at:\n {}", container.id, location_text(location));
        } else if container_weenie_type == WeenieType::Corpse {
            let location = container
                .biota_properties_position
                .iter()
                .find(|p| p.position_type == PositionType::Location.0)
                .expect("NullReferenceException: corpse.Location");

            logline += &format!(
                "\n 0x{:08X} {container_name}'s Main Pack 0x{:08X} at placement position {placement} and located at:\n {}",
                container.id,
                container.id,
                location_text(location)
            );
        } else if [
            WeenieType::Creature,
            WeenieType::Admin,
            WeenieType::Sentinel,
            WeenieType::Cow,
            WeenieType::Pet,
            WeenieType::CombatPet,
            WeenieType::Vendor,
        ]
        .contains(&container_weenie_type)
        {
            logline += &format!(
                "\n 0x{:08X} {container_name}'s Main Pack at placement position {placement}",
                container.id
            );
        } else {
            logline += &format!("\n Unexpected WeenieType '{}' for container 0x{:08X} {container_name}. Invalid custom content?", container.weenie_type, container.id);
        }
    } else if let (true, Some(wielder_id)) = (with_wielder, row.wielder) {
        logline += " wielded by: ";

        let wielder = find(wielder_id).expect("NullReferenceException: wielder");
        let slot = row
            .current_wielded_location
            .expect("NullReferenceException: CurrentWieldedLocation");

        logline += &format!(
            "\n 0x{:08X} {} in the {} slot",
            wielder.id,
            string_of(wielder, PropertyString::Name).unwrap_or_default(),
            EquipMask(slot.cast_unsigned()).to_dotnet_string()
        );
    } else if let Some(location) = &row.location {
        logline += &format!(
            " on a landblock and located at:\n {}",
            location_text(location)
        );
    } else {
        logline += " found in database but does not have a Container, Wielder, or Location. Orphaned object?";
    }

    logline
}

/// `ctx.Version.FirstOrDefault()`'s `PatchVersion` checked against `v0.9.271`: `true` when the
/// command may run; otherwise the reason was written.
#[allow(clippy::if_same_then_else)] // ACE's branches, kept
fn world_database_version_ok(w: &World) -> bool {
    let min_patch_ver = "v0.9.271";

    let Some(world_db_version) = w.content.get_version() else {
        console_write_line(&format!("Unable to determine World Database version. Your World Database must be {min_patch_ver} or higher to run this command."));
        return false;
    };
    let current_patch_ver = world_db_version
        .patch_version
        .clone()
        .expect("NullReferenceException: PatchVersion");

    if !current_patch_ver.starts_with('v') {
        console_write_line(&format!("Unexpected patch version format found. Your World Database must be {min_patch_ver} or higher to run this command."));
        return false;
    }

    let current_patch_ver_split: Vec<&str> = current_patch_ver
        .trim_start_matches('v')
        .split('.')
        .collect();
    let min_patch_ver_split: Vec<&str> = min_patch_ver.trim_start_matches('v').split('.').collect();

    let parse = |s: &str| crate::command_parameter_helpers::dotnet_parse::int_try_parse(s);
    let min_major = parse(min_patch_ver_split[0]).unwrap_or(0);
    let min_minor = parse(min_patch_ver_split[1]).unwrap_or(0);
    let min_build = parse(min_patch_ver_split[2]).unwrap_or(0);
    let too_old = format!("World Database must be {min_patch_ver} or higher to run this command. Your current World Database patch version is: {current_patch_ver}");

    let part = |i: usize| {
        current_patch_ver_split
            .get(i)
            .copied()
            .expect("IndexOutOfRangeException: currentPatchVerSplit")
    };
    if parse(part(0)).is_none_or(|cur_major| cur_major < min_major) {
        console_write_line(&too_old);
        return false;
    } else if parse(part(1)).is_none_or(|cur_minor| cur_minor < min_minor) {
        console_write_line(&too_old);
        return false;
    } else if parse(part(2)).is_none_or(|cur_build| cur_build < min_build) {
        if part(2).contains('-') {
            let current_build_split: Vec<&str> = part(2).split('-').collect();

            if parse(current_build_split[0]).is_none_or(|cur_build| cur_build < min_build) {
                console_write_line(&too_old);
                return false;
            }
            // all good here
        } else {
            console_write_line(&too_old);
            return false;
        }
    } else {
        // all good here
    }
    true
}

/// `WorldObjectFactory.CreateNewWorldObject(wcid)`, a new world object's biota and guid, placed
/// as `setup` says, then `wo.SaveBiotaToDatabase()` (written through the shard at once).
fn create_replacement(
    w: &mut World,
    wcid: u32,
    setup: impl FnOnce(&mut EntityBiota),
) -> Option<(ObjectGuid, String, u32)> {
    let wo = world_object_factory::create_new_world_object_by_wcid_in_world(w, wcid)?;
    let mut biota = wo.biota.clone();
    setup(&mut biota);
    let name = biota.get_property(PropertyString::Name).unwrap_or_default();
    let id = wo.guid;
    w.shard.base_database().save_biota(&mut biota, false);
    Some((id, name, wcid))
}

/// Points the first shortcut to `from` at `to` (`ctx.CharacterPropertiesShortcutBar.Where(x =>
/// x.ShortcutObjectId == from).FirstOrDefault()`).
fn move_shortcut(
    db: &mut dyn ShardDatabase,
    from: u32,
    to: u32,
) -> Result<(), empyrean_store::StoreError> {
    for mut c in all_characters(db) {
        if let Some(s) = c
            .character_properties_shortcut_bar
            .iter_mut()
            .find(|s| s.shortcut_object_id == from)
        {
            s.shortcut_object_id = to;
            return db.write_character(&c);
        }
    }
    Ok(())
}

/// Sets (or adds) the biota's `PropertyInt.Version` to 2.
fn set_version_2(biota: &mut Biota) {
    if let Some(version) = biota
        .biota_properties_int
        .iter_mut()
        .find(|x| x.r#type == PropertyInt::Version.0)
    {
        version.value = 2;
    } else {
        biota.biota_properties_int.push(BiotaPropertiesInt {
            object_id: biota.id,
            r#type: PropertyInt::Version.0,
            value: 2,
        });
        biota.biota_properties_int.sort_by_key(|r| r.r#type);
    }
    empyrean_store::shard_database::set_biota_populated_collections(biota);
}

/// The new rare's links: the old one's container, placement, wielder, wielded slot and location.
fn copy_links(row: &RareRow, biota: &mut EntityBiota) {
    if let Some(container) = row.container {
        biota.set_property(PropertyInstanceId::Container, container);
    }

    if let Some(placement) = row.placement_position {
        biota.set_property(PropertyInt::PlacementPosition, placement);
    }

    if let Some(wielder) = row.wielder {
        biota.set_property(PropertyInstanceId::Wielder, wielder);
    }

    if let Some(slot) = row.current_wielded_location {
        biota.set_property(PropertyInt::CurrentWieldedLocation, slot);
    }

    if let Some(l) = &row.location {
        let position = Position::from_components(
            l.obj_cell_id,
            l.origin_x,
            l.origin_y,
            l.origin_z,
            l.angles_x,
            l.angles_y,
            l.angles_z,
            l.angles_w,
            false,
        );
        biota.set_position(PositionType::Location, &position);
    }
}

// ACE: DeveloperFixCommands.HandleFixMeleeRares
/// `verify-melee-rares (fix)`: converts pre-MoA melee rares to their EoR wcids, and replaces the
/// rares and rare coins handed out wrongly with new random melee rares.
#[allow(clippy::too_many_lines)]
pub fn handle_fix_melee_rares(w: &mut World, _session: Option<SessionId>, parameters: &[String]) {
    let (fix, _) = fix_of(parameters);
    let mut found_issues = false;
    let mut deleted_rare_coins = 0;
    let mut new_rares_from_coins = 0;
    let mut deleted_post_rares = 0;
    let mut replaced_post_rares = 0;
    let mut adjusted_rares_pre = 0;
    let mut adjusted_rares_post = 0;
    let mut valid_post_rares = 0;
    let mut valid_post_rares_v1 = 0;

    if !world_database_version_ok(w) {
        return;
    }

    let (biotas, characters) = {
        let mut db = w.shard.base_database();
        (all_biotas(&mut **db), all_characters(&mut **db))
    };

    // get preMoA rares
    let pre_rares: Vec<RareRow> = biotas
        .iter()
        .filter(|b| (30310..=30344).contains(&b.weenie_class_id))
        .filter_map(|b| rare_row(b, &characters))
        .collect();
    let found_rares_pre = pre_rares.len();

    // get PostMoA rares
    let post_rares: Vec<RareRow> = biotas
        .iter()
        .filter(|b| (45436..=45470).contains(&b.weenie_class_id))
        .filter_map(|b| rare_row(b, &characters))
        .collect();
    let found_rares_post = post_rares.len();

    // get Rare Coins
    let rare_coins: Vec<RareRow> = biotas
        .iter()
        .filter(|b| b.weenie_class_id == 45493)
        .filter_map(|b| rare_row(b, &characters))
        .collect();
    let found_rare_coins = rare_coins.len();

    for rare in &post_rares {
        if rare.version.is_some_and(|v| v >= 2) {
            valid_post_rares += 1;
            continue;
        }

        if rare.biota.weenie_class_id == 45461 {
            valid_post_rares_v1 += 1;
        }

        found_issues = true;

        let mut logline = format!(
            "0x{:08X} {} ({}) is",
            rare.biota.id, rare.name, rare.biota.weenie_class_id
        );
        logline += &describe_links(&biotas, rare, true);

        if fix {
            if rare.biota.weenie_class_id == 45461 {
                // this rare requires no swap, it was always valid, so we'll update to version 2 and move on.

                let mut biota = rare.biota.clone();
                set_version_2(&mut biota);
                save_changes(&mut **w.shard.base_database(), |db| {
                    db.write_biota(&mut biota)
                })
                .unwrap_or_else(|e| panic!("{e}"));

                adjusted_rares_post += 1;

                logline += "\n\\----- Updated Version, no other changes required.";
            } else {
                // this rare was purchased from the Melee Rare Vendor. Generate a new v2 rare, copy over "link" data to effectively mutate in place "illegally" swapped rare for another randomly generated V2 rare that is also not the same as the one it started out as.

                let mut tier_rares: Vec<u32> = PRE_TO_POST_MELEE_RARE_CONVERSIONS
                    .iter()
                    .map(|(_, v)| *v)
                    .collect();

                if let Some(i) = tier_rares
                    .iter()
                    .position(|&x| x == rare.biota.weenie_class_id)
                {
                    tier_rares.remove(i);
                }

                let rng = ThreadSafeRandom::next(
                    0,
                    i32::try_from(tier_rares.len()).unwrap_or(i32::MAX) - 1,
                );

                let rare_wcid = tier_rares[usize::try_from(rng).unwrap_or(0)];

                if let Some((new_guid, new_name, new_wcid)) =
                    create_replacement(w, rare_wcid, |b| copy_links(rare, b))
                {
                    replaced_post_rares += 1;

                    let old = rare.biota.id;
                    save_changes(&mut **w.shard.base_database(), |db| {
                        if rare.shortcut {
                            move_shortcut(db, old, new_guid.full())?;
                        }
                        db.delete_biota(old)
                    })
                    .unwrap_or_else(|e| panic!("{e}"));
                    deleted_post_rares += 1;

                    logline +=
                        &format!("\n\\----- Replaced with 0x{new_guid} {new_name} ({new_wcid})");
                } else {
                    logline += &format!("\n\\----- Unable to replace with ({rare_wcid}). Rare not found in database.");
                    // Not ACE's (a fix): the line is written and the run goes
                    // on to the next item and the summary; ACE returned here, writing neither.
                }
            }
        } else if rare.biota.weenie_class_id == 45461 {
            logline += "\n\\----- Version needs updating, no other changes required.";
        } else {
            logline += "\n\\----- Rare obtained from Melee Rare Vendor, needs to be deleted and replaced with a random newly generated melee rare.";
        }

        console_write_line(&logline);
    }

    for rare in &pre_rares {
        found_issues = true;

        let mut logline = format!(
            "0x{:08X} {} ({}) is",
            rare.biota.id, rare.name, rare.biota.weenie_class_id
        );
        logline += &describe_links(&biotas, rare, true);

        let new_wcid = PRE_TO_POST_MELEE_RARE_CONVERSIONS
            .iter()
            .find(|(k, _)| *k == rare.biota.weenie_class_id)
            .map(|(_, v)| *v);
        if fix {
            // this is a preMoA rare and requires updating its WCID to a postMoA WCID, version 2 with no other changes.

            if let Some(new_wcid) = new_wcid {
                let old_wcid = rare.biota.weenie_class_id;

                let mut biota = rare.biota.clone();
                biota.weenie_class_id = new_wcid;
                set_version_2(&mut biota);

                adjusted_rares_pre += 1;

                logline += &format!("\n\\----- Updated WCID from {old_wcid} to {new_wcid} and Updated Version, no other changes required.");

                save_changes(&mut **w.shard.base_database(), |db| {
                    db.write_biota(&mut biota)
                })
                .unwrap_or_else(|e| panic!("{e}"));
            } else {
                logline += &format!(
                    "\n\\----- Unable to change WCID from {} for 0x{:08X}. Not a melee rare?",
                    rare.biota.weenie_class_id, rare.biota.id
                );
            }
        } else {
            let old_wcid = rare.biota.weenie_class_id;
            logline += &format!("\n\\----- Rare needs to change WCID from {old_wcid} to {} and have its version updated.", new_wcid.unwrap_or(0));
        }

        console_write_line(&logline);
    }

    for coin in &rare_coins {
        found_issues = true;

        let mut logline = format!(
            "0x{:08X} {} ({}) is",
            coin.biota.id, coin.name, coin.biota.weenie_class_id
        );
        // (the coin query has no Wielder join)
        logline += &describe_links(&biotas, coin, false);

        if fix {
            // this coin was distributed by Emissary of Asheron (45492) incorrectly to be used at the Melee Rare Vendor. Generate a new v2 rare, copy over "link" data to effectively mutate in place "illegally" swapped coin for another randomly generated V2 rare.

            let tier_rares: Vec<u32> = PRE_TO_POST_MELEE_RARE_CONVERSIONS
                .iter()
                .map(|(_, v)| *v)
                .collect();

            //tierRares.Remove(coin.Value.Biota.WeenieClassId);

            let rng =
                ThreadSafeRandom::next(0, i32::try_from(tier_rares.len()).unwrap_or(i32::MAX) - 1);

            let rare_wcid = tier_rares[usize::try_from(rng).unwrap_or(0)];

            let links = coin_links(coin);
            if let Some((new_guid, new_name, new_wcid)) =
                create_replacement(w, rare_wcid, |b| copy_links(&links, b))
            {
                new_rares_from_coins += 1;

                let old = coin.biota.id;
                save_changes(&mut **w.shard.base_database(), |db| {
                    if coin.shortcut {
                        move_shortcut(db, old, new_guid.full())?;
                    }
                    db.delete_biota(old)
                })
                .unwrap_or_else(|e| panic!("{e}"));
                deleted_rare_coins += 1;

                logline += &format!("\n\\----- Replaced with 0x{new_guid} {new_name} ({new_wcid})");
            } else {
                logline += &format!(
                    "\n\\----- Unable to replace with ({rare_wcid}). Rare not found in database."
                );
                // Not ACE's (a fix): the line is written and the run goes
                // on to the next item and the summary; ACE returned here, writing neither.
            }
        } else {
            logline += "\n\\----- Rare Coin obtained from Emissary of Asheron (45492), needs to be deleted and replaced with a random newly generated melee rare.";
        }

        console_write_line(&logline);
    }

    if !fix && found_issues {
        console_write_line(&format!("Found {} Pre-MoA Rares. These need to be converted to Post-MoA WCIDs and their version updated to V2.", format(found_rares_pre, "N0")));
        console_write_line(&format!(
            "Found {} Post-MoA Rares. {} are invalid and need to be regenerated due to incorrectly being distributed by Melee Rare Vendor. {} need to have their version updated to V2.",
            format(found_rares_post, "N0"),
            format(found_rares_post - valid_post_rares - valid_post_rares_v1, "N0"),
            format(valid_post_rares_v1, "N0")
        ));
        console_write_line(&format!("Found {} Rare Coins. These need to be deleted and replaced with newly randomly generated rare.", format(found_rare_coins, "N0")));
        console_write_line("Dry run completed. Type 'verify-melee-rares fix' to fix any issues.");
    }

    if !found_issues {
        console_write_line(&format!(
            "Verified {} melee rares. No changes required.",
            format(found_rares_pre + found_rares_post, "N0")
        ));
    }

    if found_issues && fix {
        console_write_line(&format!(
            "Found {} Pre-MoA Rares. {} were converted to Post-MoA WCIDs and their version updated to V2.",
            format(found_rares_pre, "N0"),
            format(adjusted_rares_pre, "N0")
        ));
        console_write_line(&format!(
            "Found {} Post-MoA Rares. {} valid, {} deleted, {} replaced, {} updated to V2.",
            format(found_rares_post, "N0"),
            format(valid_post_rares, "N0"),
            format(deleted_post_rares, "N0"),
            format(replaced_post_rares, "N0"),
            format(adjusted_rares_post, "N0")
        ));
        console_write_line(&format!(
            "Found {} Rare Coins. {} deleted, {} replaced.",
            format(found_rare_coins, "N0"),
            format(deleted_rare_coins, "N0"),
            format(new_rares_from_coins, "N0")
        ));
    }
}

/// A coin's link data (the coin query reads no wielder).
fn coin_links(coin: &RareRow) -> RareRow {
    RareRow {
        biota: coin.biota.clone(),
        name: coin.name.clone(),
        version: coin.version,
        container: coin.container,
        placement_position: coin.placement_position,
        wielder: None,
        current_wielded_location: None,
        location: coin.location.clone(),
        shortcut: coin.shortcut,
    }
}

// ACE: DeveloperFixCommands.HandleEnchantments
/// `verify-beneficial-enchantments (fix)`: enchantments of beneficial spells missing the
/// Beneficial stat mod flag.
pub fn handle_enchantments(w: &mut World, _session: Option<SessionId>, parameters: &[String]) {
    let (fix, fix_str) = fix_of(parameters);
    let mut found_issues = false;

    let mut biotas = all_biotas(&mut **w.shard.base_database());

    let mut num_missing_beneficial_flag = 0;
    let mut num_valid = 0;
    let mut changed: HashSet<u32> = HashSet::new();

    for biota in &mut biotas {
        for enchantment in &mut biota.biota_properties_enchantment_registry {
            if enchantment.stat_mod_type == 0 {
                //numValid++;
                continue;
            }

            let spell = Spell::new(w, enchantment.spell_id.cast_unsigned(), true);

            // (`spell != null` always holds)
            let mut stat_mod_type = EnchantmentTypeFlags(enchantment.stat_mod_type.cast_signed());

            if spell.is_beneficial()
                && (stat_mod_type & EnchantmentTypeFlags::Beneficial)
                    != EnchantmentTypeFlags::Beneficial
            {
                found_issues = true;

                num_missing_beneficial_flag += 1;

                if fix {
                    stat_mod_type |= EnchantmentTypeFlags::Beneficial;
                    enchantment.stat_mod_type = stat_mod_type.0.cast_unsigned();
                    changed.insert(enchantment.object_id);
                }

                console_write_line(&format!(
                    "Spell {} ({}) on 0x{:08X} is missing Beneficial flag{fix_str}",
                    spell.name(),
                    spell.id(),
                    enchantment.object_id
                ));
            } else {
                num_valid += 1;
            }
        }
    }
    let _ = num_valid;

    if !fix && found_issues {
        console_write_line(&format!(
            "Dry run completed. Type 'verify-beneficial-enchantments fix' to fix {} issues.",
            format(num_missing_beneficial_flag, "N0")
        ));
    }

    if fix {
        write_biotas(w, biotas.iter().filter(|b| changed.contains(&b.id)));
        console_write_line(&format!(
            "Fixed {} incorrect enchantments",
            format(num_missing_beneficial_flag, "N0")
        ));
    }

    if !found_issues {
        let count: usize = biotas
            .iter()
            .map(|b| b.biota_properties_enchantment_registry.len())
            .sum();
        console_write_line(&format!("Verified {} enchantments", format(count, "N0")));
    }
}
