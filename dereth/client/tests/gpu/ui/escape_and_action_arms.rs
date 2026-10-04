//! Six client UI action arms each work on their own: a click in target mode leaves the mode at the
//! frame tail; Escape finishes a jump, cancels the target cursor, drops the target as willingly
//! lost, stops a moving body with "Action interrupted", aborts an automatic attack silently, and
//! with nothing selected asks for the gameplay options panel; the screenshot key records a request
//! and writes a chat line; help, plugin manager and radar keys reach their arms. Fixture: a
//! software D3D12 device, a `WorldScene` with an attached body, the retail dats, and actions handed
//! to `dereth_client::interaction::use_time`; the three host halves (screenshot, stopping the
//! body, panel visibility) also run through a whole `App::frame` on the shipped layout.
//!
//! # The arms
//!
//! The client UI action dispatcher has eight live arms. A 117-entry byte index over `action - 7`
//! maps five non-default action ids to four live arms plus the default target; `0x7C` has its own
//! branch; and `0x1000001E`, `0x10000025`, and `0x1000002B` use chained subtractions. `USE` and
//! `SelectionExamine` are covered elsewhere; the other six are this module's subject, and each is
//! asserted on its own, because six in aggregate would pass with five dead.
//!
//! | action | arm | mechanism |
//! |---|---|---|
//! | `0x07`/`0x08` `SelectLeft`/`SelectRight` | target-mode leave | a **press with a target mode up arms deferred target-mode exit**, which the end of the frame applies. Not a vendor arm. |
//! | `0x27` `EscapeKey` | escape cascade | finish a jump, else relinquish focus, else (standing still and not repeat-attacking) cancel the target mode / open the options panel / drop the target, else stop the body and abort the attack |
//! | `0x55` `CaptureScreenshot` | screenshot | save a screenshot, then add a chat line naming the file. Returns true either way |
//! | `0x7B` `ToggleHelp` | help | open the third-party help target |
//! | `0x7C` `TogglePluginManager` | plugin manager | ask whether the third-party plugin manager is open, then close or open it; no UI element or visibility field is involved |
//! | `0x1000001E` `ToggleRadarPanel` | radar | flip a radar-visible boolean and broadcast a visibility notice; no UI element is involved, and nothing handles the notice |
//!
//! Neither of the last two performs an element lookup, so they do not share the examine toggle's
//! shape, nor each other's. `EscapeKey`'s first cascade leg cancels the use/examine cursor rather
//! than anything about a vendor, and `ToggleRadarPanel` toggles the radar flag rather than the
//! leave-target-mode one.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;
use dereth_client::world::SceneWrites;

use std::sync::Arc;

use dereth_client::character::PLAYER_OBJECT_ID;
use dereth_client::interaction::{action as ia, Interaction, TargetMode};
use dereth_client::objects::ObjectStream;
use dereth_client::ui::UiMouseEvent;
use dereth_client::world::{SceneConfig, WorldScene};
use dereth_client_model::combat::PowerBarMode;
use dereth_client_net::client_session::SessionEvent;
use dereth_dat::RetailDatStore;
use dereth_primitives::{LocalTime, ObjectId, Position, Quat, Vec3};
use dereth_render::device::Gpu;
use dereth_ui_screens::view::UiRequest;

const AUTO_REPEAT_OPTION: usize = dereth_client_model::player::options::option::AUTO_REPEAT_ATTACK;

/// A plain object to hold as a selection.
const TARGET: ObjectId = ObjectId(0x8300_0001);
const TARGET_AT: (f32, f32, f32) = (1.8, 2.4, 0.0);

// =================================================================================================
// Bench
// =================================================================================================

struct Bench {
    _gpu: Gpu,
    scene: WorldScene,
    store: Arc<RetailDatStore>,
    objects: ObjectStream,
    inter: Interaction,
    player: Position,
    now: f64,
}

impl Bench {
    fn new() -> Self {
        let store = Arc::new(
            dereth_dat::testing::open_store()
                .expect("the retail dats are `use_time`'s own argument: set DERETH_TEST_DAT_DIR"),
        );
        let mut gpu = crate::common::test_gpu(800, 600);
        let region = dereth_client::world::load_region(&store).expect("the region decodes");
        let scfg = SceneConfig {
            cell_statics: false,
            mesh_collision: false,
            land_radius: 1,
            scenery_radius: 0,
            particles: false,
            ..SceneConfig::default()
        };
        let mut scene = WorldScene::load(&store, &mut gpu, scfg).expect("the scene loads");
        scene
            .attach_character(&store, &region, &mut gpu)
            .expect("the body is created");
        let player = scene.character.as_ref().expect("a body").position();

        let mut objects = ObjectStream::new();
        objects.world.player = Some(PLAYER_OBJECT_ID);
        let mut me = dereth_client_model::Weenie::new(PLAYER_OBJECT_ID);
        me.valid = true;
        me.has_phys_obj = true;
        me.qualities = Some(dereth_client_model::qualities::Qualities::new());
        objects.world.tables.weenies.insert(PLAYER_OBJECT_ID, me);
        // Without this the range cull drops the selection on its own. This bench runs frame step 8
        // and not step 11, and the draw pass that raises the in-view latch is in step 11, so the
        // latch would stay down here for a reason that has nothing to do with what this file
        // measures. `selection::selection_persistence` drives the draw pass itself.
        objects.world.selected_object_in_view = true;

        let b = Self {
            _gpu: gpu,
            scene,
            store,
            objects,
            inter: Interaction::new(),
            player,
            now: 1.0,
        };
        b.stand_still(true);
        b
    }

    /// The standing-still predicate begins with the motion layer's on-ground state. A freshly
    /// attached body is **not** on the ground, so the default here is `false` and both answers are
    /// reachable.
    fn stand_still(&self, still: bool) {
        self.scene
            .character
            .as_ref()
            .expect("a body")
            .driver_mut()
            .env
            .on_ground = still;
    }

    fn place(&mut self, id: ObjectId, offset: (f32, f32, f32)) {
        use dereth_protocol::types::{physicsdesc::flags, ObjDesc, PhysicsDesc, PublicWeenieDesc};

        let v = Vec3::new(offset.0, offset.1, offset.2);
        let origin = dereth_physics::math::localtoglobal(&self.player.frame, v);
        let body = dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(
            dereth_protocol::objects::ObjectCreatePayload {
                id,
                objdesc: ObjDesc::default(),
                physicsdesc: PhysicsDesc {
                    bitfield: flags::POSITION,
                    state: 0,
                    position: Some(dereth_protocol::types::PositionWire {
                        objcell_id: self.player.cell.0,
                        frame: dereth_protocol::types::Frame {
                            origin: origin.into(),
                            orientation: Quat::IDENTITY.into(),
                        },
                    }),
                    ..PhysicsDesc::default()
                },
                wdesc: PublicWeenieDesc::default(),
            },
        ))
        .expect("encode");
        self.objects.apply_event(
            &SessionEvent::WorldObject {
                opcode: dereth_protocol::Opcode::ITEM_CREATE_OBJECT,
                body,
            },
            LocalTime(self.now),
        );
        self.objects.world.update_visible_object_list();
    }

    /// One frame with the given actions; returns how many were left unconsumed.
    fn deliver(&mut self, actions: Vec<u32>) -> usize {
        let events: Vec<_> = actions
            .into_iter()
            .map(|a| dereth_client_runtime::actions::Action {
                id: dereth_input::ActionId(a),
                phase: dereth_client_runtime::actions::ActionPhase::Begin,
                extent: 1.0,
                repeats: 0,
            })
            .collect();
        let (unowned, left) = dereth_client::interaction::use_time(
            &mut self.inter,
            &self.store,
            Some(&self.scene),
            &mut self.objects,
            None,
            events,
            false,
            (800, 600),
            LocalTime(self.now),
        );
        assert!(unowned.is_empty(), "no unowned UI requests were expected");
        left.len()
    }

    fn press(&mut self, action: u32) {
        assert_eq!(
            self.deliver(vec![action]),
            0,
            "{action:#010X} must be consumed by the client UI action handler's arm"
        );
    }

    fn frame(&mut self) {
        assert_eq!(self.deliver(Vec::new()), 0);
    }

    /// Deliver a pointer event: the same event shape `App::interaction_use_time` queues from the
    /// shell.
    fn click(&mut self, action: u32, start: bool) {
        self.inter.queue(
            vec![UiMouseEvent {
                action,
                start,
                x: 400,
                y: 300,
                over: None,
            }],
            Vec::new(),
        );
    }

    /// Arm use target mode: the toolbar's Use button with nothing selected, which is the client's
    /// only writer of the target-mode field.
    fn arm_use_cursor(&mut self) {
        self.inter.queue(
            Vec::new(),
            vec![UiRequest::SetTargetMode(
                dereth_ui_screens::view::TargetMode::Use,
            )],
        );
        self.frame();
        assert_eq!(
            self.inter.target_mode(),
            TargetMode::Use,
            "premise: the use cursor is armed"
        );
    }

    fn s(&self) -> dereth_client::interaction::InteractionStats {
        self.inter.stats
    }
}

// =================================================================================================
// Arm 1: `SelectLeft` / `SelectRight`
// =================================================================================================

/// **The click that arms deferred target-mode exit, and the frame tail that acts on it.**
///
/// The arm reads the event's start flag, returns for a release, reads the target mode,
/// returns when no mode is active, and otherwise sets the deferred leave flag.
///
/// Both gates are asserted on their own: a press with **no** target mode arms nothing, and a
/// **release** with a target mode up arms nothing either; that second one is the whole reason
/// the event's start flag is read here and is the only start-edge read in the UI action
/// dispatcher.
///
/// **What it is worth to a player.** A use cursor armed and then clicked at empty sky is put away
/// rather than staying armed for the rest of the session.
#[test]
fn a_click_with_a_target_mode_up_arms_the_leave_flag_and_the_frame_tail_drops_the_mode() {
    // (a) press with the cursor armed: the flag goes up and the per-frame update tail drops the
    // mode.
    let mut b = Bench::new();
    b.arm_use_cursor();
    b.click(ia::SELECT_LEFT, true);
    b.frame();
    assert_eq!(
        b.s().leave_target_mode_armed,
        1,
        "(a) deferred target-mode exit was armed"
    );
    assert_eq!(
        b.s().target_modes_left,
        1,
        "(a) the client UI per-frame update acted on it"
    );
    assert_eq!(
        b.inter.target_mode(),
        TargetMode::None,
        "(a) and the cursor is gone"
    );

    // (b) the same press with **no** target mode: the target-mode test returns.
    let mut b = Bench::new();
    b.click(ia::SELECT_LEFT, true);
    b.frame();
    assert_eq!(
        b.s().leave_target_mode_armed,
        0,
        "(b) no target mode, nothing armed"
    );
    assert_eq!(b.s().target_modes_left, 0);

    // (c) the **release** edge with the cursor armed: the event-start test returns.
    let mut b = Bench::new();
    b.arm_use_cursor();
    b.click(ia::SELECT_LEFT, false);
    b.frame();
    assert_eq!(
        b.s().leave_target_mode_armed,
        0,
        "(c) a release arms nothing"
    );
    assert_eq!(
        b.inter.target_mode(),
        TargetMode::Use,
        "(c) so the cursor survives it"
    );

    // (d) the right button shares the arm: the index table sends 0x07 and 0x08 to one target.
    let mut b = Bench::new();
    b.arm_use_cursor();
    b.click(ia::SELECT_RIGHT, true);
    b.frame();
    assert_eq!(
        b.s().leave_target_mode_armed,
        1,
        "(d) `SelectRight` is the same arm"
    );
    assert_eq!(b.inter.target_mode(), TargetMode::None);
}

// =================================================================================================
// Arm 2: `EscapeKey`
// =================================================================================================

/// Behaviour: ui.escape.unwinds-jump-target-cursor-and-target-before-opening-options
///
/// **Leg 1: a jump in progress is finished, and nothing below runs.**
///
/// The comparison takes this leg only for strictly positive jump power. Equal or lower
/// values continue through the rest of the Escape cascade.
#[test]
fn escape_finishes_a_jump_before_it_looks_at_anything_else() {
    let mut b = Bench::new();
    b.place(TARGET, TARGET_AT);
    b.objects.world.set_selected_object(
        Some(TARGET),
        false,
        &mut dereth_client_model::RecordingSink::default(),
    );
    b.objects.world.combat.jump_pending = true;
    b.objects.world.combat.power_bar_mode = PowerBarMode::Jump;
    b.objects.world.combat.build_in_progress = true;
    b.objects.world.combat.build_start_time = b.now;
    assert!(
        b.objects.world.combat.jump_power_level(LocalTime(b.now)) > 0.0,
        "premise: jump power is strictly positive while jumping"
    );

    b.press(ia::ESCAPE_KEY);
    assert_eq!(b.s().escape_finish_jumps, 1, "the finish-jump leg ran");
    assert!(
        !b.objects.world.combat.jump_pending,
        "the jump-pending flag was cleared"
    );
    assert_eq!(
        b.objects.world.combat.power_bar_mode,
        PowerBarMode::Undef,
        "the power-bar mode was reset"
    );
    // Nothing below it ran: the selection is untouched and no other leg was counted.
    assert_eq!(
        b.objects.world.selected,
        Some(TARGET),
        "the target survived"
    );
    assert_eq!(b.s().escape_deselects, 0);
    assert_eq!(b.s().escape_stops, 0);
    assert_eq!(b.s().escape_options_toggles, 0);
}

/// **Leg 3a: the use/examine cursor is cancelled first.** The field read is the target mode, not
/// a pending vendor id. The arm clears target mode and returns before it can look at
/// the selection.
#[test]
fn escape_cancels_the_target_cursor_before_it_drops_the_target() {
    let mut b = Bench::new();
    b.place(TARGET, TARGET_AT);
    b.objects.world.set_selected_object(
        Some(TARGET),
        false,
        &mut dereth_client_model::RecordingSink::default(),
    );
    b.arm_use_cursor();

    b.press(ia::ESCAPE_KEY);
    assert_eq!(
        b.s().escape_target_mode_clears,
        1,
        "the target mode was cleared"
    );
    assert_eq!(b.inter.target_mode(), TargetMode::None);
    assert_eq!(
        b.objects.world.selected,
        Some(TARGET),
        "and the target is still held"
    );
    assert_eq!(
        b.s().escape_deselects,
        0,
        "the deselect leg is below this one"
    );
    assert!(
        !b.objects.world.combat.target_willingly_lost,
        "so the flag was not set"
    );
}

/// **Leg 3b: with nothing selected, Escape opens the gameplay options panel.**
///
/// The arm dispatches `0x1000001B`, `ToggleGameplayOptionsPanel`. This is why Escape opens the
/// options menu, and it is the leg an ordinary player hits most often.
#[test]
fn escape_with_nothing_selected_asks_for_the_gameplay_options_panel() {
    let mut b = Bench::new();
    assert_eq!(b.objects.world.selected, None, "premise: nothing selected");
    b.press(ia::ESCAPE_KEY);
    assert_eq!(
        b.s().escape_options_toggles,
        1,
        "the options visibility toggle was requested"
    );
    assert_eq!(
        b.inter.take_visibility_toggle(),
        Some(ia::TOGGLE_GAMEPLAY_OPTIONS_PANEL),
        "it asked for action 0x1000001B, the gameplay-options toggle"
    );
    assert_eq!(b.s().escape_deselects, 0, "there was nothing to deselect");
}

/// **Leg 3c: with a target held, Escape drops it *and says so*.**
///
/// The flag store is the **only writer of `true`**. Its reader is the selection-change handler's
/// fifteen-second auto-target fallback: the flag makes the very notice this deselect raises refuse
/// to auto-target, so the game does not immediately hand back the target the player just dropped.
///
/// The store is **before** the deselection request, which is what makes it visible
/// to that notice; both orders would deselect and only one has the effect.
#[test]
fn escape_with_a_target_drops_it_and_marks_the_loss_as_willing() {
    let mut b = Bench::new();
    b.place(TARGET, TARGET_AT);
    b.objects.world.set_selected_object(
        Some(TARGET),
        false,
        &mut dereth_client_model::RecordingSink::default(),
    );
    assert!(
        !b.objects.world.combat.target_willingly_lost,
        "premise: the flag starts clear"
    );

    b.press(ia::ESCAPE_KEY);
    assert_eq!(
        b.s().escape_deselects,
        1,
        "the Escape arm deselected the target"
    );
    assert_eq!(b.objects.world.selected, None, "the target is gone");
    assert!(
        b.objects.world.combat.target_willingly_lost,
        "the willing-target-loss flag was set before the selection-change notice reads it"
    );
    assert_eq!(
        b.s().escape_options_toggles,
        0,
        "the options leg is the *other* branch"
    );
}

/// **Leg 4: the body was not standing still, so Escape stops it and says *"Action interrupted"*.**
///
/// The tested value is the **inverse** of standing still, so its final test prints the line
/// exactly when the player was moving. The standing query is `physobj ? motion_is_standing_still :
/// true`, and this bench drives it from the body's own motion interpreter.
///
/// Asserted **both ways from the same gesture**: standing still takes the cascade and prints
/// nothing, moving takes this leg and prints.
#[test]
fn escape_while_moving_stops_the_body_and_prints_action_interrupted() {
    let mut b = Bench::new();
    b.place(TARGET, TARGET_AT);
    b.objects.world.set_selected_object(
        Some(TARGET),
        false,
        &mut dereth_client_model::RecordingSink::default(),
    );
    b.stand_still(false);

    b.press(ia::ESCAPE_KEY);
    assert_eq!(
        b.s().escape_stops,
        1,
        "the command path requested a complete stop"
    );
    assert!(
        b.inter.take_stop_completely(),
        "and the request is there for `App` to perform"
    );
    assert_eq!(
        b.s().escape_interrupts,
        1,
        "the moving-body arm emitted its interruption line"
    );
    assert_eq!(
        b.inter.last_refusal.as_deref(),
        Some("Action interrupted"),
        "the moving-body arm retains the action-interrupted text"
    );
    assert_eq!(
        b.objects.world.selected,
        Some(TARGET),
        "and the cascade legs did NOT run"
    );
    assert_eq!(b.s().escape_deselects, 0);

    // The control: the same gesture with the body standing still takes the cascade instead.
    let mut b = Bench::new();
    b.place(TARGET, TARGET_AT);
    b.objects.world.set_selected_object(
        Some(TARGET),
        false,
        &mut dereth_client_model::RecordingSink::default(),
    );
    b.stand_still(true);
    b.press(ia::ESCAPE_KEY);
    assert_eq!(b.s().escape_stops, 0, "standing still: no stop");
    assert_eq!(b.s().escape_interrupts, 0, "and no line");
    assert_eq!(b.s().escape_deselects, 1, "the cascade ran instead");
}

/// **Leg 4, the other way in: a repeat attack is running, so Escape aborts it.**
///
/// The repeat-attack predicate is the second half of `standing && !repeating`, and the arm asks it
/// a **second** time on the way out before aborting the automatic attack. The body is standing
/// still here, so this leg is reached by the repeat-attack half alone and the
/// *"Action interrupted"* line is **not** printed, which is the discrimination the two halves need.
#[test]
fn escape_during_an_automatic_attack_aborts_it_without_printing() {
    let mut b = Bench::new();
    b.place(TARGET, TARGET_AT);
    b.objects.world.set_selected_object(
        Some(TARGET),
        false,
        &mut dereth_client_model::RecordingSink::default(),
    );
    b.stand_still(true);
    b.objects
        .world
        .player_system
        .options
        .set(AUTO_REPEAT_OPTION, true);
    b.objects.world.combat.attack_in_progress = true;
    b.objects.world.combat.repeat_attacking = true;
    assert!(
        b.objects.world.repeat_attack_in_progress(),
        "premise: repeat attack is in progress"
    );

    b.press(ia::ESCAPE_KEY);
    assert_eq!(
        b.s().escape_stops,
        1,
        "the stop leg was reached by the repeat-attack half"
    );
    assert_eq!(
        b.s().escape_interrupts,
        0,
        "but the body was standing still, so nothing printed"
    );
    assert_eq!(
        b.s().escape_attack_aborts,
        1,
        "the automatic attack was aborted"
    );
    assert!(
        !b.objects.world.combat.repeat_attacking,
        "and the attack is off"
    );
    assert_eq!(
        b.objects.world.selected,
        Some(TARGET),
        "the cascade did not run"
    );
}

/// **Every Escape is consumed by the arm.** `0x27` does not fall through `Interaction::on_actions`'
/// `_ =>` into `left`; there is no arm for it in `GamePlayScreen::handle_key_press`, and the
/// pre-game arms (`UiShell::mode_on_action`, the chat window's `deactivate_chat_entry`, and focused
/// text elements' escape handling) are all above this point in the pipeline.
///
/// That last fact also explains why the focus-element leg is not handled here:
/// `interaction::use_time` is handed `InputShell::take_events`, i.e. only the actions the UI
/// **declined**, so an Escape that arrives here is by construction one no focused element wanted.
#[test]
fn escape_no_longer_falls_through_the_action_dispatch() {
    for still in [true, false] {
        let mut b = Bench::new();
        b.stand_still(still);
        assert_eq!(
            b.deliver(vec![ia::ESCAPE_KEY]),
            0,
            "still={still}: the arm consumed it rather than leaving it for the next handler"
        );
    }
}

// =================================================================================================
// Arm 3: `CaptureScreenshot`
// =================================================================================================

/// **The screenshot key asks the device, and the arm returns TRUE either way.**
///
/// The arm calls the screenshot saver, skips the chat line on `false`, and returns true
/// on both paths. The device half is `App`'s and is asserted below.
#[test]
fn the_screenshot_key_asks_for_one_and_is_consumed() {
    let mut b = Bench::new();
    assert!(
        !b.inter.take_screenshot_request(),
        "premise: nothing is pending"
    );
    b.press(ia::CAPTURE_SCREENSHOT);
    assert_eq!(
        b.s().screenshots_requested,
        1,
        "the screenshot request was recorded"
    );
    assert!(
        b.inter.take_screenshot_request(),
        "and the request is there for `App`"
    );
    assert!(
        !b.inter.take_screenshot_request(),
        "drained once, as `App` drains it"
    );
}

// =================================================================================================
// Arm 4: `ToggleHelp`
// =================================================================================================

/// **Open help with `(0, 0x10000001)` and return true.**
///
/// The external browser integration is third-party, so the client implements the application's
/// side of the seam: the key is consumed here rather than falling through to
/// `GamePlayScreen::handle_key_press`'s empty `action::OPEN_HELP => {}`, and the request is
/// counted. **The deviation is that nothing is drawn**, and the counter is the only thing that can
/// see the difference between that and an arm that is not there.
#[test]
fn the_help_key_reaches_its_arm_rather_than_falling_through() {
    let mut b = Bench::new();
    b.press(ia::TOGGLE_HELP);
    assert_eq!(b.s().help_opens, 1);
    b.press(ia::TOGGLE_HELP);
    assert_eq!(
        b.s().help_opens,
        2,
        "help is not a toggle -- it opens twice"
    );
}

// =================================================================================================
// Arm 5: `TogglePluginManager`
// =================================================================================================

/// **A toggle by another mechanism: ask the external plugin manager, then take the other branch.**
///
/// The arm asks whether the plugin manager is open and then calls the corresponding close
/// or open operation.
///
/// No element lookup, visibility bit, or element visibility setter is involved, so the examine
/// toggle is not this shape, and neither is the radar one below. **Asserted over two presses**,
/// because one press cannot distinguish a toggle from a set: a build that only ever opened would
/// pass a single-press test.
#[test]
fn the_plugin_manager_key_is_a_toggle_and_not_a_set() {
    let mut b = Bench::new();
    assert!(
        !b.inter.plugin_manager_open(),
        "premise: the plugin manager starts closed"
    );

    b.press(ia::TOGGLE_PLUGIN_MANAGER);
    assert!(b.inter.plugin_manager_open(), "press 1 opened it");
    assert_eq!(
        (b.s().plugin_manager_opens, b.s().plugin_manager_closes),
        (1, 0)
    );

    b.press(ia::TOGGLE_PLUGIN_MANAGER);
    assert!(
        !b.inter.plugin_manager_open(),
        "press 2 closed the plugin manager"
    );
    assert_eq!(
        (b.s().plugin_manager_opens, b.s().plugin_manager_closes),
        (1, 1)
    );

    b.press(ia::TOGGLE_PLUGIN_MANAGER);
    assert!(b.inter.plugin_manager_open(), "press 3 opened it again");
    assert_eq!(
        (b.s().plugin_manager_opens, b.s().plugin_manager_closes),
        (2, 1)
    );
}

// =================================================================================================
// Arm 6: `ToggleRadarPanel`
// =================================================================================================

/// **A `bool` member and a notice: the third distinct toggle mechanism in one dispatcher.**
///
/// The arm reads the radar-visible field, negates it, stores the result, and broadcasts
/// the result.
///
/// **Starts `true`**, which the client stores at construction and session teardown restores, not
/// a default chosen here. So the *first* press hides, the opposite of what a `bool::default()`
/// would have done, and it is why the field is a `StartsTrue` rather than a bare `bool`.
#[test]
fn the_radar_key_toggles_a_flag_that_starts_true() {
    let mut b = Bench::new();
    assert!(
        b.inter.radar_visible(),
        "the constructor's `= true`, not `bool::default()`"
    );

    b.press(ia::TOGGLE_RADAR_PANEL);
    assert!(!b.inter.radar_visible(), "press 1 flips it off");
    assert_eq!(b.s().radar_visibility_notices, 1, "and sent the notice");

    b.press(ia::TOGGLE_RADAR_PANEL);
    assert!(
        b.inter.radar_visible(),
        "press 2 flips it back: a toggle, not a set"
    );
    assert_eq!(b.s().radar_visibility_notices, 2);
}

// =================================================================================================
// The three host halves, through a whole `App::frame`
// =================================================================================================

fn app_in_gameplay(frames: u32) -> dereth_client::app::App {
    // `App::screenshot_path` composes the name the way retail's screenshot saver does. It places
    // the image under the directory containing the default preferences file, represented here by
    // `Config::preferences_file`, so a test that presses the screenshot key must point that
    // somewhere disposable. `Config::default()` leaves it empty, and an empty default preferences
    // path is relative to the working directory, which here would be the workspace.
    let prefs = std::env::temp_dir()
        .join("dereth-action-screenshots")
        .join("dereth-client.ini");
    if let Some(d) = prefs.parent() {
        std::fs::create_dir_all(d).expect("a disposable preferences directory");
        for n in 0..4u32 {
            let _ = std::fs::remove_file(d.join(format!("ScreenShot{n:05}.png")));
        }
    }
    let cfg = dereth_client::config::Config {
        ui: true,
        headless: true,
        sound: false,
        dat_dir: client_dir(),
        preferences_file: prefs,
        ..dereth_client::config::Config::default()
    };
    let mut app = dereth_client::app::App::new(cfg).expect("an application");
    app.start_shell().expect("the shell starts");
    let s = dereth_client::world::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..dereth_client::world::SceneConfig::default()
    };
    app.load_static_scene(s).expect("a static scene");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    for _ in 0..frames {
        app.frame();
    }
    app
}

fn inject(app: &mut dereth_client::app::App, action: u32) {
    let e = dereth_input::InputEvent {
        action: dereth_input::ActionId(action),
        input_map: dereth_client::ui::UI_INPUT_MAP,
        toggle: dereth_input::ToggleType::OneShot,
        extent: 1.0,
        start: true,
        repeat_delta: 1,
        repeat_total: 0,
        from_key_down: false,
    };
    app.input_manager_mut()
        .expect("an input manager")
        .inject_action(e);
    app.frame();
}

/// **The screenshot arm's device half, and the chat line that names the file.**
///
/// Driven through `App::frame` end to end: the action goes into the application's own input manager
/// with `inject_action`, then follows the production UI refusal, interaction arm, and application
/// drain. The configured headless world and temporary preferences path remain fixture inputs. This
/// test verifies the host counter, queued chat line, and channel; it does not reopen the saved
/// file or inspect pixels.
#[test]
fn the_screenshot_arm_reaches_the_device_and_writes_the_chat_line() {
    let mut app = app_in_gameplay(3);
    assert_eq!(
        app.probe().action_arm_host_stats().2,
        0,
        "premise: nothing saved yet"
    );
    let before = app.objects().world.scroll.added;

    inject(&mut app, ia::CAPTURE_SCREENSHOT);

    let (.., saved, failed) = app.probe().action_arm_host_stats();
    assert_eq!(saved + failed, 1, "the drain ran exactly once");
    assert_eq!(failed, 0, "the screenshot host reports success");
    assert_eq!(
        app.objects().world.scroll.added,
        before + 1,
        "exactly one screenshot-result line was queued"
    );
    // `pending` and not a drained log: the HUD chat drain runs earlier in `App::frame` than the
    // interaction stage that queues this result, so this frame's line is still pending.
    let line = app
        .objects()
        .world
        .scroll
        .pending()
        .iter()
        .find(|l| l.body.starts_with("Screenshot saved to file '"))
        .unwrap_or_else(|| {
            panic!(
                "the screenshot-result prefix should be present; queued: {:?}",
                app.objects().world.scroll.pending()
            )
        });
    assert_eq!(
        line.chat_type,
        dereth_client_model::scroll::LOCAL_ERROR_TYPE,
        "the line uses local-error channel 0x1A"
    );
}

/// **Escape's options leg reaches visibility-toggle dispatch in the live UI tree.**
///
/// `dereth_ui::UiSystem::dispatch_input_action` is the current visibility-toggle dispatch. This
/// asserts the host half (that `App` drains the request and hands it to a real `UiSystem`), which
/// is the half a headless bench cannot see.
#[test]
fn escapes_options_leg_reaches_the_live_ui_tree() {
    let mut app = app_in_gameplay(3);
    assert_eq!(app.probe().action_arm_host_stats().0, 0);
    assert_eq!(
        app.objects().world.selected,
        None,
        "premise: nothing selected"
    );

    // **The control, and it is what makes the count below a measurement.** A quiet frame must
    // change nobody's visibility, or "one element flipped" would be this tree's ordinary churn.
    let before = visibility_snapshot(&mut app);
    app.frame();
    let quiet = flips(&before, &visibility_snapshot(&mut app));
    assert_eq!(
        quiet, 0,
        "premise: a settled gameplay tree flips nothing on its own"
    );

    let before = visibility_snapshot(&mut app);
    inject(&mut app, ia::ESCAPE_KEY);
    let moved = flips(&before, &visibility_snapshot(&mut app));

    let (dispatched, answered, ..) = app.probe().action_arm_host_stats();
    assert_eq!(dispatched, 1, "`App` performed the call the arm asked for");
    assert_eq!(
        answered, 1,
        "visibility-toggle dispatch answered true: action 0x1000001B has a listener bucket in the shipped `classic_gameplay` tree, while radar action 0x1000001E does not"
    );
    assert!(
        moved > 0,
        "and something in the live tree actually changed visibility: element message 0x31's \
         `0x58 == 1` arm. Counting the call alone is not enough: a dispatch that did nothing \
         would still count 1."
    );
}

/// Every element under the gameplay root and its visibility state, by handle.
fn visibility_snapshot(app: &mut dereth_client::app::App) -> Vec<(dereth_ui::ElemHandle, bool)> {
    let shell = app.ui_mut().expect("the shell");
    let Some(screen) = shell.flow.current_mut() else {
        return Vec::new();
    };
    let any: &mut dyn std::any::Any = &mut **screen;
    let Some(screen) = any.downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
    else {
        return Vec::new();
    };
    let Some(root) = screen.root() else {
        return Vec::new();
    };
    let ui = &shell.ui;
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(h) = stack.pop() {
        if let Some(n) = ui.node(h) {
            out.push((h, n.region.flags.visible));
        }
        stack.extend(ui.children(h));
    }
    out.sort_by_key(|(h, _)| format!("{h:?}"));
    out
}

fn flips(
    before: &[(dereth_ui::ElemHandle, bool)],
    after: &[(dereth_ui::ElemHandle, bool)],
) -> usize {
    assert_eq!(
        before.len(),
        after.len(),
        "the tree changed shape, not just visibility"
    );
    before
        .iter()
        .zip(after)
        .filter(|(a, b)| a.0 == b.0 && a.1 != b.1)
        .count()
}
