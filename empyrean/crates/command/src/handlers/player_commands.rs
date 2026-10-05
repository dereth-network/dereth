// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Command/Handlers/PlayerCommands.cs
//! Port of `Source/ACE.Server/Command/Handlers/PlayerCommands.cs`.
//!
//! Callees that are not ported yet are private pointer functions at the bottom of this file, each
//! a `not_ported!` named after its ACE member.

use empyrean_common::config_manager::ConfigManager;
use empyrean_common::dotnet::datetime::TimeSpan;
use empyrean_common::dotnet::{format, CsCast, DotNetDateTime};
use empyrean_entity::enums::{AccessLevel, CharacterOption, ChatMessageType, PropertyString};
use empyrean_entity::ObjectGuid;
use empyrean_net::SessionId;
use empyrean_store::models::shard::CharacterPropertiesQuestRegistry;
use empyrean_world::managers::property_manager;
use empyrean_world::network::game_event::events::game_event_communication_transient_string::game_event_communication_transient_string;
use empyrean_world::network::game_event::events::game_event_player_description::game_event_player_description;
use empyrean_world::network::game_event::game_event_message::session_data;
use empyrean_world::network::game_messages::game_message::enqueue_send;
use empyrean_world::world_objects::player_inventory::{self, SearchLocations};
use empyrean_world::world_objects::{player_networking, player_tracking};
use empyrean_world::World;

use crate::command_handler_attribute::CommandHandlerAttribute;
use crate::command_handler_flag::CommandHandlerFlag;
use crate::command_handler_info::{CommandHandlerInfo, NamedHandler};
use crate::handler;
use crate::handler_common::{session_player, system_chat};
use crate::handlers::command_handler_helper;

/// This file's `[CommandHandler]` decorations, in declaration order.
#[must_use]
pub fn command_handlers() -> Vec<CommandHandlerInfo> {
    let rows: [(CommandHandlerAttribute, NamedHandler); 10] = [
        (CommandHandlerAttribute::with_count("pop", AccessLevel::Player, CommandHandlerFlag::None, 0, "Show current world population", ""), handler!(handle_pop)),
        (CommandHandlerAttribute::with_description("myquests", AccessLevel::Player, CommandHandlerFlag::RequiresWorld, "Shows your quest log", ""), handler!(handle_quests)),
        (
            CommandHandlerAttribute::with_count(
                "house-select",
                AccessLevel::Player,
                CommandHandlerFlag::RequiresWorld,
                1,
                "For characters/accounts who currently own multiple houses, used to select which house they want to keep",
                "",
            ),
            handler!(handle_house_select),
        ),
        (
            CommandHandlerAttribute::with_description("debugcast", AccessLevel::Player, CommandHandlerFlag::RequiresWorld, "Shows debug information about the current magic casting state", ""),
            handler!(handle_debug_cast),
        ),
        (
            CommandHandlerAttribute::with_description("fixcast", AccessLevel::Player, CommandHandlerFlag::RequiresWorld, "Fixes magic casting if locked up for an extended time", ""),
            handler!(handle_fix_cast),
        ),
        (
            CommandHandlerAttribute::with_description("castmeter", AccessLevel::Player, CommandHandlerFlag::RequiresWorld, "Shows the fast casting efficiency meter", ""),
            handler!(handle_cast_meter),
        ),
        (
            CommandHandlerAttribute::with_count(
                "config",
                AccessLevel::Player,
                CommandHandlerFlag::RequiresWorld,
                1,
                "Manually sets a character option on the server.\nUse /config list to see a list of settings.",
                "<setting> <on/off>",
            ),
            handler!(handle_config),
        ),
        (
            CommandHandlerAttribute::with_description(
                "objsend",
                AccessLevel::Player,
                CommandHandlerFlag::RequiresWorld,
                "Force resend of all visible objects known to this player. Can fix rare cases of invisible object bugs. Can only be used once every 5 mins max.",
                "",
            ),
            handler!(handle_obj_send),
        ),
        (
            // DIVERGE: Empyrean's `empversion` (crate::empyrean) carries ACE's description; ACE's name is listed as the same command (brand).
            CommandHandlerAttribute::with_description("aceversion", AccessLevel::Player, CommandHandlerFlag::RequiresWorld, "Same as @empversion (ACE's name).", ""),
            handler!(handle_ac_eversion),
        ),
        (
            CommandHandlerAttribute::with_count("reportbug", AccessLevel::Player, CommandHandlerFlag::RequiresWorld, 2, "Generate a Bug Report", REPORTBUG_USAGE),
            handler!(handle_reportbug),
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

const REPORTBUG_USAGE: &str = concat!(
    "<category> <description>\n",
    "This command generates a URL for you to copy and paste into your web browser to submit for review by server operators and developers.\n",
    "Category can be the following:\n",
    "Creature\n",
    "NPC\n",
    "Item\n",
    "Quest\n",
    "Recipe\n",
    "Landblock\n",
    "Mechanic\n",
    "Code\n",
    "Other\n",
    "For the first three options, the bug report will include identifiers for what you currently have selected/targeted.\n",
    "After category, please include a brief description of the issue, which you can further detail in the report on the website.\n",
    "Examples:\n",
    "/reportbug creature Drudge Prowler is over powered\n",
    "/reportbug npc Ulgrim doesn't know what to do with Sake\n",
    "/reportbug quest I can't enter the portal to the Lost City of Frore\n",
    "/reportbug recipe I cannot combine Bundle of Arrowheads with Bundle of Arrowshafts\n",
    "/reportbug code I was killed by a Non-Player Killer\n",
);

/// `session` for a handler ACE only reaches with a session (RequiresWorld, or a dereference).
fn require(session: Option<SessionId>) -> SessionId {
    session.expect("NullReferenceException: session")
}

fn property_manager_get_bool(w: &World, key: &str) -> bool {
    property_manager::get_bool(w, key, false, true).item
}

// ACE: PlayerCommands.HandlePop
/// `pop`: Show current world population.
pub fn handle_pop(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let count = empyrean_world::managers::player_manager::get_online_count(w);
    command_handler_helper::write_output_info(
        w,
        session,
        &format!("Current world population: {}", format(count, "N0")),
        ChatMessageType::Broadcast,
    );
}

// ACE: PlayerCommands.HandleQuests
/// `myquests`: quest info (uses GDLe formatting to match plugin expectations).
pub fn handle_quests(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let session = require(session);
    if !property_manager_get_bool(w, "quest_info_enabled") {
        system_chat(
            w,
            session,
            "The command \"myquests\" is not currently enabled on this server.",
            ChatMessageType::Broadcast,
        );
        return;
    }

    let player = session_player(w, session);
    let quests = quest_manager_get_quests(w, player);

    if quests.is_empty() {
        system_chat(
            w,
            session,
            "Quest list is empty.",
            ChatMessageType::Broadcast,
        );
        return;
    }

    for player_quest in quests {
        let mut text = String::new();
        let quest_name = quest_manager_get_quest_name(&player_quest.quest_name);
        let Some(quest) = w.content.get_cached_quest(&quest_name) else {
            //Console.WriteLine($"Couldn't find quest {playerQuest.QuestName}");
            continue;
        };

        let mut min_delta = quest.min_delta;
        if quest_manager_can_scale_quest_min_delta(&quest) {
            min_delta = (f64::from(quest.min_delta)
                * property_manager::get_double(w, "quest_mindelta_rate", 0.0, true).item)
                .cs_cast();
        }

        text += &format!(
            "{} - {} solves ({})",
            player_quest.quest_name.to_lowercase(),
            player_quest.num_times_completed,
            player_quest.last_time_completed
        );
        text += &format!(
            "\"{}\" {} {}",
            quest.message.as_deref().unwrap_or(""),
            quest.max_solves,
            min_delta
        );

        system_chat(w, session, &text, ChatMessageType::Broadcast);
    }
}

// ACE: PlayerCommands.HandleHouseSelect
/// `house-select`: for characters/accounts who currently own multiple houses, used to select
/// which house they want to keep.
pub fn handle_house_select(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    handle_house_select_confirmed(w, session, false, parameters);
}

// ACE: PlayerCommands.HandleHouseSelect
/// The `(session, confirmed, parameters)` overload.
pub fn handle_house_select_confirmed(
    w: &mut World,
    session: Option<SessionId>,
    confirmed: bool,
    parameters: &[String],
) {
    let session = require(session);
    let Some(house_idx) =
        crate::command_parameter_helpers::dotnet_parse::int_try_parse(&parameters[0])
    else {
        return;
    };
    let player = session_player(w, session);
    let player_name = player_name(w, player);

    // ensure current multihouse owner
    if !player_is_multi_house_owner(w, player, false) {
        log::warn!("{player_name} tried to /house-select {house_idx}, but they are not currently a multi-house owner!");
        return;
    }

    // get house info for this index
    let multihouses = player_get_multi_houses(w, player);
    let count = i32::try_from(multihouses.len()).unwrap_or(i32::MAX);

    if house_idx < 1 || house_idx > count {
        system_chat(
            w,
            session,
            &format!("Please enter a number between 1 and {count}."),
            ChatMessageType::Broadcast,
        );
        return;
    }

    let index = usize::try_from(house_idx - 1).unwrap_or(0);
    let keep_house = multihouses[index];

    // show confirmation popup
    if !confirmed {
        let house_type = house_house_type_string(w, keep_house).to_lowercase();
        let loc = house_manager_get_coords_of_slum_lord(w, keep_house);

        let msg = format!("Are you sure you want to keep the {house_type} at\n{loc}?");
        if !confirmation_manager_enqueue_send_custom(w, player, session, parameters, &msg) {
            player_networking::send_weenie_error(
                w,
                player,
                empyrean_entity::enums::WeenieError::ConfirmationInProgress,
            );
        }
        return;
    }

    // house to keep confirmed, abandon the other houses
    let mut abandon_houses = multihouses.clone();
    abandon_houses.remove(index);

    for abandon_house in abandon_houses {
        let house = player_get_house(w, player, abandon_house);

        house_manager_handle_eviction(w, house, true);
    }

    // set player properties for house to keep
    if !house_set_owner_house_properties(w, keep_house) {
        log::error!("{player_name}.HandleHouseSelect({house_idx}) - couldn't find HouseOwner for {keep_house}");
        return;
    }

    // update house panel for current player
    let mut action_chain = empyrean_world::entity::actions::action_chain::ActionChain::new();
    action_chain.add_delay_seconds(w, 3.0); // wait for slumlord inventory biotas above to save
    action_chain.add_action(
        empyrean_world::entity::actions::i_actor::Actor::Object(player),
        move |w: &mut World| {
            empyrean_world::world_objects::player_house::handle_action_query_house(w, player);
        },
    );
    action_chain.enqueue_chain(w);
}

// ACE: PlayerCommands.HandleDebugCast
/// `debugcast`: shows debug information about the current magic casting state.
pub fn handle_debug_cast(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let session = require(session);
    let player = session_player(w, session);

    let pending_actions = physics_move_to_manager_pending_actions_count(w, player);
    let curr_anim = physics_part_array_curr_anim_id(w, player);

    let magic_state = magic_state_to_string(w, player);
    system_chat(w, session, &magic_state, ChatMessageType::Broadcast);
    let is_moving = physics_is_moving_or_animating(w, player);
    system_chat(
        w,
        session,
        &format!(
            "IsMovingOrAnimating: {}",
            if is_moving { "True" } else { "False" }
        ),
        ChatMessageType::Broadcast,
    );
    system_chat(
        w,
        session,
        &format!("PendingActions: {pending_actions}"),
        ChatMessageType::Broadcast,
    );
    let curr_anim = curr_anim.map(|id| format(id, "X8")).unwrap_or_default();
    system_chat(
        w,
        session,
        &format!("CurrAnim: {curr_anim}"),
        ChatMessageType::Broadcast,
    );
}

// ACE: PlayerCommands.HandleFixCast
/// `fixcast`: fixes magic casting if locked up for an extended time.
pub fn handle_fix_cast(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let session = require(session);
    let player = session_player(w, session);

    if magic_state_is_casting(w, player)
        && w.now.utc - magic_state_start_time(w, player) > TimeSpan::from_seconds(5.0)
    {
        let msg = game_event_communication_transient_string(
            session_data(w, session),
            "Fixed casting state",
        );
        enqueue_send(w, session, msg);
        player_send_use_done_event(w, player);
        magic_state_on_cast_done(w, player);
    }
}

// ACE: PlayerCommands.HandleCastMeter
/// `castmeter`: shows the fast casting efficiency meter.
pub fn handle_cast_meter(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let session = require(session);
    let player = session_player(w, session);
    if parameters.is_empty() {
        let v = !magic_state_cast_meter(w, player);
        magic_state_set_cast_meter(w, player, v);
    } else if parameters[0].eq_ignore_ascii_case("on") {
        magic_state_set_cast_meter(w, player, true);
    } else {
        magic_state_set_cast_meter(w, player, false);
    }
    let on = magic_state_cast_meter(w, player);
    system_chat(
        w,
        session,
        &format!(
            "Cast efficiency meter {}",
            if on { "enabled" } else { "disabled" }
        ),
        ChatMessageType::Broadcast,
    );
}

/// `PlayerCommands.configList`.
const CONFIG_LIST: [&str; 6] = [
    "Common settings:\nConfirmVolatileRareUse, MainPackPreferred, SalvageMultiple, SideBySideVitals, UseCraftSuccessDialog",
    "Interaction settings:\nAcceptLootPermits, AllowGive, AppearOffline, AutoAcceptFellowRequest, DragItemOnPlayerOpensSecureTrade, FellowshipShareLoot, FellowshipShareXP, IgnoreAllegianceRequests, IgnoreFellowshipRequests, IgnoreTradeRequests, UseDeception",
    "UI settings:\nCoordinatesOnRadar, DisableDistanceFog, DisableHouseRestrictionEffects, DisableMostWeatherEffects, FilterLanguage, LockUI, PersistentAtDay, ShowCloak, ShowHelm, ShowTooltips, SpellDuration, TimeStamp, ToggleRun, UseMouseTurning",
    "Chat settings:\nHearAllegianceChat, HearGeneralChat, HearLFGChat, HearRoleplayChat, HearSocietyChat, HearTradeChat, HearPKDeaths, StayInChatMode",
    "Combat settings:\nAdvancedCombatUI, AutoRepeatAttack, AutoTarget, LeadMissileTargets, UseChargeAttack, UseFastMissiles, ViewCombatTarget, VividTargetingIndicator",
    "Character display settings:\nDisplayAge, DisplayAllegianceLogonNotifications, DisplayChessRank, DisplayDateOfBirth, DisplayFishingSkill, DisplayNumberCharacterTitles, DisplayNumberDeaths",
];

/// `PlayerCommands.translateOptions`: mapping of GDLE -> ACE CharacterOptions (looked up ignoring case, ordinal).
const TRANSLATE_OPTIONS: [(&str, &str); 53] = [
    // Common
    ("ConfirmVolatileRareUse", "ConfirmUseOfRareGems"),
    ("MainPackPreferred", "UseMainPackAsDefaultForPickingUpItems"),
    ("SalvageMultiple", "SalvageMultipleMaterialsAtOnce"),
    ("SideBySideVitals", "SideBySideVitals"),
    ("UseCraftSuccessDialog", "UseCraftingChanceOfSuccessDialog"),
    // Interaction
    ("AcceptLootPermits", "AcceptCorpseLootingPermissions"),
    ("AllowGive", "LetOtherPlayersGiveYouItems"),
    ("AppearOffline", "AppearOffline"),
    (
        "AutoAcceptFellowRequest",
        "AutomaticallyAcceptFellowshipRequests",
    ),
    (
        "DragItemOnPlayerOpensSecureTrade",
        "DragItemToPlayerOpensTrade",
    ),
    ("FellowshipShareLoot", "ShareFellowshipLoot"),
    ("FellowshipShareXP", "ShareFellowshipExpAndLuminance"),
    ("IgnoreAllegianceRequests", "IgnoreAllegianceRequests"),
    ("IgnoreFellowshipRequests", "IgnoreFellowshipRequests"),
    ("IgnoreTradeRequests", "IgnoreAllTradeRequests"),
    ("UseDeception", "AttemptToDeceiveOtherPlayers"),
    // UI
    ("CoordinatesOnRadar", "ShowCoordinatesByTheRadar"),
    ("DisableDistanceFog", "DisableDistanceFog"),
    (
        "DisableHouseRestrictionEffects",
        "DisableHouseRestrictionEffects",
    ),
    ("DisableMostWeatherEffects", "DisableMostWeatherEffects"),
    ("FilterLanguage", "FilterLanguage"),
    ("LockUI", "LockUI"),
    ("PersistentAtDay", "AlwaysDaylightOutdoors"),
    ("ShowCloak", "ShowYourCloak"),
    ("ShowHelm", "ShowYourHelmOrHeadGear"),
    ("ShowTooltips", "Display3dTooltips"),
    ("SpellDuration", "DisplaySpellDurations"),
    ("TimeStamp", "DisplayTimestamps"),
    ("ToggleRun", "RunAsDefaultMovement"),
    ("UseMouseTurning", "UseMouseTurning"),
    // Chat
    ("HearAllegianceChat", "ListenToAllegianceChat"),
    ("HearGeneralChat", "ListenToGeneralChat"),
    ("HearLFGChat", "ListenToLFGChat"),
    ("HearRoleplayChat", "ListentoRoleplayChat"),
    ("HearSocietyChat", "ListenToSocietyChat"),
    ("HearTradeChat", "ListenToTradeChat"),
    ("HearPKDeaths", "ListenToPKDeathMessages"),
    ("StayInChatMode", "StayInChatModeAfterSendingMessage"),
    // Combat
    ("AdvancedCombatUI", "AdvancedCombatInterface"),
    ("AutoRepeatAttack", "AutoRepeatAttacks"),
    ("AutoTarget", "AutoTarget"),
    ("LeadMissileTargets", "LeadMissileTargets"),
    ("UseChargeAttack", "UseChargeAttack"),
    ("UseFastMissiles", "UseFastMissiles"),
    ("ViewCombatTarget", "KeepCombatTargetsInView"),
    ("VividTargetingIndicator", "VividTargetingIndicator"),
    // Character Display
    ("DisplayAge", "AllowOthersToSeeYourAge"),
    (
        "DisplayAllegianceLogonNotifications",
        "ShowAllegianceLogons",
    ),
    ("DisplayChessRank", "AllowOthersToSeeYourChessRank"),
    ("DisplayDateOfBirth", "AllowOthersToSeeYourDateOfBirth"),
    ("DisplayFishingSkill", "AllowOthersToSeeYourFishingSkill"),
    (
        "DisplayNumberCharacterTitles",
        "AllowOthersToSeeYourNumberOfTitles",
    ),
    ("DisplayNumberDeaths", "AllowOthersToSeeYourNumberOfDeaths"),
];

/// `translateOptions.TryGetValue(key, out var param)` (`StringComparer.OrdinalIgnoreCase`).
fn translate_option(key: &str) -> Option<&'static str> {
    TRANSLATE_OPTIONS
        .iter()
        .find(|(k, _)| empyrean_world::managers::player_manager::equals_ordinal_ignore_case(k, key))
        .map(|(_, v)| *v)
}

// ACE: PlayerCommands.HandleConfig
/// `config`: manually sets a character option on the server. Use /config list to see a list of
/// settings.
pub fn handle_config(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let session = require(session);
    if !property_manager_get_bool(w, "player_config_command") {
        system_chat(
            w,
            session,
            "The command \"config\" is not currently enabled on this server.",
            ChatMessageType::Broadcast,
        );
        return;
    }

    // /config list - show character options
    if empyrean_world::managers::player_manager::equals_ordinal_ignore_case(&parameters[0], "list")
    {
        for line in CONFIG_LIST {
            system_chat(w, session, line, ChatMessageType::Broadcast);
        }

        return;
    }

    // translate GDLE CharacterOptions for existing plugins
    let character_option = translate_option(&parameters[0]).and_then(CharacterOption::from_name);
    let Some(character_option) = character_option else {
        system_chat(
            w,
            session,
            &format!("Unknown character option: {}", parameters[0]),
            ChatMessageType::Broadcast,
        );
        return;
    };

    let player = session_player(w, session);
    let mut option = player_get_character_option(w, player, character_option);

    // modes of operation:
    // on / off / toggle

    // - if none specified, default to toggle
    let mut mode = "toggle";

    if parameters.len() > 1 {
        if parameters[1].eq_ignore_ascii_case("on") {
            mode = "on";
        } else if parameters[1].eq_ignore_ascii_case("off") {
            mode = "off";
        }
    }

    // set character option
    if mode == "on" {
        option = true;
    } else if mode == "off" {
        option = false;
    } else {
        option = !option;
    }

    player_set_character_option(w, player, character_option, option);

    system_chat(
        w,
        session,
        &format!(
            "Character option {} is now {}.",
            parameters[0],
            if option { "on" } else { "off" }
        ),
        ChatMessageType::Broadcast,
    );

    // update client
    let msg = game_event_player_description(w, session);
    enqueue_send(w, session, msg);
}

// ACE: PlayerCommands.HandleObjSend
/// `objsend`: force resend of all visible objects known to this player. Can fix rare cases of
/// invisible object bugs. Can only be used once every 5 mins max.
pub fn handle_obj_send(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    // a good repro spot for this is the first room after the door in facility hub
    // in the portal drop / staircase room, the VisibleCells do not have the room after the door
    // however, the room after the door *does* have the portal drop / staircase room in its VisibleCells (the inverse relationship is imbalanced)
    // not sure how to fix this atm, seems like it triggers a client bug..
    let session = require(session);
    let player = session_player(w, session);

    let prev = empyrean_world::world_objects::player::fields(w, player).prev_obj_send;
    if w.now.utc - prev < TimeSpan::from_minutes(5.0) {
        player_networking::send_transient_error(
            w,
            player,
            "You have used this command too recently!",
        );
        return;
    }

    let creatures_only =
        !parameters.is_empty() && parameters[0].to_lowercase().contains("creature");

    let known_objs = player_get_known_objects(w, player);

    for known_obj in known_objs {
        if creatures_only
            && !w
                .objects
                .get(known_obj)
                .is_some_and(empyrean_world::world_objects::world_object::WorldObject::is_creature)
        {
            continue;
        }

        player_remove_tracked_object(w, player, known_obj, false);
        player_tracking::track_object(w, player, known_obj, false);
    }
    empyrean_world::world_objects::player::fields_mut(w, player).prev_obj_send = w.now.utc;
}

// ACE: PlayerCommands.HandleACEversion
/// `aceversion`: show player ace server versions.
pub fn handle_ac_eversion(w: &mut World, session: Option<SessionId>, _parameters: &[String]) {
    let session = require(session);
    if !property_manager_get_bool(w, "version_info_enabled") {
        // DIVERGE: names our command (brand); `@aceversion` reaches this handler too.
        system_chat(
            w,
            session,
            "The command \"empversion\" is not currently enabled on this server.",
            ChatMessageType::Broadcast,
        );
        return;
    }

    let msg = server_build_info_get_version_info(w);

    system_chat(w, session, &msg, ChatMessageType::WorldBroadcast);
}

// ACE: PlayerCommands.HandleReportbug
#[allow(clippy::const_is_empty)] // ACE tests `st.Length > 0` on its constant "ACE"
/// `reportbug < code | content > < description >`.
///
/// Registered here as in ACE, but Empyrean's `reportbug` (`crate::empyrean::report_bug`) takes
/// its slot in the table, so the command never reaches this handler (brand).
///
/// # Panics
/// When the world database has no version row (ACE's `NullReferenceException` on
/// `databaseVersion.PatchVersion`), or its patch version is null.
pub fn handle_reportbug(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    let session = require(session);
    if !property_manager_get_bool(w, "reportbug_enabled") {
        system_chat(
            w,
            session,
            "The command \"reportbug\" is not currently enabled on this server.",
            ChatMessageType::Broadcast,
        );
        return;
    }

    let mut category = parameters[0].clone();
    let mut description = String::new();

    for p in &parameters[1..] {
        description += p;
        description += " ";
    }

    // The description is trimmed before it is base64-encoded into the URL (V365; current ACE does
    // the same).
    let description = description.trim().to_owned();

    match category.to_lowercase().as_str() {
        "creature" | "npc" | "quest" | "item" | "recipe" | "landblock" | "mechanic" | "code"
        | "other" => {}
        _ => category = "Other".to_owned(),
    }

    let sn = ConfigManager::config().server.world_name.clone();
    let player = session_player(w, session);
    let c = player_name(w, player);

    let st = "ACE";

    //var versions = ServerBuildInfo.GetVersionInfo();
    let database_version = w
        .content
        .get_version()
        .expect("NullReferenceException: DatabaseManager.World.GetVersion()");
    let sv = server_build_info_full_version();
    let pv = database_version
        .patch_version
        .clone()
        .expect("NullReferenceException: databaseVersion.PatchVersion");

    //var ct = PropertyManager.GetString("reportbug_content_type").Item;
    let mut cg = category.to_lowercase();

    let mut wcid = String::new();
    let mut g = String::new();

    if cg == "creature" || cg == "npc" || cg == "item" || cg == "item" {
        let p = w
            .objects
            .get(player)
            .expect("NullReferenceException: session.Player");
        let (health, mana, appraisal) = (
            p.health_query_target(),
            p.mana_query_target(),
            p.current_appraisal_target(),
        );
        if health.is_some() || mana.is_some() || appraisal.is_some() {
            let object_id = if let Some(t) = health {
                t
            } else if let Some(t) = mana {
                t
            } else {
                appraisal.unwrap_or(0)
            };

            //var wo = session.Player.CurrentLandblock?.GetObject(objectId);

            let wo = player_inventory::find_object(
                w,
                player,
                ObjectGuid::new(object_id),
                SearchLocations::Everywhere,
            )
            .result;

            if let Some(wo) = wo.and_then(|g| w.objects.get(g)) {
                wcid = format!("{}", wo.biota.weenie_class_id);
                g = format!("0x{}", format(wo.guid.full(), "X8"));
            }
        }
    }

    let l = w
        .objects
        .get(player)
        .and_then(|p| p.location())
        .expect("NullReferenceException: session.Player.Location")
        .to_loc_string();

    let issue = description;

    let urlbase = "https://www.accpp.net/bug?";

    let mut url = urlbase.to_owned();
    if !sn.is_empty() {
        url += &format!("sn={}", base64(&sn));
    }
    if !c.is_empty() {
        url += &format!("&c={}", base64(&c));
    }
    if !st.is_empty() {
        url += &format!("&st={}", base64(st));
    }
    if !sv.is_empty() {
        url += &format!("&sv={}", base64(&sv));
    }
    if !pv.is_empty() {
        url += &format!("&pv={}", base64(&pv));
    }
    //if (ct.Length > 0)
    //    url += $"&ct={Convert.ToBase64String(System.Text.Encoding.UTF8.GetBytes(ct))}";
    if !cg.is_empty() {
        if cg == "npc" {
            cg = cg.to_uppercase();
        } else {
            let mut chars = cg.chars();
            let first = chars
                .next()
                .map(|f| f.to_uppercase().collect::<String>())
                .unwrap_or_default();
            cg = first + chars.as_str();
        }
        url += &format!("&cg={}", base64(&cg));
    }
    if !wcid.is_empty() {
        url += &format!("&w={}", base64(&wcid));
    }
    if !g.is_empty() {
        url += &format!("&g={}", base64(&g));
    }
    if !l.is_empty() {
        url += &format!("&l={}", base64(&l));
    }
    if !issue.is_empty() {
        url += &format!("&i={}", base64(&issue));
    }

    let mut msg = "\n\n\n\n".to_owned();
    msg +=
        "Bug Report - Copy and Paste the following URL into your browser to submit a bug report\n";
    msg += "-=-\n";
    msg += &format!("{url}\n");
    msg += "-=-\n";
    msg += "\n\n\n\n";

    system_chat(w, session, &msg, ChatMessageType::AdminTell);
}

/// `Convert.ToBase64String(Encoding.UTF8.GetBytes(s))`.
fn base64(s: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18u32, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> shift) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// `player.Name`.
fn player_name(w: &World, player: ObjectGuid) -> String {
    w.objects
        .get(player)
        .and_then(|p| p.get_property(PropertyString::Name))
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------------------------
// Pointers to members that are not ported yet (their owners swap these for the real calls).
// ---------------------------------------------------------------------------------------------

/// `session.Player.QuestManager.GetQuests()`.
fn quest_manager_get_quests(
    w: &World,
    player: ObjectGuid,
) -> Vec<CharacterPropertiesQuestRegistry> {
    empyrean_world::managers::quest_manager::get_quests(
        w,
        &empyrean_world::managers::quest_manager::QuestOwner::Creature(player),
    )
}

/// `QuestManager.GetQuestName(questFormat)`.
fn quest_manager_get_quest_name(quest_format: &str) -> String {
    empyrean_world::managers::quest_manager::get_quest_name(quest_format).to_owned()
}

/// `QuestManager.CanScaleQuestMinDelta(quest)`.
fn quest_manager_can_scale_quest_min_delta(quest: &empyrean_content::models::world::Quest) -> bool {
    empyrean_world::managers::quest_manager::can_scale_quest_min_delta(quest)
}

/// `session.Player.IsMultiHouseOwner(showMsg)`.
fn player_is_multi_house_owner(w: &mut World, player: ObjectGuid, show_msg: bool) -> bool {
    empyrean_world::world_objects::player_house::is_multi_house_owner(w, player, show_msg)
}

/// `session.Player.GetMultiHouses()`: the houses, by guid.
fn player_get_multi_houses(w: &mut World, player: ObjectGuid) -> Vec<ObjectGuid> {
    empyrean_world::world_objects::player_house::get_multi_houses(w, player)
}

/// `$"{keepHouse.HouseType}"`.
fn house_house_type_string(w: &World, house: ObjectGuid) -> String {
    w.objects
        .get(house)
        .expect("NullReferenceException: keepHouse")
        .house_type()
        .to_dotnet_string()
}

/// `HouseManager.GetCoords(keepHouse.SlumLord.Location)`.
fn house_manager_get_coords_of_slum_lord(w: &World, house: ObjectGuid) -> String {
    let slum_lord = empyrean_world::world_objects::house::slum_lord(w, house)
        .expect("NullReferenceException: keepHouse.SlumLord");
    let location = w
        .objects
        .get(slum_lord)
        .and_then(|o| o.location())
        .expect("NullReferenceException: SlumLord.Location");
    empyrean_world::managers::house_manager::get_coords(&location)
}

/// `session.Player.ConfirmationManager.EnqueueSend(new Confirmation_Custom(session.Player.Guid,
/// () => HandleHouseSelect(session, true, parameters)), msg)`.
fn confirmation_manager_enqueue_send_custom(
    w: &mut World,
    player: ObjectGuid,
    session: SessionId,
    parameters: &[String],
    msg: &str,
) -> bool {
    let parameters = parameters.to_vec();
    let action: empyrean_world::entity::confirmation::CustomAction =
        Box::new(move |w: &mut World| {
            handle_house_select_confirmed(w, Some(session), true, &parameters)
        });
    let confirmation = empyrean_world::entity::confirmation::Confirmation::custom(player, action);
    empyrean_world::world_objects::managers::confirmation_manager::enqueue_send(
        w,
        player,
        confirmation,
        msg,
    )
}

/// `session.Player.GetHouse(abandonHouse.Guid.Full)`.
fn player_get_house(w: &mut World, player: ObjectGuid, house: ObjectGuid) -> ObjectGuid {
    empyrean_world::world_objects::player_house::get_house_by_instance(
        w,
        player,
        Some(house.full()),
    )
    .expect("NullReferenceException: house")
}

/// `HouseManager.HandleEviction(house, house.HouseOwner ?? 0, true)`.
fn house_manager_handle_eviction(w: &mut World, house: ObjectGuid, multihouse: bool) {
    let owner = w
        .objects
        .get(house)
        .and_then(|o| o.house_owner())
        .unwrap_or(0);
    empyrean_world::managers::house_manager::handle_eviction(w, house, owner, multihouse, false);
}

/// `PlayerManager.FindByGuid(keepHouse.HouseOwner ?? 0)`, then `player.HouseId`,
/// `player.HouseInstance` and `player.SaveBiotaToDatabase()`; false when the owner is not found.
fn house_set_owner_house_properties(w: &mut World, keep_house: ObjectGuid) -> bool {
    use empyrean_entity::enums::{PropertyDataId, PropertyInstanceId};
    use empyrean_world::entity::i_player;

    let owner = w
        .objects
        .get(keep_house)
        .and_then(|o| o.house_owner())
        .unwrap_or(0);
    let (player, _) = empyrean_world::managers::player_manager::find_by_guid(w, owner);
    let Some(player) = player else { return false };

    let house_id = w.objects.get(keep_house).and_then(|o| o.house_id());
    match house_id {
        Some(id) => i_player::set_property(w, player, PropertyDataId::HouseId, id),
        None => i_player::remove_property(w, player, PropertyDataId::HouseId),
    }
    i_player::set_property(w, player, PropertyInstanceId::House, keep_house.full());

    i_player::save_biota_to_database(w, player, true);
    true
}

/// `physicsObj.MovementManager.MoveToManager.PendingActions.Count`.
fn physics_move_to_manager_pending_actions_count(w: &World, player: ObjectGuid) -> i32 {
    let Some(h) = w.objects.get(player).and_then(|o| o.phys) else {
        return 0;
    };
    i32::try_from(empyrean_world::physics::motion::move_to_pending_actions(
        w, h,
    ))
    .unwrap_or(i32::MAX)
}

/// `physicsObj.PartArray.Sequence.CurrAnim?.Value.Anim.ID`.
fn physics_part_array_curr_anim_id(w: &World, player: ObjectGuid) -> Option<u32> {
    let h = w.objects.get(player).and_then(|o| o.phys)?;
    empyrean_world::physics::motion::curr_anim_id(w, h)
}

/// `physicsObj.IsMovingOrAnimating`.
fn physics_is_moving_or_animating(w: &World, player: ObjectGuid) -> bool {
    let Some(h) = w.objects.get(player).and_then(|o| o.phys) else {
        return false;
    };
    empyrean_world::physics::motion::is_moving_or_animating(w, h)
}

/// `session.Player.MagicState.ToString()`.
fn magic_state_to_string(w: &World, player: ObjectGuid) -> String {
    empyrean_world::entity::magic_state::to_string(w, player)
}

/// `magicState.IsCasting`.
fn magic_state_is_casting(w: &World, player: ObjectGuid) -> bool {
    empyrean_world::world_objects::player_magic::fields(w, player)
        .magic_state
        .is_casting
}

/// `magicState.StartTime`.
fn magic_state_start_time(w: &World, player: ObjectGuid) -> DotNetDateTime {
    empyrean_world::world_objects::player_magic::fields(w, player)
        .magic_state
        .start_time
}

/// `magicState.OnCastDone()`.
fn magic_state_on_cast_done(w: &mut World, player: ObjectGuid) {
    empyrean_world::entity::magic_state::on_cast_done(w, player);
}

/// `session.Player.MagicState.CastMeter` (get).
fn magic_state_cast_meter(w: &World, player: ObjectGuid) -> bool {
    empyrean_world::world_objects::player_magic::fields(w, player)
        .magic_state
        .cast_meter
}

/// `session.Player.MagicState.CastMeter` (set).
fn magic_state_set_cast_meter(w: &mut World, player: ObjectGuid, value: bool) {
    empyrean_world::world_objects::player_magic::fields_mut(w, player)
        .magic_state
        .cast_meter = value;
}

/// `session.Player.SendUseDoneEvent()`.
fn player_send_use_done_event(w: &mut World, player: ObjectGuid) {
    empyrean_world::world_objects::player_use::send_use_done_event(
        w,
        player,
        empyrean_entity::enums::WeenieError::None,
    );
}

/// `session.Player.GetCharacterOption(option)`.
fn player_get_character_option(w: &World, player: ObjectGuid, option: CharacterOption) -> bool {
    empyrean_world::world_objects::player_character::get_character_option(w, player, option)
}

/// `session.Player.SetCharacterOption(option, value)`.
fn player_set_character_option(
    w: &mut World,
    player: ObjectGuid,
    option: CharacterOption,
    value: bool,
) {
    empyrean_world::world_objects::player_character::set_character_option(w, player, option, value);
}

/// `session.Player.GetKnownObjects()`.
fn player_get_known_objects(w: &World, player: ObjectGuid) -> Vec<ObjectGuid> {
    empyrean_world::world_objects::player_tracking::get_known_objects(w, player)
}

/// `session.Player.RemoveTrackedObject(wo, fromPickup)`.
fn player_remove_tracked_object(
    w: &mut World,
    player: ObjectGuid,
    wo: ObjectGuid,
    from_pickup: bool,
) {
    empyrean_world::world_objects::player_tracking::remove_tracked_object(
        w,
        player,
        wo,
        from_pickup,
    );
}

/// `ServerBuildInfo.GetVersionInfo()`.
pub(crate) fn server_build_info_get_version_info(w: &World) -> String {
    let v = w
        .content
        .get_version()
        .expect("ACE: DatabaseManager.World.GetVersion() is null (NullReferenceException)");
    let msg = empyrean_common::server_build_info::get_version_info(
        v.base_version.as_deref(),
        v.patch_version.as_deref(),
        v.last_modified,
    );
    // Not ACE: the pack's content hash and the corrections digest, which together name the world data served.
    msg + &empyrean_content::corrections::world_data_line(w.content.content_hash().as_deref())
}

/// `ServerBuildInfo.FullVersion`.
fn server_build_info_full_version() -> String {
    empyrean_common::server_build_info::full_version()
}
