//! Client Options control table ids match the shipped layout; eight row templates; slider drag
//! writes (scaled) values each tick; live and static defaults agree; cancel/close revert changed
//! controls; unticking a sound box greys its slider; input actions reach all 40 registered
//! elements.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use dereth_ui::framework::{LayoutEnum, Screen};
use dereth_ui::msg::Delivery;
use dereth_ui::{ElemHandle, ElementId, UiSystem};

use crate::common::*;
use dereth_ui_screens::env::create_and_add_root_element;
use dereth_ui_screens::options::page::{OptionControl, PlayerOptionPage};
use dereth_ui_screens::options::{config, page, pages};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::{PrefValue, UiRequest};

fn env() -> UiSystem {
    let (ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    ui
}

/// The shipped gameplay screen with `GamePlayScreen::create` run — which is
/// creation of the root element plus every window's initialization, including
/// the config page's own post-init.
fn screen() -> (UiSystem, GamePlayScreen) {
    let mut ui = env();
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds");
    (ui, s)
}

/// Every element message the tree raised, delivered to the screen the way `UiShell::frame` does:
/// `drain_outbox`, then `Screen::on_element_message` for each `Delivery::Element`.
///
/// **This is what makes the drag test a wire test rather than a hand-write.** Nothing below calls
/// the option page's element-message handler directly; the message has to come out of the scrollbar,
/// bubble to the screen root's registration and arrive here on its own.
fn pump(ui: &mut UiSystem, s: &mut GamePlayScreen) -> usize {
    let mut n = 0;
    for d in ui.drain_outbox() {
        if let Delivery::Element { msg, .. } = d {
            s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
            n += 1;
        }
    }
    n
}

/// The handle of the option control bound to `preference`, from the live array.
fn control(s: &PlayerOptionPage, preference: &str) -> (usize, ElemHandle) {
    let i = s
        .options
        .iter()
        .position(|o| o.preference == preference)
        .unwrap_or_else(|| panic!("{preference} is not on the built page"));
    (i, s.options[i].element)
}

// -------------------------------------------------------------------------------------------
// The transcribed ids, against the shipped layout
// -------------------------------------------------------------------------------------------

/// Every child id in the control table is the one the shipped layout carries.
#[test]
fn every_child_id_in_the_control_table_is_the_one_the_shipped_layout_carries() {
    let mut ui = env();
    let root =
        create_and_add_root_element(&mut ui, LayoutEnum(0x1000_0006), ElementId(0x1000_0495))
            .expect("classic_gameplay builds");
    let mut all = Vec::new();
    walk(&ui, root, &mut all);
    let page = all
        .iter()
        .copied()
        .find(|h| {
            ui.node(*h)
                .is_some_and(|n| n.element_id() == config::CONFIG_PAGE_ELEMENT)
        })
        .expect("ClientOptionsPanel");
    let box_h = ui
        .get_child_recursive(page, config::OPTION_BOX)
        .expect("option box");
    let mut lb = dereth_ui_screens::panels::listbox::ListBoxWidget::bind(&ui, box_h);
    assert_eq!(lb.templates.len(), 8, "the option box's template list");

    // Build one row of every template and record what child ids and types it carries.
    let mut seen: Vec<(usize, u32, u32)> = Vec::new();
    for i in 0..lb.templates.len() {
        let h = lb
            .add_from_template(&mut ui, i, None)
            .expect("template builds");
        let mut all = Vec::new();
        walk(&ui, h, &mut all);
        for e in all {
            let Some(n) = ui.node(e) else { continue };
            seen.push((i, n.element_id().0, n.ty().0));
        }
    }
    let has = |id: u32, ty: u32| seen.iter().any(|(_, e, t)| *e == id && *t == ty);

    // The five rows of `OPTION_CONTROLS` that name a child, with the type each must be.
    let table = [
        ("SliderOption", 0x1000_021Cu32, 0x1000_0037u32),
        ("CheckboxSliderOption", 0x1000_0219, 0x1000_0035),
        ("MenuOption", 0x1000_0224, 0x1000_0038),
        ("WideBitfieldCheckboxOption", 0x1000_0219, 0x1000_0035),
    ];
    for (class, id, ty) in table {
        let row = pages::OPTION_CONTROLS
            .iter()
            .find(|c| c.class == class)
            .unwrap();
        assert_eq!(row.child, Some(ElementId(id)), "{class}'s child id");
        assert!(
            has(id, ty),
            "{class}: {id:#010X} is not type {ty:#010X} in the shipped layout"
        );
    }

    assert!(
        has(0x1000_021B, 0x0C),
        "0x1000021B is the slider row's caption"
    );
    assert!(
        has(0x1000_0223, 0x0C),
        "0x10000223 is the menu row's caption"
    );
    assert!(!has(0x1000_021B, 0x1000_0037), "0x1000021B is not a slider");
    assert!(!has(0x1000_0223, 0x1000_0038), "0x10000223 is not a menu");
    // The wide slider's two end captions, which only the wide template carries.
    assert!(has(0x1000_021E, 0x0C));
    assert!(has(0x1000_021F, 0x0C));
}

/// Oracle: the option box's own template list (element property `0x64`) in the shipped
/// `classic_gameplay` tree, and the slider option's own template index: 6 when wide, 3 when not.
///
/// The load-bearing half is the last two assertions: template **6** is the row that carries
/// `0x1000021E`/`0x1000021F`, and template **3** is not. `14` §1.4 and this crate both had
/// `wide ? 3 : 6`, so a wide slider would have been built from the caption-less template and the
/// two end captions on every slider row of the page would have been silently dropped.
#[test]
fn the_eight_row_templates_are_the_shipped_ones_and_only_the_wide_slider_has_end_captions() {
    let mut ui = env();
    let root =
        create_and_add_root_element(&mut ui, LayoutEnum(0x1000_0006), ElementId(0x1000_0495))
            .expect("classic_gameplay builds");
    let mut all = Vec::new();
    walk(&ui, root, &mut all);
    let page = all
        .iter()
        .copied()
        .find(|h| {
            ui.node(*h)
                .is_some_and(|n| n.element_id() == config::CONFIG_PAGE_ELEMENT)
        })
        .expect("ClientOptionsPanel");
    let box_h = ui
        .get_child_recursive(page, config::OPTION_BOX)
        .expect("option box");
    let templates = dereth_ui_screens::panels::listbox::template_list(&ui, box_h);
    let roots: Vec<u32> = templates.iter().map(|(_, e)| e.0).collect();
    assert_eq!(
        roots,
        vec![
            0x1000_0216, // 0 header
            0x1000_0217, // 1 separator
            0x1000_0218, // 2 toggle
            0x1000_021A, // 3 narrow slider
            0x1000_0222, // 4 menu
            0x1000_0220, // 5 toggle + slider
            0x1000_021D, // 6 wide slider
            0x1000_0221, // 7 toggle + wide slider (no helper uses it)
        ],
        "the shipped template list, in index order"
    );
    // Every template comes out of the same layout, `classic_options`.
    for (did, _) in &templates {
        assert_eq!(
            did.0, 0x2100_002B,
            "the templates all live in classic_options"
        );
    }

    let mut lb = dereth_ui_screens::panels::listbox::ListBoxWidget::bind(&ui, box_h);
    let captions = |ui: &UiSystem, h: ElemHandle| {
        let mut all = Vec::new();
        walk(ui, h, &mut all);
        all.iter()
            .filter_map(|e| ui.node(*e))
            .filter(|n| {
                n.element_id() == ElementId(0x1000_021E) || n.element_id() == ElementId(0x1000_021F)
            })
            .count()
    };
    let wide = lb
        .add_from_template(&mut ui, page::template::SLIDER_WIDE, None)
        .unwrap();
    assert_eq!(
        captions(&ui, wide),
        2,
        "template 6 is the row with the two end captions"
    );
    let narrow = lb
        .add_from_template(&mut ui, page::template::SLIDER_NARROW, None)
        .unwrap();
    assert_eq!(captions(&ui, narrow), 0, "template 3 has none");
}

// -------------------------------------------------------------------------------------------
// The slider
// -------------------------------------------------------------------------------------------

/// Behaviour: options.client-page.a-slider-drag-writes-the-preference-every-tick
/// Dragging the sound volume slider writes the preference on every tick.
#[test]
fn dragging_the_sound_volume_slider_writes_the_preference_on_every_tick() {
    let (mut ui, mut s) = screen();
    let (_, bar) = control(&s.config_page, "Sound.SoundVolume");
    // The bar has to be on screen to be hit-tested; the options page is one of sixteen stacked
    // pages and starts hidden. A panel-visibility notice shows it.
    let mut h = bar;
    loop {
        ui.set_visible(h, true);
        match ui.parent(h) {
            Some(p) => h = p,
            None => break,
        }
    }
    ui.drain_outbox();

    let b = ui.node(bar).expect("the bar").region.box_;
    let (ox, oy) = ui.screen_origin(bar);
    assert!(b.width() > 40, "the bar is {}px wide", b.width());
    let y = oy + b.height() / 2;

    // Two presses at two places, and the values they produce.
    let press = |ui: &mut UiSystem, s: &mut GamePlayScreen, frac: f32| -> Vec<UiRequest> {
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        let x = ox + (b.width() as f32 * frac) as i32;
        ui.requests.clear();
        ui.mouse_down(7, x, y);
        ui.mouse_up(7, x, y, false);
        pump(ui, s);
        ui.requests.take()
    };

    let low = press(&mut ui, &mut s, 0.10);
    let low_v = sole_volume(&low);
    let high = press(&mut ui, &mut s, 0.90);
    let high_v = sole_volume(&high);

    assert!(
        low_v < high_v,
        "left press {low_v} should be quieter than right press {high_v}"
    );
    assert!(
        low_v < 0.35,
        "a press near the left end should be near 0, got {low_v}"
    );
    assert!(
        high_v > 0.65,
        "a press near the right end should be near 1, got {high_v}"
    );

    // The value the page wrote is the bar's own reported position mapped onto the registered
    // range — `Sound.SoundVolume` is 0.0..1.0, so the two are the same number.
    let pos = dereth_ui::widgets::scrollbar::Scrollbar::position(&ui, bar);
    assert!(
        (pos - high_v).abs() < 1e-6,
        "position {pos} vs written {high_v}"
    );
    assert_eq!(page::slider_range("Sound.SoundVolume"), (0.0, 1.0));

    // …and the page's own `current` agrees with what it emitted.
    let (i, _) = control(&s.config_page, "Sound.SoundVolume");
    assert_eq!(s.config_page.options[i].current, PrefValue::Float(high_v));
    assert!(
        s.config_page.options[i].changed(),
        "saved is still the default 1.0"
    );
}

/// The one `Sound.SoundVolume` write in a batch, and an assertion that it is the only preference
/// the gesture touched — a drag that also rewrote seven other preferences would be a defect a
/// "did it move" test cannot see.
fn sole_volume(out: &[UiRequest]) -> f32 {
    let writes: Vec<&UiRequest> = out
        .iter()
        .filter(|r| matches!(r, UiRequest::SetPreference(..)))
        .collect();
    assert_eq!(writes.len(), 1, "one drag, one preference write: {out:?}");
    match writes[0] {
        UiRequest::SetPreference("Sound.SoundVolume", PrefValue::Float(v)) => *v,
        other => panic!("expected a Sound.SoundVolume float, got {other:?}"),
    }
}

/// A slider whose range is **not** `0..1` maps the same normalised position onto a different
/// number, and this is the assertion that would catch a rebuild that forgot
/// `inq_preference_range` and wrote the raw `0x86` through.
///
/// `Camera.AdjustmentSpeed` is registered `5.0 … 80.0`, so the middle of its bar is **42.5** and
/// not 0.5. A slider that wrote 0.5 would look identical on screen and be sixteen times too slow.
#[test]
fn a_slider_whose_range_is_not_zero_to_one_writes_the_scaled_value() {
    let (mut ui, mut s) = screen();
    let (_, bar) = control(&s.config_page, "Camera.AdjustmentSpeed");
    let mut h = bar;
    loop {
        ui.set_visible(h, true);
        match ui.parent(h) {
            Some(p) => h = p,
            None => break,
        }
    }
    ui.drain_outbox();
    let b = ui.node(bar).expect("the bar").region.box_;
    let (ox, oy) = ui.screen_origin(bar);
    ui.requests.clear();
    let x = ox + b.width() / 2;
    ui.mouse_down(7, x, oy + b.height() / 2);
    ui.mouse_up(7, x, oy + b.height() / 2, false);
    pump(&mut ui, &mut s);
    let out = ui.requests.take();
    let v = match out
        .iter()
        .find(|r| matches!(r, UiRequest::SetPreference("Camera.AdjustmentSpeed", _)))
    {
        Some(UiRequest::SetPreference(_, PrefValue::Float(v))) => *v,
        other => panic!("no Camera.AdjustmentSpeed write in {other:?} / {out:?}"),
    };
    let pos = dereth_ui::widgets::scrollbar::Scrollbar::position(&ui, bar);
    let expected = 5.0 + pos * (80.0 - 5.0);
    assert!(
        (v - expected).abs() < 1e-3,
        "wrote {v}, range says {expected} at position {pos}"
    );
    assert!(v > 5.0 && v < 80.0, "{v} is outside the registered range");
    // The literal, so a wrong range constant cannot hide behind the symbol.
    assert_eq!(page::slider_range("Camera.AdjustmentSpeed"), (5.0, 80.0));
}

// -------------------------------------------------------------------------------------------
// The page's four fan-outs
// -------------------------------------------------------------------------------------------

/// The live option-page array and the static helper
/// `config::restore_default_values()` produce the same 30 writes, in the same order.
///
/// This is the cross-check that keeps the two from drifting: the static table is
/// `SetDefault` arguments read out by hand, and the live array is the
/// same arguments carried through eight row templates, four helper functions and a control type check.
/// They can only agree if every row was built with the right control on the right preference.
#[test]
fn the_live_option_array_and_the_static_row_table_agree_on_every_default() {
    let (mut ui, mut s) = screen();
    ui.requests.clear();
    let n = s.config_page.restore_default_values(&mut ui);
    assert_eq!(n, 30);
    let live: Vec<(&str, PrefValue)> = ui
        .requests
        .take()
        .into_iter()
        .filter_map(|r| match r {
            UiRequest::SetPreference(k, v) => Some((k, v)),
            _ => None,
        })
        .collect();
    let stat: Vec<(&str, PrefValue)> = config::restore_default_values();
    assert_eq!(live, stat, "the built page and CONFIG_PAGE disagree");
    assert_eq!(live.len(), 30);

    // The control kinds, counted: 12 check boxes, 10 sliders, 8 menus.
    let k = |c: OptionControl| {
        s.config_page
            .options
            .iter()
            .filter(|o| o.control == c)
            .count()
    };
    assert_eq!(k(OptionControl::Checkbox), 12);
    assert_eq!(k(OptionControl::Slider), 10);
    assert_eq!(k(OptionControl::Menu), 8);
    assert_eq!(
        s.config_page.gated.len(),
        3,
        "the three sound check+slider pairs"
    );
}

/// Behaviour: options.client-page.cancel-and-close-revert-only-what-changed
/// *"changed, then restore the saved value **on the
/// ones that changed**"*.
///
/// Both halves are asserted, because a Cancel that rewrote every control would pass a test that
/// only checked the changed one: after moving one slider, Cancel emits **exactly one** write, and
/// it is the old value.
#[test]
fn cancel_writes_the_old_value_back_and_only_for_the_controls_that_changed() {
    let (mut ui, mut s) = screen();
    let (i, _) = control(&s.config_page, "Sound.AmbientSoundVolume");
    let before = s.config_page.options[i].current.clone();
    assert_eq!(before, PrefValue::Float(1.0));

    s.config_page.options[i].current = PrefValue::Float(0.25);
    assert!(s.config_page.changed());

    ui.requests.clear();
    let n = s.config_page.restore_saved_values(&mut ui);
    assert_eq!(n, 1, "one control changed, so one is restored");
    let out = ui.requests.take();
    assert_eq!(
        out,
        vec![UiRequest::SetPreference(
            "Sound.AmbientSoundVolume",
            PrefValue::Float(1.0)
        )]
    );
    assert!(!s.config_page.changed());

    // With nothing changed, Cancel writes nothing at all.
    ui.requests.clear();
    assert_eq!(s.config_page.restore_saved_values(&mut ui), 0);
    assert!(ui.requests.take().is_empty());
}

/// The option page's visibility change — **the part a rebuild is most likely to
/// skip**, in the row's own words: hiding the page calls `restore_saved_values`, so a change the
/// player did not Apply is rolled back.
///
/// Driven through the element message raises, not by calling
/// the handler: a hidden producer is the defect, not the handler.
#[test]
fn closing_the_page_reverts_an_uncommitted_change() {
    let (mut ui, mut s) = screen();
    let page = ui
        .get_child_recursive(s.root().unwrap(), config::CONFIG_PAGE_ELEMENT)
        .expect("ClientOptionsPanel is in the tree");
    ui.set_visible(page, true);
    ui.drain_outbox();

    let (i, _) = control(&s.config_page, "Sound.SoundVolume");
    s.config_page.options[i].current = PrefValue::Float(0.4);

    ui.requests.clear();
    ui.set_visible(page, false);
    let delivered = pump(&mut ui, &mut s);
    assert!(delivered > 0, "SetVisible raised no element message");
    let out = ui.requests.take();
    assert!(
        out.contains(&UiRequest::SetPreference(
            "Sound.SoundVolume",
            PrefValue::Float(1.0)
        )),
        "hiding the page did not revert the change: {out:?}"
    );
    assert_eq!(s.config_page.options[i].current, PrefValue::Float(1.0));

    ui.requests.clear();
    ui.set_visible(page, true);
    pump(&mut ui, &mut s);
    let out = ui.requests.take();
    assert!(
        !out.iter()
            .any(|r| matches!(r, UiRequest::SetPreference(..))),
        "showing the page must not write a preference: {out:?}"
    );
    assert_eq!(
        out,
        vec![UiRequest::SetPanelVisibility {
            panel: 10,
            visible: true
        }],
        "the panel notice, and nothing else"
    );
}

/// Behaviour: options.client-page.an-unticked-sound-box-greys-its-slider
/// The paired check-box-and-slider control's message-1 arm sets the slider's state: `0x0D`
/// (disabled) when the box is unticked, state 1 when it is ticked.
///
/// Both directions, because a gate that never re-enables looks identical after one click.
///
/// **Driven by the message the check box raises, not by writing the page's field.** The click is
/// a real button mouse-up raising element message 1 on the `0x10000219` element; it
/// bubbles to the screen root and comes back through [`pump`], so both halves of
/// the arm — the value half (read `0x0E`, `Apply(true)`) and the state half
/// (grey the bar) — run on the way.
///
/// **What is asserted about the bar is the state that was *asked for*, not `n.state`.** The shipped
/// `0x1000021C` template declares **no states at all**, and
/// the tree records **state 0** for a state the description does not carry — so the bar reads 0
/// either way, in this build and in retail. Measured, not assumed: see
/// `o213_probe.rs::probe_slider_states`. Asserting `n.state == 0x0D` would be asserting something
/// the client does not do.
#[test]
fn unticking_a_sound_check_box_greys_its_paired_slider() {
    let (mut ui, mut s) = screen();
    let (box_i, cb) = control(&s.config_page, "Sound.AmbientSoundDisabled");
    let (slider_i, bar) = control(&s.config_page, "Sound.AmbientSoundVolume");
    let k = s
        .config_page
        .gated
        .iter()
        .position(|p| *p == (box_i, slider_i))
        .expect("the check box and the slider are not paired");

    // `Button` toggles its own `0x0E` and then raises message 1; the arm reads it back.
    let click = |ui: &mut UiSystem, s: &mut GamePlayScreen, on: bool| -> Vec<UiRequest> {
        ui.set_attribute_bool(cb, page::ATTR_CHECKED, on);
        ui.drain_outbox();
        ui.requests.clear();
        ui.broadcast_element_message(cb, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
        pump(ui, s);
        ui.requests.take()
    };

    let off = click(&mut ui, &mut s, false);
    assert!(
        off.contains(&UiRequest::SetPreference(
            "Sound.AmbientSoundDisabled",
            PrefValue::Bool(false)
        )),
        "the click did not write the preference: {off:?}"
    );
    assert_eq!(s.config_page.gate_states[k], page::SLIDER_DISABLED_STATE);
    assert_eq!(page::gate_state(false), dereth_ui::StateId(0x0D));

    let on = click(&mut ui, &mut s, true);
    assert!(on.contains(&UiRequest::SetPreference(
        "Sound.AmbientSoundDisabled",
        PrefValue::Bool(true)
    )));
    assert_eq!(s.config_page.gate_states[k], page::SLIDER_ENABLED_STATE);
    assert_eq!(page::gate_state(true), dereth_ui::StateId(1));

    // The other two pairs were not touched by this click.
    assert_eq!(s.config_page.gate_states.len(), 3);
    // And the shipped bar's recorded state is 0 in both directions, for the reason above.
    assert_eq!(ui.node(bar).unwrap().state, dereth_ui::StateId(0));
    assert!(
        ui.node(bar).unwrap().desc.states.is_empty(),
        "0x1000021C declares states now; the paragraph above is stale"
    );
}

/// An input action reaches every element registered for it.
#[test]
fn an_input_action_reaches_every_element_registered_for_it() {
    let mut ui = env();
    let root =
        create_and_add_root_element(&mut ui, LayoutEnum(0x1000_0006), ElementId(0x1000_0495))
            .expect("classic_gameplay builds");
    let mut all = Vec::new();
    walk(&ui, root, &mut all);
    let by_id = |ui: &UiSystem, all: &[ElemHandle], id: u32| {
        all.iter()
            .copied()
            .find(|h| ui.node(*h).is_some_and(|n| n.element_id().0 == id))
            .unwrap_or_else(|| panic!("{id:#010X} is not in the shipped tree"))
    };
    // `ClientOptionsPanel` (0x57 = 0x1000001D) and `CharacterSettingsPanel` (0x57 = 0x1000001C), two
    // different panels on two different actions.
    let config = by_id(&ui, &all, config::CONFIG_PAGE_ELEMENT.0);
    let charset = by_id(&ui, &all, 0x1000_0211);
    ui.set_visible(config, false);
    ui.set_visible(charset, false);
    assert!(!ui.node(config).unwrap().region.flags.visible);

    let e = dereth_ui::focus::InputEvent {
        action: 0x1000_001D,
        start: true,
        x: 0,
        y: 0,
    };
    ui.key_press(&e);

    assert!(
        ui.node(config).unwrap().region.flags.visible,
        "0x1000001D did not open the Client Options page"
    );
    assert!(
        !ui.node(charset).unwrap().region.flags.visible,
        "an action bound to another panel must not open this one"
    );

    // The return is the client's: `true` when the action had a bucket, false when it had none.
    assert!(ui.dispatch_input_action(0x1000_001D));
    assert!(!ui.dispatch_input_action(0xDEAD_BEEF));

    // And `key_press`'s tail runs **whether or not the focused element consumed the action** —
    // the call sits after the `if`/`else` in the key-press event, not inside it. Focus an element
    // that consumes nothing and re-press: the panel still toggles from its `0x58 = 1`.
    ui.set_visible(config, false);
    ui.key_press(&e);
    assert!(ui.node(config).unwrap().region.flags.visible);
}

/// The shipped gameplay tree registers forty elements for input actions.
#[test]
fn the_shipped_gameplay_tree_registers_forty_elements_for_input_actions() {
    let mut ui = env();
    let root =
        create_and_add_root_element(&mut ui, LayoutEnum(0x1000_0006), ElementId(0x1000_0495))
            .expect("classic_gameplay builds");
    let mut all = Vec::new();
    walk(&ui, root, &mut all);
    let hits: Vec<(u32, u32)> = all
        .iter()
        .filter_map(|h| ui.node(*h))
        .filter_map(|n| {
            let p = n.merged_properties();
            let a = p.get_enum(dereth_ui::props::attr::INPUT_ACTION)?;
            Some((n.element_id().0, a))
        })
        .collect();
    assert_eq!(hits.len(), 40, "of {} elements in the tree", all.len());
    assert!(all.len() > 1800, "the tree did not build: {}", all.len());
    // …and every one of them also carries `0x58`, which is what the `0x31` arm reads. An element
    // registered for an action but with no toggle mode would receive the message and do nothing.
    let with_mode = all
        .iter()
        .filter_map(|h| ui.node(*h))
        .filter(|n| {
            let p = n.merged_properties();
            p.get_enum(dereth_ui::props::attr::INPUT_ACTION).is_some()
                && p.get_enum(dereth_ui::props::attr::VISIBILITY_TOGGLE_MODE)
                    .is_some()
        })
        .count();
    assert_eq!(with_mode, 40, "40 of 40 carry both 0x57 and 0x58");
    // The Client Options page is one of them, and its action is the literal in the layout.
    assert!(
        hits.contains(&(config::CONFIG_PAGE_ELEMENT.0, 0x1000_001D)),
        "ClientOptionsPanel's own 0x57: {hits:?}"
    );
}

mod sound_defaults {
    //! Defaults on the Client Options page writes the eight Sound.* preferences, other pages' Defaults
    //! do not; the page is built with 27 rows, 6 headers, 5 separators.
    //! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

    use crate::common::layout::RegistrationOrder;

    use dereth_ui::framework::{LayoutEnum, Screen};
    use dereth_ui::{ElemHandle, ElementId, UiSystem};

    use crate::common::*;
    use dereth_ui_screens::env::create_and_add_root_element;
    use dereth_ui_screens::options::config;
    use dereth_ui_screens::screens::gameplay::GamePlayScreen;
    use dereth_ui_screens::{PrefValue, UiRequest};

    fn env() -> UiSystem {
        let (ui, _flow, _store) =
            crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
        ui
    }

    /// The gameplay UI's one root, with every panel page under it.
    fn gameplay_tree() -> (UiSystem, Vec<ElemHandle>) {
        let mut ui = env();
        let root =
            create_and_add_root_element(&mut ui, LayoutEnum(0x1000_0006), ElementId(0x1000_0495))
                .expect("classic_gameplay builds");
        let mut all = Vec::new();
        walk(&ui, root, &mut all);
        (ui, all)
    }

    fn gameplay_screen() -> (UiSystem, Vec<ElemHandle>, GamePlayScreen) {
        let mut ui = env();
        let mut screen = GamePlayScreen::default();
        screen
            .create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
            .expect("the gameplay screen builds");
        let root = screen.root().expect("its root");
        let mut all = Vec::new();
        walk(&ui, root, &mut all);
        (ui, all, screen)
    }

    /// The handle of `want`, but only where an ancestor is `under`. The three option pages carry the
    /// same Apply / Cancel / Defaults child ids, so the ancestor is what names the page.
    fn find_under(
        ui: &UiSystem,
        all: &[ElemHandle],
        under: ElementId,
        want: ElementId,
    ) -> ElemHandle {
        for h in all {
            let Some(n) = ui.node(*h) else { continue };
            if n.element_id() != want {
                continue;
            }
            let mut p = *h;
            while let Some(up) = ui.parent(p) {
                if ui.node(up).is_some_and(|n| n.element_id() == under) {
                    return *h;
                }
                p = up;
            }
        }
        panic!("{want:?} was not found under {under:?} in the shipped gameplay tree");
    }

    /// Behaviour: options.client-page.defaults-writes-the-eight-sound-preferences
    /// Clicking defaults on the client options page writes the eight sound preferences.
    #[test]
    fn clicking_defaults_on_the_client_options_page_writes_the_eight_sound_preferences() {
        let (mut ui, all, mut screen) = gameplay_screen();
        let defaults = find_under(
            &ui,
            &all,
            config::CONFIG_PAGE_ELEMENT,
            config::button::DEFAULTS,
        );

        ui.requests.clear();
        screen.on_element_message(
            &mut dereth_ui::framework::ScreenCx::new(&mut ui),
            &dereth_ui::msg::ElementMessage {
                source_id: config::button::DEFAULTS,
                source: defaults,
                id: dereth_ui::msg::element::id::BUTTON_CLICKED,
                p1: 0,
                p2: 0,
                point: dereth_ui::msg::MessagePoint::default(),
                serial: 1,
            },
        );
        let out = ui.requests.take();
        assert_eq!(
            out.len(),
            30,
            "27 rows plus the three paired volume sliders"
        );

        // The eight `Sound.*` writes, spelled out with the default values the page's option
        // setup supplies. This is the literal the seam is pinned by: everything else uses the symbols.
        let sound: Vec<(&str, PrefValue)> = out
            .iter()
            .filter_map(|r| match r {
                UiRequest::SetPreference(k, v) if k.starts_with("Sound.") => Some((*k, v.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(
            sound,
            vec![
                ("Sound.SoundFeatures", PrefValue::Int(0)),
                ("Sound.SoundDisabled", PrefValue::Bool(true)),
                ("Sound.SoundVolume", PrefValue::Float(1.0)),
                ("Sound.AmbientSoundDisabled", PrefValue::Bool(true)),
                ("Sound.AmbientSoundVolume", PrefValue::Float(1.0)),
                ("Sound.InterfaceSoundDisabled", PrefValue::Bool(true)),
                ("Sound.InterfaceSoundVolume", PrefValue::Float(1.0)),
                ("Sound.PlaySoundOnlyWhenActive", PrefValue::Bool(true)),
            ],
            "the Sound section's eight writes, in order"
        );
    }

    /// **A click on the *other* pages' Defaults buttons is not the client-options panel's.**
    ///
    /// The chat-options panel (`0x1000050C`) carries the same three child ids -- the shipped tree holds two
    /// copies of `0x100001FC`..`0x100001FE`, one under it and one under the client-options panel. Without the
    /// ancestor walk, opening the chat options page and pressing Defaults would rewrite the sound
    /// preferences. (The character-settings panel `0x100001FA` is a sibling of its buttons rather than their
    /// parent in this layout, so it is not a case this test can construct.)
    #[test]
    fn the_other_option_pages_defaults_buttons_do_not_write_the_sound_preferences() {
        let (mut ui, all, mut screen) = gameplay_screen();
        for page in [ElementId(0x1000_050C)] {
            let h = find_under(&ui, &all, page, config::button::DEFAULTS);
            ui.requests.clear();
            screen.on_element_message(
                &mut dereth_ui::framework::ScreenCx::new(&mut ui),
                &dereth_ui::msg::ElementMessage {
                    source_id: config::button::DEFAULTS,
                    source: h,
                    id: dereth_ui::msg::element::id::BUTTON_CLICKED,
                    p1: 0,
                    p2: 0,
                    point: dereth_ui::msg::MessagePoint::default(),
                    serial: 1,
                },
            );
            let out = ui.requests.take();
            assert!(
                out.iter()
                    .all(|r| !matches!(r, UiRequest::SetPreference(..))),
                "{page:?}'s Defaults button wrote a preference: {out:?}"
            );
        }
    }

    /// The client options page is built with its rows in it.
    #[test]
    fn the_client_options_page_is_built_with_its_rows_in_it() {
        let (ui, all, screen) = gameplay_screen();
        let page = find_under(
            &ui,
            &all,
            ElementId(0x1000_0495),
            config::CONFIG_PAGE_ELEMENT,
        );
        let box_h = ui
            .children(page)
            .into_iter()
            .find(|c| {
                ui.node(*c)
                    .is_some_and(|n| n.element_id() == config::OPTION_BOX)
            })
            .expect("the option box is in the shipped layout");
        assert_eq!(
            ui.children(box_h).len(),
            38,
            "27 option rows, 6 section headers and 5 separators"
        );
        assert_eq!(screen.config_page.row_count(), 38);
        assert_eq!(screen.config_page.headers, 6);
        assert_eq!(screen.config_page.separators, 5);
        assert_eq!(screen.config_page.failures, 0, "every row template built");
        assert_eq!(
            screen.config_page.options.len(),
            30,
            "27 rows, of which the three check+slider pairs register two controls each"
        );

        let (base_ui, base_all) = gameplay_tree();
        let count = |u: &UiSystem, hs: &[ElemHandle], ty: u32| {
            hs.iter()
                .filter_map(|h| u.node(*h))
                .filter(|n| n.ty().0 == ty)
                .count()
        };
        let delta = |ty: u32| count(&ui, &all, ty) as i64 - count(&base_ui, &base_all, ty) as i64;
        assert_eq!(
            delta(0x1000_0035),
            62,
            "checkbox options: the client-options panel's 9 toggle rows + 3 paired boxes, and the character-settings panel's 50"
        );
        assert_eq!(
            delta(0x1000_0036),
            3,
            "checkbox-slider options: the three sound rows"
        );
        let sliders_under = |page: ElementId| {
            all.iter()
                .filter(|h| ui.node(**h).is_some_and(|n| n.ty().0 == 0x1000_0037))
                .filter(|h| {
                    let mut c = **h;
                    loop {
                        if ui.node(c).is_some_and(|n| n.element_id() == page) {
                            return true;
                        }
                        match ui.parent(c) {
                            Some(p) => c = p,
                            None => return false,
                        }
                    }
                })
                .count()
        };
        assert_eq!(
            sliders_under(config::CONFIG_PAGE_ELEMENT),
            10,
            "slider options on the client-options panel's own page: 3 paired + 7 stand-alone"
        );
        assert_eq!(
            delta(0x1000_0037),
            12,
            "slider options across the screen: the client-options panel's 10 and the chat-options panel's 2 opacity sliders"
        );
        assert_eq!(delta(0x1000_0038), 8, "menu options: the eight menu rows");
        assert_eq!(
            delta(0x1000_0034),
            0,
            "action-key-map options: the keyboard-settings panel is bound but its rows need the input bindings from o236"
        );
        assert_eq!(
            delta(0x1000_0043),
            0,
            "Bitfield checkboxes are unused in the shipped pages"
        );
        assert_eq!(
            delta(0x1000_0044),
            5,
            "The chat-options panel's five per-window filters use 64-bit checkbox options"
        );
        eprintln!(
            "classic_gameplay: {} -> {} elements, option box {} rows, option array {}",
            base_all.len(),
            all.len(),
            ui.children(box_h).len(),
            screen.config_page.options.len()
        );
    }
}
