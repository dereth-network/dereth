//! The five panel-bound option checkboxes are option checkboxes in the shipped tree, place glyphs
//! of their caption, real presses toggle and raise the named SetPlayerOption request; fellowship
//! create opens at the character's options.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;
use crate::common::layout::Strings;
use std::cell::RefCell;
use std::rc::Rc;

use crate::common::*;
use dereth_ui::{Delivery, ElemHandle, ElementId, Screen, UiSystem};
use dereth_ui_screens::panels::remaining::RemainingPanels;
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::view::{GameView, PlayerOption, UiRequest};

// -------------------------------------------------------------------------------------------
// Literals, from retail — not re-read through the symbols under test
// -------------------------------------------------------------------------------------------

/// The Allegiance panel's one check box: element `0x10000262`, bound to player-option ordinal
/// 1, `IgnoreAllegianceRequests`.
const ALLEGIANCE_IGNORE: (u32, PlayerOption, &str) = (
    0x1000_0262,
    PlayerOption::IgnoreAllegianceRequests,
    "ID_PlayerOption_IgnoreAllegianceRequests",
);

/// The Fellowship panel's four, in the order it binds them, with the player-option ordinal each
/// one carries.
const FELLOWSHIP_BOXES: [(u32, PlayerOption, &str); 4] = [
    // ordinal 2
    (
        0x1000_0270,
        PlayerOption::IgnoreFellowshipRequests,
        "ID_PlayerOption_IgnoreFellowshipRequests",
    ),
    // ordinal 0x12
    (
        0x1000_0271,
        PlayerOption::FellowshipAutoAcceptRequests,
        "ID_PlayerOption_FellowshipAutoAcceptRequests",
    ),
    // ordinal 0x0F
    (
        0x1000_0272,
        PlayerOption::FellowshipShareXP,
        "ID_PlayerOption_FellowshipShareXP",
    ),
    // ordinal 0x11
    (
        0x1000_0273,
        PlayerOption::FellowshipShareLoot,
        "ID_PlayerOption_FellowshipShareLoot",
    ),
];

/// The option check box's element type is this id, and it is the type every one of the five
/// bindings above casts to.
const UIOPTION_CHECKBOX: u32 = 0x1000_0035;

/// The button's mouse-up checks this toggle attribute (`0x0B`) before flipping the checked bit
/// (`0x0E`), and that same pair is what the panel writes and reads.
const ATTR_TOGGLE_BUTTON: u32 = 0x0B;
/// The default character-options word — the word a shipped character starts with, and the
/// oracle for "what should the Create screen's four boxes look like".
const DEFAULT_CHARACTER_OPTION: u32 = 0x50C4_A54A;

/// The wire option masks for the five options above. Spelled out here rather than read
/// back through `dereth_client_model`, which this crate cannot see and which is the table under test
/// everywhere else.
const MASK_IGNORE_ALLEGIANCE: u32 = 0x0000_0004;
const MASK_IGNORE_FELLOWSHIP: u32 = 0x0000_0008;
const MASK_SHARE_XP: u32 = 0x0004_0000;
const MASK_SHARE_LOOT: u32 = 0x0010_0000;
const MASK_AUTO_ACCEPT: u32 = 0x2000_0000;

// -------------------------------------------------------------------------------------------
// Harness — the shipped `0x21000005` tree, with a real string resolver
// -------------------------------------------------------------------------------------------

fn env(with_strings: bool) -> UiSystem {
    let (mut ui, _flow, store) =
        crate::common::layout::load((800, 600), RegistrationOrder::AfterResolver);
    if with_strings {
        ui.strings = Some(Rc::new(Strings(Rc::clone(&store))));
    }
    ui
}

fn pump(ui: &mut UiSystem, s: &mut GamePlayScreen) {
    for _ in 0..16 {
        let batch = ui.drain_outbox();
        if batch.is_empty() {
            return;
        }
        for d in batch {
            if let Delivery::Element { msg, .. } = d {
                s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
            }
        }
    }
}

/// A `PlayerModule` stand-in holding one `options_` word, read through the same masks the client
/// reads it through.
#[derive(Debug, Clone)]
struct Module {
    options: Rc<RefCell<u32>>,
}

impl Module {
    fn new(word: u32) -> Self {
        Self {
            options: Rc::new(RefCell::new(word)),
        }
    }
    fn mask(o: PlayerOption) -> Option<u32> {
        Some(match o {
            PlayerOption::IgnoreAllegianceRequests => MASK_IGNORE_ALLEGIANCE,
            PlayerOption::IgnoreFellowshipRequests => MASK_IGNORE_FELLOWSHIP,
            PlayerOption::FellowshipShareXP => MASK_SHARE_XP,
            PlayerOption::FellowshipShareLoot => MASK_SHARE_LOOT,
            PlayerOption::FellowshipAutoAcceptRequests => MASK_AUTO_ACCEPT,
            _ => return None,
        })
    }
}

impl GameView for Module {
    fn player_option(&self, o: PlayerOption) -> bool {
        Self::mask(o).is_some_and(|m| *self.options.borrow() & m != 0)
    }
}

/// The shipped gameplay screen with `RemainingPanels` bound off its root, exactly as
/// `dereth_client_shell::hud::Hud::drive` binds it.
fn screen(with_strings: bool) -> (UiSystem, GamePlayScreen, RemainingPanels) {
    let mut ui = env(with_strings);
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds from the shipped layout");
    pump(&mut ui, &mut s);
    let root = s.root().expect("the gameplay root");
    let mut panels = RemainingPanels::default();
    panels.post_init(&mut ui, root);
    ui.requests.clear();
    (ui, s, panels)
}

fn find(ui: &UiSystem, s: &GamePlayScreen, id: u32) -> ElemHandle {
    let root = s.root().expect("the gameplay root");
    ui.get_child_recursive(root, ElementId(id))
        .unwrap_or_else(|| panic!("{id:#010X} is not in the shipped tree"))
}

/// The five, in post-init order: allegiance first, then the fellowship four.
fn all_five() -> Vec<(u32, PlayerOption, &'static str)> {
    let mut v = vec![ALLEGIANCE_IGNORE];
    v.extend(FELLOWSHIP_BOXES);
    v
}

/// Show every ancestor of `h` — the panels start hidden and a hidden ancestor is unhittable.
fn reveal(ui: &mut UiSystem, mut h: ElemHandle) {
    loop {
        ui.set_visible(h, true);
        match ui.parent(h) {
            Some(p) => h = p,
            None => break,
        }
    }
    ui.drain_outbox();
}

/// One frame of the host's drive.
fn frame(ui: &mut UiSystem, panels: &mut RemainingPanels, view: &dyn GameView) {
    panels.update(ui, view);
}

// -------------------------------------------------------------------------------------------
// 1. The calibration — the instrument is pointed at five real check boxes
// -------------------------------------------------------------------------------------------

/// The five panel option boxes are uioption checkboxes in the shipped tree.
#[test]
fn the_five_panel_option_boxes_are_uioption_checkboxes_in_the_shipped_tree() {
    let (ui, s, _panels) = screen(true);
    for (id, _option, _token) in all_five() {
        let h = find(&ui, &s, id);
        let n = ui.node(h).expect("live");
        assert_eq!(
            n.ty().0,
            UIOPTION_CHECKBOX,
            "{id:#010X} is not a checkbox option"
        );
        assert_eq!(
            n.merged_properties().get_bool(ATTR_TOGGLE_BUTTON),
            Some(true),
            "{id:#010X} does not carry 0x0B, so MouseUp would never flip 0x0E"
        );
    }
    let frame_h = find(&ui, &s, 0x1000_026B);
    for (id, _, _) in FELLOWSHIP_BOXES {
        assert!(
            ui.get_child_recursive(frame_h, ElementId(id)).is_some(),
            "{id:#010X} is not under the not-in-fellowship frame"
        );
    }
}

// -------------------------------------------------------------------------------------------
// 2. The caption
// -------------------------------------------------------------------------------------------

/// Behaviour: options.panel-boxes.each-panel-option-box-draws-its-caption-and-a-press-names-its-option
/// Every panel option box places the glyphs of its id playeroption caption.
#[test]
fn every_panel_option_box_places_the_glyphs_of_its_id_playeroption_caption() {
    let (mut ui, s, _panels) = screen(true);
    let table = dereth_ui_screens::options::character::table(&ui);
    let mut captioned = 0;
    for (id, _option, token) in all_five() {
        let h = find(&ui, &s, id);
        // What the string table says this token is, resolved independently of the element.
        let want = ui
            .resolve_string(
                table,
                dereth_primitives::num::hash::str_hash(token.as_bytes()),
            )
            .unwrap_or_else(|| panic!("{token} is not in string table {:#010X}", table.0));
        assert!(!want.is_empty(), "{token} resolves to the empty string");
        let (text, glyphs, advance) = placed(&mut ui, h);
        assert_eq!(text, want, "{id:#010X} draws the wrong caption");
        assert_eq!(
            glyphs,
            want.chars().count(),
            "{id:#010X} laid out no glyph list"
        );
        assert!(
            advance > 0,
            "{id:#010X}'s caption was never measured by a font"
        );
        captioned += 1;
    }
    assert_eq!(captioned, 5, "five panel-bound check boxes, five captions");
}

// -------------------------------------------------------------------------------------------
// 3. The gesture
// -------------------------------------------------------------------------------------------

/// A real press on the Allegiance tab's box toggles it and names its option.
///
/// Nothing here broadcasts a message: the press is `UiSystem::mouse_down`/`mouse_up` at a screen
/// position [`UiSystem::hit_test_screen`] is first asserted to resolve to the box. That is the
/// distinction this project keeps needing — a test that broadcast message 1 by hand would have
/// been green over a control the player cannot reach.
///
/// **No datagram leaves this process**: the assertion is that
/// `UiRequest::SetPlayerOption(IgnoreAllegianceRequests, true)` was *raised*, and the queue is
/// drained here rather than sent.
#[test]
fn a_real_press_on_the_allegiance_box_toggles_it_and_names_the_option() {
    let (mut ui, mut s, mut panels) = screen(true);
    // A character with the option **off**, which is the shipped default for this one.
    let view = Module::new(DEFAULT_CHARACTER_OPTION);
    assert_eq!(
        DEFAULT_CHARACTER_OPTION & MASK_IGNORE_ALLEGIANCE,
        0,
        "off in the default character options"
    );
    frame(&mut ui, &mut panels, &view);

    let h = find(&ui, &s, ALLEGIANCE_IGNORE.0);
    assert!(
        !checked(&ui, h),
        "it opens unticked because the character has it off"
    );
    reveal(&mut ui, h);
    ui.requests.clear();

    let b = ui.screen_box(h);
    let (cx, cy) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
    assert_eq!(
        ui.hit_test_screen(cx, cy),
        Some(h),
        "the press lands on the check box"
    );
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, cx, cy);
    ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, cx, cy, false);
    // The button's mouse-up has already flipped 0x0E by the time message 1 goes
    // out; the panel's arm reads it back.
    assert!(checked(&ui, h), "the press did not flip 0x0E");
    for _ in 0..8 {
        let batch = ui.drain_outbox();
        if batch.is_empty() {
            break;
        }
        for d in batch {
            if let Delivery::Element { msg, .. } = d {
                s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(&mut ui), &msg);
                panels.on_element_message(&mut ui, &msg, &view);
            }
        }
    }

    let reqs = ui.requests.take();
    assert!(
        reqs.contains(&UiRequest::SetPlayerOption(
            PlayerOption::IgnoreAllegianceRequests,
            true
        )),
        " did not reach the module: {reqs:?}"
    );
}

/// The same gesture on the Fellowship Create screen's *Share XP* box, which is the one the Create
/// button itself reads (`FellowshipPanel`'s `view.player_option(FellowshipShareXP)`), so an
/// unwired box here makes the **request the player sends** wrong, not just the picture.
#[test]
fn a_real_press_on_the_share_xp_box_names_its_option() {
    let (mut ui, mut s, mut panels) = screen(true);
    // Start from a character with Share XP **on** (the shipped default), so the press turns it
    // off and the request carries `false` — a direction a hard-coded `true` could not fake.
    let view = Module::new(DEFAULT_CHARACTER_OPTION);
    assert_ne!(
        DEFAULT_CHARACTER_OPTION & MASK_SHARE_XP,
        0,
        "on in the default character options"
    );
    frame(&mut ui, &mut panels, &view);

    let h = find(&ui, &s, FELLOWSHIP_BOXES[2].0);
    assert!(
        checked(&ui, h),
        "Share XP opens ticked on a shipped character"
    );
    reveal(&mut ui, h);
    ui.requests.clear();

    let b = ui.screen_box(h);
    let (cx, cy) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
    assert_eq!(
        ui.hit_test_screen(cx, cy),
        Some(h),
        "the press lands on the check box"
    );
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, cx, cy);
    ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, cx, cy, false);
    assert!(!checked(&ui, h), "the press did not clear 0x0E");
    for _ in 0..8 {
        let batch = ui.drain_outbox();
        if batch.is_empty() {
            break;
        }
        for d in batch {
            if let Delivery::Element { msg, .. } = d {
                s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(&mut ui), &msg);
                panels.on_element_message(&mut ui, &msg, &view);
            }
        }
    }
    let reqs = ui.requests.take();
    assert!(
        reqs.contains(&UiRequest::SetPlayerOption(
            PlayerOption::FellowshipShareXP,
            false
        )),
        "the Share XP box reached nothing: {reqs:?}"
    );
}

/// The fellowship create screen opens at the character s options not all off.
#[test]
fn the_fellowship_create_screen_opens_at_the_character_s_options_not_all_off() {
    let (mut ui, s, mut panels) = screen(true);
    let view = Module::new(DEFAULT_CHARACTER_OPTION);
    frame(&mut ui, &mut panels, &view);

    let want: [(u32, bool); 4] = [
        (
            FELLOWSHIP_BOXES[0].0,
            DEFAULT_CHARACTER_OPTION & MASK_IGNORE_FELLOWSHIP != 0,
        ),
        (
            FELLOWSHIP_BOXES[1].0,
            DEFAULT_CHARACTER_OPTION & MASK_AUTO_ACCEPT != 0,
        ),
        (
            FELLOWSHIP_BOXES[2].0,
            DEFAULT_CHARACTER_OPTION & MASK_SHARE_XP != 0,
        ),
        (
            FELLOWSHIP_BOXES[3].0,
            DEFAULT_CHARACTER_OPTION & MASK_SHARE_LOOT != 0,
        ),
    ];
    assert_eq!(
        want.iter().filter(|(_, v)| *v).count(),
        2,
        "the default character options tick exactly two of the four"
    );
    for (id, v) in want {
        assert_eq!(
            checked(&ui, find(&ui, &s, id)),
            v,
            "{id:#010X} opened at the wrong value"
        );
    }
    // And the allegiance box follows the same word.
    assert_eq!(
        checked(&ui, find(&ui, &s, ALLEGIANCE_IGNORE.0)),
        DEFAULT_CHARACTER_OPTION & MASK_IGNORE_ALLEGIANCE != 0
    );

    // A character with the opposite word ticks the other two — so this is a read, not a constant.
    let (mut ui, s, mut panels) = screen(true);
    let view = Module::new(!DEFAULT_CHARACTER_OPTION);
    frame(&mut ui, &mut panels, &view);
    for (id, v) in want {
        assert_eq!(
            checked(&ui, find(&ui, &s, id)),
            !v,
            "{id:#010X} ignored the character's word"
        );
    }
}

// -------------------------------------------------------------------------------------------
// 5. The instrument can report absence
// -------------------------------------------------------------------------------------------

/// With no string table the boxes report no caption rather than a wrong one.
#[test]
fn with_no_string_table_the_boxes_report_no_caption_rather_than_a_wrong_one() {
    let (mut ui, s, _panels) = screen(false);
    for (id, _option, _token) in all_five() {
        let h = find(&ui, &s, id);
        let (text, glyphs, _w) = placed(&mut ui, h);
        assert!(
            text.is_empty(),
            "{id:#010X} drew {text:?} with no string table"
        );
        assert_eq!(glyphs, 0);
    }
}

/// A world of an era, over the module's options.
#[derive(Debug)]
struct EraWorld {
    module: Module,
    era: dereth_ui_screens::view::EraView,
}

impl GameView for EraWorld {
    fn player_option(&self, o: PlayerOption) -> bool {
        self.module.player_option(o)
    }
    fn era(&self) -> Option<&dereth_ui_screens::view::EraView> {
        Some(&self.era)
    }
}

/// Behaviour: options.panel-boxes.share-experience-names-luminance-only-on-a-world-with-it
#[test]
fn the_share_experience_box_names_luminance_only_on_a_world_with_luminance() {
    let (mut ui, s, mut panels) = screen(true);
    let h = find(&ui, &s, FELLOWSHIP_BOXES[2].0);
    let (shipped, _, _) = placed(&mut ui, h);
    assert_eq!(shipped, "Share Fellowship Experience and Luminance");
    let world = |era| EraWorld {
        module: Module::new(DEFAULT_CHARACTER_OPTION),
        era: dereth_ui_screens::view::EraView {
            era,
            era_announced: true,
            ..Default::default()
        },
    };
    frame(
        &mut ui,
        &mut panels,
        &world(dereth_primitives::EraId::Infiltration),
    );
    let (text, glyphs, _) = placed(&mut ui, h);
    assert_eq!(text, "Share Fellowship Experience");
    assert_eq!(glyphs, text.chars().count(), "laid out again");
    // The other boxes keep their captions.
    let loot = find(&ui, &s, FELLOWSHIP_BOXES[3].0);
    let (loot, _, _) = placed(&mut ui, loot);
    assert!(!loot.is_empty());
    frame(&mut ui, &mut panels, &world(dereth_primitives::EraId::Eor));
    let (text, _, _) = placed(&mut ui, h);
    assert_eq!(text, shipped, "an end-of-retail world names luminance");
}

mod appear_offline {
    //! The Friends panel's Appear Offline box is an option checkbox bound over its player option,
    //! draws its layout caption (no string-table token), opens at the character's bit, a steady frame
    //! rewrites nothing, press/untick raise the request, other buttons raise none.
    //! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

    #![cfg(windows)]

    use crate::common::layout::RegistrationOrder;
    use crate::common::layout::Strings;
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::common::*;
    use dereth_ui::{Delivery, ElemHandle, ElementId, Screen, UiSystem};
    use dereth_ui_screens::options::toggle::Caption;
    use dereth_ui_screens::panels::remaining::RemainingPanels;
    use dereth_ui_screens::screens::gameplay::GamePlayScreen;
    use dereth_ui_screens::view::{GameView, PlayerOption, UiRequest};

    // -------------------------------------------------------------------------------------------
    // Literals, taken from retail and from the options document — not re-read through the
    // symbols under test
    // -------------------------------------------------------------------------------------------

    /// The element the panel's post-init binds.
    const APPEAR_OFFLINE: u32 = 0x1000_052C;
    /// The option check box's element type, which that binding casts to.
    const UIOPTION_CHECKBOX: u32 = 0x1000_0035;
    /// The player-option ordinal the binding carries.
    const APPEAR_OFFLINE_ORDINAL: u32 = 0x27;
    /// The button's mouse-up checks this toggle attribute before flipping the checked bit.
    const ATTR_TOGGLE_BUTTON: u32 = 0x0B;
    /// Character options word 2, bit 12 — `PLAYER_OPTIONS[39]`'s mask, spelled here rather than read
    /// back through `dereth_client_model`, which this crate cannot see.
    const MASK_APPEAR_OFFLINE: u32 = 0x0000_1000;
    /// The default second character-options word. `AppearOffline` is **clear** in it, so a
    /// shipped character opens this box unticked — correctly, and for a reason rather than by
    /// accident.
    const DEFAULT_CHARACTER_OPTIONS2: u32 = 0x0094_8700;
    /// The token `options::character::label_token(AppearOffline)` produces, which this box's post-init
    /// never hashes. Spelled as a literal so the assertion is not a restatement of that function.
    const LABEL_TOKEN: &str = "ID_PlayerOption_AppearOffline";

    // -------------------------------------------------------------------------------------------
    // Harness — the shipped `0x21000005` tree, with a real string resolver
    // -------------------------------------------------------------------------------------------

    fn env(with_strings: bool) -> UiSystem {
        let (mut ui, _flow, store) =
            crate::common::layout::load((800, 600), RegistrationOrder::AfterResolver);
        if with_strings {
            ui.strings = Some(Rc::new(Strings(Rc::clone(&store))));
        }
        ui
    }

    fn pump(ui: &mut UiSystem, s: &mut GamePlayScreen) {
        for _ in 0..16 {
            let batch = ui.drain_outbox();
            if batch.is_empty() {
                return;
            }
            for d in batch {
                if let Delivery::Element { msg, .. } = d {
                    s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
                }
            }
        }
    }

    /// A `PlayerModule` stand-in holding the **second** options word, read through the one mask this
    /// file needs.
    #[derive(Debug, Clone)]
    struct Module {
        options2: Rc<RefCell<u32>>,
    }

    impl Module {
        fn new(word: u32) -> Self {
            Self {
                options2: Rc::new(RefCell::new(word)),
            }
        }
    }

    impl GameView for Module {
        fn player_option(&self, o: PlayerOption) -> bool {
            match o {
                PlayerOption::AppearOffline => *self.options2.borrow() & MASK_APPEAR_OFFLINE != 0,
                _ => false,
            }
        }
    }

    /// The shipped gameplay screen with `RemainingPanels` bound off its root, exactly as
    /// `dereth_client_shell::hud::Hud::drive` binds it.
    fn screen(with_strings: bool) -> (UiSystem, GamePlayScreen, RemainingPanels) {
        let mut ui = env(with_strings);
        let mut s = GamePlayScreen::default();
        s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
            .expect("the gameplay screen builds from the shipped layout");
        pump(&mut ui, &mut s);
        let root = s.root().expect("the gameplay root");
        let mut panels = RemainingPanels::default();
        panels.post_init(&mut ui, root);
        ui.requests.clear();
        (ui, s, panels)
    }

    fn find(ui: &UiSystem, s: &GamePlayScreen, id: u32) -> ElemHandle {
        let root = s.root().expect("the gameplay root");
        ui.get_child_recursive(root, ElementId(id))
            .unwrap_or_else(|| panic!("{id:#010X} is not in the shipped tree"))
    }

    /// Show every ancestor of `h` — the panels start hidden and a hidden ancestor is unhittable.
    fn reveal(ui: &mut UiSystem, mut h: ElemHandle) {
        loop {
            ui.set_visible(h, true);
            match ui.parent(h) {
                Some(p) => h = p,
                None => break,
            }
        }
        ui.drain_outbox();
    }

    /// One frame of the host's drive.
    fn frame(ui: &mut UiSystem, panels: &mut RemainingPanels, view: &dyn GameView) {
        panels.update(ui, view);
    }

    // -------------------------------------------------------------------------------------------
    // 1. The calibration
    // -------------------------------------------------------------------------------------------

    /// `0x1000052C` exists in the shipped `0x21000005` tree, carries element type `0x10000035`
    /// and is a toggle button (`0x0B`), and `FriendsPanel` binds it.
    ///
    /// **This passes before the fix as well as after.** That is the point: it separates "the box is
    /// missing" from "the box is present and unwired", and this defect is the second.
    #[test]
    fn the_appear_offline_box_is_a_uioption_checkbox_in_the_shipped_tree() {
        let (ui, s, panels) = screen(true);
        let h = find(&ui, &s, APPEAR_OFFLINE);
        let n = ui.node(h).expect("live");
        assert_eq!(
            n.ty().0,
            UIOPTION_CHECKBOX,
            "the type-checked cast would have failed"
        );
        assert_eq!(
            n.merged_properties().get_bool(ATTR_TOGGLE_BUTTON),
            Some(true),
            "without 0x0B, would never flip 0x0E"
        );
        assert!(
            panels.friends.bound(),
            "the friends panel itself must be bound"
        );
        // And it is inside the Friends panel, not somewhere else that happens to share the id.
        let panel = panels.friends.panel.expect("the friends panel element");
        assert!(
            ui.get_child_recursive(panel, ElementId(APPEAR_OFFLINE))
                .is_some(),
            "{APPEAR_OFFLINE:#010X} is not under the friends panel"
        );
    }

    /// `FriendsPanel::post_init` reproduces the lookup → cast to `0x10000035` → set-player-option
    /// triple, and the cast is not a formality: the client binds nothing when
    /// it fails, and reproducing that is what stops a layout change from making this a plain button that
    /// toggles and sends.
    #[test]
    fn friends_post_init_binds_the_box_over_a_player_option() {
        let (ui, s, panels) = screen(true);
        let h = find(&ui, &s, APPEAR_OFFLINE);
        assert_eq!(
            panels.friends.appear_offline(),
            Some(h),
            "FriendsPanel's sixth descendant lookup found a different element"
        );
        assert_eq!(
            panels.friends.option_box.failures, 0,
            "the lookup or the type-checked cast refused it"
        );
        assert_eq!(
            panels.friends.option_box.boxes.first().map(|b| b.option),
            Some(PlayerOption::AppearOffline),
        );
    }

    /// The ordinal is the one post-init passes. Pinned as a literal against retail's value in
    /// this file's header, because a bridge checked only through the symbols it is built from is
    /// unfalsifiable — and a wrong ordinal here would edit a different character option.
    #[test]
    fn the_ordinal_is_the_one_postinit_pushes() {
        assert_eq!(APPEAR_OFFLINE_ORDINAL, 39, "0x27");
        assert_eq!(
            dereth_ui_screens::panels::friends::APPEAR_OFFLINE_OPTION_BOX,
            [(ElementId(APPEAR_OFFLINE), PlayerOption::AppearOffline)]
        );
        // This assertion covers the option name, not its runtime wire ordinal.
        assert_eq!(
            dereth_ui_screens::options::character::option_name(PlayerOption::AppearOffline),
            "AppearOffline"
        );
    }

    // -------------------------------------------------------------------------------------------
    // 2. The caption — a regression guard, and the measurement that justifies Caption::FromLayout
    // -------------------------------------------------------------------------------------------

    /// The box draws its caption, on the **glyphs the text element laid out**.
    ///
    /// **This passes before the fix and after, and it is here deliberately.** The obvious
    /// implementation — append a row to `options::toggle`'s table and let `post_init` run — would call
    /// `set_string_info` with a token that resolves to nothing and `set_tooltip(h, None)` with a help
    /// token that resolves to nothing, on an element whose caption and tooltip the **layout** already
    /// supplies. The label this box draws today is the thing such a change could take away, so it is
    /// asserted before and after rather than not at all.
    #[test]
    fn the_appear_offline_box_draws_its_layout_caption() {
        let (mut ui, s, _panels) = screen(true);
        let h = find(&ui, &s, APPEAR_OFFLINE);
        let (text, glyphs, advance) = placed(&mut ui, h);
        assert!(!text.is_empty(), "the box draws no caption at all");
        assert_eq!(
            glyphs,
            text.chars().count(),
            "the caption was stored but never laid out"
        );
        assert!(advance > 0, "the caption was never measured by a font");
    }

    /// **The premise behind [`Caption::FromLayout`], measured.** `ID_PlayerOption_AppearOffline` is not
    /// in the string table the five captioned boxes resolve out of — so the token
    /// `options::character::label_token` produces for this option has nothing behind it, and
    /// the friends panel's initialization not hashing it is consistent rather than an omission.
    ///
    /// If this ever starts failing, the option grew a caption string and
    /// [`Caption::FromToken`] becomes arguable for this box. It is not today.
    #[test]
    fn the_id_playeroption_token_for_this_box_is_not_in_the_string_table() {
        let ui = env(true);
        let table = dereth_ui_screens::options::character::table(&ui);
        // The instrument can look: a token that IS there resolves through the same call.
        assert!(
            ui.resolve_string(
                table,
                dereth_primitives::num::hash::str_hash(b"ID_PlayerOption_ShowCloak")
            )
            .is_some(),
            "the string table is not readable, so a None below would mean nothing"
        );
        assert_eq!(
            ui.resolve_string(
                table,
                dereth_primitives::num::hash::str_hash(LABEL_TOKEN.as_bytes())
            ),
            None,
            "{LABEL_TOKEN} is in the string table after all"
        );
        assert_eq!(
            ui.resolve_string(
                table,
                dereth_primitives::num::hash::str_hash(format!("{LABEL_TOKEN}_Help").as_bytes())
            ),
            None,
            "the _Help token is in the string table after all"
        );
    }

    /// The box records that its caption came from the layout.
    #[test]
    fn the_box_records_that_its_caption_came_from_the_layout() {
        let (mut ui, s, panels) = screen(true);
        let h = find(&ui, &s, APPEAR_OFFLINE);
        let drawn = placed(&mut ui, h).0;
        assert_eq!(
            panels.friends.option_box.boxes.len(),
            1,
            "the box never bound"
        );
        let b = &panels.friends.option_box.boxes[0];
        assert_eq!(b.caption, Caption::FromLayout);
        assert_eq!(b.label_token, None, " hashes no caption token");
        assert_eq!(b.help_token, None, "…and no tooltip token either");
        // The binder read the layout's caption back rather than writing one over it.
        assert_eq!(panels.friends.option_box.captions, 1);
        assert_eq!(
            b.label.as_deref(),
            Some(drawn.as_str()),
            "PanelOptionBox::label must be what is drawn, not what a token resolved to"
        );
        assert_eq!(
            panels.allegiance_options.boxes[0].caption,
            Caption::FromToken
        );
        assert_eq!(
            panels.allegiance_options.boxes[0].label_token.as_deref(),
            Some("ID_PlayerOption_IgnoreAllegianceRequests")
        );
    }

    // -------------------------------------------------------------------------------------------
    // 3. The value
    // -------------------------------------------------------------------------------------------

    /// The box opens at the characters own appear offline bit.
    #[test]
    fn the_box_opens_at_the_characters_own_appear_offline_bit() {
        // A shipped character: the default second options word leaves AppearOffline clear.
        assert_eq!(
            DEFAULT_CHARACTER_OPTIONS2 & MASK_APPEAR_OFFLINE,
            0,
            "the premise: clear in the default second options word"
        );
        let (mut ui, s, mut panels) = screen(true);
        let view = Module::new(DEFAULT_CHARACTER_OPTIONS2);
        frame(&mut ui, &mut panels, &view);
        assert_eq!(
            panels.friends.option_box.values_seen, 1,
            "no GameView answered for the box"
        );
        assert!(
            !checked(&ui, find(&ui, &s, APPEAR_OFFLINE)),
            "it must open unticked for this word"
        );

        // …and a character who has it set opens ticked, which is what makes the line above a read.
        let (mut ui, s, mut panels) = screen(true);
        let view = Module::new(DEFAULT_CHARACTER_OPTIONS2 | MASK_APPEAR_OFFLINE);
        frame(&mut ui, &mut panels, &view);
        assert!(
            checked(&ui, find(&ui, &s, APPEAR_OFFLINE)),
            "the box ignored the character's own word"
        );
    }

    /// `refresh` writes only when the bit moved, so a steady frame is a no-op — the same treatment
    /// every other panel gets, and the reason this poll can run before the friends-list guard.
    #[test]
    fn a_steady_frame_rewrites_nothing() {
        let (mut ui, _s, mut panels) = screen(true);
        let view = Module::new(DEFAULT_CHARACTER_OPTIONS2);
        frame(&mut ui, &mut panels, &view);
        let before = panels.friends.option_box.values_seen;
        assert_eq!(
            panels.friends.option_box.refresh(&mut ui, &view),
            0,
            "nothing moved"
        );
        assert_eq!(panels.friends.option_box.values_seen, before);
        // Move the character's bit and the next poll writes exactly one box.
        *view.options2.borrow_mut() |= MASK_APPEAR_OFFLINE;
        assert_eq!(panels.friends.option_box.refresh(&mut ui, &view), 1);
    }

    // -------------------------------------------------------------------------------------------
    // 4. The gesture
    // -------------------------------------------------------------------------------------------

    /// Behaviour: friends.appear-offline.the-box-opens-at-the-characters-bit-and-a-press-raises-the-option
    /// A real press on the appear offline box names the option.
    #[test]
    fn a_real_press_on_the_appear_offline_box_names_the_option() {
        let (mut ui, mut s, mut panels) = screen(true);
        let view = Module::new(DEFAULT_CHARACTER_OPTIONS2);
        frame(&mut ui, &mut panels, &view);

        let h = find(&ui, &s, APPEAR_OFFLINE);
        assert!(
            !checked(&ui, h),
            "it opens unticked because the character has it clear"
        );
        reveal(&mut ui, h);
        ui.requests.clear();

        let b = ui.screen_box(h);
        let (cx, cy) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
        assert_eq!(
            ui.hit_test_screen(cx, cy),
            Some(h),
            "the press lands on the check box"
        );
        ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, cx, cy);
        ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, cx, cy, false);
        // The button's mouse-up has already flipped 0x0E by the time message 1 goes out; the arm reads
        // it back rather than negating a copy.
        assert!(checked(&ui, h), "the press did not flip 0x0E");
        for _ in 0..8 {
            let batch = ui.drain_outbox();
            if batch.is_empty() {
                break;
            }
            for d in batch {
                if let Delivery::Element { msg, .. } = d {
                    s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(&mut ui), &msg);
                    panels.on_element_message(&mut ui, &msg, &view);
                }
            }
        }

        let reqs = ui.requests.take();
        assert!(
            reqs.contains(&UiRequest::SetPlayerOption(
                PlayerOption::AppearOffline,
                true
            )),
            " did not reach the module: {reqs:?}"
        );
        assert_eq!(panels.friends.appear_offline_presses, 1);
        // Exactly one option request, and it is this option — not a word composed from several boxes.
        let n = reqs
            .iter()
            .filter(|r| matches!(r, UiRequest::SetPlayerOption(..)))
            .count();
        assert_eq!(n, 1, "one press must name one option: {reqs:?}");
    }

    /// The other direction, which a hard-coded `true` could not fake: a character who already appears
    /// offline un-ticks it and the request carries `false`.
    #[test]
    fn un_ticking_the_box_raises_the_request_with_false() {
        let (mut ui, mut s, mut panels) = screen(true);
        let view = Module::new(DEFAULT_CHARACTER_OPTIONS2 | MASK_APPEAR_OFFLINE);
        frame(&mut ui, &mut panels, &view);

        let h = find(&ui, &s, APPEAR_OFFLINE);
        assert!(checked(&ui, h), "the premise: this character has it set");
        reveal(&mut ui, h);
        ui.requests.clear();

        let b = ui.screen_box(h);
        let (cx, cy) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
        assert_eq!(ui.hit_test_screen(cx, cy), Some(h));
        ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, cx, cy);
        ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, cx, cy, false);
        assert!(!checked(&ui, h), "the press did not clear 0x0E");
        for _ in 0..8 {
            let batch = ui.drain_outbox();
            if batch.is_empty() {
                break;
            }
            for d in batch {
                if let Delivery::Element { msg, .. } = d {
                    s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(&mut ui), &msg);
                    panels.on_element_message(&mut ui, &msg, &view);
                }
            }
        }
        let reqs = ui.requests.take();
        assert!(
            reqs.contains(&UiRequest::SetPlayerOption(
                PlayerOption::AppearOffline,
                false
            )),
            "the direction was not read off the element: {reqs:?}"
        );
    }

    /// A press on one of the Friends panel's **other** buttons must not raise an option request — the arm
    /// is keyed on `0x1000052C` and not on "any click this panel sees".
    #[test]
    fn a_press_on_another_friends_button_raises_no_option_request() {
        let (mut ui, mut s, mut panels) = screen(true);
        let view = Module::new(DEFAULT_CHARACTER_OPTIONS2);
        frame(&mut ui, &mut panels, &view);

        let h = find(&ui, &s, dereth_ui_screens::panels::friends::ADD_BUTTON.0);
        reveal(&mut ui, h);
        ui.requests.clear();
        let b = ui.screen_box(h);
        let (cx, cy) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
        assert_eq!(
            ui.hit_test_screen(cx, cy),
            Some(h),
            "the press lands on the Add button"
        );
        ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, cx, cy);
        ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, cx, cy, false);
        for _ in 0..8 {
            let batch = ui.drain_outbox();
            if batch.is_empty() {
                break;
            }
            for d in batch {
                if let Delivery::Element { msg, .. } = d {
                    s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(&mut ui), &msg);
                    panels.on_element_message(&mut ui, &msg, &view);
                }
            }
        }
        let reqs = ui.requests.take();
        assert!(
            !reqs
                .iter()
                .any(|r| matches!(r, UiRequest::SetPlayerOption(..))),
            "the Add button named a character option: {reqs:?}"
        );
        assert_eq!(panels.friends.appear_offline_presses, 0);
    }
}
