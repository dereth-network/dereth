//! The nine item shortcuts, and the spell favorites bar: seven spell tabs, a layout that
//! follows the bar's width, and the cast control.
use super::super::*;
use super::common::*;
use super::items::entry;
use crate::int::{i32_from, u32_from};
use dereth_client_contract::view::MagicNotice;
#[derive(Debug, Default)]
pub struct Shortcuts;
impl Panel for Shortcuts {
    fn id(&self) -> &'static str {
        "shortcuts"
    }
    fn frame(&self, ctx: &Context<'_>) -> PanelFrame {
        let mut f = PanelFrame::new(300, 32);
        image(&mut f, 0x06001149, rect(0, 0, 7, 32), None, false, false);
        for i in 0..9 {
            let x = 7 + i * 32;
            image(
                &mut f,
                0x060010fa + i as u32,
                rect(x, 0, 32, 32),
                None,
                false,
                false,
            );
            f.control(
                format!("slot:{i}"),
                rect(x, 0, 32, 32),
                ControlKind::Items {
                    // An empty slot still holds one vacant entry, so a drag over it shows
                    // whether the item can go there.
                    entries: vec![ctx.game.shortcut(i as u32).map_or_else(
                        || ItemEntry {
                            icon: Some(DataId(0x060010fa + i as u32)),
                            ..ItemEntry::empty()
                        },
                        |id| entry(ctx.game, id),
                    )],
                    columns: 1,
                    slot_size: 32,
                    selected: ctx.game.selected_object(),
                },
                true,
            );
        }
        f
    }
    fn event(&mut self, e: ControlEvent, ctx: &Context<'_>) -> Vec<PanelAction> {
        let dragging = matches!(e, ControlEvent::DragStart { .. });
        match e {
            ControlEvent::Select { id, .. } | ControlEvent::DragStart { id, .. } => {
                if let Some(object) = id
                    .strip_prefix("slot:")
                    .and_then(|s| s.parse::<u32>().ok())
                    .filter(|i| *i < 9)
                    .and_then(|i| ctx.game.shortcut(i))
                {
                    return if dragging {
                        let from = id[5..].parse::<u32>().unwrap();
                        vec![
                            PanelAction::Game(UiRequest::RemoveShortcut(object)),
                            PanelAction::BeginDrag(DragPayload::Shortcut { object, from }),
                        ]
                    } else {
                        super::items::item_click(object, super::items::Click::Left, ctx)
                            .unwrap_or_else(|| vec![PanelAction::Game(UiRequest::Select(object))])
                    };
                }
            }
            ControlEvent::RightClick { id, .. } => {
                if let Some(object) = id
                    .strip_prefix("slot:")
                    .and_then(|s| s.parse::<u32>().ok())
                    .filter(|i| *i < 9)
                    .and_then(|i| ctx.game.shortcut(i))
                {
                    return super::items::item_click(object, super::items::Click::Right, ctx)
                        .unwrap_or_default();
                }
            }
            ControlEvent::Activate(id) => {
                if let Some(object) = id
                    .strip_prefix("slot:")
                    .and_then(|s| s.parse::<u32>().ok())
                    .filter(|i| *i < 9)
                    .and_then(|i| ctx.game.shortcut(i))
                {
                    return vec![PanelAction::Game(UiRequest::Use(object))];
                }
            }
            ControlEvent::Drop {
                id,
                payload: DragPayload::Shortcut { object: item, from },
                ..
            } => {
                if let Some(slot) = id
                    .strip_prefix("slot:")
                    .and_then(|s| s.parse::<u32>().ok())
                    .filter(|i| *i < 9)
                {
                    return vec![PanelAction::Host(HostAction::ClassicShortcutDrop {
                        object: item,
                        slot,
                        from: Some(from),
                    })];
                }
            }
            ControlEvent::Drop {
                id,
                payload: DragPayload::Object(item),
                ..
            } => {
                if let Some(slot) = id
                    .strip_prefix("slot:")
                    .and_then(|s| s.parse::<u32>().ok())
                    .filter(|i| *i < 9)
                {
                    return vec![PanelAction::Host(HostAction::ClassicShortcutDrop {
                        object: item,
                        slot,
                        from: None,
                    })];
                }
            }
            _ => {}
        }
        vec![]
    }
}
/// The tab art: seven in the classic interface's portal; an eighth tab, which a world after it
/// has, wears the seventh's.
const NORMAL: [u32; 8] = [
    0x06001cbb, 0x06001cbc, 0x06001cbd, 0x06001cbe, 0x06001cbf, 0x06002624, 0x06002625, 0x06002625,
];
const SELECTED: [u32; 8] = [
    0x06001cb6, 0x06001cb7, 0x06001cb8, 0x06001cb9, 0x06001cb5, 0x06002622, 0x06002623, 0x06002623,
];

/// How many spell tabs the world has: seven before Throne of Destiny, eight after.
fn tab_count(game: &dyn GameView) -> usize {
    if game.era().is_some_and(|e| e.before_throne_of_destiny()) {
        7
    } else {
        8
    }
}
#[derive(Debug, Default)]
pub struct Favorites {
    tab: usize,
    selected: Option<u32>,
    endowment: bool,
    offset: usize,
    tab_state: [(Option<u32>, bool, usize); 8],
    /// The bar's width: the 3D view's.
    width: u32,
}
impl Favorites {
    fn width(&self) -> i32 {
        i32::try_from(self.width).unwrap_or(0).max(400)
    }
    fn spell_at(&self, id: &str, g: &dyn GameView) -> Option<u32> {
        id.strip_prefix("spell:")?
            .parse::<usize>()
            .ok()
            .and_then(|i| g.spell_tab(self.tab).get(i).copied())
    }
    fn change_tab(&mut self, tab: usize) {
        self.tab_state[self.tab] = (self.selected, self.endowment, self.offset);
        self.tab = tab;
        (self.selected, self.endowment, self.offset) = self.tab_state[tab];
    }
    /// The favorites list shows the equipped item's endowed spell before the ordinary spells.
    fn magic(&mut self, notice: MagicNotice, ctx: &Context<'_>) -> Vec<PanelAction> {
        use MagicNotice::*;
        let g = ctx.game;
        let count = tab_count(g);
        match notice {
            PrevSpellTab => self.change_tab((self.tab + count - 1) % count),
            NextSpellTab => self.change_tab((self.tab + 1) % count),
            FirstSpellTab => self.change_tab(0),
            LastSpellTab => self.change_tab(count - 1),
            CastCurrentSpell => return self.event(ControlEvent::Activate("cast".into()), ctx),
            CastQuickslotSpell { slot } => {
                if let Some(spell) = g.spell_tab(self.tab).get(slot).copied() {
                    self.selected = Some(spell);
                    self.endowment = false;
                    return self.event(ControlEvent::Activate("cast".into()), ctx);
                }
            }
            PrevSpellSelection | NextSpellSelection | FirstSpellSelection | LastSpellSelection => {
                let mut choices = Vec::new();
                if let Some((object, spell)) = g.endowment() {
                    choices.push((Some(object), spell));
                }
                choices.extend(g.spell_tab(self.tab).iter().map(|spell| (None, *spell)));
                if choices.is_empty() {
                    return vec![];
                }
                let current = self
                    .choice(g)
                    .and_then(|choice| choices.iter().position(|v| *v == choice))
                    .unwrap_or(0);
                let next = match notice {
                    PrevSpellSelection => (current + choices.len() - 1) % choices.len(),
                    NextSpellSelection => (current + 1) % choices.len(),
                    FirstSpellSelection => 0,
                    LastSpellSelection => choices.len() - 1,
                    _ => unreachable!(),
                };
                let (object, spell) = choices[next];
                self.endowment = object.is_some();
                self.selected = object.is_none().then_some(spell);
                if object.is_none() {
                    let index = next - usize::from(g.endowment().is_some());
                    let visible = layout(self.width(), g.endowment().is_some()).2 as usize / 32;
                    if index < self.offset {
                        self.offset = index;
                    } else if index >= self.offset + visible {
                        self.offset = (index + 1).saturating_sub(visible);
                    }
                }
            }
        }
        vec![]
    }
    fn choice(&self, g: &dyn GameView) -> Option<(Option<ObjectId>, u32)> {
        if self.endowment {
            return g.endowment().map(|(o, s)| (Some(o), s));
        }
        if let Some(s) = self.selected.filter(|s| g.spell_tab(self.tab).contains(s)) {
            return Some((None, s));
        }
        g.endowment()
            .map(|(o, s)| (Some(o), s))
            .or_else(|| g.spell_tab(self.tab).first().copied().map(|s| (None, s)))
    }
    fn cast_label(&self, g: &dyn GameView) -> (String, bool) {
        let Some((object, spell)) = self.choice(g) else {
            return (
                if g.endowment().is_none() && g.spell_tab(self.tab).is_empty() {
                    "You have no spells\nready to cast"
                } else {
                    "Select a spell to cast"
                }
                .into(),
                false,
            );
        };
        let name = object
            .and_then(|o| g.name(o).map(str::to_owned))
            .or_else(|| g.spell(spell).map(|s| s.name))
            .unwrap_or_default();
        let target = g.selected_object();
        let (verb, subject, self_target, compatible) = if let Some(o) = object {
            (
                "USE",
                format!("the {name}"),
                g.item_useable_self_target(o),
                g.item_target_compatible(o),
            )
        } else {
            (
                "CAST",
                name,
                g.spell_is_untargeted(spell) || g.spell(spell).is_some_and(|s| s.bitfield & 8 != 0),
                g.spell_target_compatible(spell),
            )
        };
        if self_target {
            return (format!("{verb} {subject}"), true);
        }
        if target.is_none() {
            return (format!("You must select a target for\n{subject}"), false);
        }
        if !compatible {
            return (
                format!("You must select an appropriate\ntarget for {subject}"),
                false,
            );
        }
        (
            format!(
                "{verb} {subject}\non {}",
                target.and_then(|o| g.name(o)).unwrap_or("")
            ),
            true,
        )
    }
}
fn layout(width: i32, endowment: bool) -> (i32, i32, i32) {
    let extra = if endowment { 42 } else { 0 };
    let list_width = (width - extra - 86) & !31;
    let total = extra + 56 + list_width;
    let left = (width - total) / 2;
    let list_x = left + extra + 28;
    (left, list_x, list_width)
}
impl Panel for Favorites {
    fn id(&self) -> &'static str {
        "spell-favorites"
    }
    fn resize(&mut self, width: u32, _: u32) {
        self.width = width;
    }
    fn frame(&self, ctx: &Context<'_>) -> PanelFrame {
        // The bar runs the 3D view's width: its texture tiles up to the right-hand piece, which
        // (with the cast button) keeps to the right edge.
        let w = self.width();
        let mut f = PanelFrame::new(u32::try_from(w).unwrap_or(400), 102);
        let g = ctx.game;
        image(
            &mut f,
            0x06001a8c,
            rect(0, 20, w - 327, 82),
            None,
            true,
            false,
        );
        image(
            &mut f,
            0x060019e5,
            rect(w - 327, 20, 327, 82),
            None,
            false,
            false,
        );
        for i in 0..tab_count(g) {
            art(
                f.button(
                    format!("tab:{i}"),
                    rect(i32_from(i) * 47, 0, 47, 20),
                    "",
                    true,
                ),
                if i == self.tab {
                    SELECTED[i]
                } else {
                    NORMAL[i]
                },
                SELECTED[i],
                NORMAL[i],
            );
        }
        image(
            &mut f,
            0x06001cba,
            rect(329, 0, w - 329, 20),
            None,
            true,
            false,
        );
        let (left, x, width) = layout(self.width(), g.endowment().is_some());
        let rows = g.spell_tab(self.tab);
        let visible = width as usize / 32;
        let offset = self.offset.min((rows.len() + 1).saturating_sub(visible));
        if let Some((object, spell)) = g.endowment() {
            f.control(
                "endowment",
                rect(left, 24, 32, 32),
                ControlKind::Items {
                    entries: vec![entry(g, object)],
                    columns: 1,
                    slot_size: 32,
                    selected: self.endowment.then_some(object),
                },
                true,
            );
            if let Some(s) = g.spell(spell) {
                spell_icon(
                    &mut f,
                    s.icon,
                    s.level,
                    s.bitfield,
                    rect(left, 24, 32, 32),
                    None,
                );
            }
        }
        art(
            f.button("previous", rect(x - 28, 21, 28, 40), "", offset > 0),
            0x06001a90,
            0x06001a8f,
            0x06001a90,
        );
        art(
            f.button(
                "next",
                rect(x + width, 21, 28, 40),
                "",
                offset + visible < rows.len() + 1,
            ),
            0x06001a91,
            0x06001a8e,
            0x06001a91,
        );
        for i in 0..visible {
            let id = rows.get(offset + i).copied();
            let number = offset + i;
            let r = rect(x + i32_from(i) * 32, 24, 32, 32);
            image(
                &mut f,
                if number < 9 {
                    0x06001aa6 + u32_from(number)
                } else {
                    0x06001a97
                },
                r,
                None,
                false,
                false,
            );
            if let Some(s) = id.and_then(|id| g.spell(id)) {
                spell_icon(
                    &mut f,
                    s.icon,
                    s.level,
                    s.bitfield,
                    rect(x + i32_from(i) * 32, 24, 32, 32),
                    None,
                );
            }
            if self
                .choice(g)
                .is_some_and(|(o, s)| o.is_none() && Some(s) == id)
            {
                image(&mut f, 0x060019e8, r, None, false, true);
            }
            if id.is_some() && number < 9 {
                image(
                    &mut f,
                    (if g.combat_mode() == 8 {
                        0x060019ed
                    } else {
                        0x06001acc
                    }) + u32_from(number),
                    r,
                    None,
                    false,
                    true,
                );
            }
            let c = f.button(
                format!("spell:{}", offset + i),
                rect(x + i32_from(i) * 32, 24, 32, 32),
                "",
                true,
            );
            c.paint = false;
            c.slot = true;
        }
        let (label, enabled) = self.cast_label(g);
        let button = f.button("cast", rect(w - 214, 66, 200, 32), label, enabled);
        button.font = "15-6".into();
        button.color = 0xffffffff;
        art(button, 0x060019e9, 0x060019eb, 0x060019ea);
        f
    }
    fn event(&mut self, e: ControlEvent, ctx: &Context<'_>) -> Vec<PanelAction> {
        let g = ctx.game;
        match e {
            ControlEvent::Magic(notice) => return self.magic(notice, ctx),
            // A double click casts the spell; a right click examines it.
            ControlEvent::DoubleClick { id, .. } if id.starts_with("spell:") => {
                if let Some(spell) = self.spell_at(&id, g) {
                    self.selected = Some(spell);
                    self.endowment = false;
                    return self.event(ControlEvent::Activate("cast".into()), ctx);
                }
            }
            ControlEvent::RightClick { id, .. } if id.starts_with("spell:") => {
                if let Some(spell) = self.spell_at(&id, g) {
                    return vec![PanelAction::OpenSpell {
                        id: "examine-spell".into(),
                        spell,
                    }];
                }
            }
            ControlEvent::Activate(id) if id.starts_with("tab:") => {
                let count = tab_count(ctx.game);
                if let Some(tab) = id[4..].parse::<usize>().ok().filter(|t| *t < count) {
                    self.change_tab(tab);
                }
            }
            ControlEvent::Activate(id) if id == "previous" => {
                self.offset = self.offset.saturating_sub(1)
            }
            ControlEvent::Activate(id) if id == "next" => self.offset += 1,
            ControlEvent::Select { id, .. } | ControlEvent::Activate(id) if id == "endowment" => {
                self.endowment = true;
                self.selected = None;
            }
            ControlEvent::Activate(id) if id.starts_with("spell:") => {
                if let Some(spell) = id[6..]
                    .parse::<usize>()
                    .ok()
                    .and_then(|i| g.spell_tab(self.tab).get(i).copied())
                {
                    self.selected = Some(spell);
                    self.endowment = false;
                }
            }
            ControlEvent::Activate(id) if id == "cast" => {
                if self.cast_label(g).1 {
                    if let Some((object, spell_id)) = self.choice(g) {
                        return vec![PanelAction::Game(if let Some(object) = object {
                            UiRequest::Use(object)
                        } else {
                            UiRequest::CastSpell { spell_id }
                        })];
                    }
                }
            }
            ControlEvent::DragStart { id, .. } if id.starts_with("spell:") => {
                if let Some(spell) = id[6..]
                    .parse::<usize>()
                    .ok()
                    .and_then(|i| g.spell_tab(self.tab).get(i).copied())
                {
                    self.selected = None;
                    self.endowment = false;
                    return vec![
                        PanelAction::Game(UiRequest::RemoveSpellFavorite {
                            spell_id: spell,
                            tab: self.tab,
                        }),
                        PanelAction::BeginDrag(DragPayload::Spell(spell)),
                    ];
                }
            }
            // A spell dropped on a tab's button goes to the back of that tab.
            ControlEvent::Drop {
                id,
                payload: DragPayload::Spell(spell_id),
                ..
            } if id.starts_with("tab:") => {
                if let Some(tab) = id[4..].parse::<usize>().ok().filter(|t| *t < tab_count(g)) {
                    if g.is_spell_known(spell_id) {
                        let spells = g.spell_tab(tab);
                        let mut actions = vec![];
                        let mut index = i32_from(spells.len());
                        if spells.contains(&spell_id) {
                            index -= 1;
                            actions.push(PanelAction::Game(UiRequest::RemoveSpellFavorite {
                                spell_id,
                                tab,
                            }));
                        }
                        actions.push(PanelAction::Game(UiRequest::AddSpellFavorite {
                            spell_id,
                            index,
                            tab,
                        }));
                        return actions;
                    }
                }
            }
            ControlEvent::Drop {
                id,
                payload: DragPayload::Spell(spell_id),
                ..
            } if id.starts_with("spell:") => {
                if let Ok(index) = id[6..].parse::<i32>() {
                    let mut index = index.clamp(0, i32_from(g.spell_tab(self.tab).len()));
                    if g.is_spell_known(spell_id) {
                        let mut actions = vec![];
                        if let Some(old) = g.spell_tab(self.tab).iter().position(|s| *s == spell_id)
                        {
                            if i32_from(old) < index {
                                index -= 1;
                            }
                            actions.push(PanelAction::Game(UiRequest::RemoveSpellFavorite {
                                spell_id,
                                tab: self.tab,
                            }));
                        }
                        actions.push(PanelAction::Game(UiRequest::AddSpellFavorite {
                            spell_id,
                            index,
                            tab: self.tab,
                        }));
                        self.selected = Some(spell_id);
                        self.endowment = false;
                        let visible = layout(self.width(), g.endowment().is_some()).2 as usize / 32;
                        if (index as usize) < self.offset {
                            self.offset = index as usize;
                        } else if index as usize >= self.offset + visible {
                            self.offset = (index as usize + 1).saturating_sub(visible);
                        }
                        return actions;
                    }
                }
            }
            _ => {}
        }
        vec![]
    }
}

#[cfg(test)]
mod magic_tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    #[derive(Debug)]
    struct World;
    impl GameView for World {
        fn spell_tab(&self, tab: usize) -> &[u32] {
            if tab == 0 {
                &[11, 12]
            } else {
                &[20]
            }
        }
        fn endowment(&self) -> Option<(ObjectId, u32)> {
            Some((ObjectId(5), 99))
        }
        fn spell_is_untargeted(&self, _: u32) -> bool {
            true
        }
        fn is_spell_known(&self, _: u32) -> bool {
            true
        }
        fn item_useable_self_target(&self, _: ObjectId) -> bool {
            true
        }
    }
    #[test]
    fn spell_keys_cycle_through_endowment_and_retain_selection_per_tab() {
        let world = World;
        // Bind default values outside Context so their lifetime spans dispatch.
        let pregame = Default::default();
        let keyboard = Default::default();
        let settings = Default::default();
        let classic = Default::default();
        let ctx = Context {
            game: &world,
            pregame: &pregame,
            keyboard: &keyboard,
            settings: &settings,
            map_teleport_allowed: false,
            classic: &classic,
        };
        let mut panel = Favorites::default();
        panel.magic(MagicNotice::PrevSpellSelection, &ctx);
        assert_eq!(panel.choice(&world), Some((None, 12)));
        panel.magic(MagicNotice::NextSpellSelection, &ctx);
        assert_eq!(panel.choice(&world), Some((Some(ObjectId(5)), 99)));
        panel.magic(MagicNotice::NextSpellSelection, &ctx);
        assert_eq!(panel.choice(&world), Some((None, 11)));
        panel.magic(MagicNotice::NextSpellTab, &ctx);
        assert_eq!(panel.tab, 1);
        panel.magic(MagicNotice::LastSpellSelection, &ctx);
        assert_eq!(panel.choice(&world), Some((None, 20)));
        panel.magic(MagicNotice::PrevSpellTab, &ctx);
        assert_eq!(panel.choice(&world), Some((None, 11)));
        // A world after the classic interface (no era announced) has eight spell tabs.
        panel.magic(MagicNotice::PrevSpellTab, &ctx);
        assert_eq!(panel.tab, 7);
        panel.magic(MagicNotice::FirstSpellTab, &ctx);
        assert_eq!(panel.tab, 0);
        assert_eq!(
            panel.magic(MagicNotice::CastQuickslotSpell { slot: 1 }, &ctx),
            [PanelAction::Game(UiRequest::CastSpell { spell_id: 12 })]
        );
        assert!(panel
            .magic(MagicNotice::CastQuickslotSpell { slot: 99 }, &ctx)
            .is_empty());
        assert_eq!(panel.choice(&world), Some((None, 12)));
    }
    #[test]
    fn a_double_click_casts_a_right_click_examines_and_a_drag_removes() {
        let world = World;
        let pregame = Default::default();
        let keyboard = Default::default();
        let settings = Default::default();
        let classic = Default::default();
        let ctx = Context {
            game: &world,
            pregame: &pregame,
            keyboard: &keyboard,
            settings: &settings,
            map_teleport_allowed: false,
            classic: &classic,
        };
        let mut panel = Favorites::default();
        let at = |id: &str| id.to_string();
        assert_eq!(
            panel.event(
                ControlEvent::DoubleClick {
                    id: at("spell:1"),
                    index: 0
                },
                &ctx
            ),
            [PanelAction::Game(UiRequest::CastSpell { spell_id: 12 })]
        );
        assert_eq!(
            panel.event(
                ControlEvent::RightClick {
                    id: at("spell:0"),
                    index: 0
                },
                &ctx
            ),
            [PanelAction::OpenSpell {
                id: "examine-spell".into(),
                spell: 11
            }]
        );
        assert_eq!(
            panel.event(
                ControlEvent::DragStart {
                    id: at("spell:0"),
                    index: 0
                },
                &ctx
            ),
            [
                PanelAction::Game(UiRequest::RemoveSpellFavorite {
                    spell_id: 11,
                    tab: 0
                }),
                PanelAction::BeginDrag(DragPayload::Spell(11)),
            ]
        );
    }
    #[test]
    fn a_spell_dropped_on_another_tab_goes_to_its_back() {
        let world = World;
        let pregame = Default::default();
        let keyboard = Default::default();
        let settings = Default::default();
        let classic = Default::default();
        let ctx = Context {
            game: &world,
            pregame: &pregame,
            keyboard: &keyboard,
            settings: &settings,
            map_teleport_allowed: false,
            classic: &classic,
        };
        let mut panel = Favorites::default();
        assert_eq!(
            panel.event(
                ControlEvent::Drop {
                    id: "tab:1".into(),
                    payload: DragPayload::Spell(11),
                    slot: 0
                },
                &ctx
            ),
            [PanelAction::Game(UiRequest::AddSpellFavorite {
                spell_id: 11,
                index: 1,
                tab: 1
            })]
        );
    }
    #[test]
    fn the_bar_runs_the_width_it_is_given_with_its_right_piece_at_the_edge() {
        let world = World;
        let pregame = Default::default();
        let keyboard = Default::default();
        let settings = Default::default();
        let classic = Default::default();
        let ctx = Context {
            game: &world,
            pregame: &pregame,
            keyboard: &keyboard,
            settings: &settings,
            map_teleport_allowed: false,
            classic: &classic,
        };
        let mut panel = Favorites::default();
        panel.resize(800, 102);
        let f = panel.frame(&ctx);
        assert_eq!(f.screen.width, 800);
        assert_eq!(
            f.controls.iter().find(|c| c.id == "cast").unwrap().rect,
            rect(586, 66, 200, 32)
        );
        assert!(f
            .controls
            .iter()
            .filter(|c| c.id.starts_with("spell:"))
            .all(|c| c.slot));
    }
}
