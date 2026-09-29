// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Aetheria.cs
//! Port of `Source/ACE.Server/Entity/Aetheria.cs`.
//!
//! The coalesced aetheria and its mana stone: bathing one in the stone reveals a random sigil (an
//! equipment set, icon and equip slot by colour) with a random surge spell.

use empyrean_common::dotnet::CsCast;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::{
    ChatMessageType, CombatMode, EquipMask, EquipmentSet, MotionCommand, PropertyBool,
    PropertyDataId, PropertyInt, PropertyString, SpellId, WeenieError,
};
use empyrean_entity::ObjectGuid;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::world_objects::player_inventory::{find_object, SearchLocations};
use crate::world_objects::player_use::send_use_done_event;
use crate::World;

/// ACE enum `Sigil` (declared in ACE.Server), underlying `int`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(i32)]
pub enum Sigil {
    /// Increased damage resistance rating
    Defense = 0,
    /// Increased damage rating
    Destruction = 1,
    /// Increased critical damage rating
    Fury = 2,
    /// Increased healing rating
    Growth = 3,
    /// Vital regeneration spells
    Vigor = 4,
}

impl Sigil {
    /// `(Sigil)value`; `None` for a value no member has (ACE's dictionaries then throw).
    #[must_use]
    pub fn from_i32(value: i32) -> Option<Self> {
        match value {
            0 => Some(Sigil::Defense),
            1 => Some(Sigil::Destruction),
            2 => Some(Sigil::Fury),
            3 => Some(Sigil::Growth),
            4 => Some(Sigil::Vigor),
            _ => None,
        }
    }
}

/// ACE enum `Surge` (declared in ACE.Server), underlying `int`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(i32)]
pub enum Surge {
    Destruction = 0,
    Protection = 1,
    Regeneration = 2,
    Affliction = 3,
    Festering = 4,
}

/// ACE enum `AetheriaColor` (declared in ACE.Server), underlying `int`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(i32)]
pub enum AetheriaColor {
    Blue = 0,
    Yellow = 1,
    Red = 2,
}

impl AetheriaColor {
    /// `(AetheriaColor)value`; `None` for a value no member has.
    #[must_use]
    pub fn from_i32(value: i32) -> Option<Self> {
        match value {
            0 => Some(AetheriaColor::Blue),
            1 => Some(AetheriaColor::Yellow),
            2 => Some(AetheriaColor::Red),
            _ => None,
        }
    }
}

// ACE: Aetheria.AetheriaBlue
pub const AETHERIA_BLUE: u32 = 42635;
// ACE: Aetheria.AetheriaRed
pub const AETHERIA_RED: u32 = 42636;
// ACE: Aetheria.AetheriaYellow
pub const AETHERIA_YELLOW: u32 = 42637;

// ACE: Aetheria.AetheriaManaStone
pub const AETHERIA_MANA_STONE: u32 = 42645;

// ACE: Aetheria.Icons, Aetheria.Aetheria
/// `Icons[color][sigil]`, as the static constructor fills it.
#[must_use]
pub fn icons(color: AetheriaColor, sigil: Sigil) -> u32 {
    match (color, sigil) {
        (AetheriaColor::Blue, Sigil::Defense) => 0x0600_6BF2,
        (AetheriaColor::Blue, Sigil::Destruction) => 0x0600_6BFE,
        (AetheriaColor::Blue, Sigil::Fury) => 0x0600_6BFF,
        (AetheriaColor::Blue, Sigil::Growth) => 0x0600_6C00,
        (AetheriaColor::Blue, Sigil::Vigor) => 0x0600_6C01,

        (AetheriaColor::Yellow, Sigil::Defense) => 0x0600_6C06,
        (AetheriaColor::Yellow, Sigil::Destruction) => 0x0600_6C07,
        (AetheriaColor::Yellow, Sigil::Fury) => 0x0600_6BF3,
        (AetheriaColor::Yellow, Sigil::Growth) => 0x0600_6C08,
        (AetheriaColor::Yellow, Sigil::Vigor) => 0x0600_6BFD,

        (AetheriaColor::Red, Sigil::Defense) => 0x0600_6C02,
        (AetheriaColor::Red, Sigil::Destruction) => 0x0600_6C03,
        (AetheriaColor::Red, Sigil::Fury) => 0x0600_6C04,
        (AetheriaColor::Red, Sigil::Growth) => 0x0600_6BF4,
        (AetheriaColor::Red, Sigil::Vigor) => 0x0600_6C05,
    }
}

// ACE: Aetheria.IsAetheria
/// Whether `wcid` is one of the three coalesced aetheria weenies.
#[must_use]
pub fn is_aetheria(wcid: u32) -> bool {
    wcid == AETHERIA_BLUE || wcid == AETHERIA_YELLOW || wcid == AETHERIA_RED
}

// ACE: Aetheria.GetColor
/// The colour of a coalesced aetheria weenie, or `None`.
#[must_use]
pub fn get_color(wcid: u32) -> Option<AetheriaColor> {
    match wcid {
        AETHERIA_BLUE => Some(AetheriaColor::Blue),
        AETHERIA_YELLOW => Some(AetheriaColor::Yellow),
        AETHERIA_RED => Some(AetheriaColor::Red),
        _ => None,
    }
}

fn name(w: &World, wo: ObjectGuid) -> String {
    crate::dispatch::name::name(w, wo).unwrap_or_default()
}

// ACE: Aetheria.UseObjectOnTarget
/// The player uses an aetheria mana stone on a piece of coalesced aetheria.
pub fn use_object_on_target(
    w: &mut World,
    player: ObjectGuid,
    source: ObjectGuid,
    target: ObjectGuid,
) {
    //Console.WriteLine($"Aetheria.UseObjectOnTarget({player.Name}, {source.Name}, {target.Name})");

    if crate::world_objects::player_magic::is_busy(w, player) {
        send_use_done_event(w, player, WeenieError::YoureTooBusy);
        return;
    }

    // verify use requirements
    let use_error = verify_use_requirements(w, player, source, target);
    if use_error != WeenieError::None {
        send_use_done_event(w, player, use_error);
        return;
    }

    let mut anim_time = 0.0f32;

    let mut action_chain = ActionChain::new();

    crate::world_objects::player_magic::set_is_busy(w, player, true);

    // handle switching to peace mode
    if crate::world_objects::creature_combat::combat_mode(w, player) != CombatMode::NonCombat {
        let stance_time = crate::world_objects::creature_combat::set_combat_mode(
            w,
            player,
            CombatMode::NonCombat,
        );
        action_chain.add_delay_seconds(w, f64::from(stance_time));

        anim_time += stance_time;
    }

    // perform clapping motion
    anim_time += crate::world_objects::world_object_networking::enqueue_motion(
        w,
        player,
        &mut action_chain,
        MotionCommand::ClapHands,
        1.0,
        true,
        None,
        false,
        false,
    );

    action_chain.add_action(Actor::Object(player), move |w: &mut World| {
        // re-verify
        let use_error = verify_use_requirements(w, player, source, target);
        if use_error != WeenieError::None {
            send_use_done_event(w, player, use_error);
            return;
        }

        activate_sigil(w, player, source, target);
    });

    crate::world_objects::world_object_networking::enqueue_motion(
        w,
        player,
        &mut action_chain,
        MotionCommand::Ready,
        1.0,
        true,
        None,
        false,
        false,
    );

    action_chain.add_action(Actor::Object(player), move |w: &mut World| {
        crate::world_objects::player_magic::set_is_busy(w, player, false)
    });

    action_chain.enqueue_chain(w);

    let next = w.now.utc.add_seconds(f64::from(anim_time));
    crate::world_objects::player_combat::set_next_use_time(w, player, next);
}

// ACE: Aetheria.VerifyUseRequirements
pub fn verify_use_requirements(
    w: &mut World,
    player: ObjectGuid,
    source: ObjectGuid,
    target: ObjectGuid,
) -> WeenieError {
    use crate::world_objects::player_networking::send_transient_error;

    if source == target {
        let msg = format!("You can't use the {} on itself.", name(w, source));
        send_transient_error(w, player, &msg);
        return WeenieError::YouDoNotPassCraftingRequirements;
    }

    // ensure both source and target are in player's inventory
    if find_object(w, player, source, SearchLocations::MyInventory)
        .result
        .is_none()
    {
        return WeenieError::YouDoNotPassCraftingRequirements;
    }

    if find_object(w, player, target, SearchLocations::MyInventory)
        .result
        .is_none()
    {
        return WeenieError::YouDoNotPassCraftingRequirements;
    }

    let source_wcid = w
        .objects
        .get(source)
        .expect("ACE: source is null")
        .weenie_class_id();
    let target_wcid = w
        .objects
        .get(target)
        .expect("ACE: target is null")
        .weenie_class_id();
    if source_wcid != AETHERIA_MANA_STONE
        || target_wcid != AETHERIA_BLUE
            && target_wcid != AETHERIA_YELLOW
            && target_wcid != AETHERIA_RED
    {
        return WeenieError::YouDoNotPassCraftingRequirements;
    }

    if name(w, target) != "Coalesced Aetheria" {
        let msg = format!(
            "You can't use the {} on {} because the sigil is already visible.",
            name(w, source),
            name(w, target)
        );
        send_transient_error(w, player, &msg);
        return WeenieError::YouDoNotPassCraftingRequirements;
    }

    WeenieError::None
}

// ACE: Aetheria.ActivateSigil
/// Reveals a random sigil (its equipment set and icon) and a random surge spell on the aetheria,
/// sets the colour's equip slot, and renames it.
///
/// # Panics
/// For a target that is not a coloured aetheria (ACE: `GetColor(...).Value`).
pub fn activate_sigil(w: &mut World, player: ObjectGuid, _source: ObjectGuid, target: ObjectGuid) {
    use crate::world_objects::player_properties::update_property_string;

    // rng select a sigil / spell set
    let rand_sigil =
        Sigil::from_i32(ThreadSafeRandom::next(0, 4)).expect("ThreadSafeRandom.Next(0, 4)");

    let equipment_set = sigil_to_equipment_set(rand_sigil);
    let o = w.objects.get_mut(target).expect("ACE: target is null");
    o.set_property(PropertyInt::EquipmentSetId, equipment_set.0);

    // change icon
    let color = get_color(o.weenie_class_id())
        .expect("ACE: InvalidOperationException (Nullable object must have a value)");
    let icon = icons(color, rand_sigil);
    o.set_property(PropertyDataId::Icon, icon);

    // rng select a surge spell
    let surge_spell = SpellId(ThreadSafeRandom::next(5204, 5208).cs_cast());

    //target.Biota.GetOrAddKnownSpell((int)surgeSpell, target.BiotaDatabaseLock, target.BiotaPropertySpells, out _);

    o.set_property(PropertyDataId::ProcSpell, surge_spell.0.cs_cast());
    //target.SetProperty(PropertyFloat.ProcSpellRate, 0.05f);   // proc rate for aetheria?

    if surge_target_self(surge_spell).expect("ACE: KeyNotFoundException (SurgeTargetSelf)") {
        o.set_property(PropertyBool::ProcSpellSelfTargeted, true);
    }

    // set equip mask
    o.set_property(
        PropertyInt::ValidLocations,
        color_to_mask(color).0.cs_cast(),
    );

    // level?
    let session = crate::managers::player_manager::player_session(w, player)
        .expect("ACE: player.Session is null (NullReferenceException)");
    let msg =
        crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat(
            "A sigil rises to the surface as you bathe the aetheria in mana.",
            ChatMessageType::Broadcast,
        );
    crate::network::game_messages::game_message::enqueue_send(w, session, msg);

    update_property_string(
        w,
        player,
        target,
        PropertyString::Name,
        Some("Aetheria"),
        false,
    );
    update_property_string(
        w,
        player,
        target,
        PropertyString::LongDesc,
        Some("This aetheria's sigil now shows on the surface."),
        false,
    );
    let msg = crate::network::game_messages::messages::game_message_update_object::game_message_update_object(w, target, false, false);
    crate::network::game_messages::game_message::enqueue_send(w, session, msg);

    crate::dispatch::save_biota_to_database::save_biota_to_database(w, target, true);

    send_use_done_event(w, player, WeenieError::None);

    log::debug!(
        "[CRAFTING] {} revealed a {color:?} {rand_sigil:?} with a surge of {surge_spell:?} on {}:0x{:08X}",
        name(w, player),
        name(w, target),
        target.full()
    );
}

// ACE: Aetheria.SigilToEquipmentSet
#[must_use]
pub fn sigil_to_equipment_set(sigil: Sigil) -> EquipmentSet {
    match sigil {
        Sigil::Defense => EquipmentSet::AetheriaDefense,
        Sigil::Destruction => EquipmentSet::AetheriaDestruction,
        Sigil::Fury => EquipmentSet::AetheriaFury,
        Sigil::Growth => EquipmentSet::AetheriaGrowth,
        Sigil::Vigor => EquipmentSet::AetheriaVigor,
    }
}

// ACE: Aetheria.ColorToMask
#[must_use]
pub fn color_to_mask(color: AetheriaColor) -> EquipMask {
    match color {
        AetheriaColor::Blue => EquipMask::SigilOne,
        AetheriaColor::Yellow => EquipMask::SigilTwo,
        AetheriaColor::Red => EquipMask::SigilThree,
    }
}

// ACE: Aetheria.SurgeTargetSelf
/// `SurgeTargetSelf[spell]`: `None` for a spell that is not a surge (ACE's
/// `KeyNotFoundException`).
#[must_use]
pub fn surge_target_self(spell: SpellId) -> Option<bool> {
    match spell {
        SpellId::AetheriaProcDamageBoost => Some(true),
        SpellId::AetheriaProcDamageOverTime => Some(false),
        SpellId::AetheriaProcDamageReduction => Some(true),
        SpellId::AetheriaProcHealDebuff => Some(false),
        SpellId::AetheriaProcHealthOverTime => Some(true),
        _ => None,
    }
}

/// `WorldObject.ItemLevel` (`WorldObject_Set.cs`): the level from `ItemTotalXp`, or null
/// without `HasItemLevel`.
fn item_level(o: &crate::world_objects::world_object::WorldObject) -> Option<i32> {
    // `HasItemLevel`: ItemBaseXp > 0, ItemMaxLevel > 0, ItemXpStyle > 0
    let base_xp = o.item_base_xp().filter(|&x| x > 0)?;
    let max_level = o.item_max_level().filter(|&x| x > 0)?;
    let xp_style = o.item_xp_style().filter(|s| s.0 > 0)?;
    let total_xp = o
        .item_total_xp()
        .expect("ACE: ItemTotalXp is null (InvalidOperationException)");
    Some(crate::entity::experience_system::item_total_xp_to_level(
        total_xp.cs_cast(),
        base_xp.cs_cast(),
        max_level,
        xp_style,
    ))
}

// ACE: Aetheria.CalcProcRate
/// About 1% per item level, +0.1% per luminance surge augmentation of a player wielder, doubled
/// in magic combat mode and x1.5 in missile.
#[must_use]
pub fn calc_proc_rate(w: &World, aetheria: ObjectGuid, wielder: ObjectGuid) -> f32 {
    // ~1% base rate per level?
    let level: f32 = item_level(w.objects.get(aetheria).expect("ACE: aetheria is null"))
        .unwrap_or(0)
        .cs_cast();
    let mut proc_rate = level * 0.01f32;

    let wo = w.objects.get(wielder).expect("ACE: wielder is null");
    if wo.is_player() {
        // +0.1% per luminance aug?
        let rating: f32 = wo.lum_aug_surge_chance_rating().cs_cast();
        let aug_bonus = rating * 0.001f32;
        proc_rate += aug_bonus;
    }

    // The proc rates depend on the attack type. Magic is best, then missile is slightly lower, then Melee is slightly lower than missile.
    match crate::world_objects::creature_combat::combat_mode(w, wielder) {
        CombatMode::Magic => proc_rate *= 2.0,
        CombatMode::Missile => proc_rate *= 1.5,
        _ => {}
    }
    // It is unconfirmed, but believed, that the act of being hit or attacked increases the chances of a surge triggering.
    proc_rate
}

// ACE: Aetheria.IsAetheriaManaStone
/// Returns TRUE if wo is AetheriaManaStone.
#[must_use]
pub fn is_aetheria_mana_stone(wo: &crate::world_objects::world_object::WorldObject) -> bool {
    wo.biota.weenie_class_id == AETHERIA_MANA_STONE
}
