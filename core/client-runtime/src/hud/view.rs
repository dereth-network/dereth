//! Read-only gameplay projection.

use super::*;

impl HudView<'_> {
    /// The local player's qualities, which belong to the player's own object and nothing else.
    ///
    /// The constructor registers the object allocated for the player id, so
    /// the panels' local-player-description view and the object table's row are one object. Two
    /// copies would let `Shop::update_total_value` read the one the purse never arrived in.
    ///
    /// **`None` means "no `0x0013` yet", not "no row yet"** — the distinction every caller relies
    /// on. The object stream gives the player's row an empty `Qualities` at `CreateObject`, so
    /// the emptiness of the row is a different question. Retail asks this one with the same
    /// initialization bit, and every
    /// skill-list rebuild guard checks for a local player description with a description unpacked into it.
    fn player_desc(&self) -> Option<&dereth_client_model::Qualities> {
        self.hud.player_desc(self.world)
    }

    /// Resolve a character title id to its display string; shared by the header's display title
    /// and the Titles tab's list.
    ///
    /// Id zero is refused. Other ids resolve to a title token, whose hash selects the display
    /// string. The two enum ids resolve through to `EnumMapper 0x22000041` and
    /// `StringTable 0x2300000E`, which are [`Hud::title_tokens`] and [`Hud::title_strings`] —
    /// both loaded at startup. `id == 0` is refused outright, and so is a token neither table
    /// carries.
    fn title_name(&self, id: u32) -> Option<String> {
        if id == 0 {
            return None;
        }
        let token = self
            .hud
            .title_tokens
            .as_ref()?
            .id_to_string
            .iter()
            .find(|(k, _)| *k == id)
            .map(|(_, s)| s.as_str())?;
        let hash = dereth_primitives::num::hash::str_hash(token.as_bytes());
        self.hud
            .title_strings
            .as_ref()?
            .strings
            .iter()
            .find(|(k, _)| *k == hash)
            .and_then(|(_, s)| s.strings.first().cloned())
    }
}

impl GameView for HudView<'_> {
    fn split_size(&self) -> i32 {
        i32::try_from(self.world.split.split_size).unwrap_or(i32::MAX)
    }
    fn max_split_size(&self) -> i32 {
        i32::try_from(self.world.split.max_split_size).unwrap_or(i32::MAX)
    }
    fn era(&self) -> Option<&dereth_client_contract::EraView> {
        Some(&self.hud.era)
    }

    /// The object table's copy first, because that is the one every other
    /// accessor keys on; otherwise it uses `0xF746`'s id when the object has not been created yet,
    /// because this reads the local player description and not the table.
    fn player(&self) -> Option<ObjectId> {
        self.world.player.or(self.hud.player)
    }

    /// The singular-name query with flag 0, composed name and all.
    ///
    /// The client prefixes the
    /// `MaterialType` string when `_material_type > 0`, and drops a leading `'+'` from an
    /// admin-hidden object. A salvage bag is named `Salvage (100)` by the shard and carries the
    /// material beside it, so the prefix is the whole difference between the shard's name and what
    /// retail draws. See [`Hud::refresh_display_names`] for why the answer is cached.
    ///
    /// The `NAME_APPROPRIATE` fork is not here: this seam hands out the singular and the plural
    /// separately ([`Self::plural_name`]) and the widget picks by stack size, moving that test one
    /// level out.
    fn name(&self, id: ObjectId) -> Option<&str> {
        if let Some(d) = self.hud.display_names.get(&id) {
            return (!d.singular.is_empty()).then_some(d.singular.as_str());
        }
        let n = self.world.weenie(id)?.pwd.name.as_str();
        (!n.is_empty()).then_some(n)
    }

    /// The Book panel's whole model, and the title the open-book handler's tail composes.
    ///
    /// The title is the one field this side has to build rather than copy: an **unsigned** book
    /// shows the object's own name and a **signed** one shows its
    /// inscription. The scribe's name is not the title in either case; it is the strip's label.
    /// The object-name query is `dereth_client_model`'s and this crate is where the two meet.
    fn book_session(&self) -> dereth_client_contract::book::BookSessionView {
        self.world.book_session_view()
    }
    fn open_book(&self) -> Option<dereth_client_contract::BookView> {
        let b = self.world.book.open.as_ref()?;
        let object_name = self.world.weenie(b.book_id).map_or_else(String::new, |w| {
            let material = material_name_of(
                self.hud.material_names.as_ref(),
                w.pwd.material_type.unwrap_or(0),
            );
            w.display_name(
                dereth_client_model::weenie::NameType::Appropriate,
                material.as_deref(),
            )
        });
        let title = if b.scribe_id == ObjectId(0) {
            object_name.clone()
        } else {
            b.inscription.clone()
        };
        Some(dereth_client_contract::BookView {
            book_id: b.book_id,
            object_name,
            player_id: self.world.player.unwrap_or(ObjectId(0)),
            // The PSR test checks access levels 0x2C and 0x2D, and additionally accepts 0x61.
            viewer_is_psr: self
                .world
                .player_qualities()
                .is_some_and(|q| [0x2C, 0x2D, 0x61].into_iter().any(|key| q.inq_bool(key))),
            max_num_pages: b.max_num_pages,
            pages: b
                .pages
                .pages
                .iter()
                .map(|p| dereth_client_contract::BookPageView {
                    author_id: p.author_id,
                    author_name: p.author_name.clone(),
                    author_account: p.author_account.clone(),
                    // The client leaves the page text unread when
                    // the text-included flag is 0, and that is the state `set_cur_page` answers
                    // with a book-page-data request rather than by drawing.
                    text: (p.text_included != 0).then(|| p.page_text.clone().unwrap_or_default()),
                    // Preserve the signed dword: the menu update tests exactly 1, while
                    // the page display and the page close test nonzero.
                    ignore_author: p.ignore_author,
                })
                .collect(),
            inscription: b.inscription.clone(),
            scribe_id: b.scribe_id,
            scribe_name: b.scribe_name.clone(),
            title,
            opening: self.world.book.opening,
            page_data_applied: self.world.book.page_data_applied,
            add_page_responses: self.world.book.add_page_responses,
            // The add-page response reads the author inside the
            // handler -- first getting the player id, then querying string property 1
            // on the local player description -- and `dereth-ui-screens` can see neither, so
            // both are composed here for the same reason `title` is.
            add_page: self
                .world
                .book
                .add_page
                .map(|a| dereth_client_contract::AddedPageView {
                    page: a.page,
                    success: a.success,
                    author_id: self.world.player.unwrap_or(ObjectId(0)),
                    author_name: self
                        .world
                        .player
                        .and_then(|p| self.world.weenie(p))
                        .map(|w| w.pwd.name.clone())
                        .unwrap_or_default(),
                }),
        })
    }

    fn barber(&self) -> Option<dereth_client_contract::BarberView> {
        let b = self.hud.barber?;
        let q = self.player_desc();
        Some(dereth_client_contract::BarberView {
            generation: self.hud.barber_generation,
            base_palette: b.base_palette,
            head_object: b.head_object,
            head_texture: b.head_texture,
            default_head_texture: b.default_head_texture,
            eyes_texture: b.eyes_texture,
            default_eyes_texture: b.default_eyes_texture,
            nose_texture: b.nose_texture,
            default_nose_texture: b.default_nose_texture,
            mouth_texture: b.mouth_texture,
            default_mouth_texture: b.default_mouth_texture,
            skin_palette: b.skin_palette,
            hair_palette: b.hair_palette,
            eyes_palette: b.eyes_palette,
            setup_id: b.setup_id,
            option1: b.option1,
            option2: b.option2,
            heritage: q
                .and_then(|q| u32::try_from(q.inq_int(HERITAGE_GROUP)).ok())
                .unwrap_or(0),
            gender: q
                .and_then(|q| u32::try_from(q.inq_int(GENDER)).ok())
                .unwrap_or(0),
        })
    }

    fn icon(&self, id: ObjectId) -> Option<DataId> {
        let i = self.world.weenie(id)?.pwd.icon_id;
        (i != 0).then_some(DataId(i))
    }

    fn int_stat(&self, id: ObjectId, prop: u32) -> Option<i32> {
        Some(self.world.weenie(id)?.qualities.as_ref()?.inq_int(prop))
    }

    /// `(stat, &value, 0)` for the current and the maximum, which is
    /// exactly the vitals UI's two reads per bar.
    fn vital(&self, id: ObjectId, which: Vital) -> Option<(u32, u32)> {
        let table = self.hud.vitals_table.as_ref()?;
        let filter = self.hud.quality_filter.as_ref();
        // The player's numbers come from the local player description handed to the vitals bar;
        // every other object's come from the object table.
        let q = if Some(id) == self.player() {
            self.player_desc()?
        } else {
            self.world.weenie(id)?.qualities.as_ref()?
        };
        let (cur, max) = which.stats();
        Some((
            inq_attribute_2nd(q, table, cur, false, filter)?,
            inq_attribute_2nd(q, table, max, false, filter)?,
        ))
    }

    fn open_inventory_container(&self) -> Option<ObjectId> {
        self.world.open_container.or_else(|| self.player())
    }
    /// The loose items in server order, which is the grid order.
    fn container_contents(&self, id: ObjectId) -> &[ObjectId] {
        self.world.inventory(id).map_or(&[][..], |i| &i.items)
    }

    /// The contained side packs.
    fn contained_containers(&self, id: ObjectId) -> &[ObjectId] {
        self.world.inventory(id).map_or(&[][..], |i| &i.containers)
    }

    /// `[tab]` — one of the eight spell bars.
    ///
    /// `dereth_client_model::player::PlayerSystem::spell_tabs` holds these, filled from
    /// `0x0013`'s `PlayerModule` by `apply_player_module`. They are the contents
    /// the spell-bar UI puts in each tab's item list, so they are also the order a quick-cast key
    /// indexes.
    fn spell_tab(&self, tab: usize) -> &[u32] {
        self.world
            .player_system
            .spell_tabs
            .get(tab)
            .map_or(&[], Vec::as_slice)
    }

    /// The three conditions for the spellcasting endowment icon.
    ///
    /// There is no endowment unless the player holds an object at location `0x1000000`, that
    /// object is known, its item type has the `0x8000` bit, and its spell id is non-zero.
    ///
    /// `0x8000` is `ITEM_TYPE::Caster`. Retail tests it as the sign of the type's second byte,
    /// which reads like a range check and is not one.
    fn endowment(&self) -> Option<(ObjectId, u32)> {
        use dereth_client_contract::panels::spellcasting::ENDOWMENT_LOCATION;
        const CASTER: u32 = dereth_rules::weenie::item_type::CASTER;
        let (item, _) = self
            .hud
            .equipment
            .iter()
            .find(|(_, loc)| *loc == ENDOWMENT_LOCATION)
            .copied()?;
        let w = self.world.weenie(item)?;
        if w.inq_type() & CASTER == 0 {
            return None;
        }
        let spell = u32::from(w.pwd.spell_id.unwrap_or(0));
        (spell != 0).then_some((item, spell))
    }

    /// The paper doll's `(object, location)` pairs.
    ///
    /// `dereth_client_model` keeps the inventory placements as
    /// `InventoryPlacement { iid, loc, priority }` and the paper doll reads only the first two, so
    /// the seam carries the pair.
    fn equipment(&self, id: ObjectId) -> &[(ObjectId, u32)] {
        // A borrowed slice is what the trait promises, so the pairs are cached beside the HUD
        // rather than built here. See [`Hud::equipment`].
        let _ = id;
        &self.hud.equipment
    }

    /// Absent means the object declares none, which
    /// the item list's container-size update treats as 0 rather than as unbounded.
    fn items_capacity(&self, id: ObjectId) -> Option<i32> {
        Some(dereth_rules::capacity::capacity(
            self.world.weenie(id)?.pwd.items_capacity.unwrap_or(0),
        ))
    }

    /// The number of container slots this object supplies.
    fn containers_capacity(&self, id: ObjectId) -> Option<i32> {
        Some(dereth_rules::capacity::capacity(
            self.world.weenie(id)?.pwd.containers_capacity.unwrap_or(0),
        ))
    }

    /// `set_waiting_state(1)` set it and only
    /// the server's move-item and attempt-failed answers clear it. **There is no timeout**,
    /// which is why the ghost can sit there for ever.
    fn item_waiting(&self, id: ObjectId) -> bool {
        self.world.weenie(id).is_some_and(|w| w.waiting)
    }

    /// The pending row, matched on the pair that names a list in
    /// this build: the parent container id and the container list.
    fn pending_row(
        &self,
        container: Option<ObjectId>,
        containers_list: bool,
    ) -> Option<(ObjectId, u32)> {
        let p = self.world.pending_row?;
        (Some(p.container) == container && p.containers_list == containers_list)
            .then_some((p.item, p.index))
    }

    fn selection(&self) -> Option<ObjectId> {
        self.world.selected
    }

    /// Query one live player option.
    ///
    /// Without this override the Character Options page would read the trait's `false` and all
    /// 50 rows would open unticked whatever the server sent. `dereth_client_model`'s
    /// `PLAYER_OPTIONS` carries all 53 masks.
    ///
    /// # Before a `0x0013` this answers the **constructed defaults**, not `false`
    ///
    /// The option word is a **member**, not a pointer, so every UI in the retail client that
    /// asks before the description lands gets the default word, which is
    /// the two default character-option words. Reading through
    /// [`character_option`](fn@character_option) and mapping its third state to `false` would give a different answer
    /// for the **sixteen** options the default-option table starts on (as `StayInChatMode` does).
    ///
    /// That shows as soon as a consumer *gates* on one of those sixteen: with a body in the world
    /// and no `0x0013` at all, the `CoordinatesOnRadar` gate (ordinal 20, one of the sixteen)
    /// would take the coordinate strip down where retail shows it. Reading the word directly is
    /// what retail does.
    ///
    /// [`character_option`](fn@character_option) keeps the third state and keeps its callers: anything that has to tell
    /// *off* from *not asked* — `Hud::option_bit`, which gates the player-module refresh
    /// re-seed on a module existing at all — still asks it.
    ///
    /// `dereth_client_model::player::options::Options::default()` is those two words, so this is one read
    /// with no branch.
    fn player_option(&self, o: dereth_client_contract::PlayerOption) -> bool {
        self.world.player_system.options.get(option_ordinal(o))
    }

    /// Query the compiled-in default-option true-list, which
    /// `dereth_client_model::player::options::default_option_value` already represents. It is the
    /// Character page's *Restore Defaults* button's only source.
    ///
    /// `Some` unconditionally: the client's function answers for every ordinal, so a `None` here
    /// would mean *this host cannot say*, and this host can. The world is not consulted — the
    /// default belongs to the option, not to the character.
    fn player_option_default(&self, o: dereth_client_contract::PlayerOption) -> Option<bool> {
        Some(dereth_client_model::player::options::default_option_value(
            option_ordinal(o),
        ))
    }

    /// `(property)` for a `Float` gameplay option — the Chat Options page's
    /// two opacity sliders.
    ///
    /// The retained module is the client's, so `None` here is
    /// *"before `0x0013`"* or *"the blob carries no such property"*, which is exactly the `false`
    /// the option query answers with and which the page turns into its default.
    fn gameplay_option_float(&self, property: u32) -> Option<f32> {
        let m = self.world.player_system.module.as_ref()?;
        match m.gameplay_options.as_ref()?.properties.get(property) {
            Some(BasePropertyValue::Float(v)) => Some(*v),
            _ => None,
        }
    }

    /// Query chat-window option `0x1000007F`.
    ///
    /// Reads the live module rather than [`Hud`]'s decoded snapshot, because the page's own
    /// `Apply` has already written the module by the time the next frame's current-value save
    /// asks — `consume_placement_requests` runs before `Hud::drive` — and a one-frame-stale
    /// snapshot would make Apply look like it had reverted the click.
    fn chat_window_filter(&self, window: u32) -> Option<u64> {
        let m = self.world.player_system.module.as_ref()?;
        decode_chat_filters(m)
            .into_iter()
            .find(|(id, _)| *id == window)
            .map(|(_, mask)| mask)
    }

    /// The four facts the selection query's not-a-stack arm branches
    /// on.
    ///
    /// `None` when the object has no weenie row, which is the same early return the client takes
    /// (it looks the object up and returns if there is none) before it can ask any of these.
    fn selection_query_facts(&self, id: ObjectId) -> Option<SelectionQueryFacts> {
        let w = self.world.weenie(id)?;
        Some(SelectionQueryFacts {
            is_player: w.is_player(),
            has_pet_owner: w.pwd.pet_owner.is_some_and(|o| o.0 != 0),
            attackable: self.world.object_is_attackable(id),
            owned_by_player: self.world.is_owned_by_player(id),
        })
    }

    /// `dereth_client_model::combat::SelectedMeters` — the `0x01C0` / `0x0264` replies.
    ///
    /// Selection clears the pair on a real selection edge, so the guard
    /// the object-health handler applies (*"is this notice about the object the
    /// toolbar is showing?"*) is already enforced upstream and is not repeated here.
    fn selected_meters(&self) -> (Option<f32>, Option<f32>) {
        (
            self.world.selected_meters.health,
            self.world.selected_meters.mana,
        )
    }

    fn radar_objects(&self) -> &[RadarEntry] {
        &self.hud.radar
    }

    fn radar_blank(&self) -> bool {
        self.hud.radar_blank
    }

    // The enchantment counts and the portal storm level are deliberately left at the trait's
    // defaults here: they are the indicator strip's inputs.

    /// `_vitae->_smod.val`, or **1.0** when
    /// there is no vitae. The vitae lamp's only input: the lamp asks for the vitae value and
    /// lights state 1 when the multiplier is below 1.0.
    ///
    /// Without this override `hud::indicators::vitae_state` could only ever answer
    /// `STATE_NOTHING` through the trait's `None`: no vitae penalty would be drawable, however deep
    /// the character's was. The value is `EnchantmentRegistry::vitae_value` from
    /// `dereth-client-model`.
    ///
    /// **The three states are kept distinct, and the middle one is why the override is safe.**
    /// No local player description yet is `None` (nothing is known); a description with no vitae is
    /// `Some(1.0)`, which `vitae_state` maps to the same `STATE_NOTHING` as `None`; only an actual
    /// penalty lights the lamp.
    ///
    /// **No capture can test the lit case.** Across the seven recorded captures: 5 of 7 carry a
    /// `0x0013` at all (`login-account-booted` and `ddd-interrogation-only` are login-only), **1**
    /// of those 5 carries an `EnchantmentRegistry`, and **0** carry a vitae — with 31 enchantment
    /// messages in the `0x02Cx` range in the corpus, none of which installs one.
    /// The lit branch is asserted against the vitae rule separately from these captures.
    fn vitae(&self) -> Option<f32> {
        Some(self.player_desc()?.enchantments.vitae_value())
    }

    /// The vitae display's three inputs.
    ///
    /// The pool is int property 129 (`VitaeCpPool`); the level is int property 139
    /// (`DeathLevel`), falling back to 25 (`Level`) when that is zero; and the experience still
    /// needed is the vitae threshold for that level minus the pool.
    ///
    /// The fall-back and the threshold are done here for the reason
    /// [`dereth_client_contract::VitaeDisplay`] gives, through
    /// `dereth_client_model::advancement::vitae_cp_pool_threshold`.
    ///
    /// `None` means no local player description, i.e. no `0x0013` yet.
    fn vitae_display(&self) -> Option<dereth_client_contract::VitaeDisplay> {
        let q = self.player_desc()?;
        let multiplier = q.enchantments.vitae_value();
        let pool = q.inq_int(VITAE_CP_POOL);
        let level = match q.inq_int(DEATH_LEVEL) {
            0 => q.inq_int(LEVEL),
            n => n,
        };
        // The client treats the level as an unsigned 32-bit value when converting it.
        // The world's era decides the curve (the older one before Throne of Destiny), unless
        // the world's rules name one.
        let threshold = dereth_rules::advancement::vitae_cp_pool_threshold_in(
            self.hud.era.world_rules.vitae_recovery(self.hud.era.era),
            f64::from(multiplier),
            f64::from(level.unsigned_abs()),
        );
        Some(dereth_client_contract::VitaeDisplay {
            multiplier,
            cp_pool: pool,
            threshold,
        })
    }

    /// The Character Info panel's six section inputs.
    ///
    /// The four int-quality queries, the eight attribute queries and the load query the client
    /// makes across the panel update's callees, plus the one derived value
    /// derived here because it lives in
    /// `dereth-client-model`. Every field is written out in
    /// [`dereth_client_contract::panels::characterinfo`].
    ///
    /// **`innate` is `_init_level`, not the base value.**
    /// The base inquiry answers `_init_level + _level_from_cp`, and `dereth_client_model::attributes::inq_attribute_base` is
    /// that sum; the innate-attribute display reads `_init_level` on its own. So this is one of
    /// the few places that goes to `Qualities::attribute` rather than through the inquiry.
    ///
    /// `None` is no `0x0013` yet.
    fn character_info(&self) -> Option<dereth_client_contract::CharacterInfo> {
        use dereth_rules::attributes::inq_attribute;
        let q = self.player_desc()?;
        // `ID_CharacterInfo_Innates`' display order: 1, 2, 4, 3, 5, 6.
        let innate_of = |id: u32| {
            i32::try_from(q.attribute(id).map_or(0, |a| a.init_level)).unwrap_or(i32::MAX)
        };
        let innate = [
            innate_of(1),
            innate_of(2),
            innate_of(4),
            innate_of(3),
            innate_of(5),
            innate_of(6),
        ];
        let raw =
            |id: u32| i32::try_from(inq_attribute(q, id, true).unwrap_or(0)).unwrap_or(i32::MAX);
        let augmentations = q.inq_int(AUG_INCREASED_CARRYING_CAPACITY);
        // Strength is set to 10 before the attribute query `(1, &strength, raw = 0)` -- the
        // **default is 10**, and it is the value `EncumbranceCapacity` is handed, not the raw one
        // the resist ladder uses.
        let cap_strength =
            i32::try_from(inq_attribute(q, 1, false).unwrap_or(10)).unwrap_or(i32::MAX);
        Some(dereth_client_contract::CharacterInfo {
            innate,
            // Retail defaults the value to `0x578` before the inquiry, i.e. 1400 is the
            // client-side default.
            chess_rank: match q.inq_int(CHESS_RANK) {
                0 => 1_400,
                n => n,
            },
            fishing_skill: q.inq_int(FAKE_FISHING_SKILL),
            num_deaths: q.inq_int(NUM_DEATHS),
            strength: raw(1),
            endurance: raw(2),
            load: dereth_rules::burden::inq_load_in(q, &self.hud.era.world_rules),
            encumbrance: q.inq_int(ENCUMBRANCE_VAL),
            capacity: dereth_rules::burden::encumbrance_capacity_in(
                cap_strength,
                augmentations,
                &self.hud.era.world_rules,
            ),
            augmentations,
            // The birth/age/deaths read-out's first two arms are
            // gated on the int-quality query's **return**, so `None` and `Some(0)` are different
            // sheets: a character with no `CreationTimestamp` gets no birth line at all, and one
            // stamped at the epoch gets `You were born on 1/1/1970 12:00:00 AM.`
            created: int_opt(q, charinfo::prop::CREATION_TIMESTAMP),
            age: int_opt(q, charinfo::prop::AGE),
            enlightenment: int_opt(q, charinfo::prop::ENLIGHTENMENT),
            melee_mastery: q.inq_int(charinfo::prop::WEAPON_MASTERY),
            ranged_mastery: q.inq_int(charinfo::prop::MISSILE_MASTERY),
            summoning_mastery: q.inq_int(charinfo::prop::SUMMONING_MASTERY),
            // The augmentations read-out asks for fifty-four ints and shows the ones
            // that came back positive. An absent key and a zero are the same answer here,
            // which is why the zeroes are dropped rather than stored.
            aug_ints: dereth_presentation::character::LUMINANCE
                .iter()
                .map(|(id, _, _)| *id)
                .chain(
                    dereth_presentation::character::AUGMENTATIONS
                        .iter()
                        .map(|(id, _)| *id),
                )
                .map(|id| (id, q.inq_int(id)))
                .filter(|(_, v)| *v != 0)
                .collect(),
            // The client runs the timestamp through `localtime` before
            // `strftime("%c")`, so the born line is in the machine's zone for the **birth**
            // instant — a character created in July reads in daylight time whatever month it is
            // read in, which is what `localtime` does and a `now`-based offset would not.
            utc_offset_secs: int_opt(q, charinfo::prop::CREATION_TIMESTAMP)
                .map_or(0, |t| utc_offset_secs(i64::from(t))),
        })
    }

    /// The burden drawn by the backpack meter and text, through
    /// `dereth_client_model::inventory::burden::inq_load`.
    ///
    /// The source is the canonical local-player object row. Before that row exists, the parked
    /// `0x0013` description supplies the attribute cache and `EncumbranceVal`; once registered,
    /// both names refer to the same qualities.
    fn load(&self) -> Option<f32> {
        Some(dereth_rules::burden::inq_load_in(
            self.player_desc()?,
            &self.hud.era.world_rules,
        ))
    }

    /// The object's public-description decoration fields used by the item-display update.
    ///
    /// `is_container` is the client's own three-term test
    /// `(bitfield & 0x800000) || items_capacity || containers_capacity`, and `0x800000` is
    /// an object that has to occupy a
    /// *container* slot, which is what a pack is, verified against the retail enum table.
    fn slot_decoration(&self, id: ObjectId) -> Option<dereth_client_contract::SlotDecoration> {
        /// Public-description flag requiring a pack slot.
        const BF_REQUIRES_PACKSLOT: u32 = 0x0080_0000;
        /// Public-description flag marking a readied item.
        const BF_OPENABLE: u32 = 0x0000_0001;
        let w = self.world.weenie(id)?;
        let items_capacity = dereth_rules::capacity::capacity(w.pwd.items_capacity.unwrap_or(0));
        let containers_capacity =
            dereth_rules::capacity::capacity(w.pwd.containers_capacity.unwrap_or(0));
        Some(dereth_client_contract::SlotDecoration {
            // A zero stack size counts as 1 — the tooltip update's own mapping.
            stack_size: u32::from(w.pwd.stack_size.unwrap_or(0)).max(1),
            is_container: w.pwd.bitfield & BF_REQUIRES_PACKSLOT != 0
                || items_capacity != 0
                || containers_capacity != 0,
            items_capacity,
            contained_items: i32::try_from(self.world.inventory(id).map_or(0, |i| i.items.len()))
                .unwrap_or(i32::MAX),
            structure: u32::from(w.pwd.structure.unwrap_or(0)),
            max_structure: u32::from(w.pwd.max_structure.unwrap_or(0)),
            // Written by selection updates; this is the selection ring's only input.
            selected: w.selected,
            // The object's sell and trade states: the Vendor panel writes sell state, while the
            // Trade panel writes trade state. The Salvage and Housing panels are the two remaining
            // writers with no window.
            sell_state: w.sell_state != 0,
            trade_state: w.trade_state != 0,
            openable: w.pwd.bitfield & BF_OPENABLE != 0,
            is_player: Some(id) == self.player(),
            containers_capacity,
            cooldown_id: w.pwd.cooldown_id.unwrap_or(0),
            cooldown_duration: w.pwd.cooldown_duration.unwrap_or(0.0),
            // The object's type selects the cell's background tile; without it every filled cell
            // would draw its icon on bare panel art.
            obj_type: w.pwd.obj_type,
            // The other four icon inputs, which
            // together with `obj_type` are exactly the five fields the icon update
            // compares before it decides to rebuild the composite.
            //
            // `_effects` is decoded with the public description. `icon_overlay_id` and
            // `icon_underlay_id` are **written** by
            // `dereth_client_model::weenie::mirror_stat_update`; this is where they are read.
            icon_id: w.pwd.icon_id,
            effects: w.pwd.effects.unwrap_or(0),
            icon_overlay_id: w.pwd.icon_overlay_id.map(DataId),
            icon_underlay_id: w.pwd.icon_underlay_id.map(DataId),
            // The waiting flag rides in the decoration, not only on
            // [`dereth_client_contract::GameView::item_waiting`], which covers the item list and
            // the doll but not the side-pack strip and the main-pack slot; without it a dragged
            // **backpack** would stay greyed for the session. The client has one mirror, in the
            // item tile's update, and it runs on every tile; carrying the flag in the decoration is
            // what lets it.
            waiting: w.waiting,
            // The cached shortcut number and its ghosted flag.
            //
            // The client caches the slot on the object: assigning a shortcut writes its index,
            // while removal writes `-1`, both from the same sparse shortcut array carried in the
            // player module. This asks the identical question one seam earlier — the same
            // reading removal's own sweep uses — and asking it
            // here cannot disagree with the bar, which is filled from that same array.
            shortcut_num: self
                .world
                .player_system
                .shortcut_slot_of(id)
                .and_then(|i| u32::try_from(i).ok()),
            // Combat mode 8 makes the toolbar inactive; every other mode makes it active. The
            // shortcut manager writes `ghost = !active` with the cached slot for all 18 entries,
            // and that update writes through to the
            // object, so it is a per-player fact and every
            // tile showing the object gets the dimmed numeral. Same predicate the bar itself uses
            // (`crate::toolbar::combat_mode::toolbar_active`), asked once here.
            shortcut_ghosted: !dereth_client_contract::combat_mode::toolbar_active(
                self.combat_mode(),
            ),
        })
    }

    /// Read cooldown state from the player's own registry, through
    /// `dereth_client_model::EnchantmentRegistry::cooldown_remaining`.
    ///
    /// Cooldown display reaches the registry through the canonical local-player object row,
    /// exactly as the vitals and vitae lamp do. `+ 0x8000` belongs to this caller, as it does in
    /// the client.
    ///
    /// The two guards above it are retained: no description yet, or
    /// one with no registry, is `None` and hides every wedge.
    fn cooldown_remaining(&self, cooldown_id: u32, now: f64) -> Option<f64> {
        self.player_desc()?
            .enchantments
            .cooldown_remaining(cooldown_id + 0x8000, dereth_primitives::LocalTime(now))
    }

    /// The plural-name query with flag 0 returns the stored `_plural_name`,
    /// with the material prefix, applied to singular and plural names alike.
    ///
    /// `None` still means "no `_plural_name`", which is the condition the widget's fallback keys
    /// on; the cached row carries `None` in exactly that case, so a cache hit cannot invent one.
    fn plural_name(&self, id: ObjectId) -> Option<&str> {
        if let Some(d) = self.hud.display_names.get(&id) {
            return d.plural.as_deref();
        }
        let n = self.world.weenie(id)?.pwd.plural_name.as_deref()?;
        (!n.is_empty()).then_some(n)
    }

    fn player_coords(&self) -> Option<(f32, f32)> {
        self.hud.coords
    }

    /// The identity the journal's page save needs. Composed in `App::frame`; see
    /// [`Hud::journal_identity`].
    fn journal(&self) -> dereth_client_contract::journal::JournalView {
        self.world.journal.view()
    }
    fn journal_identity(&self) -> Option<dereth_client_contract::journal::JournalIdentity> {
        self.hud.journal_identity.clone()
    }

    /// See [`Hud::game_date_time`].
    fn game_date_time(&self) -> Option<(String, String)> {
        self.hud.game_date_time.clone()
    }

    fn player_heading(&self) -> f32 {
        self.hud.heading
    }

    /// The shared outdoor-state predicate — see
    /// [`Hud::player_outside`], which is the single owner of the answer so that the radar's range
    /// and the chat sweep's radius cannot disagree.
    fn player_outside(&self) -> bool {
        self.hud.player_outside()
    }

    /// Current link status; see [`Hud::link_status`].
    fn link_status(&self) -> Option<f64> {
        self.hud.link_status
    }

    /// Current UI time as of this frame's `Hud::drive`.
    fn now(&self) -> f64 {
        self.hud.now.0
    }

    /// How many `0x01EA Character_ReturnPing` have been decoded — see
    /// [`crate::net::ping_holder`].
    fn ping_returns(&self) -> u64 {
        crate::net::ping_holder::returns()
    }

    /// Return the cached packet-loss percentage.
    ///
    /// The measurement is a received/expected window:
    /// [`dereth_client_net::linkstatus::LinkStatusAverages`]. The transport counts sent, received,
    /// retransmitted and NAKed packets, snapshots them every two seconds
    /// into a forty-sample ring, and [`crate::net::link_status_holder::on_packet_loss`] is
    /// the heartbeat's store of the packet-loss figure. This accessor is the read.
    fn packet_loss_percent(&self) -> f32 {
        crate::net::link_status_holder::packet_loss()
    }

    /// The quickbar object id at `slot`.
    ///
    /// The player module holds the array and `0x0013 Login_PlayerDescription` is what fills it, so
    /// the source here is the same `player_module` blob every player-module refresh reads. The
    /// wire form is a **sparse list keyed by `index`**, not a dense eighteen, so lookup is by
    /// `index` and not
    /// by position, exactly as `dereth_client_model::player::PlayerSystem::apply_player_module` does it.
    ///
    /// A shortcut whose object id is 0 is no shortcut: login skips such entries, and later
    /// insertion also returns early for a zero id.
    ///
    /// **Read the model's copy, or the bar takes no drop.** There is exactly one shortcut array in
    /// the client. The shortcut manager is its sole writer, both at login and on
    /// every drop. This build keeps **two** copies of that blob — [`Hud::player_module`], a
    /// verbatim clone of the `0x0013` bytes, and `dereth_client_model::player::PlayerSystem`, which
    /// `apply_player_module` fills from the same bytes by `index` — and the drop writes the
    /// second. `Hud::player_module` is never written again after login (`hud.rs`'s `0x0013` arm is
    /// its only writer), so reading it here would put every dragged shortcut in a store nothing
    /// draws from and leave the tile an empty numbered plate. The character options have the same
    /// two-copy hazard.
    fn shortcut(&self, slot: u32) -> Option<ObjectId> {
        let i = usize::try_from(slot).ok()?;
        let sc = (*self.world.player_system.shortcuts.get(i)?)?;
        (sc.object_id.0 != 0).then_some(sc.object_id)
    }

    /// The skill-list join — see [`Hud::build_skills`].
    fn skills(&self) -> &[SkillEntry] {
        &self.hud.skills
    }

    fn last_learned_spell(&self) -> Option<(u64, u32)> {
        self.world.magic.last_learned_spell
    }
    fn research_success(&self) -> Option<dereth_client_contract::research::ResearchSuccess> {
        self.world.magic.research_success.clone()
    }
    /// The player spellbook joined with the spell table; see [`Hud::build_spells`].
    fn spellbook(&self) -> &[SpellEntry] {
        &self.hud.spells
    }

    /// Resolve a spell from the table, independently of whether it is in the player's book;
    /// see [`Hud::spell_entry`] and the trait method's own note for why the two are different
    /// questions and why the endowment needs this one.
    fn spell(&self, spell_id: u32) -> Option<SpellEntry> {
        self.hud.spell_entry(spell_id)
    }

    /// `(spell)` — the **book itself**, not the join
    /// [`Self::spellbook`] hands the panel.
    ///
    /// Spellbook membership is a hash-table lookup, so a
    /// spell with a page is known whether or not the `SpellTable` has a row for it. That is what
    /// keeps spell-bar pruning from sending `0x01E4` for a spell
    /// the player still has.
    ///
    /// No qualities yet means the client has no local player description: retail's
    /// player-module refresh returns before the prune loop in that case, so answering "known"
    /// here is the arm that produces the same silence.
    fn is_spell_known(&self, spell_id: u32) -> bool {
        let Some(q) = self.player_desc() else {
            return true;
        };
        q.spell_book
            .as_ref()
            .is_some_and(|b| b.contains_key(&spell_id))
    }

    /// Spell examination's three dat joins, done where the tables are.
    ///
    /// * the spell-table row, which is also where the name,
    ///   description, school, `_base_mana`, `_mana_mod` and icon come from;
    /// * the appropriate-formula query — the same call
    ///   spell casting makes, so the pane lists the formula the player would actually spend
    ///   (scarab-only for an Infused augmentation or an owned spell pack, the customized one
    ///   otherwise);
    /// * the spell-component query for each slot
    ///   in the formula, **keyed by SCID** and left as
    ///   `None` on a miss so the pane can apply the client's own skip.
    ///
    /// The component table read is [`Hud::component_catalogue`] and **not** `.catalogue`,
    /// which is the same rows copied in on `0x0013`. The component table is
    /// the dat object itself, available from the moment the store is open and with no local player description
    /// in it; reading the world's copy would make a spell examined before the description landed
    /// list no components at all.
    ///
    /// Spell range is evaluated here too from the local player description: it uses the spell's
    /// own school skill, or the **best** of the five magic skills when the school is
    /// outside `1..=5`. Before a player description arrives, the skill is `0`, matching the
    /// client's null-interface result.
    ///
    /// **The formula is `get_appropriate_spell_formula`'s, not the plain spell-formula query's**. Both spell
    /// examination and casting call the selector. Those are its only two callers, which is
    /// why the pane and the cast can never disagree: a character with a Foci (or an
    /// `AugmentationInfused*Magic`) is shown *and* charged scarab + prismatic tapers, and every
    /// other character is shown *and* charged the full per-account formula. The customized arm
    /// and the foci arm's `school_of_magic_to_wcid` map are both transcribed.
    fn spell_examine(&self, spell_id: u32) -> Option<dereth_client_contract::SpellExamineView> {
        use dereth_client_contract::panels::spell_examine::{
            skill_for_spell, spell_range, MAGIC_SKILLS,
        };
        use dereth_client_contract::{SpellExamineComponent, SpellExamineView};

        let table = self.hud.spell_table.as_ref()?;
        let base = table.spells.get(&spell_id)?;
        let skill = {
            let q = self.player_desc();
            let want = skill_for_spell(base.school);
            let level = |id: u32| {
                q.map_or(0, |q| {
                    i32::try_from(dereth_rules::skills::inq_skill_level(q, id)).unwrap_or(0)
                })
            };
            if want == 0 {
                MAGIC_SKILLS.iter().map(|s| level(*s)).max().unwrap_or(0)
            } else {
                level(want)
            }
        };
        let formula = self.world.spell_formula(base);
        let n = dereth_rules::magic::num_spell_components(&formula);
        let components = formula
            .iter()
            .take(n)
            .map(|scid| {
                self.hud
                    .component_catalogue
                    .inq_spell_component_base(*scid)
                    .map(|b| SpellExamineComponent {
                        scid: *scid,
                        name: b.name.clone(),
                        icon: (b.icon != 0).then_some(DataId(b.icon)),
                    })
            })
            .collect();
        Some(SpellExamineView {
            name: base.name.clone(),
            description: base.description.clone(),
            school: base.school,
            base_mana: base.base_mana,
            mana_mod: base.mana_mod,
            // The pane uses `-1.0` when the meta-spell has no duration, exactly what
            // `SpellBase::duration` being `None` means.
            duration: base.duration.map_or(-1.0, |(d, _, _)| d),
            range: spell_range(base.base_range_constant, base.base_range_mod, skill),
            icon: (base.icon != 0).then_some(DataId(base.icon)),
            level: dereth_client_model::magic::spell_level_by_rough_heuristic(
                dereth_rules::magic::scarab_power_level(
                    dereth_client_contract::spellbook::power_component(
                        base.raw_comps[0],
                        base.comp_key,
                    ),
                ),
            ),
            icon_power: dereth_rules::magic::scarab_power_level(
                dereth_client_contract::spellbook::power_component(
                    base.raw_comps[0],
                    base.comp_key,
                ),
            ),
            bitfield: base.bitfield,
            components,
        })
    }

    /// Resolve the object carrying one spell component.
    ///
    /// Unlike [`Self::spell_examine`] above, this one reads the player's component tracker, **not**
    /// [`Hud::component_catalogue`]: the answer is about what the player is *carrying*, which only
    /// the tracker knows, and the tracker's rows were bucketed with the world's own copy of the
    /// SCID map.
    fn component_object_id(&self, scid: u32) -> Option<dereth_primitives::ObjectId> {
        self.world.component_object_id(scid)
    }

    /// Test ownership of the component keyed by the row's SCID.
    fn component_is_owned(&self, scid: u32) -> bool {
        self.world.spell_component_is_owned(scid)
    }

    /// The component-tracker change edge, represented as a serial.
    fn component_serial(&self) -> u64 {
        self.world.magic.component_serial
    }

    /// `(id, &v, raw = 0)` — one of the six primary attributes, for
    /// the Attributes panel.
    ///
    /// `dereth_client_model::attributes::inq_attribute` is retail's arithmetic; the `false`
    /// is the client's `raw = 0`, so the number is the enchanted one. The source is the canonical
    /// local-player object row; before that row exists, the parked `0x0013` description supplies
    /// the same qualities.
    fn attribute(&self, id: u32) -> Option<i32> {
        let q = self.player_desc()?;
        i32::try_from(dereth_rules::attributes::inq_attribute(q, id, false)?).ok()
    }

    /// The Skills panel's footer inputs for one skill.
    ///
    /// This is the caller of `dereth_client_model::advancement`'s four cost functions.
    ///
    /// The two `experience_to_skill_level` calls are the trained-skill footer's
    /// own, and they are made here rather than in the panel for the same reason the skill's name
    /// is: `dereth-ui-screens` must not depend on `dereth-client-model` or on the experience table.
    fn skill_advancement(&self, id: u32) -> Option<dereth_client_contract::SkillAdvancement> {
        let q = self.player_desc()?;
        let xp = self.hud.xp_table.as_ref()?;
        let skills = self.hud.skill_table.as_ref()?;
        let s = q.skill(id).copied().unwrap_or_default();
        let sac = dereth_rules::skills::Sac::from_raw(s.sac);
        let level = u32::from(s.level_from_pp);
        Some(dereth_client_contract::SkillAdvancement {
            sac: s.sac,
            pp: s.pp,
            level_from_pp: level,
            cost_to_raise: dereth_rules::advancement::skill_cost_to_raise(q, skills, xp, id),
            cost_to_raise_10: dereth_rules::advancement::skill_cost_to_raise_10(q, xp, id),
            // The experience-to-skill-level conversion returns `0xFFFFFFFF` for a skill below
            // TRAINED, which would
            // make the meter's span nonsense; the untrained footer has no meter, so 0/0 is the
            // honest answer and `meter_fill` reads a zero span as 0.0.
            xp_at_level: level_xp(xp, sac, level),
            xp_at_next_level: level_xp(xp, sac, level + 1),
        })
    }

    /// The Attributes panel's footer inputs for one of its nine rows.
    ///
    /// `id` is the row token's stat id, so for a vital it is the **odd**, maximum id (see
    /// `dereth_ui_screens::panels::attributes::AttributeRow::wire_stat`), and both `Attribute` and
    /// `SecondaryAttribute` records are reachable from either half of a pair.
    ///
    /// This is the caller of `dereth_client_model::advancement::{attribute_cost_to_raise,
    /// attribute_cost_to_raise_10}`, whose arithmetic matches the recorded
    /// `short-play-with-training` traffic.
    fn attribute_advancement(
        &self,
        id: u32,
        secondary: bool,
    ) -> Option<dereth_client_contract::AttributeAdvancement> {
        let q = self.player_desc()?;
        let xp = self.hud.xp_table.as_ref()?;
        let i32_of = |v: Option<u32>| v.and_then(|v| i32::try_from(v).ok()).unwrap_or(0);
        let (level_from_cp, cp_spent, value, effective, current, vitae) = if secondary {
            let s = q.attribute_2nd(id)?;
            // the formula base plus the stored rank,
            // enchanted unless `raw`. It needs the `Attribute2ndTable`; without it there is no
            // maximum, so the two display halves come out 0 while the **cost** below, which
            // reads only `_level_from_cp` and `_cp_spent`, is unaffected.
            let inq = |k: u32, raw: bool| -> i32 {
                self.hud.vitals_table.as_ref().map_or(0, |t| {
                    i32_of(dereth_rules::attributes::inq_attribute_2nd(
                        q,
                        t,
                        k,
                        raw,
                        self.hud.quality_filter.as_ref(),
                    ))
                })
            };
            let raw = inq(id, true);
            let eff = inq(id, false);
            // The stat id plus 1 — the current half of the same pair.
            let cur = inq(id + 1, false);
            // `Enchant(raw) - raw` with
            // **only** the vitae enchantment applied, so it is negative under a penalty and 0
            // otherwise. Not the same quantity as `eff - raw`, which also carries the spells.
            //
            // The multiplier is read straight off the registry rather than through
            // `GameView::vitae`, because **`HudView` does not implement that method**, so the
            // vitae lamp has no source; that is a separate gap.
            let vitae = dereth_presentation::inforegion::apply_vitae(
                raw,
                Some(q.enchantments.vitae_value()),
            ) - raw;
            (
                s.attribute.level_from_cp,
                s.attribute.cp_spent,
                raw,
                eff,
                cur,
                vitae,
            )
        } else {
            let a = q.attribute(id)?;
            let raw = i32_of(dereth_rules::attributes::inq_attribute(q, id, true));
            let eff = i32_of(dereth_rules::attributes::inq_attribute(q, id, false));
            // The base vitae-modifier query returns 0 and primary-attribute rows do not
            // override it: a primary attribute carries no vitae penalty.
            (a.level_from_cp, a.cp_spent, raw, eff, eff, 0)
        };
        Some(dereth_client_contract::AttributeAdvancement {
            level_from_cp,
            cp_spent,
            cost_to_raise: dereth_rules::advancement::attribute_cost_to_raise(
                xp,
                level_from_cp,
                cp_spent,
                secondary,
            ),
            cost_to_raise_10: dereth_rules::advancement::attribute_cost_to_raise_10(
                xp,
                level_from_cp,
                cp_spent,
                secondary,
            ),
            value,
            effective,
            current,
            vitae,
        })
    }

    /// The gender/heritage line for the player.
    ///
    /// It starts empty; a non-zero gender with a display name contributes that name. A non-zero
    /// heritage must have a display name (otherwise the answer is nothing) and is appended, after
    /// a space if a gender was written. With no heritage, a creature type's display name would be
    /// used instead.
    ///
    /// The character-info update passes no creature type, so the third arm is unreachable from this
    /// panel and is not implemented; the character-examination panel uses it and fills it
    /// in only when the heritage is 0.
    ///
    /// **Three heritages are hard-coded literals and do not come from the mapper.**
    /// The heritage display-name lookup answers `2` with `"Gharu'ndim"`, `5` with
    /// `"Umbraen"` and `0xD` with `"Olthoi"` before it ever reaches the mapper, whose
    /// own rows for those three are `Gharundim`, `Shadowbound` and `OlthoiAcid`.
    fn gender_heritage_display(&self) -> Option<String> {
        let q = self.player_desc()?;
        let gender = u32::try_from(q.inq_int(GENDER)).unwrap_or(0);
        let heritage = u32::try_from(q.inq_int(HERITAGE_GROUP)).unwrap_or(0);
        let lookup = |m: &Option<dereth_assets::tables::EnumMapper>, k: u32| -> Option<String> {
            m.as_ref()?
                .id_to_string
                .iter()
                .find(|(id, _)| *id == k)
                .map(|(_, s)| s.clone())
        };
        let mut out = String::new();
        if gender != 0 {
            if let Some(g) = lookup(&self.hud.gender_names, gender) {
                out.push_str(&g);
            }
        }
        if heritage != 0 {
            let h = match heritage {
                2 => Some("Gharu'ndim".to_owned()),
                5 => Some("Umbraen".to_owned()),
                0xD => Some("Olthoi".to_owned()),
                other => lookup(&self.hud.heritage_names, other),
            }?;
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&h);
        }
        (!out.is_empty()).then_some(out)
    }

    /// Resolve the currently displayed character title.
    ///
    /// `id == 0` is refused outright; otherwise the id becomes an `ID_CharacterTitle_*` token
    /// through `EnumMapper 0x22000041` and the token's string hash (`dereth_primitives::num::hash::str_hash`) indexes
    /// `StringTable 0x2300000E`.
    fn display_title(&self) -> Option<String> {
        self.title_name(self.hud.display_title)
    }

    /// The title table joined to its id lookup.
    ///
    /// The list is `player_system.social.titles`, which `0x0029` and `0x002B` write — see
    /// `Hud::ui_event`. Each id is resolved through the **same** `EnumMapper 0x22000041` /
    /// `StringTable 0x2300000E` pair [`Self::display_title`] uses, and an id the pair does not
    /// carry keeps the empty name rather than vanishing, so the panel can count it.
    fn character_titles(&self) -> dereth_client_contract::CharacterTitles {
        let social = &self.world.player_system.social;
        dereth_client_contract::CharacterTitles {
            display: self.hud.display_title,
            titles: social
                .titles
                .iter()
                .map(|id| (*id, self.title_name(*id).unwrap_or_default()))
                .collect(),
        }
    }

    /// Resolve the local player's PK status.
    ///
    /// The two predicates are `dereth_client_model::weenie::Weenie::{is_pk, is_pk_lite}`. The object is the
    /// **player's**, and a missing one is `NPK` rather than
    /// an error — the client's no-object arm falls onto the same string id.
    fn pk_status(&self) -> dereth_client_contract::PkStatus {
        let w = self.player().and_then(|p| self.world.weenie(p));
        w.map_or(dereth_client_contract::PkStatus::Npk, |w| {
            dereth_client_contract::PkStatus::of(w.is_pk(), w.is_pk_lite())
        })
    }

    /// Int64 qualities `6` and `7`.
    fn luminance(&self) -> (i64, i64) {
        let Some(q) = self.player_desc() else {
            return (0, 0);
        };
        let int64 = |p: u32| match q.get(dereth_client_model::StatKey::new(
            dereth_client_model::StatType::Int64,
            p,
        )) {
            Some(dereth_client_model::StatValue::Int64(n)) => n,
            _ => 0,
        };
        (int64(AVAILABLE_LUMINANCE), int64(MAXIMUM_LUMINANCE))
    }

    /// The name text's source.
    ///
    /// The client reads the player's object name first, then falls back to string property 1 on
    /// the local player description. The fallback is not a
    /// convenience: `0x0013` arrives before `0xF745 Object_CreatePlayer`, so between the two the
    /// object table has no name and the header would come up blank — the same ordering
    /// the Vitals panel works around by reading the local player description.
    ///
    /// **The allegiance rank prefix is not applied.** No capture carries an
    /// allegiance, so there is nothing to prefix with and nothing to test it against; it is left
    /// out rather than invented.
    fn character_name(&self) -> Option<&str> {
        if let Some(n) = self.player().and_then(|p| {
            let n = self.world.weenie(p)?.pwd.name.as_str();
            (!n.is_empty()).then_some(n)
        }) {
            return Some(n);
        }
        match self.player_desc()?.get(dereth_client_model::StatKey::new(
            dereth_client_model::StatType::String,
            CHARACTER_NAME,
        )) {
            Some(dereth_client_model::StatValue::Str(s)) if !s.is_empty() => {
                // `StatValue::Str` hands back an owned copy, and the trait returns a borrow, so the
                // name is looked up again on the map that owns it.
                self.player_desc()?
                    .strings
                    .as_ref()?
                    .get(&CHARACTER_NAME)
                    .map(String::as_str)
            }
            _ => None,
        }
    }

    /// The experience header's inputs.
    ///
    /// `dereth_client_model::advancement::experience_header` is retail's arithmetic — including
    /// both Throne-of-Destiny behaviours, the 2³²−1 saturation and the level-126 "Infinity!".
    ///
    /// **Throne of Destiny is assumed present.** There is no account-entitlement counterpart here;
    /// every retail account after 2005 has it, and the two behaviors it gates
    /// are only reachable above 2³² total experience or at exactly level 126.
    /// stated rather than silently defaulted.
    fn experience_header(&self) -> Option<dereth_client_contract::statmgmt::XpHeader> {
        let q = self.player_desc()?;
        let xp = self.hud.xp_table.as_ref()?;
        let int64 = |p: u32| match q.get(dereth_client_model::StatKey::new(
            dereth_client_model::StatType::Int64,
            p,
        )) {
            Some(dereth_client_model::StatValue::Int64(n)) => n,
            _ => 0,
        };
        let total = u64::try_from(int64(TOTAL_EXPERIENCE)).unwrap_or(0);
        let level = q.inq_int(LEVEL);
        let h = dereth_rules::advancement::experience_header(xp, total, level, true);
        let this_level = dereth_rules::advancement::experience_to_level(
            xp,
            usize::try_from(level.max(0)).unwrap_or(0),
        )
        .unwrap_or(0);
        let next_level = dereth_rules::advancement::experience_to_level(
            xp,
            usize::try_from(level.max(0)).unwrap_or(0) + 1,
        )
        .unwrap_or(0);
        Some(dereth_client_contract::statmgmt::XpHeader {
            total: h.total,
            into_level: h.xp_into_level,
            level_span: next_level.saturating_sub(this_level),
            to_level: h.xp_to_level,
            level,
            at_cap: h.at_cap,
        })
    }

    /// The Vendor panel's state. The whole of the conversion, including the four
    /// `dereth_client_model::vendor` functions it calls, is [`crate::vendor_view::shop`].
    /// Declared crossing: one line, alongside the identical `allegiance_roster` below.
    fn shop(&self) -> dereth_client_contract::ShopView {
        crate::vendor_view::shop(self.world)
    }

    /// The Trade panel's state. The whole of the conversion is [`crate::trade_view::trade`].
    /// Declared crossing, on the same terms as `shop` above.
    fn trade(&self) -> dereth_client_contract::TradeView {
        crate::trade_view::trade(self.world)
    }

    /// Test whether the vendor accepts a dragged item `(id, true)`. Same
    /// declared crossing: one line, and the whole decision is the inventory predicate.
    fn vendor_drag_item_accepted(&self, item: ObjectId) -> bool {
        self.world.drag_item_accepted(item)
    }

    /// `(id, quiet = 1)`'s ownership half.
    /// The same declared crossing the line above is: one line, and the whole
    /// decision is the inventory predicate -- which the drop path also calls, so the hint and the
    /// drop cannot disagree about ownership.
    fn trade_drag_item_acceptable(&self, item: ObjectId) -> bool {
        self.world.trade_item_acceptable(item).is_none()
    }

    /// `(id, quiet = 1)` over the window's
    /// own ground-object id.
    ///
    /// The hook-acceptance predicate is transcribed here rather than in `dereth_client_model`
    /// because both of its inputs are `PublicWeenieDesc` fields this file already reads the same
    /// way for `item_valid_locations`, and because its two callers on the *drop* side
    /// (the container-placement attempt and the drag-into-container legality check)
    /// take the `OPENABLE` route the game model already owns -- adding a
    /// second entry point there would duplicate that logic.
    ///
    /// `Weenie::is_hook` reproduces the native hook predicate's pair of tests: a
    /// container that is not a hook accepts everything, which is every chest, corpse and ground
    /// pack in the game.
    fn external_container_drag_item_acceptable(&self, item: ObjectId, ground: ObjectId) -> bool {
        // An unknown ground object accepts; an unknown item refuses.
        let Some(container) = self.world.weenie(ground) else {
            return true;
        };
        let Some(carried) = self.world.weenie(item) else {
            return false;
        };
        if !container.is_hook() {
            return true; // the accept arm, and the only one a chest or a corpse can reach.
        }
        // The next test: a hook with no house owner refuses, and sets the out-parameter nothing in
        // this path reads.
        if container.pwd.house_owner_iid.unwrap_or(ObjectId(0)).0 == 0 {
            return false;
        }
        let valid = u32::from(carried.pwd.hook_type.unwrap_or(0));
        let hook = u32::from(container.pwd.hook_type.unwrap_or(0));
        // Test the carried object's own hook-type mask against the
        // hook's location, then the item type against the hook's `_hook_item_types`.
        valid != 0
            && hook & valid != 0
            && carried.pwd.obj_type & container.pwd.hook_item_types.unwrap_or(0) != 0
    }

    /// The item's `pwd._valid_locations`, for the equipment-slot check.
    /// `None` when the object is not in the table.
    fn item_valid_locations(&self, item: ObjectId) -> Option<u32> {
        self.world
            .weenie(item)
            .map(|w| w.pwd.valid_locations.unwrap_or(0))
    }

    fn equipment_hover(&self, item: ObjectId) -> dereth_client_contract::view::EquipmentHover {
        self.world.equipment_hover(item)
    }

    /// The client predicate, answered by
    /// `dereth_client_model::inventory::salvage::is_item_suitable`.
    ///
    /// The option is read here rather than in the panel because `SalvageMultiple` is a
    /// `PlayerModule` bit. It is the third of the four tests and belongs to the player, not the item.
    fn payment_lists(&self) -> dereth_client_contract::panels::slumlord::PaymentListsView {
        self.world.payment_lists_view()
    }
    fn salvage_list(&self) -> dereth_client_contract::panels::salvage::SalvageListView {
        self.world.salvage_list_view()
    }
    fn salvage_item_suitable(&self, item: ObjectId, panel_material: u32) -> bool {
        let Some(w) = self.world.weenie(item) else {
            return false;
        };
        let multiple = character_option(
            self.world,
            dereth_client_contract::PlayerOption::SalvageMultiple,
        )
        .unwrap_or(false);
        dereth_client_model::inventory::salvage::is_item_suitable(w, multiple, panel_material)
    }

    /// The client predicate -- `drag_item_acceptable`.
    fn item_owned_by_player(&self, item: ObjectId) -> bool {
        self.world.is_owned_by_player(item)
    }

    /// The item's material type, for the window's material latch.
    fn item_material_type(&self, item: ObjectId) -> u32 {
        self.world
            .weenie(item)
            .and_then(|w| w.pwd.material_type)
            .unwrap_or(0)
    }

    /// The item's class id —
    /// `drag_item_acceptable` hands it to the needs-more check and the item add puts it in
    /// the `HousePayment` it builds.
    fn item_wcid(&self, item: ObjectId) -> u32 {
        self.world.weenie(item).map_or(0, |w| w.pwd.wcid)
    }

    /// A single unstacked item pays **one**, while a stack pays its stack size.
    /// Note what this is *not*:
    /// it does not consult the item's value, which is why a trade note needs
    /// a trade note's value, which is handled further down in `HousePaymentList`.
    fn item_house_payment(&self, item: ObjectId) -> i32 {
        self.world
            .weenie(item)
            .and_then(|w| w.pwd.stack_size)
            .map_or(1, |n| if n == 0 { 1 } else { i32::from(n) })
    }

    /// The slumlord and its owner id, projected for the window.
    fn slumlord(&self) -> Option<dereth_client_contract::SlumlordView> {
        let (slumlord, p) = self.world.slumlord.as_ref()?;
        Some(dereth_client_contract::SlumlordView {
            slumlord: *slumlord,
            owner: p.owner,
            owner_name: p.name.clone(),
            house_type: p.house_type,
            // Am-I-the-house-owner — `_owner` equals the player id, and with no
            // world objects at all it is `_owner == 0`.
            am_i_the_owner: self.world.player.unwrap_or(ObjectId(0)) == p.owner,
        })
    }

    /// The Mini Game panel's own fields.
    ///
    /// `Some` unconditionally, because the window exists in the shipped tree whether or not a game
    /// is on and a current game id of 0 is how "no game" is represented.
    fn minigame(&self) -> Option<dereth_client_contract::MiniGameView> {
        let g = &self.world.minigame;
        let mut piece_slots = [None; 64];
        for piece in &g.board.logic.pieces {
            let within_side = match piece.piece_type {
                dereth_rules::chess::PieceType::Empty => continue,
                dereth_rules::chess::PieceType::Pawn => 0,
                dereth_rules::chess::PieceType::Rook => 3,
                dereth_rules::chess::PieceType::Bishop => 1,
                dereth_rules::chess::PieceType::Knight => 2,
                dereth_rules::chess::PieceType::Queen => 4,
                dereth_rules::chess::PieceType::King => 5,
            };
            let side = match piece.player {
                0 => 0,
                1 => 6,
                _ => continue,
            };
            let Some(cell) =
                dereth_client_model::minigame::GameBoard::cell_of_coord(piece.cur_pos, g.team)
            else {
                continue;
            };
            piece_slots[cell] = Some(side + within_side);
        }
        Some(dereth_client_contract::MiniGameView {
            visible: g.visible,
            team: g.team,
            game: g.current_game,
            // The draw's index arithmetic, which is team-dependent and therefore
            // the model's to apply.
            selected_cell: g
                .board
                .selected
                .and_then(|c| dereth_client_model::minigame::GameBoard::cell_of_coord(c, g.team)),
            piece_slots,
            draws: g.board.draws,
            stalemate: g.stalemate,
        })
    }

    fn slumlord_notices(&self) -> u64 {
        self.hud.stats.house_profile_notices
    }

    /// Copy the payment profile, apply every dropped payment, then compose the
    /// requirements and paid-in-full result over that updated copy.
    ///
    /// The replay is the payment: retail mutates the working profile when a row is added and
    /// reverses it when a row is removed, while preserving a pristine backup. Here `p` begins as
    /// that pristine clone and the panel's row list supplies the mutations, so the two cannot drift.
    fn slumlord_payment(
        &self,
        rent: bool,
        drops: &[(u32, i32, Option<i32>)],
    ) -> dereth_client_contract::SlumlordPayment {
        use dereth_client_model::housing::HouseOp;
        let Some((_, p)) = self.world.slumlord.as_ref() else {
            return dereth_client_contract::SlumlordPayment::default();
        };
        let op = if rent { HouseOp::Rent } else { HouseOp::Buy };
        let mut p = p.clone();
        for (wcid, amount, trade_note_value) in drops {
            p.pay(op, *wcid, *amount, *trade_note_value);
        }
        dereth_client_contract::SlumlordPayment {
            // The house refresh uses `compose_text` for the buy tab and
            // `compose_text2` for the rent tab. The asymmetry is retail's.
            requirements: if rent {
                p.compose_text2(op)
            } else {
                p.compose_text(op)
            },
            // The button update always asks about **`HouseOp::Buy`**, whichever tab is up.
            paid_in_full: p.op_is_paid_in_full(HouseOp::Buy),
        }
    }

    /// `(op, wcid)` over the same replay.
    fn slumlord_needs_more(
        &self,
        rent: bool,
        drops: &[(u32, i32, Option<i32>)],
        wcid: u32,
        trade_note_value: Option<i32>,
    ) -> bool {
        use dereth_client_model::housing::HouseOp;
        let Some((_, p)) = self.world.slumlord.as_ref() else {
            return false;
        };
        let op = if rent { HouseOp::Rent } else { HouseOp::Buy };
        let mut p = p.clone();
        for (w, amount, note) in drops {
            p.pay(op, *w, *amount, *note);
        }
        p.needs_more(op, wcid, trade_note_value) != 0
    }

    /// `(op, {wcid, num})` over the same replay.
    fn slumlord_pay(
        &self,
        rent: bool,
        drops: &[(u32, i32, Option<i32>)],
        wcid: u32,
        amount: i32,
        trade_note_value: Option<i32>,
    ) -> bool {
        use dereth_client_model::housing::HouseOp;
        let Some((_, p)) = self.world.slumlord.as_ref() else {
            return false;
        };
        let op = if rent { HouseOp::Rent } else { HouseOp::Buy };
        let mut p = p.clone();
        for (w, a, note) in drops {
            p.pay(op, *w, *a, *note);
        }
        p.pay(op, wcid, amount, trade_note_value)
    }

    fn item_trade_note_value(&self, item: ObjectId) -> Option<i32> {
        let wcid = self.world.weenie(item)?.pwd.wcid;
        self.hud
            .trade_note_values
            .iter()
            .find(|(_, mapped_wcid)| *mapped_wcid == wcid)
            .and_then(|(value, _)| i32::try_from(*value).ok())
    }

    /// Same declared crossing.
    fn selected_object(&self) -> Option<ObjectId> {
        self.world.selected
    }

    /// The client predicate: the spell formula's targeting type is zero.
    ///
    /// `dereth_client_model::magic::spell_target_type` is that walk (on the decrypted formula, and only
    /// when its first five slots are filled: the last filled slot of the run that starts at slot 5,
    /// or slot 4 when slot 5 is empty, through the component target-type lookup), and it is the same
    /// result the
    /// cast path branches on at its untargeted arm — so the button lights for
    /// exactly the spells the cast path will send the untargeted-cast request `0x0048` for.
    fn spell_is_untargeted(&self, spell_id: u32) -> bool {
        let Some(base) = self
            .hud
            .spell_table
            .as_ref()
            .and_then(|t| t.spells.get(&spell_id))
        else {
            return false;
        };
        dereth_client_model::magic::spell_target_type(base) == 0
    }

    /// Check whether the selected object is compatible with the spell, quietly —
    /// the **quiet** call, unlike the spell-casting path's.
    fn spell_target_compatible(&self, spell_id: u32) -> bool {
        let Some(target) = self.world.selected else {
            return false;
        };
        let Some(base) = self
            .hud
            .spell_table
            .as_ref()
            .and_then(|t| t.spells.get(&spell_id))
        else {
            return false;
        };
        let mask = dereth_client_model::magic::spell_target_type(base);
        // `quiet = 1`: the sink is a local nobody reads, as in the
        // client. Building a tooltip must not put a line in the chat.
        let mut quiet = dereth_client_model::NullSink;
        self.world
            .object_compatible_with_spell_target_type(&mut quiet, Some(target), mask, true)
            .is_ok()
    }

    /// Whether the item's target-use flags permit using it on the player.
    fn useability(&self, object: ObjectId) -> Option<u32> {
        self.world.weenie(object)?.pwd.useability
    }

    fn item_useable_self_target(&self, item: ObjectId) -> bool {
        let Some(w) = self.world.weenie(item) else {
            return false;
        };
        crate::cursor::ItemUses(w.pwd.useability.unwrap_or(0)).is_useable_self_target()
    }

    /// Check whether the selected target is compatible with the item, quietly —
    /// target first, source second.
    fn item_target_compatible(&self, item: ObjectId) -> bool {
        let Some(target) = self.world.selected else {
            return false;
        };
        self.world.target_compatible_with_object(target, item)
    }

    /// The allegiance hierarchy, walked. The whole of the conversion, including
    /// the four hierarchy walks, is [`crate::allegiance_view::roster`].
    fn oath_xp_cost(&self) -> Option<u32> {
        if !self.era_features().swear_xp_cost {
            return None;
        }
        let xp = self.experience_header()?;
        let breaks = self
            .player()
            .and_then(|p| self.int_stat(p, 0x84))
            .and_then(|b| u32::try_from(b).ok())
            .unwrap_or(0);
        let span = if xp.level_span == 0 {
            u64::from(u32::MAX)
        } else {
            xp.level_span
        };
        Some(dereth_rules::allegiance::swear_xp_cost_after_breaks(
            span, breaks,
        ))
    }
    fn allegiance_roster(&self) -> dereth_client_contract::AllegianceRoster {
        crate::allegiance_view::roster(
            self.world,
            self.player_desc(),
            self.hud.quality_filter.as_ref(),
        )
    }

    /// `0x0003`'s count, straight off the world — see
    /// [`dereth_client_contract::GameView::allegiance_update_aborts`].
    fn allegiance_update_aborts(&self) -> u64 {
        self.world.allegiance_aborts
    }

    /// `0x0020`'s count, straight off the world — see
    /// [`dereth_client_contract::GameView::allegiance_updates`].
    fn allegiance_updates(&self) -> u64 {
        self.world.allegiance_updates
    }

    /// **The Portal Storm indicator's one input.**
    ///
    /// `dereth_ui_screens::hud::indicators::portal_storm_state` and the lamp's row in
    /// `screens::gameplay` read the trait's default `0.0` unless this is overridden: the four
    /// `0x02C9`…`0x02CC` arms write the level, and `dereth_client_model::portal_storm` is their
    /// transcription.
    fn portal_storm_level(&self) -> f32 {
        self.world.portal_storm_level
    }

    /// The House panel's trigger for the *clearing* update.
    ///
    /// It is a count and not a `HouseData` because `0x0226`'s payload is one `u32` that retail
    /// keeps nowhere — see [`HudStats::house_status_last_notice`] and the `0x0226` arm. The real
    /// view type sits beside it rather than widening this: [`Self::house_data`].
    fn house_status_notices(&self) -> u64 {
        self.hud.stats.house_status_notices
    }

    /// The House panel's view, projected for the six sections that read it.
    ///
    /// Everything here uses `dereth_client_model::housing` to compose payment text, compute both rent-period
    /// boundaries, and construct the warning message, plus the three
    /// physics-crate calls, which are the reason this crossing exists at all: `dereth-ui-screens`
    /// has no `dereth-physics`, and `dereth-client-model` does not either.
    fn house_data(&self) -> Option<dereth_client_contract::HouseDataView> {
        use dereth_client_model::housing;
        let h = self.world.house.as_ref()?;
        let rent_owed = h.rent_is_owed();
        // Retail's House tab groups every payment count using the shipped language rule, as a
        // side-by-side retail comparison shows. Apply that rule at this projection boundary: the
        // shared `*` methods also feed the slumlord window, whose visible grouping has not been
        // established.
        let payment_text = |list: &housing::HousePaymentList, show_paid: bool| {
            list.0
                .iter()
                .map(|payment| {
                    let required = dereth_presentation::numfmt::number(i64::from(payment.num));
                    let name = payment.get_name(payment.num);
                    if show_paid {
                        let paid = dereth_presentation::numfmt::number(i64::from(payment.paid));
                        format!("{paid}/{required} {name}")
                    } else {
                        format!("{required} {name}")
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        Some(dereth_client_contract::HouseDataView {
            buy_text: payment_text(&h.buy, false),
            rent_text: payment_text(&h.rent, true),
            buy_time: i64::from(h.buy_time),
            maintenance_period_end: h.maintenance_period_end(),
            maintenance_next_due: h.maintenance_next_due(),
            location: self.world.house_location(house_lcoord),
            rent_owed,
            rent_warning: if rent_owed {
                housing::construct_rent_warning_message(housing::rent_period_days(h.house_type))
            } else {
                String::new()
            },
            // `convert_time` is called once per row and calls
            // `localtime` each time, so each instant gets the zone *it* falls in.
            // A house bought last winter with maintenance due next summer draws two different
            // daylight offsets on retail, and here.
            utc_offset_secs: [
                utc_offset_secs(i64::from(h.buy_time)),
                utc_offset_secs(h.maintenance_period_end()),
                utc_offset_secs(h.maintenance_next_due()),
            ],
        })
    }

    /// The pane's other redraw edge — see [`HudStats::house_data_notices`].
    fn house_data_notices(&self) -> u64 {
        self.hud.stats.house_data_notices
    }

    /// The zone `localtime` would use *now*, for any panel that formats an
    /// instant it did not carry an offset for. The dated views above each carry their own,
    /// because `localtime`'s answer depends on which instant it is given.
    fn utc_offset_secs(&self) -> i32 {
        utc_offset_secs(wall_clock_unix())
    }

    /// The purchase-time display's local-player-description half.
    ///
    /// Resolving the local player description is `player_qualities()` answering `Some` — the
    /// interface is the local player's quality bag and nothing else — and int quality `0xC7` is
    /// `PropertyInt::HousePurchaseTimestamp`. The client pre-zeroes the output and ignores the
    /// query's return value, so an absent quality reads `0`, which
    /// `has_purchase_wait_period_expired` always calls expired.
    ///
    /// The seed is CRT `time(NULL)`; [`wall_clock_unix`] is this tree's
    /// transcription of it and `chat_real_time` in `interaction.rs` is the same call.
    fn house_purchase(&self) -> dereth_client_contract::HousePurchaseView {
        let Some(q) = self.world.player_qualities() else {
            return dereth_client_contract::HousePurchaseView::default();
        };
        let t = q.inq_int(dereth_client_model::housing::HOUSE_PURCHASE_TIMESTAMP);
        dereth_client_contract::HousePurchaseView {
            have_player_desc: true,
            purchase_timestamp: t,
            wait_expired: dereth_client_model::housing::has_purchase_wait_period_expired(
                wall_clock_unix(),
                i64::from(t),
            ),
            // `localtime` on int quality `0xC7` + `0x278D00` — the instant
            // the panel prints, thirty days on, not the raw quality.
            utc_offset_secs: utc_offset_secs(
                i64::from(t) + dereth_client_contract::panels::house::PURCHASE_WAIT_SECONDS,
            ),
        }
    }

    /// Same declared crossing as the line above: the
    /// conversion is three fields, and the panel does its own sorted insert because
    /// the sorted-insert-position search is the panel's function and not the list's.
    fn friends(&self) -> Vec<dereth_client_contract::FriendEntry> {
        self.world
            .friends()
            .iter()
            .map(|f| dereth_client_contract::FriendEntry {
                id: f.id,
                name: f.name.clone(),
                online: f.online,
            })
            .collect()
    }

    /// The squelch-list iteration. Same declared crossing as the line above:
    /// the walk itself is `dereth_client_model::chat::ChatState::squelch_iteration` — it belongs to the
    /// communication system and not to the panel — and the conversion here is two fields.
    ///
    /// The model this reads has one production writer (`recv_set_squelch_db`, the `0x01F4`
    /// arm), and this is its only reader outside the chat router, so a broken seam here does
    /// not show anywhere else.
    fn squelch_list(&self) -> Vec<dereth_client_contract::SquelchEntry> {
        self.world
            .chat
            .squelch_iteration()
            .into_iter()
            .map(|(name, account)| dereth_client_contract::SquelchEntry { name, account })
            .collect()
    }

    /// Join the player's contract-tracker rows to the DAT contract table.
    /// As with the two queries above, this layer resolves the data so `dereth-ui-screens`
    /// need not access the DAT object.
    ///
    /// Every string here is produced by a `dereth_client_model::quests` function, so the panel transcribes
    /// none of them a second time. The two `Option`s are the one
    /// case `update_buttons` writes **nothing at all**: a `Position` whose `objcell_id` is zero
    /// (`objcell_id == 0`).
    ///
    /// A tracker whose contract the dat does not carry is **dropped and counted**
    /// ([`HudStats::contracts_unresolved`]); the reference client assumes the lookup succeeds.
    fn contracts(&self) -> Vec<dereth_client_contract::ContractEntry> {
        use dereth_client_model::quests;
        let now = dereth_primitives::ServerTime(self.hud.now.0);
        let fmt = |s: f64| {
            dereth_client_contract::journal::delta_time_to_string(i64::from(
                dereth_primitives::num::to_i32_f64(s),
            ))
        };
        let Some(table) = self.hud.contract_table.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (id, t) in self.world.contract_trackers() {
            let Some(raw) = table.contracts.get(id) else {
                continue;
            };
            let c = contract_of(raw);
            let end = quests::contact_is_the_end_npc(&c, t.contract_stage);
            let (contact, contact_pos) = if end {
                (c.name_npc_end.clone(), raw.location_npc_end)
            } else {
                (c.name_npc_start.clone(), raw.location_npc_start)
            };
            out.push(dereth_client_contract::ContractEntry {
                contract_id: *id,
                name: c.contract_name.clone(),
                status: quests::fill_progress_string(t, &c, now, &fmt),
                stage: t.contract_stage,
                description: c.description.clone(),
                contact,
                contact_location: contract_location(contact_pos),
                area_location: contract_location(raw.location_quest_area),
                timed: quests::timed_string(t, &c, now, &fmt),
                // The contract sort-status guard, verbatim.
                repeat_remaining: (t.contract_stage == quests::stage::DONE
                    && t.time_of_server_update > 0.0)
                    .then_some(t.time_when_repeats - (now.0 - t.time_of_server_update)),
            });
        }
        out
    }

    /// Look up `id` in the allegiance profile, for
    /// the allegiance UI's final swear-button condition.
    fn allegiance_has_member(&self, id: ObjectId) -> bool {
        self.world.allegiance.look_up(id).is_some()
    }

    /// The player's `InstanceID` quality **26 (`Monarch`)**, mirrored by
    /// `dereth_client_model::weenie::mirror_stat_update`'s key-26 arm from `0x02DA`.
    fn allegiance_monarch_quality(&self) -> Option<ObjectId> {
        let player = self.world.player?;
        self.world.weenie(player)?.pwd.monarch.filter(|m| m.0 != 0)
    }
    /// The player's fellowship pointer, as the fellowship UI needs
    /// it.
    ///
    /// The one thing computed here rather than carried is the per-fellow experience share, and it
    /// is computed here for the reason [`Self::skill_advancement`]'s two lookups are: both the
    /// even-split and level-proportional paths read the experience table,
    /// and `dereth-ui-screens` must not depend on `dereth-client-model` or on a dat table.
    ///
    /// The arithmetic returns zero when sharing is disabled, uses the member count for an even
    /// split, or divides this member's level proportion by the fellowship total. It then multiplies
    /// by 100 and truncates toward zero.
    ///
    /// With no experience table loaded the third arm cannot be taken and the share reads `0`,
    /// which is the same number the *share off* arm produces — the two are not distinguishable in
    /// the panel and are not meant to be: the client shows one integer.
    fn fellowship(&self) -> Option<dereth_client_contract::FellowshipView> {
        use dereth_client_contract::{FellowEntry, FellowshipView};
        let f = self.world.fellowship.as_ref()?;
        let xp = self.hud.xp_table.as_ref();
        let sum = xp.map_or(0, |t| f.experience_proportion_sum(t));
        // The share is held as an `f32` (the panel stores it single precision before the multiply),
        // then widened: that is what makes six members read 44% rather than 45%.
        let even = f64::from(dereth_rules::fellowship::even_split_xp_percentage(
            f.members.len(),
        ));
        let members = f
            .members
            .iter()
            .map(|(id, m)| {
                let share = if !f.share_xp {
                    0.0
                } else if f.even_xp_split {
                    even
                } else if sum == 0 {
                    0.0
                } else {
                    #[allow(clippy::cast_precision_loss)]
                    let p = xp.map_or(0, |t| {
                        dereth_rules::fellowship::get_experience_proportion(t, m.level)
                    }) as f64;
                    #[allow(clippy::cast_precision_loss)]
                    let d = sum as f64;
                    #[allow(clippy::cast_possible_truncation)]
                    let share = (p / d) as f32;
                    f64::from(share)
                };
                let xp_percent = dereth_primitives::num::to_i32_f64(share * 100.0);
                FellowEntry {
                    id: *id,
                    name: m.name.clone(),
                    level: m.level,
                    xp_percent,
                    current_health: m.current_health,
                    max_health: m.max_health,
                    current_stamina: m.current_stamina,
                    max_stamina: m.max_stamina,
                    current_mana: m.current_mana,
                    max_mana: m.max_mana,
                }
            })
            .collect();
        Some(FellowshipView {
            name: f.name.clone(),
            leader: f.leader,
            share_xp: f.share_xp,
            even_xp_split: f.even_xp_split,
            open_fellow: f.open_fellow,
            locked: f.locked,
            members,
        })
    }

    /// `AppraisalProfile`'s six questions, for the identify panel.
    ///
    /// The cache behind it is `dereth_client_model::AppraisalCache`, filled by
    /// `0x00C9 Item_SetAppraiseInfo`; `Notice::AppraisalReady` reaches its consumer through this
    /// seam.
    ///
    /// Each field answers one appraisal-profile query; see
    /// [`dereth_client_contract::AppraisalView`] for the table.
    fn appraisal(
        &self,
        id: dereth_primitives::ObjectId,
    ) -> Option<dereth_client_contract::AppraisalView> {
        let p = self.world.appraisal.get(id)?;
        // The six questions below are what the frame and the value/burden
        // blocks need; everything after them is what the creature pane and the rest of
        // the examination panel's ordered blocks need. The profile is
        // decoded whole by the appraisal reader and cached whole by `AppraisalCache::set`;
        // **this function is where the fields are read**, and the panel draws only what it is
        // given.
        use dereth_client_model::appraisal_model as am;
        let w = self.world.weenie(id);
        // `pwd.valid_locations` / `pwd.ammo_type`, replaced by the hook profile's for an item on
        // a housing hook (setting the appraise info reads the hooked item's values when hooked).
        let hook = p.hook_profile;
        let valid_locations = hook.map_or_else(
            || w.and_then(|w| w.pwd.valid_locations).unwrap_or(0),
            |h| h.valid_locations,
        );
        let ammo_type = hook.map_or_else(
            || w.and_then(|w| w.pwd.ammo_type).unwrap_or(0),
            |h| u16::try_from(h.ammo_type).unwrap_or(0),
        );
        let inscribable = hook.map_or_else(
            || w.is_some_and(|w| w.pwd.bitfield & am::OBJECT_DESC_INSCRIBABLE != 0),
            |h| h.bitfield & dereth_client_model::appraisal::hook_appraisal::INSCRIBABLE != 0,
        );
        // The callers of `dereth_client_model::appraisal`'s three highlight functions.
        // `int_highlight`, `float_highlight` and `highlight_state` are transcribed
        // whole (the enchantment-modifier bit pairs, low bit "enchanted" and the same bit
        // sixteen places up "raised"); without this call every line retail draws in
        // `mod_high_font`/`mod_low_font` would draw plain. The keys are
        // the two mod blocks' own, so the list lives with them.
        let mut enchantment_mods = std::collections::BTreeMap::new();
        for (key, is_float) in dereth_presentation::appraisal::HIGHLIGHTED_PROPERTIES {
            let hl = if *is_float {
                dereth_client_model::appraisal::float_highlight(*key)
            } else {
                dereth_client_model::appraisal::int_highlight(*key)
            };
            let Some(hl) = hl else { continue };
            match dereth_client_model::appraisal::highlight_state(p, hl) {
                dereth_client_model::appraisal::HighlightState::Plain => {}
                dereth_client_model::appraisal::HighlightState::Beneficial => {
                    enchantment_mods.insert(*key, true);
                }
                dereth_client_model::appraisal::HighlightState::Harmful => {
                    enchantment_mods.insert(*key, false);
                }
            }
        }
        let mut attributes = [None; 6];
        let mut attribute_enchanted = [None; 6];
        let mut vitals = [None; 6];
        let mut vital_enchanted = [None; 6];
        for k in 1..=6u32 {
            attributes[(k - 1) as usize] = am::creature_attribute(p, k);
            attribute_enchanted[(k - 1) as usize] = am::creature_attribute_enchanted(p, k);
            vitals[(k - 1) as usize] = am::creature_vital(p, k);
            vital_enchanted[(k - 1) as usize] = am::creature_vital_enchanted(p, k);
        }
        // Creature display name -- `EnumMapper 0x2200000E` on
        // int quality `2`, and only for a creature: the item pane has no such line.
        let creature_display_name = p.creature_profile.and_then(|_| {
            let t = u32::try_from(am::inq::int(p, am::property::CREATURE_TYPE)?).ok()?;
            self.hud
                .creature_type_names
                .as_ref()?
                .id_to_string
                .iter()
                .find(|(k, _)| *k == t)
                .map(|(_, v)| v.clone())
        });
        // The examination view's eighteen quality queries,
        // in its own order. The slayer's display name is the **same**
        // mapper `creature_display_name`
        // above uses, on int quality `0xA6` instead of int quality `2`.
        use am::property::special as sp;
        let creature_name = |t: i32| {
            let t = u32::try_from(t).ok()?;
            self.hud
                .creature_type_names
                .as_ref()?
                .id_to_string
                .iter()
                .find(|(k, _)| *k == t)
                .map(|(_, v)| v.clone())
        };
        // Retail starts the mask from int property `0xB3` (0 when absent) and ORs in four more,
        // each only when present. `None` when not one of the five was present, which is the state
        // retail cannot distinguish from zero but this build can.
        let imbued = sp::IMBUED_EFFECT
            .iter()
            .filter_map(|k| am::inq::int(p, *k))
            .fold(None::<u32>, |acc, v| Some(acc.unwrap_or(0) | v as u32));
        let special = dereth_client_contract::SpecialPropertiesView {
            unique_limit: am::inq::int(p, sp::UNIQUE_LIMIT),
            cooldown_duration: am::inq::float(p, 0xA7),
            cooldown_group: am::inq::int(p, 0x118).map(|v| v as u32),
            cooldown_remaining: am::inq::int(p, 0x118).and_then(|group| {
                // Special-property display shares the same player enchantment registry as item wedges.
                self.player_desc()?
                    .enchantments
                    .cooldown_remaining((group as u32).wrapping_add(0x8000), self.hud.now)
            }),
            cleave: am::inq::int(p, sp::CLEAVE),
            slayer: am::inq::int(p, sp::SLAYER_CREATURE_TYPE)
                .and_then(|t| creature_name(t).map(|n| (t, n))),
            weapon_skill: am::inq::int(p, sp::WEAPON_SKILL),
            imbued,
            absorb_magic_damage: am::inq::float(p, sp::ABSORB_MAGIC_DAMAGE).is_some(),
            item_spellcraft: am::inq::int(p, sp::ITEM_SPELLCRAFT),
            attuned: am::inq::int(p, sp::ATTUNED),
            bonded: am::inq::int(p, sp::BONDED),
            retained: am::inq::boolean(p, sp::RETAINED),
            critical_multiplier: am::inq::float(p, sp::CRITICAL_MULTIPLIER).is_some(),
            critical_frequency: am::inq::float(p, sp::CRITICAL_FREQUENCY).is_some(),
            ignore_armor: am::inq::float(p, sp::IGNORE_ARMOR).is_some(),
            // One `&&`: the float is the gate and the int selects the text.
            resistance_cleaving: am::inq::float(p, sp::IGNORE_SHIELD)
                .and_then(|_| am::inq::int(p, sp::RESISTANCE_MODIFIER_TYPE))
                .map(|v| v as u32),
            proc_spell: am::inq::data_id(p, sp::PROC_SPELL).is_some(),
            ivoryable: am::inq::boolean(p, sp::IVORYABLE),
            dyeable: am::inq::boolean(p, sp::DYEABLE),
            tethered_left: am::inq::boolean(p, sp::TETHERED_LEFT),
        };
        // Magic-property display includes the item's spells.
        //
        // The spell list is decoded and cached whole with the rest of the profile; this is where
        // it is read.
        //
        // Spell-name lookup and the spell-description lookup are
        // both the spell-table base lookup on `(6, 2, 0x10000005)`, which is the
        // portal dat's `SpellTable 0x0E00000E` -- `Hud::spell_table`, loaded once at startup and
        // otherwise read only by the player's own spellbook join. The lookup masks off the high
        // enchantment bit first; an id the table has no row for keeps
        // its entry with empty strings, because the client marks the entry present before the
        // name is tested.
        use am::property::magic as mg;
        let magic = dereth_client_contract::MagicInfoView {
            spells: p.spell_book.as_ref().map(|ids| {
                ids.iter()
                    .map(|raw| {
                        let base = self
                            .hud
                            .spell_table
                            .as_ref()
                            .and_then(|t| t.spells.get(&(raw & 0x7FFF_FFFF)));
                        dereth_client_contract::AppraisalSpellView {
                            resolved: base.is_some(),
                            raw_id: *raw,
                            enchantment: raw & 0x8000_0000 != 0,
                            name: base.map(|b| b.name.clone()).unwrap_or_default(),
                            description: base.map(|b| b.description.clone()).unwrap_or_default(),
                        }
                    })
                    .collect()
            }),
            spellcraft: am::inq::int(p, mg::ITEM_SPELLCRAFT),
            cur_mana: am::inq::int(p, mg::ITEM_CUR_MANA),
            max_mana: am::inq::int(p, mg::ITEM_MAX_MANA),
            mana_rate: am::inq::float(p, am::property::float::MANA_RATE),
            mana_cost: am::inq::int(p, mg::ITEM_MANA_COST),
        };
        // **The eighteen remaining appraisal display blocks.**
        //
        // Everything below is a property the profile already carried and this function already
        // could have read; the blocks that read them are in
        // `dereth_client_contract::panels::examination`. Four resolutions
        // happen *here* rather than there, for the reason the file header gives — the panel crate
        // cannot read dats:
        //
        // * skill names come from `SkillTable 0x0E000004`, `Hud::skill_table`;
        // * heritage names come from `EnumMapper 0x10000002`
        //   with three hard-coded overrides before it (`2 Gharu'ndim`, `5 Umbraen`, `13 Olthoi`),
        //   and `Hud::heritage_names` is that mapper;
        // * attribute and secondary-attribute names are
        //   switches, and the panel already publishes them as `attribute_name` / `vital_name`;
        // * wield requirement names use a switch over those four, so the whole resolution lives
        //   here and the panel is handed its answer.
        let skill_name = |k: u32| -> Option<String> {
            if k == 0 {
                return None;
            }
            self.hud
                .skill_table
                .as_ref()?
                .skills
                .get(&k)
                .map(|s| s.name.clone())
        };
        // the three `if` arms precede the mapper and override it.
        let heritage_name = |v: i32| -> Option<String> {
            match v {
                2 => return Some("Gharu'ndim".to_string()),
                5 => return Some("Umbraen".to_string()),
                13 => return Some("Olthoi".to_string()),
                _ => {}
            }
            let v = u32::try_from(v).ok()?;
            self.hud
                .heritage_names
                .as_ref()?
                .id_to_string
                .iter()
                .find(|(k, _)| *k == v)
                .map(|(_, s)| s.clone())
        };
        // The wield-requirement subject is resolved here whole. The first argument is the
        // requirement, the second the skill/attribute id, the third the difficulty — and cases 11
        // and 12 read the third, not the second, which is why all three are taken.
        let requirement_subject = |req: i32, skill: i32, difficulty: i32| -> Option<String> {
            // The "base " prefix is chosen before the switch.
            let prefix = if matches!(req, 2 | 4 | 6) {
                "base "
            } else {
                ""
            };
            // **The name failing is not the subject failing.** The client sets the output to
            // the prefix and then appends the skill-name lookup's result — a skill the table has
            // no row for appends an empty string, so the subject is the bare prefix and
            // the line still draws. Same for the two attribute arms.
            let body = match req {
                1 | 2 | 8 => u32::try_from(skill)
                    .ok()
                    .and_then(&skill_name)
                    .unwrap_or_default(),
                3 | 4 => u32::try_from(skill)
                    .ok()
                    .and_then(dereth_presentation::appraisal::attribute_name)
                    .unwrap_or_default()
                    .to_string(),
                5 | 6 => u32::try_from(skill)
                    .ok()
                    .and_then(dereth_presentation::appraisal::vital_name)
                    .unwrap_or_default()
                    .to_string(),
                // Level is a plain `set`, so the `"base "` prefix is discarded.
                7 => return Some("level".to_string()),
                9 | 10 => {
                    return Some(
                        match skill {
                            0x11F => "Standing with the Celestial Hand",
                            0x120 => "Standing with the Eldrytch Web",
                            0x121 => "Standing with the Radiant Blood",
                            _ => "unknown quality",
                        }
                        .to_string(),
                    )
                }
                // Creature-type name lookup on the **difficulty**.
                11 => creature_name(difficulty)?,
                12 => return heritage_name(difficulty),
                _ => return None,
            };
            Some(format!("{prefix}{body}"))
        };
        let mut wield_requirements = Vec::new();
        for (req_k, skill_k, diff_k) in [
            (0x9Eu32, 0x9Fu32, 0xA0u32),
            (0x10E, 0x10F, 0x110),
            (0x111, 0x112, 0x113),
        ] {
            // all three int-quality queries must succeed or the triple is skipped entirely.
            let (Some(req), Some(skill), Some(diff)) = (
                am::inq::int(p, req_k),
                am::inq::int(p, skill_k),
                am::inq::int(p, diff_k),
            ) else {
                continue;
            };
            wield_requirements.push(dereth_client_contract::WieldRequirementView {
                requirement: req,
                difficulty: diff,
                subject: requirement_subject(req, skill, diff),
            });
        }
        // The appraisal ratings section, in `GEAR_RATING_ROWS`' drawn order with
        // `GearMaxHealth` in the slot after the thirteen terms.
        let gear_rows = dereth_presentation::appraisal::GEAR_RATING_ROWS;
        let mut gear_ratings = [None; 14];
        for (slot, (key, _)) in gear_rows.iter().enumerate() {
            gear_ratings[slot] = am::inq::int(p, *key);
        }
        gear_ratings[gear_rows.len()] = am::inq::int(
            p,
            dereth_client_contract::panels::examination::GEAR_MAX_HEALTH,
        );
        // The appraisal item-level section: the first int64 quality `5` query is the gate.
        let item_level = am::inq::int64(p, 5).map(|base| dereth_client_contract::ItemLevelView {
            total_xp: am::inq::int64(p, 4).unwrap_or(0) as u64,
            base_xp: base as u64,
            max_level: am::inq::int(p, 0x13F).unwrap_or(0),
            xp_style: am::inq::int(p, 0x140).unwrap_or(0),
        });
        // The appraisal activation-requirements section's three paired terms: the *level*
        // key gates and supplies the `%d`, the *limit* key names the skill/attribute, and a name
        // the table cannot answer for drops the term.
        let paired = |level_k: u32, limit_k: u32, name: &dyn Fn(i32) -> Option<String>| {
            let v = am::inq::int(p, level_k)?;
            if v <= 0 {
                return None;
            }
            let id = am::inq::int(p, limit_k)?;
            name(id).map(|n| (n, v))
        };
        use dereth_client_contract::panels::examination as ex;
        let activation_skill = paired(0x73, 0xB0, &|id| {
            u32::try_from(id).ok().and_then(&skill_name)
        });
        let activation_attribute = paired(0x102, 0x101, &|id| {
            u32::try_from(id)
                .ok()
                .and_then(dereth_presentation::appraisal::attribute_name)
                .map(ToString::to_string)
        });
        let activation_attribute_2nd = paired(0x104, 0x103, &|id| {
            u32::try_from(id)
                .ok()
                .and_then(dereth_presentation::appraisal::vital_name)
                .map(ToString::to_string)
        });
        // **The three character-pane values that need a dat table.**
        //
        // The gender name ([`GENDER_ENUM_MAPPER`]), then — if int quality `0xBC`
        // HeritageGroup is non-zero — a space and the heritage name, and **only** if it is zero
        // does it fall through to the creature display name for int quality `2`. Its `BOOL`
        // return is set to 0 when either lookup misses, and retail **ignores it** and writes the
        // element regardless, so a name that will not resolve leaves the other one standing alone
        // rather than blanking the line.
        let gender_name = |v: i32| -> Option<String> {
            let v = u32::try_from(v).ok()?;
            self.hud
                .gender_names
                .as_ref()?
                .id_to_string
                .iter()
                .find(|(k, _)| *k == v)
                .map(|(_, s)| s.clone())
        };
        let char_gender = am::inq::int(p, 0x71).unwrap_or(0);
        let char_heritage = am::inq::int(p, 0xBC).unwrap_or(0);
        let gender_heritage_display = p.creature_profile.is_some().then(|| {
            let mut out = String::new();
            if char_gender != 0 {
                if let Some(g) = gender_name(char_gender) {
                    out.push_str(&g);
                }
            }
            let tail = if char_heritage != 0 {
                heritage_name(char_heritage)
            } else {
                am::inq::int(p, am::property::CREATURE_TYPE)
                    .filter(|v| *v != 0)
                    .and_then(&creature_name)
            };
            if let Some(t) = tail {
                // The space is added only when the gender resolved.
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(&t);
            }
            out
        });
        // Profession uses the title table for int quality `0x105`, and string quality
        // `5 Template` when that integer is absent **or** the title table had no row for it
        // (the fallback flag is set inside the success arm only).
        let profession = am::inq::int(p, 0x105)
            .and_then(|t| u32::try_from(t).ok())
            .and_then(|t| self.title_name(t))
            .or_else(|| am::inq::string(p, 5));
        // The allegiance title needs only the rank, heritage and gender; the lookup is
        // `dereth_client_model::allegiance::get_title`.
        let allegiance_title = u16::try_from(am::inq::int(p, 0x1E).unwrap_or(0))
            .ok()
            .and_then(|rank| {
                dereth_client_model::allegiance::get_title(
                    rank,
                    u8::try_from(char_heritage).unwrap_or(0),
                    u8::try_from(char_gender).unwrap_or(0),
                )
            })
            .map(ToString::to_string);
        use dereth_rules::weenie::bitfield as bf;
        let bits = w.map_or(0, |w| w.pwd.bitfield);
        let hook_flags = p.hook_profile.map_or(0, |h| h.bitfield);
        use dereth_client_model::appraisal::hook_appraisal as hk;

        Some(dereth_client_contract::AppraisalView {
            delivery: self.world.appraisal.delivery(id),
            creature: p.creature_profile.is_some(),
            template: am::inq::string(p, 5).is_some(),
            character_title: am::inq::int(p, 0x105).is_some(),
            gear_plating_name: am::inq::string(p, 0x34),
            value: am::inq::int(p, 0x13),
            burden: am::inq::int(p, 5),

            success: p.success_flag != 0,

            level: am::inq::int(p, am::property::LEVEL),
            creature_display_name,
            attributes,
            vitals,
            attribute_enchanted,
            vital_enchanted,

            valid_locations,
            ammo_type,
            weapon: p
                .weapon_profile
                .map(|w| dereth_client_contract::WeaponView {
                    damage_type: w.damage_type,
                    weapon_time: w.weapon_time,
                    weapon_skill: w.weapon_skill,
                    weapon_damage: w.weapon_damage,
                    damage_variance: w.damage_variance,
                    damage_mod: w.damage_mod,
                    max_velocity: w.max_velocity,
                    weapon_offense: w.weapon_offense,
                    max_velocity_estimated: w.max_velocity_estimated,
                }),
            weapon_type: am::inq::int(p, am::property::WEAPON_TYPE),
            attack_type: am::inq::int(p, 0x2f),
            elemental_damage_bonus: am::inq::int(p, 0xcc),
            activation_heritage: am::inq::string(p, 0x13),
            armor_level: am::inq::int(p, am::property::ARMOR_LEVEL),
            enchantment_mods,
            armor_mods: p.armor_profile.map(|a| {
                [
                    a.mod_vs_slash,
                    a.mod_vs_pierce,
                    a.mod_vs_bludgeon,
                    a.mod_vs_cold,
                    a.mod_vs_fire,
                    a.mod_vs_acid,
                    a.mod_vs_nether,
                    a.mod_vs_electric,
                ]
            }),
            use_text: am::inq::string(p, am::property::string::USE),
            // All three quality queries must succeed, but only the third value is
            // read after the gates.
            remaining_lifespan: am::inq::int(p, ex::LIFESPAN)
                .and_then(|_| am::inq::int(p, ex::CREATION_TIMESTAMP))
                .and_then(|_| am::inq::int(p, ex::REMAINING_LIFESPAN)),
            long_desc: am::inq::string(p, am::property::string::LONG_DESC),
            short_desc: am::inq::string(p, ex::SHORT_DESC),
            augmentation_cost: am::inq::int64(p, 3),
            long_desc_decoration: am::inq::int(p, 0xAC).map(|v| v as u32),
            description_material: am::inq::int(p, 0x83).filter(|v| *v > 0).map(|v| {
                material_name_of(self.hud.material_names.as_ref(), v as u32).unwrap_or_default()
            }),
            description_gems: am::inq::int(p, 0xB1).zip(am::inq::int(p, 0xB2)).map(
                |(count, gem)| {
                    let name = material_name_of(self.hud.material_names.as_ref(), gem as u32)
                        .unwrap_or_default();
                    let name = if count == 1 {
                        name
                    } else {
                        dereth_presentation::appraisal::pluralized_gem_name(gem as u32, &name)
                    };
                    (count, name)
                },
            ),
            // The appraisal description section asks the same profile for `PortalBitmask`,
            // which the int table carries.
            portal_bitmask: am::inq::int(
                p,
                dereth_client_contract::panels::examination::PORTAL_BITMASK,
            ),

            // The appraisal lock-info section. The three keys are decoded by and cached whole
            // from the appraisal profile; this function is where they are read.
            //
            // The predicate is `is_hook`, not `is_creature`, and it is evaluated on the object
            // rather than the appraisal profile. Setting the appraise info makes the same call
            // and follows it with the hooked item's valid-locations query.
            weenie_is_hook: w.is_some_and(dereth_client_model::weenie::Weenie::is_hook),
            weenie_is_healer: bits & bf::HEALER != 0,
            weenie_is_lockpick: bits & bf::LOCKPICK != 0,
            items_capacity: w
                .and_then(|w| w.pwd.items_capacity)
                .map_or(0, dereth_rules::capacity::capacity),
            containers_capacity: w
                .and_then(|w| w.pwd.containers_capacity)
                .map_or(0, dereth_rules::capacity::capacity),
            hooked_item: p.hook_profile.is_some(),
            hooked_item_healer: hook_flags & hk::HEALER != 0,
            hooked_item_lockpick: hook_flags & hk::LOCKPICK != 0,

            num_times_tinkered: am::inq::int(p, 0xAB),
            tinker_name: am::inq::string(p, 0x27),
            imbuer_name: am::inq::string(p, 0x28),
            workmanship: am::inq::int(p, 0x69),
            num_items_in_material: am::inq::int(p, 0xAA),

            equipment_set_id: am::inq::int(p, 0x109),
            gear_ratings,
            defense_mods: [
                am::inq::float(p, 0x1D),
                am::inq::float(p, 0x95),
                am::inq::float(p, 0x96),
            ],
            mana_conversion_mod: am::inq::float(p, 0x90),
            elemental_damage_mod: am::inq::float(p, 0x98),
            caster_damage_type: am::inq::int(p, 0x2D),

            level_limits: (am::inq::int(p, 0x56), am::inq::int(p, 0x57)),
            portal_destination: am::inq::string(p, 0x26),

            has_allowed_wielder: am::inq::boolean(p, 0x55) == Some(true),
            craftsman_name: am::inq::string(p, 0x19),
            account_requirements: am::inq::int(p, 0x1A),
            heritage_specific_armor: am::inq::int(p, 0x144).and_then(&heritage_name),
            wield_requirements,

            use_requires_level: am::inq::int(p, 0x171),
            use_requires_skill: am::inq::int(p, 0x16E).filter(|v| *v != 0).and_then(|s| {
                // the level must be present **and** non-zero or the line is skipped.
                let lvl = am::inq::int(p, 0x16F).filter(|v| *v != 0)?;
                Some((u32::try_from(s).ok().and_then(&skill_name), lvl))
            }),
            use_requires_skill_spec: am::inq::int(p, 0x170)
                .filter(|v| *v != 0)
                .map(|s| u32::try_from(s).ok().and_then(&skill_name)),

            item_level,
            cloak_weave_proc: am::inq::int(p, 0x160),

            item_difficulty: am::inq::int(p, 0x6D),
            allegiance_rank_limit: am::inq::int(p, 0x6E),
            heritage_group: am::inq::int(p, 0xBC)
                .filter(|v| *v != 0)
                .and_then(&heritage_name),
            activation_skill,
            activation_attribute,
            activation_attribute_2nd,
            has_allowed_activator: am::inq::boolean(p, 0x5E) == Some(true),

            boost_value: am::inq::int(p, 0x5A),
            booster_enum: am::inq::int(p, 0x59),
            healkit_mod: am::inq::float(p, 0x64),

            pages: am::inq::int(p, 0xAF)
                .and_then(|max| am::inq::int(p, 0xAE).map(|cur| (cur, max))),

            stored_mana: am::inq::int(p, 0x6B),
            item_efficiency: am::inq::float(p, 0x57),
            destroy_chance: am::inq::float(p, 0x89),

            num_keys: am::inq::int(p, 0xC1),
            unlimited_use: am::inq::boolean(p, 0x3F) == Some(true),
            structure: am::inq::int(p, 0x5C),

            // Retail requires the property to be present **and** false.
            cannot_be_sold: am::inq::boolean(p, 0x45) == Some(false),

            rare_uses_timer: am::inq::boolean(p, 0x6C) == Some(true),
            rare_id: am::inq::int(p, 0x11),
            locked: am::inq::boolean(p, am::property::boolean::LOCKED),
            resist_lockpick: am::inq::int(p, am::property::RESIST_LOCKPICK),
            special,
            magic,
            lockpick_success_percent: am::inq::int(
                p,
                am::property::APPRAISAL_LOCKPICK_SUCCESS_PERCENT,
            ),

            inscribable,
            // The PSR test checks access levels 0x2C and 0x2D, and additionally accepts 0x61.
            // These are the retained local-player qualities, not facts from the object being
            // appraised.
            viewer_is_psr: self
                .player_desc()
                .is_some_and(|q| [0x2C, 0x2D, 0x61].into_iter().any(|key| q.inq_bool(key))),
            scribe_name: am::inq::string(p, am::property::string::SCRIBE_NAME),
            inscription: am::inq::string(p, am::property::string::INSCRIPTION),
            // The one question the inscription editable-state setter asks that the profile
            // cannot answer.
            owned_by_player: self.world.is_owned_by_player(id),

            // Every key below is decoded by and cached whole from the appraisal profile, and
            // the `0x4000` base-armour block has a decoder and **no other reader in the
            // workspace**: this function is where they are all read.
            gender_heritage_display,
            profession,
            weenie_is_pk: bits & bf::PLAYER_KILLER != 0,
            weenie_is_pk_lite: bits & bf::PK_LITE != 0,
            allegiance_title,
            faction_bits: am::inq::int(p, 0x119),
            viewer_faction_bits: self.player_desc().map_or(0, |q| q.inq_int(0x119)),
            society_ranks: [0x11Fu32, 0x120, 0x121].map(|k| am::inq::int(p, k).unwrap_or(0)),
            allegiance_rank: am::inq::int(p, 0x1E),
            allegiance_name: am::inq::string(p, 0x2F),
            monarch_title: am::inq::string(p, 0x15),
            patron_title: am::inq::string(p, 0x23),
            // The same key number as `patron_title`, in the **int** table.
            allegiance_followers: am::inq::int(p, 0x23),
            base_armor: p
                .base_armor
                .map(|a| a.map(|v| i32::try_from(v).unwrap_or(i32::MAX))),
            ratings: [
                0x133u32, 0x134, 0x139, 0x13A, 0x13B, 0x13C, 0x143, 0x15E, 0x15F, 0x17D, 0x17E,
                0x182, 0x183,
            ]
            .map(|k| am::inq::int(p, k)),
            fellowship: am::inq::string(p, 0x0A),
            date_of_birth: am::inq::string(p, 0x2B),
            age: am::inq::int(p, 0x7D),
            chess_rank: am::inq::int(p, 0xB5),
            fishing_skill: am::inq::int(p, 0xC0),
            // Likewise the same key number as `date_of_birth`, in the **int** table.
            num_deaths: am::inq::int(p, 0x2B),
            num_character_titles: am::inq::int(p, 0x106),
            enlightenment: am::inq::int(p, 0x186),
            world_rules: self.hud.era.world_rules.clone(),
        })
    }

    fn inscription_mouse_facts(&self, id: dereth_primitives::ObjectId) -> Option<(bool, u32)> {
        self.world
            .weenie(id)
            .map(|w| (w.pwd.bitfield & 0x2 != 0, w.pwd.location.unwrap_or(0)))
    }

    /// The examine request carried across the seam: the object being examined and its serial.
    ///
    /// `AppraisalCache::examining` is its only writer. The panel is on the other side of this
    /// trait and cannot be called from `interaction.rs`, so the notice is pulled instead of
    /// pushed; the serial is what makes a second examine of the same object a new notice rather
    /// than the old one being re-offered.
    fn examine_request(&self) -> Option<(dereth_primitives::ObjectId, u64)> {
        Some((
            self.world.appraisal.examining?,
            self.world.appraisal.examine_serial,
        ))
    }

    /// Helpful and harmful enchantment counts.
    ///
    /// This is the override `hud::indicators` reads through `GameView::enchantment_counts`;
    /// without it the buff/debuff indicator would read the trait's `(0, 0)` for every character
    /// in every session. The two fields it names are maintained by the client's spell-totals
    /// count, which must be called for them to move.
    fn enchantment_counts(&self) -> (u32, u32) {
        self.player_desc().map_or((0, 0), |q| {
            (q.enchantments.helpful_count, q.enchantments.harmful_count)
        })
    }

    /// Active enchantments joined to the `SpellTable`, which is
    /// the effect-list rebuild's whole input.
    ///
    /// Three things are done here rather than in the panel, each because the client does them
    /// here too:
    ///
    /// * **The join.** The list rebuild queries the spell table per entry and drops
    ///   an enchantment the `SpellTable` has no row for; so does this. That is also what makes
    ///   `_bitfield & 4` — the spell-effect UI-type match's only test — available to a
    ///   crate that may not depend on `dereth-assets`' tables.
    /// * **The clock.** `(e._duration + e._start_time) - current_time`, with
    ///   [`Hud::now`] as the current time. The two times were **rebased on receipt** by
    ///   `dereth_client_model::enchant`; recomputing them from a server timestamp
    ///   here would be wrong.
    /// * **The registry.** The canonical local-player object row owns it. The five
    ///   `Magic_*Enchantment*` messages update that same registry, without which no buff
    ///   acquired after login could reach either the lamp or this panel.
    ///
    /// `enchantments_in_effect` is `dereth_client_model::EnchantmentRegistry`'s; this is its
    /// production caller.
    fn active_effects(&self) -> Vec<dereth_client_contract::EffectEntry> {
        let (Some(q), Some(table)) = (self.player_desc(), self.hud.spell_table.as_ref()) else {
            return Vec::new();
        };
        q.enchantments
            .enchantments_in_effect()
            .into_iter()
            .filter_map(|e| {
                let spell = u32::from(e.spell_id());
                let b = table.spells.get(&spell)?;
                Some(dereth_client_contract::EffectEntry {
                    spell,
                    name: b.name.clone(),
                    description: b.description.clone(),
                    icon: (b.icon != 0).then_some(DataId(b.icon)),
                    // ` & 4` — `SpellBitfield::Beneficial`. The same bit the client's
                    // spell-totals count uses for the helpful lamp, which is why
                    // the lamp and the panel can never disagree about a spell.
                    beneficial: b.bitfield & 4 != 0,
                    remaining: e.remaining(self.hud.now),
                    permanent: e.is_permanent(),
                    category: e.spell_category,
                    power_level: e.power_level,
                })
            })
            .collect()
    }

    /// Join the component tracker's seven category lists to the player's desired counts.
    /// See [`component_categories`].
    ///
    /// Kept in this file rather than in a sibling module the way `vendor_view` and
    /// `allegiance_view` are, so that no new module declaration is needed.
    fn spell_components(&self) -> Vec<dereth_client_contract::ComponentCategory> {
        component_categories(self.world)
    }

    /// Look up a carried spell-component object in the tracker's object-id map.
    ///
    /// `.components` already keeps that map, filled by the component tracker's writers; this is
    /// the only reader of it outside `dereth-client-model`.
    fn object_is_owned_component(&self, obj: dereth_primitives::ObjectId) -> Option<u32> {
        self.world.magic.components.object_is_owned_component(obj)
    }

    /// `(0x18)` — available skill credits.
    fn skill_credits(&self) -> i64 {
        self.player_desc()
            .map_or(0, |q| i64::from(q.inq_int(AVAILABLE_SKILL_CREDITS)))
    }

    /// `(2)` — unassigned experience.
    fn available_experience(&self) -> i64 {
        self.player_desc().map_or(0, |q| {
            match q.get(dereth_client_model::StatKey::new(
                dereth_client_model::StatType::Int64,
                AVAILABLE_EXPERIENCE,
            )) {
                Some(dereth_client_model::StatValue::Int64(n)) => n,
                _ => 0,
            }
        })
    }

    /// The shared player state includes local filter changes before any server reply.
    fn spell_filters(&self) -> u32 {
        self.world.player_system.spell_filters
    }

    /// `UNDEF` is mapped to `NONCOMBAT` because the toolbar has
    /// no picture for "no mode" — all four stance buttons would be hidden — and a character in the
    /// world is always in one of the four.
    fn combat_mode(&self) -> u32 {
        let m = self.world.combat.combat_mode.raw();
        if m == 0 {
            dereth_client_contract::combat_mode::NONCOMBAT
        } else {
            m
        }
    }

    /// The advanced-combat option gate read before combat UI updates.
    ///
    /// This is the option word the client reads, not `CombatState::advanced_combat_mode`: the
    /// handler calls the accessor itself, while the combat system's cached copy is a separate read.
    /// Both exist and both come from the same bit.
    fn advanced_combat_ui(&self) -> bool {
        self.world.player_system.options.advanced_combat_ui()
    }

    /// The local player's skill-advancement query for skill `0x32`.
    ///
    /// `Sac::Undef` (0) when no local player description has arrived, which is what the client's
    /// null check produces: the handler still runs the comparison and
    /// `0 < 2`, so the recklessness meter stays hidden.
    fn recklessness_advancement_class(&self) -> u32 {
        // A world whose rules hide the marker answers as an untrained character does, whatever
        // the character holds.
        if !self.hud.era.world_rules.recklessness_marker {
            return 0;
        }
        self.player_desc().map_or(0, |q| {
            dereth_rules::skills::inq_skill_advancement_class(
                q,
                dereth_client_contract::combat_notice::RECKLESSNESS_SKILL,
            ) as u32
        })
    }

    /// The producer for the Combat window's three read-back notices, with all four fields taken
    /// straight from current combat state.
    ///
    /// `level` is the latest power-bar level that was *sent*, not the power-bar level evaluated at
    /// [`Hud::now`], for two reasons:
    ///
    /// * **The notice does not carry the clock's level, it carries what was *sent*.** Every
    ///   power-bar-level notice passes `set_power_bar_level`'s
    ///   argument, and the latest power-bar level is that argument. On three arms it is
    ///   **not** the live power-bar level: the per-frame use-time update pins it to the smaller of
    ///   the requested power and the level on the frame the attack fires, sends **0.0** when the
    ///   body left the ready position mid-charge, and the commence-attack handler sets it to the
    ///   requested attack power **with the build-in-progress flag cleared** — where re-evaluating
    ///   the power-bar level returns 0.0 and the meter would drop to empty on the swing.
    /// * **The cache has a per-frame writer.** The power-level update calls
    ///   `set_power_bar_level` on every arm, so the cache moves once a frame and there is nothing
    ///   to work around.
    ///
    /// The snapshot remains useful for inspecting retail state and polling height/the notch;
    /// neither display polls its level. `deliver_power_bar_notices` delivers the ordered
    /// Begin/SetLevel/Finish journal, so a hide/restart cannot erase an intermediate zero.
    /// **Deferred until the UI frame**, because `App::ui_use_time` runs
    /// before `App::interaction_use_time`.
    ///
    /// One further deviation is load-bearing here and is stated rather than relied on silently:
    /// retail's power-bar hide does **not** write the latest power-bar level (it touches four
    /// other fields and nothing else). Setting it to 0.0 on hide would not make the display agree
    /// either: the snapshot mode is already Undef and the classic meter correctly refuses it. The
    /// journal delivers the notices and leaves that cache alone, preserving both the retail stale
    /// cached float and the actual zero notice.
    fn combat_bar(&self) -> dereth_client_contract::CombatBar {
        let c = &self.world.combat;
        dereth_client_contract::CombatBar {
            requested_attack_height: c.requested_attack_height as u32,
            power_bar_mode: c.power_bar_mode as u32,
            level: c.latest_power_bar_level,
            desired_power: c.ui_requested_power,
        }
    }
}
