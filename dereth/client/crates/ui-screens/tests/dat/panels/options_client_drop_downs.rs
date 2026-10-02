//! The Client Options page's eight drop-downs carry 32 entries with registered values and shipped
//! captions, open on the stored setting, a real press opens and applies a row, and no registry
//! means no entries.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;
use crate::common::layout::Strings;
use std::rc::Rc;

use dereth_ui::framework::Screen;
use dereth_ui::{ElemHandle, ElementId, UiSystem};

use dereth_ui_screens::options::page::{OptionControl, PlayerOptionPage};
use dereth_ui_screens::options::{preferences, store};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::view::{PrefValue, UiRequest};

// -------------------------------------------------------------------------------------------
// The harness — the shipped `classic_gameplay` tree with a real string resolver.
// -------------------------------------------------------------------------------------------

fn ui_system(with_registry: bool) -> UiSystem {
    let (mut ui, _flow, store) =
        crate::common::layout::load((800, 600), RegistrationOrder::AfterResolver);
    ui.strings = Some(Rc::new(Strings(Rc::clone(&store))));
    if !with_registry {
        // The preference registry was never initialised. The controls still build; nothing
        // has a label, a range or a choice list.
        preferences::clear();
    }
    ui
}

fn screen() -> (UiSystem, GamePlayScreen) {
    let mut ui = ui_system(true);
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds");
    (ui, s)
}

/// The `UiOption` index of one preference on the page.
fn index_of(p: &PlayerOptionPage, pref: &str) -> usize {
    p.options
        .iter()
        .position(|o| o.preference == pref)
        .unwrap_or_else(|| panic!("{pref}"))
}

/// The element a preference's control is.
fn control(p: &PlayerOptionPage, pref: &str) -> ElemHandle {
    p.options[index_of(p, pref)].element
}

/// The text on one element, or `""`.
fn text_of(ui: &mut UiSystem, h: ElemHandle) -> String {
    ui.text_element_mut(h)
        .map(|t| t.glyphs.inq_text(false))
        .unwrap_or_default()
}

/// The rows of one drop-down, as `(caption, 0x10000025)`.
fn rows(ui: &mut UiSystem, menu: ElemHandle) -> Vec<(String, Option<i32>)> {
    (0..dereth_ui::widgets::menu::num_items(ui, menu))
        .map(|k| {
            let item = dereth_ui::widgets::menu::get_item(ui, menu, k).expect("an item");
            let v = dereth_ui_screens::bind::attr_int(ui, item, 0x1000_0025);
            (text_of(ui, item), v)
        })
        .collect()
}

/// Show every ancestor of `h`. The options page is one of sixteen stacked pages and starts hidden;
/// a panel-visibility notice shows it in the client, and a hidden ancestor
/// makes the element unhittable. Same helper as `options_client_page.rs`.
fn reveal(ui: &mut UiSystem, h: ElemHandle) {
    let mut h = h;
    loop {
        ui.set_visible(h, true);
        match ui.parent(h) {
            Some(p) => h = p,
            None => break,
        }
    }
    ui.drain_outbox();
}

/// Drain the outbox into the screen through the element manager's message-dispatch path.
fn pump(ui: &mut UiSystem, s: &mut GamePlayScreen) {
    for _ in 0..8 {
        let out = ui.drain_outbox();
        if out.is_empty() {
            break;
        }
        for d in out {
            if let dereth_ui::Delivery::Element { msg, .. } = d {
                s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
            }
        }
    }
}

// -------------------------------------------------------------------------------------------
// 1. The count
// -------------------------------------------------------------------------------------------

/// The eight drop downs carry thirty two entries between them.
#[test]
fn the_eight_drop_downs_carry_thirty_two_entries_between_them() {
    let (mut ui, s) = screen();
    let p = &s.config_page;
    assert_eq!(p.failures, 0, "every row built");

    // (preference, rows in its drop-down)
    let want: [(&str, usize); 8] = [
        ("Sound.SoundFeatures", 2),
        ("Display.Resolution", 0),
        ("Render.LandscapeTextureDetail", 5),
        ("Render.EnvironmentTextureDetail", 5),
        ("Render.TextureFiltering", 4),
        ("Render.LandscapeDrawDistance", 6),
        ("UI.ChatFontFace", 5),
        ("UI.ChatFontSize", 5),
    ];
    let menus: Vec<&str> = p
        .retail_options()
        .filter(|o| o.control == OptionControl::Menu)
        .map(|o| o.preference)
        .collect();
    assert_eq!(menus.len(), 8, "eight add_menu_option calls: {menus:?}");
    assert_eq!(
        menus,
        want.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
        "in option initialization order"
    );

    let mut total = 0;
    for (pref, n) in want {
        let menu = control(p, pref);
        // Counted off the list box, not off the page's own bookkeeping.
        let live = dereth_ui::widgets::menu::num_items(&ui, menu);
        assert_eq!(
            live,
            n,
            "{pref} holds {live} rows, expected {n}: {:?}",
            rows(&mut ui, menu)
        );
        assert_eq!(
            p.options[index_of(p, pref)].entries.len(),
            n,
            "{pref} — the page's record"
        );
        total += n;
        // **Every** menu has a popup, entries or not: the popup is made while the menu is
        // initialised, before any list exists.
        assert!(
            dereth_ui::widgets::menu::popup_handle(&ui, menu).is_some(),
            "{pref} has no popup — make_popup did not run"
        );
        assert!(
            dereth_ui::widgets::menu::list_box_handle(&ui, menu).is_some(),
            "{pref} list box"
        );
    }
    assert_eq!(total, 32);
    // This client's three rows from another era list the world's own and their styles, as
    // literal text: three styles for the ground and the sky, two for the objects.
    for o in p.landscape_options() {
        let texts = rows(&mut ui, o.element);
        let want: &[i32] = if o.preference == dereth_client_contract::options::landscape::OBJECTS {
            &[0, 2, 3]
        } else {
            &[0, 1, 2, 3]
        };
        assert_eq!(texts.len(), want.len(), "{}: {texts:?}", o.preference);
        assert_eq!(o.entries.iter().map(|e| e.1).collect::<Vec<_>>(), want);
    }
    assert_eq!(
        p.menu_entries,
        32 + 8 + 3,
        "the page's own counter agrees with the eleven list boxes"
    );
    assert_eq!(p.menu_popups, 11, "the eight and the three era rows");

    for (pref, n) in want {
        let menu = control(p, pref);
        assert_eq!(
            dereth_ui::widgets::menu::open(&mut ui, menu),
            n != 0,
            "{pref} with {n} rows"
        );
        dereth_ui::widgets::menu::close(&mut ui, menu);
    }
}

// -------------------------------------------------------------------------------------------
// 2. The rows themselves
// -------------------------------------------------------------------------------------------

/// The captions are the shipped `Preference` table's strings and the `0x10000025` values are the
/// **registered choice values**, which for two of the seven are neither the index nor anything a
/// reader would guess.
///
/// Spelled out as literals rather than read back through `store::ENUM_CHOICES`, because a test
/// that reads a table through the same symbol that wrote it cannot detect a wrong table. The two
/// that matter:
///
/// * `Render.LandscapeTextureDetail` / `EnvironmentTextureDetail` — the value array is
///   `[4, 3, 2, 1, 0]`, so the scale runs **backwards** and *Very High* is **0**;
/// * `Render.LandscapeDrawDistance` — the value array is `[3, 5, 8, 11, 15, 25]`, so *Extreme*
///   is **25** and *Very Low* is **3**.
///
/// Picking either wrong gives a drop-down that looks right and sets the wrong thing, which is the
/// kind of defect nobody reports.
#[test]
fn every_entry_carries_the_registered_choice_value_and_the_shipped_caption() {
    let (mut ui, s) = screen();
    let p = &s.config_page;
    let got = |ui: &mut UiSystem, pref: &str| rows(ui, control(p, pref));

    assert_eq!(
        got(&mut ui, "Sound.SoundFeatures"),
        vec![("Stereo".into(), Some(0)), ("Mono".into(), Some(1))]
    );
    let detail = vec![
        ("Very Low".to_string(), Some(4)),
        ("Low".to_string(), Some(3)),
        ("Medium".to_string(), Some(2)),
        ("High".to_string(), Some(1)),
        ("Very High".to_string(), Some(0)),
    ];
    assert_eq!(
        got(&mut ui, "Render.LandscapeTextureDetail"),
        detail,
        "the scale runs backwards"
    );
    assert_eq!(got(&mut ui, "Render.EnvironmentTextureDetail"), detail);
    assert_eq!(
        got(&mut ui, "Render.TextureFiltering"),
        vec![
            ("Bilinear".to_string(), Some(0)),
            ("Trilinear".to_string(), Some(1)),
            ("Sharp".to_string(), Some(2)),
            ("Anisotropic".to_string(), Some(3)),
        ]
    );
    assert_eq!(
        got(&mut ui, "Render.LandscapeDrawDistance"),
        vec![
            ("Very Low".to_string(), Some(3)),
            ("Low".to_string(), Some(5)),
            ("Medium".to_string(), Some(8)),
            ("High".to_string(), Some(11)),
            ("Very High".to_string(), Some(15)),
            ("Extreme".to_string(), Some(25)),
        ],
        "not the indices"
    );
    assert_eq!(
        got(&mut ui, "UI.ChatFontFace"),
        vec![
            ("Arial".to_string(), Some(0)),
            ("Courier New".to_string(), Some(1)),
            ("Palatino Linotype".to_string(), Some(2)),
            ("Tahoma".to_string(), Some(3)),
            ("Times New Roman".to_string(), Some(4)),
        ]
    );
    assert_eq!(
        got(&mut ui, "UI.ChatFontSize"),
        vec![
            ("Tiny".to_string(), Some(0)),
            ("Small".to_string(), Some(1)),
            ("Medium".to_string(), Some(2)),
            ("Large".to_string(), Some(3)),
            ("Extra Large".to_string(), Some(4)),
        ]
    );

    // Every row is mouse-visible: inserting a row marks it mouse-visible. Without it
    // the rows draw and cannot be clicked, which is a drop-down that looks fixed and is not.
    for o in p
        .options
        .iter()
        .filter(|o| o.control == OptionControl::Menu)
    {
        for k in 0..dereth_ui::widgets::menu::num_items(&ui, o.element) {
            let item = dereth_ui::widgets::menu::get_item(&ui, o.element, k).expect("item");
            assert!(
                ui.node(item).expect("live").is_mouse_visible,
                "{} row {k} is not clickable",
                o.preference
            );
        }
    }
}

// -------------------------------------------------------------------------------------------
// 3. Refresh — "it shows my current settings"
// -------------------------------------------------------------------------------------------

/// Behaviour: options.client-page.the-drop-downs-list-their-registered-choices-and-open-on-the-stored-one
/// The menu's refresh selects the row whose stored value equals the option's current value,
/// and selecting a row copies that row's text onto the **menu's own face** — which is the only
/// thing a player sees with the drop-down shut.
///
/// Driven by putting a value in the store and re-reading the page the way
/// the page's visibility change does on show, so this is the production path
/// and not a direct call to `Refresh`.
#[test]
fn the_page_opens_with_the_stored_setting_selected() {
    let (mut ui, mut s) = screen();
    // Attribute 8 names the text element inside the menu that a new selection copies the
    // chosen row's text into. The shipped menu row template `0x10000222` carries exactly two
    // children under the control `0x10000224`: this one and the drop-arrow `0x10000356`.
    let face_id = ElementId(0x1000_0355);

    // `Render.LandscapeDrawDistance` is the one where the value and the index disagree most: 25 is
    // the sixth row, and a page that matched on the index would show `Low`.
    store::set_value("Render.LandscapeDrawDistance", PrefValue::Int(25));
    store::set_value("UI.ChatFontFace", PrefValue::Int(3));
    store::set_value("Render.LandscapeTextureDetail", PrefValue::Int(0));

    // `on_visibility_changed(true)` -> -> each value read, then the
    // page's own `Refresh` fan-out.
    s.config_page.on_visibility_changed(&mut ui, true);

    let want: [(&str, &str, i32); 3] = [
        ("Render.LandscapeDrawDistance", "Extreme", 25),
        ("UI.ChatFontFace", "Tahoma", 3),
        ("Render.LandscapeTextureDetail", "Very High", 0),
    ];
    for (pref, caption, value) in want {
        let i = index_of(&s.config_page, pref);
        assert_eq!(
            s.config_page.options[i].current,
            PrefValue::Int(value),
            "{pref} current"
        );
        let menu = s.config_page.options[i].element;
        let sel = dereth_ui::widgets::menu::selected_index(&ui, menu);
        let item = usize::try_from(sel)
            .ok()
            .and_then(|k| dereth_ui::widgets::menu::get_item(&ui, menu, k))
            .unwrap_or_else(|| panic!("{pref} has no selected row (index {sel})"));
        assert_eq!(
            dereth_ui_screens::bind::attr_int(&ui, item, 0x1000_0025),
            Some(value),
            "{pref} selected the wrong row"
        );
        assert_eq!(text_of(&mut ui, item), caption);
        let face = ui
            .get_child_recursive(menu, face_id)
            .unwrap_or_else(|| panic!("{pref} face"));
        assert_eq!(
            text_of(&mut ui, face),
            caption,
            "{pref}: the shut drop-down must read `{caption}`"
        );
    }

    // A value no row carries falls back to row 0 — the refresh's tail picks item 0.
    store::set_value("Render.TextureFiltering", PrefValue::Int(99));
    s.config_page.on_visibility_changed(&mut ui, true);
    let menu = control(&s.config_page, "Render.TextureFiltering");
    assert_eq!(
        dereth_ui::widgets::menu::selected_index(&ui, menu),
        0,
        "falls back to item 0"
    );
}

// -------------------------------------------------------------------------------------------
// 4. The gesture
// -------------------------------------------------------------------------------------------

/// **The headline behaviour, driven the way a player drives it.** A press on the drop-down opens
/// the popup; a press on a row selects it; the page applies the choice; and the value survives a
/// save-and-reload of `UserPreferences.ini`.
///
/// Nothing here calls `open`, `set_selected_item`, `on_element_message` or `apply` by hand: every
/// step is `UiSystem::mouse_down` at a screen position that [`UiSystem::hit_test_screen`] is first
/// asserted to resolve to the element being pressed. That distinction is why this project keeps
/// finding green suites over unreachable code — a test that broadcasts the message directly
/// measures the arm and not the button.
///
/// `Sound.SoundFeatures` is the menu used because it is the first row of the page and therefore
/// on screen at 800x600; the rows further down sit below the option box's own scroll window.
#[test]
fn a_real_press_opens_the_drop_down_and_a_real_press_on_a_row_applies_it() {
    let (mut ui, mut s) = screen();
    let menu = control(&s.config_page, "Sound.SoundFeatures");
    reveal(&mut ui, menu);

    // Starting state: the registered default is `Stereo` = 0.
    assert_eq!(
        store::inq_value("Sound.SoundFeatures"),
        Some(PrefValue::Int(0))
    );
    let popup = dereth_ui::widgets::menu::popup_handle(&ui, menu).expect("popup");
    assert!(
        !ui.node(popup).expect("live").region.flags.visible,
        "building the popup leaves it hidden"
    );

    // --- the press that opens it -----------------------------------------------------------
    {
        let b = ui.screen_box(menu);
        let (cx, cy) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
        assert_eq!(
            ui.hit_test_screen(cx, cy),
            Some(menu),
            "the press lands on the drop-down"
        );
        ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, cx, cy);
        ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, cx, cy, false);
        pump(&mut ui, &mut s);
    }
    assert!(
        ui.node(popup).expect("live").region.flags.visible,
        " — a menu with rows opens on its own message 1"
    );

    // --- the press that chooses `Mono` -------------------------------------------------------
    let mono = dereth_ui::widgets::menu::get_item(&ui, menu, 1).expect("row 1");
    assert_eq!(text_of(&mut ui, mono), "Mono");
    ui.requests.clear();
    {
        let b = ui.screen_box(mono);
        let (cx, cy) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
        assert_eq!(
            ui.hit_test_screen(cx, cy),
            Some(mono),
            "the press lands on the row"
        );
        ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, cx, cy);
        ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, cx, cy, false);
        pump(&mut ui, &mut s);
    }

    // The option row takes the menu's selection message and applies the new value at once.
    let i = index_of(&s.config_page, "Sound.SoundFeatures");
    assert_eq!(
        s.config_page.options[i].current,
        PrefValue::Int(1),
        "current is `Mono`"
    );
    assert_eq!(
        store::inq_value("Sound.SoundFeatures"),
        Some(PrefValue::Int(1)),
        "the modified preference reached the store"
    );
    let reqs = ui.requests.take();
    assert!(
        reqs.contains(&UiRequest::SetPreference(
            "Sound.SoundFeatures",
            PrefValue::Int(1)
        )),
        "the subsystem is told: {reqs:?}"
    );
    // `new_selection` shut it again — the client's message-4 arm runs `new_selection(true)` and then closes the menu
    assert!(
        !ui.node(popup).expect("live").region.flags.visible,
        "the pick closes the popup"
    );

    // --- and it survives the trip through the file ------------------------------------------
    // The preferences are saved and loaded again, over the registry this page just
    // wrote. No file is touched: `save` builds the text and `parse` reads it back.
    let text = dereth_ui_screens::options::store::save().to_text();
    assert!(
        text.contains("[Sound]"),
        "one section per category:\n{text}"
    );
    assert!(
        text.contains("SoundFeatures=Mono\r\n"),
        "the choice label, not a number:\n{text}"
    );
    store::init(); // every variable back to its registered default
    assert_eq!(
        store::inq_value("Sound.SoundFeatures"),
        Some(PrefValue::Int(0))
    );
    let ini = dereth_ui::persist::preferences::UserPreferences::parse(&text).expect("parses");
    let (applied, _) = store::load(&ini);
    assert!(applied >= 34, "the whole profile came back: {applied}");
    assert_eq!(
        store::inq_value("Sound.SoundFeatures"),
        Some(PrefValue::Int(1)),
        "the setting the player chose survived the save/reload cycle"
    );
}

// -------------------------------------------------------------------------------------------
// 5. The calibration
// -------------------------------------------------------------------------------------------

/// The instrument must be able to report absence, and it must report it for the *right* reason.
///
/// With the preference registry never initialised, the preference lookup answers `false` for
/// all 30 controls, so the control setup returns before it ever reaches the enum-choices
/// query — no labels and no entries. The **popups** still exist, because the popup is made
/// while the menu is initialised and has nothing to do with the registry: that is what
/// separates "no registry" from "no asset environment", and it is the third state §7.8 asks for.
#[test]
fn with_no_preference_registry_no_drop_down_has_an_entry() {
    let mut ui = ui_system(false);
    assert_eq!(preferences::len(), 0, "the registry is empty");
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen still builds");
    let p = &s.config_page;
    assert_eq!(
        p.retail_options().count(),
        30,
        "all 30 controls are still bound"
    );
    // This client's three era rows ask the option value store, not this registry, so they keep
    // their literal entries (four, four and three).
    assert_eq!(p.menu_entries, 11, "and not one retail drop-down has a row");
    assert_eq!(
        p.menu_popups, 11,
        "…while all eleven popups exist: make_popup does not ask the registry"
    );
    for o in p
        .retail_options()
        .filter(|o| o.control == OptionControl::Menu)
    {
        assert_eq!(
            dereth_ui::widgets::menu::num_items(&ui, o.element),
            0,
            "{}",
            o.preference
        );
        assert_eq!(o.label, None, "{} — no caption either", o.preference);
    }
    // Put the registry back for whatever runs next in this thread.
    preferences::init_ui_preferences();
}

/// Behaviour: options.client-page.the-terrain-and-sky-modes-are-chosen-on-the-graphics-section
/// This client's Terrain Mode and Sky Mode close the Graphics section, after its retail rows: each
/// lists the world's own and the three named styles as literal text, opens on the stored choice,
/// and a real press on a row writes the landscape option, which survives a save and reload of the
/// preferences file in its one-word spelling; Restore Defaults puts both back to World Default.
#[test]
fn the_terrain_mode_drop_down_lists_the_three_named_modes_and_a_press_chooses_one() {
    use dereth_client_contract::options::landscape;
    let (mut ui, mut s) = screen();
    // The two rows sit between the Graphics section's last retail row and the Textures header.
    let order: Vec<&str> = s.config_page.options.iter().map(|o| o.preference).collect();
    let at = |p: &str| order.iter().position(|o| *o == p).expect("on the page");
    assert_eq!(at(landscape::GROUND), at("Render.DegradeDistance") + 1);
    assert_eq!(at(landscape::SKY), at(landscape::GROUND) + 1);
    assert_eq!(at(landscape::OBJECTS), at(landscape::SKY) + 1);
    assert_eq!(
        at("Render.LandscapeTextureDetail"),
        at(landscape::OBJECTS) + 1
    );

    let menu = control(&s.config_page, landscape::GROUND);
    let texts: Vec<String> = rows(&mut ui, menu).into_iter().map(|(t, _)| t).collect();
    assert_eq!(
        texts,
        [
            "World Default",
            "Palette Shift",
            "Legacy Blend",
            "Modern Blend"
        ]
    );
    let sky = control(&s.config_page, landscape::SKY);
    let texts: Vec<String> = rows(&mut ui, sky).into_iter().map(|(t, _)| t).collect();
    assert_eq!(
        texts,
        [
            "World Default",
            "Legacy Software",
            "Legacy Hardware",
            "Modern"
        ]
    );
    let objects = control(&s.config_page, landscape::OBJECTS);
    let texts: Vec<String> = rows(&mut ui, objects).into_iter().map(|(t, _)| t).collect();
    assert_eq!(texts, ["World Default", "Legacy", "Modern"]);
    assert_eq!(
        store::inq_value(landscape::GROUND),
        Some(PrefValue::Int(landscape::WORLD_DEFAULT))
    );

    // Scroll the row into the option box's window, then press as a player does.
    let row = s.config_page.options[index_of(&s.config_page, landscape::GROUND)].row;
    let list = s
        .config_page
        .option_box
        .as_ref()
        .expect("option box")
        .handle;
    reveal(&mut ui, menu);
    dereth_ui::widgets::listbox::scroll_item_to_view(&mut ui, list, row);
    pump(&mut ui, &mut s);
    let press = |ui: &mut UiSystem, s: &mut GamePlayScreen, h: ElemHandle| {
        let b = ui.screen_box(h);
        let (cx, cy) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
        assert_eq!(ui.hit_test_screen(cx, cy), Some(h), "the press lands");
        ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, cx, cy);
        ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, cx, cy, false);
        pump(ui, s);
    };
    press(&mut ui, &mut s, menu);
    let palette = dereth_ui::widgets::menu::get_item(&ui, menu, 1).expect("row 1");
    assert_eq!(text_of(&mut ui, palette), "Palette Shift");
    ui.requests.clear();
    press(&mut ui, &mut s, palette);
    assert_eq!(
        store::inq_value(landscape::GROUND),
        Some(PrefValue::Int(1)),
        "the choice reached the option store"
    );
    let reqs = ui.requests.take();
    assert!(
        reqs.contains(&UiRequest::SetPreference(
            landscape::GROUND,
            PrefValue::Int(1)
        )),
        "the scene is told: {reqs:?}"
    );
    let text = store::save().to_text();
    assert!(text.contains("Ground=PaletteShift\r\n"), "{text}");
    assert!(text.contains("Sky=World\r\n"), "{text}");
    assert!(text.contains("Objects=World\r\n"), "{text}");
    store::init();
    let ini = dereth_ui::persist::preferences::UserPreferences::parse(&text).expect("parses");
    store::load(&ini);
    assert_eq!(store::inq_value(landscape::GROUND), Some(PrefValue::Int(1)));

    // Restore Defaults puts both back to World Default.
    ui.requests.clear();
    s.config_page.restore_default_values(&mut ui);
    let reqs = ui.requests.take();
    for name in [landscape::GROUND, landscape::SKY, landscape::OBJECTS] {
        assert!(
            reqs.contains(&UiRequest::SetPreference(
                name,
                PrefValue::Int(landscape::WORLD_DEFAULT)
            )),
            "{name} not restored: {reqs:?}"
        );
    }
    assert_eq!(
        store::inq_value(landscape::GROUND),
        Some(PrefValue::Int(landscape::WORLD_DEFAULT))
    );
}
