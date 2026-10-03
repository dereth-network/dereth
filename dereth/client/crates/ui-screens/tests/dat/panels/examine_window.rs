//! The examine window's close control is a shipped button and a click shuts it; the close arm keys
//! on message 1 and that element; clearing selection shuts it and moving it re-examines; closed
//! stays closed under the combat re-poll; contents still two lines.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;
use std::cell::RefCell;

use crate::common::*;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_ui::{ElementId, Screen, UiSystem};
use dereth_ui_screens::panels::examination::CLOSE_BUTTON;
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::view::{AppraisalView, GameView, SlotDecoration, UiRequest};

// =============================================================================================
// The live tree.
// =============================================================================================

fn env() -> UiSystem {
    let (ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    ui
}

fn pump(ui: &mut UiSystem, s: &mut GamePlayScreen) {
    for _ in 0..16 {
        let batch = ui.drain_outbox();
        if batch.is_empty() {
            return;
        }
        for d in batch {
            match d {
                dereth_ui::Delivery::Element { msg, .. } => {
                    s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg)
                }
                dereth_ui::Delivery::Global { id, param, .. } => {
                    s.on_global_message(&mut dereth_ui::framework::ScreenCx::new(ui), id, param)
                }
                dereth_ui::Delivery::Notice { id, payload, .. } => {
                    s.on_notice(&mut dereth_ui::framework::ScreenCx::new(ui), id, &payload)
                }
            }
        }
    }
    panic!("the message pump did not settle in 16 rounds");
}

fn screen() -> (UiSystem, GamePlayScreen) {
    let mut ui = env();
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds from the shipped layout");
    pump(&mut ui, &mut s);
    ui.requests.clear();
    (ui, s)
}

// =============================================================================================
// A host that answers exactly the four questions the panel asks.
// =============================================================================================

/// The subject: an appraisal the panel can be waiting for, a name so the weenie-object lookup guard
/// passes, an examine notice with a serial, and a selection.
#[derive(Debug, Default)]
struct Host {
    examine: Option<(ObjectId, u64)>,
    appraisal: Option<(ObjectId, AppraisalView)>,
    selected: Option<ObjectId>,
    /// How many times the panel asked for the appraisal — a denominator, so "the re-poll never
    /// arrived" and "it arrived and was ignored" are different answers.
    asked: RefCell<u32>,
}

impl Host {
    /// Arm a fresh examine of `id`: the notice, and the reply that answers it.
    fn examine(&mut self, id: ObjectId, serial: u64, delivery: u64) {
        self.examine = Some((id, serial));
        self.appraisal = Some((
            id,
            AppraisalView {
                delivery,
                value: Some(12_500),
                burden: Some(1_200),
                ..AppraisalView::default()
            },
        ));
    }

    /// 0.75-second combat re-poll: the **same** profile arriving again on a
    /// new `0x00C9`, with no new notice. Only `delivery` moves, which is what makes it a new reply
    /// to a pull-model panel and what the client's appraisal-info setter sees as not new.
    fn re_poll(&mut self, delivery: u64) {
        self.examine = None;
        let (_, p) = self
            .appraisal
            .as_mut()
            .expect("something must have been examined first");
        p.delivery = delivery;
    }
}

impl GameView for Host {
    fn examine_request(&self) -> Option<(ObjectId, u64)> {
        self.examine
    }
    fn appraisal(&self, id: ObjectId) -> Option<AppraisalView> {
        *self.asked.borrow_mut() += 1;
        self.appraisal
            .as_ref()
            .filter(|(i, _)| *i == id)
            .map(|(_, p)| p.clone())
    }
    fn name(&self, id: ObjectId) -> Option<&str> {
        self.appraisal
            .as_ref()
            .filter(|(i, _)| *i == id)
            .map(|_| "Shimmering Isparian Sword")
    }
    fn slot_decoration(&self, _id: ObjectId) -> Option<SlotDecoration> {
        Some(SlotDecoration {
            stack_size: 1,
            ..SlotDecoration::default()
        })
    }
    fn selected_object(&self) -> Option<ObjectId> {
        self.selected
    }
}

const SWORD: ObjectId = ObjectId(0x8000_1234);
const SHIELD: ObjectId = ObjectId(0x8000_5678);

/// One frame of `Hud::drive`'s examine slice.
fn frame(ui: &mut UiSystem, s: &mut GamePlayScreen, host: &Host) {
    s.examination.update(ui, host);
    pump(ui, s);
}

/// Open the window the way the player does: a notice, then the reply that answers it.
fn open(ui: &mut UiSystem, s: &mut GamePlayScreen, host: &mut Host, id: ObjectId, serial: u64) {
    host.examine(id, serial, serial * 10);
    frame(ui, s, host);
}

// =============================================================================================
// 1. The shipped close control.
// =============================================================================================

/// `0x100005F3` is a real button element inside `<EXAM>`, and `post_init` binds it.
///
/// The id is the client's ( compares
/// against it as a literal); everything else here is read out of the shipped tree. A layout without
/// the control would fail here rather than leaving a click that silently matches nothing.
#[test]
fn the_close_control_is_a_button_in_the_shipped_examine_window() {
    let (ui, s) = screen();
    let w = window_of(&ui, &s);
    let b = ui
        .get_child_recursive(w, CLOSE_BUTTON)
        .expect("0x100005F3 under <EXAM>");
    assert_eq!(CLOSE_BUTTON, ElementId(0x1000_05F3));
    assert_eq!(
        ui.node(b).expect("node").ty(),
        dereth_ui::factory::ty::BUTTON,
        "message 1 is a button element's own click message, so the control must be a button"
    );
    let box_ = ui.node(b).expect("node").region.box_;
    assert!(
        box_.width() > 0 && box_.height() > 0,
        "the control has no area: {box_:?}"
    );
    assert!(
        s.examination.close_control_bound(),
        "ExaminationPanel::post_init must bind the close control"
    );
}

// =============================================================================================
// 2. The edge: open, then closed, by a real click at a real pixel.
// =============================================================================================

/// A click on the close control shuts the examine window.
#[test]
fn a_click_on_the_close_control_shuts_the_examine_window() {
    let (mut ui, mut s) = screen();
    let w = window_of(&ui, &s);
    let mut host = Host::default();

    assert!(!is_open(&ui, w), "station 0: <EXAM> comes up hidden");
    assert_eq!(s.examination.opened, 0);
    assert_eq!(s.examination.closed, 0);

    open(&mut ui, &mut s, &mut host, SWORD, 1);
    assert!(
        is_open(&ui, w),
        "station 1: the awaited reply opens the window"
    );
    assert_eq!(
        s.examination.opened, 1,
        "and it is set_appraise_info's is-new arm that opened it"
    );
    assert_eq!(s.examination.closed, 0);

    // The gesture: press and release inside the 13x13 control, at its own screen pixels.
    let b = ui
        .get_child_recursive(w, CLOSE_BUTTON)
        .expect("the close control");
    let g = ui.screen_box(b);
    let (x, y) = ((g.x0 + g.x1) / 2, (g.y0 + g.y1) / 2);
    assert_eq!(
        ui.hit_test_screen(x, y)
            .and_then(|h| ui.node(h).map(|n| n.element_id())),
        Some(CLOSE_BUTTON),
        "the press must land on the close control and not on something over it"
    );
    ui.mouse_move(LocalTime(1.0), x, y);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, x, y);
    ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, x, y, false);
    pump(&mut ui, &mut s);

    assert!(!is_open(&ui, w), "station 2: the click closed the window");
    assert_eq!(s.examination.closed, 1, "and it closed exactly once");
    assert_eq!(s.examination.opened, 1, "nothing re-opened it");
}

/// The close arm is keyed on message 1 and on that element alone.
#[test]
fn the_close_arm_is_keyed_on_message_1_and_on_that_element_alone() {
    use dereth_ui::msg::element::id::{BUTTON_CLICKED, VISIBILITY_CHANGED};
    // The literal pin. The examine window's element-message handler compares the message id
    // against 1 and the source element against the close control's id.
    assert_eq!(
        BUTTON_CLICKED,
        dereth_ui::MessageId(1),
        "the message the client compares against"
    );
    assert_eq!(
        CLOSE_BUTTON,
        ElementId(0x1000_05F3),
        "the element the client compares against"
    );

    let (mut ui, mut s) = screen();
    let w = window_of(&ui, &s);
    let mut host = Host::default();
    let b = ui
        .get_child_recursive(w, CLOSE_BUTTON)
        .expect("the close control");

    open(&mut ui, &mut s, &mut host, SWORD, 1);
    assert!(is_open(&ui, w));

    // Another message on the right element: no.
    ui.broadcast_element_message(b, VISIBILITY_CHANGED, 1, 0);
    pump(&mut ui, &mut s);
    assert!(
        is_open(&ui, w),
        "only a click closes it, not any message on the control"
    );
    assert_eq!(s.examination.closed, 0);

    // The right message on the wrong element: no.
    ui.broadcast_element_message(w, BUTTON_CLICKED, 7, 0);
    pump(&mut ui, &mut s);
    assert!(
        is_open(&ui, w),
        "message 1 on the window itself is not the close control"
    );
    assert_eq!(s.examination.closed, 0);

    // The right message on the right element: yes.
    ui.broadcast_element_message(b, BUTTON_CLICKED, 7, 0);
    pump(&mut ui, &mut s);
    assert!(!is_open(&ui, w), "message 1 on 0x100005F3 closes it");
    assert_eq!(s.examination.closed, 1);
}

/// The other close path: **clearing the selection** shuts an open window, and moving the selection
/// to another object re-examines instead of closing.
///
/// The selection-changed notice, at three stations, so both arms of its inner `if` are
/// exercised and the outer visibility guard is exercised in both directions.
///
/// Falsified by: dropping the `selected_object` pull from `ExaminationPanel::update`; inverting the
/// non-zero selection test (the window would then close on a new selection and re-examine on none).
#[test]
fn clearing_the_selection_shuts_the_window_and_moving_it_re_examines() {
    let (mut ui, mut s) = screen();
    let w = window_of(&ui, &s);
    let mut host = Host {
        selected: Some(SWORD),
        ..Host::default()
    };

    frame(&mut ui, &mut s, &host); // the first frame only records; there is no change yet
    host.selected = None;
    frame(&mut ui, &mut s, &host); // a deselect, hidden
    assert!(!is_open(&ui, w));
    assert_eq!(
        s.examination.closed, 0,
        "a hidden window cannot be closed again"
    );
    host.selected = Some(SHIELD);
    frame(&mut ui, &mut s, &host); // a *new selection*, hidden -- the discriminating case
    assert!(
        !is_open(&ui, w),
        "a selection does not open the window; only set_appraise_info does"
    );
    assert_eq!(
        ui.requests.take(),
        vec![],
        "a hidden panel must not re-examine -- that is the IsVisible guard "
    );
    host.selected = None;
    frame(&mut ui, &mut s, &host);
    let _ = ui.requests.take();

    // Station 1: open it, with a selection standing.
    host.selected = Some(SWORD);
    open(&mut ui, &mut s, &mut host, SWORD, 1);
    let _ = ui.requests.take();
    assert!(is_open(&ui, w));

    // Station 2: the selection moves to another object -> re-examine, do NOT close.
    host.selected = Some(SHIELD);
    frame(&mut ui, &mut s, &host);
    assert!(
        is_open(&ui, w),
        "moving the selection must not close the window"
    );
    assert_eq!(
        ui.requests.take(),
        vec![UiRequest::Examine(SHIELD)],
        "changing the selection requests an examination of the newly selected object"
    );
    assert_eq!(s.examination.closed, 0);

    // Station 3: the selection is cleared -> close.
    host.selected = None;
    frame(&mut ui, &mut s, &host);
    assert!(!is_open(&ui, w), "a cleared selection closes the window");
    assert_eq!(s.examination.closed, 1);
    assert!(
        ui.requests.take().is_empty(),
        "the else-arm sends nothing; it only hides"
    );
}

// =============================================================================================
// 3. The guard that must not regress.
// =============================================================================================

/// Behaviour: examine.window.the-close-control-and-a-deselect-shut-it-and-the-combat-re-poll-does-not-reopen-it
/// **The landed guard, re-asserted after the change.** A panel the player closed stays closed while
/// the 0.75-second combat re-poll keeps answering.
///
/// The appraise-info setter shows the panel only when the info is new, and
/// new means the id is the one awaiting appraisal. A re-poll matches the current appraisal id
/// instead, so it refills and does not re-show. Neither close path clears those two fields, which
/// is what keeps that true.
///
/// **The control is the point of the test** under the live-run calibration rule: a re-poll that never
/// arrived would also leave the window shut. So the reply count is asserted to have moved and the
/// panel's `replies_applied` to have moved with it — the window stayed shut *while being refilled*,
/// not because nothing happened.
///
/// Falsified by: clearing `current` (or setting `awaiting`) in either close path; making
/// `set_appraise_info` show the window unconditionally.
#[test]
fn a_closed_window_stays_closed_under_the_combat_re_poll() {
    let (mut ui, mut s) = screen();
    let w = window_of(&ui, &s);
    let mut host = Host::default();

    open(&mut ui, &mut s, &mut host, SWORD, 1);
    assert!(is_open(&ui, w));
    let applied_at_open = s.examination.replies_applied;
    assert_eq!(applied_at_open, 1);

    // Close it with its own control.
    let b = ui
        .get_child_recursive(w, CLOSE_BUTTON)
        .expect("the close control");
    let g = ui.screen_box(b);
    let (x, y) = ((g.x0 + g.x1) / 2, (g.y0 + g.y1) / 2);
    ui.mouse_move(LocalTime(1.0), x, y);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, x, y);
    ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, x, y, false);
    pump(&mut ui, &mut s);
    assert!(!is_open(&ui, w), "closed");

    // Eight re-polls -- more than ten frames of `UseTime`'s 0.75 s cadence would give a player who
    // closed the panel mid-fight.
    for i in 0..8 {
        host.re_poll(100 + i);
        frame(&mut ui, &mut s, &host);
        assert!(
            !is_open(&ui, w),
            "re-poll {i} re-opened a window the player closed"
        );
    }

    // The control: the re-polls really did land, and really did refill the panel.
    assert_eq!(
        s.examination.replies_applied,
        applied_at_open + 8,
        "the re-polls must have been applied -- otherwise this test proves nothing"
    );
    assert_eq!(
        s.examination.opened, 1,
        "SetVisible(true) ran once, for the awaited id only"
    );
    assert_eq!(s.examination.closed, 1);
    assert!(
        *host.asked.borrow() >= 9,
        "the panel asked the view about the appraisal every frame"
    );

    // And a **new** examine of a different object does re-open it: the close is not a latch.
    open(&mut ui, &mut s, &mut host, SHIELD, 2);
    assert!(is_open(&ui, w), "a fresh examine re-opens a closed window");
    assert_eq!(s.examination.opened, 2);
}

/// The same guard for the *other* close path, because the two clear different amounts of state and
/// only one of them was written with the re-poll in mind.
#[test]
fn a_window_closed_by_a_deselect_also_stays_closed_under_the_re_poll() {
    let (mut ui, mut s) = screen();
    let w = window_of(&ui, &s);
    let mut host = Host {
        selected: Some(SWORD),
        ..Host::default()
    };

    open(&mut ui, &mut s, &mut host, SWORD, 1);
    let _ = ui.requests.take();
    assert!(is_open(&ui, w));

    host.selected = None;
    frame(&mut ui, &mut s, &host);
    assert!(!is_open(&ui, w));
    let applied = s.examination.replies_applied;

    for i in 0..8 {
        host.re_poll(200 + i);
        frame(&mut ui, &mut s, &host);
        assert!(
            !is_open(&ui, w),
            "re-poll {i} re-opened a deselect-closed window"
        );
    }
    assert_eq!(
        s.examination.replies_applied,
        applied + 8,
        "the re-polls landed -- the control for this test too"
    );
    assert_eq!(s.examination.opened, 1);
}

/// The examine pane shows the item name value and burden.
#[test]
fn the_examine_pane_shows_the_item_name_value_and_burden() {
    let (mut ui, mut s) = screen();
    let w = window_of(&ui, &s);
    let mut host = Host::default();
    open(&mut ui, &mut s, &mut host, SWORD, 1);
    assert!(is_open(&ui, w));
    assert_eq!(
        s.examination.title_text.as_deref(),
        Some("Shimmering Isparian Sword"),
        "the title text was set"
    );
    assert_eq!(
        s.examination.item_text.as_deref(),
        Some("Value: 12,500\nBurden: 1,200\n\n"),
        "The appraisal includes value, burden and a trailing blank line"
    );
}

mod selection_guard {
    //! The examine-newly-selected guard: ordinary selection change re-examines; a component the panel
    //! selected is not re-examined but the next is; the guard covers close as well; an unresolved
    //! component does not lower it; component list id/message match the shipped image.
    //! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

    use crate::common::layout::RegistrationOrder;
    use std::cell::RefCell;

    use crate::common::*;
    use dereth_primitives::ObjectId;
    use dereth_ui::{ElementId, Screen, UiSystem};
    use dereth_ui_screens::panels::examination::{
        SPELL_COMPONENT_LIST, SPELL_COMPONENT_LIST_MESSAGE,
    };
    use dereth_ui_screens::screens::gameplay::GamePlayScreen;
    use dereth_ui_screens::view::{AppraisalView, GameView, SlotDecoration, UiRequest};

    // =============================================================================================
    // The live tree — the same harness `examine_window` uses, because it is the same window.
    // =============================================================================================

    fn env() -> UiSystem {
        let (ui, _flow, _store) =
            crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
        ui
    }

    fn pump(ui: &mut UiSystem, s: &mut GamePlayScreen) {
        for _ in 0..16 {
            let batch = ui.drain_outbox();
            if batch.is_empty() {
                return;
            }
            for d in batch {
                match d {
                    dereth_ui::Delivery::Element { msg, .. } => {
                        s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg)
                    }
                    dereth_ui::Delivery::Global { id, param, .. } => {
                        s.on_global_message(&mut dereth_ui::framework::ScreenCx::new(ui), id, param)
                    }
                    dereth_ui::Delivery::Notice { id, payload, .. } => {
                        s.on_notice(&mut dereth_ui::framework::ScreenCx::new(ui), id, &payload)
                    }
                }
            }
        }
        panic!("the message pump did not settle in 16 rounds");
    }

    fn screen() -> (UiSystem, GamePlayScreen) {
        let mut ui = env();
        let mut s = GamePlayScreen::default();
        s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
            .expect("the gameplay screen builds from the shipped layout");
        pump(&mut ui, &mut s);
        ui.requests.clear();
        (ui, s)
    }

    #[derive(Debug, Default)]
    struct Host {
        examine: Option<(ObjectId, u64)>,
        appraisal: Option<(ObjectId, AppraisalView)>,
        selected: Option<ObjectId>,
        asked: RefCell<u32>,
    }

    impl Host {
        fn examine(&mut self, id: ObjectId, serial: u64, delivery: u64) {
            self.examine = Some((id, serial));
            self.appraisal = Some((
                id,
                AppraisalView {
                    delivery,
                    value: Some(12_500),
                    burden: Some(1_200),
                    ..AppraisalView::default()
                },
            ));
        }
    }

    impl GameView for Host {
        fn examine_request(&self) -> Option<(ObjectId, u64)> {
            self.examine
        }
        fn appraisal(&self, id: ObjectId) -> Option<AppraisalView> {
            *self.asked.borrow_mut() += 1;
            self.appraisal
                .as_ref()
                .filter(|(i, _)| *i == id)
                .map(|(_, p)| p.clone())
        }
        fn name(&self, id: ObjectId) -> Option<&str> {
            self.appraisal
                .as_ref()
                .filter(|(i, _)| *i == id)
                .map(|_| "Shimmering Isparian Sword")
        }
        fn slot_decoration(&self, _id: ObjectId) -> Option<SlotDecoration> {
            Some(SlotDecoration {
                stack_size: 1,
                ..SlotDecoration::default()
            })
        }
        fn selected_object(&self) -> Option<ObjectId> {
            self.selected
        }
    }

    const SPELL: ObjectId = ObjectId(0x8000_1234);
    const COMPONENT: ObjectId = ObjectId(0x8000_00AA);
    const SHIELD: ObjectId = ObjectId(0x8000_5678);

    fn frame(ui: &mut UiSystem, s: &mut GamePlayScreen, host: &Host) -> Vec<UiRequest> {
        s.examination.update(ui, host);
        pump(ui, s);
        ui.requests.take()
    }

    /// Open `<EXAM>` on `id`, exactly as `examine_window` does, and leave the selection on it so
    /// the panel is in the state the spell pane would be clicked in.
    fn open_on(ui: &mut UiSystem, s: &mut GamePlayScreen, host: &mut Host, id: ObjectId) {
        host.examine(id, 1, 10);
        host.selected = Some(id);
        let _ = frame(ui, s, host);
        host.examine = None;
        assert!(
            is_open(ui, window_of(ui, s)),
            "the awaited reply opens the window"
        );
    }

    fn examines(r: &[UiRequest]) -> Vec<ObjectId> {
        r.iter()
            .filter_map(|x| {
                if let UiRequest::Examine(i) = x {
                    Some(*i)
                } else {
                    None
                }
            })
            .collect()
    }

    // =============================================================================================
    // 1. The control: without the guard, a selection change re-examines.
    // =============================================================================================

    /// The selection-changed notice's re-examine arm, with the flag in its constructor
    /// state (`true`, the last line of the panel's constructor).
    ///
    /// This is the positive that makes every negative below a measurement: an instrument that could
    /// never emit an `Examine` would pass the guard tests for free.
    #[test]
    fn an_ordinary_selection_change_re_examines_while_the_flag_is_up() {
        let (mut ui, mut s) = screen();
        let mut host = Host::default();
        open_on(&mut ui, &mut s, &mut host, SPELL);
        assert!(
            s.examination.examine_newly_selected_item,
            "the constructor leaves it true"
        );

        host.selected = Some(SHIELD);
        let out = frame(&mut ui, &mut s, &host);
        assert_eq!(
            examines(&out),
            vec![SHIELD],
            "the panel re-examines the new selection"
        );
        assert_eq!(
            s.examination.self_selections_absorbed, 0,
            "and it was not its own selection"
        );
        assert!(
            is_open(&ui, window_of(&ui, &s)),
            "and the window is still open"
        );
    }

    // =============================================================================================
    // 2. The clear, and the restore.
    // =============================================================================================

    /// Behaviour: examine.window.a-component-the-spell-pane-selected-is-not-re-examined
    /// A component this panel selected is not re examined and the next one still is.
    #[test]
    fn a_component_this_panel_selected_is_not_re_examined_and_the_next_one_still_is() {
        let (mut ui, mut s) = screen();
        let mut host = Host::default();
        open_on(&mut ui, &mut s, &mut host, SPELL);

        // The clear, and the select-object call it brackets.
        let req = s
            .examination
            .select_spell_component(COMPONENT)
            .expect("a non-zero component object");
        assert_eq!(
            req,
            UiRequest::Select(COMPONENT),
            "the arm's one call selects the object"
        );
        assert!(
            !s.examination.examine_newly_selected_item,
            "the flag is down for the broadcast"
        );

        // The broadcast, as this build delivers it: the next frame's pull sees the new selection.
        host.selected = Some(COMPONENT);
        let out = frame(&mut ui, &mut s, &host);
        assert!(
            examines(&out).is_empty(),
            "the guard is what stops the panel re-examining what it just selected: {out:?}"
        );
        assert_eq!(
            s.examination.self_selections_absorbed, 1,
            "and the absorb is counted"
        );
        assert!(is_open(&ui, window_of(&ui, &s)), "nor did it close");
        // The restore, on the far side of the same broadcast.
        assert!(
            s.examination.examine_newly_selected_item,
            "the flag is back up"
        );

        // The other direction, in the same run: an ordinary change still re-examines.
        host.selected = Some(SHIELD);
        let out = frame(&mut ui, &mut s, &host);
        assert_eq!(
            examines(&out),
            vec![SHIELD],
            "the guard was a bracket, not a latch"
        );
        assert_eq!(
            s.examination.self_selections_absorbed, 1,
            "and this one was not absorbed"
        );
    }

    /// The guard also protects the *close* arm, which shares the same `if`.
    ///
    /// The selection-changed handler's two arms are one branch: a non-zero selected id is
    /// examined, a zero one hides the window, both behind "visible and flag set". A component
    /// click that resolved to nothing would otherwise shut the window it was clicked in.
    #[test]
    fn the_guard_covers_the_close_arm_as_well_as_the_re_examine_arm() {
        let (mut ui, mut s) = screen();
        let mut host = Host::default();
        open_on(&mut ui, &mut s, &mut host, SPELL);
        let closed_before = s.examination.closed;

        let _ = s
            .examination
            .select_spell_component(COMPONENT)
            .expect("a component");
        host.selected = None;
        let _ = frame(&mut ui, &mut s, &host);
        assert!(
            is_open(&ui, window_of(&ui, &s)),
            "the guard held the close arm too"
        );
        assert_eq!(s.examination.closed, closed_before);

        // …and with the guard back up, clearing the selection does close it — the control that makes
        // the assertion above a measurement.
        host.selected = Some(SHIELD);
        let _ = frame(&mut ui, &mut s, &host);
        host.selected = None;
        let _ = frame(&mut ui, &mut s, &host);
        assert!(
            !is_open(&ui, window_of(&ui, &s)),
            "an unguarded deselection closes <EXAM>"
        );
        assert_eq!(s.examination.closed, closed_before + 1);
    }

    /// The arm returns early when the component-object id lookup answers 0. **Neither** write happens, so
    /// a click on a row that is not a component object must not lower the guard.
    ///
    /// This is the arm that would turn the bracket into the latch the row described: a clear with no
    /// restore behind it.
    #[test]
    fn a_component_that_resolves_to_nothing_does_not_lower_the_guard() {
        let (mut ui, mut s) = screen();
        let mut host = Host::default();
        open_on(&mut ui, &mut s, &mut host, SPELL);

        assert_eq!(
            s.examination.select_spell_component(ObjectId(0)),
            None,
            "the `objId == 0` guard"
        );
        assert!(
            s.examination.examine_newly_selected_item,
            "and the flag was not touched"
        );

        host.selected = Some(SHIELD);
        let out = frame(&mut ui, &mut s, &host);
        assert_eq!(
            examines(&out),
            vec![SHIELD],
            "so the next change still re-examines"
        );
        assert_eq!(s.examination.self_selections_absorbed, 0);
    }

    /// The two literals the arm compares against, pinned once from retail so a wrong id is
    /// falsifiable rather than self-consistent.
    ///
    /// The arm's element id is reached in three steps, `0x10000137 + 7 + 0x1EF == 0x1000032D`; the
    /// message the arm tests for is `4`.
    #[test]
    fn the_component_lists_id_and_message_are_the_ones_in_the_shipped_image() {
        assert_eq!(SPELL_COMPONENT_LIST, ElementId(0x1000_032D));
        assert_eq!(SPELL_COMPONENT_LIST_MESSAGE, 4);
        assert_eq!(ElementId(0x1000_0137 + 7 + 0x1EF), SPELL_COMPONENT_LIST);
        let (ui, s) = screen();
        let w = window_of(&ui, &s);
        let list = ui
            .get_child_recursive(w, SPELL_COMPONENT_LIST)
            .expect("0x1000032D is a child of <EXAM> in the shipped layout");
        let box_ = ui.node(list).expect("node").region.box_;
        assert!(
            box_.width() > 0 && box_.height() > 0,
            "and it has area: {box_:?}"
        );
    }
}

// =============================================================================================
// The item pane has no picture of the item.
// =============================================================================================

/// The item pane would put the examined item's icon in `0x1000013A`, but the shipped floating
/// window's item pane has no such element (only the docked panel the window replaced had one), so
/// examining an item shows its text and no icon.
/// Behaviour: examine.item.the-floating-windows-item-pane-shows-no-icon-of-the-item
#[test]
fn examining_an_item_shows_no_icon_because_the_shipped_item_pane_has_no_place_for_one() {
    use dereth_primitives::AssetSource as _;
    use dereth_ui_screens::panels::examination::ITEM_BASE;
    const ITEM_ICON: ElementId = ElementId(0x1000_013A);

    let (mut ui, mut s) = screen();
    let w = window_of(&ui, &s);
    let base = ui
        .get_child_recursive(w, ITEM_BASE)
        .expect("the shipped window has an item pane");
    assert!(
        ui.get_child_recursive(w, ITEM_ICON).is_none(),
        "the shipped window has no item icon element"
    );

    // The element exists in the shipped dats, but only in the older docked panel's layout.
    let (_, _, store) = crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    let did = dereth_primitives::DataId(0x2100_001C);
    let old = dereth_ui::desc::LayoutDesc::read(did, &store.read(did).unwrap(), &ui.property_types)
        .expect("the docked panel's layout decodes");
    fn has(d: &dereth_ui::ElementDesc, id: ElementId) -> bool {
        d.element_id == id || d.children.values().any(|c| has(c, id))
    }
    assert!(
        old.elements.values().any(|d| has(d, ITEM_ICON)),
        "the docked panel had a place for the icon, so the id is the right one"
    );

    let mut host = Host::default();
    open(&mut ui, &mut s, &mut host, SWORD, 1);
    assert!(is_open(&ui, w), "the window opened on the item");
    let mut pictures = Vec::new();
    let mut q = vec![base];
    while let Some(h) = q.pop() {
        if let Some(g) = ui.node(h).and_then(|n| n.region.image.as_ref()) {
            if g.source != dereth_ui::region::ImageSource::Interface {
                pictures.push(g.clone());
            }
        }
        q.extend(ui.children(h));
    }
    assert!(pictures.is_empty(), "nothing drew the item: {pictures:?}");
}
