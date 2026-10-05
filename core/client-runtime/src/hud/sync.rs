//! World synchronization and view snapshots.

use super::*;

impl Hud {
    /// Rebuild the derived per-frame values: the radar list, the coordinates and the heading.
    ///
    /// The radar's regeneration walks every live weenie and adds each one; this is that walk,
    /// in the same table order (`dereth_client_model::ObjMap` reproduces
    /// `LongHash`'s bucket walk on purpose).
    pub fn sync(&mut self, objects: &crate::objects::ObjectStream, viewer: Option<ViewerFrame>) {
        let world = &objects.world;
        let player = world.player;
        self.radar.clear();
        let origin = viewer.map(|v| v.position);
        for (id, presence) in objects.presences() {
            let Some(w) = world.weenie(id) else { continue };
            let pwd = &w.pwd;
            let pos = presence.position.as_ref();
            let in_world = pos.is_some();
            // Conversion to player space is exactly `localtolocal`
            // `(player.position, &out, obj.position, {0,0,0})` — which is
            // what makes the radar heading-up, because `localtolocal` applies the player frame's
            // own rotation, which is why the compass letters orbit instead of the blips.
            //
            // Without a viewer position there is no player space, so the entry is carried with
            // `in_world` clear and `draw_objects` skips it — the same skip retail's radar object draw makes for
            // an object with no cell.
            let player_space = match (origin.as_ref(), pos) {
                (Some(o), Some(p)) => {
                    let v = dereth_physics::math::localtolocal(o, p, dereth_primitives::Vec3::ZERO);
                    (v.x, v.y, v.z)
                }
                _ => (0.0, 0.0, 0.0),
            };
            self.radar.push(RadarEntry {
                id,
                player_space,
                blip_color: pwd.blip_color.unwrap_or(0),
                bitfield: pwd.bitfield,
                // The radar-behaviour field is the radar's
                // filter and this is the field it reads. Absent means `radar_enum_value::UNDEF`
                // (0), and that is not shown — the client's own default, not a substitute for one.
                radar_enum: pwd.radar_enum.unwrap_or(radar_enum_value::UNDEF),
                // `IsPlayer` is `_bitfield & 0x08` (`ObjectDescriptionFlag::Player`), which is what
                // the blip-colour lookup asks. It is a **different** question from
                // "is this our own character", which the radar asks separately (and answers by
                // leaving the player out of the blip list entirely); an admin watching another
                // player still needs that player coloured as a player.
                is_player: pwd.bitfield & bits::PLAYER != 0,
                is_self: Some(id) == player,
                // These come from the four predicates used by blip colour and shape.
                // The four one-line bit tests establish every input:
                //
                //   player         `_bitfield >> 3    & 1`  0x00000008 Player
                //   player killer  `_bitfield >> 5    & 1`  0x00000020 PlayerKiller
                //   PK lite        `_bitfield >> 0x19 & 1`  0x02000000 PkLiteStatus
                //   creature       `_type >> 4        & 1`  ITEM_TYPE 0x10
                //
                // `is_attackable` carries the creature test — `is_creature`, over `_type`, **not** a
                // `_bitfield` bit. The separate
                // `_bitfield & 0x10` half of the same branch is `Attackable`, and `get_blip_color`
                // still tests it. Without this line every creature in the world falls through to
                // `RadarDefault` and the radar plots them white instead of gold.
                is_attackable: pwd.obj_type & ITEM_TYPE_CREATURE != 0,
                is_pk: pwd.bitfield & bits::PK != 0,
                is_pk_lite: pwd.bitfield & bits::PK_LITE != 0,
                // `get_blip_shape` asks
                // the allegiance-member predicate, which compares the two
                // `_monarch` ids and **does not** consult the allegiance hierarchy — see
                // the world's allegiance-member predicate.
                is_allegiance_member: world.is_allegiance_member(id),
                // Radar updates query allegiance
                // membership and then `is_fellow` on every blip, and `get_blip_shape` asks
                // the same pair before it asks about the allegiance — so with these two nailed to
                // `false` a fellow would draw as a plain player in the radar-default/creature
                // colours and never in the fellowship colour, whatever the table held.
                //
                // The fellowship-leader test is **not** gated on membership: it is "a fellowship
                // is held and its leader is `id`", the comparison and nothing else.
                is_fellow: world.is_fellow(id),
                is_fellowship_leader: world.is_fellowship_leader(id),
                in_world: in_world && origin.is_some(),
            });
        }
        // Whether the player exists, snapshotted where the client would read it.
        // `App::frame` builds the `ViewerFrame` from `WorldScene::character`, which *is* the local
        // physics object, so `viewer.is_some()` is that gate and nothing else on this
        // struct is. Written unconditionally — outside the `if let` below — because losing the
        // body has to clear it, and `player_cell` deliberately latches.
        self.player_body = viewer.is_some();
        if let Some(v) = viewer {
            self.heading = v.heading_degrees;
            self.coords = player_coords(v.position.cell);
            // Keep the same cell `player_coords` consumes: it is the outdoor-state
            // query's only input and therefore selects the radar range. `viewer` is the character.
            self.player_cell = Some(v.position.cell);
        }
        // The inventory placements for the paper doll, in the server's own order — which is what
        // makes the paper doll's item placement's "last write wins" per slot mean what it means.
        self.equipment.clear();
        // Use the player id recorded by `0xF746`: `0x0013` arrives before the
        // player's own `0xF745`, and `remake_character_inventory` reads
        // the recorded player id for exactly that reason. Using the object table's copy
        // here leaves the doll empty for every frame between the two messages — and for the whole
        // of a corpus replay that feeds `0x0013` without the create stream.
        if let Some(p) = world.player.or(self.player) {
            if let Some(inv) = world.inventory(p) {
                self.equipment
                    .extend(inv.placements.iter().map(|p| (p.iid, p.loc)));
            }
        }
    }

    /// The display-name material prefix, for every object that has
    /// one, so a salvage bag shows its material and not the shard's bare `Salvage (100)`.
    ///
    /// The client composes the name inside its object-name query on every call and needs no
    /// cache: it has the dat mapper to hand and writes into a static buffer. This build's
    /// `GameView::name` answers a `&str` borrowed from the world and the mapper lives on this side
    /// of the seam, so the composed strings are built here, once per object per change, and the
    /// view reads them back.
    ///
    /// **Guarded on the three inputs, not on the frame.** A pack of twenty loot items would
    /// otherwise allocate forty `String`s every frame for a name that never moves;
    /// [`DisplayName::matches`] is what stops it, and [`DisplayName::wanted`] is what keeps
    /// ordinary objects out of the map entirely. `retain` first, so an object that was destroyed --
    /// or whose material was cleared -- does not leave a stale name behind for its id to be reused
    /// with. The counter is the instrument: a window full of salvage with
    /// `display_names_composed == 0` is a defect and must not read like an empty world.
    pub fn refresh_display_names(&mut self, world: &dereth_client_model::World) {
        // Taken out so the mapper (`&self.material_names`) can be read while the map is written.
        let mut cache = std::mem::take(&mut self.display_names);
        cache.retain(|id, _| world.weenie(*id).is_some_and(DisplayName::wanted));
        for (id, w) in world.tables.weenies.iter() {
            if !DisplayName::wanted(w) {
                continue;
            }
            let material = w.pwd.material_type.unwrap_or(0);
            if cache.get(&id).is_some_and(|d| d.matches(w, material)) {
                continue;
            }
            let name = material_name_of(self.material_names.as_ref(), material);
            cache.insert(id, DisplayName::compose(w, material, name.as_deref()));
            self.stats.display_names_composed += 1;
        }
        self.display_names = cache;
    }

    /// The eight answers the auto-target sweep needs from the object
    /// system, gathered once per frame.
    ///
    /// `in_range_of_player` checks `(id, player, radar radius, true, false)`,
    /// evaluated for every presence. Two deviations, both stated rather than
    /// hidden:
    ///
    /// * **the distance is centre to centre.** The range query's bounding-box flag selects
    ///   the distance query that subtracts the two
    ///   objects' physics radii; this seam carries positions and not radii, so an object leaves
    ///   the set *slightly sooner* here than in retail — by the sum of the radii, i.e. under a
    ///   metre on a 75 m radius.
    /// * **The radius follows the outdoor state.** The radius here is
    ///   [`Hud::player_outside`]`() ? 75.0 : 25.0`, the same single owner
    ///   `GameView::player_outside` answers from, so the sweep's radius and the radar's scale
    ///   cannot disagree.
    ///
    /// `self.radar` is the whole presence list with `player_space` already computed by
    /// [`Self::sync`] — `RadarEntry::in_world` is the "we have a position for it" flag — so this
    /// walks it rather than the object stream a second time.
    ///
    /// Both interface adapters and the runtime read these facts without owning the sweep.
    #[must_use]
    pub fn auto_target_world(
        &self,
        world: &dereth_client_model::World,
    ) -> dereth_client_contract::chat::mainchat::AutoTargetWorld {
        use dereth_client_contract::chat::mainchat::AutoTargetWorld;
        let player = world.player.or(self.player);
        let radius = dereth_client_contract::radar::radar_range(self.player_outside());
        let in_range_of_player: Vec<u32> = self
            .radar
            .iter()
            .filter(|e| e.in_world && !e.is_self)
            .filter(|e| {
                let (x, y, z) = e.player_space;
                (x * x + y * y + z * z).sqrt() < radius
            })
            .map(|e| e.id.0)
            .collect();
        let selected = world.selected;
        let last = world.chat.last_speakable_target;
        AutoTargetWorld {
            selected_id: selected.map_or(0, |s| s.0),
            player_id: player.map_or(0, |p| p.0),
            // Whether the selected object's description is talkable.
            selected_talkable: selected
                .and_then(|s| world.weenie(s))
                .is_some_and(dereth_client_model::weenie::Weenie::is_talkable),
            // Whether the last speakable target is owned by the player — note it is the **last
            // speakable target**, not the selection: the first arm of the sweep is about keeping
            // the target it already has.
            owned_by_player: last.is_some_and(|t| world.is_owned_by_player(t)),
            container_id: last
                .and_then(|t| world.weenie(t))
                .and_then(|w| w.pwd.container_id)
                .map_or(0, |c| c.0),
            in_range_of_player,
            // The appropriate-form name query for the selected id — name type 2 is the
            // "appropriate" form, which pluralises a stack.
            selected_name: selected
                .and_then(|s| world.weenie(s))
                .map(|w| w.object_name(dereth_client_model::weenie::NameType::Appropriate))
                .unwrap_or_default(),
            selected_squelched: selected.is_some_and(|s| world.chat.is_squelched(s, "", 1)),
        }
    }

    /// One `CharacterOptions1` bit, or the documented default when the bit is unknown.
    /// The key the player-module refresh pass is gated on.
    ///
    /// `side_by_side` is a parameter rather than a read of `character_option::SIDE_BY_SIDE_VITALS`
    /// so that a test can force a real bit and assert the latch follows it: with the option
    /// hard-coded, **no test could tell a key that reads the option from one that does not**. The
    /// constant is `Some(bit 21)`, so the parameter is not load bearing for the production path —
    /// it is kept because it is what makes the two arms of
    /// `the_applied_latch_moves_when_side_by_side_vitals_moves` distinguishable. The one
    /// production call site passes the constant.
    ///
    /// `side_by_side_vitals` is read through [`Self::option_bit`], which reads the option word the
    /// player's own tick writes rather than the login blob's copy. See that method for which copy
    /// survives.
    pub fn applied_key(
        &self,
        world: &dereth_client_model::World,
        screen_serial: u64,
        side_by_side: Option<u32>,
    ) -> AppliedKey {
        AppliedKey {
            screen_serial,
            placements: self.placements.clone(),
            side_by_side_vitals: self.option_bit(world, side_by_side),
        }
    }

    /// One `CharacterOption` (`options_`) bit off **the** option word.
    ///
    /// # Which copy this reads, and why it is that one
    ///
    /// It reads [`dereth_client_model::player::PlayerSystem::options`]`.options` — the same word
    /// [`character_option`](fn@character_option) reads, `PlayerSystem::apply_player_module` fills from `0x0013` and
    /// `PlayerSystem::set_option` read-modify-writes when the player ticks a row on the Character
    /// Options page (`interaction.rs`'s `UiRequest::SetPlayerOption` arm).
    ///
    /// [`Hud::player_module`]`.options` is a **second clone of the same `0x0013` blob** and only
    /// the `0x0013` arm ever writes it. Reading it would answer the login blob's value for the rest
    /// of the session after the player ticks a checkbox — a working-*looking* client that ignores
    /// the checkbox, with the same symptom as a stale [`AppliedKey`] latch and a different cause.
    ///
    /// `Hud::player_module` still supplies the shortcut bar and spell filters from login, while
    /// local window placements are mirrored from the retained PlayerSystem module. Its `options`
    /// word has **no** production readers and must keep none: it is a snapshot of login and it does
    /// not follow the player. Its `options2` word has exactly one reader and one writer,
    /// [`Self::lock_ui`] / [`Self::set_lock_ui`]. It is a screen-rebuild cache; the authoritative
    /// `PlayerSystem` module is written first and this cache is kept synchronized from the chosen
    /// screen value.
    ///
    /// The presence gate is `player_system.module.is_some()`, which is the same third state
    /// [`character_option`](fn@character_option) carries: before `0x0013` there is no character's word to read, and
    /// `Options::default()`'s default option word is not this character's answer.
    pub fn option_bit(&self, world: &dereth_client_model::World, bit: Option<u32>) -> bool {
        let Some(bit) = bit else { return false };
        world.player_system.module.is_some() && world.player_system.options.options & bit != 0
    }

    /// `options2_` bit 24, off the login blob.
    ///
    /// A separate accessor from [`Self::option_bit`] because the two options live in **different
    /// words**: `SideBySideVitals` is `options_`, `LockUI` is `options2_`. Reading the lock out of
    /// `options_` would answer `true` for a blob whose bit 24 of `options_` happens to be
    /// set and lock the whole HUD.
    #[must_use]
    pub fn lock_ui(&self) -> bool {
        let (Some(bit), Some(m)) = (character_option::LOCK_UI, self.player_module.as_ref()) else {
            return false;
        };
        m.options2 & bit != 0
    }

    /// Mirror `LockUI` by changing `options2_` bit 24 only when the requested value differs.
    ///
    /// This is the **presentation mirror** where the radar's padlock lands after
    /// `dereth_client_shell::ui` pushes the new value into `GamePlayScreen` and broadcasts global `0x0D`.
    /// The authoritative write is deliberately not attempted from `Hud::drive`'s immutable
    /// object view: the request owner first calls `PlayerSystem::set_option(51, ..)`, which mirrors
    /// bit 24 into the module `save_to_server` packs and emits `0x0005` because `LockUI` is one of
    /// the twenty-one auto-save option ordinals. This cache then preserves the same
    /// value across a screen rebuild.
    pub fn set_lock_ui(&mut self, v: bool) {
        let Some(bit) = character_option::LOCK_UI else {
            return;
        };
        let Some(m) = self.player_module.as_mut() else {
            return;
        };
        if (m.options2 & bit != 0) == v {
            return;
        }
        m.options2 = if v {
            m.options2 | bit
        } else {
            m.options2 & !bit
        };
        self.stats.lock_ui_writes += 1;
    }

    /// The player-outside predicate. A missing player returns false; otherwise the
    /// low 16 bits of the player's cell id are outdoors exactly when they are below `0x100`. This is
    /// `dereth_physics::landdefs::is_outdoors` — the same one comparison the renderer,
    /// the sky and the ambient sweep already branch on. It is the **only** input to
    /// the speech radius, whose whole rule is `outside ? 75.0 : 25.0`. There is no third arm: a
    /// dungeon room and an above-ground building interior are
    /// both environment cells with a cell index `>= 0x100` and both take the 25.
    ///
    /// **Without this producer** `GameView::player_outside` answers its trait default `true`, so
    /// the radar would draw at the outdoor 75 everywhere (too far out indoors) and
    /// [`Self::auto_target_world`] would filter at 75.
    ///
    /// `None` selects **false**, deliberately: that is the is-player-outside test's own null-player
    /// return. It is not reachable from the radar, whose whole regeneration is gated on there
    /// being a player, and `GamePlayScreen::update_radar`
    /// carries that gate.
    #[must_use]
    pub fn player_outside(&self) -> bool {
        self.player_cell
            .is_some_and(dereth_physics::landdefs::is_outdoors)
    }

    /// Convert one object's `x` and `y` into the player's own frame.
    ///
    /// `None` is the object having no physics body, which is the case
    /// the hearing predicate skips its distance half for.
    /// See [`dereth_client_model::chat::ChatState::can_hear`].
    ///
    /// The offsets come out of [`Self::radar`]'s own `player_space`, which [`Self::sync`] fills
    /// with the same `dereth_physics::math::localtolocal` call the radar's blips are drawn from —
    /// **one conversion per frame, shared**, so the range a line is heard at and the range its
    /// blip is drawn at cannot drift apart. `in_world` is that entry's "we have a position and a
    /// viewer" flag, and without both there is no player space to compute, which is the same
    /// `None`.
    ///
    /// **It is one frame old**, because `sync` runs in step 7 and the message arrives in step 3.
    /// Retail reads `position` live, but those positions were themselves last
    /// written by the previous frame's physics, so the two are the same vintage. A speaker whose
    /// very first `0x02BB` arrives on the same frame its object was created is the one case that
    /// differs, and it takes the `None` arm — audible — which is the safe direction.
    #[must_use]
    pub fn speaker_player_space(&self, id: ObjectId) -> Option<(f32, f32)> {
        let e = self.radar.iter().find(|e| e.id == id && e.in_world)?;
        Some((e.player_space.0, e.player_space.1))
    }

    /// The distance term for ranged speech: player to sender, without cylinder radii
    /// and without ignoring Z.
    ///
    /// With both range modifiers clear, this is the **three-dimensional** center-to-center
    /// distance without cylinder radii, so
    /// unlike the hearing test a speaker on the floor above is not at range zero.
    ///
    /// It is the norm of the same player-space vector [`Self::speaker_player_space`] reads, which
    /// is what makes this one line rather than a second position lookup: `localtolocal` is a rigid
    /// motion, so the player-space vector's norm equals the direct distance exactly. Sharing the
    /// snapshot also means the say gate, the ranged gate and the radar cannot disagree about where
    /// a speaker is.
    ///
    /// `None` means either physics body is absent, which the ranged-speech predicate answers **out
    /// of range** — the opposite direction from the hearing test's null
    /// escape, and the caller must not confuse the two.
    #[must_use]
    pub fn speaker_distance(&self, id: ObjectId) -> Option<f32> {
        let e = self.radar.iter().find(|e| e.id == id && e.in_world)?;
        let (x, y, z) = e.player_space;
        Some((x * x + y * y + z * z).sqrt())
    }

    /// The read-only seam the HUD panels see.
    #[must_use]
    pub fn view<'a>(&'a self, objects: &'a crate::objects::ObjectStream) -> HudView<'a> {
        HudView {
            hud: self,
            world: &objects.world,
        }
    }

    /// The same seam, **owned**.
    ///
    /// [`HudView`] is two references, so every `&str` and `&[T]` it answers with points into
    /// this `Hud` and this `World`. That is free in process and impossible out of it. This takes
    /// the same 115 answers and keeps them: `String` for `&str`, `Vec<T>` for `&[T]`, a map per
    /// keyed read. The copy is made by *calling the view*, so there is no second transcription of
    /// these accessors to drift from the first one.
    ///
    /// **Nothing reads one yet.** It is built here, beside the borrowing view, so that a
    /// presentation moved off `HudView` has something to move onto; the
    /// frame still hands panels `&dyn GameView` from [`Hud::view`].
    #[must_use]
    pub fn snapshot(
        &self,
        objects: &crate::objects::ObjectStream,
    ) -> dereth_client_contract::GameSnapshot {
        dereth_client_contract::GameSnapshot::from_view(&self.view(objects))
    }
}
