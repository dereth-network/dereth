//! Loaded panel tables and spell metadata.

use super::*;

impl Hud {
    /// Load `Attribute2ndTable 0x0E000003` once. A miss is reported, not fatal: the *current*
    /// vitals still read, only the maxima go missing.
    pub fn load_tables(
        &mut self,
        store: &dereth_dat::RetailDatStore,
        world: &dereth_client_model::World,
    ) {
        use dereth_assets::Decode;
        use dereth_primitives::AssetSource as _;
        if self.vitals_table.is_none() {
            match store
                .read(ATTRIBUTE_2ND_TABLE)
                .map_err(|e| e.to_string())
                .and_then(|b| {
                    Attribute2ndTable::decode_payload_in(
                        store.era_of(ATTRIBUTE_2ND_TABLE),
                        ATTRIBUTE_2ND_TABLE,
                        &b,
                    )
                    .map_err(|e| e.to_string())
                }) {
                Ok(t) => self.vitals_table = Some(t),
                Err(e) => {
                    tracing::warn!("Attribute2ndTable {ATTRIBUTE_2ND_TABLE:?}: {e}")
                }
            }
        }
        // Both are the panels' only source for a name and an icon; a miss empties the
        // page it feeds and is reported rather than fatal, like the vitals table above.
        if self.skill_table.is_none() {
            match store
                .read(SKILL_TABLE)
                .map_err(|e| e.to_string())
                .and_then(|b| {
                    SkillTable::decode_payload_in(store.era_of(SKILL_TABLE), SKILL_TABLE, &b)
                        .map_err(|e| e.to_string())
                }) {
                Ok(t) => self.skill_table = Some(t),
                Err(e) => tracing::warn!("SkillTable {SKILL_TABLE:?}: {e}"),
            }
        }
        if self.spell_table.is_none() {
            match store
                .read(SPELL_TABLE)
                .map_err(|e| e.to_string())
                .and_then(|b| {
                    SpellTable::decode_payload_in(store.era_of(SPELL_TABLE), SPELL_TABLE, &b)
                        .map_err(|e| e.to_string())
                }) {
                Ok(t) => self.spell_table = Some(t),
                Err(e) => tracing::warn!("SpellTable {SPELL_TABLE:?}: {e}"),
            }
        }
        // The contracts panel's only source for a name, a description, two NPCs and
        // three positions. A miss empties the tab and is reported rather than fatal, like the
        // three above.
        if self.contract_table.is_none() {
            match store
                .read(CONTRACT_TABLE)
                .map_err(|e| e.to_string())
                .and_then(|b| {
                    dereth_assets::tables::ContractTable::decode_payload(CONTRACT_TABLE, &b)
                        .map_err(|e| e.to_string())
                }) {
                Ok(t) => self.contract_table = Some(t),
                Err(e) => tracing::warn!("ContractTable {CONTRACT_TABLE:?}: {e}"),
            }
        }
        // The component tracker's two dat hops.
        // The enum lookup is `(3, 0x10000001, 0x28)` — a mapping whose forward direction is
        // SCID -> WCID — and this build has no cached resolver for that enum
        // (the same shortcut `GENDER_ENUM_MAPPER` and `panels::statmgmt::STRING_TABLE` take). So
        // the mapper is found by **matching**, not by a hard-coded id: every `0x27xxxxxx` is
        // decoded and the one whose keys overlap the `SpellComponentTable`'s SCIDs most is taken.
        //
        // That is deliberately an instrument that can prove it looked: `component_mapper_scan`
        // records how many mappers were examined and how many entries matched, so an empty
        // catalogue reads as "scanned N, matched 0" rather than as a silent zero.
        if self.component_catalogue.is_empty() {
            self.load_component_catalogue(store);
        }
        // The client lookup selects value 10, group `0x10000001`, and type `0x28`.
        // `did_by_enum` performs the native group/value walk; `read_typed` supplies the independent
        // `DB_TYPE_DUAL_DID_MAPPER` qualification used by the native lookup.
        if self.trade_note_values.is_empty() {
            let loaded = dereth_assets::did_by_enum(store, 0x1000_0001, 10)
                .ok_or_else(|| "enum group 0x10000001 value 10 is absent".to_owned())
                .and_then(|id| {
                    store
                        .read_typed(dereth_dat::DbType::DualDidMapper, id)
                        .map_err(|e| format!("{id:?}: {e}"))
                        .and_then(|bytes| {
                            dereth_assets::tables::DualDidMapper::decode_payload(id, &bytes)
                                .map_err(|e| format!("{id:?}: {e}"))
                        })
                });
            match loaded {
                Ok(mapper) => self.trade_note_values = mapper.0.enum_to_id,
                Err(e) => tracing::warn!("TradeNotes DualDidMapper: {e}"),
            }
        }
        // School-to-component-pack mapping:
        // The school/value/type lookup selects mapping table `0x27000003`
        // (`SchoolOfMagic` / `ComponentPacks`). The same two-hop walk and the same
        // `DB_TYPE_DUAL_DID_MAPPER` qualification as the trade notes above; the five Foci WCIDs
        // are the dat's, not this file's.
        if self.school_pack_wcid.is_empty() {
            let loaded = dereth_assets::did_by_enum(store, 0x1000_0001, 4)
                .ok_or_else(|| "enum group 0x10000001 value 4 is absent".to_owned())
                .and_then(|id| {
                    store
                        .read_typed(dereth_dat::DbType::DualDidMapper, id)
                        .map_err(|e| format!("{id:?}: {e}"))
                        .and_then(|bytes| {
                            dereth_assets::tables::DualDidMapper::decode_payload(id, &bytes)
                                .map_err(|e| format!("{id:?}: {e}"))
                        })
                });
            match loaded {
                // Row 0 is the mapper's own `Undef -> 0`; `magic_pack_is_owned` would never match a
                // WCID of 0 anyway, and `school_of_magic_to_wcid`'s miss is already 0, so it is
                // dropped here rather than carried as a school.
                Ok(mapper) => {
                    self.school_pack_wcid = mapper
                        .0
                        .enum_to_id
                        .into_iter()
                        .filter(|(k, v)| *k != 0 && *v != 0)
                        .collect();
                }
                Err(e) => tracing::warn!("SchoolOfMagic DualDidMapper: {e}"),
            }
        }
        // The quality filter, for the int and float enchantment queries. A miss leaves int and
        // float qualities un-enchanted, which is the client's returning null — it
        // returns 0 and leaves the caller's value alone.
        if self.quality_filter.is_none() {
            let ids = store.ids_of(dereth_dat::DbType::QualityFilter);
            match ids.first() {
                Some(id) => match store.read(*id).map_err(|e| e.to_string()).and_then(|b| {
                    dereth_assets::tables::QualityFilter::decode_payload(*id, &b)
                        .map_err(|e| e.to_string())
                }) {
                    Ok(t) => self.quality_filter = Some(t),
                    Err(e) => tracing::warn!("QualityFilter {id:?}: {e}"),
                },
                None => tracing::warn!("no QualityFilter in the dat"),
            }
        }
        if self.xp_table.is_none() {
            match store
                .read(XP_TABLE)
                .map_err(|e| e.to_string())
                .and_then(|b| {
                    dereth_assets::tables::XpTable::decode_payload_in(
                        store.era_of(XP_TABLE),
                        XP_TABLE,
                        &b,
                    )
                    .map_err(|e| e.to_string())
                }) {
                Ok(t) => self.xp_table = Some(t),
                Err(e) => tracing::warn!("XpTable {XP_TABLE:?}: {e}"),
            }
        }
        self.era.world_dats = store.era();
        if !self.era.era_announced {
            self.era.era = dereth_client_contract::EraView::era_of_dats(self.era.world_dats);
        }
        if let Some(t) = &self.xp_table {
            self.era.level_cap = u32::try_from(t.level_xp.len().saturating_sub(1)).unwrap_or(0);
        }
        if let Some(t) = &self.skill_table {
            self.era.skills = t.skills.keys().copied().collect();
        }
        // Four more of the same shape, for the header's heritage line. A miss leaves
        // that one field empty and is reported, exactly as the four above.
        for (slot, id, what) in [
            (
                &mut self.gender_names,
                GENDER_ENUM_MAPPER,
                "gender EnumMapper",
            ),
            (
                &mut self.heritage_names,
                HERITAGE_ENUM_MAPPER,
                "heritage EnumMapper",
            ),
            (
                &mut self.title_tokens,
                TITLE_ENUM_MAPPER,
                "title EnumMapper",
            ),
            (
                &mut self.creature_type_names,
                CREATURE_TYPE_ENUM_MAPPER,
                "creature-type EnumMapper",
            ),
        ] {
            if slot.is_some() {
                continue;
            }
            match store.read(id).map_err(|e| e.to_string()).and_then(|b| {
                dereth_assets::tables::EnumMapper::decode_payload(id, &b).map_err(|e| e.to_string())
            }) {
                Ok(t) => *slot = Some(t),
                Err(e) => tracing::warn!("{what} {id:?}: {e}"),
            }
        }
        // The salvage report's material names. A miss leaves every material
        // reading "Unknown", which is the material-name lookup returning `false` --
        // retail's own answer, and reported here as the other four are.
        if self.material_names.is_none() {
            match store
                .read(MATERIAL_TYPE_NAMES)
                .map_err(|e| e.to_string())
                .and_then(|b| {
                    dereth_assets::DidMapper::decode_payload(MATERIAL_TYPE_NAMES, &b)
                        .map_err(|e| e.to_string())
                }) {
                Ok(t) => self.material_names = Some(t),
                Err(e) => {
                    tracing::warn!("material names {MATERIAL_TYPE_NAMES:?}: {e}");
                }
            }
        }
        if self.title_strings.is_none() {
            match store
                .read(TITLE_STRING_TABLE)
                .map_err(|e| e.to_string())
                .and_then(|b| {
                    dereth_assets::ui::StringTable::decode_payload(TITLE_STRING_TABLE, &b)
                        .map_err(|e| e.to_string())
                }) {
                Ok(t) => self.title_strings = Some(t),
                Err(e) => tracing::warn!("title StringTable {TITLE_STRING_TABLE:?}: {e}"),
            }
        }
        self.rebuild_panel_tables(world);
    }

    /// The qualities from the local player description, which exists for the player's
    /// weenie and no other; the same
    /// question `HudView::player_desc` asks, for the paths that hold a `&World` instead of a view.
    ///
    /// `None` is "no `0x0013` yet" — the skill-list rebuild's own
    /// local-player-description guard — and **not** "no object row yet", which
    /// the row's empty `Qualities` would otherwise make indistinguishable.
    #[must_use]
    pub fn player_desc<'a>(
        &self,
        world: &'a dereth_client_model::World,
    ) -> Option<&'a dereth_client_model::Qualities> {
        if !self.player_desc_received {
            return None;
        }
        // The row first, because once it
        // exists it is the object every later stat update writes; the parked
        // description behind it, because `0x0013` can be unpacked before the row it belongs on
        // exists -- and the two joins below are run **on that message** and by nothing else.
        //
        // The ordinary row lookup returns silently when the player id is unset or the weenie row
        // is missing, which is the ordering ACE actually produces:
        // `Player_Networking.SendSelf` enqueues `GameEventPlayerDescription` before
        // `GameMessageCreateObject` and the two travel on different queues (`long-solo-play` has them
        // at idx 9 on queue 9 and idx 22 on queue 10). Without the fallback `build_skills` and
        // `build_spells` would take the skill-list rebuild's missing-player-description arm at the
        // one moment they are ever run, and the skills page, the spellbook and the attribute
        // page would stay empty for the rest of the session.
        //
        // In the reference client the distinction cannot arise: qualities are installed before
        // the player-description-received notice is raised, with the row guaranteed present by its queue order. Answering
        // from the park is what makes this build behave the way retail does. It is not a second
        // store and it is not a widened guard: it is the same qualities, read one step
        // before its owner is named.
        world
            .player_qualities()
            .or_else(|| world.login_player_desc())
    }

    /// Rebuild the skill-table and spellbook joins once per relevant change rather than once per
    /// frame.
    ///
    /// Both are pure functions of `(the retail table, the player's qualities)`, and both are
    /// notice-driven in the client — the player-description-received notice for each, plus the
    /// skill-advancement-class-changed notice for the skills. The two callers here are `load_tables` (the
    /// table arrived) and the `0x0013` arm of `apply_events` (the qualities arrived).
    pub fn rebuild_panel_tables(&mut self, world: &dereth_client_model::World) {
        self.skills = self.build_skills(world);
        self.spells = self.build_spells(world);
    }

    /// The skill-list rebuild's loop: **every key of the `SkillTable`**, joined to the skill query.
    ///
    /// A skill the player's skill-stats table has no entry for still gets a row — the skill query
    /// leaves `_sac` at `UNDEF` and the rebuild files it under the fourth header. So the
    /// row count is the table's size and not the player's, which is what makes the fourth group
    /// non-empty on a fresh character.
    fn build_skills(&self, world: &dereth_client_model::World) -> Vec<SkillEntry> {
        let Some(table) = self.skill_table.as_ref() else {
            return Vec::new();
        };
        // The skill-list rebuild's own guard: the body is inside
        // the local-player-description guard, so before `0x0013` the client leaves the
        // list flushed rather than drawing thirty-eight undefined rows. Without this the page
        // comes up full of zeroes in the fourth group and then rebuilds, which is visible.
        let Some(q) = self.player_desc(world) else {
            return Vec::new();
        };
        table
            .skills
            .iter()
            .map(|(id, base)| {
                // The row's advancement class and raw skill value are queried independently.
                let sac = dereth_client_model::skills::inq_skill_advancement_class(q, *id) as u32;
                let level = dereth_client_model::skills::inq_skill(q, table, *id, true)
                    .and_then(|v| i32::try_from(v).ok())
                    .unwrap_or(0);
                // The font operand is the skill query with `raw = 0`, the **enchanted** value,
                // not the base-level query's attribute contribution — see `SkillEntry::effective`.
                let effective = dereth_client_model::skills::inq_skill(q, table, *id, false)
                    .and_then(|v| i32::try_from(v).ok())
                    .unwrap_or(0);
                // The vitae penalty on **this skill's raw level**, zero or negative. The panel
                // update's colour comparison subtracts
                // it, which is what stops a vitae-carrying character's whole list drawing red.
                // Read straight off the registry, the same route the attribute-2nd rows take:
                // `HudView` does not implement `GameView::vitae`.
                let vitae = dereth_client_contract::panels::inforegion::vitae_modifier(
                    level,
                    Some(q.enchantments.vitae_value()),
                );
                SkillEntry {
                    id: *id,
                    name: base.name.clone(),
                    icon: (base.icon != 0).then_some(DataId(base.icon)),
                    min_level: base.min_level,
                    sac,
                    level,
                    effective,
                    vitae,
                }
            })
            .collect()
    }

    /// Find and decode the WCID -> SCID mapping and the component table, and join
    /// them into a [`dereth_client_model::magic::ComponentCatalogue`].
    ///
    /// See the call site for why the mapper is matched rather than named.
    fn load_component_catalogue(&mut self, store: &dereth_dat::RetailDatStore) {
        use dereth_assets::Decode;
        use dereth_primitives::AssetSource as _;
        let table: dereth_assets::tables::SpellComponentTable = match store
            .read(SPELL_COMPONENT_TABLE)
            .map_err(|e| e.to_string())
            .and_then(|b| {
                dereth_assets::tables::SpellComponentTable::decode_payload(
                    SPELL_COMPONENT_TABLE,
                    &b,
                )
                .map_err(|e| e.to_string())
            }) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!("SpellComponentTable {SPELL_COMPONENT_TABLE:?}: {e}");
                return;
            }
        };
        let ids = store.ids_of(dereth_dat::DbType::DualDidMapper);
        let mut best: Option<(usize, Vec<(u32, u32)>)> = None;
        let mut examined = 0usize;
        for id in ids {
            let Ok(bytes) = store.read(id) else { continue };
            let Ok(m) = dereth_assets::tables::DualDidMapper::decode_payload(id, &bytes) else {
                continue;
            };
            examined += 1;
            let hits =
                m.0.enum_to_id
                    .iter()
                    .filter(|(k, _)| table.components.contains_key(k))
                    .count();
            if hits > best.as_ref().map_or(0, |(n, _)| *n) {
                best = Some((hits, m.0.enum_to_id.clone()));
            }
        }
        let (matched, pairs) = best.unwrap_or((0, Vec::new()));
        self.component_mapper_scan = (examined, matched);
        self.component_catalogue = dereth_client_model::magic::ComponentCatalogue::new(
            pairs,
            table.components.iter().map(|(scid, c)| {
                (
                    *scid,
                    dereth_client_model::magic::ComponentBase {
                        name: c.name.clone(),
                        category: c.category,
                        icon: c.icon,
                    },
                )
            }),
        );
        tracing::debug!(
            "component catalogue: {} components, {matched} of them mapped from \
             {examined} component mapping table(s)",
            table.components.len()
        );
    }

    /// Join the player's transcribed-spell list to the spell table.
    ///
    /// Unlike the skills, this walks the **player's** spellbook and looks each id up in the
    /// table: the player-description refresh iterates the transcribed list, and the spell add
    /// drops an id the spell table does not know. A spell whose base is missing is
    /// therefore skipped here too rather than drawn as a blank row.
    pub(super) fn build_spells(&self, world: &dereth_client_model::World) -> Vec<SpellEntry> {
        let Some(q) = self.player_desc(world) else {
            return Vec::new();
        };
        let Some(book) = q.spell_book.as_ref() else {
            return Vec::new();
        };
        // The `SpellTable` half of the join, and the "a spell whose base is missing is skipped"
        // rule, are both [`Self::spell_entry`]'s `?`s.
        book.keys().filter_map(|id| self.spell_entry(*id)).collect()
    }

    /// Unknown favorites are removed from visible banks only, after the player's book is authoritative.
    pub fn unknown_spell_favorites(
        &self,
        world: &dereth_client_model::World,
    ) -> Vec<dereth_client_contract::UiRequest> {
        let Some(q) = self.player_desc(world) else {
            return Vec::new();
        };
        let count =
            dereth_client_contract::era::EraUiFacts::for_profile(self.era.era).spell_favorite_tabs;
        world
            .player_system
            .spell_tabs
            .iter()
            .take(count)
            .enumerate()
            .flat_map(|(tab, ids)| {
                ids.iter()
                    .copied()
                    .filter(|id| {
                        !q.spell_book
                            .as_ref()
                            .is_some_and(|book| book.contains_key(id))
                    })
                    .map(
                        move |spell_id| dereth_client_contract::UiRequest::RemoveSpellFavorite {
                            spell_id,
                            tab,
                        },
                    )
            })
            .collect()
    }

    /// One `SpellTable` row as a [`SpellEntry`] —
    /// with the four derivations [`Self::build_spells`] needs.
    ///
    /// Extracted because the Spellcasting panel asks the same question about a spell that is **not**
    /// in the player's book: the endowment icon and the cast button's tooltip both look up the
    /// endowment spell, the spell on the wielded wand.
    /// See [`dereth_client_contract::GameView::spell`].
    pub fn spell_entry(&self, id: u32) -> Option<SpellEntry> {
        use dereth_client_contract::spellbook::power_component;
        use dereth_client_model::magic::{scarab_power_level, spell_level_by_rough_heuristic};
        // Retail's spell add drops an id the spell table does not know, and so does this `?`.
        let b = self.spell_table.as_ref()?.spells.get(&id)?;
        let icon_power = scarab_power_level(power_component(b.raw_comps[0], b.comp_key));
        Some(SpellEntry {
            id,
            name: b.name.clone(),
            icon: (b.icon != 0).then_some(DataId(b.icon)),
            school: b.school,
            // The level heuristic reads the **first** formula slot after the power component is
            // selected, which keeps slot positions;
            // `SpellBase::comps` drops the zero slots, so the raw slot is used.
            level: spell_level_by_rough_heuristic(icon_power),
            icon_power,
            display_order: b.display_order,
            // Preserve the full spell bitfield: icon composition reads
            // `Reversed (0x10)` for the wash and `FellowshipSpell (0x2000)` /
            // `SelfTargeted (0x8)` for the badge.
            bitfield: b.bitfield,
        })
    }
}

/// `dereth_assets::tables::Contract` -> `dereth_client_model::quests::Contract` — the dat's eleven-string
/// array given the meanings its contract-record offsets carry.
///
/// The order is the one the client reads, i.e. ascending offset from
/// `0x0C`: name, description, description_progress, npc_start, npc_end, then the six quest flags
/// (`stamped`, `started`, `finished`, `progress`, `timer`, `repeat_time`). `dereth-assets` keeps
/// them positional on purpose — it names no gameplay semantics — so this is the one place the
/// mapping is written down.
pub(super) fn contract_of(
    c: &dereth_assets::tables::Contract,
) -> dereth_client_model::quests::Contract {
    let s = |i: usize| c.strings.get(i).cloned().unwrap_or_default();
    dereth_client_model::quests::Contract {
        version: c.version,
        contract_id: c.contract_id,
        contract_name: s(0),
        description: s(1),
        description_progress: s(2),
        name_npc_start: s(3),
        name_npc_end: s(4),
        questflag_stamped: s(5),
        questflag_started: s(6),
        questflag_finished: s(7),
        questflag_progress: s(8),
        questflag_timer: s(9),
        questflag_repeat_time: s(10),
    }
}

/// Contract-panel coordinate arithmetic. A zero cell writes nothing; a cell that cannot convert is
/// labeled `Indoors`. Both converted axes subtract `0x400`, multiply by `0.1`, and add `0.5`.
///
/// **This is not the radar conversion.** That one is the numeric half of
/// the cell-id-to-coordinate-string conversion and subtracts `0x100` from the north/south axis with no
/// `+ 0.5`. The two functions disagree and this panel uses its own; the pair is recorded here
/// rather than reconciled, because reconciling them would change the radar.
///
/// The first converted value is the **east/west** axis and the second the north/south one;
/// `location_string` prints north/south first.
pub(super) fn contract_location(p: dereth_primitives::Position) -> Option<String> {
    if p.cell.0 == 0 {
        return None;
    }
    let coords = dereth_physics::landdefs::gid_to_lcoord(p.cell).map(|(x, y)| {
        (
            f64::from(y - 0x400) * 0.1 + 0.5,
            f64::from(x - 0x400) * 0.1 + 0.5,
        )
    });
    Some(dereth_client_model::quests::contract_location_text(coords))
}

/// The skill-level experience threshold, clamped for the footer's meter.
///
/// The real function answers `0xFFFFFFFF` for a skill that is neither trained nor specialised,
/// which is a sentinel and not a number; the trained-skill footer never asks it
/// about one, because the untrained footer has no meter. 0 here keeps the meter's span at 0, which
/// `SkillAdvancement::meter_fill` reads as "no bar" rather than as a wild fraction.
pub(super) fn level_xp(
    t: &dereth_assets::tables::XpTable,
    sac: dereth_client_model::skills::Sac,
    level: u32,
) -> u32 {
    let v = dereth_client_model::advancement::experience_to_skill_level(t, sac, level as usize);
    if v == u32::MAX {
        0
    } else {
        v
    }
}

/// The spell-component panel's component walk, as a snapshot.
///
/// Seven component categories in their defined order, each already sorted by display name and
/// joined to the desired component level for each `wcid`.
///
/// The `Undef` bucket (8) is **not** included: the component-panel loop iterates categories 0
/// through 6, so a
/// component the `SpellComponentTable` does not know is tracked and never drawn. That is the
/// client's behaviour; it is reproduced rather than corrected, and `(8)`
/// is where such a row can be found by a test.
pub(super) fn component_categories(
    world: &dereth_client_model::World,
) -> Vec<dereth_client_contract::ComponentCategory> {
    use dereth_client_contract::{ComponentCategory, ComponentRow};
    world
        .magic
        .components
        .categories()
        .map(|(category, rows)| ComponentCategory {
            category,
            rows: rows
                .iter()
                .map(|d| ComponentRow {
                    wcid: d.class_id,
                    name: d.name.clone(),
                    // Component rows draw the spell-component table's icon id for the
                    // component id the class id maps to (`wcid_to_scid`), **not** the object's own
                    // `pwd.icon_id`, which nothing in this path draws — `ComponentRow::icon` is the
                    // table's. `0` is the icon builder's two early returns, and `row_icon` turns it
                    // into "leave the row's image alone", which is where the image clear sits
                    // relative to them.
                    icon: dereth_client_contract::panels::spellcomponent::row_icon(
                        world
                            .magic
                            .catalogue
                            .component_icon(d.class_id)
                            .unwrap_or(0),
                    ),
                    owned: d.num_items(),
                    desired: world.player_system.desired_comp_level(d.class_id),
                    object: d.first_object_id(),
                })
                .collect(),
        })
        .collect()
}
