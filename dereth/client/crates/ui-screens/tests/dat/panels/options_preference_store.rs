//! The first show swallows its own visibility message; showing writes nothing; the page opens on
//! the stored value, picks up outside changes, cancel reverts to the store, defaults persist across
//! reopen, a preferences file reaches the page, the store holds exactly 34 attached preferences.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use dereth_ui::framework::Screen;
use dereth_ui::msg::Delivery;
use dereth_ui::{ElemHandle, UiSystem};

use dereth_ui::persist::preferences::UserPreferences;
use dereth_ui_screens::options::store::{self, DataType};
use dereth_ui_screens::options::{config, page};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::{PrefValue, UiRequest};

fn env() -> UiSystem {
    let (ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::AfterResolver);
    ui
}

/// The shipped gameplay screen, built **after** whatever the caller did to the store — which is
/// the order that matters: the options page builds its controls long after the preferences
/// file has been read into the store.
fn screen(ui: &mut UiSystem) -> GamePlayScreen {
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(ui))
        .expect("the gameplay screen builds");
    s
}

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

fn value(s: &GamePlayScreen, preference: &str) -> PrefValue {
    s.config_page
        .options
        .iter()
        .find(|o| o.preference == preference)
        .unwrap_or_else(|| panic!("{preference} is not on the built page"))
        .current
        .clone()
}

fn index(s: &GamePlayScreen, preference: &str) -> usize {
    s.config_page
        .options
        .iter()
        .position(|o| o.preference == preference)
        .expect("on the page")
}

// -------------------------------------------------------------------------------------------
// The read
// -------------------------------------------------------------------------------------------

fn open_page(ui: &mut UiSystem, s: &mut GamePlayScreen, page_el: ElemHandle) {
    ui.set_visible(page_el, true);
    pump(ui, s);
    ui.set_visible(page_el, false);
    pump(ui, s);
    ui.set_visible(page_el, true);
    let n = pump(ui, s);
    assert!(n > 0, "the show raised no element message at all");
}

/// The first show of a panel page swallows its own visibility message.
#[test]
fn the_first_show_of_a_panel_page_swallows_its_own_visibility_message() {
    let mut ui = env();
    let mut s = screen(&mut ui);
    let page_el = ui
        .get_child_recursive(s.root().expect("a root"), config::CONFIG_PAGE_ELEMENT)
        .expect("configuration panel");
    ui.drain_outbox();

    let ids = |ui: &mut UiSystem| -> Vec<(u32, u32, u32)> {
        ui.drain_outbox()
            .into_iter()
            .filter_map(|d| match d {
                Delivery::Element { msg, .. } => Some((msg.id.0, msg.source_id.0, msg.p1)),
                _ => None,
            })
            .collect()
    };
    ui.set_visible(page_el, true);
    assert_eq!(
        ids(&mut ui),
        vec![(0x2C, 0x1000_018D, 0), (0x18, 0x1000_018D, 1)],
        "the first show delivers the panel's TAB_PAGE_CHANGED and the container's own 0x18, \
         and NOT the page's 0x18"
    );
    ui.set_visible(page_el, false);
    assert_eq!(
        ids(&mut ui),
        vec![(0x18, 0x1000_0213, 0), (0x18, 0x1000_018D, 0)],
        "the hide is delivered, and the container follows the page down"
    );
    ui.set_visible(page_el, true);
    assert_eq!(
        ids(&mut ui),
        vec![(0x18, 0x1000_0213, 1), (0x18, 0x1000_018D, 1)],
        "and so is every later show"
    );
    let _ = &mut s;
}

/// Showing the page writes no preference and one panel notice.
#[test]
fn showing_the_page_writes_no_preference_and_one_panel_notice() {
    let mut ui = env();
    let mut s = screen(&mut ui);
    let page_el = ui
        .get_child_recursive(s.root().expect("a root"), config::CONFIG_PAGE_ELEMENT)
        .expect("configuration panel");
    open_page(&mut ui, &mut s, page_el);
    ui.set_visible(page_el, false);
    pump(&mut ui, &mut s);

    ui.requests.clear();
    ui.set_visible(page_el, true);
    assert!(
        pump(&mut ui, &mut s) > 0,
        "the show raised no element message at all"
    );
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
        "that panel notice, and nothing else"
    );
}

/// The page opens showing the registered value not its own default.
#[test]
fn the_page_opens_showing_the_registered_value_not_its_own_default() {
    let mut ui = env();
    let s = screen(&mut ui);

    let disagreements = [
        (
            "Render.AutomaticDegrades",
            PrefValue::Bool(true),
            PrefValue::Bool(false),
        ),
        (
            "Input.MouseLookSensitivity",
            PrefValue::Float(0.25),
            PrefValue::Float(0.55),
        ),
        (
            "Render.TextureFiltering",
            PrefValue::Int(0),
            PrefValue::Int(1),
        ),
    ];
    for (name, registered, restored) in &disagreements {
        assert_ne!(registered, restored, "{name}: the two must differ");
        assert_eq!(
            &value(&s, name),
            registered,
            "{name} opened with its registered value"
        );
        let i = index(&s, name);
        assert_eq!(&s.config_page.options[i].default, restored, "{name}");
    }

    // The literals, so a wrong constant cannot hide behind either symbol.
    assert_eq!(value(&s, "Display.Resolution"), PrefValue::Int(0x0400_0300)); // 1024x768
    assert_eq!(value(&s, "Render.AutomaticDegrades"), PrefValue::Bool(true));
    assert_eq!(
        value(&s, "Input.MouseLookSensitivity"),
        PrefValue::Float(0.25)
    );
    assert_eq!(value(&s, "Render.TextureFiltering"), PrefValue::Int(0));

    // The other 26 controls agree with their `SetDefault`; all 29 still participate.
    let disagreeing: Vec<&str> = disagreements.iter().map(|(name, _, _)| *name).collect();
    let mut checked = 0;
    for o in s.config_page.retail_options() {
        if disagreeing.contains(&o.preference) {
            continue;
        }
        assert_eq!(
            o.current,
            store::inq_value(o.preference).expect("registered"),
            "{}",
            o.preference
        );
        checked += 1;
    }
    assert_eq!(checked, 26, "26 of the 29 controls agree either way");
}

/// The **show** arm of the page's visibility hook —
/// which sets current = saved = the stored value on every option.
///
/// The store is moved by something that is not the page (the mouse-turning preset
/// is the client's own example, and it rewrites **five** of this
/// page's own rows), and the page is then shown. Both halves are asserted: the value moved, and
/// the element moved with it — a page that updated `current` and left the bar where it was would
/// look identical in every headless assertion but wrong on screen.
#[test]
fn a_preference_changed_outside_the_page_is_picked_up_when_it_is_shown() {
    let mut ui = env();
    let mut s = screen(&mut ui);
    let page_el = ui
        .get_child_recursive(s.root().expect("a root"), config::CONFIG_PAGE_ELEMENT)
        .expect("configuration panel is in the tree");

    let i = index(&s, "Camera.AdjustmentSpeed");
    let bar = s.config_page.options[i].element;
    assert_eq!(value(&s, "Camera.AdjustmentSpeed"), PrefValue::Float(40.0));

    // Somebody else writes the variable — exactly what `set_mouse_turning_defaults` does to five of
    // these rows through the preference store.
    assert!(store::set_value(
        "Camera.AdjustmentSpeed",
        PrefValue::Float(70.0)
    ));
    assert_eq!(
        value(&s, "Camera.AdjustmentSpeed"),
        PrefValue::Float(40.0),
        "not yet"
    );

    open_page(&mut ui, &mut s, page_el);

    assert_eq!(
        value(&s, "Camera.AdjustmentSpeed"),
        PrefValue::Float(70.0),
        "the show arm re-read"
    );
    // 70 on the 5..80 range is (70-5)/75 = 0.8666…; the bar has to have been refreshed.
    let pos = dereth_ui::widgets::scrollbar::Scrollbar::position(&ui, bar);
    assert!(
        (pos - 0.866_666_7).abs() < 1e-3,
        "the bar is at {pos}, not where 70.0 sits"
    );
    assert_eq!(page::slider_range("Camera.AdjustmentSpeed"), (5.0, 80.0));

    // Showing the page still writes no preference — it is a read, not a write.
    ui.requests.clear();
    ui.set_visible(page_el, false);
    pump(&mut ui, &mut s);
    ui.set_visible(page_el, true);
    pump(&mut ui, &mut s);
    let writes: Vec<UiRequest> = ui
        .requests
        .take()
        .into_iter()
        .filter(|r| matches!(r, UiRequest::SetPreference(..)))
        .collect();
    assert!(writes.is_empty(), "showing the page wrote {writes:?}");
}

/// Behaviour: options.client-page.opens-on-the-stored-value-and-cancel-reverts-to-it
/// Cancel — **across a page visit**.
///
/// This is what the row means by "Cancel reverts only what changed … but only within one page
/// visit". The sequence is: open, drag, close (which reverts), somebody else writes the store,
/// open again, drag again, Cancel. The Cancel must revert to what the **store** held when the page
/// was last shown, not to the value the page was constructed with.
#[test]
fn cancel_reverts_to_what_the_store_held_when_the_page_opened() {
    let mut ui = env();
    let mut s = screen(&mut ui);
    let page_el = ui
        .get_child_recursive(s.root().expect("a root"), config::CONFIG_PAGE_ELEMENT)
        .expect("configuration panel");

    // Somebody else sets the ambient volume to 0.6 while the page is closed.
    assert!(store::set_value(
        "Sound.AmbientSoundVolume",
        PrefValue::Float(0.6)
    ));
    open_page(&mut ui, &mut s, page_el);
    assert_eq!(
        value(&s, "Sound.AmbientSoundVolume"),
        PrefValue::Float(0.6),
        "opened on the store"
    );

    // The player drags it to 0.2 and then presses Cancel.
    let i = index(&s, "Sound.AmbientSoundVolume");
    s.config_page.options[i].current = PrefValue::Float(0.2);
    s.config_page.apply(&mut ui.requests, i);
    assert_eq!(
        store::inq_value("Sound.AmbientSoundVolume"),
        Some(PrefValue::Float(0.2))
    );
    assert!(s.config_page.changed());

    ui.requests.clear();
    let reverted = s.config_page.restore_saved_values(&mut ui);
    assert_eq!(reverted, 1, "one control changed, so one is restored");
    assert_eq!(
        value(&s, "Sound.AmbientSoundVolume"),
        PrefValue::Float(0.6),
        "Cancel went back to 0.6, not to the 1.0 the page was built with"
    );
    // …and the revert reached the store as well as the page.
    assert_eq!(
        store::inq_value("Sound.AmbientSoundVolume"),
        Some(PrefValue::Float(0.6))
    );
    assert!(ui.requests.take().contains(&UiRequest::SetPreference(
        "Sound.AmbientSoundVolume",
        PrefValue::Float(0.6)
    )));
    assert!(!s.config_page.changed());
}

/// Behaviour: options.client-page.defaults-write-through-to-the-store
/// Defaults writes through to the store and survives a reopen.
#[test]
fn defaults_writes_through_to_the_store_and_survives_a_reopen() {
    let mut ui = env();
    let mut s = screen(&mut ui);
    let page_el = ui
        .get_child_recursive(s.root().expect("a root"), config::CONFIG_PAGE_ELEMENT)
        .expect("configuration panel");

    assert_eq!(
        value(&s, "Input.MouseLookSensitivity"),
        PrefValue::Float(0.25)
    );
    ui.requests.clear();
    // The 28 retail controls (the chat font's two are the Chat Options page's), this client's
    // three rows from another era and its performance row; the interface row stays as it is.
    assert_eq!(s.config_page.restore_default_values(&mut ui), 32);
    let requests = ui.requests.take().len();
    assert_eq!(requests, 32, "the outward notification is unchanged");
    assert_eq!(
        store::inq_value("Input.MouseLookSensitivity"),
        Some(PrefValue::Float(0.55))
    );
    assert_eq!(
        store::inq_value("Render.AutomaticDegrades"),
        Some(PrefValue::Bool(false))
    );
    assert_eq!(
        store::inq_value("Display.Resolution"),
        Some(PrefValue::Int(0x0400_0300))
    );

    // **Defaults alone does not stick, and that is retail.** Restore-defaults writes `current`
    // and the store; `saved` is untouched, so closing the page without pressing Apply runs the
    // restore-saved-values step and puts the old values back — through the store, now.
    open_page(&mut ui, &mut s, page_el);
    assert_eq!(
        value(&s, "Input.MouseLookSensitivity"),
        PrefValue::Float(0.25),
        "closing without Apply reverted the defaults, as `on_visibility_changed`'s hide arm must"
    );
    assert_eq!(
        store::inq_value("Input.MouseLookSensitivity"),
        Some(PrefValue::Float(0.25))
    );

    // Now do it the way a player makes it stick: Defaults, then Apply, then close and re-open.
    assert_eq!(s.config_page.restore_default_values(&mut ui), 32);
    assert_eq!(
        s.config_page.save_current_values(),
        0,
        "Apply re-reads and finds what it wrote"
    );
    assert!(!s.config_page.changed());
    open_page(&mut ui, &mut s, page_el);
    assert_eq!(
        value(&s, "Input.MouseLookSensitivity"),
        PrefValue::Float(0.55)
    );
    assert_eq!(
        value(&s, "Render.AutomaticDegrades"),
        PrefValue::Bool(false)
    );
    assert_eq!(value(&s, "Display.Resolution"), PrefValue::Int(0x0400_0300));
    assert_eq!(
        store::inq_value("Input.MouseLookSensitivity"),
        Some(PrefValue::Float(0.55))
    );
}

/// A user preferences file reaches the page.
#[test]
fn a_user_preferences_file_reaches_the_page() {
    let mut ui = env();
    let text = "[Sound]\r\nSoundVolume=0.375\r\nSoundDisabled=False\r\n\
                [Render]\r\nFieldOfView=120.00\r\n[UI]\r\nChatFontSize=4\r\n\
                [Camera]\r\nAlignToSlope=False\r\n[Net]\r\nComputeUniquePort=True\r\n";
    let ini = UserPreferences::parse(text).expect("the ini parses");
    let (applied, ignored) = store::load(&ini);
    assert_eq!(
        applied, 5,
        "five of the six keys name an attached preference"
    );
    assert_eq!(
        ignored, 1,
        "Net.ComputeUniquePort is not attached to the UI"
    );

    let s = screen(&mut ui);
    assert_eq!(value(&s, "Sound.SoundVolume"), PrefValue::Float(0.375));
    assert_eq!(
        value(&s, "Sound.SoundDisabled"),
        PrefValue::Bool(false),
        "False means OFF"
    );
    assert_eq!(value(&s, "Render.FieldOfView"), PrefValue::Float(120.0));
    assert_eq!(value(&s, "UI.ChatFontSize"), PrefValue::Int(4));
    assert_eq!(value(&s, "Camera.AlignToSlope"), PrefValue::Bool(false));
    // A key the file did not carry keeps its registration default.
    assert_eq!(value(&s, "Camera.Stiffness"), PrefValue::Float(0.45));

    // The elements were pushed the loaded values, not only the page's fields: the FOV bar sits at
    // (120-10)/150 on the 10..160 range.
    let i = index(&s, "Render.FieldOfView");
    let pos =
        dereth_ui::widgets::scrollbar::Scrollbar::position(&ui, s.config_page.options[i].element);
    assert!((pos - 0.733_333_3).abs() < 1e-3, "the FOV bar is at {pos}");
    // …and the check box that came up false really is unticked.
    let cb = s.config_page.options[index(&s, "Sound.SoundDisabled")].element;
    assert_eq!(
        ui.node(cb)
            .and_then(|n| n.merged_properties().get_bool(page::ATTR_CHECKED)),
        Some(false)
    );
}

/// Dragging the volume slider updates the preference store.
#[test]
fn dragging_the_volume_slider_updates_the_preference_store() {
    let mut ui = env();
    let mut s = screen(&mut ui);
    let i = index(&s, "Sound.SoundVolume");
    let bar = s.config_page.options[i].element;
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
    let y = oy + b.height() / 2;
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    let x = ox + (b.width() as f32 * 0.25) as i32;
    ui.requests.clear();
    ui.mouse_down(7, x, y);
    ui.mouse_up(7, x, y, false);
    pump(&mut ui, &mut s);

    let out = ui.requests.take();
    let written = match out
        .iter()
        .find(|r| matches!(r, UiRequest::SetPreference("Sound.SoundVolume", _)))
    {
        Some(UiRequest::SetPreference(_, PrefValue::Float(v))) => *v,
        other => panic!("no Sound.SoundVolume write: {other:?} / {out:?}"),
    };
    assert!(
        written < 0.5,
        "a press a quarter along should be quiet, got {written}"
    );
    // The new half: Apply updated the preference store as well as the request queue.
    assert_eq!(
        store::inq_value("Sound.SoundVolume"),
        Some(PrefValue::Float(written))
    );
    // …and reading it back through the control's own type is what `save_current_value` does.
    assert_eq!(
        store::inq_value_as("Sound.SoundVolume", DataType::Float),
        Some(PrefValue::Float(written))
    );
    assert_eq!(
        store::inq_value_as("Sound.SoundVolume", DataType::Bool),
        None,
        "wrong data type"
    );

    // Apply re-snapshots from the store and moves nothing.
    let moved = s.config_page.save_current_values();
    assert_eq!(moved, 0, "the page and the store already agree");
    assert!(!s.config_page.changed());
}

/// The store is registered for every host, and it is the **34 attached preferences** — not the 43
/// the client registers overall. A name that is in `UserPreferences.ini` and not on any option
/// page must miss, or `load` would report a false denominator.
#[test]
fn the_store_holds_the_thirty_four_attached_preferences_and_no_others() {
    let _ui = env();
    // ...and this client's own three options from another era, its interface, its performance
    // panel, the classic interface's six and the landscape detail texture beside them.
    assert_eq!(store::len(), 46);
    assert!(store::is_registered("Render.LandscapeDetailTextures"));
    assert!(store::is_registered("Debug.PerformancePanel"));
    assert!(store::is_registered("Render.Ground") && store::is_registered("Render.Sky"));
    assert!(store::is_registered("Render.Objects") && store::is_registered("UI.Interface"));
    for p in dereth_ui_screens::options::preferences::UI_PREFERENCES {
        assert!(store::is_registered(p.name), "{}", p.name);
    }
    for absent in [
        "Net.BindInterface",
        "Net.ComputeUniquePort",
        "Net.UserSpecifiedPort",
        "Input.KeymapFile",
        "Input.MouseLookSmoothingAmount",
        "International.UseIME",
        "Render.DisplayAdapter",
        "Render.AspectRatio",
    ] {
        assert!(
            !store::is_registered(absent),
            "{absent} is not attached to the UI"
        );
    }
    // Spot-check the registered preference types.
    assert!(store::is_registered_as(
        "Sound.SoundVolume",
        DataType::Float
    ));
    assert!(store::is_registered_as(
        "Sound.SoundDisabled",
        DataType::Bool
    ));
    assert!(store::is_registered_as(
        "Display.Resolution",
        DataType::UInt
    ));
    assert!(
        !store::is_registered_as("Display.Resolution", DataType::Int),
        "UInt32, not Int32"
    );
}

/// A handle the other tests need but nothing else uses: the page element under the screen root.
/// Kept as its own assertion so a change in the tree shape fails here rather than in six places.
#[test]
fn the_config_page_is_where_the_other_tests_look_for_it() {
    let mut ui = env();
    let s = screen(&mut ui);
    let root: ElemHandle = s.root().expect("the gameplay screen has a root");
    let page_el = ui
        .get_child_recursive(root, config::CONFIG_PAGE_ELEMENT)
        .expect("configuration panel 0x10000213 is under the gameplay root");
    assert!(
        !ui.node(page_el).expect("the page").region.flags.visible,
        "it starts hidden"
    );
    assert_eq!(s.config_page.row_count(), 39);
}

/// Behaviour: presentation.settings.an-interface-shown-again-shows-the-store-as-it-is
/// The other interface changed options while this page was put away, the page still showing: read
/// again, it shows the store's values, the interface choice among them, and greys the manual
/// degrade bias by the adaptive degrade as it is now, not as it was when the page was built.
#[test]
fn a_page_read_again_shows_the_stored_values_and_greys_by_them() {
    use dereth_client_contract::options::interface::{Interface, INTERFACE};
    let mut ui = env();
    assert!(store::set_value(
        INTERFACE,
        PrefValue::Int(Interface::Classic.value())
    ));
    assert!(store::set_value(
        "Render.AutomaticDegrades",
        PrefValue::Bool(true)
    ));
    let mut s = screen(&mut ui);
    let bias = index(&s, "Render.GraphicsPerformance");
    let control = s.config_page.options[bias].element;
    let clickable = |ui: &UiSystem| ui.node(control).is_some_and(|n| n.is_mouse_visible);
    assert!(
        !clickable(&ui),
        "the bias is greyed while adaptive degrade is on"
    );
    assert_eq!(
        value(&s, INTERFACE),
        PrefValue::Int(Interface::Classic.value())
    );

    // The other interface: back to this one, and adaptive degrade turned off there.
    assert!(store::set_value(
        INTERFACE,
        PrefValue::Int(Interface::Retail.value())
    ));
    assert!(store::set_value(
        "Render.AutomaticDegrades",
        PrefValue::Bool(false)
    ));
    assert_eq!(
        s.config_page.reread(&mut ui),
        2,
        "two rows moved under the page"
    );
    assert_eq!(
        value(&s, INTERFACE),
        PrefValue::Int(Interface::Retail.value())
    );
    assert_eq!(
        value(&s, "Render.AutomaticDegrades"),
        PrefValue::Bool(false)
    );
    assert!(
        clickable(&ui),
        "the bias is used by hand now, and takes clicks"
    );
    ui.requests.clear();
    assert_eq!(s.config_page.reread(&mut ui), 0);
    assert!(
        !ui.requests
            .take()
            .iter()
            .any(|r| matches!(r, UiRequest::SetPreference(..))),
        "reading again writes nothing"
    );
}
