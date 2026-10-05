// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Command/Handlers/SentinelCommands.cs
//! Port of `Source/ACE.Server/Command/Handlers/SentinelCommands.cs`.
//!
//! Callees that are not ported yet are private pointer functions at the bottom of this file, each
//! a `not_ported!` named after its ACE member.

use empyrean_common::dotnet::to_string;
use empyrean_entity::enums::{AccessLevel, ChatMessageType, CloakStatus, PropertyInt, SpellId};
use empyrean_entity::ObjectGuid;
use empyrean_net::enums::SessionTerminationReason;
use empyrean_net::SessionId;
use empyrean_world::entity::spell::Spell;
use empyrean_world::managers::player_manager;
use empyrean_world::network::game_event::events::game_event_magic_update_enchantment::game_event_magic_update_enchantment;
use empyrean_world::network::game_event::game_event_message::session_data;
use empyrean_world::network::game_messages::game_message::enqueue_send;
use empyrean_world::network::game_messages::messages::game_message_boot_account::game_message_boot_account;
use empyrean_world::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use empyrean_world::network::structure::enchantment::enchantment_from_registry;
use empyrean_world::world_objects::managers::{
    enchantment_manager, enchantment_manager_with_caching,
};
use empyrean_world::World;

use crate::command_handler_attribute::CommandHandlerAttribute;
use crate::command_handler_flag::CommandHandlerFlag;
use crate::command_handler_info::{CommandHandlerInfo, NamedHandler};
use crate::command_parameter_helpers::{
    self as cph, dotnet_parse, ACECommandParameter, ACECommandParameterType, AceParamValue,
};
use crate::handler;
use crate::handler_common::session_player;
use crate::handlers::command_handler_helper;

/// This file's `[CommandHandler]` decorations, in declaration order (the second `deaf` replaces
/// the first in the table, as in ACE).
#[must_use]
pub fn command_handlers() -> Vec<CommandHandlerInfo> {
    let s = AccessLevel::Sentinel;
    let world = CommandHandlerFlag::RequiresWorld;
    let none = CommandHandlerFlag::None;
    let rows: [(CommandHandlerAttribute, NamedHandler); 12] = [
        (CommandHandlerAttribute::with_count("cloak", s, world, 1, "Sets your cloaking state.", CLOAK_USAGE), handler!(handle_cloak)),
        (
            CommandHandlerAttribute::with_count("neversaydie", s, world, 0, "Turn immortality on or off.", "[ on | off ]\nDefaults to on."),
            handler!(handle_never_say_die),
        ),
        (CommandHandlerAttribute::with_count("portal_bypass", s, world, 0, "Toggles the ability to bypass portal restrictions.", ""), handler!(handle_portal_bypass)),
        (
            CommandHandlerAttribute::with_count(
                "fellowbuff",
                s,
                world,
                0,
                "Buffs your fellowship (or a player's fellowship) with all beneficial spells.",
                "[name]\nThis command buffs your fellowship (or the fellowship of the specified character).",
            ),
            handler!(handle_fellow_buff),
        ),
        (
            CommandHandlerAttribute::with_count(
                "buff",
                s,
                world,
                0,
                "Buffs you (or a player) with all beneficial spells.",
                "[name] [maxLevel]\nThis command buffs yourself (or the specified character).",
            ),
            handler!(handle_buff),
        ),
        (CommandHandlerAttribute::with_count("run", s, world, 0, "Temporarily boosts your run skill.", RUN_USAGE), handler!(handle_run)),
        (CommandHandlerAttribute::with_count("boot", s, none, 2, "Boots the character out of the game.", BOOT_USAGE), handler!(handle_boot)),
        (CommandHandlerAttribute::with_count("ban", s, none, 4, "Bans the specified player account.", BAN_USAGE), handler!(handle_ban_account)),
        (CommandHandlerAttribute::with_count("unban", s, none, 1, "Unbans the specified player account.", UNBAN_USAGE), handler!(handle_un_ban_account)),
        (CommandHandlerAttribute::with_count("banlist", s, none, 0, "Lists all banned accounts on this world.", ""), handler!(handle_banlist)),
        (CommandHandlerAttribute::with_count("deaf", s, world, 1, "", ""), handler!(handle_deaf)),
        (CommandHandlerAttribute::with_count("deaf", s, world, 2, "", ""), handler!(handle_deaf_hear_or_mute)),
    ];
    rows.into_iter()
        .map(|(attribute, (handler, handler_name))| CommandHandlerInfo {
            handler,
            handler_name,
            attribute,
        })
        .collect()
}

const CLOAK_USAGE: &str = concat!(
    "< on / off / player / creature >\n",
    "This command sets your current cloaking state\n",
    "< on > You will be completely invisible to players.\n",
    "< off > You will show up as a normal.\n",
    "< player > You will appear as a player. (No + and a white radar dot.)\n",
    "< creature > You will appear as a creature. (No + and an orange radar dot.)",
);

const RUN_USAGE: &str = concat!(
    "( on | off | toggle | check )\n",
    "Boosts the run skill of the PSR so they can pursue the \"bad folks\". The enchantment will wear off after a while. This command defaults to toggle.",
);

const BOOT_USAGE: &str = concat!(
    "[account | char | iid] who (, reason) \n",
    "This command boots the specified character out of the game. You can specify who to boot by account, character name, or player instance id. 'who' is the account / character / instance id to actually boot. You can optionally include a reason for the boot.\n",
    "Example: @boot char Character Name\n",
    "         @boot account AccountName\n",
    "         @boot iid 0x51234567\n",
    "         @boot char Character Name, Reason for being booted\n",
);

const BAN_USAGE: &str = concat!(
    "[accountname] [days] [hours] [minutes] (reason)\n",
    "This command bans the specified player account for the specified time. This player will not be able to enter the game with any character until the time expires.\n",
    "Example: @ban AccountName 0 0 5\n",
    "Example: @ban AccountName 1 0 0 banned 1 day because reasons\n",
);

const UNBAN_USAGE: &str = concat!(
    "[accountname]\n",
    "This command removes the ban from the specified account. The player will then be able to log into the game.",
);

fn system_chat(w: &mut World, session: SessionId, message: &str) {
    enqueue_send(
        w,
        session,
        game_message_system_chat(message, ChatMessageType::Broadcast),
    );
}

fn require(session: Option<SessionId>) -> SessionId {
    session.expect("NullReferenceException: session")
}

fn session_access_level(w: &World, session: SessionId) -> AccessLevel {
    w.sessions
        .get(session)
        .map_or(AccessLevel::Player, |s| s.access_level)
}

// ACE: SentinelCommands.HandleCloak
/// `cloak < on / off / player / creature >`: sets your cloaking state.
///
/// # Panics
/// With no parameters (ACE's `IndexOutOfRangeException`; the parameter count keeps chat from it).
pub fn handle_cloak(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // Please specify if you want cloaking on or off.usage: @cloak < on / off / player / creature >
    // This command sets your current cloaking state.
    // @cloak - Sets your cloaking state.

    // TODO: investigate translucensy/visbility of other cloaked admins.
    let s = require(session);
    let player = session_player(w, s);
    let cloak_status = |w: &World| {
        w.objects
            .get(player)
            .map_or(CloakStatus::Undef, |p| p.cloak_status())
    };
    let set_cloak = |w: &mut World, v: CloakStatus| {
        if let Some(p) = w.objects.get_mut(player) {
            p.set_property(PropertyInt::CloakStatus, v.0);
        }
    };

    match parameters[0].to_lowercase().as_str() {
        "off" => {
            if cloak_status(w) == CloakStatus::Off {
                return;
            }

            player_de_cloak(w, player);

            set_cloak(w, CloakStatus::Off);

            command_handler_helper::write_output_info(
                w,
                session,
                "You are no longer cloaked, can no longer pass through doors and will appear as an admin.",
                ChatMessageType::Broadcast,
            );
        }
        "on" => {
            if cloak_status(w) == CloakStatus::On {
                return;
            }

            player_handle_cloak(w, player);

            set_cloak(w, CloakStatus::On);

            command_handler_helper::write_output_info(
                w,
                session,
                "You are now cloaked.\nYou are now ethereal and can pass through doors.",
                ChatMessageType::Broadcast,
            );
        }
        "player" => {
            if session_access_level(w, s) > AccessLevel::Envoy {
                if cloak_status(w) == CloakStatus::Player {
                    return;
                }

                set_cloak(w, CloakStatus::Player);

                player_de_cloak(w, player);
                command_handler_helper::write_output_info(
                    w,
                    session,
                    "You will now appear as a player.",
                    ChatMessageType::Broadcast,
                );
            } else {
                command_handler_helper::write_output_info(
                    w,
                    session,
                    "You do not have permission to do that state",
                    ChatMessageType::Broadcast,
                );
            }
        }
        "creature" => {
            if session_access_level(w, s) > AccessLevel::Envoy {
                if cloak_status(w) == CloakStatus::Creature {
                    return;
                }

                set_cloak(w, CloakStatus::Creature);
                if let Some(p) = w.objects.get_mut(player) {
                    p.set_attackable(true);
                }

                player_de_cloak(w, player);
                command_handler_helper::write_output_info(
                    w,
                    session,
                    "You will now appear as a creature.\nUse @pk free to be allowed to attack all living things.",
                    ChatMessageType::Broadcast,
                );
            } else {
                command_handler_helper::write_output_info(
                    w,
                    session,
                    "You do not have permission to do that state",
                    ChatMessageType::Broadcast,
                );
            }
        }
        _ => system_chat(w, s, "Please specify if you want cloaking on or off."),
    }
}

// ACE: SentinelCommands.HandleNeverSayDie
/// `neversaydie [on/off]`: turn immortality on or off. Defaults to on.
pub fn handle_never_say_die(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let param = if parameters.is_empty() {
        "on"
    } else {
        parameters[0].as_str()
    };
    let player = session_player(w, s);

    if param == "off" {
        if let Some(p) = w.objects.get_mut(player) {
            p.set_invincible(false);
        }
        system_chat(w, s, "You are once again mortal.");
    } else {
        if let Some(p) = w.objects.get_mut(player) {
            p.set_invincible(true);
        }
        system_chat(w, s, "You are now immortal.");
    }
}

// ACE: SentinelCommands.HandlePortalBypass
/// `portal_bypass`: toggles the ability to bypass portal restrictions.
pub fn handle_portal_bypass(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let s = require(session);
    let player = session_player(w, s);
    let param = w
        .objects
        .get(player)
        .is_some_and(|p| p.ignore_portal_restrictions());

    if param {
        if let Some(p) = w.objects.get_mut(player) {
            p.set_ignore_portal_restrictions(false);
        }
        system_chat(w, s, "You are once again bound by portal restrictions.");
    } else {
        if let Some(p) = w.objects.get_mut(player) {
            p.set_ignore_portal_restrictions(true);
        }
        system_chat(w, s, "You are no longer bound by portal restrictions.");
    }
}

// ACE: SentinelCommands.HandleFellowBuff
/// `fellowbuff [name]`: buffs your fellowship (or a player's fellowship) with all beneficial
/// spells.
pub fn handle_fellow_buff(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let me = session_player(w, s);
    let mut ace_params = vec![ACECommandParameter {
        r#type: ACECommandParameterType::OnlinePlayerNameOrIid,
        required: false,
        default_value: Some(AceParamValue::Player(me)),
        ..ACECommandParameter::default()
    }];
    if !cph::resolve_ace_parameters(w, session, parameters, &mut ace_params, false) {
        return;
    }
    let target = ace_params[0]
        .as_player()
        .expect("NullReferenceException: AsPlayer");
    if !player_has_fellowship(w, target) {
        player_create_sentinel_buff_players(w, me, &[target], target == me, 8);
        return;
    }

    let fellowship_members = fellowship_get_fellowship_members(w, target);

    let self_only = fellowship_members.len() == 1 && fellowship_leader_guid(w, target) == me.full();
    player_create_sentinel_buff_players(w, me, &fellowship_members, self_only, 8);
}

// ACE: SentinelCommands.HandleBuff
/// `buff [name] [maxLevel]`: buffs you (or a player) with all beneficial spells.
pub fn handle_buff(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let me = session_player(w, s);
    let mut ace_params = vec![
        ACECommandParameter {
            r#type: ACECommandParameterType::OnlinePlayerNameOrIid,
            required: false,
            default_value: Some(AceParamValue::Player(me)),
            ..ACECommandParameter::default()
        },
        ACECommandParameter {
            r#type: ACECommandParameterType::ULong,
            required: false,
            default_value: Some(AceParamValue::ULong(8)),
            ..ACECommandParameter::default()
        },
    ];
    if !cph::resolve_ace_parameters(w, session, parameters, &mut ace_params, false) {
        return;
    }
    let target = ace_params[0]
        .as_player()
        .expect("NullReferenceException: AsPlayer");
    player_create_sentinel_buff_players(w, me, &[target], target == me, ace_params[1].as_ulong());
}

// ACE: SentinelCommands.HandleRun
/// `run < on | off | toggle | check >`: temporarily boosts your run skill. Boosts the run skill of
/// the PSR so they can pursue the "bad folks". The enchantment will wear off after a while. This
/// command defaults to toggle.
pub fn handle_run(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let s = require(session);
    let player = session_player(w, s);
    let param = if parameters.is_empty() {
        "toggle"
    } else {
        parameters[0].as_str()
    };

    let spell_id = SpellId::SentinelRun.0;

    let mut case = param;
    if case == "toggle" {
        case = if enchantment_manager::has_spell(w, player, spell_id) {
            "off"
        } else {
            "on"
        };
    }
    match case {
        "check" => {
            let active = if enchantment_manager::has_spell(w, player, spell_id) {
                "ACTIVE"
            } else {
                "INACTIVE"
            };
            system_chat(w, s, &format!("Run speed boost is currently {active}"));
        }
        "off" => {
            let run_boost =
                enchantment_manager::get_enchantment(w, player, spell_id, None).cloned();
            if let Some(run_boost) = run_boost {
                enchantment_manager_with_caching::remove(w, player, Some(&run_boost), true);
            } else {
                system_chat(w, s, "Run speed boost is currently INACTIVE");
            }
        }
        "on" => {
            let spell = Spell::new(w, spell_id, true);
            let add_result = enchantment_manager_with_caching::add(
                w,
                player,
                &spell,
                Some(player),
                None,
                false,
                false,
            );
            let entry = add_result
                .enchantment
                .expect("NullReferenceException: addResult.Enchantment");
            let enchantment = enchantment_from_registry(w, player, &entry);
            let msg = game_event_magic_update_enchantment(session_data(w, s), &enchantment);
            enqueue_send(w, s, msg);
            system_chat(w, s, "Run forrest, run!");
            player_handle_spell_hooks(w, player, &spell);
        }
        _ => {}
    }
}

// ACE: SentinelCommands.HandleBoot
/// `boot { account | char | iid } who (, reason)`: boots the character out of the game.
pub fn handle_boot(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // usage: @boot { account,char, iid} who
    // This command boots the specified character out of the game.You can specify who to boot by account, character name, or player instance id.  'who' is the account / character / instance id to actually boot.
    // @boot - Boots the character out of the game.

    let mut whom_to_boot = parameters[1].clone();
    let mut specified_reason: Option<String> = None;

    if parameters.len() > 1 {
        let mut parameters_after_boot_type = String::new();
        for p in &parameters[1..] {
            parameters_after_boot_type += p;
            parameters_after_boot_type += " ";
        }
        let parameters_after_boot_type = parameters_after_boot_type.trim().to_owned();
        let complete_boot_name_plus_comma_seperated_reason: Vec<&str> =
            parameters_after_boot_type.split(',').collect();
        whom_to_boot = complete_boot_name_plus_comma_seperated_reason[0]
            .trim()
            .to_owned();
        if complete_boot_name_plus_comma_seperated_reason.len() > 1 {
            specified_reason = Some(
                parameters_after_boot_type
                    .replace(&format!("{whom_to_boot},"), "")
                    .trim()
                    .to_owned(),
            );
        }
    }

    let iid_message = format!(
        "That is not a valid Instance ID (IID). IIDs must be between 0x{:08X} and 0x{:08X}",
        ObjectGuid::PLAYER_MIN,
        ObjectGuid::PLAYER_MAX
    );
    let what_to_boot;
    let session_to_boot: Option<SessionId>;
    match parameters[0].to_lowercase().as_str() {
        "char" => {
            what_to_boot = "character";
            session_to_boot = player_manager::get_online_player_by_name(w, &whom_to_boot)
                .and_then(|p| player_manager::player_session(w, p));
        }
        "account" => {
            what_to_boot = "account";
            session_to_boot = w.net.find_by_account(&whom_to_boot);
        }
        "iid" => {
            what_to_boot = "instance id";
            if !whom_to_boot.to_lowercase().starts_with("0x") {
                command_handler_helper::write_output_info(
                    w,
                    session,
                    &iid_message,
                    ChatMessageType::Broadcast,
                );
                return;
            }
            if let Some(iid) = dotnet_parse::uint_try_parse_hex(&whom_to_boot[2..]) {
                session_to_boot = player_manager::get_online_player(w, iid)
                    .and_then(|p| player_manager::player_session(w, p));
            } else {
                command_handler_helper::write_output_info(
                    w,
                    session,
                    &iid_message,
                    ChatMessageType::Broadcast,
                );
                return;
            }
        }
        _ => {
            command_handler_helper::write_output_info(
                w,
                session,
                "You must specify what you are booting with char, account, or iid as the first parameter.",
                ChatMessageType::Broadcast,
            );
            return;
        }
    }

    let Some(session_to_boot) = session_to_boot else {
        command_handler_helper::write_output_info(
            w,
            session,
            &format!("Cannot boot \"{whom_to_boot}\" because that {what_to_boot} is not currently online or cannot be found. Check syntax/spelling and try again."),
            ChatMessageType::Broadcast,
        );
        return;
    };

    // Boot the player
    let reason_suffix = specified_reason
        .as_ref()
        .map(|r| format!(" Reason: {r}"))
        .unwrap_or_default();
    let boot_text = format!("Booting {what_to_boot} {whom_to_boot}.{reason_suffix}");
    command_handler_helper::write_output_info(w, session, &boot_text, ChatMessageType::Broadcast);
    let boot_reason = specified_reason
        .as_ref()
        .map(|r| format!(" - {r}"))
        .unwrap_or_default();
    let msg = game_message_boot_account(Some(&boot_reason));
    let now = w.now;
    w.net.terminate(
        session_to_boot,
        SessionTerminationReason::AccountBooted,
        Some(msg.into_outbound()),
        specified_reason.unwrap_or_default(),
        now,
    );
    //CommandHandlerHelper.WriteOutputInfo(session, $"...Result: Success!", ChatMessageType.Broadcast);

    let issuer = session.and_then(|s| w.sessions.player(s));
    player_manager::broadcast_to_audit_channel(w, issuer, &boot_text);
}

// ACE: SentinelCommands.HandleBanAccount
/// `ban < acct > < days > < hours > < minutes > (reason)`: bans the specified player account.
pub fn handle_ban_account(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // usage: @ban < acct > < days > < hours > < minutes >
    // This command bans the specified player account for the specified time.This player will not be able to enter the game with any character until the time expires.
    // @ban - Bans the specified player account.

    let account_name = parameters[0].clone();
    let ban_days = &parameters[1];
    let ban_hours = &parameters[2];
    let ban_minutes = &parameters[3];

    let mut ban_reason = String::new();
    if parameters.len() > 4 {
        let mut parameters_after_ban_params = String::new();
        for p in &parameters[4..] {
            parameters_after_ban_params += p;
            parameters_after_ban_params += " ";
        }
        ban_reason = parameters_after_ban_params.trim().to_owned();
    }

    let account = w.auth.lock().get_account_by_name(&account_name);

    let Some(mut account) = account else {
        command_handler_helper::write_output_info(
            w,
            session,
            &format!("Cannot ban \"{account_name}\" because that account cannot be found in database. Check syntax/spelling and try again."),
            ChatMessageType::Broadcast,
        );
        return;
    };

    let Some(days) = dotnet_parse::double_try_parse(ban_days).filter(|d| d.is_nan() || *d >= 0.0)
    else {
        command_handler_helper::write_output_info(
            w,
            session,
            "Days must not be less than 0.",
            ChatMessageType::Broadcast,
        );
        return;
    };
    let Some(hours) = dotnet_parse::double_try_parse(ban_hours).filter(|h| h.is_nan() || *h >= 0.0)
    else {
        command_handler_helper::write_output_info(
            w,
            session,
            "Hours must not be less than 0.",
            ChatMessageType::Broadcast,
        );
        return;
    };
    let Some(minutes) =
        dotnet_parse::double_try_parse(ban_minutes).filter(|m| m.is_nan() || *m >= 0.0)
    else {
        command_handler_helper::write_output_info(
            w,
            session,
            "Minutes must not be less than 0.",
            ChatMessageType::Broadcast,
        );
        return;
    };

    let banned_on = w.now.utc;
    let ban_expires = w
        .now
        .utc
        .add_days(days)
        .add_hours(hours)
        .add_minutes(minutes);

    let mut banned_by = 0u32;
    if let Some(s) = session {
        banned_by = w.sessions.get(s).map_or(0, |d| d.account_id);
    }

    account.banned_time = Some(banned_on);
    account.ban_expire_time = Some(ban_expires);
    account.banned_by_account_id = Some(banned_by);
    if !ban_reason.trim().is_empty() {
        account.ban_reason = Some(ban_reason.clone());
    }

    w.auth.lock().update_account(&account);

    // Boot the player
    if w.net.find_by_account(&account_name).is_some() {
        let mut boot_args = vec!["account".to_owned()];
        if ban_reason.trim().is_empty() {
            boot_args.push(account_name.clone());
        } else {
            boot_args.push(format!("{account_name},"));
            boot_args.push(ban_reason.clone());
        }
        handle_boot(w, session, &boot_args);
    }

    let reason_suffix = if ban_reason.trim().is_empty() {
        String::new()
    } else {
        format!(" Reason: {ban_reason}")
    };
    let ban_text = format!(
        "Banned account {account_name} for {} days, {} hours and {} minutes.{reason_suffix}",
        to_string(days),
        to_string(hours),
        to_string(minutes)
    );
    command_handler_helper::write_output_info(w, session, &ban_text, ChatMessageType::Broadcast);
    let issuer = session.and_then(|s| w.sessions.player(s));
    player_manager::broadcast_to_audit_channel(w, issuer, &ban_text);
}

// ACE: SentinelCommands.HandleUnBanAccount
/// `unban < acct >`: unbans the specified player account.
pub fn handle_un_ban_account(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // usage: @unban acct
    // This command removes the ban from the specified account.The player will then be able to log into the game.
    // @unban - Unbans the specified player account.

    let account_name = parameters[0].clone();

    let account = w.auth.lock().get_account_by_name(&account_name);

    let Some(mut account) = account else {
        command_handler_helper::write_output_info(
            w,
            session,
            &format!("Cannot unban \"{account_name}\" because that account cannot be found in database. Check spelling and try again."),
            ChatMessageType::Broadcast,
        );
        return;
    };

    if account.ban_expire_time.is_none() {
        command_handler_helper::write_output_info(
            w,
            session,
            &format!("Cannot unban\"{account_name}\" because that account is not banned."),
            ChatMessageType::Broadcast,
        );
        return;
    }

    {
        let mut auth = w.auth.lock();
        account.un_ban(&mut **auth);
    }
    let ban_text = format!("UnBanned account {account_name}.");
    command_handler_helper::write_output_info(w, session, &ban_text, ChatMessageType::Broadcast);
    let issuer = session.and_then(|s| w.sessions.player(s));
    player_manager::broadcast_to_audit_channel(w, issuer, &ban_text);
}

// ACE: SentinelCommands.HandleBanlist
/// `banlist`: lists all banned accounts on this world.
pub fn handle_banlist(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let banned_accounts = w.auth.lock().get_listof_banned_accounts();

    if banned_accounts.is_empty() {
        command_handler_helper::write_output_info(
            w,
            session,
            "There are no accounts currently banned.",
            ChatMessageType::Broadcast,
        );
    } else {
        let mut msg = "The following accounts are banned:\n".to_owned();
        msg += "-------------------\n";
        for account in banned_accounts {
            msg += &account;
            msg += "\n";
        }

        command_handler_helper::write_output_info(w, session, &msg, ChatMessageType::Broadcast);
    }
}

// ACE: SentinelCommands.HandleDeaf
/// `deaf < on / off >`: ACE's body is empty ("TODO: output"); its registration is replaced by
/// [`handle_deaf_hear_or_mute`]'s, which has the same name.
pub fn handle_deaf(_w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    // @deaf - Block @tells except for the player you are currently helping.
    // @deaf on -Make yourself deaf to players.
    // @deaf off -You can hear players again.

    // TODO: output
}

// ACE: SentinelCommands.HandleDeafHearOrMute
/// `deaf < hear | mute > < player >`: ACE's body is empty ("TODO: output").
pub fn handle_deaf_hear_or_mute(
    _w: &mut World,
    _session: Option<SessionId>,
    _parameters: &[String],
) {
    // @deaf hear[name] -add a player to the list of players that you can hear.
    // @deaf mute[name] -remove a player from the list of players you can hear.

    // TODO: output
}

// ---------------------------------------------------------------------------------------------
// Pointers to members that are not ported yet (their owners swap these for the real calls).
// ---------------------------------------------------------------------------------------------

/// `session.Player.DeCloak()`.
fn player_de_cloak(w: &mut World, player: ObjectGuid) {
    empyrean_world::world_objects::player_tracking::de_cloak(w, player);
}

/// `session.Player.HandleCloak()`.
fn player_handle_cloak(w: &mut World, player: ObjectGuid) {
    empyrean_world::world_objects::player_tracking::handle_cloak(w, player);
}

/// `player.Fellowship != null`.
fn player_has_fellowship(w: &World, player: ObjectGuid) -> bool {
    empyrean_world::world_objects::player_fellowship::fellowship(w, player).is_some()
}

/// `player.Fellowship.GetFellowshipMembers().Values`.
fn fellowship_get_fellowship_members(w: &mut World, player: ObjectGuid) -> Vec<ObjectGuid> {
    let fellowship = empyrean_world::world_objects::player_fellowship::fellowship(w, player)
        .expect("ACE: Fellowship is null (NullReferenceException)");
    empyrean_world::entity::fellowship::get_fellowship_members(w, &fellowship)
        .values()
        .copied()
        .collect()
}

/// `player.Fellowship.FellowshipLeaderGuid`.
fn fellowship_leader_guid(w: &World, player: ObjectGuid) -> u32 {
    empyrean_world::world_objects::player_fellowship::fellowship(w, player)
        .expect("ACE: Fellowship is null (NullReferenceException)")
        .leader_guid(w)
}

/// `session.Player.CreateSentinelBuffPlayers(players, self, maxLevel)`.
fn player_create_sentinel_buff_players(
    w: &mut World,
    player: ObjectGuid,
    players: &[ObjectGuid],
    self_: bool,
    max_level: u64,
) {
    empyrean_world::world_objects::player_spells::create_sentinel_buff_players(
        w, player, players, self_, max_level,
    );
}

/// `session.Player.HandleSpellHooks(spell)`.
fn player_handle_spell_hooks(w: &mut World, player: ObjectGuid, spell: &Spell) {
    empyrean_world::world_objects::player_spells::handle_spell_hooks(w, player, spell);
}
