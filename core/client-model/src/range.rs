//! Object-range checks that close panels when the player walks away.
//!
//! This is the object's use radius's reader: without it, vendors, corpses, secure trades and
//! selections would survive the player walking to the other end of the landblock.
//!
//! # The mechanism
//!
//! One list records the handler, object, range in metres, geometry flags, poll interval,
//! timeout and deletion flag for each watch. Registration sets the next poll to
//! `now + interval` and the timeout deadline to `now + timeout`; timeout zero disables it.
//! The driver runs at the end of the player-system update. Per node, once per frame:
//!
//! ```text
//! if (queued_for_deletion) { remove; continue; }           // lazy, on the NEXT pass
//! if (next_update < cur_time) {
//!     next_update = cur_time + time_interval;
//!     if (!objects_in_range(object, player, range, use_radii, ignore_z_delta)) {
//!         the handler's range-exit edge (object);
//!         queued_for_deletion = true;
//!     } else if (time_out > 0.0 && execute_at_time < cur_time) {
//!         the handler's range-timeout edge (object);
//!         queued_for_deletion = true;
//!     }
//! }
//! ```
//!
//! So **a registration is one-shot**: whichever edge fires also retires the entry. The one
//! registrant that keeps watching is the selection, and it does it by *re-registering* from inside
//! its own exit handler.
//!
//! The geometry flags select the distance calculation. Ignoring Z selects horizontal point distance and
//! bypasses the radii flag. Otherwise the distance is between points when radii are disabled,
//! or between cylinders when enabled. Cylinder radii and heights are the setup dimensions
//! multiplied by each object's scale.
//!
//! The comparison makes in-range `distance <= range`.
//! Equality is inside; an unordered result such as NaN is outside.
//!
//! **The missing-object arm matters and is transcribed:** an object with no physics body is *out of
//! range*, so a container whose cell has been released closes the panel exactly as walking away
//! does.
//!
//! # The six registrants
//!
//! Seven registration call sites exist; six are distinct registrants and the seventh is
//! the selection re-arming itself.
//!
//! | # | registrant | object | range | `use_radii` | `ignore_z_delta` | interval | timeout |
//! |---|---|---|---|---|---|---|---|
//! | 1 | book panel | the book | object's use radius | 1 | 0 | 1.0 | 0.0 |
//! | 2 | vendor panel | the vendor | object's use radius | 1 | 0 | 1.0 | 0.0 |
//! | 3 | housing panel | the slumlord | **9.0** | 1 | 0 | 1.0 | 0.0 |
//! | 4 | secure trade | the partner | **5.0** | 1 | 0 | 1.0 | 0.0 |
//! | 5 | ground-container panel | the container | object's use radius | 1 | 0 | 1.0 | 0.0 |
//! | 6 | selection | selected object | radar radius | 1 | **1** | 1.0 | 0.0 |
//!
//! These arguments were verified for each registration. The interval
//! is a double 1.0; the slumlord's radius comes from a float 9.0 and trade's from a double 5.0.
//!
//! **Every timeout is 0.0**, and `calculate_object_range_checks` gates the timeout arm on
//! `time_out > 0.0`, so the timeout edge is unreachable from any shipped registration. It is
//! modelled (`RangeEdge::Timeout`) because the driver's arm is real, and pinned as unreachable.
//!
//! **Number 6's range is the radar's:** 75.0 outdoors and 25.0 indoors.
//! The selection watch and the radar cut-off use the same number,
//! selected by the caller's indoor/outdoor state. It is passed in rather than recomputed
//! here, so this crate and `dereth_ui_screens::mapradar::radar::radar_range` cannot disagree.
//!
//! # The leave edges
//!
//! Four of the six hide a panel: the exit edge compares `id` against the panel's
//! own stored id and, on a match, hides the panel.
//!
//! * Vendor exit hides the panel, unregisters its watch, resets the shop and closes the vendor.
//! * Container exit hides the panel. Its initialized hidden edge first sends a use request for
//!   the ground container, then unregisters and empties its lists. That use requests closure
//!   of an already-open container; the server's stop-viewing response clears the open-object state.
//! * Secure trade closes negotiations and resets the trade state directly.
//! * Selection exit first checks that the id still matches the selection. The drawn-in-view
//!   latch then decides: true re-registers at the current indoor/outdoor radar radius; false
//!   clears the selection. A stale id does neither.
//!
//! Book and housing panels also wire their registrations and leave edges through this list.

use dereth_primitives::ObjectId;

/// Outdoor radar radius, shared with selection range watches.
pub const RADAR_RADIUS_OUTDOORS: f32 = 75.0;
/// Indoor radar radius, shared with selection range watches.
pub const RADAR_RADIUS_INDOORS: f32 = 25.0;

/// Fixed range used when a house profile registers its slumlord watch.
pub const SLUMLORD_RANGE: f64 = 9.0;
/// Fixed range used when secure trade registers its partner watch.
pub const SECURE_TRADE_RANGE: f64 = 5.0;
/// Every one of the six registrations passes this `double` as the poll interval.
pub const POLL_INTERVAL: f64 = 1.0;

/// One range-watch owner.
///
/// The client keys the list on the *pointer*; there is exactly one instance of each of these
/// windows and one player system, so the owner kind is the identity here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RangeHandler {
    /// Book panel.
    Book,
    /// Vendor panel.
    Vendor,
    /// Housing panel.
    Slumlord,
    /// Secure-trade panel.
    SecureTrade,
    /// Ground-container panel.
    ExternalContainer,
    /// Selection, the only owner that re-arms on a range exit.
    Selection,
}

impl RangeHandler {
    /// Whether this build wires this handler's registration and range-exit consumer.
    ///
    /// All six native handler owners now have a production consumer.
    #[must_use]
    pub fn has_range_exit_consumer_in_this_build(self) -> bool {
        true
    }
}

/// `ObjectRangeInfo`, the list node's payload.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObjectRangeInfo {
    pub handler: RangeHandler,
    pub object: ObjectId,
    pub range: f64,
    pub use_radii: bool,
    pub ignore_z_delta: bool,
    pub time_interval: f64,
    pub time_out: f64,
    pub execute_at_time: f64,
    pub next_update: f64,
    pub queued_for_deletion: bool,
}

/// Which handler edge the driver dispatched to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeEdge {
    /// The range-exit edge.
    Exit,
    /// The range-timeout edge.
    ///
    /// **Unreachable from any shipped registration**: the arm is gated on `time_out > 0.0` and all
    /// six registrants pass `0.0`. Modelled because the driver's branch is real.
    Timeout,
}

/// Range watches maintained by the player system.
///
/// Each registration is inserted at the **head** of the list, and the
/// driver walks head to tail, so traversal is in reverse registration order. That is reproduced
/// because the walk order decides which handler sees a given frame first when two fire at once.
#[derive(Debug, Default, Clone)]
pub struct ObjectRangeCheckList {
    entries: Vec<ObjectRangeInfo>,
}

impl ObjectRangeCheckList {
    /// Register a watch for one owner and object.
    ///
    /// The whole body: mark every existing node with the same `(handler, object)` for deletion,
    /// then insert a fresh one at the head. It does **not** remove the old node itself — the
    /// driver's next pass does.
    #[allow(clippy::too_many_arguments)]
    pub fn register(
        &mut self,
        handler: RangeHandler,
        object: ObjectId,
        range: f64,
        use_radii: bool,
        ignore_z_delta: bool,
        time_interval: f64,
        time_out: f64,
        now: f64,
    ) {
        for e in &mut self.entries {
            if e.handler == handler && e.object == object {
                e.queued_for_deletion = true;
            }
        }
        self.entries.insert(
            0,
            ObjectRangeInfo {
                handler,
                object,
                range,
                use_radii,
                ignore_z_delta,
                time_interval,
                time_out,
                execute_at_time: now + time_out,
                next_update: now + time_interval,
                queued_for_deletion: false,
            },
        );
    }

    /// Mark watches matching `(handler, object)` for deletion.
    pub fn unregister(&mut self, handler: RangeHandler, object: ObjectId) {
        for e in &mut self.entries {
            if e.handler == handler && e.object == object {
                e.queued_for_deletion = true;
            }
        }
    }

    /// Mark every node of one handler, whatever object it watches.
    pub fn unregister_all(&mut self, handler: RangeHandler) {
        for e in &mut self.entries {
            if e.handler == handler {
                e.queued_for_deletion = true;
            }
        }
    }

    /// Every node, head first — including the ones queued for deletion, exactly as the client's
    /// list holds them until the driver's next pass reaches them.
    pub fn iter(&self) -> impl Iterator<Item = &ObjectRangeInfo> {
        self.entries.iter()
    }

    /// The nodes that are still armed: not queued for deletion.
    pub fn live(&self) -> impl Iterator<Item = &ObjectRangeInfo> {
        self.entries.iter().filter(|e| !e.queued_for_deletion)
    }

    /// Whether `(handler, object)` has a live registration.
    #[must_use]
    pub fn is_watching(&self, handler: RangeHandler, object: ObjectId) -> bool {
        self.live()
            .any(|e| e.handler == handler && e.object == object)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Drop the whole watch list.
    pub fn flush(&mut self) {
        self.entries.clear();
    }

    /// Walk the range list, with handler dispatch lifted
    /// out so the borrow of the list ends before any handler runs.
    ///
    /// This is not a liberty taken with the order: the client's handlers can only *add* nodes, and
    /// registering a handler ([`Self::register`]) adds at the head — behind a cursor that has already passed it —
    /// so a node created by a handler is never visited on the pass that created it. Doing the
    /// dispatch after the walk has exactly that property.
    pub(crate) fn step(
        &mut self,
        now: f64,
        in_range: &mut dyn FnMut(&ObjectRangeInfo) -> bool,
    ) -> Vec<(RangeHandler, ObjectId, RangeEdge)> {
        let mut fired = Vec::new();
        let mut i = 0;
        while i < self.entries.len() {
            if self.entries[i].queued_for_deletion {
                self.entries.remove(i);
                continue;
            }
            if self.entries[i].next_update < now {
                let interval = self.entries[i].time_interval;
                self.entries[i].next_update = now + interval;
                let e = self.entries[i];
                if in_range(&e) {
                    if e.time_out > 0.0 && e.execute_at_time < now {
                        fired.push((e.handler, e.object, RangeEdge::Timeout));
                        self.entries[i].queued_for_deletion = true;
                    }
                } else {
                    fired.push((e.handler, e.object, RangeEdge::Exit));
                    self.entries[i].queued_for_deletion = true;
                }
            }
            i += 1;
        }
        fired
    }
}

/// The geometry half of the object-range predicate.
///
/// The distance is asked of the caller because it is the only side that holds physics bodies:
/// `dereth-client-model` has no positions, and point, horizontal and cylinder distances are
/// already implemented in `dereth_physics::math`. This trait supplies the distance while
/// the *comparison* stays here.
///
/// `None` means "one of the two objects has no physics body", which counts as **out of range**.
pub trait ObjectRangeGeometry {
    /// Ignoring Z uses horizontal point distance, bypassing `use_radii`. Otherwise,
    /// disabling radii uses point distance; enabling them uses cylinder distance with
    /// both bodies' scaled radius and height.
    fn distance(
        &self,
        object: ObjectId,
        player: ObjectId,
        use_radii: bool,
        ignore_z_delta: bool,
    ) -> Option<f32>;
}

/// The comparison half of [`objects_in_range`]: `distance <= range`, with a missing body counting as
/// out of range.
#[must_use]
pub fn objects_in_range(
    geometry: &dyn ObjectRangeGeometry,
    player: Option<ObjectId>,
    e: &ObjectRangeInfo,
) -> bool {
    let Some(player) = player else { return false };
    objects_in_range_distance(
        geometry.distance(e.object, player, e.use_radii, e.ignore_z_delta),
        e.range,
    )
}

/// The **comparison alone**, for the one caller that resolves its own distance.
/// Factored out of [`objects_in_range`] rather than written a second time.
///
/// Ranged chat uses point distance with both geometry flags false and the range received in
/// the `0x02BC` message, and the client half of that seam already has the offset it needs in the
/// radar snapshot (`Hud::speaker_distance`), so it does not go through
/// [`ObjectRangeGeometry`]'s two-id lookup.
///
/// `None` represents a missing body and is **out of range**. The separate hearing predicate
/// permits a missing body, so these policies must not be interchanged.
///
/// Retail treats `d < range` **or** `d == range` as in range — **inclusive** — and an unordered
/// (NaN) comparison as out. `d <= range` reproduces all three arms exactly (`NaN <= x` is false in Rust too) where
/// `!(d > range)` would need a separate NaN clause to mean the same thing.
#[must_use]
pub fn objects_in_range_distance(distance: Option<f32>, range: f64) -> bool {
    match distance {
        Some(d) => f64::from(d) <= range,
        None => false,
    }
}

/// What one range-check update did, so a caller can assert the
/// denominator rather than the effect alone.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RangeCheckStats {
    /// Nodes polled this call (`next_update < cur_time`).
    pub polled: u32,
    /// Range-exit dispatches.
    pub exits: u32,
    /// Range-timeout dispatches — structurally 0 for every shipped registration.
    pub timeouts: u32,
    /// Exits whose handler has no wired consumer in this build
    /// ([`RangeHandler::has_range_exit_consumer_in_this_build`]).
    pub exits_without_a_window: u32,
    /// Selection exits that re-register because the selected object has been drawn in view.
    pub selection_rearms: u32,
    /// The other arm of the same test: the selection is cleared rather than re-armed.
    ///
    /// Counted beside `selection_rearms` rather than derived from it, because the two are not
    /// complements: a `Selection` exit whose id does not match the selected id takes neither arm.
    /// A stale registration doing nothing must not read as a drop. `rearms + clears <= exits`,
    /// and the shortfall is the
    /// stale ones.
    ///
    /// Without the draw-time setter for the selected-object-in-view latch, every selection range
    /// exit would take the clearing arm; with it, a driven run reads 0 of 6 here, with the six
    /// re-arms in [`Self::selection_rearms`] instead. It stays
    /// counted rather than asserted away because a selection whose object was never drawn — one
    /// picked out of a panel and walked away from before a frame carried it — still clears, in
    /// this build and in retail alike.
    pub selection_clears: u32,
}

/// What a range exit did with a `Selection` registration.
///
/// Three outcomes and not two: the handler's own identity test makes
/// a stale registration a no-op, and folding that into "did not re-arm" would count it as a
/// dropped selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionExitArm {
    /// Not the selection handler, or the id it was given is not the selected id.
    Untouched,
    /// The drawn-in-view test was true — the handler registers again.
    Rearmed,
    /// The drawn-in-view test was false — the selection is cleared (`set_selected_object(0, 0)`).
    Cleared,
}

impl crate::world::World {
    /// Check object ranges at the end of the player-system update.
    ///
    /// `radar_radius` is passed in rather than recomputed, so
    /// that this and `dereth_ui_screens::mapradar::radar::radar_range` cannot disagree about the one
    /// pair of constants they both read.
    ///
    /// **The selection registration is reconciled here rather than on the notice.** The original
    /// client synchronously broadcasts selection to the player system. This crate raises
    /// [`crate::Notice::SelectionChanged`] instead, and
    /// nothing on the far side of that seam owns the range list, so the handler is driven from an
    /// edge detector at the head of the driver. The cost is that a registration lands up to one
    /// frame late against a **one second** poll interval; the gate and the radius are otherwise the
    /// client's.
    pub fn calculate_object_range_checks(
        &mut self,
        now: dereth_primitives::ServerTime,
        geometry: &dyn ObjectRangeGeometry,
        radar_radius: f32,
        out: &mut dyn crate::NoticeSink,
        req: &mut dyn crate::RequestSink,
    ) -> RangeCheckStats {
        let mut stats = RangeCheckStats::default();
        // The second half of this condition:
        //
        // The original selected-item broadcast is outside the identity-change guard.
        // It runs on every call that gets past the `force` early return, including a
        // forced re-selection of the **unchanged** id, where no `SelectionChanged` is raised at
        // all. An edge detector alone cannot see that call: there is no edge.
        //
        // `selection_broadcast_pending` records that call, deferred here for the same
        // reason as the edge — `set_selected_object` is handed neither the radar radius nor a
        // clock, and the selected-item handler needs both. Taken before the `||` so a changed
        // selection consumes it rather than leaving it set for a spurious second run.
        let broadcast = std::mem::take(&mut self.selection_broadcast_pending);
        if self.selection_watch != self.selected || broadcast {
            let previous = self.selection_watch;
            self.selection_watch = self.selected;
            self.on_set_selected_item(previous, self.selected, radar_radius, now);
        }
        let player = self.player;
        let mut list = std::mem::take(&mut self.object_range_checks);
        let fired = list.step(now.0, &mut |e| {
            stats.polled += 1;
            objects_in_range(geometry, player, e)
        });
        // Put the list back **before** dispatching, so a handler that registers (the selection's
        // re-arm) inserts into the live list exactly as the client's does.
        self.object_range_checks = list;
        for (handler, object, edge) in fired {
            match edge {
                RangeEdge::Exit => {
                    stats.exits += 1;
                    if !handler.has_range_exit_consumer_in_this_build() {
                        stats.exits_without_a_window += 1;
                    }
                    match self.on_object_range_exit(handler, object, radar_radius, now, out, req) {
                        SelectionExitArm::Rearmed => stats.selection_rearms += 1,
                        SelectionExitArm::Cleared => stats.selection_clears += 1,
                        SelectionExitArm::Untouched => {}
                    }
                }
                // Unreachable from any shipped registration; counted rather than asserted away.
                RangeEdge::Timeout => stats.timeouts += 1,
            }
        }
        stats
    }

    /// Dispatch the six range-exit handlers. Return which arm of
    /// the selected-object-in-view test ran — see
    /// [`SelectionExitArm`], and note that "not the selection handler at all" and "the selection
    /// was dropped" are different answers rather than two spellings of "did not re-arm".
    ///
    /// Each begins with the client's own identity test — the handler compares the id it was given
    /// against the id *it* is holding, and does nothing when they differ, which is what makes a
    /// stale registration harmless.
    fn on_object_range_exit(
        &mut self,
        handler: RangeHandler,
        object: ObjectId,
        radar_radius: f32,
        now: dereth_primitives::ServerTime,
        out: &mut dyn crate::NoticeSink,
        req: &mut dyn crate::RequestSink,
    ) -> SelectionExitArm {
        match handler {
            // A matching book id hides the panel. Its hidden edge commits the current page, clears the
            // live book, and unregisters every watch owned by this handler.
            RangeHandler::Book => {
                if self
                    .book
                    .open
                    .as_ref()
                    .is_some_and(|book| book.book_id == object)
                {
                    out.emit(crate::Notice::CloseBook(object));
                }
            }
            // A matching owner id hides the housing panel. Its hidden edge unregisters every watch owned
            // by this handler; the fired node is already retired, and the UI subscriber performs
            // that same unregister-all for any sibling/stale registration.
            RangeHandler::Slumlord => {
                if self.slumlord.as_ref().is_some_and(|(id, _)| *id == object) {
                    out.emit(crate::Notice::CloseSlumlord(object));
                }
            }
            // A matching vendor id hides the panel, unregisters its watches, resets the shop
            // and closes the vendor. The existing close path performs those actions.
            RangeHandler::Vendor => {
                if self.shop.vendor_id == Some(object) {
                    self.object_range_checks
                        .unregister(RangeHandler::Vendor, object);
                    self.close_vendor(out);
                }
            }
            // A matching ground-container id hides the panel. The hidden edge sends a use
            // request, unregisters the watch, empties the lists and clears the panel's
            // ground-object id and parent container.
            //
            // The use request is the close: ACE's `Container.ActOnUse` closes a container that is
            // already open by this viewer, and the server's answer (`0x0052
            // Item_StopViewingObjectContents`) is what clears the open-container state —
            // which the client does **not** clear here either.
            //
            // The panel's own two acts — hiding and emptying its lists — reach this build as
            // `Notice::SetGroundObject(0)`, which is the notice a ground panel closes on.
            RangeHandler::ExternalContainer => {
                if self.ground_object == Some(object) {
                    self.use_object(
                        req,
                        out,
                        object,
                        crate::inventory::SplitState::default(),
                        now,
                    );
                    self.object_range_checks
                        .unregister(RangeHandler::ExternalContainer, object);
                    out.emit(crate::Notice::SetGroundObject(ObjectId(0)));
                }
            }
            // A matching trade partner closes negotiations and resets trade state
            // directly, without going through the window's visibility handler.
            RangeHandler::SecureTrade => {
                if self.trade.partner == object && object.0 != 0 {
                    self.close_trade_negotiations(now, out, req);
                }
            }
            // A stale selected id is a no-op. Otherwise the drawn-in-view flag decides
            // whether to clear selection or register another one-second watch with
            // both geometry flags set and no timeout.
            //
            // Note the re-arm re-evaluates the radius, so walking from outdoors into a dungeon
            // tightens the watch from 75 m to 25 m on the next edge.
            RangeHandler::Selection => {
                if self.selected != Some(object) {
                    return SelectionExitArm::Untouched;
                }
                if self.selected_object_in_view {
                    self.object_range_checks.register(
                        RangeHandler::Selection,
                        object,
                        f64::from(radar_radius),
                        true,
                        true,
                        POLL_INTERVAL,
                        0.0,
                        now.0,
                    );
                    return SelectionExitArm::Rearmed;
                }
                self.set_selected_object(None, false, out);
                // The edge detector must not immediately re-run the handler for the change this
                // very call made: the client's clear of the selection does raise the
                // selection-changed notice, and the selected-item handler then finds no object
                // for id 0 and registers nothing.
                //
                // **This line is defensive and is provably a no-op today.** Replacing it with
                // `self.selection_watch = self.selected` is the *same value*, because the call
                // above just set `selected` to `None`; and deleting it outright leaves the
                // detector to fire `on_set_selected_item(Some(object), None)` on the next tick,
                // which unregisters a node `step` has already retired and registers nothing. Both
                // routes end in the same state, so no test can distinguish them.
                self.selection_watch = None;
                return SelectionExitArm::Cleared;
            }
        }
        SelectionExitArm::Untouched
    }

    /// Update the selection's watch, registrant #6.
    ///
    /// First unregister the previous selection. If the current object is absent, return
    /// without updating the view-cone target. Otherwise, conditionally register a watch,
    /// then update that target even when ownership made the object exempt from watching.
    ///
    /// So the selection watch is armed only for something **out in the world**: an item in your
    /// pack, in the open corpse, in the vendor's stock or on the trade partner is exempt, because
    /// walking away from those is not walking away from the item.
    ///
    /// `ignore_z_delta` is `1` here and `0` at all five window registrants, so this is the only one
    /// measured with horizontal point distance — you keep a target you are directly above.
    pub fn on_set_selected_item(
        &mut self,
        previous: Option<ObjectId>,
        current: Option<ObjectId>,
        radar_radius: f32,
        now: dereth_primitives::ServerTime,
    ) {
        if let Some(p) = previous.filter(|p| p.0 != 0) {
            self.object_range_checks
                .unregister(RangeHandler::Selection, p);
        }
        if self.register_selection_range_check(current, radar_radius, now) {
            // The common tail's reachability is the client's: a missing object
            // returns before this store, while an owned object still reaches it.
            // Clearing selection therefore leaves the previous view-cone target id.
            // That stale id is harmless because range exit checks the selected id before
            // consulting the drawn-in-view latch.
            self.viewcone_check_object_id = current;
        }
    }

    /// Everything the selected-item notice does **between** its unregister loop and common tail,
    /// including **whether that tail was reached**.
    ///
    /// Split out so the client's two ways of leaving that stretch stay distinct:
    /// the trade-partner ownership arm returns `true`, and a missing current object
    /// returns `false`. Folding them together would
    /// write the tail down a path the client does not take.
    fn register_selection_range_check(
        &mut self,
        current: Option<ObjectId>,
        radar_radius: f32,
        now: dereth_primitives::ServerTime,
    ) -> bool {
        let Some(id) = current.filter(|c| c.0 != 0) else {
            return false;
        };
        if self.weenie(id).is_none() {
            return false;
        }
        if let Some(partner) = Some(self.trade.partner).filter(|p| p.0 != 0) {
            if self.is_owned_by_object(id, partner) {
                return true;
            }
        }
        let mut arm = !self.is_owned_by_player(id);
        if arm {
            if let Some(g) = self.ground_object.filter(|g| g.0 != 0) {
                arm = !self.is_owned_by_object(id, g);
            }
        }
        if arm {
            if let Some(v) = self.shop.vendor_id.filter(|v| v.0 != 0) {
                arm = !self.is_owned_by_object(id, v);
            }
        }
        if arm {
            self.object_range_checks.register(
                RangeHandler::Selection,
                id,
                f64::from(radar_radius),
                true,
                true,
                POLL_INTERVAL,
                0.0,
                now.0,
            );
        }
        true
    }

    /// Clear the drawn-in-view latch when a click starts a pick.
    ///
    /// This is the *only* clear in the client, and it sits **inside** the viewport-accepted branch
    /// (two unsigned coordinate comparisons), so a click outside the 3D viewport does not clear it.
    /// The matching setter is on the draw path in `dereth_client`.
    pub fn find_object(&mut self) {
        self.selected_object_in_view = false;
    }

    /// Register the vendor watch when opening its panel — registrant #2.
    ///
    /// The vendor's use radius is the range; a missing vendor object skips registration.
    /// An omitted radius on the wire still registers `0.0`: the original member is initialized
    /// to zero and read without testing the wire-presence bit.
    /// Registration does not itself close the panel; the normal first poll decides whether
    /// zero distance holds.
    pub fn register_vendor_range_check(
        &mut self,
        vendor: ObjectId,
        now: dereth_primitives::ServerTime,
    ) -> bool {
        self.register_use_radius_check(RangeHandler::Vendor, vendor, now)
    }

    /// Register the book watch when opening its panel — registrant #1.
    ///
    /// A carried book is deliberately not watched. An unowned book is watched at its own
    /// use radius; a missing weenie leaves the visible book unmonitored. An omitted wire field
    /// is the native default `0.0`, and is still registered.
    pub fn register_book_range_check(
        &mut self,
        book: ObjectId,
        now: dereth_primitives::ServerTime,
    ) -> bool {
        if self.is_owned_by_player(book) {
            return false;
        }
        let Some(radius) = self.weenie(book).map(|w| w.pwd.use_radius.unwrap_or(0.0)) else {
            return false;
        };
        self.object_range_checks.register(
            RangeHandler::Book,
            book,
            f64::from(radius),
            true,
            false,
            POLL_INTERVAL,
            0.0,
            now.0,
        );
        true
    }

    /// Unregister book watches when the initialized panel becomes hidden.
    pub fn unregister_book_range_checks(&mut self) {
        self.object_range_checks.unregister_all(RangeHandler::Book);
    }

    /// Register the ground-container watch — registrant #5.
    ///
    /// The panel's own notice handler: `id == 0` hides the window, anything else unregisters the
    /// previous id, registers this one at its use radius, and shows the window.
    pub fn register_ground_object_range_check(
        &mut self,
        container: ObjectId,
        now: dereth_primitives::ServerTime,
    ) -> bool {
        self.register_use_radius_check(RangeHandler::ExternalContainer, container, now)
    }

    /// Register the secure-trade watch — registrant #4.
    ///
    /// This registration uses the literal range `5.0`, with no dependence on the partner's
    /// use radius.
    pub fn register_trade_range_check(
        &mut self,
        partner: ObjectId,
        now: dereth_primitives::ServerTime,
    ) {
        self.object_range_checks.register(
            RangeHandler::SecureTrade,
            partner,
            SECURE_TRADE_RANGE,
            true,
            false,
            POLL_INTERVAL,
            0.0,
            now.0,
        );
    }

    /// Register the house-profile watch — registrant #3.
    /// The radius is the literal 9.0 rather than the object's use radius.
    pub fn register_slumlord_range_check(
        &mut self,
        slumlord: ObjectId,
        now: dereth_primitives::ServerTime,
    ) {
        self.object_range_checks.register(
            RangeHandler::Slumlord,
            slumlord,
            SLUMLORD_RANGE,
            true,
            false,
            POLL_INTERVAL,
            0.0,
            now.0,
        );
    }

    /// Unregister housing watches when the panel becomes hidden.
    pub fn unregister_slumlord_range_checks(&mut self) {
        self.object_range_checks
            .unregister_all(RangeHandler::Slumlord);
    }

    /// The shape shared by registrants 1, 2 and 5: `use_radii = 1`, `ignore_z_delta = 0`, one second,
    /// no timeout, and the object's use radius as the range.
    ///
    /// **This is `use_radius`'s production reader.** The field
    /// arrives, is decoded, and gates nothing on the use path (the use itself is sent
    /// unconditionally and refused server-side); its only job in the client is this. The Rust
    /// `Option` records wire presence, but native reads the reset-initialized member directly, so
    /// an omitted field supplies `0.0` rather than suppressing the registration.
    fn register_use_radius_check(
        &mut self,
        handler: RangeHandler,
        object: ObjectId,
        now: dereth_primitives::ServerTime,
    ) -> bool {
        let Some(radius) = self.weenie(object).map(|w| w.pwd.use_radius.unwrap_or(0.0)) else {
            return false;
        };
        self.object_range_checks.register(
            handler,
            object,
            f64::from(radius),
            true,
            false,
            POLL_INTERVAL,
            0.0,
            now.0,
        );
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A geometry that answers from a table, so the list machinery can be tested with no physics.
    struct Table(std::collections::BTreeMap<u32, f32>);
    impl ObjectRangeGeometry for Table {
        fn distance(&self, object: ObjectId, _p: ObjectId, _r: bool, _z: bool) -> Option<f32> {
            self.0.get(&object.0).copied()
        }
    }

    fn list_with_one(now: f64) -> ObjectRangeCheckList {
        let mut l = ObjectRangeCheckList::default();
        l.register(
            RangeHandler::Vendor,
            ObjectId(7),
            4.0,
            true,
            false,
            POLL_INTERVAL,
            0.0,
            now,
        );
        l
    }

    /// Pinned behavior: the dedupe loop marks, it does not remove,
    /// and the new node goes to the **head**.
    #[test]
    fn a_second_registration_marks_the_first_and_goes_to_the_head() {
        let mut l = list_with_one(100.0);
        l.register(
            RangeHandler::Vendor,
            ObjectId(7),
            9.0,
            true,
            false,
            POLL_INTERVAL,
            0.0,
            101.0,
        );
        assert_eq!(l.len(), 2, "the old node is still in the list");
        let head = l.iter().next().expect("head");
        assert!(
            (head.range - 9.0).abs() < 1e-9,
            "the newest registration is the head"
        );
        assert!(!head.queued_for_deletion);
        assert_eq!(l.live().count(), 1, "and it is the only live one");
        assert!(l.is_watching(RangeHandler::Vendor, ObjectId(7)));
    }

    /// Registration stamps are `now + interval` and `now + timeout`.
    #[test]
    fn the_two_stamps_are_now_plus_the_two_periods() {
        let mut l = ObjectRangeCheckList::default();
        l.register(
            RangeHandler::Book,
            ObjectId(1),
            2.0,
            true,
            false,
            1.0,
            30.0,
            100.0,
        );
        let e = *l.iter().next().expect("head");
        assert!((e.next_update - 101.0).abs() < 1e-9);
        assert!((e.execute_at_time - 130.0).abs() < 1e-9);
    }

    /// Single-watch removal matches both handler and object; remove-all matches the handler alone.
    #[test]
    fn unregister_matches_the_pair_and_unregister_all_matches_the_handler() {
        let mut l = ObjectRangeCheckList::default();
        l.register(
            RangeHandler::Vendor,
            ObjectId(7),
            4.0,
            true,
            false,
            1.0,
            0.0,
            0.0,
        );
        l.register(
            RangeHandler::Vendor,
            ObjectId(8),
            4.0,
            true,
            false,
            1.0,
            0.0,
            0.0,
        );
        l.register(
            RangeHandler::Selection,
            ObjectId(7),
            75.0,
            true,
            true,
            1.0,
            0.0,
            0.0,
        );
        l.unregister(RangeHandler::Vendor, ObjectId(7));
        assert!(!l.is_watching(RangeHandler::Vendor, ObjectId(7)));
        assert!(l.is_watching(RangeHandler::Vendor, ObjectId(8)));
        assert!(l.is_watching(RangeHandler::Selection, ObjectId(7)));
        l.unregister_all(RangeHandler::Vendor);
        assert!(!l.is_watching(RangeHandler::Vendor, ObjectId(8)));
        assert!(l.is_watching(RangeHandler::Selection, ObjectId(7)));
    }

    /// Pinned behavior: the poll gate is `next_update < cur_time`
    /// (strict), and a node polled once does not poll again until the interval has passed.
    #[test]
    fn the_poll_gate_is_the_interval_and_the_comparison_is_strict() {
        let mut l = list_with_one(100.0);
        let mut geom = Table([(7, 1.0)].into_iter().collect());
        // `next_update` is 101.0; 101.0 is not < 101.0.
        let fired = l.step(101.0, &mut |e| {
            objects_in_range(&geom, Some(ObjectId(1)), e)
        });
        assert!(fired.is_empty());
        assert!(
            (l.iter().next().expect("head").next_update - 101.0).abs() < 1e-9,
            "not polled"
        );
        let fired = l.step(101.5, &mut |e| {
            objects_in_range(&geom, Some(ObjectId(1)), e)
        });
        assert!(fired.is_empty(), "in range, so no edge");
        assert!(
            (l.iter().next().expect("head").next_update - 102.5).abs() < 1e-9,
            "re-stamped"
        );
        // Move it out of range, but before the next poll: nothing fires.
        geom.0.insert(7, 99.0);
        let fired = l.step(102.0, &mut |e| {
            objects_in_range(&geom, Some(ObjectId(1)), e)
        });
        assert!(fired.is_empty(), "the poll interval has not elapsed");
        let fired = l.step(103.0, &mut |e| {
            objects_in_range(&geom, Some(ObjectId(1)), e)
        });
        assert_eq!(
            fired,
            vec![(RangeHandler::Vendor, ObjectId(7), RangeEdge::Exit)]
        );
    }

    /// A node that fires is queued, and the removal is **lazy**: it is
    /// still in the list until the next pass walks to it.
    #[test]
    fn a_fired_node_is_queued_and_removed_on_the_next_pass() {
        let mut l = list_with_one(100.0);
        let geom = Table([(7, 99.0)].into_iter().collect());
        let fired = l.step(102.0, &mut |e| {
            objects_in_range(&geom, Some(ObjectId(1)), e)
        });
        assert_eq!(fired.len(), 1);
        assert_eq!(l.len(), 1, "still present");
        assert_eq!(l.live().count(), 0, "but queued");
        let fired = l.step(103.0, &mut |e| {
            objects_in_range(&geom, Some(ObjectId(1)), e)
        });
        assert!(fired.is_empty(), "a queued node never fires twice");
        assert_eq!(l.len(), 0, "and the next pass removes it");
    }

    /// Either object missing is **out of
    /// range**, which is what closes a panel whose object's cell was released.
    #[test]
    fn a_missing_body_is_out_of_range_and_so_is_a_missing_player() {
        let e = ObjectRangeInfo {
            handler: RangeHandler::Vendor,
            object: ObjectId(7),
            range: 100.0,
            use_radii: true,
            ignore_z_delta: false,
            time_interval: 1.0,
            time_out: 0.0,
            execute_at_time: 0.0,
            next_update: 0.0,
            queued_for_deletion: false,
        };
        let empty = Table(std::collections::BTreeMap::new());
        assert!(
            !objects_in_range(&empty, Some(ObjectId(1)), &e),
            "no body for the object"
        );
        let present = Table([(7, 1.0)].into_iter().collect());
        assert!(objects_in_range(&present, Some(ObjectId(1)), &e));
        assert!(!objects_in_range(&present, None, &e), "no player id");
    }

    /// Oracle: retail refuses only `distance > range` and an unordered comparison, so the boundary
    /// is **inclusive** and a NaN is out of range.
    #[test]
    fn the_boundary_is_inclusive_and_a_nan_is_out_of_range() {
        let mut e = ObjectRangeInfo {
            handler: RangeHandler::Vendor,
            object: ObjectId(7),
            range: 4.0,
            use_radii: true,
            ignore_z_delta: false,
            time_interval: 1.0,
            time_out: 0.0,
            execute_at_time: 0.0,
            next_update: 0.0,
            queued_for_deletion: false,
        };
        let at = |d: f32| Table([(7, d)].into_iter().collect());
        assert!(
            objects_in_range(&at(4.0), Some(ObjectId(1)), &e),
            "exactly at the radius is in"
        );
        assert!(!objects_in_range(&at(4.000_01), Some(ObjectId(1)), &e));
        assert!(!objects_in_range(&at(f32::NAN), Some(ObjectId(1)), &e));
        e.range = f64::INFINITY;
        assert!(objects_in_range(&at(1e30), Some(ObjectId(1)), &e));
    }

    /// The timeout arm requires a value greater than double 0.0;
    /// **no shipped registration can reach it**.
    #[test]
    fn the_timeout_arm_needs_a_positive_timeout_and_no_registrant_has_one() {
        let mut l = ObjectRangeCheckList::default();
        l.register(
            RangeHandler::Book,
            ObjectId(7),
            100.0,
            true,
            false,
            1.0,
            0.0,
            0.0,
        );
        let geom = Table([(7, 1.0)].into_iter().collect());
        // In range, timeout 0.0: nothing, however long we wait.
        let fired = l.step(1e6, &mut |e| objects_in_range(&geom, Some(ObjectId(1)), e));
        assert!(fired.is_empty(), "timeout 0.0 disarms the arm entirely");
        let mut l = ObjectRangeCheckList::default();
        l.register(
            RangeHandler::Book,
            ObjectId(7),
            100.0,
            true,
            false,
            1.0,
            5.0,
            0.0,
        );
        let fired = l.step(10.0, &mut |e| objects_in_range(&geom, Some(ObjectId(1)), e));
        assert_eq!(
            fired,
            vec![(RangeHandler::Book, ObjectId(7), RangeEdge::Timeout)]
        );
    }

    /// Oracle: the walk is head-first, i.e. **reverse** registration order.
    #[test]
    fn the_walk_is_reverse_registration_order() {
        let mut l = ObjectRangeCheckList::default();
        l.register(
            RangeHandler::Vendor,
            ObjectId(1),
            1.0,
            true,
            false,
            1.0,
            0.0,
            0.0,
        );
        l.register(
            RangeHandler::SecureTrade,
            ObjectId(2),
            1.0,
            true,
            false,
            1.0,
            0.0,
            0.0,
        );
        l.register(
            RangeHandler::Selection,
            ObjectId(3),
            1.0,
            true,
            false,
            1.0,
            0.0,
            0.0,
        );
        let geom = Table(std::collections::BTreeMap::new());
        let fired = l.step(2.0, &mut |e| objects_in_range(&geom, Some(ObjectId(9)), e));
        assert_eq!(
            fired.iter().map(|f| f.0).collect::<Vec<_>>(),
            vec![
                RangeHandler::Selection,
                RangeHandler::SecureTrade,
                RangeHandler::Vendor
            ]
        );
    }

    /// Every measured registrant now has its production range-exit consumer.
    #[test]
    fn every_range_handler_has_a_consumer_here() {
        for h in [
            RangeHandler::Book,
            RangeHandler::Slumlord,
            RangeHandler::Vendor,
            RangeHandler::SecureTrade,
            RangeHandler::ExternalContainer,
            RangeHandler::Selection,
        ] {
            assert!(h.has_range_exit_consumer_in_this_build());
        }
    }

    /// Pin the measured literals independently: a table read through its own accessor cannot
    /// detect a wrong constant.
    #[test]
    fn the_six_registrations_carry_the_constants_read_in_text() {
        assert!((RADAR_RADIUS_OUTDOORS - 75.0).abs() < f32::EPSILON);
        assert!((RADAR_RADIUS_INDOORS - 25.0).abs() < f32::EPSILON);
        assert!((SLUMLORD_RANGE - 9.0).abs() < f64::EPSILON);
        assert!((SECURE_TRADE_RANGE - 5.0).abs() < f64::EPSILON);
        assert!((POLL_INTERVAL - 1.0).abs() < f64::EPSILON);
    }

    /// A house profile registers the literal 9.0 watch; range exit
    /// closes only its own covenant crystal. Staying at the
    /// inclusive boundary preserves the one-shot registration; moving beyond emits one close.
    #[test]
    fn slumlord_registration_stays_inside_closes_outside_and_can_rearm() {
        const PLAYER: ObjectId = ObjectId(1);
        const LORD: ObjectId = ObjectId(2);
        let mut world = crate::World::new();
        world.player = Some(PLAYER);
        world.slumlord = Some((LORD, crate::housing::HouseProfile::default()));
        world.register_slumlord_range_check(LORD, dereth_primitives::ServerTime(0.0));
        let armed = world
            .object_range_checks
            .live()
            .find(|e| e.handler == RangeHandler::Slumlord)
            .copied()
            .expect("registered");
        assert_eq!(armed.object, LORD);
        assert!((armed.range - SLUMLORD_RANGE).abs() < f64::EPSILON);
        assert!(armed.use_radii && !armed.ignore_z_delta);

        let mut geometry = Table([(LORD.0, 9.0)].into_iter().collect());
        let mut out = crate::RecordingSink::default();
        let mut req = crate::RecordingRequests::default();
        let inside = world.calculate_object_range_checks(
            dereth_primitives::ServerTime(2.0),
            &geometry,
            75.0,
            &mut out,
            &mut req,
        );
        assert_eq!(inside.exits, 0);
        assert!(out.0.is_empty() && req.0.is_empty());
        assert!(world
            .object_range_checks
            .is_watching(RangeHandler::Slumlord, LORD));

        geometry.0.insert(LORD.0, 9.001);
        let outside = world.calculate_object_range_checks(
            dereth_primitives::ServerTime(4.0),
            &geometry,
            75.0,
            &mut out,
            &mut req,
        );
        assert_eq!(outside.exits, 1);
        assert_eq!(out.0, vec![crate::Notice::CloseSlumlord(LORD)]);
        assert!(!world
            .object_range_checks
            .is_watching(RangeHandler::Slumlord, LORD));

        world.register_slumlord_range_check(LORD, dereth_primitives::ServerTime(5.0));
        assert!(world
            .object_range_checks
            .is_watching(RangeHandler::Slumlord, LORD));
        world.unregister_slumlord_range_checks();
        assert!(!world
            .object_range_checks
            .is_watching(RangeHandler::Slumlord, LORD));
    }
}
