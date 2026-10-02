//! Every concrete screen, panel, window and game-specific widget of the retail UI.
//!
//! **Depends on** `dereth-primitives`, the decoded tables of `dereth-assets`, the shared rules
//! (`dereth-rules`), the game's presentation rules (`dereth-presentation`), character creation's
//! model (`dereth-chargen`), the UI engine (`dereth-ui`), the device input's action ids (`dereth-input`)
//! and the contract (`dereth-client-contract`), through which it reads the game. **Used by** the
//! client and its test kit.
//!
//! **Must never** name the object model or the runtime: it reads the game through the contract
//! only, so `dereth-client-model`, `dereth-client-runtime` and `dereth-client` appear in none of
//! its dependency tables and none of its code (`cargo xtask seams`, `seam: ui-screens deps`, `seam:
//! ui-screens code`). The client predicts nothing, so no panel may either: [`view::GameView`] is
//! read-only and a panel's only reply is a [`view::UiRequest`].
//!
//! If a player can point at it, it is here; if it is the machinery underneath, it is
//! [`dereth_ui`]'s. Nothing is hard-coded to a data id: a screen names a
//! [`LayoutEnum`](dereth_ui::framework::LayoutEnum) and the installed resolver answers it
//! ([`env`](mod@env)). A panel that animates registers for global message 3 and unregisters when idle, and
//! the association between a panel and its button is layout data, not code.
//!
//! Reading order: [`element_types`] → [`view`] → [`env`](mod@env) → [`bind`] → [`screens`] → [`hud`] →
//! [`toolbar`] → [`panels`] → [`items`] → [`chat`] → [`options`] → [`mapradar`].

#![forbid(unsafe_code)]

pub mod bind;
pub mod chat;
/// The CRT time formatters the retail client links against — `MSVCR70.DLL`'s `strftime("%c")`
/// under `setlocale(LC_ALL, "English")`, and its `asctime`. Every date this crate
/// draws goes through one of the two, and both take the zone shift as a parameter.
pub mod ctime;
pub mod element_types;
pub mod env;
pub mod hud;
pub mod items;
pub mod mapradar;
pub mod notices;
pub mod options;
pub mod panels;
pub mod requests;
pub mod screens;
pub mod toolbar;
pub mod view;

use dereth_ui::element::{Element, PlainElement};
use dereth_ui::{ElementDesc, ElementType, LayoutDesc, UiFlow, UiMode, UiSystem};

pub use element_types::{GameElementType, REGISTRATION_ORDER};
pub use hud::floaty::{ChromePiece, FloatyWindow, GameplayWindow, GAMEPLAY_WINDOWS};
pub use screens::{ScreenSpec, SCREENS, UNREGISTERED_MODES};
pub use view::{
    DropTarget, GameView, PlayerOption, PrefValue, RadarEntry, SelectionQueryFacts, UiRequest,
    Vital,
};

/// The eight mode ids.
///
/// These are the same constants [`dereth_ui::framework::mode`] carries, re-exported under the names
/// the screens use.
pub const MODE_INTRO: UiMode = dereth_ui::framework::mode::INTRO;
/// See [`MODE_INTRO`].
pub const MODE_DISCONNECTED: UiMode = dereth_ui::framework::mode::DISCONNECTED;
/// See [`MODE_INTRO`].
pub const MODE_DATAPATCH: UiMode = dereth_ui::framework::mode::DATA_PATCH;
/// See [`MODE_INTRO`].
pub const MODE_CREDITS: UiMode = dereth_ui::framework::mode::CREDITS;
/// See [`MODE_INTRO`].
pub const MODE_GAMEPLAY: UiMode = dereth_ui::framework::mode::GAME_PLAY;
/// See [`MODE_INTRO`].
pub const MODE_EPILOGUE: UiMode = dereth_ui::framework::mode::EPILOGUE;
/// See [`MODE_INTRO`].
pub const MODE_CHARMGMT: UiMode = dereth_ui::framework::mode::CHARACTER_MANAGEMENT;
/// See [`MODE_INTRO`].
pub const MODE_CHARGEN: UiMode = dereth_ui::framework::mode::CHAR_GEN;

/// The factory every game element type is registered with.
///
/// The behaviour objects in this crate hold no per-instance state that the factory could give them:
/// a constructor is `fn(&LayoutDesc, &ElementDesc) -> Box<dyn Element>`, and the *interesting* state
/// (a panel's bound children, a chat window's filter) is discovered in post-init, after the tree
/// exists. So the factory produces a plain element and the screen that owns the subtree does the
/// binding, which is also what makes every module here testable without an arena.
fn game_element(_l: &LayoutDesc, _d: &ElementDesc) -> Box<dyn Element> {
    Box::new(PlainElement)
}

/// Register all 84 game element types and all 8 screens.
///
/// The registration **order** is copied from the client's element-registration helper and from
/// its UI-flow constructor; it is global and permanent, because neither table is ever
/// cleared between UI modes.
///
/// UI-flow construction also queues mode `0x10000003` immediately, which this does at the end —
/// so a host that calls `register_all` and then runs a frame lands on the data-patch screen exactly
/// as the client does.
pub fn register_all(ui: &mut UiSystem, flow: &mut UiFlow) {
    for row in REGISTRATION_ORDER {
        ui.register_element_class(row.ty, game_element);
    }
    // Six of the 84 are not plain elements: each option element derives from a real engine
    // widget and inherits its whole behavior. A slider option is a scrollbar; as a
    // `PlainElement` it has no thumb, track or `0x0A`, and the Client Options page would have
    // nothing draggable on it.
    options::controls::register(ui);
    // Six more of the 84 are not plain elements either: each
    // indicator-strip lamp class derives from the button element, whose mouse-visibility
    // query always returns true. As a `PlainElement`,
    // a lamp answers **false**, element initialisation step 5 leaves
    // the mouse-visible bit clear, and the mouse-over hit tester's recursive walk skips
    // it — so the pointer would fall through the whole strip and no lamp could be clicked, while
    // the plain log-out button beside them works. See [`element_types::LAMP_BUTTON_CLASSES`] for
    // the byte-level reading.
    for t in element_types::LAMP_BUTTON_CLASSES {
        ui.register_element_class(t, dereth_ui::widgets::button::create);
    }
    // The item-list element inherits list-box and scrollable mouse, scroll and layout
    // behavior. Its game-object projection remains in `ItemListWidget`; `PlainElement` loses the
    // base's scrollbar listener entirely.
    ui.register_element_class(
        ElementType(0x1000_0031),
        dereth_ui::widgets::listbox::create,
    );
    ui.register_element_class(ElementType(0x1000_0032), items::runtime::create);
    // The housing-vendor panel derives from the panel element; its shipped `0x2E` table
    // owns the mutually-exclusive Buy/Rent pages and the two clickable tab labels. Leaving this
    // class on `game_element` makes both authored pages visible at once and the Rent page covers
    // the Buy button. The confirmation path is not physically usable until that base behaviour
    // runs. This is deliberately a class-specific override, not a policy for all game elements.
    ui.register_element_class(
        element_types::ty::SLUMLORD,
        dereth_ui::widgets::panel::create,
    );
    // The floaty toolbar's move constrains position even for non-mouse
    // callers (saved layout, PlayerModule restore, parent-size changes). Not a generic clamp.
    ui.register_element_class(
        element_types::ty::FLOATY_TOOLBAR,
        hud::floaty::create_toolbar,
    );
    // The client's UI-preference initialiser — the 34 attach calls that give every option
    // control its label token, its help token, its range and its enum choices, over the value
    // store the eight subsystem preference registrations fill. Without it the preference query
    // cannot answer and every row on every option page is drawn with no text on it. The client
    // runs it from its own start-up, just before the UI half of initialisation; this is that
    // call site.
    options::preferences::init_ui_preferences();
    // The client's UI-flow constructor registers the eight screens in this order.
    flow.register(
        MODE_DATAPATCH,
        screens::datapatch::DataPatchScreen::create_screen,
    );
    flow.register(MODE_INTRO, screens::intro::IntroScreen::create_screen);
    flow.register(
        MODE_CHARMGMT,
        screens::charmgmt::CharacterManagementScreen::create_screen,
    );
    flow.register(
        MODE_GAMEPLAY,
        screens::gameplay::GamePlayScreen::create_screen,
    );
    flow.register(
        MODE_EPILOGUE,
        screens::epilogue::EpilogueScreen::create_screen,
    );
    flow.register(
        MODE_DISCONNECTED,
        screens::disconnected::DisconnectedScreen::create_screen,
    );
    flow.register(MODE_CHARGEN, screens::chargen::CharGenScreen::create_screen);
    flow.register(MODE_CREDITS, screens::credits::CreditsScreen::create_screen);
    // "…and immediately queues mode `0x10000003`".
    flow.queue(MODE_DATAPATCH);
}

/// Whether an element type is one this crate registers.
#[must_use]
pub fn is_game_element_type(ty: ElementType) -> bool {
    REGISTRATION_ORDER.iter().any(|r| r.ty == ty)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Registration installs all 84 game element types and all eight screens.
    #[test]
    fn register_all_registers_every_game_type_and_every_screen() {
        let mut ui = UiSystem::new((800, 600));
        let mut flow = UiFlow::new();
        let engine_types = ui.registered_types().len();

        register_all(&mut ui, &mut flow);

        // Every game type is now registered, and the engine's own are untouched.
        for row in REGISTRATION_ORDER {
            assert!(ui.is_registered(row.ty), "{} ({:?})", row.class, row.ty);
        }
        assert_eq!(ui.registered_types().len(), engine_types + 84);
        // The combat panel is deliberately absent.
        assert!(!ui.is_registered(element_types::COMBAT_PANEL_UNREGISTERED));

        // The eight modes are registered and the ninth registration of any of them is rejected,
        // as add rejects a duplicate.
        for s in SCREENS {
            assert!(
                !flow.register(s.mode, screens::epilogue::EpilogueScreen::create_screen),
                "{} registered twice",
                s.class
            );
        }
        // …and the three unregistered ids are still unregistered.
        for m in UNREGISTERED_MODES {
            assert!(
                flow.register(m, screens::epilogue::EpilogueScreen::create_screen),
                "{m:?} should have been free"
            );
        }
    }

    /// Oracle: the recovered screen catalogue — "registers the eight screens … and **immediately
    /// queues mode `0x10000003`**".
    #[test]
    fn the_flow_comes_out_of_registration_with_the_data_patch_screen_queued() {
        let mut ui = UiSystem::new((800, 600));
        let mut flow = UiFlow::new();
        register_all(&mut ui, &mut flow);
        assert_eq!(flow.queued_mode(), Some(MODE_DATAPATCH));
        assert_eq!(
            flow.current_mode(),
            None,
            "the switch happens on the first message-3 tick"
        );
    }

    /// Oracle: trap 16 / `10` §2 — "Modes `0x10000004`, `0x10000006` and `0x10000007` are not
    /// registered. Queueing one leaves the current screen null."
    #[test]
    fn queueing_an_unregistered_mode_leaves_the_screen_null() {
        let mut ui = UiSystem::new((800, 600));
        let mut flow = UiFlow::new();
        register_all(&mut ui, &mut flow);
        for m in UNREGISTERED_MODES {
            let mut flow = UiFlow::new();
            register_all(&mut ui, &mut flow);
            flow.queue(m);
            flow.use_new_mode(&mut dereth_ui::framework::ScreenCx::new(&mut ui));
            assert!(flow.current().is_none(), "{m:?} must not produce a screen");
            assert_eq!(
                flow.current_mode(),
                None,
                "and must not become the current mode"
            );
        }
    }

    /// Oracle: the track spec's public-API block, which names these eight constants.
    #[test]
    fn the_exported_mode_constants_are_the_documented_ids() {
        assert_eq!(MODE_INTRO, UiMode(0x1000_0001));
        assert_eq!(MODE_DISCONNECTED, UiMode(0x1000_0002));
        assert_eq!(MODE_DATAPATCH, UiMode(0x1000_0003));
        assert_eq!(MODE_CREDITS, UiMode(0x1000_0005));
        assert_eq!(MODE_GAMEPLAY, UiMode(0x1000_0008));
        assert_eq!(MODE_EPILOGUE, UiMode(0x1000_0009));
        assert_eq!(MODE_CHARMGMT, UiMode(0x1000_000A));
        assert_eq!(MODE_CHARGEN, UiMode(0x1000_000B));
        assert_eq!(
            UNREGISTERED_MODES.map(|m| m.0),
            [0x1000_0004, 0x1000_0006, 0x1000_0007]
        );
    }
}
