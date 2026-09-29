//! Spell-bar underlay/overlay/ring/caption children are bound where post-init looks; selecting a
//! spell names it; a wand captions itself and its spell (or no caption if unknown); an endowed wand
//! lights icon with spell underlay; ring follows endowment; paging re-reads ring and caption.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

#![allow(clippy::pedantic)]

use crate::common::layout::RegistrationOrder;
use dereth_primitives::{DataId, ObjectId};
use dereth_ui::{ElemHandle, ElementId, Screen, UiSystem};
use dereth_ui_screens::panels::remaining::RemainingPanels;
use dereth_ui_screens::panels::spellcasting::{
    ENDOWMENT_ICON, ENDOWMENT_OVERLAY, ENDOWMENT_SELECTED, ENDOWMENT_TOOLTIP_TAIL,
    ENDOWMENT_UNDERLAY, SPELL_NAME,
};
use dereth_ui_screens::view::{GameView, SlotDecoration, SpellEntry};

/// The wand, and the spell on it. **`WAND_SPELL` is deliberately outside the book**: retail looks
/// up the endowment spell id's spell base, which is a `SpellTable` lookup, and a wand's spell is
/// ordinarily not among the player's transcribed spells. A station that fed it through
/// `spellbook()` would pass with a caption built from the wrong table.
const WAND: ObjectId = ObjectId(0x5000_0147);
const WAND_SPELL: u32 = 900;
const WAND_NAME: &str = "Rod of Frost";
const WAND_SPELL_NAME: &str = "Frost Bolt VI";
/// The wand's object icon id, used to build its drag icon.
const WAND_ICON: u32 = 0x0600_2A11;
/// The book spells' own icon id.
const BOOK_ICON: DataId = DataId(0x0600_13A5);

#[derive(Debug)]
struct View {
    spells: Vec<SpellEntry>,
    tabs: [Vec<u32>; 8],
    endowment: Option<(ObjectId, u32)>,
    /// Every row the spell-table lookup knows: the book **plus** the wand's spell.
    table: Vec<SpellEntry>,
}
impl GameView for View {
    fn spellbook(&self) -> &[SpellEntry] {
        &self.spells
    }
    fn spell_tab(&self, tab: usize) -> &[u32] {
        &self.tabs[tab]
    }
    fn endowment(&self) -> Option<(ObjectId, u32)> {
        self.endowment
    }
    /// The client's spell table, which is a superset of the book.
    fn spell(&self, spell_id: u32) -> Option<SpellEntry> {
        self.table.iter().find(|s| s.id == spell_id).cloned()
    }
    fn name(&self, id: ObjectId) -> Option<&str> {
        (id == WAND).then_some(WAND_NAME)
    }
    fn slot_decoration(&self, id: ObjectId) -> Option<SlotDecoration> {
        (id == WAND).then(|| SlotDecoration {
            icon_id: WAND_ICON,
            obj_type: 0x0000_8000, // TYPE_CASTER
            ..SlotDecoration::default()
        })
    }
}

const LISTENER: u32 = 0xF147;

fn env() -> (UiSystem, RemainingPanels, View, ElemHandle) {
    let (mut ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    let mut screen = dereth_ui_screens::screens::gameplay::GamePlayScreen::default();
    screen
        .create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the shipped gameplay screen");
    let root = screen.root().unwrap();
    let mut panels = RemainingPanels::default();
    panels.post_init(&mut ui, root);
    let book: Vec<SpellEntry> = (1..=40)
        .map(|id| SpellEntry {
            id,
            name: format!("Spell {id}"),
            icon: Some(BOOK_ICON),
            school: 4,
            level: 1,
            display_order: i32::try_from(id).expect("a small id"),
            bitfield: 0,
        })
        .collect();
    let mut table = book.clone();
    table.push(SpellEntry {
        id: WAND_SPELL,
        name: WAND_SPELL_NAME.to_owned(),
        icon: Some(DataId(0x0600_13B0)),
        school: 1,
        level: 6,
        display_order: 0,
        bitfield: 0,
    });
    let view = View {
        spells: book,
        tabs: std::array::from_fn(|tab| (1..=8).map(|i| i + tab as u32 * 8).collect()),
        endowment: None,
        table,
    };
    // The settling frame before the clear -- the allegiance panel's one-time talk-focus setup
    // triple, exactly as `spell_bar_population::env` and `spell_bar_transfer::env` do.
    panels.update(&mut ui, &view);
    panels.spellcasting.update_endowment(&mut ui, &view);
    ui.requests.clear();
    ui.notice_inbox.clear();
    ui.register_for_element_messages(root, dereth_ui::ListenerId::External(LISTENER));
    ui.drain_outbox();
    (ui, panels, view, root)
}

fn frame(ui: &mut UiSystem, panels: &mut RemainingPanels, view: &View) {
    panels.update(ui, view);
    panels.spellcasting.update_endowment(ui, view);
    ui.requests.clear();
}

fn deliver(ui: &mut UiSystem, panels: &mut RemainingPanels, view: &View) {
    for d in ui.drain_outbox() {
        if let dereth_ui::Delivery::Element {
            to: dereth_ui::ListenerId::External(LISTENER),
            msg,
        } = d
        {
            panels.on_element_message(ui, &msg, view);
        }
    }
}

fn reveal(ui: &mut UiSystem, mut h: ElemHandle) {
    loop {
        ui.set_visible(h, true);
        let Some(parent) = ui.parent(h) else { break };
        h = parent;
    }
}

/// The caption, read off the **live** text element and not off the panel's mirror.
fn caption(ui: &mut UiSystem, panels: &RemainingPanels) -> String {
    let h = panels
        .spellcasting
        .spell_name
        .expect("the spell name binds");
    ui.text_element_mut(h)
        .map(|t| t.glyphs.inq_text(false))
        .unwrap_or_default()
}

/// The `DataId` the element's picture resolves to, or `None` for a cleared image.
fn picture(ui: &UiSystem, h: ElemHandle) -> Option<DataId> {
    ui.node(h)?.region.image.as_ref().map(|g| g.did)
}

fn blit(ui: &UiSystem, h: ElemHandle) -> Option<dereth_ui::region::BlitMode> {
    Some(ui.node(h)?.region.blit_mode)
}

/// Select the row showing `spell` on the open tab with a real primary press, through the
/// production `0x1C` dispatch.
fn click_row(ui: &mut UiSystem, panels: &mut RemainingPanels, view: &View, spell: u32) {
    let tab = panels.spellcasting.open_sub_menu_index(ui);
    let w = panels.spellcasting.lists[tab]
        .as_ref()
        .expect("the spell item list binds");
    let i = w
        .slots
        .iter()
        .position(|s| s.spell == Some(spell))
        .expect("the spell is on the bar");
    let row = w.slots[i].handle;
    reveal(ui, row);
    let b = ui.screen_box(row);
    let (x, y) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
    let hit = ui
        .hit_test_screen(x, y)
        .expect("the pointer is over the bar");
    assert!(
        hit == row || ui.is_ancestor_of(row, hit),
        "the press must land on the row itself"
    );
    ui.mouse_down(7, x, y);
    deliver(ui, panels, view);
    ui.mouse_up(7, x, y, false);
    deliver(ui, panels, view);
    ui.requests.clear();
}

// =================================================================================================
// 1. The denominator: the four ids are where post-init looks for them.
// =================================================================================================

/// **The post-init searches the endowment icon, not the panel, for the three.**
///
/// The station that would have caught the catalogue's mis-naming: all three are found from
/// the endowment icon and all three really are its descendants in the shipped tree. The spell
/// name is the one of the four that hangs off the panel root.
#[test]
fn the_four_children_are_bound_where_post_init_looks_for_them() {
    let (mut ui, panels, _, root) = env();
    let p = &panels.spellcasting;
    let icon = p
        .endowment_icon
        .expect("the endowment icon binds (0x100000B1)");
    for (name, id, h) in [
        (
            "endowment_icon_underlay",
            ENDOWMENT_UNDERLAY,
            p.endowment_underlay,
        ),
        (
            "endowment_icon_overlay",
            ENDOWMENT_OVERLAY,
            p.endowment_overlay,
        ),
        (
            "endowment_icon_selected",
            ENDOWMENT_SELECTED,
            p.endowment_ring,
        ),
    ] {
        let h = h.unwrap_or_else(|| panic!("{name} ({id:?}) binds under the endowment icon"));
        assert_eq!(ui.node(h).unwrap().desc.element_id, id, "{name}");
        assert!(
            ui.is_ancestor_of(icon, h),
            "{name} is a child of the endowment icon, not the panel"
        );
    }
    let text = p.spell_name.expect("the spell name binds (0x1000048B)");
    assert_eq!(ui.node(text).unwrap().desc.element_id, SPELL_NAME);
    assert!(ui.is_ancestor_of(root, text));
    assert!(
        ui.text_element_mut(text).is_some(),
        "the post-init cast says the spell name is a text element"
    );
    // The post-init's last act on the endowment icon is to hide it,
    // so the icon starts hidden and only an endowment brings it up.
    assert!(
        !ui.node(icon).unwrap().region.flags.visible,
        "post-init hides the endowment icon"
    );
    assert_eq!(ENDOWMENT_ICON, ElementId(0x1000_00B1));
}

// =================================================================================================
// 2. The caption.
// =================================================================================================

/// Behaviour: spellbar.caption.selecting-a-spell-or-a-wand-names-it
/// **Selecting a spell on the bar writes that spell's name into the spell name.**
///
/// The caption is the selected spell's name, handed to the caption's own `SetText`.
/// The press is a real `mouse_down(7)` on the real row.
#[test]
fn selecting_a_spell_on_the_bar_names_it_in_the_caption() {
    let (mut ui, mut panels, view, _) = env();
    frame(&mut ui, &mut panels, &view);
    assert_eq!(
        caption(&mut ui, &panels),
        "",
        "nothing selected, no wand: the caption is cleared"
    );
    click_row(&mut ui, &mut panels, &view, 3);
    let tab = panels.spellcasting.open_sub_menu_index(&ui);
    assert_eq!(
        panels.spellcasting.sub_menus[tab].selected_spell, 3,
        "the press really selected"
    );
    assert_eq!(caption(&mut ui, &panels), "Spell 3");
    click_row(&mut ui, &mut panels, &view, 6);
    assert_eq!(
        caption(&mut ui, &panels),
        "Spell 6",
        "the caption follows the selection"
    );
}

/// **A wand with no spell selected captions `"<item> (<spell>)"`, out of the `SpellTable`.**
///
/// The format is `L"%s (%hs)"` — the wide object name and the narrow
/// spell name. [`WAND_SPELL`] is **not** in [`GameView::spellbook`], so a caption
/// built from the book would be empty: this is the station that separates the table from the
/// book.
#[test]
fn a_wand_captions_its_own_name_and_the_spell_from_the_table() {
    let (mut ui, mut panels, mut view, _) = env();
    frame(&mut ui, &mut panels, &view);
    assert!(
        !view.spellbook().iter().any(|s| s.id == WAND_SPELL),
        "the fixture's premise: the wand's spell is not a transcribed spell"
    );
    view.endowment = Some((WAND, WAND_SPELL));
    frame(&mut ui, &mut panels, &view);
    assert_eq!(
        caption(&mut ui, &panels),
        format!("{WAND_NAME} ({WAND_SPELL_NAME})")
    );
    // A non-zero selected spell id jumps to the spell arm whichever
    // way the endowment test went, so the selected spell wins the caption.
    click_row(&mut ui, &mut panels, &view, 2);
    frame(&mut ui, &mut panels, &view);
    assert_eq!(
        caption(&mut ui, &panels),
        "Spell 2",
        "the selection outranks the endowment"
    );
}

/// **A wand whose spell the table does not know still shows the wand, and captions nothing.**
///
/// Two branches of two functions meet on this one wand, and both are "say nothing rather than
/// guess":
///
/// * an unknown spell makes the spell-table lookup fail, which skips the caption's whole `sprintf`, so
///   the earlier `clear_all_text` is what the player is left with. Not the id, not the item name
///   alone, not `"unknown spell"`;
/// * a null spell-icon lookup skips the underlay's blit-mode set **and** its `SetImage`,
///   while the overlay's `SetImage` is unconditional. So the wand's own picture comes up over
///   an empty underlay and the icon is still raised.
#[test]
fn a_wand_whose_spell_the_table_lacks_shows_the_item_and_no_caption() {
    let (mut ui, mut panels, mut view, _) = env();
    frame(&mut ui, &mut panels, &view);
    click_row(&mut ui, &mut panels, &view, 5);
    assert_eq!(caption(&mut ui, &panels), "Spell 5");

    // 4242 is in neither the book nor the table.
    assert!(view.spell(4242).is_none(), "the fixture's premise");
    view.endowment = Some((WAND, 4242));
    // Clear the selection the way retail does, so the caption falls to the endowment arm.
    let tab = panels.spellcasting.open_sub_menu_index(&ui);
    panels.spellcasting.set_selected(&mut ui, tab, 0);
    frame(&mut ui, &mut panels, &view);

    assert_eq!(
        caption(&mut ui, &panels),
        "",
        "an unknown spell skips the sprintf"
    );
    let p = &panels.spellcasting;
    let (icon, under, over) = (
        p.endowment_icon.unwrap(),
        p.endowment_underlay.unwrap(),
        p.endowment_overlay.unwrap(),
    );
    assert!(
        ui.node(icon).unwrap().region.flags.visible,
        "the icon is still raised"
    );
    assert_eq!(
        picture(&ui, under),
        None,
        "a null spell icon leaves the underlay cleared"
    );
    assert!(
        picture(&ui, over).is_some(),
        "the overlay blits the wand regardless"
    );
}

// =================================================================================================
// 3. The icon's three children.
// =================================================================================================

/// **An endowed wand raises the icon, puts the spell under it and the wand over it.**
///
/// Both pictures go down in the three-alpha blit mode, the spell-slot mode and not the item
/// slot's normal blit. The underlay takes the spell-icon lookup's composite (so the base is the
/// `UISpellBackgrounds` row for the spell's level, not its raw icon id) and the overlay takes
/// the item's drag icon, which is the drag surface of the item's own composite.
#[test]
fn an_endowed_wand_lights_the_icon_with_the_spell_under_the_item() {
    let (mut ui, mut panels, mut view, _) = env();
    frame(&mut ui, &mut panels, &view);
    let p = &panels.spellcasting;
    let (icon, under, over) = (
        p.endowment_icon.unwrap(),
        p.endowment_underlay.unwrap(),
        p.endowment_overlay.unwrap(),
    );
    assert!(
        !ui.node(icon).unwrap().region.flags.visible,
        "no wand: the icon is kept down"
    );

    view.endowment = Some((WAND, WAND_SPELL));
    frame(&mut ui, &mut panels, &view);
    assert!(
        ui.node(icon).unwrap().region.flags.visible,
        "an endowment raises the icon"
    );
    assert_eq!(
        blit(&ui, under),
        Some(dereth_ui::region::BlitMode::Alpha3),
        "the underlay is 3-alpha"
    );
    assert_eq!(
        blit(&ui, over),
        Some(dereth_ui::region::BlitMode::Alpha3),
        "the overlay is 3-alpha"
    );

    let want_spell =
        dereth_ui_screens::items::widget::spell_recipe(&ui, 6, Some(DataId(0x0600_13B0)), 0);
    assert_eq!(
        picture(&ui, under),
        want_spell.base(),
        "the underlay is the spell icon's composite"
    );
    let want_drag = dereth_ui_screens::items::widget::object_recipe(
        &ui,
        &SlotDecoration {
            icon_id: WAND_ICON,
            obj_type: 0x0000_8000,
            ..SlotDecoration::default()
        },
    )
    .drag_surface();
    assert_eq!(
        picture(&ui, over),
        want_drag.base(),
        "the overlay is the item's drag icon, not its plain icon"
    );
    assert_ne!(
        picture(&ui, under),
        picture(&ui, over),
        "two different pictures, two elements"
    );

    // The icon also takes the caption's text as its tooltip, with the tooltip flag set.
    assert_eq!(
        ui.node(icon).unwrap().tooltip_text.as_deref(),
        Some(format!("{WAND_NAME} ({WAND_SPELL_NAME}){ENDOWMENT_TOOLTIP_TAIL}").as_str())
    );
    assert!(ui.node(icon).unwrap().region.flags.tooltip);

    // Taking the wand off hides the icon and clears nothing else.
    view.endowment = None;
    frame(&mut ui, &mut panels, &view);
    assert!(!ui.node(icon).unwrap().region.flags.visible);
    assert_eq!(
        picture(&ui, under),
        want_spell.base(),
        "the absent arm clears no image"
    );
    assert_eq!(caption(&mut ui, &panels), "", "and the caption empties");
}

/// **The ring follows the open sub-menu's endowment-selected flag and nothing else.**
///
/// The show and hide arms differ only in their argument. With a wand and no spell
/// selected the client arms the endowment itself and the ring comes up; clicking a spell row
/// calls the spell-selection path, which clears the endowment-selected flag on the row it finds,
/// and the ring goes down.
#[test]
fn the_ring_follows_the_endowment_selection() {
    let (mut ui, mut panels, mut view, _) = env();
    frame(&mut ui, &mut panels, &view);
    let ring = panels.spellcasting.endowment_ring.unwrap();
    assert!(
        !ui.node(ring).unwrap().region.flags.visible,
        "no wand, no ring"
    );

    view.endowment = Some((WAND, WAND_SPELL));
    frame(&mut ui, &mut panels, &view);
    let tab = panels.spellcasting.open_sub_menu_index(&ui);
    assert!(
        panels.spellcasting.sub_menus[tab].endowment_selected,
        "with nothing selected the endowment arms itself"
    );
    assert!(
        ui.node(ring).unwrap().region.flags.visible,
        "the selected arm shows the ring"
    );

    click_row(&mut ui, &mut panels, &view, 4);
    frame(&mut ui, &mut panels, &view);
    assert!(!panels.spellcasting.sub_menus[tab].endowment_selected);
    assert!(
        ui.node(ring)
            .unwrap()
            .region
            .flags
            .visible
            .then_some(())
            .is_none(),
        "hide arm"
    );
    assert_eq!(caption(&mut ui, &panels), "Spell 4");
}

/// Behaviour: spellbar.ring.the-ring-and-underlay-follow-the-endowment-selection-and-paging
/// **The ring is per open tab, and paging the bar re-reads it.**
///
/// The flag the ring reads is the **open** sub-menu's, and the endowment-icon update runs on
/// both of the tab-paging notices, next and previous. Tab 0 keeps its selected spell while
/// tab 1 has none, so the same frame answers differently for the two.
#[test]
fn paging_the_bar_re_reads_the_ring_and_the_caption_for_the_open_tab() {
    let (mut ui, mut panels, mut view, _) = env();
    view.endowment = Some((WAND, WAND_SPELL));
    frame(&mut ui, &mut panels, &view);
    let ring = panels.spellcasting.endowment_ring.unwrap();
    click_row(&mut ui, &mut panels, &view, 4);
    frame(&mut ui, &mut panels, &view);
    assert_eq!(caption(&mut ui, &panels), "Spell 4");
    assert!(!ui.node(ring).unwrap().region.flags.visible);

    let opened = panels.spellcasting.open_spell_tab(
        &mut ui,
        dereth_ui_screens::panels::spellcasting::TabJump::Next,
    );
    assert!(opened.is_some(), "the bar really paged");
    frame(&mut ui, &mut panels, &view);
    let tab = panels.spellcasting.open_sub_menu_index(&ui);
    assert_ne!(tab, 0, "a different sub-menu is open");
    assert_eq!(
        panels.spellcasting.sub_menus[tab].selected_spell, 0,
        "the new tab has no selection"
    );
    assert!(
        ui.node(ring).unwrap().region.flags.visible,
        "so the endowment arms itself here"
    );
    assert_eq!(
        caption(&mut ui, &panels),
        format!("{WAND_NAME} ({WAND_SPELL_NAME})"),
        "and the caption is the wand's again"
    );
}

mod cast_button {
    //! The Cast button's two states and eight tooltips: empty vs full bar; untargeted/self spell
    //! enables; a targeted spell walks three arms; unknown spell clears tooltip; endowed wand walks
    //! four arms; selected spell outranks wand; formats are shipped ones.
    //! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

    #![allow(clippy::pedantic)]

    use crate::common::layout::RegistrationOrder;
    use dereth_primitives::{DataId, ObjectId};
    use dereth_ui::{ElemHandle, Screen, StateId, UiSystem};
    use dereth_ui_screens::panels::remaining::RemainingPanels;
    use dereth_ui_screens::panels::spellcasting::cast_button;
    use dereth_ui_screens::view::{GameView, SpellEntry};
    use std::collections::BTreeSet;

    const WAND: ObjectId = ObjectId(0x5000_0164);
    const WAND_SPELL: u32 = 900;
    const WAND_NAME: &str = "Rod of Frost";
    const WAND_SPELL_NAME: &str = "Frost Bolt VI";
    const TARGET: ObjectId = ObjectId(0x5000_0999);
    const TARGET_NAME: &str = "Drudge Slinker";
    /// The spell bitfield's self-targeted flag: a spell that carries it needs no target.
    const SELF_TARGETED: u32 = 0x0000_0008;

    #[derive(Debug, Default)]
    struct View {
        spells: Vec<SpellEntry>,
        table: Vec<SpellEntry>,
        tabs: [Vec<u32>; 8],
        endowment: Option<(ObjectId, u32)>,
        selected: Option<ObjectId>,
        /// Which spells answers true for.
        untargeted: BTreeSet<u32>,
        /// Whether the selected object is a legal target for the selected spell.
        spell_ok: bool,
        /// Whether the endowment item is useable on the caster with no target at all.
        item_self: bool,
        /// Whether the endowment item is useable on the selected object.
        item_ok: bool,
    }

    impl GameView for View {
        fn spellbook(&self) -> &[SpellEntry] {
            &self.spells
        }
        fn spell_tab(&self, tab: usize) -> &[u32] {
            &self.tabs[tab]
        }
        fn spell(&self, spell_id: u32) -> Option<SpellEntry> {
            self.table.iter().find(|s| s.id == spell_id).cloned()
        }
        fn endowment(&self) -> Option<(ObjectId, u32)> {
            self.endowment
        }
        fn selected_object(&self) -> Option<ObjectId> {
            self.selected
        }
        fn name(&self, id: ObjectId) -> Option<&str> {
            match id {
                WAND => Some(WAND_NAME),
                TARGET => Some(TARGET_NAME),
                _ => None,
            }
        }
        fn spell_is_untargeted(&self, spell_id: u32) -> bool {
            self.untargeted.contains(&spell_id)
        }
        fn spell_target_compatible(&self, _spell_id: u32) -> bool {
            self.selected.is_some() && self.spell_ok
        }
        fn item_useable_self_target(&self, _item: ObjectId) -> bool {
            self.item_self
        }
        fn item_target_compatible(&self, _item: ObjectId) -> bool {
            self.selected.is_some() && self.item_ok
        }
    }

    fn spell(id: u32, name: &str, bitfield: u32) -> SpellEntry {
        SpellEntry {
            id,
            name: name.to_owned(),
            icon: Some(DataId(0x0600_13A5)),
            school: 4,
            level: 1,
            display_order: i32::try_from(id).expect("a small id"),
            bitfield,
        }
    }

    fn env() -> (UiSystem, RemainingPanels, View) {
        let (mut ui, _flow, _store) =
            crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
        let mut screen = dereth_ui_screens::screens::gameplay::GamePlayScreen::default();
        screen
            .create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
            .expect("the shipped gameplay screen");
        let root = screen.root().unwrap();
        let mut panels = RemainingPanels::default();
        panels.post_init(&mut ui, root);
        let book: Vec<SpellEntry> = (1..=8)
            .map(|id| spell(id, &format!("Spell {id}"), 0))
            .collect();
        let mut table = book.clone();
        table.push(spell(WAND_SPELL, WAND_SPELL_NAME, 0));
        // A spell whose `_bitfield` carries `SelfTargeted` but whose formula still targets.
        table.push(spell(77, "Self Only", SELF_TARGETED));
        let view = View {
            spells: book,
            table,
            spell_ok: true,
            item_ok: true,
            ..View::default()
        };
        // The settling frame applies the allegiance panel's three initial talk-focus settings.
        panels.update(&mut ui, &view);
        panels.spellcasting.update_endowment(&mut ui, &view);
        ui.requests.clear();
        ui.notice_inbox.clear();
        (ui, panels, view)
    }

    fn frame(ui: &mut UiSystem, panels: &mut RemainingPanels, view: &View) {
        panels.update(ui, view);
        panels.spellcasting.update_endowment(ui, view);
        ui.requests.clear();
    }

    fn button(panels: &RemainingPanels) -> ElemHandle {
        panels
            .spellcasting
            .cast_button
            .expect("cast button 0x100000B2 binds")
    }

    /// `(state, tooltip text, tooltip flag)`, all three off the live element.
    fn read(ui: &UiSystem, panels: &RemainingPanels) -> (StateId, Option<String>, bool) {
        let n = ui.node(button(panels)).expect("alive");
        (n.state, n.tooltip_text.clone(), n.region.flags.tooltip)
    }

    /// Arm tab 0 with `rows` and select `pick` (0 = nothing selected).
    fn bar(
        ui: &mut UiSystem,
        panels: &mut RemainingPanels,
        view: &mut View,
        rows: &[u32],
        pick: u32,
    ) {
        view.tabs[0] = rows.to_vec();
        frame(ui, panels, view);
        let tab = panels.spellcasting.open_sub_menu_index(ui);
        panels.spellcasting.set_selected(ui, tab, pick);
        frame(ui, panels, view);
    }

    // =================================================================================================
    // 1. The two "nothing is armed" tooltips.
    // =================================================================================================

    /// The two literals that depend on whether the player has anything to cast at all, and the
    /// disabled state that precedes both.
    #[test]
    fn cast_button_an_empty_bar_and_a_full_one_say_different_things() {
        let (mut ui, mut panels, mut view) = env();
        bar(&mut ui, &mut panels, &mut view, &[], 0);
        assert_eq!(
            read(&ui, &panels),
            (
                cast_button::DISABLED,
                Some(cast_button::NO_SPELLS.to_owned()),
                true
            ),
            "no endowment and nothing on the bar: the no-spells tooltip"
        );

        bar(&mut ui, &mut panels, &mut view, &[1, 2, 3], 0);
        assert_eq!(
            read(&ui, &panels),
            (
                cast_button::DISABLED,
                Some(cast_button::SELECT_A_SPELL.to_owned()),
                true
            ),
            "something on the bar, nothing selected: the select-a-spell tooltip"
        );
    }

    // =================================================================================================
    // 2. The four spell tooltips.
    // =================================================================================================

    /// The two independent tests that enable the button — the spell being untargeted, and its
    /// bitfield carrying self-targeted — and the `"CAST %hs"` both reach.
    #[test]
    fn cast_button_an_untargeted_or_self_targeted_spell_enables_the_button() {
        let (mut ui, mut panels, mut view) = env();
        view.untargeted.insert(3);
        bar(&mut ui, &mut panels, &mut view, &[1, 2, 3], 3);
        assert_eq!(
            read(&ui, &panels),
            (cast_button::ENABLED, Some("CAST Spell 3".to_owned()), true),
            "an untargeted spell enables the button"
        );

        // 77 is *not* untargeted; only its `_bitfield & 8` enables it. Same string, other test.
        view.untargeted.clear();
        assert!(
            !view.spell_is_untargeted(77),
            "the fixture's premise for the second test"
        );
        assert_eq!(
            view.spell(77).unwrap().bitfield & SELF_TARGETED,
            SELF_TARGETED
        );
        bar(&mut ui, &mut panels, &mut view, &[77], 77);
        assert_eq!(
            read(&ui, &panels),
            (
                cast_button::ENABLED,
                Some("CAST Self Only".to_owned()),
                true
            ),
            "the self-targeted bit is a second, independent way in"
        );
    }

    /// Behaviour: spellbar.cast-button.its-state-and-tooltip-follow-the-selected-spell-its-target-and-the-wand
    /// The three answers a targeted spell gives, and the `" on %s"` that is *appended* rather than
    /// formatted in.
    #[test]
    fn cast_button_a_targeted_spell_walks_its_three_arms() {
        let (mut ui, mut panels, mut view) = env();
        bar(&mut ui, &mut panels, &mut view, &[1, 2, 3], 3);
        assert_eq!(
            read(&ui, &panels),
            (
                cast_button::DISABLED,
                Some("You must select a target for Spell 3".to_owned()),
                true
            ),
            "a targeted spell with nothing selected: the needs-a-target tooltip"
        );

        view.selected = Some(TARGET);
        view.spell_ok = true;
        frame(&mut ui, &mut panels, &view);
        assert_eq!(
            read(&ui, &panels),
            (
                cast_button::ENABLED,
                Some("CAST Spell 3 on Drudge Slinker".to_owned()),
                true
            ),
            "a compatible selection appends the target's name"
        );

        view.spell_ok = false;
        frame(&mut ui, &mut panels, &view);
        assert_eq!(
            read(&ui, &panels),
            (
                cast_button::DISABLED,
                Some("You must select an appropriate target for Spell 3".to_owned()),
                true
            ),
            "an incompatible selection: the needs-a-better-target tooltip"
        );
    }

    /// A selected spell the spell table does not know writes **nothing**, so the tooltip stays the
    /// clear from the top of the pass and the tooltip flag stays 0.
    #[test]
    fn cast_button_a_spell_the_table_lacks_clears_the_tooltip_instead() {
        let (mut ui, mut panels, mut view) = env();
        view.untargeted.insert(3);
        bar(&mut ui, &mut panels, &mut view, &[1, 2, 3], 3);
        assert_eq!(read(&ui, &panels).0, cast_button::ENABLED, "the control");

        // 4242 is on the bar and in no table, so the spell lookup misses.
        assert!(view.spell(4242).is_none());
        let tab = panels.spellcasting.open_sub_menu_index(&ui);
        panels.spellcasting.set_selected(&mut ui, tab, 4242);
        frame(&mut ui, &mut panels, &view);
        assert_eq!(
            read(&ui, &panels),
            (cast_button::DISABLED, None, false),
            "the tooltip flag is 0 and the text cleared, with the disabled state from the top standing"
        );
    }

    // =================================================================================================
    // 3. The four endowment tooltips.
    // =================================================================================================

    /// The wand's four arms. Its `%s` is the **caption**, `"<item> (<spell>)"`, which is why the
    /// spell's name appears inside an item string.
    #[test]
    fn cast_button_an_endowed_wand_walks_its_four_arms() {
        let (mut ui, mut panels, mut view) = env();
        view.endowment = Some((WAND, WAND_SPELL));
        let caption = format!("{WAND_NAME} ({WAND_SPELL_NAME})");

        view.item_self = true;
        bar(&mut ui, &mut panels, &mut view, &[1, 2, 3], 0);
        assert_eq!(
            read(&ui, &panels),
            (
                cast_button::ENABLED,
                Some(format!("USE the {caption}")),
                true
            ),
            "a self-useable wand enables the button with the USE format"
        );

        view.item_self = false;
        view.selected = None;
        frame(&mut ui, &mut panels, &view);
        assert_eq!(
            read(&ui, &panels),
            (
                cast_button::DISABLED,
                Some(format!("You must select a target for the {caption}")),
                true
            ),
            "a wand that needs a target, with nothing selected"
        );

        view.selected = Some(TARGET);
        view.item_ok = true;
        frame(&mut ui, &mut panels, &view);
        assert_eq!(
            read(&ui, &panels),
            (
                cast_button::ENABLED,
                Some(format!("USE the {caption} on {TARGET_NAME}")),
                true
            ),
            "a compatible selection appends the target's name"
        );

        view.item_ok = false;
        frame(&mut ui, &mut panels, &view);
        assert_eq!(
            read(&ui, &panels),
            (
                cast_button::DISABLED,
                Some(format!(
                    "You must select an appropriate\ntarget for the {caption}"
                )),
                true
            ),
            "an incompatible selection, newline and all"
        );
    }

    /// A selected spell beats the wand for the *tooltip* too, and that is the **opposite** of the
    /// order the cast *action* takes, where the endowment item wins.
    #[test]
    fn cast_button_a_selected_spell_outranks_the_wand() {
        let (mut ui, mut panels, mut view) = env();
        view.endowment = Some((WAND, WAND_SPELL));
        view.item_self = true;
        view.untargeted.insert(2);
        bar(&mut ui, &mut panels, &mut view, &[1, 2, 3], 2);
        assert_eq!(
            read(&ui, &panels),
            (cast_button::ENABLED, Some("CAST Spell 2".to_owned()), true),
            "the spell arm is taken whichever way the endowment test went"
        );
    }

    // =================================================================================================
    // 4. The literals.
    // =================================================================================================

    /// The eight formats, verbatim, and the two states — pinned as the client's own shipped
    /// strings rather than through whatever wrote them, as required by the stated testability rule.
    #[test]
    fn cast_button_the_eight_formats_and_two_states_are_the_shipped_ones() {
        assert_eq!(
            cast_button::DISABLED,
            StateId(0x0000_000D),
            "the disabled state is 13"
        );
        assert_eq!(
            cast_button::ENABLED,
            StateId(0x0000_0001),
            "the enabled state is 1"
        );
        assert_eq!(cast_button::NO_SPELLS, "You have no spells ready to cast");
        assert_eq!(cast_button::SELECT_A_SPELL, "Select a spell to cast");
        assert_eq!(cast_button::USE_THE, "USE the %s");
        assert_eq!(
            cast_button::ITEM_NEEDS_TARGET,
            "You must select a target for the %s"
        );
        assert_eq!(
            cast_button::ITEM_NEEDS_BETTER_TARGET,
            "You must select an appropriate\ntarget for the %s" // the newline is retail's
        );
        assert_eq!(cast_button::CAST, "CAST %hs");
        assert_eq!(
            cast_button::SPELL_NEEDS_TARGET,
            "You must select a target for %hs"
        );
        assert_eq!(
            cast_button::SPELL_NEEDS_BETTER_TARGET,
            "You must select an appropriate target for %hs"
        );
        assert_eq!(cast_button::ON_TARGET, " on %s");
        // One argument, one substitution, whichever width marker the format carries.
        assert_eq!(
            cast_button::sprintf1(cast_button::CAST, "Strength Self VI"),
            "CAST Strength Self VI"
        );
        assert_eq!(
            cast_button::sprintf1(cast_button::USE_THE, "Wand"),
            "USE the Wand"
        );
        assert_eq!(
            cast_button::sprintf1(cast_button::ON_TARGET, "Rat"),
            " on Rat"
        );
    }
}
