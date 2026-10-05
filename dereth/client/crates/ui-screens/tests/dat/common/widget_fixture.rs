//! Shared character sheet, effect row and equipment-drag fixtures.
#![allow(dead_code)]

pub(crate) use crate::common::layout::RegistrationOrder;
pub(crate) use crate::common::layout::Strings;
pub(crate) use std::rc::Rc;

pub(crate) use crate::common::*;
pub(crate) use dereth_primitives::{DataId, LocalTime, ObjectId};
pub(crate) use dereth_ui::{Delivery, ElemHandle, ElementId, Screen, UiSystem};
pub(crate) use dereth_ui_screens::panels::remaining::RemainingPanels;
pub(crate) use dereth_ui_screens::panels::{characterinfo, effects, inventory};
pub(crate) use dereth_ui_screens::screens::gameplay::GamePlayScreen;
pub(crate) use dereth_ui_screens::view::{
    CharacterInfo, EffectEntry, GameView, PlayerOption, SlotDecoration, UiRequest,
};

// ---------------------------------------------------------------------------------------------
// The strip, the panels and the doll, by element id
// ---------------------------------------------------------------------------------------------

/// `BurdenIndicator` — action `0x10000005`, which opens the **character info** panel.
pub(crate) const BURDEN_LAMP: ElementId = ElementId(0x1000_00F7);
/// `EffectsIndicator`, `0x1000000C` = 1 — action `0x10000006`.
pub(crate) const BUFF_LAMP: ElementId = ElementId(0x1000_00F5);

pub(crate) const INFO_SCROLLBAR: ElementId = ElementId(0x1000_011E);
/// The scrollbar's layout pass takes child 1 as its widget — the thumb.
pub(crate) const THUMB: ElementId = ElementId(1);

/// `InventoryPanelStack`'s page of `<PANS>`.
pub(crate) const INVENTORY_PAGE: ElementId = ElementId(0x1000_018B);

pub(crate) const PLAYER: ObjectId = ObjectId(0x5000_0001);
/// A breastplate: `INVENTORY_LOC` chest **armour**, `0x0200`.
pub(crate) const BREASTPLATE: ObjectId = ObjectId(0x5000_0101);
/// A shirt: `INVENTORY_LOC` chest **clothing**, `0x0002` — the same click-map colour.
pub(crate) const SHIRT: ObjectId = ObjectId(0x5000_0102);

// ---------------------------------------------------------------------------------------------
// Harness — `lamp_panels.rs`'s, unchanged
// ---------------------------------------------------------------------------------------------

pub(crate) fn env() -> UiSystem {
    let (mut ui, _flow, store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    ui.strings = Some(Rc::new(Strings(Rc::clone(&store))));
    ui
}

pub(crate) fn draw(ui: &mut UiSystem) {
    let mut back = dereth_ui::RecordingDrawBackend::default();
    ui.draw(&mut back);
}

pub(crate) fn pump(
    ui: &mut UiSystem,
    s: &mut GamePlayScreen,
    panels: &mut RemainingPanels,
    view: &dyn GameView,
) {
    for _ in 0..16 {
        let batch = ui.drain_outbox();
        let queued = s.take_panel_messages();
        if batch.is_empty() && queued.is_empty() {
            return;
        }
        for d in batch {
            if let Delivery::Element { msg, .. } = d {
                s.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
            }
        }
        for m in queued {
            panels.on_element_message(ui, &m, view);
        }
    }
}

/// The shipped gameplay screen with `RemainingPanels` bound off its root, the way
/// `dereth_client_shell::hud::Hud::drive` binds it.
pub(crate) fn screen(view: &dyn GameView) -> (UiSystem, GamePlayScreen, RemainingPanels) {
    let mut ui = env();
    let mut s = GamePlayScreen::default();
    s.create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds from the shipped layout");
    let mut panels = RemainingPanels::default();
    let root = s.root().expect("the gameplay root");
    panels.post_init(&mut ui, root);
    pump(&mut ui, &mut s, &mut panels, view);
    ui.requests.clear();
    (ui, s, panels)
}

pub(crate) fn find(ui: &UiSystem, s: &GamePlayScreen, id: ElementId) -> ElemHandle {
    let root = s.root().expect("the gameplay root");
    ui.get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{:#010X} is not in the shipped tree", id.0))
}

pub(crate) fn visible(ui: &UiSystem, h: ElemHandle) -> bool {
    ui.node(h).expect("alive").region.flags.visible
}

/// The centre of an element's screen box — where a hand puts the pointer.
pub(crate) fn centre(ui: &UiSystem, h: ElemHandle) -> (i32, i32) {
    let b = ui.screen_box(h);
    ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
}

/// The three events a player produces, and nothing else.
pub(crate) fn click(
    ui: &mut UiSystem,
    s: &mut GamePlayScreen,
    panels: &mut RemainingPanels,
    view: &dyn GameView,
    at: (i32, i32),
) {
    ui.mouse_move(LocalTime(0.0), at.0, at.1);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, at.0, at.1);
    ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, at.0, at.1, false);
    pump(ui, s, panels, view);
}

/// Press, move, release — the element manager's own drag-threshold walk
/// happens inside the moves, so the intermediate step is what makes this a drag rather than two
/// clicks.
pub(crate) fn drag(
    ui: &mut UiSystem,
    s: &mut GamePlayScreen,
    panels: &mut RemainingPanels,
    view: &dyn GameView,
    from: (i32, i32),
    to: (i32, i32),
) {
    ui.mouse_move(LocalTime(0.0), from.0, from.1);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, from.0, from.1);
    let steps = 8;
    for i in 1..=steps {
        let x = from.0 + (to.0 - from.0) * i / steps;
        let y = from.1 + (to.1 - from.1) * i / steps;
        ui.mouse_move(LocalTime(f64::from(i)), x, y);
        pump(ui, s, panels, view);
    }
    ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, to.0, to.1, false);
    pump(ui, s, panels, view);
}

// =============================================================================================
// 1. The burden lamp's panel has no scrollbar
// =============================================================================================

/// A character heavy enough that takes its `Load_Burdened` arm and its
/// augmentation arm as well — *"needs one when actually burdened"*.
#[derive(Debug, Default)]
pub(crate) struct Burdened;

impl GameView for Burdened {
    fn player(&self) -> Option<ObjectId> {
        Some(PLAYER)
    }
    fn character_info(&self) -> Option<CharacterInfo> {
        Some(CharacterInfo {
            innate: [290, 285, 275, 260, 190, 180],
            chess_rank: 1400,
            fishing_skill: 0,
            num_deaths: 7,
            strength: 290,
            endurance: 285,
            load: 1.4,
            encumbrance: 61_000,
            capacity: 43_500,
            augmentations: 3,
            ..CharacterInfo::default()
        })
    }
}

/// The info text's content extent against the box it is drawn in — the two numbers
/// the scrollbar's size update compares to decide whether the bar has anything to do.
pub(crate) fn info_extent(ui: &mut UiSystem, h: ElemHandle) -> (i32, i32) {
    let view = ui.screen_box(h).height();
    (
        ui.text_element_mut(h)
            .expect("a text element")
            .scroll
            .height,
        view,
    )
}

pub(crate) fn info_offset(ui: &mut UiSystem, h: ElemHandle) -> i32 {
    ui.text_element_mut(h).expect("a text element").scroll.y
}

/// Every character of the pane whose composed cell lies **inside its box**, in glyph order — what
/// a player can actually read, with the scroll offset already applied.
pub(crate) fn readable(ui: &mut UiSystem, h: ElemHandle) -> String {
    let box_ = ui.screen_box(h);
    let t = ui.text_element_mut(h).expect("a text element");
    t.compose(box_)
        .into_iter()
        .filter(|g| g.y >= box_.y0 && g.y < box_.y1)
        .filter_map(|g| char::from_u32(u32::from(g.ch)))
        .collect()
}

/// Open the burden lamp's panel the way the player does and fill it.
pub(crate) fn open_character_info(
    ui: &mut UiSystem,
    s: &mut GamePlayScreen,
    panels: &mut RemainingPanels,
    view: &dyn GameView,
) -> ElemHandle {
    let lamp = find(ui, s, BURDEN_LAMP);
    light(ui, lamp);
    let at = centre(ui, lamp);
    click(ui, s, panels, view, at);
    assert!(
        visible(ui, find(ui, s, characterinfo::PANEL)),
        "the click has to open the panel before its scrollbar can matter"
    );
    panels.update(ui, view);
    draw(ui);
    find(ui, s, characterinfo::INFO_TEXT)
}

// =============================================================================================
// 2. The beneficial-spell lamp lists spells that cannot be clicked
// =============================================================================================

#[derive(Debug, Default)]
pub(crate) struct Effects {
    pub(crate) list: Vec<EffectEntry>,
}

impl GameView for Effects {
    fn player(&self) -> Option<ObjectId> {
        Some(PLAYER)
    }
    fn active_effects(&self) -> Vec<EffectEntry> {
        self.list.clone()
    }
    fn player_option(&self, o: PlayerOption) -> bool {
        o == PlayerOption::SpellDuration
    }
}

pub(crate) fn entry(spell: u32, name: &str, beneficial: bool) -> EffectEntry {
    EffectEntry {
        spell,
        name: name.to_owned(),
        description: format!("{name} description"),
        icon: None,
        beneficial,
        remaining: 900.0,
        permanent: false,
        category: 0,
        power_level: 6,
    }
}

pub(crate) fn three_buffs() -> Vec<EffectEntry> {
    vec![
        entry(0x0101, "Strength Self VI", true),
        entry(0x0102, "Armor Self VI", true),
        entry(0x0103, "Blood Drinker Self VI", true),
    ]
}

pub(crate) fn open_buff_panel(
    ui: &mut UiSystem,
    s: &mut GamePlayScreen,
    panels: &mut RemainingPanels,
    view: &dyn GameView,
) {
    let lamp = find(ui, s, BUFF_LAMP);
    light(ui, lamp);
    let at = centre(ui, lamp);
    click(ui, s, panels, view, at);
    assert!(
        visible(ui, find(ui, s, effects::HELPFUL_PANEL)),
        "the click has to open the panel"
    );
    panels.update(ui, view);
    draw(ui);
}

// =============================================================================================
// 3. Items cannot be dragged from the paper doll
// =============================================================================================

#[derive(Debug)]
pub(crate) struct Dressed {
    pub(crate) equipment: Vec<(ObjectId, u32)>,
}

impl Default for Dressed {
    fn default() -> Self {
        Self {
            equipment: vec![(SHIRT, 0x0000_0002), (BREASTPLATE, 0x0000_0200)],
        }
    }
}

impl GameView for Dressed {
    fn player(&self) -> Option<ObjectId> {
        Some(PLAYER)
    }
    fn equipment(&self, _: ObjectId) -> &[(ObjectId, u32)] {
        &self.equipment
    }
    fn icon(&self, _: ObjectId) -> Option<DataId> {
        Some(DataId(0x0600_13A5))
    }
    fn slot_decoration(&self, id: ObjectId) -> Option<SlotDecoration> {
        Some(SlotDecoration {
            is_player: Some(id) == self.player(),
            obj_type: 0x2,
            icon_id: 0x0600_13A5,
            ..SlotDecoration::default()
        })
    }
    fn items_capacity(&self, _: ObjectId) -> Option<i32> {
        Some(102)
    }
    fn containers_capacity(&self, _: ObjectId) -> Option<i32> {
        Some(7)
    }
}

/// The panel-visibility notice is the route the toolbar's backpack button takes,
/// and the only one that puts `<PANS>` itself up. Setting the *page* visible on its own leaves the
/// window down, so the hit test answers `<SBOX>` `0x1000049A` (the 3D viewport) at the doll's
/// coordinates.
pub(crate) fn open_the_backpack(ui: &mut UiSystem, s: &mut GamePlayScreen) {
    let page = s
        .panels
        .pages
        .iter()
        .find(|p| p.element == INVENTORY_PAGE)
        .copied()
        .expect("the inventory page is in the shipped panel stack");
    s.recv_set_panel_visibility(ui, page.panel_id, true);
}

/// Open the inventory page and fill the doll, then hand back the drag mask `0x100001D6` and a
/// point on the figure whose click-map colour is the **chest**.
pub(crate) fn dressed_doll(
    ui: &mut UiSystem,
    s: &mut GamePlayScreen,
    view: &Dressed,
) -> (ElemHandle, (i32, i32)) {
    open_the_backpack(ui, s);
    s.inventory.update(ui, view);
    let mask = find(ui, s, inventory::PAPER_DOLL_DRAG_MASK);
    assert!(
        visible(ui, mask),
        "doll mode is the Slots checkbox's unchecked state"
    );
    let origin = ui.screen_origin(mask);
    // The chest colour `00 FF 00`, found in the shipped click map rather than guessed at, so the
    // point is the map's and not this file's.
    let map = s
        .inventory
        .click_map
        .as_ref()
        .expect("the click map resolved its enum");
    let mut at = None;
    for y in 0..213 {
        for x in 0..99 {
            if map.mask_at(x, y) == 0x0202 {
                at = Some((origin.0 + x, origin.1 + y));
                break;
            }
        }
        if at.is_some() {
            break;
        }
    }
    let at = at.expect("the shipped click map paints a chest region");
    (mask, at)
}
