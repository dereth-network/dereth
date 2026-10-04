//! Interaction frame scheduling and request delivery.

use super::*;

/// Step 8's interaction slot: the drawing pass's pick read-out and the requests produced by it
/// and by the screens.
///
/// Called from [`crate::app::App::frame`] between the camera update and `DrawWorld`, which is
/// where the client runs it: the viewpoint update builds the ray from **this** frame's camera, and
/// the notice is raised before the UI overlay draws.
///
/// Returns the [`UiRequest`]s nothing owns yet, for the caller to report.
#[allow(clippy::too_many_arguments)]
pub fn use_time(
    inter: &mut Interaction,
    store: &RetailDatStore,
    world: Option<&dyn crate::present::Scene>,
    objects: &mut crate::objects::ObjectStream,
    net: Option<&mut crate::net::ClientNetwork>,
    actions: Vec<crate::actions::Action>,
    player_desc_received: bool,
    viewport: (u32, u32),
    now: dereth_primitives::LocalTime,
) -> (Vec<UiRequest>, Vec<crate::actions::Action>) {
    use_time_with_chat_focus(
        inter,
        store,
        world,
        objects,
        net,
        actions,
        player_desc_received,
        viewport,
        now,
        &mut |_| {},
    )
}

/// The App supplies the live chat subscriber for synchronous option-notice delivery.
#[allow(clippy::too_many_arguments)]
pub fn use_time_with_chat_focus(
    inter: &mut Interaction,
    store: &RetailDatStore,
    world: Option<&dyn crate::present::Scene>,
    objects: &mut crate::objects::ObjectStream,
    net: Option<&mut crate::net::ClientNetwork>,
    actions: Vec<crate::actions::Action>,
    player_desc_received: bool,
    viewport: (u32, u32),
    now: dereth_primitives::LocalTime,
    chat_focus: &mut dyn FnMut(&mut dereth_client_model::chat::ChatState),
) -> (Vec<UiRequest>, Vec<crate::actions::Action>) {
    interaction_frame_tail(
        inter,
        store,
        world,
        objects,
        net,
        actions,
        player_desc_received,
        viewport,
        now,
        chat_focus,
        true,
    )
}

/// App's draw_no_blit/input tail. Registered UI-system timers have already run at frame entry.
#[allow(clippy::too_many_arguments)]
pub fn draw_use_time_with_chat_focus(
    inter: &mut Interaction,
    store: &RetailDatStore,
    world: Option<&dyn crate::present::Scene>,
    objects: &mut crate::objects::ObjectStream,
    net: Option<&mut crate::net::ClientNetwork>,
    actions: Vec<crate::actions::Action>,
    player_desc_received: bool,
    viewport: (u32, u32),
    now: dereth_primitives::LocalTime,
    chat_focus: &mut dyn FnMut(&mut dereth_client_model::chat::ChatState),
) -> (Vec<UiRequest>, Vec<crate::actions::Action>) {
    interaction_frame_tail(
        inter,
        store,
        world,
        objects,
        net,
        actions,
        player_desc_received,
        viewport,
        now,
        chat_focus,
        false,
    )
}

/// A use-time tick invokes registered systems in startup registration order. Client UI, player,
/// and combat run before the timer; this helper performs the work owned by those first three.
pub fn registered_systems_use_time(
    inter: &mut Interaction,
    world: Option<&dyn crate::present::Scene>,
    objects: &mut crate::objects::ObjectStream,
    net: Option<&mut crate::net::ClientNetwork>,
    now: dereth_primitives::LocalTime,
) {
    inter.last_use_time = now;
    inter.last_sent.clear();
    inter.note_player_physics(world.and_then(crate::present::Scene::character));
    if let Some(c) = world.and_then(crate::present::Scene::character) {
        let driver = c.driver();
        objects.world.combat.current_style =
            driver.movement.interp.interpreted_state.current_style.0;
        objects.world.combat.forward_command =
            driver.movement.interp.interpreted_state.forward_command.0;
    }
    inter.run_leave_target_mode();
    inter.run_player_module_use_time(&mut objects.world, ServerTime(now.0));
    inter.run_object_range_checks(world, objects, ServerTime(now.0));
    let origin = world
        .and_then(crate::present::Scene::character)
        .map(crate::character::Character::position);
    let phys = crate::selection_geometry::SceneSelectionPhysics::new(origin.as_ref(), objects);
    let radius = dereth_client_contract::radar::radar_range(
        origin
            .as_ref()
            .is_some_and(|p| dereth_physics::landdefs::is_outdoors(p.cell)),
    );
    let ready = inter.ready_for_attack(&objects.world);
    inter.run_power_bar(&mut objects.world, ready, now);
    let ready = inter.ready_for_mode_change(&objects.world);
    inter.run_pending_combat_mode(&mut objects.world, ready, &phys, radius, now);
    inter.dispatch_ui_selection_notices(
        world.and_then(crate::present::Scene::character),
        objects,
        now,
    );
    flush_requests(inter, net, false);
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn interaction_frame_tail(
    inter: &mut Interaction,
    store: &RetailDatStore,
    world: Option<&dyn crate::present::Scene>,
    objects: &mut crate::objects::ObjectStream,
    net: Option<&mut crate::net::ClientNetwork>,
    actions: Vec<crate::actions::Action>,
    player_desc_received: bool,
    viewport: (u32, u32),
    now: dereth_primitives::LocalTime,
    chat_focus: &mut dyn FnMut(&mut dereth_client_model::chat::ChatState),
    compatibility_timers: bool,
) -> (Vec<UiRequest>, Vec<crate::actions::Action>) {
    let server_now = ServerTime(now.0);
    inter.last_use_time = now;
    // Store the render-target width and height before anything can ask for a pick: the drop
    // arm reads it out of the struct because it is reached through
    // `run_ui_requests`, which the client reaches through a singleton too. The viewport
    // width/height are a different pair of numbers -- see
    // `Interaction::game_viewport`, which the App pushes in beside this.
    inter.screen = viewport;
    // The render device's viewport rectangle, resolved once for the whole frame, so
    // the pick and its read-out cannot be measured against two different rectangles.
    let rect = inter.device_viewport(viewport);
    // 1. Pointer handling. The element manager already dispatched these events
    //    through the tree; this is the viewport-specific handling of those events.
    for e in std::mem::take(&mut inter.mouse) {
        // Each pointer event carries its dispatch position. Retain it before handling
        // the event so step 2's drop handling sees the latest position, matching the
        // original input path that completes drag/drop inside mouse release.
        inter.cursor = (e.x, e.y);
        let world_click = is_world_click(e.over);
        inter.wrapper_mouse(e, viewport, world_click);
    }
    // 1b. Combat reads the motion interpreter's current
    //     style to select the charge duration: dual wield (`0x80000046`) uses
    //     **0.800 s**, otherwise **1.000 s**. The original combat system queries
    //     motion state directly. Here animation belongs to `dereth_animation` and combat
    //     to `dereth_client_model`, so the value crosses this seam once per frame.
    //
    //     The server's movement style reaches the local motion state;
    //     this bridge supplies `World.combat`. It runs where both the body and
    //     mutable world are available. `Character::driver()` gives read-only access
    //     to motion, and the combat copy is written only when the style changes.
    // 1c. Query the player's gameplay position once a frame, so
    //     that the place-in-3-D path's `on_ground` test has the answer the client's has. Before
    //     step 3
    //     for the same reason the client's is before `draw_no_blit`: the notice is what consumes it.
    inter.note_player_physics(world.and_then(crate::present::Scene::character));
    if let Some(scene) = world {
        if let Some(c) = scene.character() {
            let style = c.driver().movement.interp.interpreted_state.current_style.0;
            if inter.combat_style_bridged != Some(style) {
                inter.combat_style_bridged = Some(style);
                objects.world.combat.current_style = style;
                inter.stats.combat_style_bridges += 1;
            }
            // The same struct's `forward_command`, which is the other half of
            // the ready-position check's missile arm: movement interpretation supplies the command,
            // and readiness compares it with `0x41000003`. Counted on the same counter because
            // style and forward command cross the same seam.
            let fwd = c
                .driver()
                .movement
                .interp
                .interpreted_state
                .forward_command
                .0;
            if inter.combat_forward_command_bridged != Some(fwd) {
                inter.combat_forward_command_bridged = Some(fwd);
                objects.world.combat.forward_command = fwd;
                inter.stats.combat_style_bridges += 1;
            }
        }
    }
    // 2. The screens' requests, and the keyboard actions the UI declined.
    let unowned = inter.run_ui_requests_with_chat_focus(
        &mut objects.world,
        player_desc_received,
        server_now,
        chat_focus,
    );
    // Each combat-mode change or attack asks for readiness at its own call site.
    //
    // **Five original consumers, two argument values:**
    // attack-build, attack-execute and the power-bar entry use true; immediate
    // mode change and pending-mode retry use false. In melee and missile, true
    // returns as soon as the weapon prerequisite holds; false additionally tests
    // `!motions_pending`.
    //
    // The full mode/weapon switch is in the two readiness functions. The remaining
    // deviation for a missing physics body at attack sites is documented in
    // [`Interaction::ready_for_attack`].
    //
    // **This frame path keeps four separate readiness reads.** They
    // read the combat mode at the time of the call. `run_combat_mode_toggle` can
    // change that mode, so hoisting one answer would make later sites use the
    // mode already left behind. The original power-bar update's order relative
    // to input actions was not established here; asking separately remains valid
    // under any such ordering, while hoisting does not.
    //
    // Tests without a scene cannot distinguish these reads: no body gives false.
    // A test supplies a headless scene and advances the real motion
    // queue to prove pending-ready retry and auto-targeting. A toolbar change in
    // the same frame as an already-queued mode is not covered by a test.
    let ready_for_attack = inter.ready_for_attack(&objects.world);
    // **Take the selection geometry's inputs once.**
    //
    // Selection distance is measured from the smart box's player, which in this build is the
    // local body — the same `Position` `run_object_range_checks` measures from
    // and the same one `Hud::sync` hands the radar. With no body the snapshot is empty, which is
    // the player-space conversion's own null-player return propagated to every candidate.
    //
    // Built here rather than inside `on_actions` because the snapshot borrows `objects.presences`
    // and the cycle needs `&mut objects.world`; the borrow ends at construction because
    // `SceneSelectionPhysics` owns its table.
    let selection_origin = world
        .and_then(crate::present::Scene::character)
        .map(crate::character::Character::position);
    let selection_phys =
        crate::selection_geometry::SceneSelectionPhysics::new(selection_origin.as_ref(), objects);
    // The radar radius comes from its single owner, exactly as `run_object_range_checks` does:
    // the outside test is
    // `is_outdoors` on the player's cell id. Passed in once because `select_next` calls it
    // per candidate for a value that cannot move during the scan.
    let selection_radius = dereth_client_contract::radar::radar_range(
        selection_origin
            .as_ref()
            .is_some_and(|p| dereth_physics::landdefs::is_outdoors(p.cell)),
    );
    // The two defender-notification handlers' shared tail, which `apply_events`
    // recorded earlier in this same `App::frame`. Here rather than there because it is the first
    // point at which the selection geometry exists, and **before** `on_actions` because retail
    // runs it from the net-blob drain, ahead of
    // every input action in the frame. It is the only writer of `CombatState::last_attacked_time`,
    // which is what makes auto-targeting's 15-second reselect arm reachable at all.
    inter.run_defender_notifications(&mut objects.world, &selection_phys, selection_radius, now);
    // Run the combat system's selection-change subscriber for every
    // selection change absorbed since the last frame — including the ones this frame's own
    // `apply_events` and `run_defender_notifications` raised, which is why it sits below them.
    // Retail dispatches the notice synchronously from inside selection assignment; the seam is
    // declared on the handler itself.
    inter.run_selection_change_notices(&mut objects.world, &selection_phys, selection_radius, now);
    // `EscapeKey` asks whether the player is standing still
    // through the command interpreter. It queries the motion interpreter when a body exists;
    // otherwise it returns true. So a frame with
    // no body answers **true**, and that is the value this leaves in
    // place rather than a convenience. Read here, one line above `on_actions`, for the same reason
    // the combat-style bridge is read at step 1c: this is the only per-frame slot that holds the
    // body and the interaction at once.
    if let Some(c) = world.and_then(crate::present::Scene::character) {
        let d = c.driver();
        let still = d.movement.interp.is_standing_still(&d.env);
        drop(d);
        inter.note_standing_still(still);
    }
    let left = inter.on_actions(
        actions,
        &mut objects.world,
        &selection_phys,
        selection_radius,
        ready_for_attack,
        now,
        world.and_then(crate::present::Scene::character),
    );
    // Combat-mode setup step 2 pushes zero, and the switch it runs reads the
    // mode being LEFT, because the new mode is assigned after the call.
    let ready_for_mode_change = inter.ready_for_mode_change(&objects.world);
    inter.run_combat_mode_toggle(
        &mut objects.world,
        ready_for_mode_change,
        &selection_phys,
        selection_radius,
        now,
    );
    // Start the combat update: charge the power bar, and swing when it arrives at the
    // requested level. This is what makes a click an attack rather than a hold.
    // This uses the attack readiness check.
    if compatibility_timers {
        let ready_for_attack = inter.ready_for_attack(&objects.world);
        inter.run_power_bar(&mut objects.world, ready_for_attack, now);
        // Finish the combat update: retry a mode change the ready check refused earlier,
        // using the mode-change readiness check answered from the body.
        let ready_for_mode_change = inter.ready_for_mode_change(&objects.world);
        inter.run_pending_combat_mode(
            &mut objects.world,
            ready_for_mode_change,
            &selection_phys,
            selection_radius,
            now,
        );
        // Run the 480-second whole-module flush.
        inter.run_player_module_use_time(&mut objects.world, server_now);
        // Object range checks finish the frame update: this closes a vendor, a corpse
        // or a secure trade when the player walks away from it.
        inter.run_object_range_checks(world, objects, server_now);
    }

    // 3. The drawing pass's tail — the pick and the notice it raises.
    if inter.pick.looking_for_object() {
        match world {
            Some(w) => {
                if let Some(id) =
                    inter
                        .pick
                        .draw_no_blit(store, w.as_pick_scene(), objects, viewport, rect)
                {
                    inter.on_world_object_found(id, &mut objects.world, server_now);
                }
            }
            // **This arm raises the notice even without a scene.**
            //
            // The native drawing tail first tests whether a player exists. A missing player skips
            // viewer and normal-mode rendering but lands at the same armed-pick test. When a pick
            // is armed, it reads the selected part and object, raises the object-found notice,
            // clears the armed flag, and then clears the selection cursor.
            //
            // The player-present guard leads **to** the notice block, not past it, so a frame with
            // no player still answers an armed pick. What it answers *with* is 0:
            // `clear_selection_cursor` left the mouse-select found-polygon and
            // found-sphere flags false at the end of the previous frame, nothing swept this one,
            // and the mouse-selection object id is the polygon's only when found-polygon
            // and otherwise the sphere's only when found-sphere — zero.
            //
            // Dropping it silently would latch the drop search reason: the reason is cleared
            // **only** at the object-found notice's unconditional tail, so a scene-less
            // frame between the release and the sweep would leave reason 5 set, and every later
            // `< SearchReason::Examine` / `< SearchReason::Use` gate in
            // [`Interaction::wrapper_mouse`] would refuse — a wedged viewport. The drop's item is
            // not lost either: the zero id takes `place_in_3d`'s `onto = None` leg, which is the
            // ground, and the ground leg is refused in a scene-less frame because there is no
            // physics object to be on the ground.
            // Use `draw_no_blit_scene_less` rather than merely clearing the
            // selection cursor. The normal read-out writes **both** answer fields before raising
            // the notice, while cursor clearing writes neither. The id was already 0 on this arm;
            // `click_object_index` was
            // `find_object`'s `-1` and the client's is `0`.
            None => {
                if let Some(id) = inter.pick.draw_no_blit_scene_less() {
                    inter.stats.scene_less_notices += 1;
                    inter.on_world_object_found(id, &mut objects.world, server_now);
                }
            }
        }
    }

    // 3b. The UI-system target-mode tail. It runs after the pick,
    //     deliberately: see [`Interaction::run_leave_target_mode`] for why that ordering is a
    //     choice made here rather than one read from retail.
    if compatibility_timers {
        inter.run_leave_target_mode();
    }

    // 4. The wire. One `send_action` per request, and the rollback is the session's.
    flush_requests(inter, net, compatibility_timers);
    (unowned, left)
}

fn flush_requests(
    inter: &mut Interaction,
    net: Option<&mut crate::net::ClientNetwork>,
    clear: bool,
) {
    let outbox = std::mem::take(&mut inter.outbox);
    if clear {
        inter.last_sent.clear();
    }
    inter.last_sent.extend(outbox.iter().cloned());
    match net {
        Some(net) => {
            for r in outbox {
                if send_request(&mut net.session, &r) {
                    inter.stats.requests_sent += 1;
                } else {
                    inter.stats.requests_undeliverable += 1;
                }
            }
        }
        None => {
            for r in outbox {
                inter.stats.requests_undeliverable += 1;
                tracing::warn!("{r:?} with no server to send it to");
            }
        }
    }
}
