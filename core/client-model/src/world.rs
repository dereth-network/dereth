//! Client-side world state, object maintenance, and the local player's identity.

use crate::inventory::{requests::RequestLock, SPLIT_WINDOW};
use crate::objects::{
    DoomEntry, LostCell, NullPlaceholder, ObjectInventory, ObjectTables, PhysicsPresence,
    QueuedBlob, DESTRUCTION_DELAY, DESTRUCTION_RECHECK_EPSILON, FORCE_OBJDESC_INTERVAL,
    VISIBLE_REBUILD_INTERVAL,
};
use crate::qualities::{Qualities, QualityNotifications, QualityScope, StatKey, StatValue};
use crate::weenie::{mirror_stat_update, Weenie};
use crate::{GameError, Notice, NoticeSink, Request, RequestSink};
use dereth_primitives::{CellId, LandblockId, LocalTime, ObjectId, ServerTime};
use dereth_protocol::objects::{is_newer, ObjectCreatePayload, ObjectSendForceObjdesc};
use dereth_protocol::types::{ContentProfile, PublicWeenieDesc};
use std::cmp::Reverse;

/// The whole client-side world model.
#[derive(Debug)]
pub struct World {
    pub tables: ObjectTables,
    physics_setups: std::collections::BTreeMap<u32, crate::objects::PhysicsSetupFacts>,
    /// Process-owned, already-normalized material-type names. Object notices live in
    /// the game model, while the retail DAT table is loaded by the client host; keeping the
    /// resolved strings here lets every notice use [`Weenie::display_name`] without copying its
    /// material composition.
    material_names: std::collections::BTreeMap<u32, String>,
    /// Process-owned `Attribute2ndTable` (the three vital-maximum formulas), installed by the
    /// client host once the portal tables load. The receipt-time vital clamp needs it to compute a
    /// maximum; see [`Qualities::set_received`].
    vital_formulas: Option<dereth_assets::tables::Attribute2ndTable>,
    /// Process-owned quality filter, installed beside the vital formulas: the maximum the
    /// receipt-time clamp computes is enchanted only as far as it allows.
    quality_filter: Option<dereth_assets::tables::QualityFilter>,
    /// The local player's object id.
    pub player: Option<ObjectId>,
    /// The currently selected object id.
    pub selected: Option<ObjectId>,
    /// The previously selected object id.
    pub prev_selected: Option<ObjectId>,
    /// The previous valid selection — the last *non-zero* selection.
    pub prev_selected_valid: Option<ObjectId>,
    /// The single, global, un-timed inventory request lock.
    pub request_lock: RequestLock,
    /// blocks every inventory request.
    pub attack_in_progress: bool,
    /// The open pack preferred over the player as an inventory destination. A new-parent notice
    /// updates it whenever the list's parent actually changes. This is separate from a panel's
    /// selected pack row: it is the destination used when an item is picked up.
    pub open_container: Option<ObjectId>,
    /// The optimistic row inserted into a destination list before the server answers. The client
    /// inserts and marks the row before sending either a container move or a ground drop. This
    /// rebuild stores it here because panels reconstruct their rows from world state each frame.
    ///
    /// One at a time, because the list's pending item is one slot and refuses a second drop
    /// on a list that already has one with *"Already attempting to place %s here"*.
    pub pending_row: Option<crate::inventory::PendingRow>,
    /// The corpse, chest, or other ground container currently open. Items inside it are exempt
    /// from the normal double-click restriction so they can be moved into the player's pack.
    pub ground_object: Option<ObjectId>,
    /// The ground container whose contents were most recently requested.
    ///
    /// Separate from [`Self::ground_object`] in the client and here, because the *panel* does not
    /// open until the server answers. The contents notice raises
    /// [`Notice::SetGroundObject`] only when the container whose contents arrived is **this**.
    pub requested_ground_object: Option<ObjectId>,
    /// Every corpse opened during this character session.
    ///
    /// Updated when corpse contents arrive and queried by [`Self::has_corpse_been_opened`].
    pub opened_corpses: std::collections::BTreeSet<ObjectId>,
    /// The last successful use time; uses inside the following 200 ms are refused.
    ///
    /// `None` until the first use. The retail static starts at 0.0, which
    /// which gives retail a 200 ms dead zone at process start that nothing can observe; `None`
    /// is that reading without the artefact.
    pub last_used: Option<ServerTime>,
    /// The object captured when targeting begins, cleared before the compatibility check consumes
    /// it. This is deliberately separate from the mutable selection.
    pub targeting_object: ObjectId,
    /// A **copy** of the player's `ObjDesc`.
    pub player_objdesc: Option<dereth_protocol::types::ObjDesc>,
    /// The 24 named equipment slots.
    pub inv_slots: crate::inventory::slots::InvSlotModule,
    /// The occupied `INVENTORY_LOC` bits.
    pub inventory_mask: u32,
    /// The occupied clothing-priority bits.
    pub clothing_priority_mask: u32,
    /// The pending split's class id, stack size and time: the 10 s pending-split window.
    pub pending_split: Option<crate::inventory::PendingSplit>,
    /// The unblock retry state: the blocking and blocked ids, the blocked side and the attempt
    /// number.
    pub unblock: crate::inventory::equip::UnblockState,
    /// Client combat mode and target — see [`crate::combat`].
    pub combat: crate::combat::CombatState,
    /// The toolbar's two selected-object meters. They are not part of combat-state reset, so they
    /// remain a separate [`crate::combat::SelectedMeters`] value.
    pub selected_meters: crate::combat::SelectedMeters,
    /// The local player's persisted options and social lists — see [`crate::player`].
    pub player_system: crate::player::PlayerSystem,
    /// Talk focus, squelch and the text-type routing model — see [`crate::chat`].
    pub chat: crate::chat::ChatState,
    /// The client text scroll — see [`crate::scroll`].
    ///
    /// Every [`crate::Notice::DisplayString`] this crate raises ends here, because
    /// display-string handling appends `(text, channel, true, 0)` to the singleton scroll.
    /// It lives on `World` rather than on the wiring file's `Interaction` for the same reason: the
    /// producers (`interaction.rs`) and the consumer (`hud.rs`) are two owners that cannot see each
    /// other, and this is the one object both already hold a `&mut` to.
    pub scroll: crate::scroll::Scroll,
    /// The transient assess profiles — see [`crate::appraisal`].
    pub appraisal: crate::appraisal::AppraisalCache,
    /// The component tracker and the busy count — see [`crate::magic`].
    pub magic: crate::magic::MagicState,
    /// The current allegiance hierarchy — see [`crate::allegiance`].
    pub allegiance: crate::allegiance::AllegianceHierarchy,
    /// How many `0x0003 Allegiance_AllegianceUpdateAborted` have arrived.
    ///
    /// This counter preserves the panel's notice edge in a UI that pulls state once per frame. See
    /// [`Self::allegiance_update_aborted`].
    pub allegiance_aborts: u64,
    /// The `u32` the last one carried — ACE's `WeenieError`. **A measurement, not an input**:
    /// retail's receiver ignores its parameter entirely, and this exists so that a shard sending
    /// something other than the usual code is visible rather than silently taking the same arm.
    pub allegiance_abort_last_reason: u32,
    /// The client's argument, as it stands.
    /// See [`crate::portal_storm`].
    ///
    /// `0.0` at construction matches the initially dark indicator, which lights only for
    /// `level > 0.0`.
    pub portal_storm_level: f32,
    /// How many `0x02CB Misc_PortalStorm` have landed — *"you have been moved"*, the one of the
    /// four that has already happened. Kept because it is the only one with no lasting state:
    /// `portal_storm_level` goes to `0.0`, which is where it already was.
    pub portal_storms_struck: u64,
    /// Portal-storm warning script modifiers waiting for the player's physics scene.
    pub pending_portal_storm_scripts: Vec<f32>,
    /// The player's own house, or `None`. The receive layer stores it here because the house panel
    /// is a pulled view; see [`crate::housing`].
    pub house: Option<crate::housing::HouseData>,
    /// How many `0x0225 House_HouseData` have arrived: the panel's redraw
    /// edge, analogous to [`Self::allegiance_aborts`].
    pub house_data_notices: u64,
    /// The **slumlord's** house profile and owner id, which are separate from [`Self::house`].
    ///
    /// The client keeps both values together. They are here for the same reason as `house`, and
    /// the pair travels together because every request the window makes
    /// (`0x021C`, `0x0221`, `0x0258`) names the slumlord and nothing else does.
    pub slumlord: Option<(dereth_primitives::ObjectId, crate::housing::HouseProfile)>,
    /// How many `0x021D House_HouseProfile` have arrived — the edge that **raises** the window,
    /// because receiving a profile makes that window visible.
    pub house_profile_notices: u64,
    /// The chess window's complete state.
    ///
    /// It lives on `World` because the six
    /// inbound opcodes arrive at the message layer, the five outbound ones are constructed from
    /// gestures, and both directions must share the same board. The panel is a view of it.
    pub minigame: crate::minigame::MiniGame,
    /// Whether player initialization has already run this session. The handler tests and sets this
    /// byte, so initialization runs **once per session** however many `0x0013` messages arrive.
    /// The client clears it during
    /// session reset; here it is cleared for free, because `ObjectStream::reset`
    /// replaces the whole `World` on session reset.
    pub player_initialized: bool,
    /// The current fellowship, or `None` when not in one.
    pub fellowship: Option<crate::fellowship::Fellowship>,
    /// The secure-trade mirror and the ids needed to reject a second partner and defer an
    /// item-on-player gesture until registration.
    pub trade: crate::trade::TradeSystem,
    /// The open vendor and both purchase baskets.
    ///
    /// `toolbar::splitter::for_selection`'s vendor arm and `use_object`'s two vendor guards read
    /// it. See [`crate::vendor::Shop`].
    pub shop: crate::vendor::Shop,
    /// The live contract trackers — see [`crate::quests`].
    pub contracts: crate::quests::ContractTrackerTable,
    /// The current book id, pages, and inscription — see [`crate::book`].
    pub book: crate::book::BookState,
    /// Object-range checks that close a panel when the player walks away from what opened it. See
    /// [`crate::range`].
    pub object_range_checks: crate::range::ObjectRangeCheckList,
    /// A latch recording whether the selected object has been drawn. A new pick clears it and
    /// drawing a part belonging to the selected object sets it. Range exit re-arms the selection
    /// watch when this latch is set; otherwise it clears selection. Nothing clears it per frame.
    ///
    /// Rendering reports the latch at the start of the next frame, matching the client order in
    /// which drawing precedes the later range check. This build has no per-object frustum test, so
    /// it sets the latch when resident geometry is submitted. The client waits until a part is
    /// inside the view cone. This can make the latch rise earlier here, but never causes a range
    /// exit to clear a selection the client would retain.
    pub selected_object_in_view: bool,
    /// The selection id captured by the range-watch notice and compared with each drawn part before
    /// setting [`Self::selected_object_in_view`]. It is not always the mutable selection: a normal
    /// deselection returns before this capture is updated, while the trade-partner arm reaches the
    /// common tail. A stale captured id is harmless because range exit first matches the exiting id
    /// against the current selection.
    pub viewcone_check_object_id: Option<ObjectId>,
    /// The selection id for which range-watch registration last ran. It is the edge detector for
    /// the selected-item notice; see [`Self::calculate_object_range_checks`].
    pub(crate) selection_watch: Option<ObjectId>,
    /// Whether selection is owed a broadcast. The setter broadcasts on every path past the
    /// `force` early return, and that call is **outside** the `old != id` skip, so a forced
    /// re-selection of an unchanged id broadcasts while raising no `SelectionChanged`. The one
    /// subscriber this crate models is the selection range-watch registrant, and it needs the
    /// radar radius and a clock, neither of which the setter receives. The setter records the edge
    /// here and the range-calculation pass consumes it with both values available.
    ///
    /// This pending edge is what gives `force = true` an observable effect for an unchanged id.
    pub(crate) selection_broadcast_pending: bool,
    /// When [`Self::update_visible_object_list`] last ran.
    last_visible_rebuild: ServerTime,
    /// initialized to one at start-up. Not application focus.
    pub maintenance_active: bool,
    /// The complete player qualities delivered by `0x0013 Login_PlayerDescription`. Later private
    /// quality updates write the same store.
    ///
    /// It is held here as well as installed on the player's weenie row because **the description
    /// can arrive first**: ACE's `Player.SendSelf` enqueues `GameEventPlayerDescription`
    /// before `GameMessagePlayerCreate` and `GameMessageCreateObject`, and the two travel on
    /// different queues, so the row it belongs on need not exist yet. The create and login paths
    /// replay this parked value when the player row arrives.
    ///
    /// The normal frame drains object messages before game events, so the player row normally exists
    /// before `0x0013` is dispatched. Parking covers the case where the halves arrive in separate
    /// receives.
    /// It is not a second owner: nothing reads it except the two install points.
    pub(crate) player_desc: Option<Qualities>,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    #[must_use]
    pub fn new() -> Self {
        Self {
            tables: ObjectTables::new(),
            physics_setups: std::collections::BTreeMap::new(),
            material_names: std::collections::BTreeMap::new(),
            vital_formulas: None,
            quality_filter: None,
            player: None,
            selected: None,
            prev_selected: None,
            prev_selected_valid: None,
            request_lock: RequestLock::default(),
            attack_in_progress: false,
            open_container: None,
            pending_row: None,
            ground_object: None,
            requested_ground_object: None,
            opened_corpses: std::collections::BTreeSet::new(),
            last_used: None,
            targeting_object: ObjectId(0),
            player_objdesc: None,
            inv_slots: crate::inventory::slots::InvSlotModule::new(),
            inventory_mask: 0,
            clothing_priority_mask: 0,
            pending_split: None,
            unblock: crate::inventory::equip::UnblockState::default(),
            combat: crate::combat::CombatState::default(),
            selected_meters: crate::combat::SelectedMeters::default(),
            player_system: crate::player::PlayerSystem::new(),
            chat: crate::chat::ChatState::new(),
            scroll: crate::scroll::Scroll::default(),
            appraisal: crate::appraisal::AppraisalCache::default(),
            magic: crate::magic::MagicState::default(),
            allegiance: crate::allegiance::AllegianceHierarchy::default(),
            allegiance_aborts: 0,
            allegiance_abort_last_reason: 0,
            portal_storm_level: 0.0,
            portal_storms_struck: 0,
            pending_portal_storm_scripts: Vec::new(),
            house: None,
            house_data_notices: 0,
            slumlord: None,
            house_profile_notices: 0,
            minigame: crate::minigame::MiniGame::new(),
            player_initialized: false,
            fellowship: None,
            trade: crate::trade::TradeSystem::default(),
            shop: crate::vendor::Shop::default(),
            contracts: crate::quests::ContractTrackerTable::default(),
            book: crate::book::BookState::default(),
            object_range_checks: crate::range::ObjectRangeCheckList::default(),
            selected_object_in_view: false,
            viewcone_check_object_id: None,
            selection_watch: None,
            selection_broadcast_pending: false,
            last_visible_rebuild: ServerTime(0.0),
            maintenance_active: true,
            player_desc: None,
        }
    }

    // -----------------------------------------------------------------------------------------
    // Object lookup and process-owned supporting data.
    // -----------------------------------------------------------------------------------------

    #[must_use]
    pub fn weenie(&self, id: ObjectId) -> Option<&Weenie> {
        self.tables.weenies.get(id)
    }

    pub fn weenie_mut(&mut self, id: ObjectId) -> Option<&mut Weenie> {
        self.tables.weenies.get_mut(id)
    }

    /// Install the names resolved through the client's one material-type-to-name transcription.
    pub fn install_material_names(&mut self, names: std::collections::BTreeMap<u32, String>) {
        self.material_names = names;
    }

    /// The normalized material name, or retail's null-string miss represented by `None`.
    #[must_use]
    pub fn material_name(&self, material: u32) -> Option<&str> {
        self.material_names.get(&material).map(String::as_str)
    }

    /// Character-session teardown does not unload the process-owned material mapper.
    pub fn preserve_material_names_from(&mut self, old: &mut Self) {
        self.material_names = std::mem::take(&mut old.material_names);
    }

    /// Install the vital-maximum formulas the receipt-time vital clamp computes maxima from.
    pub fn install_vital_formulas(&mut self, table: dereth_assets::tables::Attribute2ndTable) {
        self.vital_formulas = Some(table);
    }

    /// The installed vital-maximum formulas, if the host has loaded them.
    #[must_use]
    pub fn vital_formulas(&self) -> Option<&dereth_assets::tables::Attribute2ndTable> {
        self.vital_formulas.as_ref()
    }

    /// Install the quality filter the receipt-time vital clamp reads the maximum's enchantments
    /// through.
    pub fn install_quality_filter(&mut self, filter: dereth_assets::tables::QualityFilter) {
        self.quality_filter = Some(filter);
    }

    /// The installed quality filter, if the host has loaded it.
    #[must_use]
    pub fn quality_filter(&self) -> Option<&dereth_assets::tables::QualityFilter> {
        self.quality_filter.as_ref()
    }

    /// Character-session teardown does not unload the process-owned vital formulas or the quality
    /// filter either.
    pub fn preserve_vital_formulas_from(&mut self, old: &mut Self) {
        self.vital_formulas = old.vital_formulas.take();
        self.quality_filter = old.quality_filter.take();
    }

    #[must_use]
    pub fn inventory(&self, id: ObjectId) -> Option<&ObjectInventory> {
        self.tables.inventories.get(id)
    }

    pub fn inventory_mut(&mut self, id: ObjectId) -> Option<&mut ObjectInventory> {
        self.tables.inventories.get_mut(id)
    }

    #[must_use]
    pub fn physics(&self, id: ObjectId) -> Option<&PhysicsPresence> {
        self.tables.physics.get(id)
    }

    /// Replace the physics-state word retained for an object. The `0xF74B Item_SetState` handler
    /// calls this after its
    /// `update_times[STATE_TS]` gate, and the whole word is replaced rather than merged
    /// gate; the whole word is replaced rather than merged. Both readers use the same word the
    /// setter writes. Without this route, an object made static after creation keeps its old
    /// visibility membership.
    ///
    /// The gate is deliberately **not** here: it is
    /// `dereth_client_runtime::objects::ObjectStream::set_state` that holds
    /// `update_times[STATE_TS]`, and a second copy of the stamp beside a second copy of the word
    /// is exactly the shape to avoid. Returns whether an object of that
    /// id was present, so a caller can count a write that landed on nothing rather than assume it.
    pub fn set_physics_state(&mut self, id: ObjectId, state: u32) -> bool {
        match self.tables.physics.get_mut(id) {
            Some(p) => {
                p.state = state;
                true
            }
            None => false,
        }
    }

    /// Test whether `id` is the local player.
    #[must_use]
    pub fn is_the_player(&self, id: ObjectId) -> bool {
        self.player == Some(id)
    }

    /// The player id is set once; subsequent attempts are ignored.
    pub fn set_player(&mut self, id: ObjectId) -> bool {
        if self.player.is_some() {
            return false;
        }
        self.player = Some(id);
        // The player object is the only one that owns a player description; construction decides
        // that by comparing the new object id with the current player id.
        if let Some(w) = self.tables.weenies.get_mut(id) {
            w.qualities.get_or_insert_with(Qualities::new);
        }
        // `0x0013` may already have landed, in which case its DataID table is parked
        // and this is the moment its owner became known.
        self.install_player_desc();
        true
    }

    // -----------------------------------------------------------------------------------------
    // Object creation and merge handling.
    // -----------------------------------------------------------------------------------------

    /// `0xF745 Item_CreateObject`: older instance drops, equal merges, newer deletes/recreates.
    ///
    /// Despite the common opcode labels, `0xF745` merges an equal instance while `0xF7DB`
    /// always recreates it.
    pub fn create_or_merge(
        &mut self,
        p: &ObjectCreatePayload,
        now: ServerTime,
        out: &mut dyn NoticeSink,
    ) -> Result<ObjectId, GameError> {
        self.create_or_merge_with_dispatch(p, now, &mut |_, notice| out.emit(notice))
    }

    /// The host observes deletion callbacks before a same-id replacement is instantiated.
    /// Legacy NoticeSink callers use the wrapper above; a live owner must not defer this callback
    /// until after recreate has replaced the Weenie that the subscriber is about to read.
    pub fn create_or_merge_with_dispatch(
        &mut self,
        p: &ObjectCreatePayload,
        now: ServerTime,
        dispatch: &mut dyn FnMut(&mut Self, Notice),
    ) -> Result<ObjectId, GameError> {
        self.create_or_merge_with_dispatch_and_requests(p, now, dispatch, &mut crate::NullRequests)
    }

    /// Request-aware host seam for create-or-merge processing.
    ///
    /// The compatibility wrapper above deliberately retains its old signature. The live object
    /// stream supplies a sink because the duplicate-create arm can synchronously request a fresh
    /// object description.
    pub fn create_or_merge_with_dispatch_and_requests(
        &mut self,
        p: &ObjectCreatePayload,
        now: ServerTime,
        dispatch: &mut dyn FnMut(&mut Self, Notice),
        requests: &mut dyn RequestSink,
    ) -> Result<ObjectId, GameError> {
        let id = p.id;
        let seq = p.physicsdesc.timestamps.instance;
        // The merge/instance gate requires a physical object.
        // A preceding failed placeholder initialisation may have created only the Weenie.
        if self.tables.weenies.contains_key(id) && !self.tables.physics.contains_key(id) {
            self.delete_object_with_dispatch(id, now, dispatch);
            self.instantiate_with_dispatch(p, seq, now, dispatch);
            return Ok(id);
        }
        if let Some(existing) = self.tables.weenies.get(id) {
            if is_newer(existing.instance_seq, seq) {
                self.delete_object_with_dispatch(id, now, dispatch);
                self.instantiate_with_dispatch(p, seq, now, dispatch);
                return Ok(id);
            }
            // The instance-sequence gate, before anything else.
            if !is_newer(existing.instance_seq, seq) && existing.instance_seq != seq {
                return Err(GameError::StaleInstance {
                    id,
                    got: seq,
                    have: existing.instance_seq,
                });
            }
            // A duplicate physical create does not instantiate another object, but its descriptor
            // still follows the shared merge path. Passing `is_update = true` is load-bearing: it
            // enables step 5's desync request and restoration of the old containment. Without it,
            // a dropped item can retain a stale row in its old pack.
            let mut notices = crate::RecordingSink::default();
            if let Some(request) =
                self.set_weenie_desc(id, p.wdesc.clone(), true, now, &mut notices)
            {
                requests.send(request);
            }
            self.apply_physics_desc(p, now, false);
            for notice in notices.0 {
                dispatch(self, notice);
            }
            return Err(GameError::DuplicateCreate(id));
        }
        self.instantiate_with_dispatch(p, seq, now, dispatch);
        Ok(id)
    }

    /// `0xF7DB Item_UpdateObject` semantics: **always** delete and rebuild.
    pub fn recreate(
        &mut self,
        p: &ObjectCreatePayload,
        now: ServerTime,
        out: &mut dyn NoticeSink,
    ) -> Result<ObjectId, GameError> {
        self.recreate_with_dispatch(p, now, &mut |_, notice| out.emit(notice))
    }

    pub fn recreate_with_dispatch(
        &mut self,
        p: &ObjectCreatePayload,
        now: ServerTime,
        dispatch: &mut dyn FnMut(&mut Self, Notice),
    ) -> Result<ObjectId, GameError> {
        let id = p.id;
        let seq = p.physicsdesc.timestamps.instance;
        // Recreate deliberately skips the instance-sequence merge branch.
        if self.tables.weenies.contains_key(id) || self.tables.physics.contains_key(id) {
            self.delete_object_with_dispatch(id, now, dispatch);
        }
        self.instantiate_with_dispatch(p, seq, now, dispatch);
        Ok(id)
    }

    fn instantiate_with_dispatch(
        &mut self,
        p: &ObjectCreatePayload,
        seq: u16,
        now: ServerTime,
        dispatch: &mut dyn FnMut(&mut Self, Notice),
    ) {
        let mut notices = crate::RecordingSink::default();
        self.instantiate(p, seq, now, &mut notices);
        for notice in notices.0 {
            dispatch(self, notice);
        }
    }

    fn instantiate(
        &mut self,
        p: &ObjectCreatePayload,
        seq: u16,
        now: ServerTime,
        out: &mut dyn NoticeSink,
    ) {
        let id = p.id;
        // A null placeholder is promoted rather than replaced, and its destruction is unscheduled.
        let parked = self.tables.null_weenies.remove(id);
        self.remove_object_to_be_destroyed(id);

        let mut w = Weenie::new(id);
        w.instance_seq = seq;
        let is_player = self.player == Some(id);
        if is_player {
            // Reset the player description: tear down and recreate its qualities.
            w.qualities = Some(Qualities::new());
        }
        self.tables.weenies.insert(id, w);
        if is_player {
            // `0x0013` can land before this row exists — ACE enqueues
            // `GameEventPlayerDescription` ahead of `GameMessagePlayerCreate`/`CreateObject`, on a
            // different queue — and the empty `Qualities` just written would otherwise discard the
            // `CombatTable` DataID the client's melee arm reads.
            self.install_player_desc();
        }
        self.apply_physics_desc(p, now, true);
        self.set_weenie_desc(id, p.wdesc.clone(), false, now, out);

        // Blobs parked on the placeholder are replayed in `SequenceGate` sequence order.
        if let Some(mut ph) = parked {
            ph.queued_blobs.sort_by_key(|b| b.sequence.unwrap_or(0));
        }
        out.emit(Notice::ObjectCreated(id));
    }

    fn apply_physics_desc(&mut self, p: &ObjectCreatePayload, now: ServerTime, set_children: bool) {
        let d = &p.physicsdesc;
        let prior = self.tables.physics.get(p.id).copied();
        let setup_id = if set_children {
            d.setup_id.unwrap_or(0)
        } else {
            prior.map_or(0, |p| p.setup_id)
        };
        let inherited = self.physics_parent(p.id);
        if set_children {
            // CreateObject: a failed placeholder initialisation deletes only the physical object;
            // the independent Weenie creation/notice still occurs. Successful initialization
            // removes the null-table membership, but placeholder initialisation does not unparent.
            let failed = matches!(
                self.physics_setup_facts(setup_id),
                Some(crate::objects::PhysicsSetupFacts::Failed)
            );
            self.tables.null_physics.remove(p.id);
            if failed {
                self.unparent_physics_children(p.id, now);
                self.tables.physics.remove(p.id);
                return;
            }
        }
        // A decoded wire cell is not a loaded runtime cell. Asset-backed objects acquire that
        // truth from their actual physical owner, not from description arrival. Preserve an
        // existing object's cell until its accepted motion/position consumer runs.
        let cell = if set_children {
            if self.physics_setup_facts(setup_id).is_some() {
                None
            } else {
                d.position
                    .filter(|pos| pos.objcell_id != 0)
                    .map(|pos| CellId(pos.objcell_id))
            }
        } else {
            prior.and_then(|p| p.cell)
        };
        // The **whole** physics-state word, not `state & 1`. Each reader masks the
        // shared word for itself; keeping a single bit here would make
        // `CLOAKED_PS` and `REPORT_COLLISIONS_AS_ENVIRONMENT_PS` unanswerable from this table — see
        // `PhysicsPresence::state` and `objects::STATIC_PS`.
        let presence = PhysicsPresence {
            cell,
            state: d.state,
            parent: inherited,
            setup_id,
        };
        self.tables.physics.insert(p.id, presence);
        if let Some(w) = self.tables.weenies.get_mut(p.id) {
            w.has_phys_obj = true;
            w.phys_has_cell = cell.is_some();
        }
        if set_children {
            // SetChildren occurs before this object's own descriptor parent.
            self.unparent_physics_children(p.id, now);
            for c in d.children.iter().flatten() {
                if !self.tables.physics.contains_key(c.child_id) {
                    self.get_null_physics_object(c.child_id, now);
                }
                self.set_physics_parent(c.child_id, p.id, c.location_id, now);
            }
            if let Some((parent, location)) = d.parent.filter(|(id, _)| id.0 != 0) {
                self.set_physics_parent(p.id, parent, location, now);
            }
        }
    }

    pub fn physics_setup_facts(&self, setup: u32) -> Option<&crate::objects::PhysicsSetupFacts> {
        self.physics_setups.get(&setup)
    }

    /// Register resolved, immutable DAT initialization facts. No asset dependency belongs here.
    pub fn register_physics_setup(&mut self, setup: u32, facts: crate::objects::PhysicsSetupFacts) {
        self.physics_setups.entry(setup).or_insert(facts);
    }

    /// Rows of the resolved-setup memo. Instrumentation.
    ///
    /// Keyed by setup id and never removed, deliberately — a setup's holding locations do not
    /// change and re-resolving one costs a DAT read. So this is bounded by the *distinct setups*
    /// a session has seen and not by its live objects, and a long-session station prints it to
    /// tell that bound apart from a per-object leak standing next to it.
    #[must_use]
    pub fn physics_setup_count(&self) -> usize {
        self.physics_setups.len()
    }

    pub fn physics_parent(&self, id: ObjectId) -> Option<(ObjectId, u32)> {
        self.tables
            .physics
            .get(id)
            .and_then(|p| p.parent)
            .or_else(|| self.tables.null_physics.get(id).and_then(|p| p.parent))
    }

    /// add_child validates the parent before unset_parent/leave_world.
    /// A refused location/self/missing part array changes neither the old edge nor its timer.
    pub fn set_physics_parent(
        &mut self,
        id: ObjectId,
        parent: ObjectId,
        location: u32,
        now: ServerTime,
    ) -> bool {
        if id == parent
            || (!self.tables.physics.contains_key(id) && !self.tables.null_physics.contains_key(id))
        {
            return false;
        }
        let valid = self
            .tables
            .physics
            .get(parent)
            .and_then(|p| self.physics_setup_facts(p.setup_id))
            .is_some_and(|facts| {
                matches!(facts, crate::objects::PhysicsSetupFacts::Ready { holding_locations }
                if holding_locations.contains(&location))
            });
        if !valid {
            return false;
        }
        self.unset_physics_parent(id, now);
        self.leave_physics_world(id);
        if let Some(p) = self.tables.physics.get_mut(id) {
            p.parent = Some((parent, location));
        }
        if let Some(p) = self.tables.null_physics.get_mut(id) {
            p.parent = Some((parent, location));
        }
        // That path explicitly calls change_cell with the loaded parent's cell. enter_cell
        // has a part_array guard: a physical null cannot enter, even though its edge is valid.
        if let Some(cell) = self.tables.physics.get(parent).and_then(|p| p.cell) {
            if self.has_initialized_parts(id) {
                self.publish_physics_cell(id, Some(cell));
            }
        }
        true
    }

    fn has_initialized_parts(&self, id: ObjectId) -> bool {
        self.tables
            .physics
            .get(id)
            .and_then(|p| self.physics_setup_facts(p.setup_id))
            .is_some_and(|f| matches!(f, crate::objects::PhysicsSetupFacts::Ready { .. }))
    }

    /// Current loaded-cell truth from an actual independent physical owner. enter_cell
    /// and leave_cell propagate parent cell transitions to children, but null children
    /// have no part array and cannot enter. No transition means no re-run of enter_cell: late
    /// placeholder initialisation by itself must not acquire its unchanged parent's cell.
    pub fn publish_physics_cell(&mut self, id: ObjectId, cell: Option<CellId>) {
        let mut pending = vec![id];
        let mut visited = std::collections::BTreeSet::new();
        while let Some(id) = pending.pop() {
            if !visited.insert(id) {
                continue;
            }
            let Some(p) = self.tables.physics.get_mut(id) else {
                continue;
            };
            let changed = p.cell != cell;
            p.cell = cell;
            if let Some(w) = self.tables.weenies.get_mut(id) {
                w.phys_has_cell = cell.is_some();
            }
            if !changed {
                continue;
            }
            for child in self.physics_children(id) {
                if cell.is_none() || self.has_initialized_parts(child) {
                    pending.push(child);
                }
            }
        }
    }

    /// Unsetting the parent is a no-op without a parent; physical nulls retain the source update-time edge.
    pub fn unset_physics_parent(&mut self, id: ObjectId, now: ServerTime) {
        if let Some(p) = self.tables.physics.get_mut(id) {
            p.parent = None;
        }
        if let Some(p) = self.tables.null_physics.get_mut(id) {
            if p.parent.take().is_some() {
                p.update_time = now;
            }
        }
    }

    pub fn physics_children(&self, parent: ObjectId) -> Vec<ObjectId> {
        self.tables
            .physics
            .iter()
            .filter_map(|(id, p)| p.parent.is_some_and(|(p, _)| p == parent).then_some(id))
            .chain(
                self.tables
                    .null_physics
                    .iter()
                    .filter_map(|(id, p)| p.parent.is_some_and(|(p, _)| p == parent).then_some(id)),
            )
            .collect()
    }

    fn unparent_physics_children(&mut self, parent: ObjectId, now: ServerTime) {
        for child in self.physics_children(parent) {
            self.unset_physics_parent(child, now);
        }
    }

    /// leave_world cancels destruction even for a physical null, independently of cells.
    pub fn leave_physics_world(&mut self, id: ObjectId) {
        self.remove_object_to_be_destroyed(id);
        for (_, lost) in self.tables.lost_cells.iter_mut() {
            lost.objects.retain(|child| *child != id);
        }
        self.publish_physics_cell(id, None);
    }

    /// The null-object lookup with create set: only a new placeholder receives the current clock
    /// and destruction deadline. Looking up an existing placeholder does not renew either.
    /// A successful validated attachment cancels the deadline; a declared link alone does not.
    pub fn get_null_physics_object(&mut self, id: ObjectId, now: ServerTime) {
        if !self.tables.null_physics.contains_key(id) {
            self.tables.null_physics.insert(
                id,
                NullPlaceholder {
                    update_time: now,
                    ..Default::default()
                },
            );
            self.schedule_destroy(id, now);
        }
    }

    /// Merge a public descriptor during create or update handling.
    ///
    /// The update-only desync step compares the old and new container, wielder, and location. On
    /// the first mismatch it requests a fresh object description, sets the authentication latch,
    /// and restores all three old containment values before move handling runs. The move handler
    /// reads the old container from the object itself; restoring it is what removes the item from
    /// the list it actually left. This matters when a dropped object's create arrives before the
    /// corresponding move reply.
    ///
    /// The latch behavior is preserved: a later update that finds it set clears it and skips the
    /// comparison and restore. A descriptor-event reply does not pass through this function, so
    /// only that later update clears the latch. At most one force-description request is emitted
    /// for an accepted update.
    pub fn set_weenie_desc(
        &mut self,
        id: ObjectId,
        d: PublicWeenieDesc,
        is_update: bool,
        now: ServerTime,
        out: &mut dyn NoticeSink,
    ) -> Option<Request> {
        let w = self.tables.weenies.get_mut(id)?;
        // The `Option`s as well as the dwords: the three fields are put back *as they were* when
        // step 5 restores them, so an absent container does not become a present zero.
        let (was_container, was_wielder, was_location) =
            (w.pwd.container_id, w.pwd.wielder_id, w.pwd.location);
        let old_container = was_container.unwrap_or_default();
        let old_wielder = was_wielder.unwrap_or_default();
        let old_location = was_location.unwrap_or(0);

        w.pwd = d;
        w.determine_position_state();

        let new_container = w.pwd.container_id.unwrap_or_default();
        let new_wielder = w.pwd.wielder_id.unwrap_or_default();
        let new_location = w.pwd.location.unwrap_or(0);
        let changed = new_container != old_container
            || new_wielder != old_wielder
            || new_location != old_location;

        // Step 5, in the client's own order: the flag is tested **before** the comparison, and
        // when it is set it is cleared and everything below it is skipped. (Testing
        // `changed && !asked` first would leave the flag standing on a changed update, which is a
        // different machine.)
        let mut ask = None;
        if is_update {
            if w.awaiting_authentication {
                w.awaiting_authentication = false;
            } else if changed {
                ask = Some(Request::ForceObjdesc(ObjectSendForceObjdesc { id }));
                w.awaiting_authentication = true;
                // ... See this function's own note: `server_says_move_item`
                // reads the old container off `pwd`, so it has to be there when it runs.
                w.pwd.container_id = was_container;
                w.pwd.wielder_id = was_wielder;
                w.pwd.location = was_location;
            }
        }
        // Read the current wielder **after** restoring the old containment fields, as retail does.
        let wielder_now = self
            .tables
            .weenies
            .get(id)
            .and_then(|w| w.pwd.wielder_id)
            .unwrap_or_default();

        // Step 4: a container the client was viewing keeps its contents list across an update —
        // and keeps the slot `server_says_contain_id` pre-placed the id in before
        // the id had an object at all.
        //
        // Ask the **one** list the child belongs in
        // (its place in the items list or the containers list) and substitutes 0 for the client's
        // `-1`. Substituting 0 is why an un-pre-placed item lands at the *head* of the pack rather
        // than in its slot; that fallback is the client's and is reproduced, not corrected.
        let containers_list = self
            .tables
            .weenies
            .get(id)
            .is_some_and(Weenie::goes_in_containers_list);
        let place = self
            .tables
            .inventories
            .get(new_container)
            .and_then(|inv| inv.place_in_list(id, containers_list))
            .unwrap_or(0);

        // **The move is three arms, not one call.**
        //
        // ```text
        //   if (new_container != 0)
        //       place = the place in the items/containers list, -1 -> 0
        //       server_says_move_item(new_container, place, 0, 0, 1)
        //   else if ((the player's id matches the object's LIVE wielder id,
        //             and the world view and player id are non-null) or is_update)
        //       server_says_move_item(0, 0, new_wielder, new_location, 1)
        //   else  -- a CREATE of an unwielded, uncontained object makes NO call at all
        // ```
        //
        // Two details are visible: the container arm passes the
        // **literals 0 and 0** for wielder and location rather than the
        // descriptor's, and the `new_container == 0` arm is guarded — an object created loose in
        // the world with no wielder is not moved at all, so it raises no `ItemMoved` and does not
        // clear the request lock.
        if new_container.0 != 0 {
            self.server_says_move_item(
                id,
                new_container,
                u32::try_from(place).unwrap_or(0),
                ObjectId(0),
                0,
                true,
                out,
            );
        } else if self.player.is_some_and(|p| p.0 != 0 && p == wielder_now) || is_update {
            self.server_says_move_item(id, ObjectId(0), 0, new_wielder, new_location, true, out);
        }
        self.declare_valid(id, now, out);
        ask
    }

    /// Mark an object valid and apply validity-triggered state transitions.
    ///
    /// `pub(crate)` because the vendor calls it on
    /// each stock object it manufactures, and that loop lives in [`crate::vendor`] because the
    /// shop does.
    pub(crate) fn declare_valid(
        &mut self,
        id: ObjectId,
        now: ServerTime,
        out: &mut dyn NoticeSink,
    ) {
        let contents = self
            .tables
            .inventories
            .get(id)
            .map(ObjectInventory::all_contents);
        if let Some(ids) = contents {
            for c in ids {
                self.remove_object_to_be_destroyed(c);
            }
        }
        if let Some(w) = self.tables.weenies.get_mut(id) {
            w.valid = true;
        }
        out.emit(Notice::ItemAttributesChanged {
            object: id,
            kind: 1,
        });

        // ..: a split's newly valid result becomes the selection when both
        // qualities the writers saved agree. The match is tested before expiry; a non-match is
        // what reaches the strict `10.0 < cur_time - split_time` clear.
        if let Some(pending) = self.pending_split {
            let matches = self.tables.weenies.get(id).is_some_and(|w| {
                pending.wcid == w.pwd.wcid
                    && pending.stack_size == u32::from(w.pwd.stack_size.unwrap_or(0).max(1))
            });
            if matches {
                self.set_selected_object(Some(id), false, out);
                self.pending_split = None;
            } else if SPLIT_WINDOW < now.0 - pending.at.0 {
                self.pending_split = None;
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Null tables — the "waiting for" objects.
    // -----------------------------------------------------------------------------------------

    /// With `create = true`, build the placeholder,
    /// stamp `update_time`, and schedule it for destruction so a never-resolved reference dies.
    pub fn get_null_weenie_object(&mut self, id: ObjectId, now: ServerTime) {
        if self.tables.weenies.contains_key(id) || self.tables.null_weenies.contains_key(id) {
            return;
        }
        self.tables.null_weenies.insert(
            id,
            NullPlaceholder {
                update_time: now,
                ..Default::default()
            },
        );
        self.schedule_destroy(id, now);
    }

    /// Queue an ordered or unordered blob for an object that has not arrived yet.
    ///
    /// This is the session's `object_arrived(id, instance_seq)` seam seen from the other side:
    /// a blob addressed to an object that does not exist yet is parked on the placeholder and
    /// replayed when the real object arrives.
    pub fn queue_blob_for_weenie_object(
        &mut self,
        id: ObjectId,
        blob: QueuedBlob,
        now: ServerTime,
    ) -> bool {
        if self.tables.weenies.contains_key(id) {
            return false;
        }
        self.get_null_weenie_object(id, now);
        if let Some(p) = self.tables.null_weenies.get_mut(id) {
            p.queued_blobs.push(blob);
        }
        self.schedule_destroy(id, now);
        true
    }

    /// Take the blobs parked on a placeholder, in `SequenceGate` sequence order.
    pub fn take_queued_blobs(&mut self, id: ObjectId) -> Vec<QueuedBlob> {
        let mut b = self
            .tables
            .null_weenies
            .get_mut(id)
            .map(|p| std::mem::take(&mut p.queued_blobs))
            .unwrap_or_default();
        b.sort_by_key(|x| x.sequence.unwrap_or(0));
        b
    }

    // -----------------------------------------------------------------------------------------
    // Lost cells.
    // -----------------------------------------------------------------------------------------

    /// Park an object in a lost cell only when it has no parent.
    pub fn goto_lost_cell(&mut self, id: ObjectId, cell: CellId) {
        if self.tables.physics.get(id).and_then(|p| p.parent).is_some() {
            return;
        }
        let entry = self.tables.lost_cells.get_mut(cell);
        match entry {
            Some(c) => {
                if !c.objects.contains(&id) {
                    c.objects.push(id);
                }
            }
            None => {
                self.tables.lost_cells.insert(
                    cell,
                    LostCell {
                        cell,
                        objects: vec![id],
                    },
                );
            }
        }
    }

    /// Remove an object from a lost cell only when it has neither a loaded cell nor a parent.
    pub fn remove_from_lost_cell(&mut self, id: ObjectId, cell: CellId) {
        let has_cell_or_parent = self
            .tables
            .physics
            .get(id)
            .is_some_and(|p| p.cell.is_some() || p.parent.is_some());
        if has_cell_or_parent {
            return;
        }
        if let Some(c) = self.tables.lost_cells.get_mut(cell) {
            c.objects.retain(|x| *x != id);
        }
    }

    /// When a cell loads, pop its lost-cell entry and return every object awaiting reentry.
    pub fn init_obj_cell(&mut self, cell: CellId) -> Vec<ObjectId> {
        self.tables
            .lost_cells
            .remove(cell)
            .map(|c| c.objects)
            .unwrap_or_default()
    }

    /// The model tail of prepare_to_enter_world, before reenter_visibility places the
    /// actual surviving body. Cell initialization has removed this cell's list before invoking it.
    pub fn prepare_physics_reentry(&mut self, id: ObjectId) {
        for (_, lost) in self.tables.lost_cells.iter_mut() {
            lost.objects.retain(|child| *child != id);
        }
        self.remove_object_to_be_destroyed(id);
        for child in self.physics_children(id) {
            self.remove_object_to_be_destroyed(child);
        }
    }

    /// Release an unloading object cell.
    ///
    /// Build the departure list **before** calling [`Self::leave_visibility`], because leaving
    /// mutates cell membership. Only non-static objects with no parent are processed. Each receives
    /// the supplied `now` for its destruction deadline. Returning the ids lets the caller observe
    /// what left.
    ///
    /// This path must run during cell teardown: otherwise a remote object keeps `cell = Some(...)`,
    /// remains in the visible table, and is never scheduled for destruction. This build has no
    /// separate per-cell object list, so `p.cell == Some(cell)` selects the same set from the global
    /// physics table.
    pub fn release_obj_cell(&mut self, cell: CellId, now: ServerTime) -> Vec<ObjectId> {
        let leaving: Vec<ObjectId> = self
            .tables
            .physics
            .iter()
            // Release only non-static objects without a parent.
            .filter(|(_, p)| p.cell == Some(cell) && !p.is_static() && p.parent.is_none())
            .map(|(id, _)| id)
            .collect();
        for id in &leaving {
            self.leave_visibility(*id, now);
        }
        leaving
    }

    /// Release the **outdoor** cells of a departing landblock before its visible interior cells.
    /// The client visits `side_cell_count²` outdoor cells when a loaded block is released. Each
    /// cell uses [`Self::release_obj_cell`], so the non-static/no-parent filter is shared with the
    /// interior route.
    ///
    /// The distinct cell ids are derived from resident objects because this crate has no cell
    /// registry. Empty cells would release nothing, so omitting them is equivalent here. Without
    /// this outdoor half, objects on departed terrain retain a cell, remain visible, and never get
    /// a destruction deadline. Returns every object that left visibility.
    pub fn release_land_cells(&mut self, block: LandblockId, now: ServerTime) -> Vec<ObjectId> {
        let mut cells: Vec<CellId> = self
            .tables
            .physics
            .iter()
            .filter_map(|(_, p)| p.cell)
            .filter(|c| c.landblock() == block && c.is_outdoor())
            .collect();
        cells.sort_unstable_by_key(|c| c.0);
        cells.dedup();
        let mut left = Vec::new();
        for c in cells {
            left.extend(self.release_obj_cell(c, now));
        }
        left
    }

    /// Release every distinct **interior** cell represented by objects in a departing landblock.
    /// Call this after [`Self::release_land_cells`]. Empty cells need no object-side work.
    pub fn flush_cells(&mut self, block: LandblockId, now: ServerTime) -> Vec<ObjectId> {
        let mut cells: Vec<CellId> = self
            .tables
            .physics
            .iter()
            .filter_map(|(_, p)| p.cell)
            .filter(|c| c.landblock() == block && !c.is_outdoor())
            .collect();
        cells.sort_unstable_by_key(|c| c.0);
        cells.dedup();
        let mut left = Vec::new();
        for c in cells {
            left.extend(self.release_obj_cell(c, now));
        }
        left
    }

    /// Remove an object from visibility using the state held by this crate.
    ///
    /// The client removes shadows, attempts lost-cell removal, leaves the cell, schedules the
    /// object and every child for destruction, stores its position, then parks it in the lost-cell
    /// table and clears the transient visibility bit.
    ///
    /// **Three of those eight lines are not this crate's, and each is named rather than dropped:**
    ///
    /// * Shadow removal is bookkeeping on the retained *physics* object —
    ///   `dereth_physics::PhysicsWorld`'s, reached from `dereth_client_runtime`'s
    ///   `ObjectPhysics::leave_visibility`, called by the production release wrapper while
    ///   retaining the actual body. Nothing here holds a shadow list.
    /// * `store_position` copies `position` into the object's saved slot. Positions live in
    ///   `dereth_client_runtime::objects::Presence::position`, not in this table; the production wrapper
    ///   captures the current physical pose there rather than retaining an older wire pose.
    /// * `transient_state &= ~0x80` clears the *active* bit in the same physical-owner wrapper.
    ///
    /// **The lost-cell removal is kept even though it is a no-op here, and so it is in
    /// retail.** It proceeds only when the object has neither a cell nor a parent, and it runs *before* `leave_cell`, so an object arriving down this
    /// path still holds its cell and the gate refuses it. It is transcribed because the ordering
    /// is the thing that makes it a no-op, and a later reader moving the two lines would change
    /// behaviour with no visible edit — see [`Self::remove_from_lost_cell`].
    ///
    /// Returns `false` for an object this table does not hold, or one already in no cell.
    pub fn leave_visibility(&mut self, id: ObjectId, now: ServerTime) -> bool {
        let Some(cell) = self.tables.physics.get(id).and_then(|p| p.cell) else {
            return false;
        };
        self.park_physics_visibility(id, cell, now);
        true
    }

    /// The position setter's missing-cell tail can run after entry preparation has
    /// already canceled the old deadline, while this physical object still has no cell.
    pub fn failed_physics_reentry(&mut self, id: ObjectId, cell: CellId, now: ServerTime) {
        if self.tables.physics.contains_key(id) {
            self.park_physics_visibility(id, cell, now);
        }
    }

    fn park_physics_visibility(&mut self, id: ObjectId, cell: CellId, now: ServerTime) {
        // The lost-cell removal — before `leave_cell`, and therefore refused by its own gate.
        self.remove_from_lost_cell(id, cell);
        // `leave_cell(this, 0)`. This is the write `update_visible_object_list` reads: its whole
        // test is `p.cell.is_some() && !p.is_static()`.
        self.publish_physics_cell(id, None);
        // Schedule the object for destruction, then **every child** — the loop over its
        // children. This build stores the edge on the child
        // (`PhysicsPresence::parent`) and derives a holder's children by scanning, exactly as
        // `dereth_client_runtime::objects::ObjectStream::unset_parent` records.
        self.schedule_destroy(id, now);
        let children = self.physics_children(id);
        for c in children {
            self.schedule_destroy(c, now);
        }
        // Send it to the lost cell for its own `position.objcell_id` — the cell it was standing
        // in, which is why it is read above and not after `leave_cell` clears it. The retail call
        // reads `position.objcell_id`, which `leave_cell` does not touch; here the same value is
        // the cell taken at the top.
        self.goto_lost_cell(id, cell);
    }

    // -----------------------------------------------------------------------------------------
    // Destruction and delayed cleanup.
    // -----------------------------------------------------------------------------------------

    /// Schedule destruction at exactly `now + 25.0` seconds.
    ///
    /// The existing table entry is replaced but the stale *queue* entry is left behind; the
    /// `2e-4` deadline re-check is what makes a re-scheduled object survive a stale queue entry.
    pub fn schedule_destroy(&mut self, id: ObjectId, now: ServerTime) {
        let when = now.0 + DESTRUCTION_DELAY;
        self.tables.doomed.insert(id, ServerTime(when));
        self.tables.doom_queue.push(Reverse(DoomEntry { when, id }));
    }

    /// Cancel a pending destruction deadline.
    pub fn remove_object_to_be_destroyed(&mut self, id: ObjectId) {
        self.tables.doomed.remove(id);
    }

    /// Schedule every directly contained object for destruction.
    pub fn add_contents_to_destruction_queue(&mut self, id: ObjectId, now: ServerTime) {
        if let Some(ids) = self.tables.inventories.get(id).map(|inv| {
            inv.containers
                .iter()
                .chain(&inv.items)
                .copied()
                .collect::<Vec<_>>()
        }) {
            for c in ids {
                self.schedule_destroy(c, now);
            }
        }
    }

    /// Cancel destruction for every directly contained object.
    pub fn remove_contents_from_destruction_queue(&mut self, id: ObjectId) {
        if let Some(inv) = self.tables.inventories.get(id).cloned() {
            for c in inv.containers {
                self.remove_object_to_be_destroyed(c);
                if self.weenie(c).is_some() {
                    self.remove_contents_from_destruction_queue(c);
                }
            }
            for c in inv.items {
                self.remove_object_to_be_destroyed(c);
            }
        }
    }

    /// Run periodic object maintenance.
    ///
    /// Visible refresh first; strictly expired matching deadlines next; then both null tables.
    /// Twenty seconds is each unresolved object's age, not a global sweep throttle.
    pub fn use_time(
        &mut self,
        now: ServerTime,
        out: &mut dyn NoticeSink,
        req: &mut dyn RequestSink,
    ) {
        self.use_time_with_dispatch(now, &mut |_, notice| out.emit(notice), req);
    }

    /// The same maintenance with object-being-deleted callbacks completed before table removal.
    pub fn use_time_with_dispatch(
        &mut self,
        now: ServerTime,
        dispatch: &mut dyn FnMut(&mut Self, Notice),
        req: &mut dyn RequestSink,
    ) {
        if !self.maintenance_active {
            return;
        }
        if now.0 - self.last_visible_rebuild.0 > VISIBLE_REBUILD_INTERVAL {
            self.update_visible_object_list();
            self.last_visible_rebuild = now;
        }
        while let Some(Reverse(top)) = self.tables.doom_queue.peek().copied() {
            if top.when >= now.0 {
                break;
            }
            self.tables.doom_queue.pop();
            let scheduled = self.tables.doomed.get(top.id).copied();
            match scheduled {
                Some(t) if (t.0 - top.when).abs() < DESTRUCTION_RECHECK_EPSILON => {
                    self.tables.doomed.remove(top.id);
                    self.delete_object_with_dispatch(top.id, now, dispatch);
                }
                // Either un-scheduled, or re-scheduled to a later time: the queue entry is stale.
                _ => {}
            }
        }

        self.sweep_null_tables(now, req);
    }

    /// Clear and rebuild visibility from objects that have a cell and whose **`STATIC_PS`** bit is
    /// clear. Runs at most once per second.
    ///
    /// The bit is `state & 1`, which is `STATIC_PS` and **not** `HIDDEN_PS`
    /// (`0x4000`); see `objects::STATIC_PS` for why the two readings differ in meaning.
    pub fn update_visible_object_list(&mut self) {
        self.tables.visible.clear();
        for (id, p) in self.tables.physics.iter() {
            if p.cell.is_some() && !p.is_static() {
                self.tables.visible.insert(id);
            }
        }
    }

    /// The 20 s half of the per-frame object maintenance: walk **both** null tables and re-request
    /// anything whose
    /// `update_time` is more than 20 s old. This is the client's only recovery path for a
    /// permanently dangling reference.
    fn sweep_null_tables(&mut self, now: ServerTime, req: &mut dyn RequestSink) {
        for table in [&mut self.tables.null_physics, &mut self.tables.null_weenies] {
            for (id, p) in table.iter_mut() {
                if now.0 - p.update_time.0 > FORCE_OBJDESC_INTERVAL {
                    p.update_time = now;
                    req.send(Request::ForceObjdesc(ObjectSendForceObjdesc { id }));
                }
            }
        }
    }

    /// Delete an object by id, with the being-deleted callback before table removal.
    pub fn delete_object(&mut self, id: ObjectId, now: ServerTime, out: &mut dyn NoticeSink) {
        self.delete_object_with_dispatch(id, now, &mut |_, notice| out.emit(notice));
    }

    /// The old object remains queryable through removal notices and child work. The deleted notice
    /// follows actual table removal.
    pub fn delete_object_with_dispatch(
        &mut self,
        id: ObjectId,
        now: ServerTime,
        dispatch: &mut dyn FnMut(&mut Self, Notice),
    ) {
        // Corpse-history cleanup runs even if the base object lookup fails.
        self.opened_corpses.remove(&id);
        let physical = self.tables.physics.remove(id).is_some()
            | self.tables.null_physics.remove(id).is_some();
        if physical {
            self.tables.doomed.remove(id);
            // leave_world removes lost-cell membership; DeleteObject unparents children.
            let cells: Vec<_> = self
                .tables
                .lost_cells
                .iter()
                .filter_map(|(cell, lost)| lost.objects.contains(&id).then_some(cell))
                .collect();
            for cell in cells {
                self.remove_from_lost_cell(id, cell);
            }
            self.unparent_physics_children(id, now);
        }
        // visible_object_table is a periodic cache; deletion does not edit it.

        let Some(w) = self.tables.weenies.get(id) else {
            let placeholder = self.tables.null_weenies.remove(id).is_some();
            if physical || placeholder {
                dispatch(self, Notice::ObjectDeleted(id));
            }
            return;
        };

        let skip_remove = w.marked_for_deletion && !w.moved_while_marked_for_deletion;
        if let Some(w) = self.tables.weenies.get_mut(id) {
            w.has_phys_obj = false;
            w.phys_has_cell = false;
        }

        // Step 2: a cleanly marked-and-unmoved object skips `Remove`, because the removal was
        // already announced.
        if !skip_remove {
            self.remove_from_ui_with_dispatch(id, dispatch);
        }

        // Step 3: contents die with their container, but only conditionally.
        // A blanket cascade deletes items the server has already re-homed.
        if let Some(inv) = self.tables.inventories.get(id) {
            let contents = inv.all_contents();
            for c in contents {
                let still_ours = match self.tables.weenies.get(c) {
                    None => true,
                    Some(cw) => cw.pwd.container_id.unwrap_or_default() == id,
                };
                let no_cell = self.tables.physics.get(c).is_none_or(|p| p.cell.is_none());
                if still_ours && no_cell {
                    self.schedule_destroy(c, now);
                }
            }
        }
        // The object-being-deleted notice runs while the weenie lookup can still find this exact old
        // instance.
        self.tables.weenies.remove(id);
        self.tables.null_weenies.remove(id);
        self.tables.inventories.remove(id);
        dispatch(self, Notice::ObjectDeleted(id));
    }

    /// Apply the shared object-removal path used by the server remove handler.
    fn remove_from_ui_with_dispatch(
        &mut self,
        id: ObjectId,
        dispatch: &mut dyn FnMut(&mut Self, Notice),
    ) {
        if let Some(w) = self.tables.weenies.get_mut(id) {
            w.pre_remove_container = w.pwd.container_id.unwrap_or_default();
            w.pre_remove_wielder = w.pwd.wielder_id.unwrap_or_default();
            w.pre_remove_location = w.pwd.location.unwrap_or(0);
            w.being_removed = true;
        }
        let mut notices = crate::RecordingSink::default();
        self.server_says_move_item(id, ObjectId(0), 0, ObjectId(0), 0, true, &mut notices);
        for notice in notices.0 {
            dispatch(self, notice);
        }
        if self.selected == Some(id) {
            let mut notices = crate::RecordingSink::default();
            self.set_selected_object(None, false, &mut notices);
            for notice in notices.0 {
                dispatch(self, notice);
            }
        }
        if let Some(w) = self.tables.weenies.get_mut(id) {
            w.being_removed = false;
        }
    }

    /// The UI queue's remove case: known object detaches and is scheduled; unknown emits only
    /// the all-zero destination ItemMoved notice. Remove does not set marked_for_deletion.
    pub fn server_says_remove(&mut self, id: ObjectId, now: ServerTime, out: &mut dyn NoticeSink) {
        self.server_says_remove_with_dispatch(id, now, &mut |_, notice| out.emit(notice));
    }

    /// Remove keeps being_removed true across ItemMoved and SelectionChanged; the
    /// combat last-attacker branch reads that flag synchronously before choosing another target.
    pub fn server_says_remove_with_dispatch(
        &mut self,
        id: ObjectId,
        now: ServerTime,
        dispatch: &mut dyn FnMut(&mut Self, Notice),
    ) {
        if self.weenie(id).is_none() {
            dispatch(
                self,
                Notice::ItemMoved {
                    object: id,
                    old_container: ObjectId(0),
                    old_wielder: ObjectId(0),
                    old_location: 0,
                    container: ObjectId(0),
                    place: 0,
                    wielder: ObjectId(0),
                    location: 0,
                },
            );
            return;
        }
        self.remove_from_ui_with_dispatch(id, dispatch);
        self.add_contents_to_destruction_queue(id, now);
        self.schedule_destroy(id, now);
    }

    // -----------------------------------------------------------------------------------------
    // Selection and range-watch broadcasts.
    // -----------------------------------------------------------------------------------------

    /// Set the selected object.
    ///
    /// `SelectionChanged` is emitted only when `old != id`. The selected-item broadcast is outside
    /// that equality guard, so `force = true` on an unchanged id re-arms the selection range watch
    /// without emitting `SelectionChanged`. This method has neither a radar radius nor a clock, so
    /// it records the broadcast and the next range-calculation pass consumes it. That pending edge
    /// is the observable effect of forcing an unchanged selection.
    ///
    /// The client also performs a plugin callback outside the guard. This build has no plugin API
    /// and does not model that callback.
    pub fn set_selected_object(
        &mut self,
        id: Option<ObjectId>,
        force: bool,
        out: &mut dyn NoticeSink,
    ) {
        if !force && self.selected == id {
            return;
        }
        let old = self.selected;
        if let Some(o) = old {
            if let Some(w) = self.tables.weenies.get_mut(o) {
                w.selected = false;
            }
        }
        self.selected = id;
        if let Some(n) = id {
            if let Some(w) = self.tables.weenies.get_mut(n) {
                w.selected = true;
            }
        }
        if old != id {
            // The two toolbar meters are per-*selection*, not per-object. Health and item-mana
            // notices write them only for the selected id, and selection rewrites both before any
            // reply can arrive. Carrying the old object's fraction across a selection change would
            // draw the previous target's health on the new one for one round trip.
            self.selected_meters = crate::combat::SelectedMeters::default();
            self.prev_selected = old;
            if old.is_some() {
                self.prev_selected_valid = old;
            }
            out.emit(Notice::SelectionChanged {
                previous: old,
                current: id,
            });
        }
        // This broadcast sits outside the `old != id` skip and runs on every call that passes the
        // `force` guard. It is the whole observable effect of forcing an unchanged selection. The
        // subscriber modeled here is the selection range-watch registrant, so a forced re-selection **re-arms the
        // watch at the current radar radius** — which is how a player who walks indoors and
        // re-presses the same target gets the 25 m watch instead of the 75 m one. See
        // It is recorded because this method has neither the radar radius nor a clock.
        //
        // The plugin dispatch is the other thing outside the skip and this build has
        // no plugin API at all, so it is named rather than modelled.
        self.selection_broadcast_pending = true;
    }

    // -----------------------------------------------------------------------------------------
    // Containment and inventory projection.
    // -----------------------------------------------------------------------------------------

    /// Project a container's contents into item and container rows.
    pub fn view_object_contents(
        &mut self,
        container: ObjectId,
        profiles: &[ContentProfile],
        out: &mut dyn NoticeSink,
    ) {
        let inv = match self.tables.inventories.get_mut(container) {
            Some(inv) => inv,
            None => {
                self.tables
                    .inventories
                    .insert(container, ObjectInventory::new(container));
                self.tables
                    .inventories
                    .get_mut(container)
                    .expect("just inserted")
            }
        };
        inv.items.clear();
        inv.containers.clear();
        for p in profiles {
            // A non-zero `container_properties` means "this entry is itself a container".
            if p.container_properties == 0 {
                inv.items.push(p.iid);
            } else {
                inv.containers.push(p.iid);
            }
        }
        out.emit(Notice::InventoryChanged(container));
    }

    /// Stop viewing a container's contents.
    pub fn stop_viewing_object_contents(&mut self, container: ObjectId, out: &mut dyn NoticeSink) {
        if self.tables.inventories.remove(container).is_some() {
            out.emit(Notice::InventoryChanged(container));
        }
    }

    /// Replace the player's inventory placements wholesale.
    pub fn update_object_inventory(
        &mut self,
        container: ObjectId,
        placements: Vec<crate::objects::InventoryPlacement>,
    ) {
        if let Some(inv) = self.tables.inventories.get_mut(container) {
            inv.placements = placements;
        }
        if Some(container) == self.player {
            self.remake_character_inventory();
        }
    }

    /// Rebuild the inventory mask,
    /// the clothing-priority mask and the doll slots from the player's inventory placements.
    ///
    /// `0x0013`'s `inventory_placements` reach [`Self::update_object_inventory`], and this turns
    /// them into masks; without it the client would know what the character was wearing and still
    /// answer "every slot is free" to every equip decision. The client's own order: zero both
    /// masks, set UI item 0 into location `0x7FFFFFFF` to blank all twenty-four slots, then one
    /// pass over the list.
    ///
    /// Note the asymmetry, which is the client's: the inventory mask takes **every** placement
    /// whose `loc_` is non-zero, but the clothing-priority mask takes the priority only when
    /// `loc_ & 0x08007FFF` — the placement's own location, not the item's `_valid_locations`.
    pub fn remake_character_inventory(&mut self) {
        let Some(player) = self.player else { return };
        self.inventory_mask = 0;
        self.clothing_priority_mask = 0;
        self.inv_slots
            .set_into_location(crate::inventory::slots::loc::ALL, ObjectId(0));
        let Some(inv) = self.tables.inventories.get(player) else {
            return;
        };
        let placements: Vec<(ObjectId, u32, u32)> = inv
            .placements
            .iter()
            .map(|p| (p.iid, p.loc, p.priority))
            .collect();
        for (iid, loc, priority) in placements {
            if loc != 0 {
                self.inventory_mask |= loc;
            }
            if loc & crate::inventory::slots::loc::WEARABLE != 0 {
                self.clothing_priority_mask |= priority;
            }
            self.inv_slots.set_into_location(loc, iid);
        }
    }

    /// Test ownership by walking the container and wielder ids upward.
    ///
    /// **The identity arm matters.** The client's first
    /// test returns 1 when the object's own id, container id or wielder id equals the owner —
    /// so **an object is owned by itself**, and in particular the player owns the player. A walk
    /// that started at `item`'s container and never compared `item` with `owner` would answer
    /// false for the one object the question is asked about most.
    ///
    /// It is load-bearing for: clicking the main-pack
    /// slot re-points the grid at the **player**, and without this arm the notice would be
    /// refused and would stay pointed at the side pack the player had
    /// just navigated away from. \[verified\]
    #[must_use]
    pub fn is_owned_by_object(&self, item: ObjectId, owner: ObjectId) -> bool {
        // A zero owner owns nothing.
        if owner.0 == 0 {
            return false;
        }
        // The object's own id — the first disjunct.
        if item == owner {
            return true;
        }
        let mut cur = item;
        // The chain is short in practice; the bound stops a cycle from hanging the client, which
        // the original achieves by the same walk terminating on a NULL lookup.
        for _ in 0..32 {
            let Some(w) = self.tables.weenies.get(cur) else {
                return false;
            };
            let next = match (w.pwd.container_id, w.pwd.wielder_id) {
                (Some(c), _) if c.0 != 0 => c,
                (_, Some(x)) if x.0 != 0 => x,
                _ => return false,
            };
            if next == owner {
                return true;
            }
            cur = next;
        }
        false
    }

    /// Test whether an object belongs to the local player.
    #[must_use]
    pub fn is_owned_by_player(&self, item: ObjectId) -> bool {
        self.player
            .is_some_and(|p| self.is_owned_by_object(item, p))
    }

    /// Record a new open inventory destination only when its object exists and is owned by the
    /// player. Both guards matter: an unseen id cannot become a destination, and ground containers
    /// such as corpses and chests must remain in [`Self::ground_object`] instead. There is no
    /// clear-to-zero arm; session reset clears the field.
    ///
    /// Returns whether the field changed, for the caller's counter.
    ///
    /// The explicit existence guard is retained even though the ownership walk also returns false
    /// on a missing row. It represents the client's null-object guard structurally.
    pub fn on_new_parent_container(&mut self, container: ObjectId) -> bool {
        // No weenie for the container -> return.
        if self.weenie(container).is_none() {
            return false;
        }
        // Not owned by the player -> return.
        if !self.is_owned_by_player(container) {
            return false;
        }
        let changed = self.open_container != Some(container);
        self.open_container = Some(container);
        changed
    }

    /// Recursively determine whether all referenced inventory objects have arrived; this gates the
    /// login completion check.
    #[must_use]
    pub fn all_contained_objects_exist(&self, id: ObjectId) -> bool {
        let Some(inv) = self.tables.inventories.get(id) else {
            return true;
        };
        inv.all_contents()
            .into_iter()
            .all(|c| self.tables.weenies.contains_key(c) && self.all_contained_objects_exist(c))
    }

    /// Apply the server's authoritative item move.
    ///
    /// Note `pwd.wielder_id` is written **only** when the wielder is (or was) the local player; for
    /// other creatures the field is left as the descriptor set it.
    #[allow(clippy::too_many_arguments)]
    pub fn server_says_move_item(
        &mut self,
        id: ObjectId,
        container: ObjectId,
        place: u32,
        wielder: ObjectId,
        location: u32,
        notify_ui: bool,
        out: &mut dyn NoticeSink,
    ) {
        let player = self.player;
        let Some(w) = self.tables.weenies.get(id) else {
            return;
        };
        let old_container = w.pwd.container_id.unwrap_or_default();
        let old_wielder = w.pwd.wielder_id.unwrap_or_default();
        let old_location = w.pwd.location.unwrap_or(0);
        // The client's own list test, which is not the container test — see
        // `Weenie::goes_in_containers_list`.
        let is_container = w.goes_in_containers_list();
        let priority = w.pwd.priority.unwrap_or(0);
        let valid_locations = w.pwd.valid_locations.unwrap_or(0);

        if old_container.0 != 0 {
            if let Some(inv) = self.tables.inventories.get_mut(old_container) {
                inv.remove_content(id);
            }
        }
        if let Some(w) = self.tables.weenies.get_mut(id) {
            w.pwd.container_id = Some(container);
        }
        if container.0 != 0 {
            if let Some(inv) = self.tables.inventories.get_mut(container) {
                inv.add_content(id, is_container, place as usize);
            }
        }

        if Some(wielder) == player && wielder.0 != 0 {
            self.set_player_wield_location(id, location, priority);
            if let Some(w) = self.tables.weenies.get_mut(id) {
                w.pwd.wielder_id = Some(wielder);
            }
        } else if Some(old_wielder) == player && old_wielder.0 != 0 {
            self.set_player_wield_location(id, 0, priority);
            if let Some(w) = self.tables.weenies.get_mut(id) {
                w.pwd.wielder_id = Some(ObjectId(0));
            }
        }

        // Authoritative move handling is the only post-login writer for `inventory_mask`,
        // `clothing_priority_mask`, and the named equipment slots. Equip planning and unblock
        // retry consume this state.
        //
        // Two independent `if`s, not an either/or: a wield-to-wield move clears the old slot and
        // fills the new one in the same call. Each half forks on **`_valid_locations & 0x08007FFF`
        // (`CLOTHING_LOC`)**: a wieldable item's own `location` is the single bit that moved, while
        // a *wearable* one clears or sets its whole `_valid_locations` and its `_priority` in
        // the clothing-priority mask — which is what `auto_wear_is_legal` tests.
        if Some(old_wielder) == player && old_location != 0 {
            let (mask, slot_mask) = if valid_locations & crate::inventory::slots::loc::WEARABLE == 0
            {
                (old_location, old_location)
            } else {
                self.clothing_priority_mask &= !priority;
                (valid_locations, valid_locations)
            };
            self.inventory_mask &= !mask;
            self.inv_slots.set_into_location(slot_mask, ObjectId(0));
        }
        if Some(wielder) == player && location != 0 {
            let mask = if valid_locations & crate::inventory::slots::loc::WEARABLE == 0 {
                location
            } else {
                self.clothing_priority_mask |= priority;
                valid_locations
            };
            self.inventory_mask |= mask;
            self.inv_slots.set_into_location(mask, id);
        }

        // The list handler refills and flushes its pending row when the move's **old** or **new**
        // container is this list's parent, or when it finds the moved id already
        // in it. The provisional row is in it, so the authoritative answer replaces it with the
        // real row in one pass and never shows two.
        self.flush_pending_row_for(id, [old_container, container]);

        let mut fire_attributes_changed = false;
        if let Some(w) = self.tables.weenies.get_mut(id) {
            w.pwd.location = Some(location);
            w.determine_position_state();
            if w.waiting {
                w.waiting = false;
                fire_attributes_changed = w.valid;
            }
            if w.marked_for_deletion {
                w.moved_while_marked_for_deletion = true;
            }
        }
        if fire_attributes_changed {
            out.emit(Notice::ItemAttributesChanged {
                object: id,
                kind: 0,
            });
        }

        // The lock is cleared here and in **three** other places, every one under the
        // same `id == locked object id` guard: the response recorder, the stack-size handler,
        // this move-item handler and the attempt-failed handler. The inventory-request module
        // records all four guarded clearers.
        //
        // There is no timeout: if the server never answers, the retail client wedges until an
        // unrelated server move-item notice for that id arrives. **There is no general release**;
        // the wedge is reproduced.
        self.request_lock.clear_if_matches(id);

        // Component tracking updates the moved object itself and, when it is a
        // `TYPE_CONTAINER`, every item inside it. This is the seam an item entering or leaving the
        // player's pack crosses, and it is the reason the `ComponentTracker` is filled at all.
        self.update_spell_components_under(id);

        if notify_ui {
            out.emit(Notice::ItemMoved {
                object: id,
                old_container,
                old_wielder,
                old_location,
                container,
                place,
                wielder,
                location,
            });
        }
    }

    /// Pre-place an unknown item in its container's ordered list until its create message arrives.
    ///
    /// The other half of the client's `0x0022
    /// Item_ServerSaysContainID` arm. When the item named by the reply has no runtime object yet,
    /// the client cannot move it, so it records the id in the **container's** ordered list at the
    /// slot the server named and lets the later `0xF745 Item_CreateObject` find it there. It looks
    /// the container up and, if found, hands it the item id, slot and properties word; the
    /// container returns early without an inventory, and otherwise inserts the id at that slot in
    /// its containers list (properties set) or its items list.
    ///
    /// Both guards are reproduced and both are reachable. The container must be a known object
    /// *and* must have an object inventory, which the client only keeps for a container it is
    /// actively viewing (created by `view_object_contents`). A `0x0022` about a chest the player has not opened is
    /// dropped here by the retail client too.
    ///
    /// Note which list the *props* word chooses. It is the server telling the client something it
    /// cannot yet know, because the thing that would answer — the item's own
    /// [`Weenie::goes_in_containers_list`] — is exactly what has not arrived.
    ///
    /// Returns whether the id was pre-placed, so a caller can count the branch it took rather than
    /// assume it. This preserves evidence that the negative branch was actually evaluated.
    pub fn server_says_contain_id(
        &mut self,
        container: ObjectId,
        item: ObjectId,
        slot: u32,
        container_properties: u32,
    ) -> bool {
        if !self.tables.weenies.contains_key(container) {
            return false;
        }
        let Some(inv) = self.tables.inventories.get_mut(container) else {
            return false;
        };
        inv.add_at_num(item, container_properties != 0, slot as usize);
        true
    }

    /// Update the player's placement record for a wielded object.
    fn set_player_wield_location(&mut self, id: ObjectId, loc: u32, priority: u32) {
        let Some(player) = self.player else { return };
        if let Some(inv) = self.tables.inventories.get_mut(player) {
            inv.set_placement(id, loc, priority);
        }
    }

    /// Apply an attempt-failed reply, the **fourth** clearer of the inventory lock. It is guarded
    /// by the requested object id like the other three clearers.
    ///
    /// It does **not** always clear the lock. Retail's tail compares
    /// the id with the locked object id and skips four stores of zero when they differ — the same
    /// guard as the record-response, set-stack-size and move-item handlers.
    ///
    /// The [`crate::inventory::requests::RequestLock::clear`] below it is nevertheless
    /// unconditional, and that is faithful rather than a shortcut: the **caller** substitutes.
    /// The net-blob dispatch's `0x00A0` arm replaces the message's object id with
    /// the locked object id whenever the lock is held, and that arm
    /// holds the function's **only** call site in the client — no indirect route. So `id` here *is* the
    /// locked object whenever there is one, and the guard can only fail on an already-idle lock
    /// where the four stores would write zero over zero.
    ///
    /// That argument is about the **caller**, so it is only true while the caller substitutes.
    /// A caller that did not substitute would still clear the lock, but would run the handler on
    /// the wrong object — this function's `waiting` clear, its refusal text and both its notices
    /// all name `id`.
    pub fn server_says_attempt_failed(
        &mut self,
        id: ObjectId,
        reason: u32,
        out: &mut dyn NoticeSink,
    ) {
        let object = self.weenie(id);
        let material_name =
            object.and_then(|w| self.material_name(w.pwd.material_type.unwrap_or(0)));
        let text = crate::inventory::requests::attempt_failed_text(
            self.request_lock.pending,
            object,
            material_name,
            reason,
        );
        if let Some(w) = self.tables.weenies.get_mut(id) {
            w.waiting = false;
        }
        // The list handler refills and flushes its pending item when the failed id is either this list's
        // parent container or the item id of any row in it. The
        // provisional row is a row, so a refused move deletes it and the source un-ghosts above.
        self.flush_pending_row_for(id, [id, id]);
        self.request_lock.clear();
        out.emit(Notice::DisplayString {
            channel: crate::inventory::requests::FEEDBACK_CHANNEL,
            text,
        });
        out.emit(Notice::AttemptFailed { object: id, reason });
    }

    /// Apply the stack-size reply, the only message that carries a source stack's new count.
    ///
    /// The retail stack-size update, which is the caller's half, returns false for id 0, for an
    /// unknown object, when no stamper can be set up, or when the stamper (key `0x1000c`) rejects
    /// the sequence. Otherwise it applies the new size and value only when
    /// `amount <= max_stack_size`, and returns true whether or not that test passed.
    ///
    /// Two things there are easy to get wrong and both are reproduced. The maximum test is a plain
    /// `amount <= max_stack_size` with **no exemption for a zero maximum** — an object whose desc
    /// carried no maximum stack size reads 0 and accepts nothing but 0 — and the **sequence is
    /// consumed either way**, so a rejected amount still advances the stamper and the function
    /// still answers `true`. `\[verified\]`
    ///
    /// The accepted update writes stack size, raises an attribute change, writes value, clears
    /// `waiting` whenever it was nonzero, conditionally raises another change for a valid object,
    /// clears the matching request lock, marks movement while pending deletion, and raises the
    /// final attribute change in that order.
    ///
    /// The lock release is load-bearing. A split's
    /// locked object id names the **source** stack; the `0x0022 Item_ServerSaysContainID`
    /// that comes back names the **new** object, so the client's
    /// `id == locked object id` test cannot match. This is one of **four** clearers of
    /// that global: the response recorder, this stack-size handler, move-item handling, and the
    /// attempt-failed handler. Without this clearer, one split can wedge inventory requests for the
    /// rest of the session.
    ///
    /// Retail raises the item-attributes-changed notice **two or three times** (unconditionally at the top,
    /// again inside the `waiting && valid` arm, and unconditionally at the end). One notice is
    /// raised here: every consumer in this build is a per-frame poll keyed on the *state* rather
    /// than on the arrival count, so the repetition has no observable in this rebuild. Stated
    /// rather than silently dropped.
    ///
    /// Returns the retail update's own `bool`: `false` only when the object is unknown or the
    /// stamper rejects the sequence.
    pub fn server_says_set_stack_size(
        &mut self,
        id: ObjectId,
        sequence: u8,
        amount: u32,
        value: u32,
        out: &mut dyn NoticeSink,
    ) -> bool {
        let key = StatKey::new(crate::qualities::StatType::Int, 12);
        let Some(w) = self.tables.weenies.get_mut(id) else {
            return false;
        };
        if !w.setup_stamper().update(key.0, sequence) {
            return false;
        }
        // Only an amount within the maximum is applied — and the `return true` is outside that.
        if amount > u32::from(w.pwd.max_stack_size.unwrap_or(0)) {
            return true;
        }
        #[allow(clippy::cast_possible_truncation)] // clamped against max_stack_size above
        {
            w.pwd.stack_size = Some(amount as u16);
        }
        w.pwd.value = Some(value);
        let was_waiting = w.waiting;
        if was_waiting {
            w.waiting = false;
        }
        if w.marked_for_deletion {
            w.moved_while_marked_for_deletion = true;
        }
        // If `id` is the locked object: zero the locked id, set the pending request to `IR_NONE`, ...
        // — the client's body, inlined here in the client and reached through the
        // same helper in this build.
        self.record_response(id);
        out.emit(Notice::ItemAttributesChanged {
            object: id,
            kind: 0,
        });
        // Component tracking is one of that notice's listeners; it applies the
        // `ITEM_TYPE & TYPE_SPELL_COMPONENTS` gate followed by
        // `update_spell_component` — and `stack_size` is exactly the field the component tracker's
        // `numItems` reads. `apply_stat_update` and `server_says_move_item` were the two writers
        // the tracker had; this is the third, and it is the one a split produces.
        self.update_spell_component(id);
        true
    }

    // -----------------------------------------------------------------------------------------
    // Property updates and quality fan-out.
    // -----------------------------------------------------------------------------------------

    /// One of the 50 `Qualities_*` handlers, all of which share this body.
    ///
    /// Returns `false` when the `PropertySequenceGate` gate rejects the update. **The value is stored only
    /// when the object has a player description**; for every other object the call exists purely to drive
    /// the `PublicWeenieDesc` mirror and the `QualityNotifications` fan-out.
    pub fn apply_stat_update(
        &mut self,
        id: ObjectId,
        key: StatKey,
        value: StatValue,
        sequence: u8,
        out: &mut dyn NoticeSink,
    ) -> bool {
        let player = self.player;
        let request_object = self.request_lock.object;
        let owned_by_player = self.is_owned_by_player(id);
        let Some(w) = self.tables.weenies.get_mut(id) else {
            return false;
        };
        if !w.setup_stamper().update(key.0, sequence) {
            return false;
        }
        if let Some(q) = w.qualities.as_mut() {
            q.set_received(
                key,
                value.clone(),
                self.vital_formulas.as_ref(),
                self.quality_filter.as_ref(),
            );
        }
        // The mirror's guard: skip when this object is the subject of the in-flight request, or is
        // owned by the player.
        let guarded = request_object == Some(id) || owned_by_player;
        let effect = mirror_stat_update(w, key, &value, guarded);
        if effect.icon_changed {
            out.emit(Notice::ItemAttributesChanged {
                object: id,
                kind: 2,
            });
        } else if effect.changed {
            out.emit(Notice::ItemAttributesChanged {
                object: id,
                kind: 0,
            });
        }
        // Component tracking is one of the notice's listeners; it applies the
        // `ITEM_TYPE & TYPE_SPELL_COMPONENTS` gate followed by
        // `update_spell_component`. `StatType::Int` property **12** is `StackSize`, which is exactly
        // the change that has to reach. This and
        // [`Self::server_says_move_item`] are the two writers of the `ComponentTracker`.
        if effect.changed {
            self.update_spell_component(id);
        }
        for n in QualityNotifications::fan_out(id, player, key, false) {
            match n.scope {
                QualityScope::Object(_) => out.emit(Notice::StatUpdated(id, key)),
                // The player-quality subscriber updates the purse even when the vendor window is closed.
                QualityScope::Player => self.player_quality_handlers(key),
                QualityScope::Global => {}
            }
        }
        true
    }

    /// Notify subscribers when a changed quality belongs to the player. Public and player-addressed
    /// update paths share this fan-out because they write the same qualities store. Coin value
    /// (`0x14`) refreshes the vendor purse and displayed total even when the window is closed.
    ///
    /// **`EncumbranceVal` (`0x05`) is registered and not delivered**: its arm in the
    /// quality-changed handler is the alternate-currency recount (the vendor's trade currency is
    /// not `INVALID_DID`), and no vendor in the corpus trades in anything but pyreals. A declared
    /// gap.
    fn player_quality_handlers(&mut self, key: StatKey) {
        if key == StatKey::new(crate::qualities::StatType::Int, crate::vendor::COIN_VALUE) {
            self.update_total_value();
        }
    }

    /// Apply a player-addressed quality update to the same store used by public updates. The private
    /// wire form differs only by substituting the local player id before the shared stamper and
    /// setter. Both forms consume one sequence counter keyed by `property | 0x10000`; keeping two
    /// stores would let each accept an update the other had superseded.
    ///
    /// The value is written only when the player's object already owns a qualities value. A message
    /// after logoff must not recreate it, although its accepted sequence is still consumed.
    ///
    /// Only the two live player-only mirrors are applied on this path:
    ///
    /// Property Int 134's player-killer-status mirror runs here because combat and radar read
    /// those PWD bits immediately. InstanceID 26's `Monarch` mirror runs here because
    /// radar recomputes other blips' allegiance shapes from the player's PWD monarch. A private
    /// form has no later object dispatcher, so that one needs the
    /// mirror here. A public player form also reaches [`Self::apply_stat_update`] afterward, and
    /// equal stamps are accepted, so its existing full mirror remains that form's owner. The other
    /// mirror arms and the `QualityScope::Object` notice remain
    /// [`Self::apply_stat_update`]'s: several require item notices or containment callbacks, which
    /// this HUD-owned player seam cannot deliver. `0x14 CoinValue` has no mirror in any case. The
    /// wider missing player mirror/object fan-out remains a successor rather than being silently
    /// approximated here.
    ///
    /// Returns `None` for retail's `if (!obj) return 0` — no player, no row, or a public form
    /// naming somebody else.
    pub fn apply_player_quality_update(
        &mut self,
        u: &crate::qualities::update::QualityUpdate,
    ) -> Option<crate::qualities::update::Outcome> {
        use crate::qualities::update::Outcome;
        let player = self.player?;
        // The private form carries no subject because it is the player's by construction; a public
        // form that names anybody else is not this handler's.
        if u.subject.is_some_and(|s| s != player) {
            return None;
        }
        let w = self.tables.weenies.get_mut(player)?;
        // Consume the timestamp before the setter and regardless of whether a store exists.
        if !w.setup_stamper().update(u.key.0, u.sequence) {
            return Some(Outcome::Stale);
        }
        // Only an object that has qualities is written -- a **null test that skips the store
        // write**, not an allocation. It is tempting to use `get_or_insert_with` on the grounds
        // that the player's object always has qualities while it exists, but session teardown
        // removes the object table and releases the player description and object row, after
        // which lookup returns nothing to write to. Creating it here would make a `0x02CD` arriving
        // *after* a logoff drive a combat-mode change -- a callback after the player qualities
        // have been torn down.
        //
        // The stamp is consumed either way, which is why this is `Unstorable` and not `Stale`.
        let Some(q) = w.qualities.as_mut() else {
            return Some(Outcome::Unstorable);
        };
        let outcome = if q.set_received(
            u.key,
            u.value.clone(),
            self.vital_formulas.as_ref(),
            self.quality_filter.as_ref(),
        ) {
            Outcome::Applied
        } else {
            Outcome::Unstorable
        };
        if outcome == Outcome::Applied {
            // Each player-addressed handler substitutes the player id and calls the same typed
            // update as its public form. Property 134 updates the player-killer mirror; instance-id
            // property 26 writes `pwd._monarch`, the player input the radar's special
            // all-rows arm needs. Keep this slice narrow: the other mirror arms can require item
            // notices or containment callbacks that this player-only seam does not carry.
            if u.key == StatKey::new(crate::qualities::StatType::Int, 134) {
                if let StatValue::Int(value) = &u.value {
                    w.set_player_killer_status(*value);
                }
            } else if u.subject.is_none()
                && u.key == StatKey::new(crate::qualities::StatType::Iid, 26)
            {
                if let StatValue::Iid(monarch) = &u.value {
                    w.pwd.monarch = Some(*monarch);
                }
            }
            self.player_quality_handlers(u.key);
        }
        Some(outcome)
    }

    /// Apply a public quality-removal event. The handler forwards the wire's own object id here.
    ///
    /// The body is [`Self::apply_stat_update`]'s up to the setter and then diverges twice, and
    /// both divergences are transcribed rather than assumed:
    ///
    /// * **the `PublicWeenieDesc` mirror does not run.** Stat updates refresh that mirror, but none
    ///   of the eight stat-removal paths does. So [`mirror_stat_update`] is
    ///   deliberately *not* called here and a removed `ItemType` leaves the mirrored copy alone.
    /// * **the fan-out uses remove handlers, not change handlers** — the same three scopes in the
    ///   same order: the global set for an id of 0, the player set, then the global set always.
    ///
    /// A vendor quality removal recomputes total value just as an update does because the purse is
    /// pulled from current player qualities.
    ///
    /// All eight public removal forms route here. Fan-out uses the registrar order, because which
    /// panel sees the change first is observable, and the return distinguishes a stale sequence
    /// from a missing object just as the update path does.
    pub fn apply_stat_remove(
        &mut self,
        id: ObjectId,
        key: StatKey,
        sequence: u8,
        out: &mut dyn NoticeSink,
    ) -> bool {
        let player = self.player;
        let Some(w) = self.tables.weenies.get_mut(id) else {
            return false;
        };
        if !w.setup_stamper().update(key.0, sequence) {
            return false;
        }
        // The optional-store result is **ignored** and processing falls through to the registrar, so a
        // property the object was not carrying still fires the handlers and still spends the stamp.
        if let Some(q) = w.qualities.as_mut() {
            q.remove(key);
        }
        for n in QualityNotifications::fan_out(id, player, key, true) {
            match n.scope {
                QualityScope::Object(_) => out.emit(Notice::StatRemoved(id, key)),
                QualityScope::Player => self.player_quality_handlers(key),
                QualityScope::Global => {}
            }
        }
        true
    }

    /// Apply the player-addressed counterpart of [`Self::apply_stat_remove`]. Both routes use one
    /// qualities store and differ only in how they choose the subject. This route does not emit an
    /// object-scope notice or update the public descriptor mirror, matching all removal forms.
    ///
    /// Returns `None` for retail's `if (!obj) return 0` — no player, no row, or a public form
    /// naming somebody else.
    pub fn apply_player_quality_remove(
        &mut self,
        r: &crate::qualities::remove::QualityRemove,
    ) -> Option<crate::qualities::remove::Outcome> {
        use crate::qualities::remove::Outcome;
        let player = self.player?;
        if r.subject.is_some_and(|s| s != player) {
            return None;
        }
        let w = self.tables.weenies.get_mut(player)?;
        if !w.setup_stamper().update(r.key.0, r.sequence) {
            return Some(Outcome::Stale);
        }
        // The qualities check is a **null test that skips the delete**, not an allocation --
        // the same reading `apply_player_quality_update` records, and the same reason a message
        // arriving after a logoff must not resurrect the player description. The stamp is spent either
        // way, which is why this is `Absent` and not `Stale`.
        let removed = w.qualities.as_mut().is_some_and(|q| q.remove(r.key));
        let outcome = if removed {
            Outcome::Removed
        } else {
            Outcome::Absent
        };
        // The client's player set -- fired for both outcomes,
        // because the client's remove never tests the setter's return.
        self.player_quality_handlers(r.key);
        Some(outcome)
    }

    /// The local player's qualities, if the player object exists.
    #[must_use]
    pub fn player_qualities(&self) -> Option<&Qualities> {
        self.weenie(self.player?)?.qualities.as_ref()
    }

    pub fn player_qualities_mut(&mut self) -> Option<&mut Qualities> {
        let p = self.player?;
        self.weenie_mut(p)?.qualities.as_mut()
    }

    /// Store a **copy** of the player description and always fire its notice, even when no player
    /// row is available for the copy.
    pub fn set_player_visual_desc(
        &mut self,
        d: dereth_protocol::types::ObjDesc,
        out: &mut dyn NoticeSink,
    ) {
        self.player_objdesc = Some(d);
        out.emit(Notice::PlayerObjDescChanged);
    }

    /// The local clock this crate hands to the rebasing paths.
    ///
    /// A convenience, not a policy: enchantment and skill times are rebased against the local
    /// monotonic clock represented by [`dereth_primitives::LocalTime`].
    #[must_use]
    pub fn local_time(now: ServerTime) -> LocalTime {
        LocalTime(now.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RecordingSink;
    use dereth_protocol::types::physicsdesc::flags;
    use dereth_protocol::types::{PhysicsDesc, PhysicsTimestamps, PositionWire};

    fn payload(id: u32, seq: u16, name: &str) -> ObjectCreatePayload {
        ObjectCreatePayload {
            id: ObjectId(id),
            objdesc: dereth_protocol::types::ObjDesc::default(),
            physicsdesc: PhysicsDesc {
                bitfield: flags::POSITION,
                position: Some(PositionWire {
                    objcell_id: 0x00A9_0125,
                    ..PositionWire::default()
                }),
                timestamps: PhysicsTimestamps {
                    instance: seq,
                    ..PhysicsTimestamps::default()
                },
                ..PhysicsDesc::default()
            },
            wdesc: PublicWeenieDesc {
                name: name.into(),
                ..PublicWeenieDesc::default()
            },
        }
    }

    /// Both quality-update paths clamp a received current vital to the object's own computed
    /// maximum once the host has installed the vital formulas, and store it raw before then.
    #[test]
    fn received_current_vitals_are_clamped_on_both_update_paths() {
        use crate::qualities::update::{Outcome, QualityUpdate};
        use crate::qualities::StatType;
        use dereth_assets::tables::{Attribute2ndTable, SkillFormula};
        use dereth_protocol::types::qualities::{Attribute, SecondaryAttribute};
        const PLAYER: ObjectId = ObjectId(0x5000_0001);
        const OTHER: ObjectId = ObjectId(0x5000_0002);
        let table = Attribute2ndTable {
            id: dereth_primitives::DataId(0x0E00_0003),
            // MaxHealth = Endurance / 2
            health: SkillFormula {
                w: 0,
                x: 1,
                y: 0,
                z: 2,
                attr1: 2,
                attr2: 0,
            },
            stamina: SkillFormula {
                w: 0,
                x: 1,
                y: 0,
                z: 1,
                attr1: 2,
                attr2: 0,
            },
            mana: SkillFormula {
                w: 0,
                x: 1,
                y: 0,
                z: 1,
                attr1: 6,
                attr2: 0,
            },
        };
        let body = || {
            let mut q = Qualities::new();
            q.set_attribute(
                2,
                Attribute {
                    init_level: 200,
                    level_from_cp: 0,
                    cp_spent: 0,
                },
            );
            q.set_attribute_2nd(
                1,
                SecondaryAttribute {
                    attribute: Attribute {
                        init_level: 200,
                        level_from_cp: 0,
                        cp_spent: 0,
                    },
                    current_level: 250,
                },
            );
            q
        };
        let mut w = World::new();
        w.player = Some(PLAYER);
        for id in [PLAYER, OTHER] {
            let mut o = crate::Weenie::new(id);
            o.qualities = Some(body());
            w.tables.weenies.insert(id, o);
        }
        let health = StatKey::new(StatType::Attribute2nd, 2);
        let tick = |sequence, value| QualityUpdate {
            subject: None,
            sequence,
            key: health,
            value: StatValue::Attribute2ndLevel(value),
        };
        let cur = |w: &World, id| {
            w.weenie(id)
                .unwrap()
                .qualities
                .as_ref()
                .unwrap()
                .attribute_2nd(2)
                .unwrap()
                .current_level
        };
        // No formulas installed yet: stored as received.
        assert_eq!(
            w.apply_player_quality_update(&tick(1, 310)),
            Some(Outcome::Applied)
        );
        assert_eq!(cur(&w, PLAYER), 310);

        // Installed: 310 over a maximum of 100 + 200 = 300 is stored as 300.
        w.install_vital_formulas(table);
        assert_eq!(
            w.apply_player_quality_update(&tick(2, 310)),
            Some(Outcome::Applied)
        );
        assert_eq!(cur(&w, PLAYER), 300);
        let mut out = crate::RecordingSink::default();
        assert!(w.apply_stat_update(
            OTHER,
            health,
            StatValue::Attribute2ndLevel(999),
            1,
            &mut out
        ));
        assert_eq!(cur(&w, OTHER), 300);

        // The formulas outlive a character-session rebuild of the world.
        let mut fresh = World::new();
        fresh.preserve_vital_formulas_from(&mut w);
        assert_eq!(fresh.vital_formulas(), Some(&table));
    }

    /// Equal-instance create messages merge; only a newer instance recreates the object.
    #[test]
    fn a_duplicate_create_is_dropped_but_the_descriptor_still_merges() {
        let mut w = World::new();
        let mut out = RecordingSink::default();
        assert_eq!(
            w.create_or_merge(&payload(5, 1, "Sword"), ServerTime(0.0), &mut out),
            Ok(ObjectId(5))
        );
        assert_eq!(w.weenie(ObjectId(5)).unwrap().pwd.name, "Sword");

        let r = w.create_or_merge(&payload(5, 1, "Sharper Sword"), ServerTime(1.0), &mut out);
        assert_eq!(r, Err(GameError::DuplicateCreate(ObjectId(5))));
        assert_eq!(
            w.weenie(ObjectId(5)).unwrap().pwd.name,
            "Sharper Sword",
            "0xF745 merges into the existing object"
        );
        assert_eq!(w.tables.weenies.len(), 1);
    }

    /// The instance-sequence gate runs **before** every other object-handler action.
    #[test]
    fn a_stale_instance_sequence_is_rejected_before_anything_else() {
        let mut w = World::new();
        let mut out = RecordingSink::default();
        w.create_or_merge(&payload(5, 10, "Sword"), ServerTime(0.0), &mut out)
            .unwrap();
        let r = w.create_or_merge(&payload(5, 9, "Stale"), ServerTime(1.0), &mut out);
        assert_eq!(
            r,
            Err(GameError::StaleInstance {
                id: ObjectId(5),
                got: 9,
                have: 10
            })
        );
        assert_eq!(w.weenie(ObjectId(5)).unwrap().pwd.name, "Sword");
    }

    /// `0xF7DB` always deletes and rebuilds, despite its common "update" label.
    #[test]
    fn recreate_rebuilds_rather_than_merging() {
        let mut w = World::new();
        let mut out = RecordingSink::default();
        w.create_or_merge(&payload(5, 1, "Sword"), ServerTime(0.0), &mut out)
            .unwrap();
        w.weenie_mut(ObjectId(5)).unwrap().selected = true;
        w.recreate(&payload(5, 2, "New"), ServerTime(1.0), &mut out)
            .unwrap();
        assert_eq!(w.weenie(ObjectId(5)).unwrap().pwd.name, "New");
        assert!(
            !w.weenie(ObjectId(5)).unwrap().selected,
            "the object was destroyed and rebuilt, so per-object state is gone"
        );
        assert!(out.0.contains(&Notice::ObjectDeleted(ObjectId(5))));
    }

    /// The queue is popped while `key <= cur_time` and each pop re-checks
    /// `destruction_object_table[id]` within `2e-4`. A re-scheduled object survives its stale
    /// queue entry.
    #[test]
    fn a_destroy_rescheduled_inside_twenty_five_seconds_survives() {
        let mut w = World::new();
        let mut out = RecordingSink::default();
        let mut req = crate::NullRequests;
        w.create_or_merge(&payload(5, 1, "Sword"), ServerTime(0.0), &mut out)
            .unwrap();

        w.schedule_destroy(ObjectId(5), ServerTime(0.0)); // due at 25.0
        w.schedule_destroy(ObjectId(5), ServerTime(10.0)); // due at 35.0; the 25.0 entry is stale

        w.use_time(ServerTime(26.0), &mut out, &mut req);
        assert!(
            w.weenie(ObjectId(5)).is_some(),
            "the stale 25.0 queue entry must not destroy it"
        );

        w.use_time(ServerTime(36.0), &mut out, &mut req);
        assert!(w.weenie(ObjectId(5)).is_none(), "the live 35.0 entry does");
    }

    /// The same shape as the previous test, but the object is *un*-scheduled instead: an item
    /// dropped and re-picked-up inside 25 s never re-downloads.
    #[test]
    fn unscheduling_cancels_a_pending_destruction() {
        let mut w = World::new();
        let mut out = RecordingSink::default();
        let mut req = crate::NullRequests;
        w.create_or_merge(&payload(5, 1, "Sword"), ServerTime(0.0), &mut out)
            .unwrap();
        w.schedule_destroy(ObjectId(5), ServerTime(0.0));
        w.remove_object_to_be_destroyed(ObjectId(5));
        w.use_time(ServerTime(100.0), &mut out, &mut req);
        assert!(w.weenie(ObjectId(5)).is_some());
    }

    /// The 20 s sweep sends a force-description request, the only recovery path for a dangling
    /// reference, and it re-stamps so it fires once per interval, not once per tick.
    #[test]
    fn the_null_table_sweep_re_requests_once_per_twenty_seconds() {
        let mut w = World::new();
        let mut out = RecordingSink::default();
        let mut req = crate::RecordingRequests::default();
        w.get_null_weenie_object(ObjectId(0x1234), ServerTime(0.0));

        w.use_time(ServerTime(1.0), &mut out, &mut req);
        assert!(req.0.is_empty(), "not yet 20 s old");

        w.use_time(ServerTime(21.0), &mut out, &mut req);
        assert_eq!(req.0.len(), 1);
        w.use_time(ServerTime(22.0), &mut out, &mut req);
        assert_eq!(req.0.len(), 1, "the sweep itself is throttled to 20 s");

        // Kept alive past the 25 s destruction, the sweep fires again a full interval later.
        w.schedule_destroy(ObjectId(0x1234), ServerTime(22.0));
        w.use_time(ServerTime(42.0), &mut out, &mut req);
        assert_eq!(req.0.len(), 2);
    }

    /// All three queued-blob paths schedule destruction, so a null object
    /// that is never resolved dies on its own after the 25 s delay.
    #[test]
    fn an_unresolved_null_placeholder_dies_on_its_own() {
        let mut w = World::new();
        let mut out = RecordingSink::default();
        let mut req = crate::NullRequests;
        w.get_null_weenie_object(ObjectId(0x1234), ServerTime(0.0));
        assert!(w.tables.null_weenies.contains_key(ObjectId(0x1234)));
        w.use_time(ServerTime(24.9), &mut out, &mut req);
        assert!(w.tables.null_weenies.contains_key(ObjectId(0x1234)));
        w.use_time(ServerTime(25.1), &mut out, &mut req);
        assert!(!w.tables.null_weenies.contains_key(ObjectId(0x1234)));
    }

    /// Blobs park on the placeholder and are replayed in receive-sequence order once the object
    /// becomes real.
    #[test]
    fn blobs_park_on_a_placeholder_and_replay_in_sequence_order() {
        use crate::objects::QueuedBlob;
        let mut w = World::new();
        let id = ObjectId(0x1234);
        assert!(w.queue_blob_for_weenie_object(
            id,
            QueuedBlob {
                sequence: Some(7),
                payload: vec![7]
            },
            ServerTime(0.0)
        ));
        assert!(w.queue_blob_for_weenie_object(
            id,
            QueuedBlob {
                sequence: Some(3),
                payload: vec![3]
            },
            ServerTime(0.0)
        ));
        let blobs = w.take_queued_blobs(id);
        assert_eq!(
            blobs.iter().map(|b| b.sequence).collect::<Vec<_>>(),
            vec![Some(3), Some(7)]
        );

        // Once the object is real, nothing parks any more.
        let mut out = RecordingSink::default();
        w.create_or_merge(&payload(0x1234, 1, "Sword"), ServerTime(0.0), &mut out)
            .unwrap();
        assert!(!w.queue_blob_for_weenie_object(
            id,
            QueuedBlob {
                sequence: Some(1),
                payload: vec![]
            },
            ServerTime(0.0)
        ));
    }

    /// The visible list holds uncelled and static objects out.
    #[test]
    fn the_visible_list_holds_uncelled_and_static_objects_out() {
        let mut w = World::new();
        let mut out = RecordingSink::default();
        w.create_or_merge(&payload(5, 1, "In a cell"), ServerTime(0.0), &mut out)
            .unwrap();
        w.create_or_merge(&payload(6, 1, "Also"), ServerTime(0.0), &mut out)
            .unwrap();
        w.tables.physics.get_mut(ObjectId(6)).unwrap().state = crate::objects::STATIC_PS;
        w.create_or_merge(&payload(7, 1, "No cell"), ServerTime(0.0), &mut out)
            .unwrap();
        w.tables.physics.get_mut(ObjectId(7)).unwrap().cell = None;

        w.update_visible_object_list();
        assert!(w.tables.visible.contains(&ObjectId(5)));
        assert!(!w.tables.visible.contains(&ObjectId(6)));
        assert!(!w.tables.visible.contains(&ObjectId(7)));
    }

    /// Contents die with their container only when they
    /// still name it as their container **and** their physics object has no cell.
    #[test]
    fn the_contents_cascade_is_conditional() {
        let mut w = World::new();
        let mut out = RecordingSink::default();
        let mut req = crate::NullRequests;
        for id in [10u32, 11, 12] {
            w.create_or_merge(&payload(id, 1, "item"), ServerTime(0.0), &mut out)
                .unwrap();
        }
        w.create_or_merge(&payload(1, 1, "chest"), ServerTime(0.0), &mut out)
            .unwrap();
        w.view_object_contents(
            ObjectId(1),
            &[
                ContentProfile {
                    iid: ObjectId(10),
                    container_properties: 0,
                },
                ContentProfile {
                    iid: ObjectId(11),
                    container_properties: 0,
                },
                ContentProfile {
                    iid: ObjectId(12),
                    container_properties: 0,
                },
            ],
            &mut out,
        );
        // 10 still names the chest and has no cell -> cascades.
        w.weenie_mut(ObjectId(10)).unwrap().pwd.container_id = Some(ObjectId(1));
        w.tables.physics.get_mut(ObjectId(10)).unwrap().cell = None;
        // 11 has been re-homed by the server -> must NOT cascade.
        w.weenie_mut(ObjectId(11)).unwrap().pwd.container_id = Some(ObjectId(99));
        w.tables.physics.get_mut(ObjectId(11)).unwrap().cell = None;
        // 12 names the chest but is in a cell -> must NOT cascade.
        w.weenie_mut(ObjectId(12)).unwrap().pwd.container_id = Some(ObjectId(1));

        w.delete_object(ObjectId(1), ServerTime(0.0), &mut out);
        assert!(w.tables.doomed.contains_key(ObjectId(10)));
        assert!(!w.tables.doomed.contains_key(ObjectId(11)));
        assert!(!w.tables.doomed.contains_key(ObjectId(12)));
        w.use_time(ServerTime(1000.0), &mut out, &mut req);
        assert!(w.weenie(ObjectId(10)).is_none());
        assert!(w.weenie(ObjectId(11)).is_some());
    }

    /// Selecting the current id is a no-op unless forced, and `prev_selected_valid`
    /// only tracks non-null selections.
    #[test]
    fn selection_tracks_previous_and_previous_valid() {
        let mut w = World::new();
        let mut out = RecordingSink::default();
        w.set_selected_object(Some(ObjectId(5)), false, &mut out);
        w.set_selected_object(Some(ObjectId(5)), false, &mut out);
        assert_eq!(
            out.0.len(),
            1,
            "re-selecting the same object is a no-op unless forced"
        );
        w.set_selected_object(Some(ObjectId(6)), false, &mut out);
        assert_eq!(w.prev_selected, Some(ObjectId(5)));
        assert_eq!(w.prev_selected_valid, Some(ObjectId(5)));
        w.set_selected_object(None, false, &mut out);
        assert_eq!(w.prev_selected, Some(ObjectId(6)));
        assert_eq!(w.prev_selected_valid, Some(ObjectId(6)));
        w.set_selected_object(Some(ObjectId(7)), false, &mut out);
        assert_eq!(
            w.prev_selected, None,
            "the previous selection does record the null"
        );
        assert_eq!(
            w.prev_selected_valid,
            Some(ObjectId(6)),
            "the previous valid selection does not"
        );
    }

    /// A quality value is stored **only** when the object has a player qualities store.
    #[test]
    fn a_non_player_update_mirrors_but_does_not_store() {
        let mut w = World::new();
        let mut out = RecordingSink::default();
        w.create_or_merge(&payload(5, 1, "Sword"), ServerTime(0.0), &mut out)
            .unwrap();
        let key = StatKey::new(crate::qualities::StatType::Int, 19); // Value
        assert!(w.apply_stat_update(ObjectId(5), key, StatValue::Int(250), 1, &mut out));
        assert!(w.weenie(ObjectId(5)).unwrap().qualities.is_none());
        assert_eq!(
            w.weenie(ObjectId(5)).unwrap().pwd.value,
            Some(250),
            "mirrored into the PWD"
        );

        // The stamper gate rejects a stale sequence.
        assert!(!w.apply_stat_update(ObjectId(5), key, StatValue::Int(1), 0, &mut out));
        assert_eq!(w.weenie(ObjectId(5)).unwrap().pwd.value, Some(250));
    }

    #[test]
    fn a_player_update_is_stored_in_the_player_desc() {
        let mut w = World::new();
        let mut out = RecordingSink::default();
        w.set_player(ObjectId(0x5000_0001));
        w.create_or_merge(&payload(0x5000_0001, 1, "Lark"), ServerTime(0.0), &mut out)
            .unwrap();
        let key = StatKey::new(crate::qualities::StatType::Int, 5); // EncumbranceVal
        assert!(w.apply_stat_update(
            ObjectId(0x5000_0001),
            key,
            StatValue::Int(1234),
            1,
            &mut out
        ));
        assert_eq!(w.player_qualities().unwrap().inq_int(5), 1234);
    }

    /// Viewing contents splits rows on `container_properties`.
    #[test]
    fn viewing_contents_splits_items_from_side_packs() {
        let mut w = World::new();
        let mut out = RecordingSink::default();
        w.view_object_contents(
            ObjectId(1),
            &[
                ContentProfile {
                    iid: ObjectId(10),
                    container_properties: 0,
                },
                ContentProfile {
                    iid: ObjectId(11),
                    container_properties: 1,
                },
                ContentProfile {
                    iid: ObjectId(12),
                    container_properties: 0,
                },
            ],
            &mut out,
        );
        let inv = w.inventory(ObjectId(1)).unwrap();
        assert_eq!(inv.items, vec![ObjectId(10), ObjectId(12)]);
        assert_eq!(inv.containers, vec![ObjectId(11)]);
        w.stop_viewing_object_contents(ObjectId(1), &mut out);
        assert!(w.inventory(ObjectId(1)).is_none());
    }
}
