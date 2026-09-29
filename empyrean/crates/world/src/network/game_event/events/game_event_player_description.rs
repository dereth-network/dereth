// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventPlayerDescription.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventPlayerDescription.cs`.

use empyrean_net::GameMessageGroup;
use empyrean_net::SessionId;

use crate::network::game_event::game_event_message::game_event_message_with_capacity;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::World;

// ACE: GameEventPlayerDescription.GameEventPlayerDescription
#[must_use]
pub fn game_event_player_description(w: &mut World, session: SessionId) -> GameMessage {
    // 10,333 is the average seen in retail pcaps, 28,828 is the max seen in retail pcaps
    let mut msg = game_event_message_with_capacity(
        GameEventType::PlayerDescription,
        GameMessageGroup::UIQueue,
        session_data(w, session),
        16384,
    );
    write_event_body(w, session, &mut msg);
    msg
}

/// `[Flags] enum DescriptionPropertyFlag` (private in ACE).
mod description_property_flag {
    pub const PROPERTY_INT32: u32 = 0x0001;
    pub const PROPERTY_BOOL: u32 = 0x0002;
    pub const PROPERTY_DOUBLE: u32 = 0x0004;
    pub const PROPERTY_DID: u32 = 0x0008;
    pub const PROPERTY_STRING: u32 = 0x0010;
    pub const POSITION: u32 = 0x0020;
    pub const PROPERTY_IID: u32 = 0x0040;
    pub const PROPERTY_INT64: u32 = 0x0080;
}

/// `[Flags] enum DescriptionVectorFlag` (private in ACE).
mod description_vector_flag {
    pub const ATTRIBUTE: u32 = 0x0001;
    pub const SKILL: u32 = 0x0002;
    pub const SPELL: u32 = 0x0100;
    pub const ENCHANTMENT: u32 = 0x0200;
}

// ACE: GameEventPlayerDescription.WriteEventBody
/// The player's qualities (the send-on-login properties in bucket order, the last outside death
/// position), attributes, vitals, skills, spell book, enchantments, character options, shortcuts,
/// spell bars, fill components, then the inventory and the equipped items. "refactor this -- it
/// is kind of ridiculous for this to all be in 1 giant function. this is a combination of
/// CACQualities, BaseQualities, PlayerModule, and other structures"
#[allow(
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn write_event_body(w: &mut World, session: SessionId, msg: &mut GameMessage) {
    use description_property_flag as dpf;
    use description_vector_flag as dvf;
    use empyrean_common::dotnet::DotNetHashSet;
    use empyrean_entity::enums::ext::send_on_login_properties as sol;
    use empyrean_entity::enums::{
        AttributeCache, CharacterOptionDataFlag, CloakStatus, ContainerType, PositionType,
        PropertyAttribute, PropertyAttribute2nd, PropertyString, WeenieType,
    };

    use crate::network::game_event::game_event_message::session_player;
    use crate::network::game_messages::game_message::{ace_str, write_record};
    use crate::network::structure::enchantment_registry::{self, enchantment_registry_new};
    use crate::network::structure::hash_comparer::{
        sorted, PropertyBoolComparer, PropertyDataIdComparer, PropertyFloatComparer,
        PropertyInstanceIdComparer, PropertyInt64Comparer, PropertyIntComparer,
        PropertyStringComparer, SkillComparer, SpellComparer,
    };
    use crate::network::structure::packable_hash_table;
    use crate::network::structure::packable_list::count;
    use crate::network::structure::shortcut::{self, Shortcut};
    use crate::world_objects::world_object_networking::shims;
    use dereth_protocol::types::{ContentProfile, InventoryPlacement};

    const PROPERTY_INT_COMPARER: PropertyIntComparer = PropertyIntComparer::new(64);
    const PROPERTY_INT64_COMPARER: PropertyInt64Comparer = PropertyInt64Comparer::new(64);
    const PROPERTY_BOOL_COMPARER: PropertyBoolComparer = PropertyBoolComparer::new(32);
    const PROPERTY_DOUBLE_COMPARER: PropertyFloatComparer = PropertyFloatComparer::new(32);
    const PROPERTY_STRING_COMPARER: PropertyStringComparer = PropertyStringComparer::new(32); // 16 in client, 32 sent across wire
    const PROPERTY_DATA_ID_COMPARER: PropertyDataIdComparer = PropertyDataIdComparer::new(32);
    const PROPERTY_INSTANCE_ID_COMPARER: PropertyInstanceIdComparer =
        PropertyInstanceIdComparer::new(32);
    const SKILL_COMPARER: SkillComparer = SkillComparer::new(32);
    const SPELL_COMPARER: SpellComparer = SpellComparer::new(64);

    fn set<T: Copy>(ids: &[T], id: impl Fn(T) -> u16) -> DotNetHashSet<u16> {
        ids.iter().map(|&i| id(i)).collect()
    }

    use dereth_protocol::archive::PackedHash;
    use dereth_protocol::types::{qualities as q, PropertyTables};

    let player = session_player(w, session);

    let mut property_flags = 0u32;
    let p = w.objects.get(player).expect("ACE: Session.Player is null");
    let weenie_type = p.biota.weenie_type.0;

    // Each table is its comparer's bucket count and order.
    fn table<K: Copy, V: Clone>(
        entries: impl IntoIterator<Item = (K, V)>,
        comparer: &impl crate::network::structure::hash_comparer::KeyComparer<K>,
        buckets: u16,
        key: impl Fn(K) -> u32,
    ) -> PackedHash<u32, V> {
        PackedHash {
            table_size: u32::from(buckets),
            entries: sorted(entries, comparer)
                .into_iter()
                .map(|(k, v)| (key(k), v))
                .collect(),
        }
    }
    let mut tables = PropertyTables::default();

    let properties_int = p.get_all_property_int_where(&set(sol::PROPERTIES_INT, |x| x.0));

    if !properties_int.is_empty() {
        property_flags |= dpf::PROPERTY_INT32;
        tables.ints = Some(table(
            properties_int.iter().map(|(k, v)| (*k, *v)),
            &PROPERTY_INT_COMPARER,
            PROPERTY_INT_COMPARER.num_buckets,
            u32::from,
        ));
    }

    let properties_int64 = p.get_all_property_int64_where(&set(sol::PROPERTIES_INT64, |x| x.0));

    if !properties_int64.is_empty() {
        property_flags |= dpf::PROPERTY_INT64;
        tables.int64s = Some(table(
            properties_int64.iter().map(|(k, v)| (*k, *v)),
            &PROPERTY_INT64_COMPARER,
            PROPERTY_INT64_COMPARER.num_buckets,
            u32::from,
        ));
    }

    let properties_bool = p.get_all_property_bools_where(&set(sol::PROPERTIES_BOOL, |x| x.0));

    if !properties_bool.is_empty() {
        property_flags |= dpf::PROPERTY_BOOL;
        // each value as a `uint`: just as fast as inlining
        tables.bools = Some(table(
            properties_bool.iter().map(|(k, v)| (*k, i32::from(*v))),
            &PROPERTY_BOOL_COMPARER,
            PROPERTY_BOOL_COMPARER.num_buckets,
            u32::from,
        ));
    }

    let properties_double = p.get_all_property_float_where(&set(sol::PROPERTIES_DOUBLE, |x| x.0));

    if !properties_double.is_empty() {
        property_flags |= dpf::PROPERTY_DOUBLE;
        tables.floats = Some(table(
            properties_double.iter().map(|(k, v)| (*k, *v)),
            &PROPERTY_DOUBLE_COMPARER,
            PROPERTY_DOUBLE_COMPARER.num_buckets,
            u32::from,
        ));
    }

    let properties_string = p.get_all_property_string_where(&set(sol::PROPERTIES_STRING, |x| x.0));

    if !properties_string.is_empty() {
        property_flags |= dpf::PROPERTY_STRING;

        // `Session.Player.IsPlussed`: `Character.IsPlussed`, or the session's access level with
        // `OverrideCharacterPermissions`.
        let is_plussed = crate::world_objects::player_properties::is_plussed(w, p.guid);

        let strings = properties_string.iter().map(|(k, v)| {
            let value = if *k == PropertyString::Name
                && is_plussed
                && p.cloak_status() < CloakStatus::Player
            {
                format!("+{v}")
            } else {
                v.clone()
            };
            (*k, ace_str(value.as_str()))
        });
        tables.strings = Some(table(
            strings,
            &PROPERTY_STRING_COMPARER,
            PROPERTY_STRING_COMPARER.num_buckets,
            u32::from,
        ));
    }

    let properties_did = p.get_all_property_data_id_where(&set(sol::PROPERTIES_DATA_ID, |x| x.0));

    if !properties_did.is_empty() {
        property_flags |= dpf::PROPERTY_DID;
        tables.dids = Some(table(
            properties_did.iter().map(|(k, v)| (*k, *v)),
            &PROPERTY_DATA_ID_COMPARER,
            PROPERTY_DATA_ID_COMPARER.num_buckets,
            u32::from,
        ));
    }

    let properties_iid =
        p.get_all_property_instance_id_where(&set(sol::PROPERTIES_INSTANCE_ID, |x| x.0));

    if !properties_iid.is_empty() {
        property_flags |= dpf::PROPERTY_IID;
        let iids = properties_iid
            .iter()
            .map(|(k, v)| (*k, dereth_primitives::ObjectId(*v)));
        tables.iids = Some(table(
            iids,
            &PROPERTY_INSTANCE_ID_COMPARER,
            PROPERTY_INSTANCE_ID_COMPARER.num_buckets,
            u32::from,
        ));
    }

    /*if ((propertyFlags & DescriptionPropertyFlag.Resource) != 0)
    {
    }*/

    /*if ((propertyFlags & DescriptionPropertyFlag.Link) != 0)
    {
    }*/

    let last_outside_death = p.get_position(PositionType::LastOutsideDeath);

    if let Some(last_outside_death) = last_outside_death {
        property_flags |= dpf::POSITION;

        // A one-entry table of 16 buckets.
        tables.positions = Some(PackedHash {
            table_size: 16,
            entries: vec![(
                u32::from(PositionType::LastOutsideDeath),
                (&last_outside_death).into(),
            )],
        });
    }

    let mut vector_flags = dvf::ATTRIBUTE | dvf::SKILL;

    // `Biota.CloneSpells`: the spell book's keys and values (probabilities), in dictionary order.
    let known_spells: Vec<(i32, f32)> = p
        .biota
        .properties_spell_book
        .as_ref()
        .map(|b| b.iter().map(|(k, v)| (*k, *v)).collect())
        .unwrap_or_default();

    if !known_spells.is_empty() {
        vector_flags |= dvf::SPELL;
    }

    if shims::enchantment_manager_has_enchantments(w, player) {
        vector_flags |= dvf::ENCHANTMENT;
    }

    // `Convert.ToUInt32(Session.Player.Health != null)`: the vital always exists (the Creature
    // constructor creates it).
    let has_health = 1;

    let mut attribute_cache = None;
    if (vector_flags & dvf::ATTRIBUTE) != 0 {
        let attribute_flags = AttributeCache::Full;

        // `CreatureAttribute`: Ranks = LevelFromCP, StartingValue = InitLevel, ExperienceSpent =
        // CPSpent (a missing entry is a new, zero one).
        let attribute = |flag: AttributeCache, a: PropertyAttribute| {
            ((attribute_flags & flag).0 != 0).then(|| {
                let a = p
                    .biota
                    .properties_attribute
                    .as_ref()
                    .and_then(|d| d.get(&a).cloned())
                    .unwrap_or_default();
                q::Attribute {
                    level_from_cp: a.level_from_cp,
                    init_level: a.init_level,
                    cp_spent: a.cp_spent,
                }
            })
        };
        let vital = |flag: AttributeCache, v: PropertyAttribute2nd| {
            ((attribute_flags & flag).0 != 0).then(|| {
                let v = p
                    .biota
                    .properties_attribute_2nd
                    .as_ref()
                    .and_then(|d| d.get(&v).cloned())
                    .unwrap_or_default();
                q::SecondaryAttribute {
                    // init_level - always appears to be 0
                    attribute: q::Attribute {
                        level_from_cp: v.level_from_cp,
                        init_level: v.init_level,
                        cp_spent: v.cp_spent,
                    },
                    current_level: v.current_level,
                }
            })
        };

        attribute_cache = Some(q::AttributeCache {
            flags: attribute_flags.0,
            strength: attribute(AttributeCache::Strength, PropertyAttribute::Strength),
            endurance: attribute(AttributeCache::Endurance, PropertyAttribute::Endurance),
            quickness: attribute(AttributeCache::Quickness, PropertyAttribute::Quickness),
            coordination: attribute(
                AttributeCache::Coordination,
                PropertyAttribute::Coordination,
            ),
            focus: attribute(AttributeCache::Focus, PropertyAttribute::Focus),
            self_: attribute(AttributeCache::Self_, PropertyAttribute::Self_),
            health: vital(AttributeCache::Health, PropertyAttribute2nd::MaxHealth),
            stamina: vital(AttributeCache::Stamina, PropertyAttribute2nd::MaxStamina),
            mana: vital(AttributeCache::Mana, PropertyAttribute2nd::MaxMana),
        });
    }

    let mut skills = None;
    if (vector_flags & dvf::SKILL) != 0 {
        // `Session.Player.Skills`: one `CreatureSkill` per biota skill entry.
        let list: Vec<_> = p
            .biota
            .properties_skill
            .as_ref()
            .map(|d| d.iter().map(|(k, v)| (*k, v.clone())).collect())
            .unwrap_or_default();
        let _ = count(list.len());

        // TODO: Network.Structure.Skill
        let skill = |value: &empyrean_entity::models::properties_skill::PropertiesSkill| q::Skill {
            level_from_pp: value.level_from_pp, // points raised
            format_version: 1,
            sac: value.sac.0,             // skill state
            pp: value.pp,                 // xp spent on this skill
            init_level: value.init_level, // init_level, for training/specialized bonus from character creation
            resistance_of_last_check: 0,  // task difficulty, aka "resistance_of_last_check"
            last_used_time: 0.0,          // last_time_used
        };
        skills = Some(table(
            list.iter().map(|(k, v)| (*k, skill(v))),
            &SKILL_COMPARER,
            SKILL_COMPARER.num_buckets,
            |k| k.0.cast_unsigned(),
        )); // skill id
    }

    let mut spell_book = None;
    if (vector_flags & dvf::SPELL) != 0 {
        let _ = count(known_spells.len());
        // This sets a flag to use new spell configuration always 2: the page's likelihood 0 is
        // written as 0 + 2.
        let page = q::SpellBookPage {
            casting_likelihood: 0.0,
            legacy: None,
        };
        spell_book = Some(table(
            known_spells.iter().map(|(k, _)| (*k, page)),
            &SPELL_COMPARER,
            SPELL_COMPARER.num_buckets,
            i32::cast_unsigned,
        ));
    }

    let mut enchantments = None;
    if (vector_flags & dvf::ENCHANTMENT) != 0 {
        // `Session.Player.EnchantmentManager.SendRegistry(Writer)`
        let registry = enchantment_registry_new(w, player);
        enchantments = Some(enchantment_registry::record(&registry));
    }

    let qualities = q::AcQualities {
        base: q::AcBaseQualities {
            flags: property_flags,
            weenie_type,
            tables,
        },
        flags: vector_flags,
        has_health,
        attribute_cache,
        skills,
        spell_book,
        enchantments,
        event_filter: None,
        creation_profiles: None,
    };
    let strings: Vec<&str> = qualities
        .base
        .tables
        .strings
        .iter()
        .flat_map(|t| t.entries.iter().map(|(_, v)| v.as_str()))
        .collect();
    write_record(&mut msg.data, &strings, |wr| qualities.write(wr));

    let p = w.objects.get(player).expect("ACE: Session.Player is null");
    let character =
        shims::player_character(p).expect("ACE: Player.Character is null (NullReferenceException)");

    // TODO: Refactor this to set all of these flags based on data. Og II
    let mut option_flags = CharacterOptionDataFlag::CharacterOptions2;

    option_flags |= CharacterOptionDataFlag::SpellLists8;

    let shortcuts: Vec<Shortcut> = character
        .character_properties_shortcut_bar
        .iter()
        .map(Shortcut::from_shortcut_bar)
        .collect();
    if !shortcuts.is_empty() {
        option_flags |= CharacterOptionDataFlag::Shortcut;
    }

    if character
        .gameplay_options
        .as_ref()
        .is_some_and(|g| !g.is_empty())
    {
        option_flags |= CharacterOptionDataFlag::GameplayOptions;
    }

    let fill_comps = character.character_properties_fill_comp_book.clone();
    if !fill_comps.is_empty() {
        option_flags |= CharacterOptionDataFlag::DesiredComps;
    }

    option_flags |= CharacterOptionDataFlag::SpellbookFilters;

    // The player module, written as ACE writes it: its words and dereth-protocol's records for the
    // shortcuts, spell bars and fill components, then the gameplay options as the raw bytes ACE
    // keeps (with no alignment after them, where the retail record aligns).
    write_record(
        &mut msg.data,
        &[],
        |wr| -> Result<(), dereth_protocol::MessageError> {
            wr.u32(option_flags.0);
            wr.i32(character.character_options_1);

            if !shortcuts.is_empty() {
                let list: Vec<_> = shortcuts.iter().map(shortcut::record).collect();
                wr.packed_list(&list, |wr, s| {
                    s.write(wr);
                    Ok(())
                })?;
            }

            if (option_flags & CharacterOptionDataFlag::SpellLists8).0 != 0 {
                for i in 0..=7i64 {
                    // `GetSpellsInSpellBar(i)`: the bar's spells ordered by index (a stable sort).
                    let mut spells: Vec<_> = character
                        .character_properties_spell_bar
                        .iter()
                        .filter(|x| i64::from(x.spell_bar_number) == i + 1)
                        .collect();
                    spells.sort_by_key(|x| x.spell_bar_index);
                    let _ = count(spells.len());
                    let ids: Vec<u32> = spells.iter().map(|s| s.spell_id).collect();
                    wr.packed_list(&ids, |wr, id| {
                        wr.u32(*id);
                        Ok(())
                    })?;
                }
            } else {
                wr.u32(0);
            }

            if (option_flags & CharacterOptionDataFlag::DesiredComps).0 != 0 {
                // verify
                wr.packed_hash(
                    &packable_hash_table::fill_comps_record(&fill_comps),
                    |wr, k, v| {
                        wr.u32(*k);
                        wr.i32(*v);
                        Ok(())
                    },
                )?;
            }

            //if ((optionFlags & CharacterOptionDataFlag.SpellbookFilters) != 0)
            wr.u32(character.spellbook_filters);

            if (option_flags & CharacterOptionDataFlag::CharacterOptions2).0 != 0 {
                wr.i32(character.character_options_2);
            }

            /*if ((optionFlags & DescriptionOptionFlag.Unk100) != 0)
            {
            }*/

            if (option_flags & CharacterOptionDataFlag::GameplayOptions).0 != 0 {
                wr.bytes(character.gameplay_options.as_deref().unwrap_or_default());
            }

            /*if ((optionFlags & DescriptionOptionFlag.Unk400) != 0)
            {
            }*/
            Ok(())
        },
    );

    // Write total count.
    let inventory = crate::world_objects::container::inventory_values(w, player);

    let item =
        |g: &empyrean_entity::ObjectGuid| w.objects.get(*g).expect("ACE: null inventory item");
    // `UseBackpackSlot`: `WeenieType == Container || RequiresPackSlot`.
    let use_backpack_slot = |g: &empyrean_entity::ObjectGuid| {
        let o = item(g);
        o.biota.weenie_type == WeenieType::Container || o.requires_pack_slot()
    };

    let mut content_profiles = Vec::with_capacity(inventory.len());
    // write out all of the non-containers and foci
    let mut non_containers: Vec<_> = inventory
        .iter()
        .filter(|i| !use_backpack_slot(i))
        .copied()
        .collect();
    non_containers.sort_by_key(|i| item(i).placement_position());
    for g in non_containers {
        content_profiles.push(ContentProfile {
            iid: g.into(),
            container_properties: ContainerType::NonContainer.0.cast_unsigned(),
        });
    }
    // Containers and foci go in side slots, they come last with their own placement order.
    let mut containers: Vec<_> = inventory
        .iter()
        .filter(|i| use_backpack_slot(i))
        .copied()
        .collect();
    containers.sort_by_key(|i| item(i).placement_position());
    for g in containers {
        let container_type = if item(&g).biota.weenie_type == WeenieType::Container {
            ContainerType::Container
        } else {
            ContainerType::Foci
        };
        content_profiles.push(ContentProfile {
            iid: g.into(),
            container_properties: container_type.0.cast_unsigned(),
        });
    }
    // `inventory.Count`: the two lists partition the inventory.
    debug_assert_eq!(content_profiles.len(), inventory.len());

    let equipped = crate::world_objects::creature_equipment::equipped_objects_values(w, player);
    let placements: Vec<_> = equipped
        .iter()
        .map(|g| {
            let o = item(g);
            InventoryPlacement {
                iid: (*g).into(),
                location: o.current_wielded_location().map_or(0, |c| c.0),
                priority: o.clothing_priority().map_or(0, |c| c.0),
            }
        })
        .collect();

    write_record(&mut msg.data, &[], |wr| {
        wr.packed_list(&content_profiles, |wr, c| {
            c.write(wr);
            Ok(())
        })?;
        wr.packed_list(&placements, |wr, pl| {
            pl.write(wr);
            Ok(())
        })
    });
}
