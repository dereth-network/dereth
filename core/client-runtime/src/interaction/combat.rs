//! Combat mode transitions and selection notifications.

use super::*;

impl Interaction {
    /// Toggle from peace to the default mode for the held equipment, or from any combat mode
    /// back to peace.
    pub(super) fn toggle_combat_mode(&mut self) {
        // The original element handler calls shared combat state directly. Here UI
        // dispatch has no mutable world borrow, so `run_combat_mode_toggle` performs
        // the call one step later in the same frame.
        self.pending_combat_toggle = true;
    }

    /// **The tail of the two defender-notification handlers, run once per
    /// notification that arrived this frame.**
    ///
    /// The world's defender-notification auto-target operation is the whole of it and carries the
    /// two listings; this function is only the seam, and it exists for the same reason
    /// [`Self::run_combat_mode_toggle`]'s second half does — `auto_target` reaches `select_next`,
    /// which needs [`crate::selection_geometry::SceneSelectionPhysics`], and that lives on this
    /// side of the crate boundary.
    ///
    /// The loop runs the tail **per notification**, not once for the batch: the stamp is
    /// idempotent within a frame but the `auto_target` under it is not, because its own
    /// no-selection gate is falsified by the first one succeeding.
    pub(super) fn run_defender_notifications(
        &mut self,
        game: &mut dereth_client_model::World,
        phys: &crate::selection_geometry::SceneSelectionPhysics,
        radar_radius: f32,
        now: dereth_primitives::LocalTime,
    ) {
        let n = std::mem::take(&mut self.pending_defender_notifications);
        if n == 0 {
            return;
        }
        let lookup = |id| phys.get(id);
        let mut notices = Notices::default();
        for _ in 0..n {
            self.stats.defender_notifications += 1;
            if game.defender_notification_auto_target(&lookup, radar_radius, now, &mut notices) {
                self.stats.defender_auto_targets += 1;
            }
        }
        self.absorb(game, notices, RecordingRequests::default());
    }

    /// **Combat selection-change handling, run once per
    /// selection-change notice absorbed since the last frame.**
    ///
    /// The world's selection-changed operation is the whole handler and carries the
    /// listing; this is only the seam, and it is the same one
    /// [`Self::run_defender_notifications`] uses and for the same reason — `auto_target` reaches
    /// `select_next`, which needs `crate::selection_geometry::SceneSelectionPhysics`.
    ///
    /// **Per notice, not once for the batch**, because `target_willingly_lost` is cleared by the
    /// notice that it refuses: two notices in one frame are *refuse, then auto-target*, and one
    /// call for the batch would lose the second.
    ///
    /// The handler can itself change the selection — `auto_target` ends in either
    /// setting the selected object or `select_next` — so it raises further notices. Those are
    /// absorbed like any other. App's synchronous owner bridge completes reentry before the next
    /// input; standalone `use_time` callers retain their existing next-frame delivery. A non-zero
    /// selected id refuses those follow-up notices, so
    /// the chain terminates rather than ringing, which is the same bound
    /// retail's synchronous re-entry has.
    pub(super) fn run_selection_change_notices(
        &mut self,
        game: &mut dereth_client_model::World,
        phys: &crate::selection_geometry::SceneSelectionPhysics,
        radar_radius: f32,
        now: dereth_primitives::LocalTime,
    ) {
        let n = std::mem::take(&mut self.pending_selection_changes);
        if n == 0 {
            return;
        }
        let lookup = |id| phys.get(id);
        let mut notices = Notices::default();
        let mut req = RecordingRequests::default();
        for _ in 0..n {
            self.stats.selection_change_notices += 1;
            let lost_before = game.combat.target_willingly_lost;
            if game.on_selection_changed(&lookup, radar_radius, now, &mut req, &mut notices) {
                self.stats.selection_change_auto_targets += 1;
            } else if lost_before && !game.combat.target_willingly_lost {
                self.stats.selection_changes_willingly_lost += 1;
            }
        }
        self.absorb(game, notices, req);
    }

    /// **The per-frame UI update's target-mode tail**,
    /// the consumer of the flag armed by `SelectLeft`/`SelectRight`:
    ///
    /// When the flag is set: if a target mode is up, it is cleared to none, the targeting input
    /// map `0x1000000B` is unregistered and the cursor state is updated; the flag is cleared
    /// either way.
    ///
    /// Retail clears the flag twice on the inner path; both are kept as the one `take` here.
    ///
    /// Production runs this registered client-UI system at the NEXT frame's UIQueue phase,
    /// before timer/device input. A pick armed later in the
    /// preceding frame therefore finishes first. The standalone `use_time` adapter keeps a
    /// combined tail, but that is not App's scheduler.
    ///
    /// App mirrors this result into the device input's target-mode registration after
    /// `use_time`, removing
    /// the exact map/callback pair before the next input drain. `update_cursor_state` then reads
    /// the resulting mode. Geometric pick completion remains at draw_no_blit, not inside input.
    pub(super) fn run_leave_target_mode(&mut self) {
        if !std::mem::take(&mut self.leave_target_mode) {
            return;
        }
        if self.target_mode != TargetMode::None {
            self.target_mode = TargetMode::None;
            self.stats.target_modes_left += 1;
        }
    }

    /// Apply an authoritative combat-mode change after the player's Int `0x28`
    /// has passed the timestamp check. Server authority bypasses compatibility, teleport and
    /// ready-position
    /// checks, does not echo ChangeCombatMode, and preserves the pending local request.
    pub fn on_combat_mode_quality_changed(
        &mut self,
        game: &mut dereth_client_model::World,
        mode: dereth_client_model::combat::CombatMode,
        phys: &crate::selection_geometry::SceneSelectionPhysics,
        radar_radius: f32,
        now: dereth_primitives::LocalTime,
    ) {
        let before = game.combat.combat_mode;
        let mut req = RecordingRequests::default();
        let mut notices = Notices::default();
        // `set_combat_mode` only refuses/queues inside its send-to-server arm.
        // The last two arguments are deliberately hostile: authority must not consult them.
        let _ = game.set_combat_mode(&mut req, &mut notices, mode, false, false, true);
        if game.combat.combat_mode != before {
            let lookup = |id| phys.get(id);
            if game.combat_mode_auto_target(&lookup, radar_radius, now, &mut notices) {
                self.stats.auto_targets += 1;
            }
        }
        self.absorb(game, notices, req);
    }

    /// Apply a pending combat-mode toggle once the world is in hand.
    ///
    /// **And `set_combat_mode`'s `auto_target` tail, which is a second call here.**
    /// `phys`, `radar_radius`, and `now` are the seam inputs auto-targeting
    /// needs and `set_combat_mode` cannot carry; see that function for why the split is a declared
    /// deviation rather than an accident.
    pub(super) fn run_combat_mode_toggle(
        &mut self,
        game: &mut dereth_client_model::World,
        ready: bool,
        phys: &crate::selection_geometry::SceneSelectionPhysics,
        radar_radius: f32,
        now: dereth_primitives::LocalTime,
    ) {
        if !std::mem::take(&mut self.pending_combat_toggle) {
            return;
        }
        let (mode, refusal) = game.toggle_combat_mode_target(false);
        if let Some(text) = refusal {
            self.refuse(game, &text);
        }
        let mut req = RecordingRequests::default();
        // `set_combat_mode`'s tail raises `SelectionChanged`, so the sink is a real one rather
        // than a discarded `Notices::default()`.
        let mut notices = Notices::default();
        // `set_combat_mode(mode, send to server)`. `teleport_in_progress` is
        // false here; `ready` is the combat-ready predicate, which the caller supplies.
        let before = game.combat.combat_mode;
        if let Err(text) = game.set_combat_mode(&mut req, &mut notices, mode, true, ready, false) {
            self.refuse(game, &text);
            self.stats.requests_refused += 1;
        }
        // Retail reaches it only by falling through the combat-mode store
        // in the native path; three of `set_combat_mode`'s `Ok` returns are above that store (the
        // no-op, the incompatible-mode refusal and the not-ready queue), so the guard is that the
        // mode actually moved rather than that the call returned `Ok`.
        if game.combat.combat_mode != before {
            let lookup = |id| phys.get(id);
            if game.combat_mode_auto_target(&lookup, radar_radius, now, &mut notices) {
                self.stats.auto_targets += 1;
            }
        }
        self.absorb(game, notices, req);
    }

    /// Run the 480-second dirty flush once per frame.
    ///
    /// `PlayerSystem::use_time` answers the timer question. It is the
    /// second of the client's two senders of the character-options event, and it is what makes
    /// a deferred option change reach the shard when the player never reopens the options page.
    ///
    /// The comparison is a strict `>` on the first-dirtied time `+ 480.0` and it fires **once**, not
    /// once per change; `use_time` clears the flag itself.
    pub fn run_player_module_use_time(
        &mut self,
        game: &mut dereth_client_model::World,
        now: ServerTime,
    ) {
        let mut req = RecordingRequests::default();
        if game.player_system.use_time_save(&mut req, now) {
            self.stats.player_modules_sent += 1;
        }
        self.absorb(game, Notices::default(), req);
    }

    /// Object-range checking.
    ///
    /// Called immediately after [`Self::run_player_module_use_time`] because that is where the
    /// client calls it: the player-system update runs the player-module update first
    /// and range-check calculation as its last statement.
    ///
    /// With no local body there is no player id to measure from, so the checks are **skipped**
    /// rather than run — running them would report every watched object out of range and close
    /// every panel. The client cannot reach that state: the player-system update is only reached
    /// in-game, where the local player body exists.
    pub(super) fn run_object_range_checks(
        &mut self,
        scene: Option<&dyn crate::present::Scene>,
        objects: &mut crate::objects::ObjectStream,
        now: ServerTime,
    ) {
        // **Part drawing's half of the seam.**
        //
        // Selected-object visibility is one global in the client
        // and two halves here: the draw observes, `dereth_client_model::World` latches. This is
        // where the observation crosses. Draining rather than copying keeps the observation
        // single-use; the latch is what persists, exactly as retail's does.
        //
        // **The order is the client's frame order, rotated to start here.** The client runs the
        // player system's update, and with it the range checks, at the very start of a frame,
        // before the UI update; object search clears the flag in the UI update, and part drawing
        // sets it later in the same frame. So the flag a range exit reads is always the one the
        // previous frame's draw left, after the previous frame's clear. This build polls the
        // ranges after the frame's input has been dispatched, so it raises the latch from the
        // last draw first, polls, and only then applies a clear that input asked for.
        //
        // The clear has to come last. Object search runs every frame the pointer rests on the
        // world view, the hover search, and not only on a click. Applied before the poll, it
        // would take the latch down every such frame, and a far selection would be dropped at
        // its first range exit although every frame drew it.
        //
        // Both halves run whether or not there is a body to measure from, so the observation
        // never outlives a frame with no body.
        if scene.is_some_and(crate::present::Scene::take_selected_part_drawn) {
            objects.world.selected_object_in_view = true;
        }
        self.poll_object_ranges(scene, objects, now);
        if self.pick.take_selected_object_in_view_clear() {
            objects.world.find_object();
        }
    }

    /// The range poll itself, between the two halves of the selection latch's seam in
    /// [`Self::run_object_range_checks`].
    fn poll_object_ranges(
        &mut self,
        scene: Option<&dyn crate::present::Scene>,
        objects: &mut crate::objects::ObjectStream,
        now: ServerTime,
    ) {
        let player_id = objects.world.player;
        let Some(geometry) = crate::object_range::SceneRangeGeometry::new(
            scene.and_then(crate::present::Scene::character),
            &objects.physics,
            player_id,
        ) else {
            return;
        };
        // Retail's outdoors predicate asks whether the player's current cell id is
        // outdoors, and its one consumer here is
        // the outdoors/indoors `75.0 : 25.0` range pair the radar
        // scales by, fetched from its one owner rather than re-derived. Taken from the body's own
        // cell, which is the `ViewerFrame::position` `Hud::sync` reads.
        let outside = scene
            .and_then(crate::present::Scene::character)
            .is_some_and(|c| dereth_physics::landdefs::is_outdoors(c.position().cell));
        let radar_radius = dereth_client_contract::radar::radar_range(outside);
        let mut out = Notices::default();
        let mut req = RecordingRequests::default();
        let stats = objects.world.calculate_object_range_checks(
            now,
            &geometry,
            radar_radius,
            &mut out,
            &mut req,
        );
        self.stats.range_polls += u64::from(stats.polled);
        self.stats.range_exits += u64::from(stats.exits);
        self.stats.range_exits_without_a_window += u64::from(stats.exits_without_a_window);
        self.stats.range_selection_rearms += u64::from(stats.selection_rearms);
        self.stats.range_selection_clears += u64::from(stats.selection_clears);
        // Selection assignment publishes the current selected id to rendering at its tail,
        // which the range-exit handler reaches on every path --
        // `calculate_object_range_checks` is where that handler runs in this build, so the write
        // is published to the render side immediately after it.
        if let Some(sc) = scene {
            sc.set_selected_object_id(objects.world.viewcone_check_object_id);
        }
        self.absorb(&mut objects.world, out, req);
    }

    /// The combat update's **head**, run once per frame and **before**
    /// [`Self::run_pending_combat_mode`], which is the same order the client has.
    ///
    /// **This is where a click becomes an attack.** The attack control is not a *held* one: a
    /// single click starts the speed/power bar charging, which kicks off an attack when it
    /// reaches the right spot. Both triggers — the combat control's `0x1C`/`1` pair and
    /// `handle_combat_action`'s press/release pair — are the *same* pair
    /// (`set_requested_attack_height` then `end_attack_request`). This call is what advances the
    /// bar; without it the only producer of an attack would be the end-attack request's release
    /// arm. See the world's combat power-bar update.
    pub(super) fn run_power_bar(
        &mut self,
        game: &mut dereth_client_model::World,
        ready: bool,
        now: dereth_primitives::LocalTime,
    ) {
        let mut req = RecordingRequests::default();
        let before = req.0.len();
        let refusal = game.combat_power_bar_use_time(&mut req, ready, now);
        if req.0.len() > before {
            self.stats.attacks_released += 1;
        }
        if let Some(text) = refusal {
            self.refuse(game, text);
            self.stats.requests_refused += 1;
        }
        self.absorb(game, Notices::default(), req);
    }

    /// The combat-update tail, run once per frame.
    ///
    /// `set_combat_mode`'s not-ready branch stores the requested mode and sends
    /// nothing; this is the only thing that ever retries it. Without this call a toggle pressed
    /// while the body is busy is dropped for ever — see the busy-state gate below.
    pub(super) fn run_pending_combat_mode(
        &mut self,
        game: &mut dereth_client_model::World,
        ready: bool,
        phys: &crate::selection_geometry::SceneSelectionPhysics,
        radar_radius: f32,
        now: dereth_primitives::LocalTime,
    ) {
        let mut req = RecordingRequests::default();
        let mut notices = Notices::default();
        // `use_time` clears the pending combat mode after `set_combat_mode`'s `auto_target` tail.
        // The callback keeps that order while using this frame's existing geometry snapshot.
        let refusal = game.combat_use_time_with_mode_change(
            &mut req,
            &mut notices,
            ready,
            |game, notices| {
                let lookup = |id| phys.get(id);
                if game.combat_mode_auto_target(&lookup, radar_radius, now, notices) {
                    self.stats.auto_targets += 1;
                }
            },
        );
        if let Some(text) = refusal {
            self.refuse(game, &text);
            self.stats.requests_refused += 1;
        }
        self.absorb(game, notices, req);
    }
}
