//! The game element type ids and the exact order the client registers them in.
//!
//! The id-to-class table and the registration order are different things; the registration order
//! is what this module encodes.
//!
//! Registration is **global and permanent**:
//! each registration inserts into the class-factory table and the table is never cleared between UI modes, so
//! order only matters for reproducing the client's behaviour on a duplicate id — of which there
//! are none.
//!
//! ## The count is 84, not 88
//!
//! The client registers **84** game types: every id in `0x10000001..=0x10000056` except
//! `0x1000000B` (which is not an element type at all — it is a layout enum) and `0x10000055`
//! (`CombatPanelStack`, which reports element type `0x10000055` as its element type but is **not**
//! registered as a factory). 84 is what this module encodes and what [`REGISTRATION_ORDER`] is
//! asserted against.

use dereth_ui::ElementType;

/// One row of the game element registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameElementType {
    pub ty: ElementType,
    /// A local behavior label used in diagnostics; registration itself is keyed by [`Self::ty`].
    pub class: &'static str,
}

const fn t(id: u32, class: &'static str) -> GameElementType {
    GameElementType {
        ty: ElementType(id),
        class,
    }
}

/// The 84 game element types **in order**.
///
/// This is the client's registration order. It is not id order: the six indicator widgets and the
/// three shared item widgets come first, then the HUD windows, then the toolbar panels, then the
/// seven option controls, then the six char-gen pages.
pub const REGISTRATION_ORDER: [GameElementType; 84] = [
    // --- custom-drawn widgets -----------------------------------------------------------------
    t(0x1000_0001, "BurdenIndicator"),
    t(0x1000_0002, "EffectsIndicator"),
    t(0x1000_0003, "LinkStatusIndicator"),
    t(0x1000_0004, "MiniGameIndicator"),
    t(0x1000_0005, "PortalStormIndicator"),
    t(0x1000_0006, "VitaeIndicator"),
    t(0x1000_0030, "WorldViewWrapper"),
    t(0x1000_0031, "ItemListWidget"),
    t(0x1000_0032, "UiItemWidget"),
    // --- HUD windows and the floaty wrappers --------------------------------------------------
    t(0x1000_004A, "BarberPanel"),
    t(0x1000_000E, "KeyboardPanel"),
    t(0x1000_000F, "PowerBar"),
    t(0x1000_0010, "Radar"),
    t(0x1000_0014, "WorldView"),
    t(0x1000_0016, "MessageLogPanel"),
    t(0x1000_0054, "FloatingCombatStack"),
    t(0x1000_004E, "FloatingEnvironmentStack"),
    t(0x1000_004C, "FloatingExamination"),
    t(0x1000_0052, "FloatingIndicators"),
    t(0x1000_0050, "FloatingMainChat"),
    t(0x1000_004F, "FloatingPanelStack"),
    t(0x1000_0053, "FloatingPowerBar"),
    t(0x1000_0056, "FloatingSideVitals"),
    t(0x1000_0051, "FloatingToolbar"),
    t(0x1000_004D, "FloatingVitals"),
    t(0x1000_0040, "FloatingChat"),
    t(0x1000_0041, "MainChat"),
    t(0x1000_003F, "AdminPropertiesPanel"),
    t(0x1000_0033, "EnvironmentPanelStack"),
    t(0x1000_000C, "CombatWindow"),
    t(0x1000_000D, "ExternalContainerPanel"),
    t(0x1000_0011, "SalvagePanel"),
    t(0x1000_0012, "TradePanel"),
    t(0x1000_0013, "HousingPanel"),
    t(0x1000_0015, "SpellcastingPanel"),
    t(0x1000_0017, "VendorPanel"),
    t(0x1000_0009, "VitalsPanel"),
    t(0x1000_000A, "IndicatorStrip"),
    t(0x1000_0007, "Toolbar"),
    t(0x1000_0008, "PanelStack"),
    // --- toolbar panels -----------------------------------------------------------------------
    t(0x1000_0018, "AbusePanel"),
    t(0x1000_0019, "BookPanel"),
    t(0x1000_001A, "CharacterInfoPanel"),
    t(0x1000_001B, "EffectsPanel"),
    t(0x1000_001C, "ExaminationPanel"),
    t(0x1000_001D, "LinkStatusPanel"),
    t(0x1000_001E, "MiniGamePanel"),
    t(0x1000_001F, "UrgentAssistancePanel"),
    t(0x1000_0020, "VitaePanel"),
    t(0x1000_0021, "ItemsPanel"),
    t(0x1000_0022, "BackpackPanel"),
    t(0x1000_0023, "InventoryPanelStack"),
    t(0x1000_0024, "EquipmentPanel"),
    t(0x1000_0025, "HousePanel"),
    t(0x1000_0026, "MapPanel"),
    t(0x1000_0027, "CharacterSettingsPanel"),
    t(0x1000_0042, "ChatOptionsPanel"),
    t(0x1000_0028, "ClientOptionsPanel"),
    t(0x1000_0029, "GameplayOptionsPanel"),
    t(0x1000_002A, "AttributesPanel"),
    t(0x1000_002B, "SkillsPanel"),
    t(0x1000_0046, "TitlesPanel"),
    t(0x1000_004B, "ContractsPanel"),
    t(0x1000_0048, "JournalPanel"),
    t(0x1000_0049, "PageListPanel"),
    t(0x1000_002C, "AllegiancePanel"),
    t(0x1000_002D, "FellowshipPanel"),
    t(0x1000_0045, "FriendsPanel"),
    t(0x1000_0047, "SquelchPanel"),
    t(0x1000_002E, "SpellbookPanel"),
    t(0x1000_002F, "SpellComponentPanel"),
    // --- the seven option controls ------------------------------------------------------------
    t(0x1000_0034, "ActionKeyMapOption"),
    t(0x1000_0035, "CheckboxOption"),
    t(0x1000_0043, "BitfieldCheckboxOption"),
    t(0x1000_0044, "WideBitfieldCheckboxOption"),
    t(0x1000_0037, "SliderOption"),
    t(0x1000_0036, "CheckboxSliderOption"),
    t(0x1000_0038, "MenuOption"),
    // --- the six char-gen pages ---------------------------------------------------------------
    t(0x1000_0039, "HeritagePage"),
    t(0x1000_003A, "ProfessionPage"),
    t(0x1000_003B, "SkillsPage"),
    t(0x1000_003C, "AppearancePage"),
    t(0x1000_003D, "TownPage"),
    t(0x1000_003E, "SummaryPage"),
];

/// `CombatPanelStack` reports this as its element type but is **never** registered, because
/// **`FloatingCombatStack` (`0x10000054`) derives from it** and that subclass is what the layout
/// instantiates, with `CombatPanelStack` as its base. So `<COMB>` (`0x100006B5`) *is* a live `CombatPanelStack` and no element in
/// `classic_gameplay` carries this type. \[verified\]
///
/// It is **not** a described child of `CombatWindow`: it is the other way round —
/// `CombatWindow` (`0x1000005C`) is a *child* of the `CombatPanelStack` instance — and a
/// recursive child search for `0x10000055` fetches an **element id**
/// that collides with this **element type**: it is the *ViewCombatTarget* option checkbox
/// (bound to the `ViewCombatTarget` player option).
pub const COMBAT_PANEL_UNREGISTERED: ElementType = ElementType(0x1000_0055);

/// The six lamp classes of the indicator strip, which **derive from `Button`** and must
/// be registered with its factory rather than with [`crate::register_all`]'s plain
/// `game_element`. In retail each lamp class carries its own copy of the button element's
/// element-message handler, because these classes **derive from `Button`**; so the six lamp
/// element classes get the button arm here, in this crate's `register_all` rather than in
/// `dereth-ui`. See [`crate::screens::gameplay::GamePlayScreen::handle_button_click`].
///
/// # Why a plain-element lamp is unclickable: the hit test, not the handler
///
/// The element base's initialisation step 5 sets the element's mouse-visible flag when either an
/// explicit "should be mouse visible" flag or the element's own visibility rule says so, and the
/// mouse-over hit test returns an element only when it is mouse-visible or blocks clicks. A lamp built as a `PlainElement` answers **false** to all
/// three of the routes into that flag:
///
/// 1. **the element's own visibility rule** — the element base's rule is "has a context menu or a
///    valid tooltip text", and no lamp in the shipped `classic_gameplay` layout
///    carries a tooltip or a context menu;
/// 2. **an explicit mouse-visible setter** — nothing calls one on a lamp;
/// 3. **step 4**, which sets the flag for an
///    element id someone has registered one of the four mouse messages for. Nothing registers one
///    for `0x100000F3..0x100000F8`.
///
/// So the pointer would fall straight through the strip to `<INDI>` and then to the root, `0x19`
/// would be raised on the wrong element, and element message `1` would never reach a lamp. The
/// log-out button `0x100000FA` beside them is a plain `ty::BUTTON` (type `1`), gets
/// [`dereth_ui::widgets::button::create`] from `register_engine_classes`, and so works either
/// way; that asymmetry is the tell.
///
/// # Verified against retail
///
/// For `Button`, `VitaeIndicator`, `PortalStormIndicator`, `MiniGameIndicator`,
/// `LinkStatusIndicator`, `EffectsIndicator`, and `BurdenIndicator` alike, the element's own
/// mouse-visibility rule simply answers `true`. All six lamps
/// share the button element's mouse-over, mouse-down and mouse-up handlers, which is the
/// derivation itself, and their element-message handler is the button element's own — so the two
/// really are one body.
///
/// The four mouse messages of route 3 are **`0x19, 0x1C, 0x1D, 0x40` in that order**, which is exactly [`dereth_ui::msg::element::id::AUTO_MOUSE_VISIBLE`]'s
/// `[MOUSE_CLICK, MOUSE_PRESS, MOUSE_RELEASE, MOUSE_TAP]`. Confirmed.
///
/// # It is six, not five — and the two sixes are different sixes
///
/// There are **six lamp classes** (`0x10000001..=0x10000006`), and all six are registered here.
/// The strip `<INDI>` `0x10000611` holds seven children of which **six are lamp elements**, but
/// those six cover only *five* of the classes — `EffectsIndicator` appears twice, once
/// with attribute `0x1000000C = 1` (helpful) and once with `= 2` (harmful) — and the seventh child
/// is the plain log-out button:
///
/// | x | element | type | class | attribute `0x12` |
/// |---|---|---|---|---|
/// | 5..24 | `0x100000F8` | `0x10000003` | `LinkStatusIndicator` | `0x10000009` |
/// | 25..44 | `0x100000F5` | `0x10000002` | `EffectsIndicator` (helpful) | `0x10000006` |
/// | 45..64 | `0x100000F6` | `0x10000002` | `EffectsIndicator` (harmful) | `0x10000007` |
/// | 65..84 | `0x100000F4` | `0x10000006` | `VitaeIndicator` | `0x1000000C` |
/// | 85..104 | `0x100000F7` | `0x10000001` | `BurdenIndicator` | `0x10000005` |
/// | 105..124 | `0x100000F3` | `0x10000004` | `MiniGameIndicator` | `0x1000000A` |
/// | 125..144 | `0x100000FA` | `0x00000001` | plain `Button` | *none* |
///
/// The mini-game lamp `0x100000F3` would be dead in exactly the same way as the other five and is
/// fixed by the same registration. `PortalStormIndicator` (`0x10000005`) is the sixth
/// class and has **no instance anywhere in `classic_gameplay`** — measured over all 1,870 elements
/// of the tree — so it is registered for correctness and has nothing to click today.
pub const LAMP_BUTTON_CLASSES: [ElementType; 6] = [
    ty::BURDEN_INDICATOR,
    ty::EFFECTS_INDICATOR,
    ty::LINK_STATUS_INDICATOR,
    ty::MINI_GAME_INDICATOR,
    ty::PORTAL_STORM_INDICATOR,
    ty::VITAE_INDICATOR,
];

/// The named ids, so screens and panels never spell an id twice.
///
/// The names are the element types' own names.
#[allow(missing_docs)]
pub mod ty {
    use dereth_ui::ElementType;
    macro_rules! ids {
        ($($name:ident = $v:expr;)*) => { $(pub const $name: ElementType = ElementType($v);)* };
    }
    ids! {
        BURDEN_INDICATOR       = 0x1000_0001;
        EFFECTS_INDICATOR      = 0x1000_0002;
        LINK_STATUS_INDICATOR  = 0x1000_0003;
        MINI_GAME_INDICATOR    = 0x1000_0004;
        PORTAL_STORM_INDICATOR = 0x1000_0005;
        VITAE_INDICATOR        = 0x1000_0006;
        TOOLBAR                = 0x1000_0007;
        PANEL                  = 0x1000_0008;
        VITALS                 = 0x1000_0009;
        INDICATORS             = 0x1000_000A;
        COMBAT                 = 0x1000_000C;
        EXTERNAL_CONTAINER     = 0x1000_000D;
        KEYBOARD               = 0x1000_000E;
        POWERBAR               = 0x1000_000F;
        RADAR                  = 0x1000_0010;
        SALVAGE                = 0x1000_0011;
        SECURE_TRADE           = 0x1000_0012;
        SLUMLORD               = 0x1000_0013;
        SMART_BOX              = 0x1000_0014;
        SPELLCASTING           = 0x1000_0015;
        SPEW_BOX               = 0x1000_0016;
        VENDOR                 = 0x1000_0017;
        ABUSE                  = 0x1000_0018;
        BOOK                   = 0x1000_0019;
        CHARACTER_INFO         = 0x1000_001A;
        EFFECTS                = 0x1000_001B;
        EXAMINATION            = 0x1000_001C;
        LINK_STATUS            = 0x1000_001D;
        MINI_GAME              = 0x1000_001E;
        URGENT_ASSISTANCE      = 0x1000_001F;
        VITAE                  = 0x1000_0020;
        ITEMS_3D               = 0x1000_0021;
        BACKPACK               = 0x1000_0022;
        INVENTORY              = 0x1000_0023;
        PAPER_DOLL             = 0x1000_0024;
        HOUSE                  = 0x1000_0025;
        MAP                    = 0x1000_0026;
        CHARACTER_SETTINGS     = 0x1000_0027;
        CONFIG                 = 0x1000_0028;
        GAMEPLAY_OPTIONS       = 0x1000_0029;
        ATTRIBUTE              = 0x1000_002A;
        SKILL                  = 0x1000_002B;
        ALLEGIANCE             = 0x1000_002C;
        FELLOWSHIP             = 0x1000_002D;
        SPELLBOOK              = 0x1000_002E;
        SPELL_COMPONENT        = 0x1000_002F;
        SMART_BOX_WRAPPER      = 0x1000_0030;
        ITEM_LIST              = 0x1000_0031;
        UI_ITEM                = 0x1000_0032;
        ENV_PANEL              = 0x1000_0033;
        OPTION_ACTION_KEY_MAP  = 0x1000_0034;
        OPTION_CHECKBOX        = 0x1000_0035;
        OPTION_CHECKBOX_SLIDER = 0x1000_0036;
        OPTION_SLIDER          = 0x1000_0037;
        OPTION_MENU            = 0x1000_0038;
        CG_HERITAGE_PAGE       = 0x1000_0039;
        CG_PROFESSION_PAGE     = 0x1000_003A;
        CG_SKILLS_PAGE         = 0x1000_003B;
        CG_APPEARANCE_PAGE     = 0x1000_003C;
        CG_TOWN_PAGE           = 0x1000_003D;
        CG_SUMMARY_PAGE        = 0x1000_003E;
        ADMIN_QUALITIES        = 0x1000_003F;
        FLOATY_CHAT            = 0x1000_0040;
        MAIN_CHAT              = 0x1000_0041;
        CHAT_OPTIONS           = 0x1000_0042;
        OPTION_CB_BITFIELD     = 0x1000_0043;
        OPTION_CB_BITFIELD64   = 0x1000_0044;
        FRIENDS                = 0x1000_0045;
        CHARACTER_TITLE        = 0x1000_0046;
        SQUELCH                = 0x1000_0047;
        JOURNAL                = 0x1000_0048;
        PAGE_LIST              = 0x1000_0049;
        BARBER                 = 0x1000_004A;
        CONTRACTS              = 0x1000_004B;
        FLOATY_EXAMINATION     = 0x1000_004C;
        FLOATY_VITALS          = 0x1000_004D;
        FLOATY_ENV_PANEL       = 0x1000_004E;
        FLOATY_PANEL           = 0x1000_004F;
        FLOATY_MAIN_CHAT       = 0x1000_0050;
        FLOATY_TOOLBAR         = 0x1000_0051;
        FLOATY_INDICATORS      = 0x1000_0052;
        FLOATY_POWER_BAR       = 0x1000_0053;
        FLOATY_COMBAT_PANEL    = 0x1000_0054;
        FLOATY_SIDE_VITALS     = 0x1000_0056;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Oracle: the recovered UI element model's two id tables and `02`'s registration list, which
    /// are two independent transcriptions of the client's element registration.
    ///
    /// The invariant that ties them together: the registered set is exactly
    /// `0x10000001..=0x10000056` minus `0x1000000B` and minus `0x10000055`.
    #[test]
    fn the_registered_set_is_the_contiguous_block_minus_the_two_documented_holes() {
        let got: BTreeSet<u32> = REGISTRATION_ORDER.iter().map(|r| r.ty.0).collect();
        assert_eq!(
            got.len(),
            REGISTRATION_ORDER.len(),
            "no duplicate id may be registered"
        );
        let want: BTreeSet<u32> = (0x1000_0001..=0x1000_0056)
            .filter(|i| *i != 0x1000_000B && *i != COMBAT_PANEL_UNREGISTERED.0)
            .collect();
        assert_eq!(got, want);
        assert_eq!(
            got.len(),
            84,
            "the enumeration in 02 §4 is 84 names, not the 88 the prose says"
        );
    }

    /// Oracle: the recovered element-description behavior, the literal call order.
    #[test]
    fn the_registration_order_starts_and_ends_where_the_document_says() {
        assert_eq!(REGISTRATION_ORDER[0].class, "BurdenIndicator");
        assert_eq!(REGISTRATION_ORDER[8].class, "UiItemWidget");
        assert_eq!(REGISTRATION_ORDER[9].class, "BarberPanel");
        assert_eq!(REGISTRATION_ORDER[83].class, "SummaryPage");
        // The seven option controls are contiguous and in the documented (non-id) order.
        let names: Vec<&str> = REGISTRATION_ORDER[71..78].iter().map(|r| r.class).collect();
        assert_eq!(
            names,
            vec![
                "ActionKeyMapOption",
                "CheckboxOption",
                "BitfieldCheckboxOption",
                "WideBitfieldCheckboxOption",
                "SliderOption",
                "CheckboxSliderOption",
                "MenuOption",
            ]
        );
    }

    /// Oracle: `01` §5.2's closing note — `CombatPanelStack` is found by a recursive child search,
    /// not through the factory table.
    #[test]
    fn the_combat_panel_type_is_never_registered() {
        assert!(!REGISTRATION_ORDER
            .iter()
            .any(|r| r.ty == COMBAT_PANEL_UNREGISTERED));
    }

    /// Oracle: each lamp's mouse-visibility rule in retail — see [`LAMP_BUTTON_CLASSES`]'s table.
    /// All six answer `true`, as `Button`'s does, and all six share its mouse handlers.
    ///
    /// The six are exactly the first six ids, which is also the head of the registration order —
    /// The client registers the lamps first.
    #[test]
    fn the_six_lamp_classes_are_the_first_six_registered_types() {
        let want: Vec<u32> = (0x1000_0001..=0x1000_0006).collect();
        let got: Vec<u32> = LAMP_BUTTON_CLASSES.iter().map(|t| t.0).collect();
        assert_eq!(
            got, want,
            "the six lamp classes are 0x10000001..=0x10000006"
        );
        let head: Vec<u32> = REGISTRATION_ORDER[..6].iter().map(|r| r.ty.0).collect();
        assert_eq!(
            head, want,
            "and they are the first six rows of the registration order"
        );
        for t in LAMP_BUTTON_CLASSES {
            assert!(
                REGISTRATION_ORDER.iter().any(|r| r.ty == t),
                "{t:?} is a registered type"
            );
        }
    }
}
