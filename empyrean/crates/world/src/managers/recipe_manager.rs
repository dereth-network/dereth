// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Managers/RecipeManager.cs
//! Port of `Source/ACE.Server/Managers/RecipeManager.cs`.
//!
//! Crafting and tinkering: the recipe lookup (`GetRecipe`: the cook book, then
//! `RecipeManager_New`), the requirements, the success chance (`GetRecipeChance`,
//! `GetTinkerChance`), the confirmation dialog, and `HandleRecipe`: the roll, the create and
//! destroy of items, the recipe mods and the mutation scripts (4.10's `MutationCache`).
//!
//! **Objects.** ACE's `WorldObject` arguments are guids of objects in `World.objects`. A recipe can
//! destroy its source or target (`DestroyItem`) and still name it afterwards: ACE keeps using the
//! destroyed object in memory. Here a destroyed object has left the store, so the names it is
//! mentioned by later are taken just before it is destroyed ([`Tombstones`]), and the property
//! writes and `UpdateObject` sends ACE still makes to it are skipped (`DIVERGE` markers).
//!
//! Members ported in other files are reached through [`shims`], by ACE's names.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use empyrean_common::dotnet::{format, CsCast, DotNetDict, DotNetHashSet};
use empyrean_common::extensions::float_extensions;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_content::models::world::{
    Recipe, RecipeModsBool, RecipeModsDID, RecipeModsFloat, RecipeModsIID, RecipeModsInt,
    RecipeModsString,
};
use empyrean_entity::enums::{
    CharacterOption, ChatMessageType, CombatMode, CompareType, DamageType, ImbuedEffectType,
    MaterialType, ModificationOperation, ModificationType, MotionCommand, PropertyAttribute2nd,
    PropertyBool, PropertyDataId, PropertyFloat, PropertyInstanceId, PropertyInt, PropertyString,
    RecipeSourceType, RequirementType, Skill, SkillAdvancementClass, SpellId, UiEffects, Usable,
    WeenieClassName, WeenieError,
};
use empyrean_entity::ObjectGuid;
use empyrean_net::SessionId;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::confirmation::Confirmation;
use crate::managers::player_manager::player_session;
use crate::managers::property_manager;
use crate::managers::recipe_manager_new::get_new_recipe;
use crate::network::game_event::events::game_event_communication_transient_string::game_event_communication_transient_string;
use crate::network::game_event::events::game_event_weenie_error::game_event_weenie_error;
use crate::network::game_messages::game_message::{enqueue_send, GameMessage};
use crate::network::game_messages::messages::game_message_obj_desc_event::game_message_obj_desc_event;
use crate::network::game_messages::messages::game_message_public_update_property_int::game_message_public_update_property_int;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::network::game_messages::messages::game_message_update_object::game_message_update_object;
use crate::sessions::SessionData;
use crate::world_objects::managers::confirmation_manager;
use crate::world_objects::player_inventory::{self, DequipObjectAction, SearchLocations};
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::{
    creature_combat, player, player_character, player_combat, player_properties,
    world_object_networking,
};
use crate::{dispatch, World};

/// The mutable static state of ACE's `RecipeManager`, held as a field of `World`.
#[derive(Debug, Default)]
pub struct RecipeManagerState {
    /// `RecipeManager_New.Precursors`: tool -> target -> recipe, filled by `ReadJSON` (which
    /// nothing in ACE calls).
    // ACE: RecipeManager.Precursors
    pub precursors: Option<DotNetDict<u32, DotNetDict<u32, u32>>>,

    /// `(uint?)MaterialType ?? WeenieClassId` of each object the current `CreateDestroyItems` has
    /// destroyed: `HandleTinkerLog` reads a consumed salvage bag's material (ACE keeps the
    /// destroyed object in memory).
    pub consumed_sources: HashMap<ObjectGuid, u32>,
}

// ================================================================================ helpers

fn obj(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects.get(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

fn obj_mut(w: &mut World, g: ObjectGuid) -> &mut WorldObject {
    w.objects.get_mut(g).unwrap_or_else(|| {
        panic!(
            "System.NullReferenceException: object 0x{:08X} is not in World.objects",
            g.full()
        )
    })
}

/// `Name` (a null name interpolates as empty).
fn name(w: &World, g: ObjectGuid) -> String {
    dispatch::name::name(w, g).unwrap_or_default()
}

/// `Player.Session.Network.EnqueueSend(msg)`.
fn send(w: &mut World, player: ObjectGuid, msg: GameMessage) {
    if let Some(s) = player_session(w, player) {
        enqueue_send(w, s, msg);
    }
}

/// Builds a game event for the player's session (consuming its `GameEventSequence`).
fn event(
    w: &mut World,
    player: ObjectGuid,
    build: impl FnOnce(&mut SessionData) -> GameMessage,
) -> Option<GameMessage> {
    let s: SessionId = player_session(w, player)?;
    let data = w.sessions.get_mut(s)?;
    Some(build(data))
}

/// `player.Session.Network.EnqueueSend(new GameMessageSystemChat(message, type))`.
fn system_chat(
    w: &mut World,
    player: ObjectGuid,
    message: &str,
    chat_message_type: ChatMessageType,
) {
    send(
        w,
        player,
        game_message_system_chat(message, chat_message_type),
    );
}

/// `new GameEventCommunicationTransientString(player.Session, message)`, sent.
fn transient(w: &mut World, player: ObjectGuid, message: &str) {
    if let Some(m) = event(w, player, |d| {
        game_event_communication_transient_string(d, message)
    }) {
        send(w, player, m);
    }
}

/// The names a destroyed source or target is still mentioned by after `DestroyItem` (ACE keeps the
/// destroyed object in memory).
#[derive(Debug, Default)]
struct Tombstones(HashMap<ObjectGuid, Tombstone>);

#[derive(Debug, Clone)]
struct Tombstone {
    name_with_material: String,
    inscription: Option<String>,
    scribe_name: Option<String>,
    workmanship: Option<f32>,
}

impl Tombstones {
    /// Records `g`'s names while it is still in the store.
    fn keep(&mut self, w: &mut World, g: ObjectGuid) {
        let Some(workmanship) = w.objects.get_mut(g).map(WorldObject::workmanship) else {
            return;
        };
        let o = obj(w, g);
        let t = Tombstone {
            name_with_material: name_with_material(w, g),
            inscription: o.inscription(),
            scribe_name: o.scribe_name(),
            workmanship,
        };
        self.0.insert(g, t);
    }

    /// `obj.Workmanship`, live or as it was when destroyed.
    fn workmanship(&self, w: &mut World, g: ObjectGuid) -> Option<f32> {
        match w.objects.get_mut(g) {
            Some(o) => o.workmanship(),
            None => self.0.get(&g).and_then(|t| t.workmanship),
        }
    }

    /// `obj.NameWithMaterial`, live or as it was when destroyed.
    fn name_with_material(&self, w: &World, g: ObjectGuid) -> String {
        if w.objects.get(g).is_some() {
            return name_with_material(w, g);
        }
        self.0
            .get(&g)
            .map(|t| t.name_with_material.clone())
            .unwrap_or_default()
    }

    /// `(obj.Inscription, obj.ScribeName)`, live or as they were when destroyed.
    fn inscription(&self, w: &World, g: ObjectGuid) -> (Option<String>, Option<String>) {
        if let Some(o) = w.objects.get(g) {
            return (o.inscription(), o.scribe_name());
        }
        self.0
            .get(&g)
            .map(|t| (t.inscription.clone(), t.scribe_name.clone()))
            .unwrap_or_default()
    }
}

/// `obj.NameWithMaterial`.
fn name_with_material(w: &World, g: ObjectGuid) -> String {
    shims::get_name_with_material(w, g, None)
}

// ================================================================================ RecipeManager.cs

// ACE: RecipeManager.GetRecipe
/// PY16 recipes (the cook book), else a `RecipeManager_New` recipe.
#[must_use]
pub fn get_recipe(
    w: &World,
    player: ObjectGuid,
    source: ObjectGuid,
    target: ObjectGuid,
) -> Option<Arc<Recipe>> {
    // PY16 recipes
    let cookbook = w.content.get_cached_cookbook(
        obj(w, source).biota.weenie_class_id,
        obj(w, target).biota.weenie_class_id,
    );
    if let Some(cookbook) = cookbook {
        return cookbook.recipe.clone();
    }

    // if none exists, try finding new recipe
    get_new_recipe(w, player, source, target)
}

// ACE: RecipeManager.UseObjectOnTarget
/// Uses `source` on `target`: the checks, the chance, the clap, then the dialog or the recipe.
/// `confirmed` is ACE's default `false` unless the player has answered the dialog.
pub fn use_object_on_target(
    w: &mut World,
    player: ObjectGuid,
    source: ObjectGuid,
    target: ObjectGuid,
    confirmed: bool,
) {
    if obj(w, player).wo.world_object.is_busy {
        shims::send_use_done_event(w, player, WeenieError::YoureTooBusy);
        return;
    }

    let allow_craft_in_combat =
        property_manager::get_bool(w, "allow_combat_mode_crafting", false, true).item;

    if !allow_craft_in_combat && creature_combat::combat_mode(w, player) != CombatMode::NonCombat {
        shims::send_use_done_event(w, player, WeenieError::YouMustBeInPeaceModeToTrade);
        return;
    }

    if source == target {
        let n = name_with_material(w, source);
        system_chat(
            w,
            player,
            &format!("The {n} cannot be combined with itself."),
            ChatMessageType::Craft,
        );
        transient(w, player, &format!("You can't use the {n} on itself."));
        shims::send_use_done_event(w, player, WeenieError::None);
        return;
    }

    let recipe = get_recipe(w, player, source, target);

    let Some(recipe) = recipe else {
        let (s, t) = (name_with_material(w, source), name_with_material(w, target));
        system_chat(
            w,
            player,
            &format!("The {s} cannot be used on the {t}."),
            ChatMessageType::Craft,
        );
        shims::send_use_done_event(w, player, WeenieError::None);
        return;
    };

    // DIVERGE: a world without tinkering (`EraFeatures::tinkering`) refuses a tinkering recipe
    // (salvage applied to an item) and any recipe on a salvage bag (combining salvage) (V421).
    let salvage = |w: &World, g: ObjectGuid| {
        obj(w, g).item_type() == empyrean_entity::enums::ItemType::TinkeringMaterial
    };
    if !w.era.features.tinkering
        && (is_tinkering(&recipe) || salvage(w, source) || salvage(w, target))
    {
        crate::world_objects::era_gates::has(w, player, false, "tinkering");
        shims::send_use_done_event(w, player, WeenieError::None);
        return;
    }

    // verify requirements
    if !verify_requirements(w, &recipe, player, source, target) {
        shims::send_use_done_event(w, player, WeenieError::YouDoNotPassCraftingRequirements);
        return;
    }

    if is_tinkering(&recipe) {
        log::info!(
            "[TINKERING] {}.UseObjectOnTarget({}, {}) | Status: {}confirmed",
            name(w, player),
            name_with_material(w, source),
            name_with_material(w, target),
            if confirmed { "" } else { "un" }
        );
    }

    let percent_success = get_recipe_chance(w, player, source, target, &recipe);

    let Some(percent_success) = percent_success else {
        shims::send_use_done_event(w, player, WeenieError::None);
        return;
    };

    let show_dialog = has_difficulty(&recipe)
        && player_character::get_character_option(
            w,
            player,
            CharacterOption::UseCraftingChanceOfSuccessDialog,
        );

    let lum_aug_skilled_craft = obj(w, player).lum_aug_skilled_craft();
    if !confirmed && lum_aug_skilled_craft > 0 {
        player::send_message(w, player, &format!("Your Aura of the Craftman augmentation increased your skill by {lum_aug_skilled_craft}!"), ChatMessageType::Broadcast);
    }

    let motion_command = MotionCommand::ClapHands;

    let mut action_chain = ActionChain::new();
    let mut next_use_time = 0.0f32;

    obj_mut(w, player).wo.world_object.is_busy = true;

    if allow_craft_in_combat && creature_combat::combat_mode(w, player) != CombatMode::NonCombat {
        // Drop out of combat mode.  This depends on the server property "allow_combat_mode_craft" being True.
        // If not, this action would have aborted due to not being in NonCombat mode.
        let stance_time = creature_combat::set_combat_mode(w, player, CombatMode::NonCombat);
        action_chain.add_delay_seconds(w, f64::from(stance_time));

        next_use_time += stance_time;
    }

    // `new Motion(player, motionCommand)`: built and never used in ACE (no side effects).
    let current_stance = world_object_networking::shims::current_motion_state(obj(w, player))
        .expect("ACE: player.CurrentMotionState is null (NullReferenceException)")
        .stance; // expected to be MotionStance.NonCombat
    let clap_time = if confirmed {
        0.0
    } else {
        let motion_table_id = obj(w, player).motion_table_id();
        crate::physics::motion_table::get_animation_length(
            w,
            motion_table_id,
            current_stance,
            motion_command,
            1.0,
        )
    };

    if !confirmed {
        action_chain.add_action(Actor::Object(player), move |w| {
            crate::world_objects::player_location::send_motion_as_commands(
                w,
                player,
                motion_command,
                current_stance,
            );
        });
        action_chain.add_delay_seconds(w, f64::from(clap_time));

        next_use_time += clap_time;
    }

    if show_dialog && !confirmed {
        let r = Arc::clone(&recipe);
        action_chain.add_action(Actor::Object(player), move |w| {
            show_dialog_(w, player, source, target, &r, percent_success)
        });
        action_chain.add_action(Actor::Object(player), move |w| {
            if let Some(o) = w.objects.get_mut(player) {
                o.wo.world_object.is_busy = false;
            }
        });
    } else {
        let r = Arc::clone(&recipe);
        action_chain.add_action(Actor::Object(player), move |w| {
            handle_recipe(w, player, source, target, &r, percent_success)
        });

        action_chain.add_action(Actor::Object(player), move |w| {
            if !show_dialog {
                shims::send_use_done_event(w, player, WeenieError::None);
            }

            if let Some(o) = w.objects.get_mut(player) {
                o.wo.world_object.is_busy = false;
            }
        });
    }

    action_chain.enqueue_chain(w);

    let t = w.now.utc.add_seconds(f64::from(next_use_time));
    player_combat::set_next_use_time(w, player, t);
}

// ACE: RecipeManager.HasDifficulty
#[must_use]
pub fn has_difficulty(recipe: &Recipe) -> bool {
    if is_tinkering(recipe) {
        return true;
    }

    recipe.skill > 0 && recipe.difficulty > 0
}

// ACE: RecipeManager.GetRecipeChance
/// The chance of success, or `None` when the recipe cannot be attempted (an untrained skill).
pub fn get_recipe_chance(
    w: &mut World,
    player: ObjectGuid,
    source: ObjectGuid,
    target: ObjectGuid,
    recipe: &Recipe,
) -> Option<f64> {
    if is_tinkering(recipe) {
        return get_tinker_chance(w, player, source, target, recipe);
    }

    if !has_difficulty(recipe) {
        return Some(1.0);
    }

    let player_skill = obj_mut(w, player).get_creature_skill(Skill(recipe.skill.cs_cast()), true);

    let Some(player_skill) = player_skill else {
        // this shouldn't happen, but sanity check for unexpected nulls
        log::warn!(
            "RecipeManager.GetRecipeChance({}, {}, {}): recipe {} missing skill",
            name(w, player),
            name(w, source),
            name(w, target),
            recipe.id
        );
        return None;
    };

    // check for pre-MoA skill
    // convert into appropriate post-MoA skill
    // pre-MoA melee weapons: get highest melee weapons skill
    let new_skill =
        crate::world_objects::world_object::convert_to_mo_a_skill(w, player, player_skill.skill);

    let player_skill = obj_mut(w, player)
        .get_creature_skill(new_skill, true)
        .expect("ACE: GetCreatureSkill(skill, add: true)");

    //Console.WriteLine("Required skill: " + skill.Skill);

    if player_skill.advancement_class(obj(w, player)) < SkillAdvancementClass::Trained {
        crate::world_objects::player_networking::send_weenie_error(
            w,
            player,
            WeenieError::YouAreNotTrainedInThatTradeSkill,
        );
        return None;
    }

    //Console.WriteLine("Skill difficulty: " + recipe.Recipe.Difficulty);

    // `playerSkill.Current + (uint)player.LumAugSkilledCraft`: an unchecked uint sum.
    let lum: u32 = obj(w, player).lum_aug_skilled_craft().cs_cast();
    let player_current_plus_lum_aug_skilled_craft =
        player_skill.current(w, player).wrapping_add(lum);

    let success_chance = crate::world_objects::skill_check::get_skill_chance_uint(
        player_current_plus_lum_aug_skilled_craft,
        recipe.difficulty,
        crate::world_objects::skill_check::DEFAULT_FACTOR,
    );

    Some(success_chance)
}

// ACE: RecipeManager.GetTinkerChance
/// The tinkering chance: the difficulty from the salvage's material, both workmanships and the
/// number of previous tinkers; an imbue divides by 3 (plus 5% per imbue augmentation).
pub fn get_tinker_chance(
    w: &mut World,
    player: ObjectGuid,
    tool: ObjectGuid,
    target: ObjectGuid,
    recipe: &Recipe,
) -> Option<f64> {
    // calculate % success chance

    let tool_workmanship = obj_mut(w, tool).workmanship().unwrap_or(0.0);
    let item_workmanship = obj_mut(w, target).workmanship().unwrap_or(0.0);

    let tinkered_count = obj(w, target).num_times_tinkered();

    let material_type = obj(w, tool)
        .material_type()
        .unwrap_or(MaterialType::Unknown);
    let salvage_mod = get_material_mod(material_type);

    let mut workmanship_mod = 1.0f32;
    if tool_workmanship >= item_workmanship {
        workmanship_mod = 2.0;
    }

    let recipe_skill = Skill(recipe.skill.cs_cast());

    let skill = obj_mut(w, player)
        .get_creature_skill(recipe_skill, true)
        .expect("ACE: GetCreatureSkill(skill, add: true)");

    // tinkering skill must be trained
    if skill.advancement_class(obj(w, player)) < SkillAdvancementClass::Trained {
        let msg = format!(
            "You are not trained in {}.",
            shims::to_sentence(skill.skill)
        );
        system_chat(w, player, &msg, ChatMessageType::Broadcast);
        return None;
    }

    // thanks to Endy's Tinkering Calculator for this formula!
    // `TinkeringDifficulty[tinkeredCount]`: a negative or too-large count throws
    // (ArgumentOutOfRangeException).
    let attempt_mod = TINKERING_DIFFICULTY
        [usize::try_from(tinkered_count).expect("ACE: ArgumentOutOfRangeException")];

    let difficulty = tinker_difficulty(
        salvage_mod,
        item_workmanship,
        tool_workmanship,
        workmanship_mod,
        attempt_mod,
    );

    // `skill.Current + (uint)player.LumAugSkilledCraft`, then cast to int.
    let lum: u32 = obj(w, player).lum_aug_skilled_craft().cs_cast();
    let player_current_plus_lum_aug_skilled_craft = skill.current(w, player).wrapping_add(lum);

    let mut success_chance = crate::world_objects::skill_check::get_skill_chance(
        player_current_plus_lum_aug_skilled_craft.cs_cast(),
        difficulty,
        crate::world_objects::skill_check::DEFAULT_FACTOR,
    );

    // imbue: divide success by 3
    if is_imbuing(recipe) {
        success_chance /= 3.0;

        let augs = obj(w, player).augmentation_bonus_imbue_chance();
        if augs > 0 {
            // `int * 0.05f` is a float product, widened.
            #[allow(clippy::cast_precision_loss)] // int to float, as C# promotes it
            let bonus = augs as f32 * 0.05f32;
            success_chance += f64::from(bonus);
        }
    }

    // todo: remove this once foolproof salvage recipes are updated
    // `(WeenieClassName)tool.WeenieClassId`: the enum is a ushort, so the wcid is truncated.
    let tool_class: u16 = obj(w, tool).biota.weenie_class_id.cs_cast();
    if FOOLPROOF_TINKERS.contains(&u32::from(tool_class)) {
        success_chance = 1.0;
    }

    Some(success_chance)
}

/// `(int)Math.Floor(((salvageMod * 5.0f) + (itemWorkmanship * salvageMod * 2.0f) - (toolWorkmanship
/// * workmanshipMod * salvageMod / 5.0f)) * attemptMod)`: `float` arithmetic, floored as `double`.
/// Part of [`get_tinker_chance`], split out for the vectors.
#[must_use]
pub fn tinker_difficulty(
    salvage_mod: f32,
    item_workmanship: f32,
    tool_workmanship: f32,
    workmanship_mod: f32,
    attempt_mod: f32,
) -> i32 {
    let inner = (salvage_mod * 5.0f32) + (item_workmanship * salvage_mod * 2.0f32)
        - (tool_workmanship * workmanship_mod * salvage_mod / 5.0f32);
    f64::from(inner * attempt_mod).floor().cs_cast()
}

// ACE: RecipeManager.GetMaterialMod
/// Returns the modifier for a bag of salvaging material
#[must_use]
pub fn get_material_mod(material: MaterialType) -> f32 {
    match material {
        MaterialType::Gold | MaterialType::Oak => 10.0,

        MaterialType::Alabaster
        | MaterialType::ArmoredilloHide
        | MaterialType::Brass
        | MaterialType::Bronze
        | MaterialType::Ceramic
        | MaterialType::Granite
        | MaterialType::Linen
        | MaterialType::Marble
        | MaterialType::Moonstone
        | MaterialType::Opal
        | MaterialType::Pine
        | MaterialType::ReedSharkHide
        | MaterialType::Velvet
        | MaterialType::Wool => 11.0,

        MaterialType::Ebony
        | MaterialType::GreenGarnet
        | MaterialType::Iron
        | MaterialType::Mahogany
        | MaterialType::Porcelain
        | MaterialType::Satin
        | MaterialType::Steel
        | MaterialType::Teak => 12.0,

        MaterialType::Bloodstone
        | MaterialType::Carnelian
        | MaterialType::Citrine
        | MaterialType::Hematite
        | MaterialType::LavenderJade
        | MaterialType::Malachite
        | MaterialType::RedJade
        | MaterialType::RoseQuartz => 25.0,

        _ => 20.0,
    }
}

// ACE: RecipeManager.TinkeringDifficulty
/// Thanks to Endy's Tinkering Calculator for these values! (A `public static List<float>` in ACE
/// that nothing modifies.)
pub const TINKERING_DIFFICULTY: [f32; 10] = [
    // attempt #
    1.0, // 1
    1.1, // 2
    1.3, // 3
    1.6, // 4
    2.0, // 5
    2.5, // 6
    3.0, // 7
    3.5, // 8
    4.0, // 9
    4.5, // 10
];

// ACE: RecipeManager.ShowDialog
/// The chance-of-success dialog (a craft confirmation).
pub fn show_dialog_(
    w: &mut World,
    player: ObjectGuid,
    source: ObjectGuid,
    target: ObjectGuid,
    recipe: &Recipe,
    success_chance: f64,
) {
    let percent = success_chance * 100.0;

    // retail messages:

    // You determine that you have a 100 percent chance to succeed.
    // You determine that you have a 99 percent chance to succeed.
    // You determine that you have a 38 percent chance to succeed. 5 percent is due to your augmentation.

    let mut floor_msg = format!(
        "You determine that you have a {} percent chance to succeed.",
        float_extensions::round_f64(percent, 0)
    );

    let num_augs = if is_imbuing(recipe) {
        obj(w, player).augmentation_bonus_imbue_chance()
    } else {
        0
    };

    if num_augs > 0 {
        floor_msg += &format!(
            "\n{} percent is due to your augmentation.",
            num_augs.wrapping_mul(5)
        );
    }

    if !confirmation_manager::enqueue_send(
        w,
        player,
        Confirmation::craft_interation(player, source, target),
        &floor_msg,
    ) {
        shims::send_use_done_event(w, player, WeenieError::ConfirmationInProgress);
        return;
    }

    if property_manager::get_bool(w, "craft_exact_msg", false, true).item {
        #[allow(clippy::cast_possible_truncation)] // ACE's `(float)percent`
        let exact_msg = format!(
            "You have a {} percent chance of using {} on {}.",
            format::to_string(percent as f32),
            name_with_material(w, source),
            name_with_material(w, target)
        );

        system_chat(w, player, &exact_msg, ChatMessageType::Craft);
    }
    shims::send_use_done_event(w, player, WeenieError::None);
}

// ACE: RecipeManager.HandleRecipe
/// Re-verifies, rolls, creates and destroys the items and applies the mods.
pub fn handle_recipe(
    w: &mut World,
    player: ObjectGuid,
    source: ObjectGuid,
    target: ObjectGuid,
    recipe: &Recipe,
    success_chance: f64,
) {
    // re-verify
    // (A source or target destroyed meanwhile is no longer found where the player can use it.)
    if w.objects.get(source).is_none()
        || w.objects.get(target).is_none()
        || !verify_requirements(w, recipe, player, source, target)
    {
        crate::world_objects::player_networking::send_weenie_error(
            w,
            player,
            WeenieError::YouDoNotPassCraftingRequirements,
        );
        return;
    }

    let success = ThreadSafeRandom::next_float(0.0, 1.0) < success_chance;

    if is_imbuing(recipe) {
        let p = obj_mut(w, player);
        p.set_imbue_attempts(p.imbue_attempts().wrapping_add(1));
        if success {
            p.set_imbue_successes(p.imbue_successes().wrapping_add(1));
        }
    }

    let modified = create_destroy_items(w, player, recipe, source, target, success_chance, success);

    if let Some(modified) = modified {
        if modified.contains(&source.full()) {
            update_obj(w, player, source);
        }

        if modified.contains(&target.full()) {
            update_obj(w, player, target);
        }
    }

    if success && recipe.skill > 0 && recipe.difficulty > 0 {
        let skill = obj_mut(w, player)
            .get_creature_skill(Skill(recipe.skill.cs_cast()), true)
            .expect("ACE: GetCreatureSkill(skill, add: true)");
        shims::proficiency_on_success_use(w, player, skill.skill, recipe.difficulty);
    }
}

// ACE: RecipeManager.UpdateObj
/// Sends an UpdateObj to the client for modified sources / targets
fn update_obj(w: &mut World, player: ObjectGuid, obj_guid: ObjectGuid) {
    if DEBUG {
        log::debug!("{}.UpdateObj({})", name(w, player), name(w, obj_guid));
    }

    // DIVERGE: ACE still broadcasts an UpdateObject for a source or target the recipe has just
    // destroyed (it is gone from the client already); a destroyed object has left World.objects
    // here, so nothing is sent.
    if w.objects.get(obj_guid).is_none() {
        return;
    }

    let m = game_message_update_object(w, obj_guid, false, false);
    world_object_networking::enqueue_broadcast(w, player, true, &[m]);

    if obj(w, obj_guid).current_wielded_location().is_some() {
        // retail possibly required sources / targets to be in the player's inventory,
        // and not equipped. this scenario might already be prevented beforehand in VerifyUse()
        let m = game_message_obj_desc_event(w, player);
        world_object_networking::enqueue_broadcast(w, player, true, &[m]);
        return;
    }

    // client automatically moves item to first slot in container
    // when an UpdateObject is sent. we must mimic this process on the server for persistance

    // only run this for items in the player's inventory
    // ie. skip for items on landblock, such as chorizite ore

    let inv_obj =
        player_inventory::find_object(w, player, obj_guid, SearchLocations::MyInventory).result;

    if inv_obj.is_some() {
        player_inventory::move_item_to_first_container_slot(w, player, obj_guid);
    }
}

/// `(int?)(x * c)` on a nullable int: `None` stays `None`; the float product is cast back.
fn mul_f32(v: Option<i32>, by: f32) -> Option<i32> {
    #[allow(clippy::cast_precision_loss)] // int to float, as C# promotes it
    v.map(|x| (x as f32 * by).cs_cast())
}

/// `x += c` on a nullable double with a float constant: `None` stays `None` (and the setter then
/// removes the property).
fn add_f(v: Option<f64>, c: f32) -> Option<f64> {
    v.map(|x| x + f64::from(c))
}

// ACE: RecipeManager.TryMutateNative
/// The legacy C# mutations, keyed by a recipe mod's data id (unused by default: ACE runs the
/// mutation scripts).
pub fn try_mutate_native(
    w: &mut World,
    player: ObjectGuid,
    source: ObjectGuid,
    target: ObjectGuid,
    _recipe: &Recipe,
    data_id: u32,
) -> bool {
    let t = obj_mut(w, target);
    // legacy method, unused by default
    match data_id {
        // armor tinkering
        0x3800_0011 => t.set_armor_level(t.armor_level().map(|x| x.wrapping_add(20))), // Steel

        // mutations apparently didn't cap to 2.0 here, clamps are applied in damage calculations though
        0x3800_0017 => t.set_armor_mod_vs_pierce(add_f(t.armor_mod_vs_pierce(), 0.2)), // Alabaster
        0x3800_0018 => t.set_armor_mod_vs_slash(add_f(t.armor_mod_vs_slash(), 0.2)),   // Bronze
        0x3800_0013 => t.set_armor_mod_vs_bludgeon(add_f(t.armor_mod_vs_bludgeon(), 0.2)), // Marble
        0x3800_0012 => t.set_armor_mod_vs_acid(add_f(t.armor_mod_vs_acid(), 0.4)), // ArmoredilloHide
        0x3800_0016 => t.set_armor_mod_vs_fire(add_f(t.armor_mod_vs_fire(), 0.4)), // Ceramic
        0x3800_0014 => t.set_armor_mod_vs_cold(add_f(t.armor_mod_vs_cold(), 0.4)), // Wool
        0x3800_0015 => t.set_armor_mod_vs_electric(add_f(t.armor_mod_vs_electric(), 0.4)), // ReedSharkHide
        0x3800_0038 => t.set_imbued_effect(ImbuedEffectType::MeleeDefense), // Peridot
        0x3800_0039 => t.set_imbued_effect(ImbuedEffectType::MissileDefense), // YellowTopaz
        0x3800_0037 => t.set_imbued_effect(ImbuedEffectType::MagicDefense), // Zircon

        // item tinkering
        0x3800_001E => t.set_value(mul_f32(t.value(), 0.75)), // Pine
        0x3800_001F => t.set_value(mul_f32(t.value(), 1.25)), // Gold
        0x3800_0019 => t.set_encumbrance_val(mul_f32(t.encumbrance_val(), 0.75)), // Linen
        // Ivory is handled purely in recipe mod?
        0x3800_0043 => t.set_retained(true),  // Leather
        0x3900_004E => t.set_retained(false), // Sandstone: 43 -> 4E
        0x3800_002F => t.set_item_max_mana(t.item_max_mana().map(|x| x.wrapping_add(500))), // Moonstone

        0x3900_0042 => {
            // legacy, these are handled in recipe mods
        }

        0x3800_0035 => {
            // Copper

            // handled in requirements, only here for legacy support?
            if t.item_skill_limit() != Some(Skill::MissileDefense)
                || t.item_skill_level_limit().is_none()
            {
                return false;
            }

            // change activation requirement: missile defense -> melee defense
            t.set_item_skill_limit(Some(Skill::MeleeDefense));
            #[allow(clippy::cast_precision_loss)]
            let v: i32 = (t.item_skill_level_limit().expect("checked") as f32 / 0.7f32).cs_cast();
            t.set_item_skill_level_limit(Some(v));
        }

        0x3800_0034 => {
            // Silver

            // handled in requirements, only here for legacy support?
            if t.item_skill_limit() != Some(Skill::MeleeDefense)
                || t.item_skill_level_limit().is_none()
            {
                return false;
            }

            // change activation requirement: melee defense -> missile defense
            t.set_item_skill_limit(Some(Skill::MissileDefense));
            #[allow(clippy::cast_precision_loss)]
            let v: i32 = (t.item_skill_level_limit().expect("checked") as f32 * 0.7f32).cs_cast();
            t.set_item_skill_level_limit(Some(v));
        }

        0x3800_0036 => {
            // Silk

            // remove allegiance rank limit, set difficulty to spellcraft
            t.set_item_allegiance_rank_limit(None);
            t.set_item_difficulty(t.item_spellcraft());
        }

        // armatures / trinkets
        // these are handled in recipe mod
        0x3900_0048 | 0x3900_0049 | 0x3900_0050 | 0x3900_0051 | 0x3900_0052 | 0x3900_0053 => {
            return false
        }

        // magic item tinkering
        0x3800_0025 => t.set_imbued_effect(ImbuedEffectType::ArmorRending), // Sunstone
        0x3800_0024 => t.set_imbued_effect(ImbuedEffectType::CripplingBlow), // FireOpal
        0x3800_0023 => t.set_imbued_effect(ImbuedEffectType::CriticalStrike), // BlackOpal
        0x3800_002E => {
            // Opal
            //target.ManaConversionMod += 0.01f;
            t.set_mana_conversion_mod(Some(
                t.mana_conversion_mod().unwrap_or(0.0) + f64::from(0.01f32),
            ));
        }
        0x3800_004B => {
            // GreenGarnet: 44 -> 4B
            t.set_elemental_damage_mod(Some(
                t.elemental_damage_mod().unwrap_or(0.0) + f64::from(0.01f32),
            )); // + 1% vs. monsters, + 0.25% vs. players
        }

        0x3800_0041 => {
            // these are handled in recipe mods already
            t.set_imbued_effect(ImbuedEffectType::Spellbook);
        }

        // weapon tinkering
        0x3800_001A => t.set_damage(t.damage().map(|x| x.wrapping_add(1))), // Iron
        0x3800_001B => t.set_damage_mod(add_f(t.damage_mod(), 0.04)),       // Mahogany
        0x3800_001C => t.set_damage_variance(t.damage_variance().map(|x| x * f64::from(0.8f32))), // Granite / Lucky Rabbit's Foot
        0x3800_001D => {
            t.set_weapon_time(Some(0.max(t.weapon_time().unwrap_or(0).wrapping_sub(50))))
        } // Oak
        0x3800_0020 => t.set_weapon_defense(add_f(t.weapon_defense(), 0.01)), // Brass
        0x3800_0021 => t.set_weapon_offense(add_f(t.weapon_offense(), 0.01)), // Velvet

        // only 1 imbue can be applied per piece of armor?
        0x3800_003A => t.set_imbued_effect(ImbuedEffectType::AcidRending), // Emerald
        0x3800_003B => t.set_imbued_effect(ImbuedEffectType::BludgeonRending), // WhiteSapphire
        0x3800_003C => t.set_imbued_effect(ImbuedEffectType::ColdRending), // Aquamarine
        0x3800_003D => t.set_imbued_effect(ImbuedEffectType::ElectricRending), // Jet
        0x3800_003E => t.set_imbued_effect(ImbuedEffectType::FireRending), // RedGarnet
        0x3800_003F => t.set_imbued_effect(ImbuedEffectType::PierceRending), // BlackGarnet
        0x3800_0040 => t.set_imbued_effect(ImbuedEffectType::SlashRending), // ImperialTopaz

        // addons
        0x3800_000F => t.set_icon_overlay_id(t.icon_overlay_secondary()), // Stamps

        0x3800_0046 => {
            // Fetish of the Dark Idols

            // shouldn't exist on player items, but just recreating original script here
            if t.imbued_effect().0 >= ImbuedEffectType::IgnoreAllArmor.0 {
                t.set_imbued_effect(ImbuedEffectType::Undef);
            }

            t.set_imbued_effect(
                t.imbued_effect() | ImbuedEffectType::IgnoreSomeMagicProjectileDamage,
            );
            //target.AbsorbMagicDamage = 0.25f;   // not in original mods / mutation?
        }

        0x3900_0000 => {
            // Paragon Weapons
            t.set_item_max_level(Some(t.item_max_level().unwrap_or(0).wrapping_add(1)));
            t.set_item_base_xp(Some(2_000_000_000));
            t.set_item_total_xp(Some(t.item_total_xp().unwrap_or(0)));
        }

        _ => {
            log::error!(
                "{}.RecipeManager.Tinkering_ModifyItem({} ({}), {} ({})) - unknown mutation id: {data_id:08X}",
                name(w, player),
                name(w, source),
                source,
                name(w, target),
                target
            );
            return false;
        }
    }

    if INC_ITEM_TINKERED.contains(&data_id) {
        handle_tinker_log(w, source, target);
    }

    true
}

// ACE: RecipeManager.incItemTinkered
/// only needed for legacy method
/// ideally this wouldn't even be needed for the legacy method, and recipe.IsTinkering() would suffice
/// however, that would break for rare salvages, which have 0 difficulty and salvage_Type 0
const INC_ITEM_TINKERED: [u32; 38] = [
    0x3800_0011, // Steel
    0x3800_0012, // Armoredillo Hide
    0x3800_0013, // Marble
    0x3800_0014, // Wool
    0x3800_0015, // Reedshark Hide
    0x3800_0016, // Ceramic
    0x3800_0017, // Alabaster
    0x3800_0018, // Bronze
    0x3800_0019, // Linen
    0x3800_001A, // Iron
    0x3800_001B, // Mahogany
    0x3800_001C, // Granite
    0x3800_001D, // Oak
    0x3800_001E, // Pine
    0x3800_001F, // Gold
    0x3800_0020, // Brass
    0x3800_0021, // Velvet
    0x3800_0023, // Black Opal
    0x3800_0024, // Fire Opal
    0x3800_0025, // Sunstone
    0x3800_002E, // Opal
    0x3800_002F, // Moonstone
    0x3800_0034, // Silver
    0x3800_0035, // Copper
    0x3800_0036, // Silk
    0x3800_0037, // Zircon
    0x3800_0038, // Peridot
    0x3800_0039, // Yellow Topaz
    0x3800_003A, // Emerald
    0x3800_003B, // White Sapphire
    0x3800_003C, // Aquamarine
    0x3800_003D, // Jet
    0x3800_003E, // Red Garnet
    0x3800_003F, // Black Garnet
    0x3800_0040, // Imperial Topaz
    0x3800_0041, // Cantrips
    0x3800_0042, // Heritage
    0x3800_004B, // Green Garnet
];

// ACE: RecipeManager.AddSpell
/// Adds `spell` to the target's spell book; `difficulty` (ACE's default 25) raises its spellcraft
/// and difficulty; a target with no UI effects becomes Magical.
pub fn add_spell(
    w: &mut World,
    player: ObjectGuid,
    target: ObjectGuid,
    spell: SpellId,
    difficulty: i32,
) {
    let t = obj_mut(w, target);
    let _ = t.biota.get_or_add_known_spell(spell.0.cs_cast(), 2.0);
    t.wo.world_object_database.changes_detected = true;

    if difficulty != 0 {
        t.set_item_spellcraft(Some(
            t.item_spellcraft().unwrap_or(0).wrapping_add(difficulty),
        ));
        t.set_item_difficulty(Some(
            t.item_difficulty().unwrap_or(0).wrapping_add(difficulty),
        ));
    }
    if t.ui_effects().is_none() {
        t.set_ui_effects(Some(UiEffects::Magical));
        let v: i32 = t.ui_effects().expect("just set").0.cs_cast();
        let m =
            game_message_public_update_property_int(obj_mut(w, target), PropertyInt::UiEffects, v);
        send(w, player, m);
    }
}

// ACE: RecipeManager.IconUnderlay
/// derrick's input => output mappings
pub static ICON_UNDERLAY: LazyLock<DotNetDict<ImbuedEffectType, u32>> = LazyLock::new(|| {
    let mut d = DotNetDict::new();
    d.add(ImbuedEffectType::ColdRending, 0x0600_3353);
    d.add(ImbuedEffectType::ElectricRending, 0x0600_3354);
    d.add(ImbuedEffectType::AcidRending, 0x0600_3355);
    d.add(ImbuedEffectType::ArmorRending, 0x0600_3356);
    d.add(ImbuedEffectType::CripplingBlow, 0x0600_3357);
    d.add(ImbuedEffectType::CriticalStrike, 0x0600_3358);
    d.add(ImbuedEffectType::FireRending, 0x0600_3359);
    d.add(ImbuedEffectType::BludgeonRending, 0x0600_335a);
    d.add(ImbuedEffectType::PierceRending, 0x0600_335b);
    d.add(ImbuedEffectType::SlashRending, 0x0600_335c);
    d
});

// ACE: RecipeManager.GetImbuedEffects
/// The five `ImbuedEffect*` properties OR-ed.
#[must_use]
pub fn get_imbued_effects(target: &WorldObject) -> ImbuedEffectType {
    let mut imbued_effects = 0i32;

    imbued_effects |= target.get_property(PropertyInt::ImbuedEffect).unwrap_or(0);
    imbued_effects |= target.get_property(PropertyInt::ImbuedEffect2).unwrap_or(0);
    imbued_effects |= target.get_property(PropertyInt::ImbuedEffect3).unwrap_or(0);
    imbued_effects |= target.get_property(PropertyInt::ImbuedEffect4).unwrap_or(0);
    imbued_effects |= target.get_property(PropertyInt::ImbuedEffect5).unwrap_or(0);

    ImbuedEffectType(imbued_effects.cs_cast())
}

// ACE: RecipeManager.VerifyRequirements
/// `VerifyRequirements(recipe, player, source, target)`: where the items are, then the target's,
/// the source's and the player's requirements, in that order.
pub fn verify_requirements(
    w: &mut World,
    recipe: &Recipe,
    player: ObjectGuid,
    source: ObjectGuid,
    target: ObjectGuid,
) -> bool {
    if !verify_use(w, player, source, target) {
        return false;
    }

    if !verify_requirements_of(w, recipe, player, target, RequirementType::Target) {
        return false;
    }

    if !verify_requirements_of(w, recipe, player, source, RequirementType::Source) {
        return false;
    }

    if !verify_requirements_of(w, recipe, player, player, RequirementType::Player) {
        return false;
    }

    true
}

// ACE: RecipeManager.VerifyUse
/// `VerifyUse(player, source, target)`: the source's `ItemUseable` says where each may be.
#[must_use]
pub fn verify_use(w: &World, player: ObjectGuid, source: ObjectGuid, target: ObjectGuid) -> bool {
    let usable = obj(w, source).item_useable().unwrap_or(Usable::Undef);

    if usable == Usable::Undef {
        log::warn!(
            "{}.RecipeManager.VerifyUse({} ({}), {} ({})) - source not usable, falling back on defaults",
            name(w, player),
            name(w, source),
            source,
            name(w, target),
            target
        );

        // re-verify
        if player_inventory::find_object(w, player, source, SearchLocations::MyInventory)
            .result
            .is_none()
        {
            return false;
        }

        // almost always MyInventory, but sometimes can be applied to equipped
        if player_inventory::find_object(
            w,
            player,
            target,
            SearchLocations::MyInventory | SearchLocations::MyEquippedItems,
        )
        .result
        .is_none()
        {
            return false;
        }

        return true;
    }

    let source_use = usable.get_source_flags();
    let target_use = usable.get_target_flags();

    verify_use_of(w, player, source, source_use) && verify_use_of(w, player, target, target_use)
}

// ACE: RecipeManager.VerifyUse
/// `VerifyUse(player, obj, usable)`: the object is where the flags allow.
#[must_use]
pub fn verify_use_of(w: &World, player: ObjectGuid, obj_guid: ObjectGuid, usable: Usable) -> bool {
    let mut search_locations = SearchLocations::None;

    // `Enum.HasFlag`: every bit of the flag.
    let has = |f: Usable| usable.0 & f.0 == f.0;

    // TODO: figure out other Usable flags
    if has(Usable::Contained) {
        search_locations =
            search_locations | SearchLocations::MyInventory | SearchLocations::MyEquippedItems;
    }
    if has(Usable::Wielded) {
        search_locations = search_locations | SearchLocations::MyEquippedItems;
    }
    if has(Usable::Remote) {
        search_locations = search_locations | SearchLocations::LocationsICanMove;
        // TODO: moveto for this type
    }

    player_inventory::find_object(w, player, obj_guid, search_locations)
        .result
        .is_some()
}

// ACE: RecipeManager.Debug
/// A `public static bool` in ACE that nothing sets.
pub const DEBUG: bool = false;

// ACE: RecipeManager.VerifyRequirements
/// `VerifyRequirements(recipe, player, obj, reqType)`: the requirements of one kind (bool, int,
/// float, string, instance id, data id, in that order), each against `obj`'s property.
pub fn verify_requirements_of(
    w: &mut World,
    recipe: &Recipe,
    player: ObjectGuid,
    obj_guid: ObjectGuid,
    req_type: RequirementType,
) -> bool {
    let idx = req_type.0;
    let bool_reqs: Vec<_> = recipe
        .recipe_requirements_bool
        .iter()
        .filter(|i| i32::from(i.index) == idx)
        .collect();
    let int_reqs: Vec<_> = recipe
        .recipe_requirements_int
        .iter()
        .filter(|i| i32::from(i.index) == idx)
        .collect();
    let float_reqs: Vec<_> = recipe
        .recipe_requirements_float
        .iter()
        .filter(|i| i32::from(i.index) == idx)
        .collect();
    let str_reqs: Vec<_> = recipe
        .recipe_requirements_string
        .iter()
        .filter(|i| i32::from(i.index) == idx)
        .collect();
    let iid_reqs: Vec<_> = recipe
        .recipe_requirements_iid
        .iter()
        .filter(|i| i32::from(i.index) == idx)
        .collect();
    let did_reqs: Vec<_> = recipe
        .recipe_requirements_did
        .iter()
        .filter(|i| i32::from(i.index) == idx)
        .collect();

    let total_reqs = bool_reqs.len()
        + int_reqs.len()
        + float_reqs.len()
        + str_reqs.len()
        + iid_reqs.len()
        + did_reqs.len();

    if DEBUG && total_reqs > 0 {
        log::debug!("{req_type:?} Requirements: {total_reqs}");
    }

    let prop_u16 = |stat: i32| -> u16 { stat.cs_cast() };

    for requirement in bool_reqs {
        let value = obj(w, obj_guid).get_property(PropertyBool(prop_u16(requirement.stat)));
        // `Convert.ToDouble(bool)`: 1 or 0.
        let normalized = value.map(|v| f64::from(u8::from(v)));

        if !verify_requirement(
            w,
            player,
            CompareType(requirement.r#enum),
            normalized,
            f64::from(u8::from(requirement.value)),
            requirement.message.as_deref(),
        ) {
            return false;
        }
    }

    for requirement in int_reqs {
        let value = obj(w, obj_guid).get_property(PropertyInt(prop_u16(requirement.stat)));
        let normalized = value.map(f64::from);

        if !verify_requirement(
            w,
            player,
            CompareType(requirement.r#enum),
            normalized,
            f64::from(requirement.value),
            requirement.message.as_deref(),
        ) {
            return false;
        }
    }

    for requirement in float_reqs {
        let value = obj(w, obj_guid).get_property(PropertyFloat(prop_u16(requirement.stat)));

        if !verify_requirement(
            w,
            player,
            CompareType(requirement.r#enum),
            value,
            requirement.value,
            requirement.message.as_deref(),
        ) {
            return false;
        }
    }

    for requirement in str_reqs {
        let value = obj(w, obj_guid).get_property(PropertyString(prop_u16(requirement.stat)));

        if !verify_requirement_string(
            w,
            player,
            CompareType(requirement.r#enum),
            value.as_deref(),
            requirement.value.as_deref(),
            requirement.message.as_deref(),
        ) {
            return false;
        }
    }

    for requirement in iid_reqs {
        let value = obj(w, obj_guid).get_property(PropertyInstanceId(prop_u16(requirement.stat)));
        let normalized = value.map(f64::from);

        if !verify_requirement(
            w,
            player,
            CompareType(requirement.r#enum),
            normalized,
            f64::from(requirement.value),
            requirement.message.as_deref(),
        ) {
            return false;
        }
    }

    for requirement in did_reqs {
        let value = obj(w, obj_guid).get_property(PropertyDataId(prop_u16(requirement.stat)));
        let normalized = value.map(f64::from);

        if !verify_requirement(
            w,
            player,
            CompareType(requirement.r#enum),
            normalized,
            f64::from(requirement.value),
            requirement.message.as_deref(),
        ) {
            return false;
        }
    }

    if DEBUG && total_reqs > 0 {
        log::debug!("-----");
    }

    true
}

// ACE: RecipeManager.VerifyRequirement
/// The numeric overload: `false` (and `failMsg` to the player) when the comparison **holds**: the
/// compare type names the failing condition.
pub fn verify_requirement(
    w: &mut World,
    player: ObjectGuid,
    compare_type: CompareType,
    prop: Option<f64>,
    val: f64,
    fail_msg: Option<&str>,
) -> bool {
    let success = verify_requirement_value(compare_type, prop, val);

    if !success {
        system_chat(
            w,
            player,
            fail_msg.unwrap_or_default(),
            ChatMessageType::Craft,
        );
    }

    success
}

/// The comparison of [`verify_requirement`], without the message (for the vectors).
#[must_use]
#[allow(clippy::float_cmp)] // C# `==` / `!=` on doubles
#[allow(clippy::collapsible_match)] // ACE's switch-then-if shape
pub fn verify_requirement_value(compare_type: CompareType, prop: Option<f64>, val: f64) -> bool {
    let mut success = true;
    let p = prop.unwrap_or(0.0);
    // `(int)(prop ?? 0) & (int)val`: double to int casts.
    let bits = |a: f64, b: f64| -> (i32, i32) { (a.cs_cast(), b.cs_cast()) };

    match compare_type {
        CompareType::GreaterThan => {
            if p > val {
                success = false;
            }
        }
        CompareType::LessThanEqual => {
            if p <= val {
                success = false;
            }
        }
        CompareType::LessThan => {
            if p < val {
                success = false;
            }
        }
        CompareType::GreaterThanEqual => {
            if p >= val {
                success = false;
            }
        }
        CompareType::NotEqual => {
            if p != val {
                success = false;
            }
        }
        CompareType::NotEqualNotExist => {
            if prop.is_none_or(|v| v != val) {
                success = false;
            }
        }
        CompareType::Equal => {
            if p == val {
                success = false;
            }
        }
        CompareType::NotExist => {
            if prop.is_none() {
                success = false;
            }
        }
        CompareType::Exist => {
            if prop.is_some() {
                success = false;
            }
        }
        CompareType::NotHasBits => {
            let (a, b) = bits(p, val);
            if a & b == 0 {
                success = false;
            }
        }
        CompareType::HasBits => {
            let (a, b) = bits(p, val);
            if a & b == b {
                success = false;
            }
        }
        _ => {}
    }

    success
}

// ACE: RecipeManager.VerifyRequirement
/// The string overload (only the equality and existence compare types apply).
pub fn verify_requirement_string(
    w: &mut World,
    player: ObjectGuid,
    compare_type: CompareType,
    prop: Option<&str>,
    val: Option<&str>,
    fail_msg: Option<&str>,
) -> bool {
    let success = verify_requirement_string_value(compare_type, prop, val);
    if !success {
        system_chat(
            w,
            player,
            fail_msg.unwrap_or_default(),
            ChatMessageType::Craft,
        );
    }

    success
}

/// The comparison of [`verify_requirement_string`] (`string.Equals(null)` is false).
#[must_use]
#[allow(clippy::collapsible_match)] // ACE's switch-then-if shape
pub fn verify_requirement_string_value(
    compare_type: CompareType,
    prop: Option<&str>,
    val: Option<&str>,
) -> bool {
    let mut success = true;
    let equals = |a: &str| val.is_some_and(|v| v == a);

    match compare_type {
        CompareType::NotEqual => {
            if !equals(prop.unwrap_or("")) {
                success = false;
            }
        }
        CompareType::NotEqualNotExist => {
            if prop.is_none_or(|p| !equals(p)) {
                success = false;
            }
        }
        CompareType::Equal => {
            if equals(prop.unwrap_or("")) {
                success = false;
            }
        }
        CompareType::NotExist => {
            if prop.is_none() {
                success = false;
            }
        }
        CompareType::Exist => {
            if prop.is_some() {
                success = false;
            }
        }
        _ => {}
    }
    success
}

// ACE: RecipeManager.CreateDestroyItems
/// Returns a list of object guids that have been modified (`None`: the recipe's product is missing
/// from the database). Two draws: destroy target, then destroy source.
pub fn create_destroy_items(
    w: &mut World,
    player: ObjectGuid,
    recipe: &Recipe,
    source: ObjectGuid,
    target: ObjectGuid,
    success_chance: f64,
    success: bool,
) -> Option<DotNetHashSet<u32>> {
    let destroy_target_chance = if success {
        recipe.success_destroy_target_chance
    } else {
        recipe.fail_destroy_target_chance
    };
    let destroy_source_chance = if success {
        recipe.success_destroy_source_chance
    } else {
        recipe.fail_destroy_source_chance
    };

    let destroy_target = ThreadSafeRandom::next_float(0.0, 1.0) < destroy_target_chance;
    let destroy_source = ThreadSafeRandom::next_float(0.0, 1.0) < destroy_source_chance;

    let create_item = if success {
        recipe.success_wcid
    } else {
        recipe.fail_wcid
    };
    let create_amount = if success {
        recipe.success_amount
    } else {
        recipe.fail_amount
    };

    if create_item > 0 && w.content.get_cached_weenie(create_item).is_none() {
        log::error!(
            "RecipeManager.CreateDestroyItems: Recipe.Id({}) couldn't find {}WCID {create_item} in database.",
            recipe.id,
            if success { "Success" } else { "Fail" }
        );
        if let Some(m) = event(w, player, |d| {
            game_event_weenie_error(d, WeenieError::CraftGeneralErrorUiMsg)
        }) {
            send(w, player, m);
        }
        return None;
    }

    let mut tombstones = Tombstones::default();
    w.recipe_manager.consumed_sources.clear();
    for g in [target, source] {
        if let Some(o) = w.objects.get(g) {
            let v = material_or_wcid(o);
            w.recipe_manager.consumed_sources.insert(g, v);
        }
    }

    if destroy_target {
        let destroy_target_amount = if success {
            recipe.success_destroy_target_amount
        } else {
            recipe.fail_destroy_target_amount
        };
        let destroy_target_message = if success {
            &recipe.success_destroy_target_message
        } else {
            &recipe.fail_destroy_target_message
        };

        tombstones.keep(w, target);
        destroy_item(
            w,
            player,
            recipe,
            target,
            destroy_target_amount,
            destroy_target_message.as_deref(),
        );
    }

    if destroy_source {
        let destroy_source_amount = if success {
            recipe.success_destroy_source_amount
        } else {
            recipe.fail_destroy_source_amount
        };
        let destroy_source_message = if success {
            &recipe.success_destroy_source_message
        } else {
            &recipe.fail_destroy_source_message
        };

        tombstones.keep(w, source);
        destroy_item(
            w,
            player,
            recipe,
            source,
            destroy_source_amount,
            destroy_source_message.as_deref(),
        );
    }

    let mut result = None;

    if create_item > 0 {
        result = create_item_(w, player, create_item, create_amount);
    }

    let modified = modify_item(w, player, recipe, source, target, result, success);

    // DIVERGE: a product that did not fit in the player's inventory is dropped unreferenced in ACE
    // (the garbage collector's); it leaves World.objects here (4.5a's rule for unclaimed objects).
    if let Some(r) = result {
        if w.objects.get(r).is_some_and(|o| {
            o.wo.world_object_properties.container.is_none() && o.current_landblock.is_none()
        }) {
            w.objects.remove(r);
        }
    }

    // broadcast different messages based on recipe type
    if is_tinkering(recipe) {
        broadcast_tinkering_(
            w,
            player,
            source,
            target,
            success_chance,
            success,
            &tombstones,
        );
    } else {
        let message = if success {
            &recipe.success_message
        } else {
            &recipe.fail_message
        };
        let message = message.as_deref().unwrap_or_default();

        system_chat(w, player, message, ChatMessageType::Craft);

        log::info!(
            "[CRAFTING] {} used {} on {} {}successfully. {}{}| {message}",
            name(w, player),
            tombstones.name_with_material(w, source),
            tombstones.name_with_material(w, target),
            if success { "" } else { "un" },
            if destroy_source {
                format!(
                    "| {} was destroyed ",
                    tombstones.name_with_material(w, source)
                )
            } else {
                String::new()
            },
            if destroy_target {
                format!(
                    "| {} was destroyed ",
                    tombstones.name_with_material(w, target)
                )
            } else {
                String::new()
            }
        );
    }

    Some(modified)
}

// ACE: RecipeManager.BroadcastTinkering
/// The local Craft-channel broadcast of a tinker.
pub fn broadcast_tinkering(
    w: &mut World,
    player: ObjectGuid,
    tool: ObjectGuid,
    target: ObjectGuid,
    chance: f64,
    success: bool,
) {
    broadcast_tinkering_(
        w,
        player,
        tool,
        target,
        chance,
        success,
        &Tombstones::default(),
    );
}

fn broadcast_tinkering_(
    w: &mut World,
    player: ObjectGuid,
    tool: ObjectGuid,
    target: ObjectGuid,
    chance: f64,
    success: bool,
    tombstones: &Tombstones,
) {
    // retail AC had some inconsistency with respect to the messages broadcast by tinkering.
    //
    // Largely this revolved around the name of the weenies that represented each of the salvage bags.
    // First, there were name changes that were a result of the client side pre-pending of material type which resulted in bags being named "Salvage", "Salvaged"
    // Second, these bags that were generated from loot by players always ended with the number of materials in the bag, such as (100), (88), (1)
    // while non-lootgen bags did not include the number, so the name of the bag when displayed or broadcast varied like "Steel Salvage (100)", "Steel Salvaged (100)", "Steel Salvage"
    // Third, Foolproof bags, again depending on weenie names, also had their own variations which resulted in display names like "Foolproof Black Garnet Gem", "Zircon Foolproof Zircon", "Imperial Topaz Foolproof Imperial Topaz"
    // in many cases, there are multiple weenies of a particular salvage type, used by various systems, which resulted in each tinker operation having varied output even when doing essentially the same thing
    // Finally, items that were inscribed were surprisingly identified in the broadcast like "Reed Shark Hide Studded Leather Sleeves inscribed by Callaway", "Copper Frost Bow inscribed by Mini Bonsai"
    //
    // ACE output, as seen below, has for the most part standardized the message due to weenie name changes, salvage coding and recipe handling differences
    //
    let source_name = strip_count_suffix(&tombstones.name_with_material(w, tool));

    // `(tool.Workmanship ?? 0):#.00` (a consumed tool: as it was when destroyed).
    let tool_workmanship = tombstones.workmanship(w, tool).unwrap_or(0.0);
    let (inscription, scribe_name) = tombstones.inscription(w, target);
    let inscribed = match (inscription, scribe_name) {
        (Some(_), Some(scribe)) => format!(" inscribed by {scribe}"),
        _ => String::new(),
    };

    let msg = format!(
        "{} {} the {source_name} (workmanship {}) to the {}{inscribed}.{}",
        name(w, player),
        if success {
            "successfully applies"
        } else {
            "fails to apply"
        },
        format::format(tool_workmanship, "#.00"),
        tombstones.name_with_material(w, target),
        if success {
            ""
        } else {
            " The target is destroyed."
        }
    );

    // send local broadcast
    world_object_networking::enqueue_broadcast_range(
        w,
        player,
        &game_message_system_chat(&msg, ChatMessageType::Craft),
        crate::world_objects::world_object::LOCAL_BROADCAST_RANGE,
        Some(ChatMessageType::Craft),
    );

    log::info!("[TINKERING] {msg} | Chance: {chance}");
}

/// `Regex.Replace(name, @" \(\d+\)$", "")`: drops one trailing ` (digits)`.
fn strip_count_suffix(s: &str) -> String {
    // `$` also matches before a final newline.
    let (body, tail) = s.strip_suffix('\n').map_or((s, ""), |b| (b, "\n"));
    if let Some(inner) = body.strip_suffix(')') {
        if let Some(open) = inner.rfind(" (") {
            let digits = &inner[open + 2..];
            if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
                return format!("{}{tail}", &inner[..open]);
            }
        }
    }
    s.to_owned()
}

// ACE: RecipeManager.CreateItem
/// Creates `amount` of `wcid` in the player's inventory; returns the new object (in the store even
/// when it did not fit, as ACE returns it).
pub fn create_item(
    w: &mut World,
    player: ObjectGuid,
    wcid: u32,
    amount: u32,
) -> Option<ObjectGuid> {
    create_item_(w, player, wcid, amount)
}

fn create_item_(w: &mut World, player: ObjectGuid, wcid: u32, amount: u32) -> Option<ObjectGuid> {
    let wo = shims::create_new_world_object(w, wcid);

    let Some(wo) = wo else {
        log::warn!(
            "RecipeManager.CreateItem({}, {wcid}, {amount}): failed to create {wcid}",
            name(w, player)
        );
        return None;
    };

    if amount > 1 {
        obj_mut(w, wo).set_stack_size(Some(amount.cs_cast()));
    }

    player_inventory::try_create_in_inventory_with_networking(w, player, wo);
    Some(wo)
}

// ACE: RecipeManager.DestroyItem
/// Consumes `amount` of an inventory item, dequips (and consumes) a wielded one, or destroys the
/// object; then `msg`, if any.
pub fn destroy_item(
    w: &mut World,
    player: ObjectGuid,
    _recipe: &Recipe,
    item: ObjectGuid,
    amount: u32,
    msg: Option<&str>,
) {
    let owner_id = obj(w, item).owner_id();
    let wielder_id = obj(w, item).wielder_id();
    if owner_id == Some(player.full())
        || crate::world_objects::container::get_inventory_item(w, player, item).is_some()
    {
        let item_name = name(w, item);
        if !player_inventory::try_consume_from_inventory_with_networking(
            w,
            player,
            item,
            amount.cs_cast(),
        ) {
            log::warn!("RecipeManager.DestroyItem({}, {item_name}, {amount}, {}): failed to remove {item_name}", name(w, player), msg.unwrap_or_default());
        }
    } else if wielder_id == Some(player.full()) {
        let item_name = name(w, item);
        if player_inventory::try_dequip_object_with_networking(
            w,
            player,
            item,
            DequipObjectAction::ConsumeItem,
        )
        .is_none()
        {
            log::warn!("RecipeManager.DestroyItem({}, {item_name}, {amount}, {}): failed to remove {item_name}", name(w, player), msg.unwrap_or_default());
        }
    } else {
        crate::world_objects::world_object::destroy(w, item, true, false);
    }
    if let Some(msg) = msg.filter(|m| !m.is_empty()) {
        let destroy_message = game_message_system_chat(msg, ChatMessageType::Craft);
        send(w, player, destroy_message);
    }
}

// ACE: RecipeManager.GetSourceMod
/// The object a mod copies from: the player or the source (`None`, and a warning, otherwise).
pub fn get_source_mod(
    w: &World,
    source_type: RecipeSourceType,
    player: ObjectGuid,
    source: ObjectGuid,
) -> Option<ObjectGuid> {
    match source_type {
        RecipeSourceType::Player => return Some(player),
        RecipeSourceType::Source => return Some(source),
        _ => {}
    }
    log::warn!(
        "RecipeManager.GetSourceMod({}, {}, {}) - unknown source type",
        source_type.to_dotnet_string(),
        name(w, player),
        w.objects
            .get(source)
            .map(|_| name(w, source))
            .unwrap_or_default()
    );
    None
}

// ACE: RecipeManager.GetTargetMod
/// The object a mod writes to.
#[must_use]
pub fn get_target_mod(
    r#type: ModificationType,
    source: ObjectGuid,
    target: ObjectGuid,
    player: ObjectGuid,
    result: Option<ObjectGuid>,
) -> ObjectGuid {
    match r#type {
        ModificationType::SuccessSource | ModificationType::FailureSource => source,

        ModificationType::SuccessPlayer | ModificationType::FailurePlayer => player,

        ModificationType::SuccessResult | ModificationType::FailureResult => {
            result.unwrap_or(target)
        }

        _ => target,
    }
}

// ACE: RecipeManager.ModifyItem
/// Returns a list of object guids that have been modified
pub fn modify_item(
    w: &mut World,
    player: ObjectGuid,
    recipe: &Recipe,
    source: ObjectGuid,
    target: ObjectGuid,
    result: Option<ObjectGuid>,
    success: bool,
) -> DotNetHashSet<u32> {
    let mut modified = DotNetHashSet::new();

    for m in &recipe.recipe_mod {
        if m.executes_on_success != success {
            continue;
        }

        // adjust vitals
        if m.health != 0 {
            modify_vital(w, player, PropertyAttribute2nd::Health, m.health);
        }

        if m.stamina != 0 {
            modify_vital(w, player, PropertyAttribute2nd::Stamina, m.stamina);
        }

        if m.mana != 0 {
            modify_vital(w, player, PropertyAttribute2nd::Mana, m.mana);
        }

        // apply type mods
        for bool_mod in &m.recipe_mods_bool {
            modify_bool(w, player, bool_mod, source, target, result, &mut modified);
        }

        for int_mod in &m.recipe_mods_int {
            modify_int(w, player, int_mod, source, target, result, &mut modified);
        }

        for float_mod in &m.recipe_mods_float {
            modify_float(w, player, float_mod, source, target, result, &mut modified);
        }

        for string_mod in &m.recipe_mods_string {
            modify_string(w, player, string_mod, source, target, result, &mut modified);
        }

        for iid_mod in &m.recipe_mods_iid {
            modify_instance_id(w, player, iid_mod, source, target, result, &mut modified);
        }

        for did_mod in &m.recipe_mods_did {
            modify_data_id(w, player, did_mod, source, target, result, &mut modified);
        }

        // run mutation script, if applicable
        if m.data_id != 0 {
            try_mutate(
                w,
                player,
                source,
                target,
                recipe,
                m.data_id.cast_unsigned(),
                &mut modified,
            );
        }
    }

    modified
}

// ACE: RecipeManager.ModifyVital
/// A recipe's vital change on the player (a Health loss can kill).
fn modify_vital(
    w: &mut World,
    player: ObjectGuid,
    attribute_2nd: PropertyAttribute2nd,
    value: i32,
) {
    let vital = obj(w, player)
        .get_creature_vital(attribute_2nd)
        .expect("ACE: GetCreatureVital is null (NullReferenceException)");

    // `(uint)Math.Abs(int)`: int.MinValue throws (OverflowException).
    let delta = crate::world_objects::creature_vitals::update_vital_delta(w, player, vital, value);
    let vital_change: u32 = delta
        .checked_abs()
        .expect("System.OverflowException: Math.Abs(int.MinValue)")
        .cast_unsigned();

    if attribute_2nd == PropertyAttribute2nd::Health {
        if value >= 0 {
            crate::entity::damage_history::on_heal(w, player, vital_change);
        } else {
            crate::entity::damage_history::add(w, player, player, DamageType::Health, vital_change);
        }

        let health = obj(w, player).health();
        if health.current(obj(w, player)) == 0 {
            // should this be possible?
            //var lastDamager = player != null ? new DamageHistoryInfo(player) : null;
            let last_damager =
                crate::entity::damage_history_info::DamageHistoryInfo::new(w, player, 0.0);

            dispatch::on_death::on_death(w, player, Some(last_damager), DamageType::Health, false);
            crate::world_objects::creature_death::die(w, player);
        }
    }
}

/// Whether `g` is still in the store; ACE writes to destroyed objects too.
fn live(w: &World, g: ObjectGuid) -> bool {
    w.objects.get(g).is_some()
}

// ACE: RecipeManager.ModifyBool
pub fn modify_bool(
    w: &mut World,
    player: ObjectGuid,
    bool_mod: &RecipeModsBool,
    source: ObjectGuid,
    target: ObjectGuid,
    result: Option<ObjectGuid>,
    modified: &mut DotNetHashSet<u32>,
) {
    let op = ModificationOperation(bool_mod.r#enum);
    let prop = PropertyBool(bool_mod.stat.cs_cast());
    let value = bool_mod.value;

    let target_mod = get_target_mod(
        ModificationType(i32::from(bool_mod.index)),
        source,
        target,
        player,
        result,
    );

    // always SetValue?
    if op != ModificationOperation::SetValue {
        log::warn!(
            "RecipeManager.ModifyBool({}, {}): unhandled operation {}",
            name(w, source),
            name(w, target),
            op.to_dotnet_string()
        );
        return;
    }
    // DIVERGE: see update_obj (a destroyed object is not written or announced).
    if live(w, target_mod) {
        player_properties::update_property_bool(w, player, target_mod, prop, Some(value), false);
    }
    modified.insert(target_mod.full());
}

fn int_prop(w: &World, g: ObjectGuid, prop: PropertyInt) -> Option<i32> {
    w.objects.get(g).and_then(|o| o.get_property(prop))
}

fn float_prop(w: &World, g: ObjectGuid, prop: PropertyFloat) -> Option<f64> {
    w.objects.get(g).and_then(|o| o.get_property(prop))
}

fn did_prop(w: &World, g: ObjectGuid, prop: PropertyDataId) -> Option<u32> {
    w.objects.get(g).and_then(|o| o.get_property(prop))
}

/// ACE's `null` result on a `CopyFromSourceToResult` (NullReferenceException).
fn result_or_nre(result: Option<ObjectGuid>) -> ObjectGuid {
    result.expect("System.NullReferenceException: recipe result is null")
}

/// ACE's `null` source mod dereferenced.
fn source_or_nre(source_mod: Option<ObjectGuid>) -> ObjectGuid {
    source_mod.expect("System.NullReferenceException: GetSourceMod returned null")
}

// ACE: RecipeManager.ModifyInt
pub fn modify_int(
    w: &mut World,
    player: ObjectGuid,
    int_mod: &RecipeModsInt,
    source: ObjectGuid,
    target: ObjectGuid,
    result: Option<ObjectGuid>,
    modified: &mut DotNetHashSet<u32>,
) {
    let op = ModificationOperation(int_mod.r#enum);
    let prop = PropertyInt(int_mod.stat.cs_cast());
    let value = int_mod.value;

    let source_mod = get_source_mod(w, RecipeSourceType(int_mod.source), player, source);
    let target_mod = get_target_mod(
        ModificationType(i32::from(int_mod.index)),
        source,
        target,
        player,
        result,
    );

    // DIVERGE (each arm): see update_obj (a destroyed object is not written or announced).
    match op {
        ModificationOperation::SetValue => {
            if live(w, target_mod) {
                player_properties::update_property_int(
                    w,
                    player,
                    target_mod,
                    prop,
                    Some(value),
                    false,
                );
            }
            modified.insert(target_mod.full());
        }
        ModificationOperation::Add => {
            if live(w, target_mod) {
                let v = int_prop(w, target_mod, prop)
                    .unwrap_or(0)
                    .wrapping_add(value);
                player_properties::update_property_int(w, player, target_mod, prop, Some(v), false);
            }
            modified.insert(target_mod.full());
        }
        ModificationOperation::CopyFromSourceToTarget => {
            let v = int_prop(w, source_or_nre(source_mod), prop).unwrap_or(0);
            if live(w, target) {
                player_properties::update_property_int(w, player, target, prop, Some(v), false);
            }
            modified.insert(target.full());
        }
        ModificationOperation::CopyFromSourceToResult => {
            let result = result_or_nre(result);
            let v = int_prop(w, player, prop).unwrap_or(0); // ??
            if live(w, result) {
                player_properties::update_property_int(w, player, result, prop, Some(v), false);
            }
            modified.insert(result.full());
        }
        ModificationOperation::AddSpell => {
            let added = w
                .objects
                .get_mut(target_mod)
                .is_some_and(|o| o.biota.get_or_add_known_spell(int_mod.stat, 2.0).1);
            modified.insert(target_mod.full());
            if added {
                obj_mut(w, target_mod)
                    .wo
                    .world_object_database
                    .changes_detected = true;
            }
        }
        ModificationOperation::SetBitsOn => {
            if live(w, target_mod) {
                let mut bits = int_prop(w, target_mod, prop).unwrap_or(0);
                bits |= value;
                player_properties::update_property_int(
                    w,
                    player,
                    target_mod,
                    prop,
                    Some(bits),
                    false,
                );
            }
            modified.insert(target_mod.full());
        }
        ModificationOperation::SetBitsOff => {
            if live(w, target_mod) {
                let mut bits = int_prop(w, target_mod, prop).unwrap_or(0);
                bits &= !value;
                player_properties::update_property_int(
                    w,
                    player,
                    target_mod,
                    prop,
                    Some(bits),
                    false,
                );
            }
            modified.insert(target_mod.full());
        }
        _ => {
            log::warn!(
                "RecipeManager.ModifyInt({}, {}): unhandled operation {}",
                name(w, source),
                name(w, target),
                op.to_dotnet_string()
            );
        }
    }
}

// ACE: RecipeManager.ModifyFloat
pub fn modify_float(
    w: &mut World,
    player: ObjectGuid,
    float_mod: &RecipeModsFloat,
    source: ObjectGuid,
    target: ObjectGuid,
    result: Option<ObjectGuid>,
    modified: &mut DotNetHashSet<u32>,
) {
    let op = ModificationOperation(float_mod.r#enum);
    let prop = PropertyFloat(float_mod.stat.cs_cast());
    let value = float_mod.value;

    let source_mod = get_source_mod(w, RecipeSourceType(float_mod.source), player, source);
    let target_mod = get_target_mod(
        ModificationType(i32::from(float_mod.index)),
        source,
        target,
        player,
        result,
    );

    // DIVERGE (each arm): see update_obj (a destroyed object is not written or announced).
    match op {
        ModificationOperation::SetValue => {
            if live(w, target_mod) {
                player_properties::update_property_float(
                    w,
                    player,
                    target_mod,
                    prop,
                    Some(value),
                    false,
                );
            }
            modified.insert(target_mod.full());
        }
        ModificationOperation::Add => {
            if live(w, target_mod) {
                let v = float_prop(w, target_mod, prop).unwrap_or(0.0) + value;
                player_properties::update_property_float(
                    w,
                    player,
                    target_mod,
                    prop,
                    Some(v),
                    false,
                );
            }
            modified.insert(target_mod.full());
        }
        ModificationOperation::CopyFromSourceToTarget => {
            let v = float_prop(w, source_or_nre(source_mod), prop).unwrap_or(0.0);
            if live(w, target) {
                player_properties::update_property_float(w, player, target, prop, Some(v), false);
            }
            modified.insert(target.full());
        }
        ModificationOperation::CopyFromSourceToResult => {
            let result = result_or_nre(result);
            let v = float_prop(w, player, prop).unwrap_or(0.0);
            if live(w, result) {
                player_properties::update_property_float(w, player, result, prop, Some(v), false);
            }
            modified.insert(result.full());
        }
        _ => {
            log::warn!(
                "RecipeManager.ModifyFloat({}, {}): unhandled operation {}",
                name(w, source),
                name(w, target),
                op.to_dotnet_string()
            );
        }
    }
}

// ACE: RecipeManager.ModifyString
pub fn modify_string(
    w: &mut World,
    player: ObjectGuid,
    string_mod: &RecipeModsString,
    source: ObjectGuid,
    target: ObjectGuid,
    result: Option<ObjectGuid>,
    modified: &mut DotNetHashSet<u32>,
) {
    let op = ModificationOperation(string_mod.r#enum);
    let prop = PropertyString(string_mod.stat.cs_cast());
    let value = string_mod.value.clone();

    let source_mod = get_source_mod(w, RecipeSourceType(string_mod.source), player, source);
    let target_mod = get_target_mod(
        ModificationType(i32::from(string_mod.index)),
        source,
        target,
        player,
        result,
    );

    // DIVERGE (each arm): see update_obj (a destroyed object is not written or announced).
    match op {
        ModificationOperation::SetValue => {
            if live(w, target_mod) {
                player_properties::update_property_string(
                    w,
                    player,
                    target_mod,
                    prop,
                    value.as_deref(),
                    false,
                );
            }
            modified.insert(target_mod.full());
        }
        ModificationOperation::CopyFromSourceToTarget => {
            let sm = source_or_nre(source_mod);
            // `sourceMod.GetProperty(prop) ?? sourceMod.Name`: both may be null (then the property is removed).
            let v = w
                .objects
                .get(sm)
                .and_then(|o| o.get_property(prop))
                .or_else(|| dispatch::name::name(w, sm));
            if live(w, target) {
                player_properties::update_property_string(
                    w,
                    player,
                    target,
                    prop,
                    v.as_deref(),
                    false,
                );
            }
            modified.insert(target.full());
        }
        ModificationOperation::CopyFromSourceToResult => {
            let result = result_or_nre(result);
            let v = obj(w, player)
                .get_property(prop)
                .or_else(|| dispatch::name::name(w, player));
            if live(w, result) {
                player_properties::update_property_string(
                    w,
                    player,
                    result,
                    prop,
                    v.as_deref(),
                    false,
                );
            }
            modified.insert(result.full());
        }
        _ => {
            log::warn!(
                "RecipeManager.ModifyString({}, {}): unhandled operation {}",
                name(w, source),
                name(w, target),
                op.to_dotnet_string()
            );
        }
    }
}

// ACE: RecipeManager.ModifyInstanceID
pub fn modify_instance_id(
    w: &mut World,
    player: ObjectGuid,
    iid_mod: &RecipeModsIID,
    source: ObjectGuid,
    target: ObjectGuid,
    result: Option<ObjectGuid>,
    modified: &mut DotNetHashSet<u32>,
) {
    let op = ModificationOperation(iid_mod.r#enum);
    let prop = PropertyInstanceId(iid_mod.stat.cs_cast());
    let value = iid_mod.value;

    let source_mod = get_source_mod(w, RecipeSourceType(iid_mod.source), player, source);
    let target_mod = get_target_mod(
        ModificationType(i32::from(iid_mod.index)),
        source,
        target,
        player,
        result,
    );

    // DIVERGE (each arm): see update_obj (a destroyed object is not written or announced).
    match op {
        ModificationOperation::SetValue => {
            if live(w, target_mod) {
                player_properties::update_property_instance_id(
                    w,
                    player,
                    target_mod,
                    prop,
                    Some(value),
                    false,
                );
            }
            modified.insert(target_mod.full());
        }
        ModificationOperation::CopyFromSourceToTarget => {
            let v = modify_instance_id_rule_set(w, prop, source_or_nre(source_mod), target_mod);
            if live(w, target) {
                player_properties::update_property_instance_id(
                    w,
                    player,
                    target,
                    prop,
                    Some(v),
                    false,
                );
            }
            modified.insert(target.full());
        }
        ModificationOperation::CopyFromSourceToResult => {
            let result = result_or_nre(result);
            let v = modify_instance_id_rule_set(w, prop, player, target_mod); // ??
            if live(w, result) {
                player_properties::update_property_instance_id(
                    w,
                    player,
                    result,
                    prop,
                    Some(v),
                    false,
                );
            }
            modified.insert(result.full());
        }
        _ => {
            log::warn!(
                "RecipeManager.ModifyInstanceID({}, {}): unhandled operation {}",
                name(w, source),
                name(w, target),
                op.to_dotnet_string()
            );
        }
    }
}

// ACE: RecipeManager.ModifyInstanceIDRuleSet
/// The value a CopyFrom writes: the source's own guid for the wielder/activator restrictions.
fn modify_instance_id_rule_set(
    w: &World,
    property: PropertyInstanceId,
    source_mod: ObjectGuid,
    _target_mod: ObjectGuid,
) -> u32 {
    match property {
        PropertyInstanceId::AllowedWielder | PropertyInstanceId::AllowedActivator => {
            return source_mod.full()
        }
        _ => {}
    }

    w.objects
        .get(source_mod)
        .and_then(|o| o.get_property(property))
        .unwrap_or(0)
}

// ACE: RecipeManager.ModifyDataID
pub fn modify_data_id(
    w: &mut World,
    player: ObjectGuid,
    did_mod: &RecipeModsDID,
    source: ObjectGuid,
    target: ObjectGuid,
    result: Option<ObjectGuid>,
    modified: &mut DotNetHashSet<u32>,
) {
    let op = ModificationOperation(did_mod.r#enum);
    let prop = PropertyDataId(did_mod.stat.cs_cast());
    let value = did_mod.value;

    let source_mod = get_source_mod(w, RecipeSourceType(did_mod.source), player, source);
    let target_mod = get_target_mod(
        ModificationType(i32::from(did_mod.index)),
        source,
        target,
        player,
        result,
    );

    // DIVERGE (each arm): see update_obj (a destroyed object is not written or announced).
    match op {
        ModificationOperation::SetValue => {
            if live(w, target_mod) {
                player_properties::update_property_data_id(
                    w,
                    player,
                    target_mod,
                    prop,
                    Some(value),
                    false,
                );
            }
            modified.insert(target_mod.full());
        }
        ModificationOperation::CopyFromSourceToTarget => {
            let v = did_prop(w, source_or_nre(source_mod), prop).unwrap_or(0);
            if live(w, target) {
                player_properties::update_property_data_id(w, player, target, prop, Some(v), false);
            }
            modified.insert(target.full());
        }
        ModificationOperation::CopyFromSourceToResult => {
            let result = result_or_nre(result);
            let v = did_prop(w, player, prop).unwrap_or(0);
            if live(w, result) {
                player_properties::update_property_data_id(w, player, result, prop, Some(v), false);
            }
            modified.insert(result.full());
        }
        _ => {
            log::warn!(
                "RecipeManager.ModifyDataID({}, {}): unhandled operation {}",
                name(w, source),
                name(w, target),
                op.to_dotnet_string()
            );
        }
    }
}

// ACE: RecipeManager.TryMutate
/// Runs the mutation script `data_id` on the target (one draw, 4.10's `MutationFilter`); a changed
/// `NumTimesTinkered` appends the tinker log.
pub fn try_mutate(
    w: &mut World,
    _player: ObjectGuid,
    source: ObjectGuid,
    target: ObjectGuid,
    _recipe: &Recipe,
    data_id: u32,
    modified: &mut DotNetHashSet<u32>,
) -> bool {
    //if (useMutateNative)
    //    return TryMutateNative(player, source, target, recipe, dataId);

    // DIVERGE: see update_obj. ACE mutates a target the recipe has destroyed (drawing once); a
    // destroyed target has left World.objects here, so nothing is drawn. No shipped recipe with a
    // mutation script destroys its target (checked against ACE's world database).
    if !live(w, target) {
        modified.insert(target.full());
        return false;
    }

    let num_times_tinkered = obj(w, target).num_times_tinkered();

    let mutation_script = crate::entity::mutations::mutation_cache::get_mutation_by_id(w, data_id);

    let Some(mutation_script) = mutation_script else {
        log::error!(
            "RecipeManager.TryApplyMutation({data_id:08X}, {}) - couldn't find mutation script",
            name(w, target)
        );
        return false;
    };

    let result = mutation_script.try_mutate(obj_mut(w, target), 1);

    if num_times_tinkered != obj(w, target).num_times_tinkered() {
        handle_tinker_log(w, source, target);
    }

    modified.insert(target.full());

    result
}

// ACE: RecipeManager.HandleTinkerLog
/// Appends the source's material (or wcid) to the target's comma-separated `TinkerLog`.
fn handle_tinker_log(w: &mut World, source: ObjectGuid, target: ObjectGuid) {
    // `(uint?)source.MaterialType ?? source.WeenieClassId`. The source may have been consumed
    // already (ACE reads the destroyed object).
    let entry = source_material_or_wcid(w, source);
    let t = obj_mut(w, target);
    let mut log = t.tinker_log();
    if let Some(l) = log.as_mut() {
        l.push(',');
    }

    // `target.TinkerLog += uint`: string concatenation (a null log is "").
    let mut s = log.unwrap_or_default();
    s += &entry.to_string();
    t.set_tinker_log(Some(s));
}

/// `(uint?)source.MaterialType ?? source.WeenieClassId`, for a live source or one this recipe has
/// just consumed (remembered by `CreateDestroyItems` in `RecipeManagerState::consumed_sources`).
fn source_material_or_wcid(w: &World, source: ObjectGuid) -> u32 {
    if let Some(s) = w.objects.get(source) {
        return material_or_wcid(s);
    }
    *w.recipe_manager
        .consumed_sources
        .get(&source)
        .expect("ACE: the tinkering source is read after it is destroyed")
}

fn material_or_wcid(s: &WorldObject) -> u32 {
    s.material_type().map_or(s.biota.weenie_class_id, |m| m.0)
}

// ACE: RecipeManager.MaterialDualDID
/// The portal dat's `DualDidMapper` of material names (a `public static uint` nothing modifies).
pub const MATERIAL_DUAL_DID: u32 = 0x2700_0000;

// ACE: RecipeManager.GetMaterialName
/// The client's name for a material (`_` as spaces), or the enum name when the dat has none.
#[must_use]
pub fn get_material_name(w: &World, material_type: MaterialType) -> String {
    let dual_dids = w
        .dats
        .portal_dat()
        .read_from_dat::<empyrean_dat::file_types::DualDidMapper>(MATERIAL_DUAL_DID);

    let material_name = dual_dids.as_ref().and_then(|m| {
        m.0.enum_to_name
            .iter()
            .find(|(k, _)| *k == material_type.0)
            .map(|(_, v)| v.clone())
    });

    let Some(material_name) = material_name else {
        log::error!(
            "RecipeManager.GetMaterialName({}): couldn't find material name",
            material_type.to_dotnet_string()
        );
        return material_type.to_dotnet_string();
    };
    material_name.replace('_', " ")
}

// ACE: RecipeManager.foolproofTinkers
/// todo: remove this once foolproof salvage recipes are updated
const FOOLPROOF_TINKERS: [u32; 26] = [
    // rare foolproof
    wcid(WeenieClassName::W_MATERIALRAREFOOLPROOFAQUAMARINE_CLASS),
    wcid(WeenieClassName::W_MATERIALRAREFOOLPROOFBLACKGARNET_CLASS),
    wcid(WeenieClassName::W_MATERIALRAREFOOLPROOFBLACKOPAL_CLASS),
    wcid(WeenieClassName::W_MATERIALRAREFOOLPROOFEMERALD_CLASS),
    wcid(WeenieClassName::W_MATERIALRAREFOOLPROOFFIREOPAL_CLASS),
    wcid(WeenieClassName::W_MATERIALRAREFOOLPROOFIMPERIALTOPAZ_CLASS),
    wcid(WeenieClassName::W_MATERIALRAREFOOLPROOFJET_CLASS),
    wcid(WeenieClassName::W_MATERIALRAREFOOLPROOFPERIDOT_CLASS),
    wcid(WeenieClassName::W_MATERIALRAREFOOLPROOFREDGARNET_CLASS),
    wcid(WeenieClassName::W_MATERIALRAREFOOLPROOFSUNSTONE_CLASS),
    wcid(WeenieClassName::W_MATERIALRAREFOOLPROOFWHITESAPPHIRE_CLASS),
    wcid(WeenieClassName::W_MATERIALRAREFOOLPROOFYELLOWTOPAZ_CLASS),
    wcid(WeenieClassName::W_MATERIALRAREFOOLPROOFZIRCON_CLASS),
    // regular foolproof
    wcid(WeenieClassName::W_MATERIALACE36619FOOLPROOFAQUAMARINE),
    wcid(WeenieClassName::W_MATERIALACE36620FOOLPROOFBLACKGARNET),
    wcid(WeenieClassName::W_MATERIALACE36621FOOLPROOFBLACKOPAL),
    wcid(WeenieClassName::W_MATERIALACE36622FOOLPROOFEMERALD),
    wcid(WeenieClassName::W_MATERIALACE36623FOOLPROOFFIREOPAL),
    wcid(WeenieClassName::W_MATERIALACE36624FOOLPROOFIMPERIALTOPAZ),
    wcid(WeenieClassName::W_MATERIALACE36625FOOLPROOFJET),
    wcid(WeenieClassName::W_MATERIALACE36626FOOLPROOFREDGARNET),
    wcid(WeenieClassName::W_MATERIALACE36627FOOLPROOFSUNSTONE),
    wcid(WeenieClassName::W_MATERIALACE36628FOOLPROOFWHITESAPPHIRE),
    wcid(WeenieClassName::W_MATERIALACE36634FOOLPROOFPERIDOT),
    wcid(WeenieClassName::W_MATERIALACE36635FOOLPROOFYELLOWTOPAZ),
    wcid(WeenieClassName::W_MATERIALACE36636FOOLPROOFZIRCON),
];

/// `(uint)WeenieClassName.X`.
#[must_use]
pub(crate) const fn wcid(name: WeenieClassName) -> u32 {
    name.0 as u32
}

// ================================================================================ RecipeExtensions

// ACE: RecipeExtensions.IsTinkering
#[must_use]
pub fn is_tinkering(recipe: &Recipe) -> bool {
    recipe.salvage_type > 0
}

// ACE: RecipeExtensions.IsImbuing
#[must_use]
pub fn is_imbuing(recipe: &Recipe) -> bool {
    recipe.salvage_type == 2
}

// ================================================================================ shims

/// Callees ported in other files, by ACE's names.
pub mod shims {
    use empyrean_entity::enums::{Skill, WeenieError};
    use empyrean_entity::ObjectGuid;

    use super::{obj, World};

    /// `Player.SendUseDoneEvent(errorType)` (`Player_Use.cs`).
    pub fn send_use_done_event(w: &mut World, player: ObjectGuid, error_type: WeenieError) {
        crate::world_objects::player_use::send_use_done_event(w, player, error_type);
    }

    /// SHIM: `WorldObject.GetNameWithMaterial(stackSize)` (`WorldObject_Properties.cs`), over this
    /// file's `RecipeManager.GetMaterialName`.
    #[must_use]
    pub fn get_name_with_material(w: &World, item: ObjectGuid, stack_size: Option<i32>) -> String {
        let mut name = if stack_size.is_some_and(|s| s != 1) {
            crate::world_objects::world_object::get_plural_name(w, item)
        } else {
            super::name(w, item)
        };

        let Some(material_type) = obj(w, item).material_type() else {
            return name;
        };

        let material = super::get_material_name(w, material_type);

        if name.contains(&material) {
            name = name.replace(&material, "");
        }

        format!("{material} {name}")
    }

    /// `Skill.ToSentence()` (`SkillExtensions`, empyrean-entity's port).
    #[must_use]
    pub fn to_sentence(skill: Skill) -> String {
        skill.to_sentence()
    }

    /// `Proficiency.OnSuccessUse(player, skill, difficulty)` (`Entity/Proficiency.cs`).
    pub fn proficiency_on_success_use(
        w: &mut World,
        player: ObjectGuid,
        skill: Skill,
        difficulty: u32,
    ) {
        let skill = crate::entity::proficiency::get_creature_skill(w, player, skill);
        crate::entity::proficiency::on_success_use(w, player, skill, difficulty);
    }

    /// SHIM: `WorldObjectFactory.CreateNewWorldObject(wcid)`, put in `World.objects`.
    pub fn create_new_world_object(w: &mut World, weenie_class_id: u32) -> Option<ObjectGuid> {
        let o =
            crate::factories::player_factory::create_new_world_object_by_wcid(w, weenie_class_id)?;
        let guid = o.guid;
        assert!(
            w.objects.insert(o).is_ok(),
            "a new dynamic guid 0x{:08X} is already live",
            guid.full()
        );
        crate::world_objects::creature::post_insert(w, guid);
        Some(guid)
    }
}
