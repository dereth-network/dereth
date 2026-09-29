// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Spells.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Spells.cs`.
//!
//! The spellbook (known spells, learning with and without networking, removal), the spellbook
//! filters and component fill levels, the equipment-set spells, the Sentinel buff command, foci,
//! and the spell hooks that refresh max vitals and run rate.
//!
//! Members are free functions `(w, this, ..)` over a Player with a session. Members of classes
//! ported elsewhere are pointers at the bottom of this file, named after ACE's member.

use std::collections::BTreeSet;

use empyrean_entity::enums::properties::PropertyAttribute2nd;
use empyrean_entity::enums::{
    AccessLevel, ChatMessageType, EnchantmentMask, EquipmentSet, MagicSchool, PlayScript,
    SpellBookFilterOptions, SpellId, WeenieType,
};
use empyrean_entity::models::PropertiesEnchantmentRegistry;
use empyrean_entity::ObjectGuid;
use empyrean_net::SessionId;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::spell::Spell;
use crate::managers::property_manager;
use crate::network::game_event::events::game_event_communication_transient_string::game_event_communication_transient_string;
use crate::network::game_event::events::game_event_magic_remove_spell::game_event_magic_remove_spell;
use crate::network::game_event::events::game_event_magic_update_enchantment::game_event_magic_update_enchantment;
use crate::network::game_event::events::game_event_magic_update_spell::game_event_magic_update_spell;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_messages::game_message::{enqueue_send, GameMessage};
use crate::network::game_messages::messages::game_message_private_update_attribute2nd_level::game_message_private_update_attribute2nd_level;
use crate::network::game_messages::messages::game_message_script::game_message_script;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::network::structure::enchantment::{
    enchantment_from_registry, enchantment_new, Enchantment,
};
use crate::world_objects::managers::enchantment_manager as em;
use crate::world_objects::managers::enchantment_manager_with_caching as emc;
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::{container, creature_equipment, world_object_networking};
use crate::World;

/// Non-property fields declared in `Player_Spells.cs`.
#[derive(Debug, Default)]
pub struct PlayerSpellsFields {}

fn obj(w: &World, g: ObjectGuid) -> &WorldObject {
    w.objects
        .get(g)
        .unwrap_or_else(|| panic!("ACE: {g:?} is null (NullReferenceException)"))
}

fn obj_mut(w: &mut World, g: ObjectGuid) -> &mut WorldObject {
    w.objects
        .get_mut(g)
        .unwrap_or_else(|| panic!("ACE: {g:?} is null (NullReferenceException)"))
}

fn name_of(w: &World, g: ObjectGuid) -> String {
    crate::dispatch::name::name(w, g).unwrap_or_default()
}

/// `Session`: ACE's `Session.Network` throws for a player without one.
fn session_of(w: &World, this: ObjectGuid) -> SessionId {
    crate::managers::player_manager::player_session(w, this)
        .expect("ACE: Player.Session is null (NullReferenceException)")
}

fn send(w: &mut World, this: ObjectGuid, msg: GameMessage) {
    let s = session_of(w, this);
    enqueue_send(w, s, msg);
}

/// `ChangesDetected = true` (`WorldObject_Database.cs`).
fn set_changes_detected(w: &mut World, this: ObjectGuid) {
    obj_mut(w, this).wo.world_object_database.changes_detected = true;
}

// ACE: Player.SpellIsKnown
#[must_use]
pub fn spell_is_known(w: &World, this: ObjectGuid, spell_id: u32) -> bool {
    obj(w, this).biota.spell_is_known(spell_id.cast_signed())
}

/// Will return true if the spell was added, or false if the spell already exists.
// ACE: Player.AddKnownSpell
pub fn add_known_spell(w: &mut World, this: ObjectGuid, spell_id: u32) -> bool {
    // `GetOrAddKnownSpell(spell, lock, out added)`: the probability defaults to 2.0f
    let (_, spell_added) = obj_mut(w, this)
        .biota
        .get_or_add_known_spell(spell_id.cast_signed(), 2.0);

    if spell_added {
        set_changes_detected(w, this);
    }

    spell_added
}

/// Removes a known spell from the player's spellbook.
// ACE: Player.RemoveKnownSpell
pub fn remove_known_spell(w: &mut World, this: ObjectGuid, spell_id: u32) -> bool {
    obj_mut(w, this)
        .biota
        .try_remove_known_spell(spell_id.cast_signed())
}

/// Adds the spell to the spellbook and tells the client. With `ui_output` (ACE's default): a chat
/// line and the purple level-up effect; otherwise a transient "You have learned a new spell.".
// ACE: Player.LearnSpellWithNetworking
pub fn learn_spell_with_networking(
    w: &mut World,
    this: ObjectGuid,
    spell_id: u32,
    ui_output: bool,
) {
    let spell_name = crate::entity::spell_formula::spell_table(w)
        .spells
        .get(&spell_id)
        .map(|s| s.name.clone());

    let Some(spell_name) = spell_name else {
        let error_message = game_message_system_chat(
            "SpellID not found in Spell Table",
            ChatMessageType::Broadcast,
        );
        send(w, this, error_message);
        return;
    };

    if !add_known_spell(w, this, spell_id) {
        if ui_output {
            let error_message = game_message_system_chat(
                "You already know that spell!",
                ChatMessageType::Broadcast,
            );
            send(w, this, error_message);
        }
        return;
    }

    let s = session_of(w, this);
    #[allow(clippy::cast_possible_truncation)] // `(ushort)spellId`
    let update_spell_event = game_event_magic_update_spell(session_data(w, s), spell_id as u16, 0);
    enqueue_send(w, s, update_spell_event);

    // Check to see if we echo output to the client text area and do playscript animation
    if ui_output {
        // Always seems to be this SkillUpPurple effect
        crate::world_objects::world_object::apply_visual_effects(
            w,
            this,
            PlayScript::SkillUpPurple,
            1.0,
        );

        let message = format!("You learn the {spell_name} spell.\n");
        let learn_message = game_message_system_chat(&message, ChatMessageType::Broadcast);
        send(w, this, learn_message);
    } else {
        let m = game_event_communication_transient_string(
            session_data(w, s),
            "You have learned a new spell.",
        );
        enqueue_send(w, s, m);
    }
}

/// Learns spells in bulk, without notification, filtered by school and level
/// (`with_networking` defaults to true).
// ACE: Player.LearnSpellsInBulk
pub fn learn_spells_in_bulk(
    w: &mut World,
    this: ObjectGuid,
    school: MagicSchool,
    spell_level: u32,
    with_networking: bool,
) {
    for spell_id in player_player_spell_table() {
        if !crate::entity::spell_formula::spell_table(w)
            .spells
            .contains_key(&spell_id)
        {
            log::info!("Unknown spell ID in PlayerSpellID table: {spell_id}");
            continue;
        }
        let spell = Spell::new(w, spell_id, false);
        if spell.school() == school && spell.formula_ref().level() == spell_level {
            if with_networking {
                learn_spell_with_networking(w, this, spell.id(), false);
            } else {
                add_known_spell(w, this, spell.id());
            }
        }
    }
}

// ACE: Player.HandleActionMagicRemoveSpellId
pub fn handle_action_magic_remove_spell_id(w: &mut World, this: ObjectGuid, spell_id: u32) {
    if !obj_mut(w, this)
        .biota
        .try_remove_known_spell(spell_id.cast_signed())
    {
        log::error!("Invalid spellId passed to Player.RemoveSpellFromSpellBook");
        return;
    }

    set_changes_detected(w, this);

    let s = session_of(w, this);
    #[allow(clippy::cast_possible_truncation)] // `(ushort)spellId`
    let remove_spell_event = game_event_magic_remove_spell(session_data(w, s), spell_id as u16, 0);
    enqueue_send(w, s, remove_spell_event);
}

/// The equipped items of `item`'s equipment set (`EquippedObjects.Values.Where(i => i.HasItemSet
/// && i.EquipmentSetId == item.EquipmentSetId)`), in equip order.
fn equipped_set_items(w: &World, this: ObjectGuid, item: ObjectGuid) -> Vec<ObjectGuid> {
    let set_id = obj(w, item).equipment_set_id();
    creature_equipment::equipped_objects_values(w, this)
        .into_iter()
        .filter(|&i| {
            let o = obj(w, i);
            o.equipment_set_id().is_some() && o.equipment_set_id() == set_id
        })
        .collect()
}

// ACE: Player.EquipItemFromSet
pub fn equip_item_from_set(w: &mut World, this: ObjectGuid, item: ObjectGuid) {
    if obj(w, item).equipment_set_id().is_none() {
        return;
    }

    let mut set_items = equipped_set_items(w, this, item);

    let spells = world_object_get_spell_set(w, &set_items, 0);

    // get the spells from before / without this item
    if let Some(i) = set_items.iter().position(|&g| g == item) {
        set_items.remove(i);
    }
    let prev_spells = world_object_get_spell_set(w, &set_items, 0);

    equip_dequip_item_from_set(w, this, item, &spells, &prev_spells, None);
}

/// `Enumerable.Except`: the distinct elements of `a` not in `b`, in `a`'s order (by `Spell.Equals`,
/// the spell id).
fn except(a: &[Spell], b: &[Spell]) -> Vec<Spell> {
    let mut seen = BTreeSet::new();
    let mut out: Vec<Spell> = Vec::new();
    for s in a {
        if !b.iter().any(|x| x.id() == s.id()) && seen.insert(s.id()) {
            out.push(s.clone());
        }
    }
    out
}

// ACE: Player.EquipDequipItemFromSet
pub fn equip_dequip_item_from_set(
    w: &mut World,
    this: ObjectGuid,
    item: ObjectGuid,
    spells: &[Spell],
    prev_spells: &[Spell],
    surrogate_item: Option<ObjectGuid>,
) {
    // compare these 2 spell sets -
    // see which spells are being added, and which are being removed
    let add_spells = except(spells, prev_spells);
    let remove_spells = except(prev_spells, spells);

    // set spells are not affected by mana
    // if it's equipped, it's active.

    for spell in &remove_spells {
        let set_id = obj(w, item)
            .equipment_set_id()
            .expect("ACE: item.EquipmentSetId.Value (InvalidOperationException)");
        let entry = em::get_enchantment_set(w, this, spell.id(), set_id).cloned();
        emc::dispel(w, this, entry.as_ref());
    }

    let add_item = surrogate_item.unwrap_or(item);

    for spell in &add_spells {
        creature_create_item_spell(w, this, add_item, spell.id());
    }
}

// ACE: Player.DequipItemFromSet
pub fn dequip_item_from_set(w: &mut World, this: ObjectGuid, item: ObjectGuid) {
    if obj(w, item).equipment_set_id().is_none() {
        return;
    }

    let mut set_items = equipped_set_items(w, this, item);

    // for better bookkeeping, and to avoid a rarish error with AuditItemSpells detecting -1 duration item enchantments where
    // the CasterGuid is no longer in the player's possession
    let surrogate_item = set_items.last().copied();

    let spells = world_object_get_spell_set(w, &set_items, 0);

    // get the spells from before / with this item
    set_items.push(item);
    let prev_spells = world_object_get_spell_set(w, &set_items, 0);

    if surrogate_item.is_none() {
        let add_spells = except(&spells, &prev_spells);

        if !add_spells.is_empty() {
            log::error!(
                "{}.DequipItemFromSet({}) -- last item in set dequipped, but addSpells still contains {} -- this shouldn't happen!",
                name_of(w, this),
                name_of(w, item),
                add_spells.iter().map(|s| s.name().to_owned()).collect::<Vec<_>>().join(", ")
            );
        }
    }

    equip_dequip_item_from_set(w, this, item, &spells, &prev_spells, surrogate_item);
}

// ACE: Player.OnItemLevelUp
pub fn on_item_level_up(w: &mut World, this: ObjectGuid, item: ObjectGuid, prev_item_level: i32) {
    if obj(w, item).equipment_set_id().is_none() {
        return;
    }

    let set_items = equipped_set_items(w, this, item);

    let level_diff = prev_item_level.wrapping_sub(world_object_item_level(w, item).unwrap_or(0));

    let prev_spells = world_object_get_spell_set(w, &set_items, level_diff);

    let spells = world_object_get_spell_set(w, &set_items, 0);

    equip_dequip_item_from_set(w, this, item, &spells, &prev_spells, None);
}

/// A prepared buff for [`create_sentinel_buff_players`].
// ACE: Player.BuffMessage
#[derive(Debug, Clone)]
struct BuffMessage {
    bane: bool,
    spell: Spell,
    enchantment: Enchantment,
}

impl BuffMessage {
    /// Bakes the target into the enchantment; the session message (`GameEventMagicUpdateEnchantment`)
    /// and `SetLandblockMessage`'s script are built from it when sent.
    // ACE: Player.BuffMessage.SetTargetPlayer
    fn set_target_player(&mut self, p: ObjectGuid) {
        self.enchantment.target = p;
    }

    /// `new GameMessageScript(target, Spell.TargetEffect, 1f)`.
    // ACE: Player.BuffMessage.SetLandblockMessage
    fn set_landblock_message(&self, target: ObjectGuid) -> GameMessage {
        game_message_script(target, self.spell.target_effect(), 1.0)
    }
}

/// The Sentinel buff command: casts every buff of `Buffs` at `max_level` (ACE's default 8; 7 when
/// the level-8 spells are not installed) on each player, silently; banes go on their equipped
/// clothing and shields. `self_` defaults to false.
// ACE: Player.CreateSentinelBuffPlayers
pub fn create_sentinel_buff_players(
    w: &mut World,
    this: ObjectGuid,
    players: &[ObjectGuid],
    self_: bool,
    max_level: u64,
) {
    let s = session_of(w, this);
    if w.sessions
        .get(s)
        .map_or(AccessLevel::Player, |d| d.access_level)
        < AccessLevel::Sentinel
    {
        return;
    }

    let self_or_other = if self_ { "Self" } else { "Other" };

    // ensure level 8s are installed
    let mut max_spell_level = max_level.clamp(1, 8);
    if max_spell_level == 8 && w.content.get_cached_spell(SpellId::ArmorOther8.0).is_none() {
        max_spell_level = 7;
    }

    let mut buff_messages: Vec<BuffMessage> = Vec::new();
    // prepare messages
    let mut buffs_not_implemented_yet: Vec<String> = Vec::new();
    for &spell in BUFFS {
        let mut spell_nam_prefix = spell;
        let mut is_bane = false;
        if let Some(rest) = spell_nam_prefix.strip_prefix('@') {
            is_bane = true;
            spell_nam_prefix = rest;
        }
        let other_self = if self_or_other == "Self" {
            "Other"
        } else {
            "Self"
        };
        let full_spell_enum_name = format!(
            "{spell_nam_prefix}{}{max_spell_level}",
            if is_bane { "" } else { self_or_other }
        );
        let full_spell_enum_name_alt = format!(
            "{spell_nam_prefix}{}{max_spell_level}",
            if is_bane { "" } else { other_self }
        );
        let spell_id = enum_parse_spell_id(&full_spell_enum_name);
        let mut buff_msg = build_buff_message(w, spell_id);

        if buff_msg.is_none() {
            let spell_id = enum_parse_spell_id(&full_spell_enum_name_alt);
            buff_msg = build_buff_message(w, spell_id);
        }

        if let Some(mut buff_msg) = buff_msg {
            buff_msg.bane = is_bane;
            buff_messages.push(buff_msg);
        } else {
            buffs_not_implemented_yet.push(full_spell_enum_name);
        }
    }
    // buff each player
    for &target_player in players {
        if buff_messages.iter().any(|k| !k.bane) {
            // bake player into the messages
            for k in buff_messages.iter_mut().filter(|k| !k.bane) {
                k.set_target_player(target_player);
            }
            // update client-side enchantments
            let ts = session_of(w, target_player);
            let enchantments: Vec<Enchantment> = buff_messages
                .iter()
                .filter(|k| !k.bane)
                .map(|k| k.enchantment.clone())
                .collect();
            for e in &enchantments {
                let m = game_event_magic_update_enchantment(session_data(w, ts), e);
                enqueue_send(w, ts, m);
            }
            // run client-side effect scripts, omitting duplicates
            let mut seen = Vec::new();
            let mut landblock_messages = Vec::new();
            for k in buff_messages.iter().filter(|k| !k.bane) {
                let effect = k.spell.target_effect();
                if !seen.contains(&effect) {
                    seen.push(effect);
                    landblock_messages.push(k.set_landblock_message(target_player));
                }
            }
            let _ = world_object_networking::enqueue_broadcast(
                w,
                target_player,
                true,
                &landblock_messages,
            );
            // update server-side enchantments

            let buffs_for_player: Vec<Spell> = buff_messages
                .iter()
                .filter(|k| !k.bane)
                .map(|k| k.spell.clone())
                .collect();

            for school in [
                MagicSchool::LifeMagic,
                MagicSchool::CreatureEnchantment,
                MagicSchool::ItemEnchantment,
            ] {
                for spl in buffs_for_player.iter().filter(|k| k.school() == school) {
                    create_enchantment_silent(w, this, spl, target_player);
                }
            }
        }
        if buff_messages.iter().any(|k| k.bane) {
            // Impen/bane
            let items = creature_equipment::equipped_objects_values(w, target_player);
            let itembuffs: Vec<Spell> = buff_messages
                .iter()
                .filter(|k| k.bane)
                .map(|k| k.spell.clone())
                .collect();
            for item_buff in &itembuffs {
                for &item in &items {
                    let o = obj(w, item);
                    if (o.biota.weenie_type == WeenieType::Clothing || o.is_shield())
                        && world_object_networking::shims::is_enchantable(w, item)
                    {
                        create_enchantment_silent(w, this, item_buff, item);
                    }
                }
            }
        }
    }
}

// ACE: Player.CreateEnchantmentSilent
fn create_enchantment_silent(w: &mut World, this: ObjectGuid, spell: &Spell, target: ObjectGuid) {
    let add_result = emc::add(w, target, spell, Some(this), None, false, false);

    if obj(w, target).is_player() {
        let entry = add_result
            .enchantment
            .as_ref()
            .expect("ACE: addResult.Enchantment is null (NullReferenceException)");
        let enchantment = enchantment_from_registry(w, target, entry);
        let ts = session_of(w, target);
        let msg = game_event_magic_update_enchantment(session_data(w, ts), &enchantment);
        enqueue_send(w, ts, msg);

        handle_spell_hooks(w, target, spell);
    }
}

/// TODO: switch this over to SpellProgressionTables. `@` indicates impenetrability or a bane.
// ACE: Player.Buffs
const BUFFS: &[&str] = &[
    "Strength",
    "Invulnerability",
    "FireProtection",
    "Armor",
    "Rejuvenation",
    "Regeneration",
    "ManaRenewal",
    "Impregnability",
    "MagicResistance",
    //"AxeMastery",    // light weapons
    "LightWeaponsMastery",
    //"DaggerMastery", // finesse weapons
    "FinesseWeaponsMastery",
    //"MaceMastery",
    //"SpearMastery",
    //"StaffMastery",
    //"SwordMastery",  // heavy weapons
    "HeavyWeaponsMastery",
    //"UnarmedCombatMastery",
    //"BowMastery",    // missile weapons
    "MissileWeaponsMastery",
    //"CrossbowMastery",
    //"ThrownWeaponMastery",
    "AcidProtection",
    "CreatureEnchantmentMastery",
    "ItemEnchantmentMastery",
    "LifeMagicMastery",
    "WarMagicMastery",
    "ManaMastery",
    "ArcaneEnlightenment",
    "ArcanumSalvaging",
    "ArmorExpertise",
    "ItemExpertise",
    "MagicItemExpertise",
    "WeaponExpertise",
    "MonsterAttunement",
    "PersonAttunement",
    "DeceptionMastery",
    "HealingMastery",
    "LeadershipMastery",
    "LockpickMastery",
    "Fealty",
    "JumpingMastery",
    "Sprint",
    "BludgeonProtection",
    "ColdProtection",
    "LightningProtection",
    "BladeProtection",
    "PiercingProtection",
    "Endurance",
    "Coordination",
    "Quickness",
    "Focus",
    "Willpower",
    "CookingMastery",
    "FletchingMastery",
    "AlchemyMastery",
    "VoidMagicMastery",
    "SummoningMastery",
    "SwiftKiller",
    "Defender",
    "BloodDrinker",
    "HeartSeeker",
    "HermeticLink",
    "SpiritDrinker",
    "DualWieldMastery",
    "TwoHandedMastery",
    "DirtyFightingMastery",
    "RecklessnessMastery",
    "SneakAttackMastery",
    "ShieldMastery",
    "@Impenetrability",
    "@PiercingBane",
    "@BludgeonBane",
    "@BladeBane",
    "@AcidBane",
    "@FlameBane",
    "@FrostBane",
    "@LightningBane",
];

/// `(uint)Enum.Parse(typeof(SpellId), name)`: throws `ArgumentException` for an unknown name.
fn enum_parse_spell_id(name: &str) -> u32 {
    SpellId::ALL
        .iter()
        .find(|s| s.to_dotnet_string() == name)
        .map(|s| s.0)
        .unwrap_or_else(|| panic!("ACE: Enum.Parse(SpellId, {name}) (ArgumentException)"))
}

// ACE: Player.BuildBuffMessage
fn build_buff_message(w: &World, spell_id: u32) -> Option<BuffMessage> {
    let spell = Spell::new(w, spell_id, true);
    if spell.not_found() {
        return None;
    }
    let enchantment = enchantment_new(
        w,
        ObjectGuid::default(),
        0,
        spell_id,
        1,
        EnchantmentMask(spell.stat_mod_type().0),
        Some(spell.stat_mod_val()),
    );
    Some(BuffMessage {
        bane: false,
        spell,
        enchantment,
    })
}

// ACE: Player.HandleSpellbookFilters
pub fn handle_spellbook_filters(w: &mut World, this: ObjectGuid, filters: SpellBookFilterOptions) {
    let character = player_character_mut(w, this);
    character.spellbook_filters = filters.0;
}

// ACE: Player.HandleSetDesiredComponentLevel
pub fn handle_set_desired_component_level(
    w: &mut World,
    this: ObjectGuid,
    component_wcid: u32,
    amount: u32,
) {
    // ensure wcid is spell component
    if !spell_component_is_valid(w, component_wcid) {
        log::warn!("{}.HandleSetDesiredComponentLevel({component_wcid}, {amount}): invalid spell component wcid", name_of(w, this));
        return;
    }
    let character = player_character_mut(w, this);
    if amount > 0 {
        let existing = character
            .character_properties_fill_comp_book
            .iter_mut()
            .find(|i| i64::from(i.spell_component_id) == i64::from(component_wcid));

        if let Some(existing) = existing {
            existing.quantity_to_rebuy = amount.cast_signed();
        } else {
            let _ = character.add_fill_component(component_wcid, amount);
        }
    } else {
        let _ = character.try_remove_fill_component(component_wcid);
    }

    set_character_changes_detected(w, this);
}

// ACE: Player.FociWCIDs
pub const FOCI_WCIDS: [(MagicSchool, u32); 5] = [
    (MagicSchool::CreatureEnchantment, 15268), // Foci of Enchantment
    (MagicSchool::ItemEnchantment, 15269),     // Foci of Artifice
    (MagicSchool::LifeMagic, 15270),           // Foci of Verdancy
    (MagicSchool::WarMagic, 15271),            // Foci of Strife
    (MagicSchool::VoidMagic, 43173),           // Foci of Shadow
];

/// The school's infusion augmentation, or a foci of that school in the pack.
///
/// # Panics
/// For a school without foci (ACE: `KeyNotFoundException`).
// ACE: Player.HasFoci
#[must_use]
pub fn has_foci(w: &World, this: ObjectGuid, school: MagicSchool) -> bool {
    let o = obj(w, this);
    match school {
        MagicSchool::CreatureEnchantment if o.augmentation_infused_creature_magic() > 0 => {
            return true
        }
        MagicSchool::ItemEnchantment if o.augmentation_infused_item_magic() > 0 => return true,
        MagicSchool::LifeMagic if o.augmentation_infused_life_magic() > 0 => return true,
        MagicSchool::VoidMagic if o.augmentation_infused_void_magic() > 0 => return true,
        MagicSchool::WarMagic if o.augmentation_infused_war_magic() > 0 => return true,
        _ => {}
    }

    let wcid = FOCI_WCIDS
        .iter()
        .find(|(s, _)| *s == school)
        .map(|(_, wcid)| *wcid)
        .unwrap_or_else(|| {
            panic!(
                "ACE: FociWCIDs[{}] (KeyNotFoundException)",
                school.to_dotnet_string()
            )
        });
    container::inventory_values(w, this)
        .into_iter()
        .any(|i| obj(w, i).biota.weenie_class_id == wcid)
}

/// Called when an enchantment is added or removed: the max-vital refresh, and (with the
/// `runrate_add_hooks` option) the run rate.
// ACE: Player.HandleSpellHooks
pub fn handle_spell_hooks(w: &mut World, this: ObjectGuid, spell: &Spell) {
    handle_max_vital_update(w, this, spell);

    // unsure if spell hook was here in retail,
    // but this has the potential to take the client out of autorun mode
    // which causes them to stop if they hit a turn key afterwards
    if property_manager::get_bool(w, "runrate_add_hooks", false, true).item {
        let _ = handle_run_rate_update_spell(w, this, spell);
    }
}

/// Called when an enchantment is added or removed, checks if the spell affects the max vitals,
/// and if so, updates the client immediately (after 1 s: the client needs time for primary
/// attribute updates).
// ACE: Player.HandleMaxVitalUpdate
pub fn handle_max_vital_update(w: &mut World, this: ObjectGuid, spell: &Spell) {
    let max_vitals: Vec<PropertyAttribute2nd> = spell.updates_max_vitals();

    if max_vitals.is_empty() {
        return;
    }

    let mut action_chain = ActionChain::new();
    action_chain.add_delay_seconds(w, f64::from(1.0f32)); // client needs time for primary attribute updates
    action_chain.add_action(Actor::Object(this), move |w| {
        for &max_vital in &max_vitals {
            let player_vital = *obj(w, this)
                .vitals()
                .get(&max_vital)
                .expect("ACE: Vitals[maxVital] (KeyNotFoundException)");
            let current = player_vital.current(obj(w, this));
            let msg = game_message_private_update_attribute2nd_level(
                obj_mut(w, this),
                player_vital.to_enum(),
                current,
            );
            send(w, this, msg);
        }
    });
    action_chain.enqueue_chain(w);
}

/// The `HandleRunRateUpdate(Spell)` overload: only for a spell that updates the run rate.
// ACE: Player.HandleRunRateUpdate
pub fn handle_run_rate_update_spell(w: &mut World, this: ObjectGuid, spell: &Spell) -> bool {
    if !spell.updates_run_rate() {
        return false;
    }

    player_handle_run_rate_update(w, this)
}

/// Cleans up bugged chars with dangling item set spells from previous bugs: removes item
/// enchantments (duration -1, not vitae) whose caster is no longer equipped (possessed, for a set
/// spell), and set spells no longer active for the equipped set.
// ACE: Player.AuditItemSpells
pub fn audit_item_spells(w: &mut World, this: ObjectGuid) {
    // cleans up bugged chars with dangling item set spells
    // from previous bugs

    let all_possessions: Vec<ObjectGuid> = player_get_all_possessions(w, this);

    // this is a legacy method, but is still a decent failsafe to catch any existing issues

    // get active item enchantments
    #[allow(clippy::float_cmp)]
    let enchantments: Vec<PropertiesEnchantmentRegistry> = obj(w, this)
        .biota
        .properties_enchantment_registry
        .as_ref()
        .map(|r| {
            r.iter()
                .filter(|i| i.duration == -1.0 && i.spell_id != SpellId::Vitae.0.cast_signed())
                .cloned()
                .collect()
        })
        .unwrap_or_default();

    for enchantment in &enchantments {
        let caster = ObjectGuid::new(enchantment.caster_object_id);
        let in_table = if enchantment.has_spell_set_id {
            all_possessions.contains(&caster)
        } else {
            creature_equipment::equipped_objects_values(w, this).contains(&caster)
        };

        // if this item is not equipped, remove enchantment
        if !in_table {
            let spell = Spell::new(w, enchantment.spell_id.cast_unsigned(), false);
            log::error!(
                "{}.AuditItemSpells(): removing spell {} from {} item",
                name_of(w, this),
                spell.name(),
                if enchantment.has_spell_set_id {
                    "non-possessed"
                } else {
                    "non-equipped"
                }
            );

            emc::dispel(w, this, Some(enchantment));
            continue;
        }
        let item = caster;

        // is this item part of a set?
        let Some(item_set_id) = obj(w, item).equipment_set_id() else {
            continue;
        };

        // get all of the equipped items in this set
        let set_items = equipped_set_items(w, this, item);

        // get all of the spells currently active from this set
        let current_spells = world_object_get_spell_set(w, &set_items, 0);

        // get all of the spells possible for this item set
        let possible_spells = world_object_get_spell_set_all(w, item_set_id);

        // get the difference between them
        let inactive_spells = except(&possible_spells, &current_spells);

        // remove any item set spells that shouldn't be active
        for inactive_spell in &inactive_spells {
            let remove_spells: Vec<PropertiesEnchantmentRegistry> = enchantments
                .iter()
                .filter(|i| {
                    i.spell_set_id == item_set_id
                        && i.spell_id.cast_unsigned() == inactive_spell.id()
                })
                .cloned()
                .collect();

            for remove_spell in &remove_spells {
                log::error!(
                    "{}.AuditItemSpells(): removing spell {} from {}",
                    name_of(w, this),
                    inactive_spell.name(),
                    item_set_id.to_dotnet_string()
                );

                emc::dispel(w, this, Some(remove_spell));
            }
        }
    }
}

// ------------------------------------------------------------------ pointers (members ported elsewhere)

/// `Player.Character` (the shard `Character`, a `Player.cs` field).
fn player_character_mut(
    w: &mut World,
    this: ObjectGuid,
) -> &mut empyrean_store::models::shard::Character {
    obj_mut(w, this)
        .player
        .as_mut()
        .and_then(|p| p.player.character.as_mut())
        .expect("ACE: Player.Character is null (NullReferenceException)")
}

/// `CharacterChangesDetected = true` (`Player_Database.cs`).
fn set_character_changes_detected(w: &mut World, this: ObjectGuid) {
    obj_mut(w, this)
        .player
        .as_mut()
        .expect("a player")
        .player_database
        .character_changes_detected = true;
}

/// `SpellComponent.IsValid(wcid)` (`SpellComponent.cs`).
fn spell_component_is_valid(w: &World, wcid: u32) -> bool {
    crate::world_objects::spell_component::is_valid(w, wcid)
}

/// `Creature.CreateItemSpell(item, spellId)` (`Creature_Magic.cs`).
fn creature_create_item_spell(w: &mut World, this: ObjectGuid, item: ObjectGuid, spell_id: u32) {
    let _ = crate::world_objects::creature_magic::create_item_spell(w, this, item, spell_id);
}

/// `Player.PlayerSpellTable` (`Player_AllowedSpellID.cs`).
fn player_player_spell_table() -> Vec<u32> {
    crate::world_objects::player_allowed_spell_id::PLAYER_SPELL_TABLE.to_vec()
}

/// `WorldObject.GetSpellSet(setItems, levelDiff)` (`WorldObject_Set.cs`).
fn world_object_get_spell_set(w: &World, set_items: &[ObjectGuid], level_diff: i32) -> Vec<Spell> {
    crate::world_objects::world_object_set::get_spell_set(w, set_items, level_diff)
}

/// `WorldObject.GetSpellSetAll(equipmentSet)` (`WorldObject_Set.cs`).
fn world_object_get_spell_set_all(w: &World, equipment_set: EquipmentSet) -> Vec<Spell> {
    crate::world_objects::world_object_set::get_spell_set_all(w, equipment_set)
}

/// `WorldObject.ItemLevel` (the item XP level, `WorldObject_Set.cs`).
fn world_object_item_level(w: &World, item: ObjectGuid) -> Option<i32> {
    w.objects
        .get(item)
        .expect("ACE: item is null (NullReferenceException)")
        .item_level()
}

/// `Player.HandleRunRateUpdate()` (`Player.cs`).
fn player_handle_run_rate_update(w: &mut World, this: ObjectGuid) -> bool {
    crate::world_objects::player::handle_run_rate_update(w, this)
}

/// `Player.GetAllPossessions()` (`Player_Inventory.cs`).
fn player_get_all_possessions(w: &World, this: ObjectGuid) -> Vec<ObjectGuid> {
    crate::world_objects::player_inventory::get_all_possessions(w, this)
}
