//! The classic interface's HUD: its docked layout, the selected-object bar, the vitals, the chat
//! window, the link lamp and the radar.
use super::*;
use crate::int::{i32_from, i32_from_i64};
use dereth_client_contract::view::{PkStatus, RadarEntry, TargetMode, Vital};
use dereth_primitives::num::{to_i32, to_i32_f64};

mod character;
pub use character::{augmentation_text, AugmentationSheet};
mod chat_log;
mod combat;

const INK: u32 = 0xffd2d2c8;

pub fn make(id: &str) -> Option<Box<dyn Panel>> {
    Some(match id {
        "hud" => Box::new(Hud::default()),
        "radar" => Box::new(Radar::default()),
        "vitals" => Box::new(Vitals {
            width: 800,
            numeric: true,
            link: LinkLamp::default(),
        }),
        "chat" => Box::new(Chat::default()),
        "combat" => Box::new(combat::Combat::default()),
        "character-info" => Box::new(character::Character::default()),
        _ => return None,
    })
}

/// The health and mana queries selecting an object starts. The mana-active latch is set by a
/// received mana fraction, whereas health becomes active when its query is sent.
#[derive(Debug, Default)]
pub struct SelectionQueries {
    selected: Option<ObjectId>,
    health_active: bool,
    mana_active: bool,
}
impl SelectionQueries {
    pub fn update(
        &mut self,
        selected: Option<ObjectId>,
        facts: Option<dereth_client_contract::view::SelectionQueryFacts>,
        meters: (Option<f32>, Option<f32>),
    ) -> Vec<UiRequest> {
        self.mana_active |= meters.1.is_some();
        if self.selected == selected {
            return vec![];
        }
        self.selected = selected;
        let health = selected.filter(|_| facts.is_some_and(|f| f.attackable || f.is_player));
        let mana = selected.filter(|_| facts.is_some_and(|f| f.owned_by_player));
        let mut out = vec![];
        if self.health_active || health.is_some() {
            out.push(UiRequest::QueryHealth(health.unwrap_or(ObjectId(0))));
        }
        self.health_active = health.is_some();
        if self.mana_active || mana.is_some() {
            out.push(UiRequest::QueryItemMana(mana.unwrap_or(ObjectId(0))));
        }
        if mana.is_none() {
            self.mana_active = false;
        }
        out
    }
}

/// The classic host supplies live split state because HudView's trait defaults
/// do not expose it. Model-only callers retain their GameView implementation.
pub(crate) fn stack_split(c: &Context<'_>) -> (i32, i32) {
    let (split, max) = c
        .classic
        .stack_split
        .map(|(split, max)| {
            (
                split.min(i32::MAX as u32) as i32,
                max.min(i32::MAX as u32) as i32,
            )
        })
        .unwrap_or_else(|| (c.game.split_size(), c.game.max_split_size()));
    if max <= 0 {
        (0, 0)
    } else {
        (split.clamp(1, max), max)
    }
}

pub(crate) fn classic_date(t: i64, offset: i32) -> String {
    character::date(t, offset)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HudRegions {
    pub world: Rect,
    pub right: Rect,
    pub toolbar: Rect,
    pub bottom: Rect,
    pub chat: Rect,
    pub radar: Rect,
}

/// Whether the chat is at its taller height. The chat has two heights: dragging the bar along
/// its top edge up by 20 pixels or more makes it taller, and dragging it down again makes it
/// shorter; the 3D view gives up (or takes back) a quarter of its height.
static CHAT_EXPANDED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether the chat is at its taller height.
#[must_use]
pub fn chat_expanded() -> bool {
    CHAT_EXPANDED.load(std::sync::atomic::Ordering::Relaxed)
}

/// Set the chat's height.
pub fn set_chat_expanded(on: bool) {
    CHAT_EXPANDED.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Normal docked layout. Bottom service height is supplied by its own panel.
pub fn regions(width: u32, height: u32) -> HudRegions {
    regions_stretched(width, height, false)
}
/// The docked layout, optionally stretched to the window's height.
pub fn regions_stretched(width: u32, height: u32, stretch: bool) -> HudRegions {
    layout(width, height, stretch, chat_expanded())
}
/// The layout for a window size, the stretched interface and the chat's height.
fn layout(width: u32, height: u32, stretch: bool, chat_tall: bool) -> HudRegions {
    let w = (width as i32).max(640);
    let h = (height as i32).max(480);
    let right_h = if stretch { h - 118 } else { 362 };
    let right_y = h - 90 - right_h;
    // The taller chat takes a quarter of the 3D view's height, rounded down to an even number.
    let raise = if chat_tall { ((h - 129) / 4) & !1 } else { 0 };
    HudRegions {
        world: rect(0, 28, w - 309, h - 128 - raise),
        right: rect(w - 300, right_y, 300, right_h),
        toolbar: rect(w - 309, h - 90, 309, 90),
        bottom: rect(0, h - 101 - raise, w - 309, 0),
        chat: rect(0, h - 101 - raise, w - 309, 101 + raise),
        radar: radar_rect(w, h, stretch, true),
    }
}

/// Where the radar goes. With the side panel open, the interface not stretched and a window at
/// least 600 tall, it sits docked on its backdrop at the top of the right column. Otherwise it
/// floats over the 3D view's top-right corner: 120 pixels across when the view is at least 351
/// wide, else 80.
pub fn radar_rect(w: i32, h: i32, stretch: bool, side_open: bool) -> Rect {
    if side_open && !stretch && h >= 600 {
        return rect(w - 212, 28, 120, 120);
    }
    let right = if side_open { w - 309 } else { w };
    if right < 351 {
        rect(right - 81, 51, 80, 80)
    } else {
        rect(right - 121, 29, 120, 120)
    }
}

fn image(f: &mut PanelFrame, did: u32, r: Rect, tile: bool, clip: Option<Rect>) {
    f.image(&format!("{did:08X}"), r, tile, false);
    if let Some(Command::Image { clip: c, .. }) = f.screen.commands.last_mut() {
        *c = clip.map(|r| [r.x, r.y, r.x + r.w, r.y + r.h]);
    }
}
/// [`image`] with its black see-through.
fn keyed_image(f: &mut PanelFrame, did: u32, r: Rect, clip: Option<Rect>) {
    f.image(&format!("{did:08X}"), r, false, true);
    if let Some(Command::Image { clip: c, .. }) = f.screen.commands.last_mut() {
        *c = clip.map(|r| [r.x, r.y, r.x + r.w, r.y + r.h]);
    }
}
/// The page the toolbar's journal button opens: the journal on a world with it, else the
/// contracts; `None` on a world with neither, whose toolbar has no such button.
fn quest_page(game: &dyn GameView) -> Option<&'static str> {
    use dereth_client_contract::era::{quest_page, QuestPage};
    quest_page(game.era_features(), None).map(|page| match page {
        QuestPage::Journal => "journal",
        QuestPage::Contracts => "contracts",
    })
}

/// The side pages a toolbar button stands for: it is lit while any of them is shown, and a press
/// while one is shown closes it. The magic button's pages are the spellbook's tabs and the
/// research page, the social button's the social window's pages, the character button's its
/// tabs, the journal button's the journal and the contracts.
fn button_pages(id: &str) -> &'static [&'static str] {
    match id {
        "social" => &[
            "social",
            "allegiance",
            "fellowship",
            "trade-intro",
            "friends",
            "squelch",
        ],
        "spellbook" => &["spellbook", "components", "spell-research"],
        "character-stats" => &["character-stats", "attributes", "skills", "titles"],
        "journal" => &["journal", "contracts"],
        "map" => &["map"],
        "options" => &["options"],
        _ => &[],
    }
}

/// The toolbar's panel buttons left to right: id, left edge, width and the normal, lit and
/// pressed pictures. Five fill the row between its two end pieces; with the journal's there are
/// six, so the right end piece goes, each picture is cut a little narrower from both sides, and
/// the journal's sits between the character's and the map's.
fn toolbar_buttons(journal: bool) -> Vec<(&'static str, i32, i32, [u32; 3])> {
    let five = [
        ("social", 35, [0x0600111f, 0x06001120, 0x06001121]),
        ("spellbook", 34, [0x06001119, 0x0600111a, 0x0600111b]),
        ("character-stats", 34, [0x06001122, 0x06001123, 0x06001124]),
        ("map", 34, [0x06001116, 0x06001117, 0x06001118]),
        ("options", 39, [0x0600111c, 0x0600111d, 0x0600111e]),
    ];
    let mut out = vec![];
    if journal {
        let mut six = five.to_vec();
        six.insert(3, ("journal", 34, crate::composed::JOURNAL_BUTTON));
        let mut x = 62;
        for ((id, _, art), w) in six.into_iter().zip([31, 31, 30, 31, 30, 31]) {
            out.push((
                id,
                x,
                w,
                art.map(|a| crate::composed::narrowed(a, w as u32)),
            ));
            x += w;
        }
    } else {
        let mut x = 61;
        for (id, w, art) in five {
            out.push((id, x, w, art));
            x += w;
        }
    }
    out
}

fn button(f: &mut PanelFrame, id: &str, r: Rect, art: [u32; 3]) {
    f.button(id, r, "", true).images = Some(art.map(|a| format!("{a:08X}")));
}
fn append(f: &mut PanelFrame, child: PanelFrame, x: i32, y: i32, prefix: &str) {
    f.screen.commands.extend(
        child
            .screen
            .commands
            .into_iter()
            .map(|c| crate::desktop::translate_command(c, x, y)),
    );
    f.controls.extend(child.controls.into_iter().map(|mut c| {
        c.rect.x += x;
        c.rect.y += y;
        c.id = format!("{prefix}{}", c.id);
        c
    }));
}
/// The selected-object bar: the object's name, its stack splitter when it is a stack, and its
/// health or mana.
fn selected_object(f: &mut PanelFrame, c: &Context<'_>, x: i32, y: i32, draft: Option<&str>) {
    let (split, max) = stack_split(c);
    let stack = c.game.selected_object().is_some() && max > 1;
    let name_rect = if stack {
        rect(x + 78, y + 42, 146, 16)
    } else {
        rect(x + 78, y + 28, 146, 30)
    };
    if stack {
        image(f, 0x06001218, name_rect, false, None);
        f.edit(
            "stack-count",
            rect(x + 78, y + 28, 50, 14),
            draft
                .map(str::to_owned)
                .unwrap_or_else(|| split.to_string()),
            10,
            false,
            true,
        )
        .font = "14-6".into();
        f.controls.last_mut().unwrap().background = Some(0xff323232);
        let slider = rect(x + 128, y + 28, 96, 14);
        image(f, 0x0600127f, slider, false, None);
        image(
            f,
            0x06001150,
            rect(
                slider.x + i32_from_i64((split - 1) as i64 * 83 / (max - 1) as i64),
                slider.y,
                14,
                14,
            ),
            false,
            None,
        );
        f.control(
            "stack-slider",
            slider,
            ControlKind::ScrollBar {
                min: 1,
                max,
                value: split,
                page: 0,
                step: 1,
                vertical: false,
                arrow_size: 0,
                thumb_size: 14,
            },
            true,
        )
        .paint = false;
    } else {
        let (health, mana) = c.game.selected_meters();
        if let Some((level, back, front)) = health
            .map(|v| (v, 0x0600193e, 0x0600193f))
            .or_else(|| mana.map(|v| (v, 0x060022d5, 0x060022d6)))
        {
            image(f, back, name_rect, false, None);
            image(
                f,
                front,
                name_rect,
                false,
                Some(rect(
                    name_rect.x,
                    name_rect.y,
                    to_i32(146.0 * level.clamp(0.0, 1.0)),
                    30,
                )),
            );
        } else {
            image(f, 0x06001126, name_rect, false, None);
        }
    }
    f.fill(rect(x + 78, y + 27, 146, 1), 0xff000000);
    f.text_box(
        name_rect,
        c.game
            .selected_object()
            .and_then(|o| c.game.name(o))
            .unwrap_or(""),
        "14-6",
        INK,
        TextAlign::Center,
        !stack,
        None,
    );
}

fn unprefix(e: ControlEvent, prefix: &str) -> Option<ControlEvent> {
    Some(match e {
        ControlEvent::Activate(id) => ControlEvent::Activate(id.strip_prefix(prefix)?.into()),
        ControlEvent::Held { id, pressed } => ControlEvent::Held {
            id: id.strip_prefix(prefix)?.into(),
            pressed,
        },
        ControlEvent::Submit { id } => ControlEvent::Submit {
            id: id.strip_prefix(prefix)?.into(),
        },
        ControlEvent::Commit { id } => ControlEvent::Commit {
            id: id.strip_prefix(prefix)?.into(),
        },
        ControlEvent::Edit { id, text } => ControlEvent::Edit {
            id: id.strip_prefix(prefix)?.into(),
            text,
        },
        ControlEvent::Select { id, index } => ControlEvent::Select {
            id: id.strip_prefix(prefix)?.into(),
            index,
        },
        ControlEvent::DoubleClick { id, index } => ControlEvent::DoubleClick {
            id: id.strip_prefix(prefix)?.into(),
            index,
        },
        ControlEvent::Scroll { id, value } => ControlEvent::Scroll {
            id: id.strip_prefix(prefix)?.into(),
            value,
        },
        ControlEvent::Drop { id, payload, slot } => ControlEvent::Drop {
            id: id.strip_prefix(prefix)?.into(),
            payload,
            slot,
        },
        ControlEvent::DragStart { id, index } => ControlEvent::DragStart {
            id: id.strip_prefix(prefix)?.into(),
            index,
        },
        ControlEvent::RightClick { id, index } => ControlEvent::RightClick {
            id: id.strip_prefix(prefix)?.into(),
            index,
        },
        _ => return None,
    })
}

#[derive(Debug)]
struct Hud {
    width: u32,
    height: u32,
    vitals: Vitals,
    radar: Radar,
    chat: Chat,
    shortcuts: Box<dyn Panel>,
    split_draft: Option<(ObjectId, String)>,
    /// Where a drag of the chat's top bar started.
    divider_drag: Option<i32>,
}
impl Default for Hud {
    fn default() -> Self {
        Self {
            width: 800,
            height: 600,
            vitals: Vitals {
                width: 800,
                numeric: true,
                link: LinkLamp::default(),
            },
            radar: Radar::default(),
            chat: Chat::default(),
            shortcuts: super::game::make("shortcuts").unwrap(),
            split_draft: None,
            divider_drag: None,
        }
    }
}
impl Panel for Hud {
    fn id(&self) -> &'static str {
        "hud"
    }
    fn selected_text(&self) -> Option<String> {
        self.chat.selected_text()
    }
    /// The chat log is selectable text, so a press on it is the interface's even where no
    /// control lies under it.
    fn claims(&self, x: i32, y: i32) -> bool {
        let chat = regions(self.width, self.height).chat;
        x >= chat.x && x < chat.x + chat.w - 16 && y >= chat.y + 10 && y < chat.y + chat.h
    }
    fn input(
        &mut self,
        input: &crate::widgets::Input,
        context: &Context<'_>,
    ) -> Option<Vec<PanelAction>> {
        use crate::widgets::Input;
        let chat = regions(self.width, self.height).chat;
        // The chat log selects text.
        if let Input::PointerDown { x, y }
        | Input::PointerMove { x, y }
        | Input::PointerUp { x, y } = *input
        {
            let local = match *input {
                Input::PointerDown { .. } => Input::PointerDown {
                    x: x - chat.x,
                    y: y - chat.y,
                },
                Input::PointerMove { .. } => Input::PointerMove {
                    x: x - chat.x,
                    y: y - chat.y,
                },
                _ => Input::PointerUp {
                    x: x - chat.x,
                    y: y - chat.y,
                },
            };
            if let Some(actions) = self.chat.input(&local, context) {
                return Some(actions);
            }
        }
        match *input {
            Input::PointerDown { x, y }
                if x >= chat.x && x < chat.x + chat.w - 16 && y >= chat.y && y < chat.y + 10 =>
            {
                self.divider_drag = Some(y);
            }
            Input::PointerMove { y, .. } => {
                if let Some(start) = self.divider_drag {
                    let expanded = chat_expanded();
                    if (!expanded && start - y >= 20) || (expanded && y - start >= 20) {
                        set_chat_expanded(!expanded);
                        self.divider_drag = None;
                        let (w, h) = (self.width, self.height);
                        self.resize(w, h);
                    }
                }
            }
            Input::PointerUp { .. } => self.divider_drag = None,
            _ => {}
        }
        None
    }
    fn resize(&mut self, width: u32, height: u32) {
        self.width = width.max(640);
        self.height = height.max(480);
        self.vitals.width = self.width;
        let r = regions(self.width, self.height);
        self.chat.resize(r.chat.w as u32, r.chat.h as u32);
        self.radar.size = r.radar.w;
    }
    fn frame(&self, c: &Context<'_>) -> PanelFrame {
        let r = regions_stretched(
            self.width,
            self.height,
            c.classic.option_words[0] & 0x200000 != 0,
        );
        let mut f = PanelFrame::new(self.width, self.height);
        let stretch = c.classic.option_words[0] & 0x200000 != 0;
        let side_open = !c.classic.active_right.is_empty();
        let (w, h) = (self.width as i32, self.height as i32);
        let radar = radar_rect(w, h, stretch, side_open);
        if side_open {
            // The separator and the right column's strip stand beside the open side panel,
            // with the radar's backdrop when the radar is docked.
            image(
                &mut f,
                0x0600114c,
                rect(r.right.x - 9, 28, 9, h - 118),
                true,
                None,
            );
            image(
                &mut f,
                0x0600114d,
                rect(r.right.x, 28, 300, r.right.y - 28),
                true,
                None,
            );
            if radar.y == 28 && radar.w == 120 && radar.x == w - 212 {
                image(
                    &mut f,
                    0x060011dd,
                    rect(r.right.x, 28, 300, 120),
                    false,
                    None,
                );
            }
        } else {
            // With the side panel closed the 3D view fills the right column; the chat's top bar
            // runs on across it, and below a taller chat the column is filled with the strip's
            // texture down to the toolbar.
            image(
                &mut f,
                0x06001125,
                rect(w - 309, r.chat.y, 309, 10),
                true,
                None,
            );
            let below = r.toolbar.y - (r.chat.y + 10);
            if below > 0 {
                image(
                    &mut f,
                    0x0600114d,
                    rect(w - 309, r.chat.y + 10, 309, below),
                    true,
                    None,
                );
            }
        }
        append(&mut f, self.vitals.frame(c), 0, 0, "vitals:");
        append(
            &mut f,
            Radar { size: radar.w }.frame(c),
            radar.x,
            radar.y,
            "radar:",
        );
        // The radar takes clicks on its blips, over the world as well as over the side column.
        f.button("radar-hit", radar, "", true).paint = false;
        // The player's coordinates under the radar dial, in the final client's form
        // ("42.2N, 33.8E"), with the "coordinates below radar" character option.
        if c.classic.option_words[0] & 0x40_0000 != 0 {
            if let Some(coords) = c.game.player_coords() {
                let text = dereth_presentation::coordinates::update_coordinates(coords).combined;
                f.text_box(
                    rect(radar.x - 20, radar.y + radar.h - 1, radar.w + 40, 14),
                    text,
                    "14-6",
                    0xffd2_d2c8,
                    TextAlign::Center,
                    false,
                    None,
                );
            }
        }
        append(&mut f, self.chat.frame(c), r.chat.x, r.chat.y, "chat:");
        // The chat's top bar takes the pointer, so a drag on it can change the chat's height.
        f.button(
            "chat-divider",
            rect(r.chat.x, r.chat.y, r.chat.w - 16, 10),
            "",
            true,
        )
        .paint = false;
        let x = r.toolbar.x;
        let y = r.toolbar.y;
        image(&mut f, 0x0600112b, rect(x + 55, y, 7, 27), false, None);
        let quests = quest_page(c.game);
        if quests.is_none() {
            image(&mut f, 0x0600112c, rect(x + 237, y, 10, 27), false, None);
        }
        image(
            &mut f,
            0x0600120f,
            rect(x + 295, y + 58, 14, 32),
            false,
            None,
        );
        append(&mut f, self.shortcuts.frame(c), x, y + 58, "shortcut:");
        for (id, dx, w, [normal, selected, pressed]) in toolbar_buttons(quests.is_some()) {
            let active = button_pages(id).contains(&c.classic.active_right.as_str());
            button(
                &mut f,
                id,
                rect(x + dx, y, w, 27),
                [if active { selected } else { normal }, pressed, normal],
            );
        }
        button(
            &mut f,
            "use",
            rect(x + 55, y + 27, 23, 31),
            [0x06001129, 0x0600112a, 0x0600120e],
        );
        button(
            &mut f,
            "examine",
            rect(x + 224, y + 27, 22, 31),
            [0x06001127, 0x06001128, 0x06001127],
        );
        selected_object(
            &mut f,
            c,
            x,
            y,
            self.split_draft
                .as_ref()
                .filter(|(id, _)| Some(*id) == c.game.selected_object())
                .map(|(_, text)| text.as_str()),
        );
        let combat_art = match c.game.combat_mode() {
            2 => [0x060010f5, 0x0600119d, 0x060010f5],
            4 => [0x060010f6, 0x0600119e, 0x060010f6],
            8 => [0x06001959, 0x0600195a, 0x06001959],
            _ => [0x060010f7, 0x0600119c, 0x060010f7],
        };
        button(&mut f, "combat", rect(x, y, 55, 58), combat_art);
        button(
            &mut f,
            "inventory",
            rect(x + 246, y, 63, 58),
            [
                if c.classic.active_right == "inventory" {
                    0x060010ee
                } else {
                    0x0600113d
                },
                0x060010ee,
                0x060010ee,
            ],
        );
        f
    }
    fn event(&mut self, e: ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        if matches!(e, ControlEvent::Tick) {
            self.vitals.event(ControlEvent::Tick, c);
        }
        match &e {
            ControlEvent::ChatEntry(update) => {
                self.chat.text.clone_from(&update.text);
                return vec![];
            }
            // The toolbar's backpack takes an object dropped on it into the backpack.
            ControlEvent::Drop {
                id,
                payload: DragPayload::Object(item),
                ..
            } if id == "inventory" => {
                return vec![PanelAction::Game(UiRequest::DragDrop {
                    item: *item,
                    target: dereth_client_contract::view::DropTarget::BackpackButton,
                })];
            }
            ControlEvent::Action(name)
                if name == "SelectionSplitStack" || name == "SplitSelected" =>
            {
                // The split-stack command focuses the stack count only while the
                // splitter is shown; its edit field exists only for a stack of more than one.
                return if c.game.selected_object().is_some() && stack_split(c).1 > 1 {
                    vec![PanelAction::Host(HostAction::FocusControl(
                        "hud:stack-count".into(),
                    ))]
                } else {
                    vec![]
                };
            }
            ControlEvent::Edit { id, text } if id == "stack-count" => {
                self.split_draft = c.game.selected_object().map(|o| (o, text.clone()));
                return vec![];
            }
            ControlEvent::Commit { id } | ControlEvent::Submit { id } if id == "stack-count" => {
                let Some((object, text)) = self.split_draft.take() else {
                    return vec![];
                };
                if Some(object) != c.game.selected_object() {
                    return vec![];
                }
                let (_, max) = stack_split(c);
                if max <= 0 {
                    return vec![];
                }
                if let Ok(split) = text.trim().parse::<i32>() {
                    if (1..=max).contains(&split) {
                        return vec![PanelAction::Game(UiRequest::StackSliderChanged {
                            split: split as u32,
                            max: max as u32,
                        })];
                    }
                }
                return vec![];
            }
            ControlEvent::Scroll { id, value } if id == "stack-slider" => {
                self.split_draft = None;
                let (_, max) = stack_split(c);
                if max <= 0 {
                    return vec![];
                }
                return vec![PanelAction::Game(UiRequest::StackSliderChanged {
                    split: (*value).clamp(1, max) as u32,
                    max: max as u32,
                })];
            }
            _ => {}
        }
        if let ControlEvent::Action(name) = &e {
            let panel = match name.as_str() {
                "ToggleInventoryPanel" => Some("inventory"),
                "ToggleOptionsPanel" => Some("options"),
                "ToggleAllegiancePanel" => Some("allegiance"),
                "ToggleFellowshipPanel" => Some("fellowship"),
                "TradePanel" => Some("trade-intro"),
                "ToggleSpellbookPanel" => Some("spellbook"),
                "ToggleSpellComponentsPanel" => Some("components"),
                "ToggleCharacterTitlePanel" => Some("titles"),
                "ToggleContractsPanel" => Some("contracts"),
                "ToggleJournalPanel" => Some("journal"),
                // No final-client action: the classic command arrives under its own name.
                "SpellResearchPanel" => Some("spell-research"),
                "ToggleHousePanel" => Some("house"),
                "ToggleAttributesPanel" => Some("character-stats"),
                "ToggleCharacterInfoPanel" => Some("character-info"),
                "ToggleSkillsPanel" => Some("skills"),
                "ToggleMapPanel" => Some("map"),
                "ToggleCharacterOptionsPanel" => Some("character-options"),
                "ToggleConfigOptionsPanel" => Some("sound-graphics"),
                "TogglePositiveEffectsPanel" => Some("beneficial-effects"),
                "ToggleNegativeEffectsPanel" => Some("harmful-effects"),
                "ToggleLinkStatusPanel" => Some("link-status"),
                "ToggleVitaePanel" => Some("vitae"),
                // The final client's windows this interface has under other names: the social
                // window and its friends page, the journal's pages, the magic and character
                // windows, the map, the Options window and the two forms.
                "ToggleSocialPanel" => Some("social"),
                "ToggleFriendsPanel" => Some("friends"),
                "TogglePageListPanel" => quest_page(c.game),
                "ToggleSpellManagementPanel" => Some("spellbook"),
                "ToggleSkillManagementPanel" => Some("character-stats"),
                "ToggleWorldPanel" => Some("map"),
                "ToggleGameplayOptionsPanel" => Some("options"),
                "ToggleAbusePanel" => Some("abuse"),
                "ToggleUrgentAssistancePanel" => Some("urgent-assistance"),
                _ => None,
            };
            if let Some(panel) = panel {
                // The research page's key does nothing on a world without spell research.
                if panel == "spell-research" && !super::game::research_on(c.game) {
                    return vec![];
                }
                return vec![PanelAction::Toggle(panel.into())];
            }
            if name == "ToggleHelp" {
                return vec![PanelAction::Host(HostAction::LegacyHelp(51))];
            }
            // The keyboard window is the character screen's: going there leaves the world, so
            // it asks first, as the Options window's Configure Keyboard does.
            if name == "ToggleKeyboardPanel" {
                return vec![crate::panels::configure_keyboard()];
            }
            if name == "LOGOUT" {
                return vec![PanelAction::Confirm {
                    id: "logout".into(),
                    text: "\nThis will exit your character from the game world.\n\nAre you \
                           sure?\n\n(Default is No)"
                        .into(),
                    accept: vec![PanelAction::Game(UiRequest::EndCharacterSession {
                        ask: false,
                    })],
                }];
            }
            // A shortcut's select key selects what the shortcut holds.
            if let Some(slot) = name
                .strip_prefix("SelectQuickSlot_")
                .and_then(|n| n.parse::<u32>().ok())
                .filter(|n| (1..=9).contains(n))
            {
                return c
                    .game
                    .shortcut(slot - 1)
                    .map(|object| vec![PanelAction::Game(UiRequest::Select(object))])
                    .unwrap_or_default();
            }
            if name == "CombatToggleCombat" {
                return vec![PanelAction::Host(HostAction::CombatMode(
                    if c.game.combat_mode() == 1 { 0 } else { 1 },
                ))];
            }
            return self.chat.event(e, c);
        }
        if let Some(child) = unprefix(e.clone(), "shortcut:") {
            return self.shortcuts.event(child, c);
        }
        if let Some(child) = unprefix(e.clone(), "chat:") {
            return self.chat.event(child, c);
        }
        if let Some(child) = unprefix(e.clone(), "vitals:") {
            return self.vitals.event(child, c);
        }
        if let ControlEvent::Pointer { x, y, pressed } = e {
            let r = radar_rect(
                self.width as i32,
                self.height as i32,
                c.classic.option_words[0] & 0x200000 != 0,
                !c.classic.active_right.is_empty(),
            );
            if x >= r.x && y >= r.y && x < r.x + r.w && y < r.y + r.h {
                return Radar { size: r.w }.event(
                    ControlEvent::Pointer {
                        x: x - r.x,
                        y: y - r.y,
                        pressed,
                    },
                    c,
                );
            }
        }
        if let ControlEvent::Activate(id) = e {
            match id.as_str() {
                "social" | "spellbook" | "character-stats" | "options" | "map" => {
                    let shown = c.classic.active_right.as_str();
                    if button_pages(&id).contains(&shown) {
                        return vec![PanelAction::Toggle(shown.into())];
                    }
                    return vec![PanelAction::Toggle(id)];
                }
                "inventory" => return vec![PanelAction::Toggle(id)],
                // The journal's button closes its page or the contracts beside it when either is
                // shown, and otherwise opens its page.
                "journal" => {
                    let shown = c.classic.active_right.as_str();
                    return match quest_page(c.game) {
                        Some(_) if matches!(shown, "journal" | "contracts") => {
                            vec![PanelAction::Toggle(shown.into())]
                        }
                        Some(page) => vec![PanelAction::Open(page.into())],
                        None => vec![],
                    };
                }
                "combat" => {
                    return vec![PanelAction::Host(HostAction::CombatMode(
                        if c.game.combat_mode() == 1 { 0 } else { 1 },
                    ))];
                }
                "use" | "examine" => {
                    return vec![PanelAction::Game(
                        match (id.as_str(), c.game.selected_object()) {
                            ("use", Some(o)) => UiRequest::Use(o),
                            ("examine", Some(o)) => UiRequest::Examine(o),
                            ("use", None) => UiRequest::SetTargetMode(TargetMode::Use),
                            _ => UiRequest::SetTargetMode(TargetMode::Examine),
                        },
                    )];
                }
                _ => {}
            }
        }
        vec![]
    }
}

/// The connection lamp: it samples the link after more than 4 seconds and blinks at intervals
/// of at least 0.75 seconds.
#[derive(Debug)]
struct LinkLamp {
    checked: f64,
    changed: f64,
    state: u8,
    lost: bool,
}
impl Default for LinkLamp {
    fn default() -> Self {
        Self {
            checked: f64::NEG_INFINITY,
            changed: 0.0,
            state: 1,
            lost: false,
        }
    }
}
impl LinkLamp {
    fn tick(&mut self, now: f64, seconds: Option<f64>) {
        if now - self.checked > 4.0 {
            self.checked = now;
            let state = match seconds {
                Some(t) if t <= 5.0 => 1,
                Some(t) if t <= 20.0 => 2,
                Some(t) if t <= 40.0 => 3,
                _ => 4,
            };
            if state != self.state {
                self.state = state;
                if state == 3 {
                    self.changed = now;
                    self.lost = true;
                }
            }
        }
        if self.state == 3 && now - self.changed >= 0.75 {
            self.lost = !self.lost;
            self.changed = now;
        }
    }
    fn image(&self) -> u32 {
        match self.state {
            1 => 0x06001360,
            2 => 0x06001362,
            3 if !self.lost => 0x06001362,
            _ => 0x06001361,
        }
    }
}

/// The top of a vital bar's number line: its capitals then stand on the bar's fourteenth pixel row.
const VITAL_TEXT_TOP: i32 = 1;
#[derive(Debug)]
struct Vitals {
    width: u32,
    numeric: bool,
    link: LinkLamp,
}
impl Vitals {
    fn indicators(&self, c: &Context<'_>) -> Vec<(&'static str, u32)> {
        // Only the lit lamps show, in this order: beneficial spells (a cyan star), harmful spells
        // (a broken red star), then vitae (a sunburst on red).
        let (good, bad) = c.game.enchantment_counts();
        let mut v = vec![];
        if good > 0 {
            v.push(("beneficial-effects", 0x06001111));
        }
        if bad > 0 {
            v.push(("harmful-effects", 0x06002629));
        }
        if c.game.vitae().is_some_and(|x| x < 1.0) {
            v.push(("vitae", 0x0600110c));
        }
        if c.game.portal_storm_level() > 0.0 {
            v.push(("portal-storm", 0x060016d1));
        }
        if c.game.minigame().is_some_and(|g| g.game.0 != 0) {
            v.push(("game-center", 0x06002817));
        }
        v
    }
}
impl Panel for Vitals {
    fn id(&self) -> &'static str {
        "vitals"
    }
    fn resize(&mut self, w: u32, _: u32) {
        self.width = w.max(320);
    }
    fn frame(&self, c: &Context<'_>) -> PanelFrame {
        let w = self.width as i32;
        let mut f = PanelFrame::new(self.width, 28);
        let indicators = self.indicators(c);
        let n = i32_from(indicators.len()) + 2;
        let available = w - 20 * n;
        let health = (available - 129) / 3;
        let stamina_x = health + 32;
        let stamina = (available - 88 - stamina_x - 9) / 2;
        let mana_x = health + 41 + stamina;
        let mana = available - 88 - mana_x + 1;
        for (did, x, width) in [
            (0x0600112d, 0, 24),
            (0x0600112e, health + 24, 8),
            (0x0600112f, stamina_x + stamina, 9),
            (0x06001130, mana_x + mana, 27),
        ] {
            image(&mut f, did, rect(x, 0, width, 28), false, None);
        }
        for (vital, title, x, width, left, right, bg, fg, icons) in [
            (
                Vital::Health,
                "Health",
                24,
                health,
                13,
                10,
                [0x06001141, 0x06001140, 0x0600113f],
                [0x06001131, 0x06001132, 0x06001133],
                [(0x0600131a, 104), (0x0600131e, 30)],
            ),
            (
                Vital::Stamina,
                "Stamina",
                stamina_x,
                stamina,
                14,
                11,
                [0x06001147, 0x06001146, 0x06001145],
                [0x06001137, 0x06001138, 0x06001139],
                [(0x06001319, 85), (0x0600131c, 85)],
            ),
            (
                Vital::Mana,
                "Mana",
                mana_x,
                mana,
                12,
                14,
                [0x06001144, 0x06001143, 0x06001142],
                [0x06001134, 0x06001135, 0x06001136],
                [(0x0600131b, 122), (0x0600131d, 122)],
            ),
        ] {
            let value = c.game.player().and_then(|p| c.game.vital(p, vital));
            let ratio = value
                .filter(|v| v.1 != 0)
                .map_or(0.0, |(cur, max)| (cur as f64 / max as f64).clamp(0.0, 1.0));
            for (pieces, clip, (icon, icon_width)) in [
                (bg, None, icons[0]),
                (
                    fg,
                    Some(rect(x, 0, to_i32_f64(width as f64 * ratio), 28)),
                    icons[1],
                ),
            ] {
                image(&mut f, pieces[0], rect(x, 0, left, 28), false, clip);
                image(
                    &mut f,
                    pieces[1],
                    rect(x + left, 0, width - left - right, 28),
                    true,
                    clip,
                );
                image(
                    &mut f,
                    pieces[2],
                    rect(x + width - right, 0, right, 28),
                    false,
                    clip,
                );
                // The icon's black is the bar showing through it: the empty bar's grey icon,
                // the full bar's coloured one cut to the vital's level over it.
                if !self.numeric {
                    keyed_image(
                        &mut f,
                        icon,
                        rect(x + width / 2 - icon_width / 2, 0, icon_width, 28),
                        clip,
                    );
                }
            }
            f.button(format!("vital:{title}"), rect(x, 0, width, 28), "", true)
                .paint = false;
            if let Some((cur, max)) = value.filter(|_| self.numeric) {
                f.text_box(
                    // Centred across the bar and set high in it, inside the meter's 2-pixel inset:
                    // the capitals run from the bar's fifth pixel row to its fourteenth.
                    rect(x, VITAL_TEXT_TOP, width, 16),
                    format!("{title} {cur}/{max}"),
                    "16-7",
                    INK,
                    TextAlign::Center,
                    false,
                    None,
                );
            }
        }
        button(
            &mut f,
            "logout",
            rect(w - 28, 0, 28, 28),
            [0x06001932, 0x06001933, 0x06001932],
        );
        button(
            &mut f,
            "help",
            rect(w - 60, 0, 32, 28),
            [0x06001148, 0x0600114a, 0x06001148],
        );
        let link = self.link.image();
        button(&mut f, "link-status", rect(w - 80, 0, 20, 28), [link; 3]);
        let load = c.game.load().unwrap_or(0.0);
        let burden = if load >= 2.0 {
            0x0600135c
        } else if load >= 1.0 {
            0x0600110f
        } else {
            0x06001103
        };
        button(&mut f, "burden", rect(w - 100, 0, 20, 28), [burden; 3]);
        for (i, (id, did)) in indicators.into_iter().enumerate() {
            button(
                &mut f,
                id,
                rect(w - (i32_from(i) + 6) * 20, 0, 20, 28),
                [did; 3],
            );
        }
        f
    }
    fn event(&mut self, e: ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        match e {

            ControlEvent::Tick=>{self.link.tick(c.game.now(),c.game.link_status());vec![]}
            ControlEvent::Activate(id) if id.starts_with("vital:") => {self.numeric = !self.numeric;vec![]}

            ControlEvent::Activate(id) if id == "help" => vec![PanelAction::Host(HostAction::LegacyHelp(51))],
            ControlEvent::Activate(id)
                if matches!(
                    id.as_str(),
                    "inventory"
                        | "link-status"
                        | "beneficial-effects"
                        | "harmful-effects"
                        | "vitae"
                        | "game-center"
                ) =>
            {
                vec![PanelAction::Open(id)]
            }
            ControlEvent::Activate(id) if id == "logout" => vec![PanelAction::Confirm { id: "logout".into(), text: "\nThis will exit your character from the game world.\n\nAre you sure?\n\n(Default is No)".into(), accept: vec![PanelAction::Game(UiRequest::EndCharacterSession {ask:false})] }],
            ControlEvent::Activate(id) if id == "burden" => {
                vec![PanelAction::Open("character-info".into())]
            }
            _ => vec![],
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Blip {
    object: ObjectId,
    x: i32,
    y: i32,
    color: u32,
    shape: u8,
    bright: bool,
}
#[derive(Debug)]
struct Radar {
    size: i32,
}
impl Default for Radar {
    fn default() -> Self {
        Self { size: 120 }
    }
}
/// An object's radar colour index, which the selection indicator shares.
pub(crate) fn blip_color(e: &RadarEntry) -> u8 {
    if e.blip_color != 0 {
        return e.blip_color;
    }
    let bits = e.bitfield;
    if bits & 0x40000 != 0 {
        return 4;
    }
    if bits & 0x4000 != 0 {
        return 1;
    }
    if bits & 0x200 != 0 {
        return 8;
    }
    if bits & 0x10 != 0 && e.is_attackable && !e.is_player {
        return 2;
    }
    if e.is_player {
        if e.is_fellow || e.is_fellowship_leader {
            return 10;
        }
        if bits & 0x100000 != 0 && bits & 0x40 == 0 {
            return 9;
        }
        if e.is_pk {
            return 5;
        }
        if e.is_pk_lite {
            return 6;
        }
        if bits & 0x200000 != 0 {
            return 2;
        }
    }
    3
}
fn palette(index: u8, bright: bool) -> u32 {
    let [r, g, b]: [u32; 3] = match index {
        1 => [64, 169, 255],
        2 => [255, 170, 0],
        3 => [255, 255, 255],
        4 => [192, 100, 255],
        5 => [255, 64, 100],
        6 => [255, 169, 192],
        7 => [0, 128, 64],
        8 => [255, 255, 127],
        9 => [0, 255, 255],
        10 => [0, 255, 0],
        _ => [0, 0, 0],
    };
    let dim = |v| {
        if bright {
            v
        } else {
            u32::try_from(to_i32(v as f32 * 0.65)).unwrap_or(0)
        }
    };
    0xff000000 | dim(r) << 16 | dim(g) << 8 | dim(b)
}
impl Radar {
    fn blips(&self, c: &Context<'_>) -> Vec<Blip> {
        if c.game.radar_blank() {
            return vec![];
        }
        let range: f32 = if c.game.player_outside() { 75.0 } else { 25.0 };
        let radius = if self.size == 80 { 32 } else { 50 };
        let center = self.size / 2;
        c.game
            .radar_objects()
            .iter()
            .filter(|e| {
                !e.is_self && e.in_world && e.bitfield & 0x80 == 0 && matches!(e.radar_enum, 2..=4)
            })
            .filter_map(|e| {
                let (x, y, z) = e.player_space;
                if !x.is_finite()
                    || !y.is_finite()
                    || dereth_primitives::num::math::hypotf(x, y) >= range - 1.0
                {
                    return None;
                }
                let bright = z.abs() < 5.0;
                let shape = if !e.is_player {
                    1
                } else if e.is_fellow {
                    if e.is_fellowship_leader {
                        5
                    } else {
                        6
                    }
                } else if e.is_allegiance_member {
                    2
                } else if (e.is_pk && c.game.pk_status() == PkStatus::Pk)
                    || (e.is_pk_lite && c.game.pk_status() == PkStatus::PkLite)
                {
                    3
                } else {
                    1
                };
                Some(Blip {
                    object: e.id,
                    x: center + to_i32(x * radius as f32 / range),
                    y: center - to_i32(y * radius as f32 / range),
                    color: palette(blip_color(e), bright),
                    shape,
                    bright,
                })
            })
            .collect()
    }
}
fn dot(f: &mut PanelFrame, x: i32, y: i32, color: u32) {
    f.fill(rect(x, y, 1, 1), color);
}
fn circle(f: &mut PanelFrame, cx: i32, cy: i32, radius: i32, outline: u32, fill: u32) {
    // Filled by concentric midpoint outlines, including radius zero.
    for r in (0..=radius).rev() {
        let color = if r == radius { outline } else { fill };
        let mut x = 0;
        let mut y = r;
        let mut error = (5 - 4 * r) / 4;
        let points = |f: &mut PanelFrame, x: i32, y: i32| {
            for (dx, dy) in [
                (x, y),
                (-x, y),
                (x, -y),
                (-x, -y),
                (y, x),
                (-y, x),
                (y, -x),
                (-y, -x),
            ] {
                dot(f, cx + dx, cy + dy, color);
            }
        };
        points(f, x, y);
        while x < y {
            x += 1;
            if error < 0 {
                error += 1 + 2 * x;
            } else {
                y -= 1;
                error += 1 + 2 * (x - y);
            }
            if x <= y {
                points(f, x, y);
            }
        }
    }
}
fn blip(f: &mut PanelFrame, b: Blip, selected: bool) {
    if selected {
        circle(f, b.x, b.y, 3, b.color, 0xff080808);
    }
    for y in -2_i32..=2 {
        for x in -2_i32..=2 {
            let on = match b.shape {
                1 => x * x + y * y <= 1,
                2 => x.abs() <= 1 && y.abs() <= 1 && (x.abs() == 1 || y.abs() == 1),
                3 => x.abs() <= 1 && y.abs() == x.abs(),
                4 => x == 0 || y == 0,
                5 => (y == 1 && x.abs() <= 2) || (y == 0 && x.abs() <= 1) || (y == -1 && x == 0),
                6 => (y == -1 && x.abs() <= 2) || (y == 0 && x.abs() <= 1) || (y == 1 && x == 0),
                7 => {
                    (x.abs() <= 1 && y.abs() <= 1 && (x.abs() == 1 || y.abs() == 1))
                        || (x.abs() == 2 && y.abs() == 2)
                }
                _ => false,
            };
            if on {
                dot(f, b.x + x, b.y + y, b.color);
            }
        }
    }
}
impl Panel for Radar {
    fn id(&self) -> &'static str {
        "radar"
    }
    fn resize(&mut self, width: u32, _: u32) {
        self.size = if width < 351 { 80 } else { 120 };
    }
    fn frame(&self, c: &Context<'_>) -> PanelFrame {
        let mut f = PanelFrame::new(self.size as u32, self.size as u32);
        // The dial is drawn with black as its colour key, so its square corners show the
        // backdrop behind the radar.
        f.image(
            &format!(
                "{:08X}",
                if self.size == 80 {
                    0x06001214
                } else {
                    0x060011dc
                }
            ),
            rect(0, 0, self.size, self.size),
            false,
            true,
        );
        for b in self.blips(c) {
            blip(&mut f, b, c.game.selected_object() == Some(b.object));
        }
        let center = self.size / 2;
        let small = self.size == 80;
        let (iw, ih, orbit) = if small { (8, 7, 36.0) } else { (10, 9, 55.0) };
        let heading = c.game.player_heading().to_radians();
        for (i, did) in (if small {
            [0x06001215, 0x06001939, 0x0600193b, 0x0600193d]
        } else {
            [0x060011fb, 0x06001938, 0x0600193a, 0x0600193c]
        })
        .into_iter()
        .enumerate()
        {
            let angle = -heading + i as f32 * std::f32::consts::FRAC_PI_2;
            f.image(
                &format!("{did:08X}"),
                rect(
                    center - iw / 2 + to_i32(dereth_primitives::num::math::sinf(angle) * orbit),
                    center - ih / 2 - to_i32(dereth_primitives::num::math::cosf(angle) * orbit),
                    iw,
                    ih,
                ),
                false,
                true,
            );
        }
        for d in -2..=2 {
            dot(&mut f, center + d, center, 0xff00ff00);
            dot(&mut f, center, center + d, 0xff00ff00);
        }
        f
    }
    fn event(&mut self, e: ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        if let ControlEvent::Pointer {
            x,
            y,
            pressed: true,
        } = e
        {
            let mut nearest: Option<(i32, Blip)> = None;
            for b in self.blips(c) {
                let d = (b.x - x).pow(2) + (b.y - y).pow(2);
                if d < 100
                    && nearest.is_none_or(|(best, prior)| {
                        d < best || (d == best && b.bright && !prior.bright)
                    })
                {
                    nearest = Some((d, b));
                }
            }
            // A blip is selected; with a targeted use armed it is the target instead.
            if let Some((_, b)) = nearest {
                return vec![PanelAction::Game(if c.classic.cursor_mode == 4 {
                    UiRequest::ExecuteTargetItem(b.object)
                } else {
                    UiRequest::Select(b.object)
                })];
            }
        }
        vec![]
    }
}

/// The chat window's colour for each text type.
fn chat_color(ty: u32) -> u32 {
    0xff000000
        | dereth_client_contract::chat::colors::interface_color(
            ty,
            dereth_client_contract::options::interface::Interface::Classic,
        )
        .hex
}

const CHAT_FOCUSES: [u32; 14] = [0, 2, 1, 3, 5, 4, 6, 7, 8, 9, 10, 11, 12, 13];

#[derive(Debug)]
struct Chat {
    width: u32,
    height: u32,
    text: String,
    offset: usize,
    destination: usize,

    /// The selected part of the log, from where the press landed to where the pointer is.
    selection: Option<(chat_log::Place, chat_log::Place)>,
    /// Whether a press in the log is still selecting.
    selecting: bool,
    /// The selected text, ready for the copy key.
    selected: Option<String>,
    /// When and where the last press in the log landed, to tell a double click.
    last_press: Option<(std::time::Instant, i32, i32)>,
}
impl Default for Chat {
    fn default() -> Self {
        Self {
            width: 491,
            height: 101,
            text: String::new(),
            offset: 0,
            destination: 2,

            selection: None,
            selecting: false,
            selected: None,
            last_press: None,
        }
    }
}
/// The chat log's font.
const CHAT_FONT: &str = "15-6";
fn chat_measure(text: &str) -> i32 {
    crate::renderer::measure_text_width(CHAT_FONT, text).unwrap_or(0)
}
fn chat_lines<'a>(c: &'a Context<'_>) -> Vec<&'a str> {
    c.classic.chat.iter().map(|(_, s)| s.as_str()).collect()
}
impl Chat {
    fn entry_action(
        &self,
        action: dereth_client_contract::chat::entry::EntryAction,
    ) -> Vec<PanelAction> {
        vec![PanelAction::Game(UiRequest::ChatEntry {
            window: dereth_client_contract::chat::interface::window::MAIN,
            text: self.text.clone(),
            action,
        })]
    }

    /// The log's rows, the row height, its full height, the visible height and how far it is
    /// scrolled.
    fn log(&self, c: &Context<'_>) -> (Vec<chat_log::Row>, i32, i32, i32, i32) {
        let row_height = crate::renderer::font_line_height(CHAT_FONT).unwrap_or(15);
        let (rows, total) = chat_log::rows(
            &chat_lines(c),
            self.width as i32 - 20,
            row_height,
            15,
            &chat_measure,
        );
        let body = self.height as i32 - 27;
        let max_scroll = (total - body).max(0);
        let scroll = max_scroll - i32_from(self.offset.min(max_scroll as usize));
        (rows, row_height, total, body, scroll)
    }
    /// The place in the log under a point in the window; outside the log only when `clamp`.
    fn log_place(&self, c: &Context<'_>, x: i32, y: i32, clamp: bool) -> Option<chat_log::Place> {
        let (rows, row_height, _, body, scroll) = self.log(c);
        let inside = x >= 0 && x < self.width as i32 - 16 && y >= 10 && y < body + 10;
        if !inside && !clamp {
            return None;
        }
        let y = (y - 10).clamp(0, body - 1) + scroll;
        chat_log::place_at(&chat_lines(c), &rows, row_height, x - 2, y, &chat_measure)
    }
    fn select(&mut self, c: &Context<'_>, selection: Option<(chat_log::Place, chat_log::Place)>) {
        self.selection = selection.filter(|(a, b)| a != b);
        self.selected = self.selection.map(|s| chat_log::text(&chat_lines(c), s));
    }
    fn destination(&self, c: &Context<'_>) -> usize {
        c.classic
            .chat_focus
            .map_or(self.destination, |(focus, _)| match focus {
                2 => 1,
                3 => 3,
                4 => 5,
                5 => 4,
                6 => 6,
                7..=13 => usize::from(focus),
                _ => 2,
            })
    }
    fn target(c: &Context<'_>) -> Option<(ObjectId, String)> {
        if c.classic.chat_focus.is_some() {
            return c.classic.chat_target.clone();
        }
        c.game
            .selected_object()
            .map(|id| (id, c.game.name(id).unwrap_or("").to_owned()))
    }
    fn enabled(c: &Context<'_>) -> Vec<bool> {
        let target = Self::target(c).is_some();
        let enabled = c
            .classic
            .chat_focus
            .map_or([true; 14], |(_, enabled)| enabled);
        CHAT_FOCUSES
            .iter()
            .map(|focus| {
                if *focus == 0 {
                    target
                } else {
                    enabled[*focus as usize] && (*focus != 2 || target)
                }
            })
            .collect()
    }
}
impl Panel for Chat {
    fn id(&self) -> &'static str {
        "chat"
    }
    fn selected_text(&self) -> Option<String> {
        self.selected.clone()
    }
    /// A press in the log selects from there to where the button comes up; a double click
    /// selects a word.
    fn input(
        &mut self,
        input: &crate::widgets::Input,
        c: &Context<'_>,
    ) -> Option<Vec<PanelAction>> {
        use crate::widgets::Input;
        match *input {
            Input::PointerDown { x, y } => {
                let place = self.log_place(c, x, y, false)?;
                let double = self.last_press.is_some_and(|(t, px, py)| {
                    (x - px).abs() + (y - py).abs() < 4 && t.elapsed().as_millis() < 500
                });
                self.last_press = (!double).then(|| (std::time::Instant::now(), x, y));
                if double {
                    let word = chat_log::word(&chat_lines(c), place);
                    self.select(c, Some(word));
                    self.selecting = false;
                } else {
                    self.selection = Some((place, place));
                    self.selected = None;
                    self.selecting = true;
                }
                Some(vec![])
            }
            Input::PointerMove { x, y } if self.selecting => {
                let place = self.log_place(c, x, y, true)?;
                let anchor = self.selection.map_or(place, |(a, _)| a);
                self.selection = Some((anchor, place));
                Some(vec![])
            }
            Input::PointerUp { x, y } if self.selecting => {
                self.selecting = false;
                if let Some(place) = self.log_place(c, x, y, true) {
                    let anchor = self.selection.map_or(place, |(a, _)| a);
                    self.select(c, Some((anchor, place)));
                }
                Some(vec![])
            }
            _ => None,
        }
    }
    fn resize(&mut self, w: u32, h: u32) {
        self.width = w;
        self.height = h.max(44);
    }
    fn frame(&self, c: &Context<'_>) -> PanelFrame {
        let w = self.width as i32;
        let h = self.height as i32;
        let mut f = PanelFrame::new(self.width, self.height);
        let body = h - 27;
        image(&mut f, 0x06001125, rect(0, 0, w - 16, 10), true, None);
        image(&mut f, 0x06001115, rect(0, 10, w - 16, body), true, None);
        // The log takes the pointer for selecting text; it paints nothing of its own here.
        let log = f.button("log", rect(0, 10, w - 16, body), "", true);
        log.paint = false;
        log.silent = true;
        let (rows, row_height, total, _, scroll) = self.log(c);
        let max_scroll = (total - body).max(0);
        let lines = chat_lines(c);
        let clip = Some([0, 10, w - 16, body + 10]);
        // The selection is marked behind its text.
        if let Some(selection) = self.selection.filter(|_| !lines.is_empty()) {
            for (row, x0, x1) in chat_log::highlights(&lines, &rows, selection, &chat_measure) {
                let area = rect(2 + x0, row.y - scroll + 10, x1 - x0, row_height);
                if let Some(area) = area.intersect(rect(0, 10, w - 16, body)) {
                    f.fill(area, 0xff27_4657);
                }
            }
        }
        for row in &rows {
            let y = row.y - scroll;
            if y + row_height > 0 && y < body {
                f.text_box(
                    rect(2, y + 10, w - 20, row_height),
                    &lines[row.line][row.start..row.end],
                    CHAT_FONT,
                    chat_color(c.classic.chat[row.line].0),
                    TextAlign::Left,
                    false,
                    clip,
                );
            }
        }
        f.control(
            "scroll",
            rect(w - 16, 0, 16, body + 10),
            ControlKind::ScrollBar {
                min: 0,
                max: max_scroll,
                value: scroll,
                page: body,
                step: 15,
                vertical: true,
                arrow_size: 16,
                thumb_size: 16,
            },
            true,
        );
        let mut destinations = [
            "Squelch (ignore) Selected",
            "Tell to Selected",
            "Chat to All",
            "Tell to Fellows",
            "Tell to Monarch",
            "Tell to Patron",
            "Tell to Vassals",
            "Tell to Allegiance",
            "Tell to General",
            "Tell to Trade",
            "Tell to LFG",
            "Tell to Roleplay",
            "Tell to Society",
            "Tell to Olthoi",
        ]
        .map(String::from)
        .to_vec();
        if let Some((_, name)) = Self::target(c) {
            destinations[0] = format!("Squelch (ignore) {name}");
            destinations[1] = format!("Tell to {name}");
        }
        let destination = f.control(
            "destination",
            rect(0, h - 17, 48, 17),
            ControlKind::Choice {
                options: destinations,
                selected: self.destination(c),
            },
            true,
        );
        destination.paint = false;
        destination.chat_popup = true;
        destination.choice_enabled = Some(Self::enabled(c));
        image(&mut f, 0x060010ed, rect(0, h - 17, 46, 17), false, None);
        f.text_box(
            rect(2, h - 15, 44, 13),
            [
                "Chat", "Tell", "Chat", "Fell", "Mon", "Patr", "Vas", "Alleg", "Gen", "Trade",
                "LFG", "Role", "Soc", "Olthoi",
            ][self.destination(c)],
            "15-6",
            INK,
            TextAlign::Left,
            false,
            Some([0, h - 17, 46, h]),
        );
        let input = f.edit(
            "input",
            rect(47, h - 17, (w - 94).max(0), 17),
            &self.text,
            180,
            false,
            true,
        );
        input.font = "15-6".into();
        input.smooth_scroll = true;
        input.background = None;
        input.images = Some(["0600113A", "060011AB", "0600113A"].map(String::from));
        button(
            &mut f,
            "send",
            rect(w - 46, h - 17, 46, 17),
            [0x06001915, 0x06001916, 0x06001934],
        );
        if let Some(control) = f.controls.iter_mut().find(|c| c.id == "send") {
            control.kind = ControlKind::Button {
                caption: "Send".into(),
            };
            control.font = "15-6".into();
        }
        f
    }
    fn event(&mut self, e: ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        match e {
            ControlEvent::Edit { id, text } if id == "input" => {
                self.text = text;
                return self.entry_action(dereth_client_contract::chat::entry::EntryAction::Draft);
            }
            ControlEvent::Scroll { id, value } if id == "scroll" => {
                let (_, _, total, _, _) = self.log(c);
                self.offset = ((total - (self.height as i32 - 27)).max(0) - value).max(0) as usize;
            }
            ControlEvent::Action(name) => match name.as_str() {
                "ExpandChatAlias" => {
                    return self.entry_action(
                        dereth_client_contract::chat::entry::EntryAction::ExpandAlias,
                    )
                }
                "Reply" | "MonarchReply" | "PatronReply" => {
                    use dereth_client_contract::chat::entry::{EntryAction, ReplyTarget};
                    let target = match name.as_str() {
                        "Reply" => ReplyTarget::LastTeller,
                        "MonarchReply" => ReplyTarget::Monarch,
                        _ => ReplyTarget::Patron,
                    };
                    return self.entry_action(EntryAction::Reply {
                        target,
                        prefix: "@tell ".into(),
                    });
                }
                "IssueSlashCommand" | "START_COMMAND" => {
                    self.text = "/".into();
                    let mut actions =
                        self.entry_action(dereth_client_contract::chat::entry::EntryAction::Draft);
                    actions.push(PanelAction::Host(HostAction::FocusControl(
                        "chat:input".into(),
                    )));
                    return actions;
                }
                // A tell to the selected character begun in the entry.
                "TellSelected" => {
                    let Some(name) = c
                        .game
                        .selected_object()
                        .filter(|id| Some(*id) != c.game.player())
                        .and_then(|id| c.game.name(id))
                        .filter(|n| !n.is_empty())
                    else {
                        return vec![];
                    };
                    return self.entry_action(
                        dereth_client_contract::chat::entry::EntryAction::StartTell {
                            name: name.to_owned(),
                        },
                    );
                }
                "EnterChat" | "ChatMode" | "Chat" | "EnterChatMode" | "ToggleChatEntry" => {
                    return vec![PanelAction::Host(HostAction::FocusControl(
                        "chat:input".into(),
                    ))];
                }
                "RecallLastMessage" => {
                    return self
                        .entry_action(dereth_client_contract::chat::entry::EntryAction::RecallLast)
                }
                "PreviousMessage" => {
                    return self
                        .entry_action(dereth_client_contract::chat::entry::EntryAction::Previous)
                }
                "NextMessage" => {
                    return self
                        .entry_action(dereth_client_contract::chat::entry::EntryAction::Next)
                }
                _ => {}
            },
            ControlEvent::Select { id, index }
                if id == "destination" && index < CHAT_FOCUSES.len() =>
            {
                if !Self::enabled(c)[index] {
                    return vec![];
                }
                if index == 0 {
                    if let Some((object, _)) = Self::target(c) {
                        return vec![PanelAction::Game(UiRequest::ToggleCharacterSquelch(object))];
                    }
                    return vec![];
                }
                self.destination = index;
                let mut actions = vec![PanelAction::Game(UiRequest::SetTalkFocus {
                    focus: CHAT_FOCUSES[index],
                })];
                if index == 1 {
                    if let Some((_, name)) = Self::target(c) {
                        actions.push(PanelAction::Game(UiRequest::StartTell { name }));
                    }
                }
                return actions;
            }
            ControlEvent::Activate(id) | ControlEvent::Submit { id }
                if (id == "send" || id == "input") && !self.text.is_empty() =>
            {
                self.offset = 0;
                let mut actions = vec![PanelAction::Game(UiRequest::ChatLine {
                    text: std::mem::take(&mut self.text),
                    window: dereth_client_contract::chat::interface::window::MAIN,
                })];
                // The entry keeps the caret after a send only with "stay in chat mode"; otherwise
                // sending leaves the chat.
                actions.push(PanelAction::Host(HostAction::FocusControl(
                    if c.game
                        .player_option(dereth_client_contract::view::PlayerOption::StayInChatMode)
                    {
                        "chat:input".into()
                    } else {
                        String::new()
                    },
                )));
                return actions;
            }
            // Enter on an empty line leaves the chat.
            ControlEvent::Submit { id } if id == "input" => {
                return vec![PanelAction::Host(HostAction::FocusControl(String::new()))];
            }
            _ => {}
        }
        vec![]
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic panel adapter; no retail behaviour claim).
    use super::*;
    #[test]
    fn the_chat_log_is_the_interfaces_where_no_control_lies_and_the_world_view_is_not() {
        let mut hud = make("hud").expect("the interface");
        hud.resize(800, 600);
        let chat = regions(800, 600).chat;
        assert!(
            hud.claims(chat.x + 20, chat.y + chat.h / 2),
            "the chat log's text"
        );
        assert!(!hud.claims(200, 200), "the 3D view");
    }
    #[derive(Debug, Default)]
    struct World {
        rows: Vec<RadarEntry>,
        selected: Option<ObjectId>,
        outside: bool,
        blank: bool,
        combat: u32,
        split: i32,
        max_split: i32,
        meters: (Option<f32>, Option<f32>),
        mini: Option<dereth_client_contract::view::MiniGameView>,
    }
    impl GameView for World {
        fn minigame(&self) -> Option<dereth_client_contract::view::MiniGameView> {
            self.mini
        }
        fn split_size(&self) -> i32 {
            self.split
        }
        fn max_split_size(&self) -> i32 {
            self.max_split
        }
        fn selected_meters(&self) -> (Option<f32>, Option<f32>) {
            self.meters
        }
        fn radar_objects(&self) -> &[RadarEntry] {
            &self.rows
        }
        fn selected_object(&self) -> Option<ObjectId> {
            self.selected
        }
        fn player_outside(&self) -> bool {
            self.outside
        }
        fn radar_blank(&self) -> bool {
            self.blank
        }
        fn combat_mode(&self) -> u32 {
            self.combat
        }
        fn player(&self) -> Option<ObjectId> {
            Some(ObjectId(1))
        }
        fn name(&self, _: ObjectId) -> Option<&str> {
            Some("Aerin")
        }
        fn vital(&self, _: ObjectId, vital: Vital) -> Option<(u32, u32)> {
            Some(match vital {
                Vital::Health => (50, 100),
                Vital::Stamina => (1, 0),
                Vital::Mana => (0, 100),
            })
        }
    }
    fn context<'a>(
        g: &'a dyn GameView,
        state: &'a ClassicState,
        p: &'a PregameView,
        k: &'a KeyboardState,
        s: &'a ClassicSettings,
    ) -> Context<'a> {
        Context {
            game: g,
            pregame: p,
            keyboard: k,
            settings: s,
            map_teleport_allowed: false,
            classic: state,
        }
    }
    #[test]
    fn the_journal_button_fits_between_the_character_and_map_buttons_only_where_it_exists() {
        let five = toolbar_buttons(false);
        assert_eq!(five.len(), 5);
        assert_eq!((five[0].1, five[4].1 + five[4].2), (61, 237));
        let six = toolbar_buttons(true);
        let ids: Vec<_> = six.iter().map(|b| b.0).collect();
        assert_eq!(
            ids,
            [
                "social",
                "spellbook",
                "character-stats",
                "journal",
                "map",
                "options"
            ]
        );
        // Six narrowed buttons fill the row from the left end piece to the backpack, side by side.
        assert_eq!((six[0].1, six[5].1 + six[5].2), (62, 246));
        assert!(six.windows(2).all(|p| p[0].1 + p[0].2 == p[1].1));
        assert!(six
            .iter()
            .all(|b| b.3.iter().all(|a| crate::composed::is_composed(*a))));
        // A world with neither the journal nor contracts has no such button.
        let mut era = dereth_client_contract::EraView::default();
        era.era = dereth_primitives::era::EraId::Infiltration;
        era.era_announced = true;
        #[derive(Debug)]
        struct Era(dereth_client_contract::EraView);
        impl GameView for Era {
            fn era(&self) -> Option<&dereth_client_contract::EraView> {
                Some(&self.0)
            }
        }
        assert_eq!(quest_page(&Era(era)), None);
        assert_eq!(quest_page(&World::default()), Some("journal"));
    }
    #[test]
    fn the_journal_button_opens_the_journal_and_closes_the_page_it_shows() {
        let g = World::default();
        let mut state = ClassicState::default();
        let (p, k, s) = Default::default();
        let mut hud = Hud::default();
        assert_eq!(
            hud.event(
                ControlEvent::Activate("journal".into()),
                &context(&g, &state, &p, &k, &s)
            ),
            vec![PanelAction::Open("journal".into())]
        );
        state.active_right = "contracts".into();
        assert_eq!(
            hud.event(
                ControlEvent::Activate("journal".into()),
                &context(&g, &state, &p, &k, &s)
            ),
            vec![PanelAction::Toggle("contracts".into())]
        );
    }
    fn with_context(g: &World, f: impl FnOnce(&Context<'_>)) {
        f(&context(
            g,
            &ClassicState::default(),
            &PregameView::default(),
            &KeyboardState::default(),
            &ClassicSettings::default(),
        ));
    }
    #[test]
    fn the_taller_chat_takes_a_quarter_of_the_view() {
        let short = layout(800, 600, false, false);
        let tall = layout(800, 600, false, true);
        assert_eq!(short.world.h, 472);
        assert_eq!(tall.world.h, 472 - 116);
        assert_eq!(tall.chat.y, short.chat.y - 116);
        assert_eq!(tall.chat.h, 101 + 116);
    }
    #[test]
    fn reply_keys_emit_distinct_shared_entry_intents_without_local_history_or_sender_state() {
        use dereth_client_contract::chat::entry::{EntryAction, ReplyTarget};
        with_context(&World::default(), |c| {
            let mut chat = Chat {
                text: "unsent draft".into(),
                ..Default::default()
            };
            for (name, target) in [
                ("Reply", ReplyTarget::LastTeller),
                ("MonarchReply", ReplyTarget::Monarch),
                ("PatronReply", ReplyTarget::Patron),
            ] {
                assert_eq!(
                    chat.event(ControlEvent::Action(name.into()), c),
                    vec![PanelAction::Game(UiRequest::ChatEntry {
                        window: 8,
                        text: "unsent draft".into(),
                        action: EntryAction::Reply {
                            target,
                            prefix: "@tell ".into()
                        }
                    })]
                );
                assert_eq!(chat.text, "unsent draft");
            }
        });
    }
    #[test]
    fn connected_chat_uses_talk_target_and_rejects_disabled_destination() {
        let g = World::default();
        let mut state = ClassicState {
            chat_focus: Some((
                4,
                [
                    false, true, true, false, true, false, false, false, false, false, false,
                    false, false, false,
                ],
            )),
            ..Default::default()
        };
        state.chat_target = Some((ObjectId(9), "Patron Name".into()));
        let pregame = PregameView::default();
        let keyboard = KeyboardState::default();
        let settings = ClassicSettings::default();
        let c = context(&g, &state, &pregame, &keyboard, &settings);
        let mut chat = Chat::default();
        assert_eq!(chat.destination(&c), 5);
        assert_eq!(Chat::target(&c), Some((ObjectId(9), "Patron Name".into())));
        assert!(chat
            .event(
                ControlEvent::Select {
                    id: "destination".into(),
                    index: 3
                },
                &c
            )
            .is_empty());
        assert_eq!(
            Chat::enabled(&c),
            vec![
                true, true, true, false, false, true, false, false, false, false, false, false,
                false, false
            ]
        );
        assert_eq!(
            chat.event(
                ControlEvent::Select {
                    id: "destination".into(),
                    index: 1
                },
                &c
            ),
            vec![
                PanelAction::Game(UiRequest::SetTalkFocus { focus: 2 }),
                PanelAction::Game(UiRequest::StartTell {
                    name: "Patron Name".into()
                }),
            ]
        );
    }
    /// Behaviour: chat.talk-to-menu.the-squelch-row-is-a-toggle-and-its-message-names-the-speaker
    #[test]
    fn the_squelch_menu_sends_the_talk_targets_identity_to_the_runtime() {
        let game = World {
            selected: Some(ObjectId(88)),
            ..Default::default()
        };
        let mut state = ClassicState {
            chat_focus: Some((1, [true; 14])),
            chat_target: Some((ObjectId(9), "Aerin".into())),
            ..Default::default()
        };
        let (pregame, keyboard, settings) = Default::default();
        let mut chat = Chat::default();
        let click = || ControlEvent::Select {
            id: "destination".into(),
            index: 0,
        };
        let c = context(&game, &state, &pregame, &keyboard, &settings);
        assert_eq!(
            chat.event(click(), &c),
            vec![PanelAction::Game(UiRequest::ToggleCharacterSquelch(
                ObjectId(9)
            ))]
        );
        state.chat_target = None;
        let c = context(&game, &state, &pregame, &keyboard, &settings);
        assert!(chat.event(click(), &c).is_empty());
    }

    #[test]
    fn split_hotkey_focuses_only_a_visible_stack_entry() {
        let g = World {
            max_split: 12,
            ..Default::default()
        };
        with_context(&g, |c| {
            // The view has no selection even when a stale maximum remains.
            assert!(Hud::default()
                .event(ControlEvent::Action("SelectionSplitStack".into()), c)
                .is_empty());
        });
    }
    fn row(id: u32, x: f32, z: f32) -> RadarEntry {
        RadarEntry {
            id: ObjectId(id),
            player_space: (x, 0.0, z),
            radar_enum: 4,
            in_world: true,
            ..Default::default()
        }
    }
    #[test]
    fn small_world_places_compact_radar_inside_world_viewport() {
        let r = regions(640, 480);
        assert_eq!(r.world, rect(0, 28, 331, 352));
        assert_eq!(r.radar, rect(250, 51, 80, 80));
        assert_eq!(r.toolbar, rect(331, 390, 309, 90));
        assert_eq!(regions(800, 600).radar, rect(588, 28, 120, 120));
    }
    #[test]
    fn radar_excludes_boundary_hidden_self_and_unshowable_rows() {
        let mut g = World {
            outside: true,
            rows: vec![
                row(2, 73.9, 0.0),
                row(3, 74.0, 0.0),
                row(4, 0.0, 0.0),
                row(5, 0.0, 0.0),
                row(6, 0.0, 0.0),
            ],
            ..Default::default()
        };
        g.rows[2].bitfield = 0x80;
        g.rows[3].is_self = true;
        g.rows[4].radar_enum = 0;
        with_context(&g, |c| {
            assert_eq!(
                Radar::default()
                    .blips(c)
                    .iter()
                    .map(|b| b.object)
                    .collect::<Vec<_>>(),
                vec![ObjectId(2)]
            )
        });
        g.outside = false;
        g.rows[0].player_space.0 = 24.0;
        with_context(&g, |c| assert!(Radar::default().blips(c).is_empty()));
    }
    #[test]
    fn radar_dims_at_five_units_and_prefers_bright_over_equal_distance_dim() {
        let g = World {
            outside: true,
            rows: vec![row(2, 0.0, 5.0), row(3, 0.0, 4.99)],
            ..Default::default()
        };
        with_context(&g, |c| {
            let mut r = Radar::default();
            let bs = r.blips(c);
            assert!(!bs[0].bright);
            assert!(bs[1].bright);
            assert_eq!(
                r.event(
                    ControlEvent::Pointer {
                        x: 69,
                        y: 60,
                        pressed: true
                    },
                    c
                ),
                vec![PanelAction::Game(UiRequest::Select(ObjectId(3)))]
            );
            assert!(r
                .event(
                    ControlEvent::Pointer {
                        x: 71,
                        y: 60,
                        pressed: true
                    },
                    c
                )
                .is_empty());
        });
    }
    #[test]
    fn radar_override_precedes_fellowship_color_and_fellowship_precedes_pk() {
        let mut e = row(2, 0.0, 0.0);
        e.is_player = true;
        e.is_pk = true;
        e.is_fellow = true;
        assert_eq!(blip_color(&e), 10);
        e.blip_color = 4;
        assert_eq!(blip_color(&e), 4);
        e.blip_color = 0;
        e.is_fellow = false;
        assert_eq!(blip_color(&e), 5);
    }
    #[test]
    fn allegiance_blip_is_hollow_and_selected_circle_is_seven_pixels_wide() {
        let b = Blip {
            object: ObjectId(2),
            x: 10,
            y: 10,
            color: 0xffffffff,
            shape: 2,
            bright: true,
        };
        let mut f = PanelFrame::new(20, 20);
        blip(&mut f, b, false);
        assert!(!f
            .screen
            .commands
            .iter()
            .any(|c| matches!(c, Command::Fill { x: 10, y: 10, .. })));
        let mut f = PanelFrame::new(20, 20);
        blip(&mut f, b, true);
        assert!(f
            .screen
            .commands
            .iter()
            .any(|c| matches!(c, Command::Fill { x: 7, y: 10, .. })));
        assert!(!f
            .screen
            .commands
            .iter()
            .any(|c| matches!(c,Command::Fill{x,..} if *x<7 || *x>13)));
    }
    #[test]
    fn chat_blur_preserves_unsent_text_and_submit_sends_once() {
        with_context(&World::default(), |c| {
            let mut chat = Chat::default();
            chat.event(
                ControlEvent::Edit {
                    id: "input".into(),
                    text: "hello".into(),
                },
                c,
            );
            assert!(chat
                .event(ControlEvent::Commit { id: "input".into() }, c)
                .is_empty());
            assert_eq!(
                chat.event(ControlEvent::Submit { id: "input".into() }, c),
                vec![
                    PanelAction::Game(UiRequest::ChatLine {
                        text: "hello".into(),
                        window: 8
                    }),
                    // Without "stay in chat mode" a send leaves the chat.
                    PanelAction::Host(HostAction::FocusControl(String::new())),
                ]
            );
            // Enter on the now empty line sends nothing and leaves the chat.
            assert_eq!(
                chat.event(ControlEvent::Submit { id: "input".into() }, c),
                vec![PanelAction::Host(HostAction::FocusControl(String::new()))]
            );
        });
    }
    #[test]
    fn allegiance_chat_selects_legacy_destination_six() {
        with_context(&World::default(), |c| {
            assert_eq!(
                Chat::default().event(
                    ControlEvent::Select {
                        id: "destination".into(),
                        index: 7
                    },
                    c
                ),
                vec![PanelAction::Game(UiRequest::SetTalkFocus { focus: 7 })]
            )
        });
    }
    #[test]
    fn hud_inventory_toggle_does_not_change_combat_mode() {
        with_context(
            &World {
                combat: 1,
                ..Default::default()
            },
            |c| {
                let mut hud = Hud::default();
                assert_eq!(
                    hud.event(ControlEvent::Activate("inventory".into()), c),
                    vec![PanelAction::Toggle("inventory".into())]
                );
                assert_eq!(
                    hud.event(ControlEvent::Activate("combat".into()), c),
                    vec![PanelAction::Host(HostAction::CombatMode(0))]
                );
                let f = hud.frame(c);
                assert_eq!(
                    f.controls.iter().find(|c| c.id == "combat").unwrap().rect,
                    rect(491, 510, 55, 58)
                );
                assert_eq!(
                    f.controls
                        .iter()
                        .find(|c| c.id == "inventory")
                        .unwrap()
                        .rect,
                    rect(737, 510, 63, 58)
                );
            },
        );
    }
    #[test]
    fn the_radar_takes_clicks_over_its_whole_dial() {
        with_context(&World::default(), |c| {
            let hud = Hud::default();
            let f = hud.frame(c);
            let hit = f.controls.iter().find(|c| c.id == "radar-hit").unwrap();
            assert_eq!(hit.rect, radar_rect(800, 600, false, false));
            assert!(hit.enabled);
        });
    }
    #[test]
    fn vitals_zero_maximum_does_not_emit_nonfinite_geometry() {
        with_context(&World::default(), |c| {
            let f = Vitals {
                width: 800,
                numeric: true,
                link: LinkLamp::default(),
            }
            .frame(c);
            let clips: Vec<_> = f
                .screen
                .commands
                .iter()
                .filter_map(|c| match c {
                    Command::Image {
                        did, clip: Some(r), ..
                    } if did == "06001137" => Some(*r),
                    _ => None,
                })
                .collect();
            assert_eq!(clips.len(), 1);
            assert_eq!(clips[0][0], clips[0][2]);
        });
    }

    #[test]
    fn stack_controls_clamp_and_replace_full_name_meter() {
        with_context(
            &World {
                selected: Some(ObjectId(17)),
                split: 4,
                max_split: 10,
                meters: (Some(0.5), None),
                ..Default::default()
            },
            |c| {
                let mut h = Hud::default();
                let f = h.frame(c);
                assert!(f.controls.iter().any(|c| c.id == "stack-count"));
                assert!(!f
                    .screen
                    .commands
                    .iter()
                    .any(|c| matches!(c,Command::Image{did,..} if did=="0600193F")));
                assert_eq!(
                    h.event(
                        ControlEvent::Scroll {
                            id: "stack-slider".into(),
                            value: 99
                        },
                        c
                    ),
                    vec![PanelAction::Game(UiRequest::StackSliderChanged {
                        split: 10,
                        max: 10
                    })]
                );
            },
        );
    }
    #[test]
    fn selected_health_precedes_mana_and_clips_at_fraction() {
        with_context(
            &World {
                selected: Some(ObjectId(17)),
                meters: (Some(0.5), Some(0.8)),
                ..Default::default()
            },
            |c| {
                let f = Hud::default().frame(c);
                let health = f
                    .screen
                    .commands
                    .iter()
                    .find_map(|c| match c {
                        Command::Image { did, clip, .. } if did == "0600193F" => *clip,
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(health[2] - health[0], 73);
                assert!(!f
                    .screen
                    .commands
                    .iter()
                    .any(|c| matches!(c,Command::Image{did,..} if did=="060022D6")));
            },
        );
    }
    #[test]
    fn chat_submission_emits_one_main_window_line_and_default_color_is_green() {
        with_context(&World::default(), |c| {
            let mut chat = Chat::default();
            for n in 0..12 {
                chat.text = format!("line{n}");
                let requests = chat.event(ControlEvent::Submit { id: "input".into() }, c);
                assert_eq!(
                    requests
                        .iter()
                        .filter(|r| matches!(
                            r,
                            PanelAction::Game(UiRequest::ChatLine { window: 8, .. })
                        ))
                        .count(),
                    1
                );
            }
            assert!(chat.text.is_empty());
            assert_eq!(chat_color(0), 0xff80ff7f);
            assert_eq!(chat_color(7), 0xff3fbfff);
        });
    }

    #[test]
    fn selection_queries_are_independent_and_clear_active_subscriptions() {
        use dereth_client_contract::view::SelectionQueryFacts;
        let mut q = SelectionQueries::default();
        let both = SelectionQueryFacts {
            is_player: true,
            has_pet_owner: true,
            attackable: false,
            owned_by_player: true,
        };
        assert_eq!(
            q.update(Some(ObjectId(7)), Some(both), (None, None)),
            vec![
                UiRequest::QueryHealth(ObjectId(7)),
                UiRequest::QueryItemMana(ObjectId(7))
            ]
        );
        assert!(q
            .update(Some(ObjectId(7)), Some(both), (Some(0.5), Some(0.7)))
            .is_empty());
        assert_eq!(
            q.update(None, None, (None, None)),
            vec![
                UiRequest::QueryHealth(ObjectId(0)),
                UiRequest::QueryItemMana(ObjectId(0))
            ]
        );
        assert!(q.update(None, None, (None, None)).is_empty());
    }
    #[test]
    fn stack_text_commits_only_valid_complete_values() {
        with_context(
            &World {
                selected: Some(ObjectId(17)),
                split: 4,
                max_split: 10,
                ..Default::default()
            },
            |c| {
                let mut h = Hud::default();
                assert!(h
                    .event(
                        ControlEvent::Edit {
                            id: "stack-count".into(),
                            text: "9".into()
                        },
                        c
                    )
                    .is_empty());
                assert_eq!(
                    h.event(
                        ControlEvent::Commit {
                            id: "stack-count".into()
                        },
                        c
                    ),
                    vec![PanelAction::Game(UiRequest::StackSliderChanged {
                        split: 9,
                        max: 10
                    })]
                );
                h.event(
                    ControlEvent::Edit {
                        id: "stack-count".into(),
                        text: "99".into(),
                    },
                    c,
                );
                assert!(h
                    .event(
                        ControlEvent::Commit {
                            id: "stack-count".into()
                        },
                        c
                    )
                    .is_empty());
            },
        );
    }
    #[test]
    fn vitals_toggle_number_labels_to_source_icon_composites() {
        with_context(&World::default(), |c| {
            let mut v = Vitals {
                width: 800,
                numeric: true,
                link: LinkLamp::default(),
            };
            let has_icon = |f: PanelFrame| {
                f.screen
                    .commands
                    .iter()
                    .any(|c| matches!(c,Command::Image{did,color_key,..} if did=="0600131A" && color_key.is_some()))
            };
            assert!(!has_icon(v.frame(c)));
            v.event(ControlEvent::Activate("vital:Health".into()), c);
            assert!(has_icon(v.frame(c)));
            assert_eq!(
                v.event(ControlEvent::Activate("burden".into()), c),
                vec![PanelAction::Open("character-info".into())]
            );
        });
    }

    #[test]
    fn link_samples_after_four_seconds_and_blinks_between_twenty_and_forty() {
        let mut lamp = LinkLamp::default();
        lamp.tick(0.0, Some(5.0));
        assert_eq!(lamp.image(), 0x06001360);
        lamp.tick(4.0, Some(21.0));
        assert_eq!(lamp.image(), 0x06001360);
        lamp.tick(4.01, Some(21.0));
        assert_eq!(lamp.image(), 0x06001361);
        lamp.tick(4.75, Some(21.0));
        assert_eq!(lamp.image(), 0x06001361);
        lamp.tick(4.8, Some(21.0));
        assert_eq!(lamp.image(), 0x06001362);
        lamp.tick(9.0, Some(41.0));
        assert_eq!(lamp.image(), 0x06001361);
        lamp.tick(10.0, Some(41.0));
        assert_eq!(lamp.image(), 0x06001361);
    }
    #[test]
    fn stretched_sidebar_uses_full_height_and_floating_radar() {
        let r = regions_stretched(800, 600, true);
        assert_eq!(r.right, rect(500, 28, 300, 482));
        assert_eq!(r.radar, rect(370, 29, 120, 120));
        assert_eq!(r.chat, rect(0, 499, 491, 101));
        assert_eq!(r.bottom.y, 499);
    }
    #[test]
    fn host_split_state_overrides_trait_defaults_and_preserves_clear() {
        let w = World::default();
        let p = PregameView::default();
        let k = KeyboardState::default();
        let settings = ClassicSettings::default();
        let mut state = ClassicState {
            stack_split: Some((3, 12)),
            ..Default::default()
        };
        assert_eq!(
            stack_split(&context(&w, &state, &p, &k, &settings)),
            (3, 12)
        );
        state.stack_split = Some((0, 0));
        assert_eq!(stack_split(&context(&w, &state, &p, &k, &settings)), (0, 0));
        state.stack_split = None;
        assert_eq!(
            stack_split(&context(&w, &state, &p, &k, &settings)),
            (w.split_size(), w.max_split_size())
        );
    }
    #[test]
    fn chat_input_limits_visible_text_to_180_without_counting_sentinel() {
        with_context(&World::default(), |c| {
            let frame = Chat::default().frame(c);
            let input = frame.controls.iter().find(|c| c.id == "input").unwrap();
            assert_eq!(input.rect.x, 47);
            assert!(matches!(
                input.kind,
                ControlKind::Edit { max_chars: 180, .. }
            ));
        });
    }
    #[test]
    fn game_center_indicator_follows_joined_game_and_opens_existing_panel() {
        use dereth_client_contract::view::MiniGameView;
        let mut g = World::default();
        let mut vitals = Vitals {
            width: 800,
            numeric: true,
            link: LinkLamp::default(),
        };
        with_context(&g, |c| {
            assert!(!vitals.indicators(c).iter().any(|x| x.0 == "game-center"))
        });
        g.mini = Some(MiniGameView {
            game: ObjectId(40),
            visible: false,
            ..Default::default()
        });
        with_context(&g, |c| {
            assert!(vitals.indicators(c).contains(&("game-center", 0x06002817)));
            assert_eq!(
                vitals.event(ControlEvent::Activate("game-center".into()), c),
                vec![PanelAction::Open("game-center".into())]
            );
        });
        g.mini.as_mut().unwrap().game = ObjectId(0);
        with_context(&g, |c| {
            assert!(!vitals.indicators(c).iter().any(|x| x.0 == "game-center"))
        });
    }
}
