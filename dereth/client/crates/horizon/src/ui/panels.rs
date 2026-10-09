//! The windows the HUD opens:
//!
//! * **Character**: the figure, wearing what is worn, between the equipment slots, and beside
//!   them the attributes, vitals and skills;
//! * **Inventory**: the pack column (the main pack, the side packs and foci), and the open
//!   pack's grid;
//! * **Spellbook**: the spells by school, the spell components, and Create Spell;
//! * **Social** and **Allegiance** (the allegiance window);
//! * **Map**, **Settings** and **HUD Layout**.

use dereth_client_contract::UiRequest;

use crate::art::{Family, Sprite};
use crate::draw::{Rect, WHITE};
use crate::ui::game::{GameState, Item};
use crate::ui::input::vk;
use crate::ui::kit::{self, Ctx, WindowState};
use crate::ui::paint::{Align, Painter, TextStyle};
use crate::ui::Outcome;

mod commerce;
mod examine;
pub mod info;
mod items;
mod journal;
mod notebook;
pub mod options;
pub mod pad_items;
mod reading;
mod social;
mod spells;
mod split;
mod stats;
pub mod world;

/// The windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WindowId {
    Character,
    Inventory,
    Actions,
    Social,
    Allegiance,
    Map,
    Options,
    Layout,
    Examine,
    Loot,
    Vendor,
    Trade,
    Journal,
    Book,
    Salvage,
    /// Buying a dwelling and paying its maintenance, at its deed.
    Maintenance,
    /// The player's own house.
    House,
    /// Chess.
    GameCenter,
    /// The barber.
    Barber,
    /// A spell's details, identified by a right-click.
    SpellInfo,
    /// The vitae penalty, from its lamp.
    Vitae,
    /// The load carried, from its lamp.
    Burden,
}

impl WindowId {
    pub const ALL: [Self; 22] = [
        Self::Character,
        Self::Inventory,
        Self::Actions,
        Self::Social,
        Self::Allegiance,
        Self::Map,
        Self::Options,
        Self::Layout,
        Self::Examine,
        Self::Loot,
        Self::Vendor,
        Self::Trade,
        Self::Journal,
        Self::Book,
        Self::Salvage,
        Self::Maintenance,
        Self::House,
        Self::GameCenter,
        Self::Barber,
        Self::SpellInfo,
        Self::Vitae,
        Self::Burden,
    ];

    /// The name `--horizon-open` uses.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "character" => Self::Character,
            "inventory" => Self::Inventory,
            "actions" | "spells" => Self::Actions,
            "social" => Self::Social,
            "allegiance" | "fc" => Self::Allegiance,
            "map" => Self::Map,
            "options" | "config" => Self::Options,
            "layout" | "hud" => Self::Layout,
            "examine" => Self::Examine,
            "journal" | "duty" => Self::Journal,
            "house" => Self::House,
            "chess" | "game-center" => Self::GameCenter,
            _ => return None,
        })
    }

    fn title(self) -> &'static str {
        match self {
            Self::Character => "Character",
            Self::Inventory => "Inventory",
            Self::Actions => "Spellbook",
            Self::Social => "Social",
            Self::Allegiance => "Allegiance",
            Self::Map => "Map",
            Self::Options => "Settings",
            Self::Layout => "HUD Layout",
            Self::Examine => "Examine",
            Self::Loot => "Loot",
            Self::Vendor => "Shop",
            Self::Trade => "Trade",
            Self::Journal => "Journal",
            Self::Book => "Book",
            Self::Salvage => "Salvage",
            Self::Maintenance => "Housing",
            Self::House => "House",
            Self::GameCenter => "Game Center",
            Self::Barber => "Barber",
            Self::SpellInfo => "Spell",
            Self::Vitae => "Vitae",
            Self::Burden => "Burden",
        }
    }

    /// The window's size in layout units, and where it opens.
    pub fn geometry(self) -> ((f32, f32), (f32, f32)) {
        match self {
            Self::Character => ((800.0, 640.0), (240.0, 150.0)),
            Self::Inventory => ((540.0, 560.0), (1080.0, 220.0)),
            Self::Actions => ((570.0, 600.0), (420.0, 120.0)),
            Self::Social => ((600.0, 500.0), (560.0, 200.0)),
            Self::Allegiance => ((520.0, 520.0), (640.0, 160.0)),
            Self::Map => ((562.0, 640.0), (680.0, 120.0)),
            Self::Options => ((760.0, 560.0), (580.0, 180.0)),
            Self::Layout => ((420.0, 300.0), (760.0, 300.0)),
            Self::Examine => ((440.0, 560.0), (1180.0, 160.0)),
            Self::Loot => ((440.0, 420.0), (560.0, 260.0)),
            Self::Vendor => ((820.0, 620.0), (240.0, 140.0)),
            Self::Trade => ((640.0, 480.0), (480.0, 200.0)),
            Self::Journal => ((720.0, 520.0), (500.0, 160.0)),
            Self::Book => ((520.0, 600.0), (700.0, 120.0)),
            Self::Salvage => ((380.0, 340.0), (740.0, 260.0)),
            Self::Maintenance => ((480.0, 300.0), (700.0, 260.0)),
            Self::House => ((460.0, 420.0), (640.0, 180.0)),
            Self::GameCenter => ((420.0, 500.0), (720.0, 140.0)),
            Self::Barber => ((420.0, 420.0), (300.0, 160.0)),
            Self::SpellInfo => ((400.0, 440.0), (1040.0, 180.0)),
            Self::Vitae => ((420.0, 480.0), (1240.0, 120.0)),
            Self::Burden => ((420.0, 380.0), (1240.0, 120.0)),
        }
    }
}

/// The room the inventory keeps under its grid, in layout units: the stack splitter, the count
/// and the burden gauge.
const FOOT_ROOM: f32 = 76.0;

/// A burden as the game words it: its share of what the character can carry, in percent.
#[must_use]
pub fn burden_percent(burden: f32) -> String {
    format!(
        "{}%",
        dereth_primitives::num::to_i32((burden * 100.0).round())
    )
}

/// The equipment slots of the character window, each one location of the game's paper doll
/// (its own bits, so a shirt and chest armour are two slots), with its name. Over the head: the
/// trinket, the head and the cloak.
const TOP_SLOTS: [(&str, u32); 3] = {
    use dereth_rules::slots::loc;
    [
        ("Trinket", loc::TRINKET_ONE),
        ("Head", loc::HEAD_WEAR),
        ("Cloak", loc::CLOAK),
    ]
};
/// Down the figure's left, the armour from the shoulders down: the arms and hands, then the
/// abdomen, legs and feet.
const LEFT_SLOTS: [(&str, u32); 8] = {
    use dereth_rules::slots::loc;
    [
        ("Chest Armour", loc::CHEST_ARMOR),
        ("Upper Arm Armour", loc::UPPER_ARM_ARMOR),
        ("Lower Arm Armour", loc::LOWER_ARM_ARMOR),
        ("Hands", loc::HAND_WEAR),
        ("Abdomen Armour", loc::ABDOMEN_ARMOR),
        ("Upper Leg Armour", loc::UPPER_LEG_ARMOR),
        ("Lower Leg Armour", loc::LOWER_LEG_ARMOR),
        ("Feet", loc::FOOT_WEAR),
    ]
};
/// Down the figure's right: the necklace and the clothes, a row's space, then the bracelets and
/// the rings, left above right.
const RIGHT_SLOTS: [Option<(&str, u32)>; 8] = {
    use dereth_rules::slots::loc;
    [
        Some(("Necklace", loc::NECK_WEAR)),
        Some(("Shirt", loc::CHEST_WEAR)),
        Some(("Trousers", loc::UPPER_LEG_WEAR)),
        None,
        Some(("Bracelet (L)", loc::WRIST_WEAR_LEFT)),
        Some(("Bracelet (R)", loc::WRIST_WEAR_RIGHT)),
        Some(("Ring (L)", loc::FINGER_WEAR_LEFT)),
        Some(("Ring (R)", loc::FINGER_WEAR_RIGHT)),
    ]
};
/// The placeholder an empty slot of `mask` shows, and its tint: each sigil in its own colour.
fn placeholder(mask: u32) -> Option<(&'static str, u32)> {
    use dereth_rules::slots::loc;
    let plain = WHITE;
    Some(match mask {
        loc::HEAD_WEAR => ("head", plain),
        loc::CHEST_ARMOR => ("chest", plain),
        loc::UPPER_ARM_ARMOR => ("upper-arm", plain),
        loc::LOWER_ARM_ARMOR => ("lower-arm", plain),
        loc::HAND_WEAR => ("hands", plain),
        loc::ABDOMEN_ARMOR => ("abdomen", plain),
        loc::UPPER_LEG_ARMOR => ("upper-leg", plain),
        loc::LOWER_LEG_ARMOR => ("lower-leg", plain),
        loc::FOOT_WEAR => ("feet", plain),
        loc::CHEST_WEAR => ("shirt", plain),
        loc::UPPER_LEG_WEAR => ("trousers", plain),
        loc::NECK_WEAR => ("necklace", plain),
        loc::WRIST_WEAR_LEFT | loc::WRIST_WEAR_RIGHT => ("bracelet", plain),
        loc::FINGER_WEAR_LEFT | loc::FINGER_WEAR_RIGHT => ("ring", plain),
        loc::TRINKET_ONE => ("trinket", plain),
        loc::CLOAK => ("cloak", plain),
        loc::SIGIL_ONE => ("sigil", 0xFF8C_B8FF),
        loc::SIGIL_TWO => ("sigil", 0xFFFF_E080),
        loc::SIGIL_THREE => ("sigil", 0xFFFF_8C80),
        loc::WEAPON_READY_SLOT => ("weapon", plain),
        loc::SHIELD => ("shield", plain),
        loc::MISSILE_AMMO => ("ammo", plain),
        _ => return None,
    })
}

/// The width of the Character window's column for the figure and its slots, which stand centred
/// in it, the sheet beside it.
const DOLL_COLUMN: f32 = 384.0;

/// What is held, in a row of its own at the foot.
const HELD_SLOTS: [(&str, u32); 3] = {
    use dereth_rules::slots::loc;
    [
        ("Weapon", loc::WEAPON_READY_SLOT),
        ("Shield", loc::SHIELD),
        ("Ammunition", loc::MISSILE_AMMO),
    ]
};
const SIGIL_SLOTS: [(&str, u32); 3] = {
    use dereth_rules::slots::loc;
    [
        ("Sigil (Blue)", loc::SIGIL_ONE),
        ("Sigil (Yellow)", loc::SIGIL_TWO),
        ("Sigil (Red)", loc::SIGIL_THREE),
    ]
};

/// One setting of the Options window: the preference, its label in the game's own words, its kind
/// (2 a choice, 3 a number, 4 a switch), its range and its choices.
#[derive(Debug, Clone, PartialEq)]
pub struct OptionRow {
    pub name: &'static str,
    pub label: String,
    pub kind: u32,
    pub range: Option<(f32, f32)>,
    /// `(stored value, label)` for a choice.
    pub choices: Vec<(i32, String)>,
}

/// One of the character's own options, as the Options window offers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterOption {
    pub option: dereth_client_contract::view::PlayerOption,
    pub label: String,
    pub help: String,
    /// What it needs of the world's era to mean anything.
    pub needs: dereth_client_contract::options::sheet::Needs,
}

/// Every window's state.
#[derive(Debug, Default)]
pub struct Windows {
    /// The pad's item menu, open on an item.
    pub item_menu: Option<pad_items::ItemMenu>,
    /// The pad's split box: the stack, and how many to split off.
    pub split_box: Option<(Item, u32)>,
    /// Where the item options or the split box stood last frame: over every window.
    pub pad_box: Option<Rect>,
    /// A spell was chosen with the pad: the focus goes on to what acts on it.
    pub focus_spell_details: bool,
    /// A thing to salvage, waiting for the salvage window the Ust was used to open.
    pub salvage_waiting: Option<dereth_primitives::ObjectId>,
    /// A thing to put on once what is worn in its places has come off.
    pub equip_waiting: Option<pad_items::EquipWaiting>,
    /// A pack picked up with the pad to be put down at another place among the packs, and its
    /// picture as last drawn.
    pub moving_pack: Option<dereth_primitives::ObjectId>,
    moving_icon: Option<Sprite>,
    /// Where the pad's menu stood last frame: over every window.
    pub pad_menu: Option<Rect>,
    /// How far the inventory's grid and its pack column are scrolled, in pixels.
    inventory_scroll: f32,
    /// Each cooling item's countdown, run down every frame between the game's reports.
    item_cooldowns: std::collections::HashMap<dereth_primitives::ObjectId, (f64, f64)>,
    packs_scroll: f32,
    /// Where the Character window shows the paper doll this frame, and how many quads of the
    /// draw list are under it.
    pub doll: Option<(Rect, usize)>,
    /// The settings the Options window offers.
    pub options: Vec<OptionRow>,
    /// The character's own options, the server keeps: each section's heading and its switches,
    /// with their labels and help in the game's words.
    pub character_options: Vec<(String, Vec<CharacterOption>)>,
    /// How far the character options are scrolled.
    character_scroll: f32,
    states: std::collections::HashMap<WindowId, WindowState>,
    /// Windows asked for by name before the world was up.
    pending: Vec<WindowId>,
    inventory_bag: usize,
    actions_tab: usize,
    character_tab: usize,
    social_tab: usize,
    /// The front-to-back order windows are drawn in, last on top.
    order: Vec<WindowId>,
    /// The selected stack, how many of it a drag moves, and its size; and what is typed in the
    /// splitter's box.
    split: Option<split::Split>,
    split_typed: String,
    /// The vendor window: whether it is up for the game's shop, its tab and type filter, the
    /// stock row chosen and how many of it, the basket row chosen, the lists' scrolls, and the
    /// stock rows' stack sizes as last sent.
    vendor_shown: bool,
    vendor_tab: usize,
    vendor_filter: usize,
    /// The vendor's category list while it is open: the box it hangs from and its first row shown.
    vendor_filter_menu: Option<(Rect, usize)>,
    vendor_selected: Option<dereth_primitives::ObjectId>,
    vendor_quantity: u32,
    vendor_scroll: f32,
    basket_selected: Option<dereth_primitives::ObjectId>,
    basket_scroll: f32,
    vendor_sizes: Vec<(dereth_primitives::ObjectId, i32)>,
    /// The trade window: whether it is up for the game's trade, and its two lists' scrolls.
    trade_shown: bool,
    trade_scroll: (f32, f32),
    /// The spell bar the spellbook adds to: the second bar's current tab.
    pub spell_tab: usize,
    /// The spellbook's scroll, its chosen spell, and the spell whose forgetting is being asked.
    actions_scroll: f32,
    actions_selected: Option<u32>,
    /// The spells the character knew last frame, once the spellbook has come: a spell new to it
    /// opens the Spellbook on it.
    known_spells: Option<std::collections::BTreeSet<u32>>,
    forget_asked: Option<u32>,
    /// The social windows: whether the game is keeping the fellowship and the allegiance up to
    /// date, what is being typed for a new fellowship and a new friend, which text box has the
    /// keyboard, and the rows chosen.
    fellowship_updates: bool,
    allegiance_updates: bool,
    fellowship_name: String,
    friend_name: String,
    typing_field: u32,
    fellow_selected: Option<dereth_primitives::ObjectId>,
    friend_selected: Option<dereth_primitives::ObjectId>,
    allegiance_selected: Option<dereth_primitives::ObjectId>,
    /// The book window: the opening it shows; whether the salvage window is up, and its
    /// scroll; the components' scroll.
    book_opening: Option<u64>,
    /// The page being written on, while the page has the keyboard.
    book_writing: Option<i32>,
    salvage_shown: bool,
    salvage_scroll: f32,
    components_scroll: f32,
    /// The wanted count being typed for a spell component, and the component whose box had the
    /// keyboard last frame (what is typed is set when the box lets the keyboard go).
    component_typed: String,
    component_typing: Option<u32>,
    /// The Journal's chosen contract, the one whose abandoning is being asked, and the titles'
    /// scroll.
    contract_selected: Option<u32>,
    abandon_asked: Option<(u32, String)>,
    titles_scroll: f32,
    /// The row whose raise is waiting for the game's answer: its key, what it showed, and when.
    raise_wait: Option<((bool, u32), String, f64)>,
    /// How far the skills list is scrolled.
    skills_scroll: f32,
    /// The chest the loot window shows, while it is open.
    loot_open: Option<dereth_primitives::ObjectId>,
    /// What the inventory and the loot window held last frame, while each was open: a
    /// selection among them is let go when its window closes.
    shown_items: [Option<Vec<dereth_primitives::ObjectId>>; 2],
    /// How far the examine window's text is scrolled, and the selection it last followed.
    examine_scroll: f32,
    examine_last_target: Option<dereth_primitives::ObjectId>,
    /// The inscription being written, the item it is for, and whether its box had the keyboard
    /// last frame (it is committed when the box lets the keyboard go).
    inscription_edit: String,
    /// How far the inscription box is scrolled, in pixels.
    inscription_scroll: f32,
    inscription_for: Option<dereth_primitives::ObjectId>,
    inscription_was_focused: bool,
    /// The inscription last committed: its object, its words and its scribe, shown without
    /// waiting for the server.
    inscribed: Option<(dereth_primitives::ObjectId, String, String)>,
    tip: Option<(String, Vec<String>)>,
    /// The settings window's own state.
    pub options_page: options::OptionsState,
    /// The blacklist: the name being typed, and the row chosen.
    squelch_name: String,
    squelch_selected: Option<usize>,
    /// The Journal's tab; whether the game has been told the notebook is on screen; what is
    /// typed into its search.
    journal_tab: usize,
    notebook_shown: bool,
    notebook_search: String,
    /// The world's services' windows.
    pub world: world::WorldWindows,
    /// What is typed into the spellbook's search.
    spell_search: String,
    /// The spell the spell information window shows, and whether it is an effect in force.
    info_spell: Option<(u32, bool)>,
}

impl Windows {
    /// Open the window named `name` (as `--horizon-open` spells it) once the world is up.
    pub fn open_by_name(&mut self, name: &str) {
        if let Some(id) = WindowId::from_name(name) {
            self.pending.push(id);
        }
    }

    pub fn open(&mut self, id: WindowId, now: f64) {
        let s = self.states.entry(id).or_default();
        if !s.open {
            s.open = true;
            s.opened_at = now;
        }
        self.order.retain(|w| *w != id);
        self.order.push(id);
    }

    /// Where each open window is on screen, bottom first, as last placed.
    #[must_use]
    pub fn rects(&self, scale: f32) -> Vec<(WindowId, Rect)> {
        self.order
            .iter()
            .filter(|id| self.is_open(**id) && **id != WindowId::Layout)
            .map(|id| {
                let ((w, h), (x, y)) = id.geometry();
                let (x, y) = self
                    .states
                    .get(id)
                    .and_then(|s| s.pos)
                    .unwrap_or((x * scale, y * scale));
                (*id, Rect::new(x, y, w * scale, h * scale))
            })
            .collect()
    }

    /// A press on a window brings it to the top, before anything sees the press, so the press
    /// lands on it and on nothing under it.
    fn raise_pressed(&mut self, input: &crate::ui::input::InputFrame, scale: f32) {
        if !(input.pressed[0] || input.pressed[1]) {
            return;
        }
        let (mx, my) = input.mouse;
        if let Some((top, _)) = self
            .rects(scale)
            .into_iter()
            .rev()
            .find(|(_, r)| r.contains(mx, my))
        {
            self.order.retain(|w| *w != top);
            self.order.push(top);
        }
    }

    /// Bring the window drawn over `r` (the pad's focus is in it) to the top.
    fn raise_at(&mut self, r: Rect, scale: f32) {
        let same = |a: &Rect| (a.x - r.x).abs() < 2.0 && (a.y - r.y).abs() < 2.0;
        if let Some((id, _)) = self.rects(scale).into_iter().find(|(_, w)| same(w)) {
            if self.order.last() != Some(&id) {
                self.order.retain(|w| *w != id);
                self.order.push(id);
            }
        }
    }

    pub fn close(&mut self, id: WindowId) {
        if let Some(s) = self.states.get_mut(&id) {
            s.open = false;
        }
    }

    pub fn toggle(&mut self, id: WindowId, now: f64) {
        if self.is_open(id) {
            if let Some(s) = self.states.get_mut(&id) {
                s.open = false;
            }
        } else {
            self.open(id, now);
        }
    }

    /// A window's state, for a window drawn elsewhere.
    #[must_use]
    pub fn state(&self, id: WindowId) -> WindowState {
        self.states.get(&id).copied().unwrap_or_default()
    }

    pub fn set_state(&mut self, id: WindowId, state: WindowState) {
        self.states.insert(id, state);
    }

    /// The tooltip the windows showed this frame: its title and lines.
    #[must_use]
    pub fn tip(&self) -> Option<&(String, Vec<String>)> {
        self.tip.as_ref()
    }

    /// Whether one of the windows' text boxes has the keyboard.
    #[must_use]
    pub const fn has_text_focus(&self) -> bool {
        self.typing_field != 0 || self.book_writing.is_some()
    }

    /// Whether any window is open.
    /// Where a stack no open window shows is split, unless the HUD layout moves it: a box right
    /// of the parameter bar, standing a little higher than it.
    #[must_use]
    pub fn ground_split_rect(scale: f32, screen: (f32, f32)) -> Rect {
        Rect::new(
            screen.0 / 2.0 + 256.0 * scale,
            screen.1 - 46.0 * scale,
            360.0 * scale,
            40.0 * scale,
        )
    }

    /// The box a stack no open window shows is split in (on the ground or in a closed pack): the
    /// tooltip's plain ground, the splitter in it. A HUD element; while the layout is being
    /// arranged it shows empty, to be moved.
    pub fn ground_split(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        out: &mut Outcome,
    ) {
        let shown = self.is_open(WindowId::Inventory)
            || (self.is_open(WindowId::Loot)
                && state
                    .loot
                    .as_ref()
                    .is_some_and(|(_, _, items)| self.splitting_one_of(items)));
        let r = Self::ground_split_rect(p.scale, p.screen);
        let k = p.scale;
        if self.split.is_none() || shown {
            if self.is_open(WindowId::Layout) {
                kit::plain_ground(p, r);
                let label =
                    TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
                p.text_in(&label, r.offset(10.0 * k, 0.0), Align::Left, "Split");
            }
            return;
        }
        kit::plain_ground(p, r);
        if ctx.over(&r) && (ctx.input.pressed[0] || ctx.input.pressed[1]) {
            ctx.input.captured = true;
        }
        self.split_row(
            p,
            ctx,
            Rect::new(r.x + 10.0 * k, r.y + 8.0 * k, r.w - 20.0 * k, 24.0 * k),
            out,
        );
    }

    /// A selection inside the inventory or a chest or corpse is let go when that window closes.
    fn follow_container_selection(&mut self, state: &GameState, out: &mut Outcome) {
        let target = state.target.as_ref().map(|t| t.id);
        let was = std::mem::take(&mut self.shown_items);
        let open = [
            self.is_open(WindowId::Inventory),
            self.is_open(WindowId::Loot),
        ];
        for (held, open) in was.iter().zip(open) {
            if let (Some(held), false, Some(t)) = (held, open, target) {
                if held.contains(&t) {
                    out.requests
                        .push(UiRequest::Select(dereth_primitives::ObjectId(0)));
                }
            }
        }
        self.shown_items = [
            open[0].then(|| {
                state
                    .pack
                    .iter()
                    .chain(
                        state
                            .side_packs
                            .iter()
                            .flat_map(|(pack, items)| std::iter::once(pack).chain(items.iter())),
                    )
                    .map(|i| i.id)
                    .collect()
            }),
            open[1].then(|| {
                state
                    .loot
                    .iter()
                    .flat_map(|(_, _, items)| items.iter().map(|i| i.id))
                    .collect()
            }),
        ];
    }

    /// When the most lately opened of the open windows was opened, if any is open.
    #[must_use]
    pub fn latest_opened_at(&self) -> Option<f64> {
        self.states
            .values()
            .filter(|s| s.open)
            .map(|s| s.opened_at)
            .max_by(f64::total_cmp)
    }

    #[must_use]
    pub fn any_open(&self) -> bool {
        self.states.values().any(|s| s.open)
    }

    #[must_use]
    pub fn is_open(&self, id: WindowId) -> bool {
        self.states.get(&id).is_some_and(|s| s.open)
    }

    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        out: &mut Outcome,
    ) {
        self.follow_examine(ctx, state, out);
        self.follow_loot(ctx, state, out);
        self.follow_container_selection(state, out);
        self.follow_split(state, out);
        self.follow_commerce(ctx, state, out);
        self.follow_social(out);
        self.follow_reading(ctx, state, out);
        self.follow_notebook(state, out);
        self.follow_world(ctx, state, out);
        self.follow_learned_spells(ctx, state);
        for id in std::mem::take(&mut self.pending) {
            // Staggered, so a capture that opens several shows each one.
            self.open(id, ctx.time - 1.0);
        }
        self.tip = None;
        // Escape closes the window on top, but for a key the key bindings page is waiting for, a
        // dropdown list open in the settings, and a text box with the keyboard, which take it
        // themselves.
        let capturing = state
            .key_bindings
            .as_ref()
            .is_some_and(|k| k.capture.is_some())
            || self.options_page.menu.is_some()
            || self.has_text_focus();
        if !capturing && ctx.input.take_key(vk::ESCAPE) {
            if let Some(top) = self.order.iter().rev().find(|w| self.is_open(**w)).copied() {
                if let Some(s) = self.states.get_mut(&top) {
                    s.open = false;
                }
            }
        }
        self.raise_pressed(ctx.input, p.scale);
        if let Some(r) = ctx.input.pad.raise.take() {
            self.raise_at(r, p.scale);
        }
        let rects = self.rects(p.scale);
        let order: Vec<WindowId> = self.order.clone();
        for id in order {
            // The HUD draws its own layout window, over the element outlines.
            if !self.is_open(id) || id == WindowId::Layout {
                continue;
            }
            // The windows over this one take the pointer where they cover it.
            let at = rects
                .iter()
                .position(|(w, _)| *w == id)
                .unwrap_or(rects.len());
            ctx.input.occluders = rects.iter().skip(at + 1).map(|(_, r)| *r).collect();
            // The pad's item options and split box are over every window.
            ctx.input.occluders.extend(self.pad_box);
            ctx.input.occluders.extend(self.pad_menu);
            let (size, pos) = id.geometry();
            let k = p.scale;
            let mut s = self.states.get(&id).copied().unwrap_or_default();
            let title = Self::window_title(p, state, id, size.0 * k);
            let win = kit::window(p, ctx, &mut s, &title, size, (pos.0 * k, pos.1 * k));
            // Known to the pad's focus by what the window is, whatever its title says now.
            ctx.input.nav.rename(id.title());
            self.states.insert(id, s);
            let body = win.body;
            match id {
                WindowId::Character => self.character(p, ctx, state, body, out),
                WindowId::Inventory => self.inventory(p, ctx, state, body, out),
                WindowId::Actions => self.actions(p, ctx, state, body, out),
                WindowId::Social => self.social(p, ctx, state, body, out),
                WindowId::Allegiance => self.allegiance(p, ctx, state, body, out),
                WindowId::Map => self.map(p, ctx, state, body),
                WindowId::Options => self.options(p, ctx, state, body, out),
                WindowId::Layout => {}
                WindowId::Examine => self.examine(p, ctx, state, body, out),
                WindowId::Loot => self.loot(p, ctx, state, body, out),
                WindowId::Vendor => self.vendor(p, ctx, state, body, out),
                WindowId::Trade => self.trade(p, ctx, state, body, out),
                WindowId::Journal => self.journal(p, ctx, state, body, out),
                WindowId::Book => self.book(p, ctx, state, body, out),
                WindowId::Salvage => self.salvage(p, ctx, state, body, out),
                WindowId::Maintenance => self.maintenance(p, ctx, state, body, out),
                WindowId::House => self.house(p, ctx, state, body, out),
                WindowId::GameCenter => self.chess(p, ctx, state, body, out),
                WindowId::Barber => self.barber(p, ctx, state, body, out),
                WindowId::SpellInfo => self.spell_info(p, ctx, state, body, out),
                WindowId::Vitae => self.vitae(p, ctx, state, body),
                WindowId::Burden => self.burden(p, ctx, state, body),
            }
        }
        ctx.input.occluders.clear();
        self.pad_item_boxes(p, ctx, state, out);
        // A pack being moved, carried where the pad's focus is.
        if self.moving_pack.is_none() {
            self.moving_icon = None;
        }
        if let (Some(icon), Some(r)) = (self.moving_icon, ctx.input.pad.focus) {
            crate::ui::draw_lifted(p, &icon, (r.x + r.w / 2.0, r.y + r.h / 2.0));
        }
        if let Some((title, lines)) = &self.tip {
            kit::tooltip(p, ctx, title, lines);
        }
    }

    /// The title on window `id`'s bar, `width` wide: its name, but for a book, which carries the
    /// title of the work, cut to fit between the bar's ends.
    fn window_title(p: &Painter<'_>, state: &GameState, id: WindowId, width: f32) -> String {
        let work = match id {
            WindowId::Book => state
                .book
                .as_ref()
                .map(|b| b.title.trim())
                .filter(|t| !t.is_empty()),
            _ => None,
        };
        work.map_or_else(
            || id.title().to_owned(),
            |t| {
                let heading = TextStyle::new(Family::Heading, 23.0, WHITE);
                p.fit(&heading, t, width - 120.0 * p.scale)
            },
        )
    }

    fn item_slot(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        r: Rect,
        item: Option<&Item>,
    ) -> bool {
        // The game's own tile, composed as its item lists compose it.
        let icon: Option<Sprite> = item.and_then(|i| p.art.ac_item(i));
        let clicked = kit::icon_slot(p, ctx, r, icon.as_ref(), WHITE);
        if let Some(i) = item {
            // The item's cooldown, swept over its tile as the hotbar sweeps it.
            if let Some((left, length)) = i.cooldown {
                let seen = self.item_cooldowns.entry(i.id).or_insert((left, ctx.time));
                let left = crate::ui::hud::run_down(seen, left, ctx.time);
                if left > 0.0 {
                    crate::ui::hud::recast_sweep(p, Rect::new(r.x, r.y, r.w, r.w), left, length);
                }
            } else {
                self.item_cooldowns.remove(&i.id);
            }
            if let Some(n) = i.stack.filter(|n| *n > 1) {
                let style =
                    TextStyle::new(Family::Numerals, 10.0, 0xFFFF_FFFF).edge(ctx.colours.edge());
                let s = short_count(n);
                let w = p.measure(&style, &s);
                p.text(
                    &style,
                    r.right() - w - 3.0 * p.scale,
                    r.bottom() - 14.0 * p.scale,
                    &s,
                );
            }
            if ctx.input.hover(&r) {
                // The whole count, where the tile shows it short.
                let lines = i
                    .stack
                    .filter(|n| *n >= 1000)
                    .map(|n| vec![crate::ui::hud::grouped(u64::from(n))])
                    .unwrap_or_default();
                self.tip = Some((i.name.clone(), lines));
            }
        }
        clicked
    }

    // -----------------------------------------------------------------------------------------
    // Character
    // -----------------------------------------------------------------------------------------

    fn character(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        // The figure with its slots around it; the character's sheet on
        // the right.
        // The figure's column, narrower than its share of the window and centred in it; the
        // whole of it stood in the middle of the window's height.
        let slot = 42.0 * k;
        let gap = 6.0 * k;
        let step = slot + gap;
        let rows = 8.0;
        let figure_w = 212.0 * k;
        let gear_w = figure_w + 2.0 * (slot + 10.0 * k);
        let held_gap = 30.0 * k;
        let tall = step + rows * step - gap + 10.0 * k + slot + held_gap + slot;
        let gear = Rect::new(
            body.x + (DOLL_COLUMN * k - gear_w) / 2.0,
            body.y + ((body.h - tall) / 2.0).max(0.0),
            gear_w,
            tall,
        );
        // An item dropped anywhere on the figure or its slots is worn where it goes, as the
        // game's paper doll takes a drop on the figure.
        ctx.drops.push((
            gear,
            Some(dereth_client_contract::view::DropTarget::EquipCanvas.into()),
        ));
        // The top row over the figure, a column down each side of it, the sigils under it,
        // and what is held in a row of its own at the foot.
        let figure = Rect::new(
            gear.x + slot + 10.0 * k,
            gear.y + step,
            figure_w,
            rows * step - gap,
        );
        kit::figure_ground(p, figure);
        // The doll stands on the window, under whatever is drawn over it, and is seen only
        // inside the frame's bars.
        self.doll = Some((kit::figure_inside(p, figure), p.list.mark()));
        let features = state.world_services.features;
        let mut tiles: Vec<(Rect, &(&str, u32))> = Vec::new();
        // The cloak and the trinket only where the world has them.
        let top: Vec<&(&str, u32)> = TOP_SLOTS
            .iter()
            .filter(|(_, mask)| match *mask {
                dereth_rules::slots::loc::CLOAK => features.cloaks,
                dereth_rules::slots::loc::TRINKET_ONE => features.trinkets,
                _ => true,
            })
            .collect();
        let centred_row = |n: usize, gap: f32| {
            #[allow(clippy::cast_precision_loss)]
            let w = n as f32 * slot + n.saturating_sub(1) as f32 * gap;
            gear.x + (gear.w - w) / 2.0
        };
        let mut x = centred_row(top.len(), 10.0 * k);
        for s in top {
            tiles.push((Rect::new(x, gear.y, slot, slot), s));
            x += slot + 10.0 * k;
        }
        for (i, s) in LEFT_SLOTS.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let y = gear.y + (i as f32 + 1.0) * step;
            tiles.push((Rect::new(gear.x, y, slot, slot), s));
        }
        for (i, s) in RIGHT_SLOTS.iter().enumerate() {
            if let Some(s) = s {
                #[allow(clippy::cast_precision_loss)]
                let y = gear.y + (i as f32 + 1.0) * step;
                tiles.push((Rect::new(gear.right() - slot, y, slot, slot), s));
            }
        }
        // Each sigil the character has unlocked.
        let sigils: Vec<&(&str, u32)> = SIGIL_SLOTS
            .iter()
            .enumerate()
            .filter(|(i, _)| state.sigil_slots & (1 << i) != 0)
            .map(|(_, s)| s)
            .collect();
        let sigil_y = figure.bottom() + 10.0 * k;
        let mut x = centred_row(sigils.len(), gap);
        for s in sigils {
            tiles.push((Rect::new(x, sigil_y, slot, slot), s));
            x += step;
        }
        // What is held, spread apart, each under a small heading of its own.
        let held_y = sigil_y + slot + held_gap;
        let heading =
            TextStyle::new(Family::Heading, 15.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let spread = 34.0 * k;
        let mut x = centred_row(HELD_SLOTS.len(), spread);
        for (s, label) in HELD_SLOTS.iter().zip(["WEAPON", "SHIELD", "AMMO"]) {
            p.text_in(
                &heading,
                Rect::new(x - spread / 2.0, held_y - 20.0 * k, slot + spread, 16.0 * k),
                Align::Centre,
                label,
            );
            tiles.push((Rect::new(x, held_y, slot, slot), s));
            x += slot + spread;
        }
        for (r, (name, mask)) in tiles {
            let worn = state.equipped.iter().find(|e| e.worn & mask != 0);
            // The shield's slot takes what is dropped on it there: a shield, or with Dual Wield
            // a second weapon for the other hand, as the game's rules allow. Elsewhere the figure
            // takes the drop and the item goes where it is worn.
            if *mask == dereth_rules::slots::loc::SHIELD {
                ctx.drops.push((
                    r,
                    Some(
                        dereth_client_contract::view::DropTarget::EquipLocation {
                            mask: *mask,
                            side: 0,
                        }
                        .into(),
                    ),
                ));
            }
            self.item_tile(p, ctx, state, r, worn, out);
            // With the pad, what is worn is not picked up: confirming on it selects it, and its
            // options (X) take it off.
            if ctx.input.pad.mode.is_some() {
                if let Some(w) = worn {
                    if ctx
                        .drag
                        .as_ref()
                        .is_some_and(|d| d.item == w.id && !d.active)
                    {
                        if let Some(d) = ctx.drag.take() {
                            out.requests.extend(d.on_click);
                        }
                    }
                }
            }
            // An empty slot shows faintly what goes in it.
            if worn.is_none() {
                if let Some((kind, tint)) = placeholder(*mask) {
                    if let Some(ghost) = p.piece(&format!("slot.placeholder.{kind}")) {
                        let at = p
                            .art
                            .piece_value("slot.icon")
                            .filter(|v| v.len() == 3)
                            .unwrap_or_else(|| vec![4.0, 4.0, 40.0]);
                        let u = r.w / 48.0;
                        let icon =
                            Rect::new(r.x + at[0] * u, r.y + at[1] * u, at[2] * u, at[2] * u);
                        p.sprite(&ghost, icon, crate::draw::with_alpha(tint, 0.6));
                    }
                }
            }
            if ctx.input.hover(&r) {
                // The slot's name over what is in it, filled or not.
                let what = worn.map_or_else(|| "Empty".to_owned(), |w| w.name.clone());
                self.tip = Some(((*name).to_owned(), vec![what]));
            }
        }
        // The character's sheet beside the figure's column.
        let sheet_x = body.x + (DOLL_COLUMN + 12.0) * k;
        let mid = Rect::new(sheet_x, body.y, body.right() - sheet_x, body.h);
        kit::panel(p, mid, 0.5);
        let head =
            TextStyle::new(Family::Heading, 34.0, ctx.colours.heading()).edge(ctx.colours.edge());
        let body_style =
            TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
        let small = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        p.text_in(
            &head,
            Rect::new(mid.x, mid.y + 8.0 * k, mid.w, 40.0 * k),
            Align::Centre,
            &state.name,
        );
        let sub = [state.title.as_str(), state.heritage.as_str()]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join("  ·  ");
        p.text_in(
            &small,
            Rect::new(mid.x, mid.y + 50.0 * k, mid.w, 18.0 * k),
            Align::Centre,
            &sub,
        );
        p.text_in(
            &body_style,
            Rect::new(mid.x, mid.y + 70.0 * k, mid.w, 20.0 * k),
            Align::Centre,
            &format!("Level {}", state.level),
        );
        // The tabs keep as far from the panel's sides as its lists do; the tabs overlap each
        // other by a little, which the row's width makes up so the last ends on the margin.
        let labels = ["Attributes", "Skills", "Profile"];
        let margin = 12.0 * k;
        #[allow(clippy::cast_precision_loss)]
        let overlap = 4.0 * k * (labels.len() - 1) as f32;
        let tabs = Rect::new(
            mid.x + margin,
            mid.y + 98.0 * k,
            mid.w - 2.0 * margin + overlap,
            26.0 * k,
        );
        kit::tabs_filling(p, ctx, tabs, &labels, &mut self.character_tab);
        let mut y = tabs.bottom() + 14.0 * k;
        let row = |p: &mut Painter<'_>, y: f32, label: &str, value: String| {
            stats::label_and_value(
                p,
                Rect::new(mid.x, y - 3.0 * k, mid.w, 20.0 * k),
                &small,
                mid.x + 20.0 * k,
                label,
                &body_style,
                mid.right() - 20.0 * k,
                &value,
            );
        };
        // Down to the sheet's foot, as the skills' list is, so both footers stand alike.
        let tab_area = Rect::new(mid.x, y - 6.0 * k, mid.w, mid.bottom() - (y - 6.0 * k));
        match self.character_tab {
            0 => self.attributes_tab(p, ctx, state, tab_area, out),
            // The skills' list stands just under the tabs.
            1 => self.skills_tab(
                p,
                ctx,
                state,
                Rect::new(
                    mid.x,
                    tabs.bottom() + 3.0 * k,
                    mid.w,
                    mid.bottom() - tabs.bottom() - 3.0 * k,
                ),
                out,
            ),
            _ => {
                row(p, y, "Experience", crate::ui::hud::grouped(state.total_xp));
                y += 20.0 * k;
                row(
                    p,
                    y,
                    "Unassigned",
                    crate::ui::hud::grouped(state.unassigned_xp),
                );
                y += 20.0 * k;
                row(p, y, "Pyreals", crate::ui::hud::grouped(state.pyreals));
                y += 20.0 * k;
                if let Some(b) = state.burden {
                    #[allow(clippy::cast_possible_truncation)]
                    let pct = dereth_primitives::num::to_i32((b * 100.0).round());
                    row(p, y, "Burden", format!("{pct}%"));
                }
                y += 30.0 * k;
                let rest = Rect::new(mid.x, y, mid.w, mid.bottom() - y - 8.0 * k);
                self.profile_extra(p, ctx, state, rest, out);
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Inventory
    // -----------------------------------------------------------------------------------------

    fn inventory(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        // The packs down the left, as the game's pack column: the main pack, then each side pack
        // and focus in the order they are carried.
        let packs: Vec<(Option<dereth_primitives::ObjectId>, Option<&Item>)> =
            std::iter::once((state.player_id, None))
                .chain(
                    state
                        .side_packs
                        .iter()
                        .map(|(pack, _)| (Some(pack.id), Some(pack))),
                )
                .collect();
        if self.inventory_bag >= packs.len() {
            self.inventory_bag = 0;
        }
        let slot = 44.0 * k;
        let gap = 6.0 * k;
        let pitch = slot + gap;
        let foot_top = body.bottom() - FOOT_ROOM * k;
        let column = Rect::new(body.x, body.y, slot + 8.0 * k, foot_top - body.y);
        #[allow(clippy::cast_precision_loss)]
        let column_content = packs.len() as f32 * pitch - gap;
        let column_offset = kit::scroll(p, ctx, column, column_content, &mut self.packs_scroll);
        // The clip leaves the tiles' frames, which stand a little outside them, whole.
        let margin = 4.0 * k;
        p.list.push_clip(Rect::new(
            column.x - margin,
            column.y - margin,
            column.w + 2.0 * margin,
            column.h + 2.0 * margin,
        ));
        let mut pack_rects = Vec::with_capacity(packs.len());
        for (i, (id, pack)) in packs.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let r = Rect::new(
                column.x,
                column.y + i as f32 * pitch - column_offset,
                slot,
                slot,
            );
            pack_rects.push(r);
            if r.bottom() < column.y || r.y > column.bottom() {
                ctx.input.nav.note_hidden(r);
                continue;
            }
            // A drop on a pack puts the item in it.
            if let Some(id) = id {
                ctx.drops.push((
                    r,
                    Some(dereth_client_contract::view::DropTarget::Container(*id).into()),
                ));
            }
            self.pack_tile(p, ctx, state, r, i, *pack, out);
        }
        p.list.pop_clip();
        // The pad's shoulder buttons step through the packs.
        let shown_icon = pack_rects.get(self.inventory_bag).copied();
        ctx.input.nav.steps(pack_rects, self.inventory_bag);
        let pack_id = packs.get(self.inventory_bag).and_then(|(id, _)| *id);
        let (items, capacity) = self.bag(state, self.inventory_bag);
        // The open pack's name over its grid.
        let grid_x = column.right() + 14.0 * k;
        let heading =
            TextStyle::new(Family::Body, 15.0, ctx.colours.heading()).edge(ctx.colours.edge());
        let name = packs
            .get(self.inventory_bag)
            .and_then(|(_, pack)| *pack)
            .map_or("Main Pack", |pack| pack.name.as_str());
        p.text_in(
            &heading,
            Rect::new(grid_x, body.y, body.right() - grid_x, 22.0 * k),
            Align::Left,
            name,
        );
        let cols = 8;
        #[allow(clippy::cast_precision_loss)]
        let grid_w = cols as f32 * pitch;
        let x0 = grid_x + ((body.right() - grid_x) - grid_w).max(0.0) / 2.0;
        let y0 = body.y + 30.0 * k;
        let slots = capacity
            .map_or(items.len(), |c| c as usize)
            .max(items.len());
        // A pack with no room (a focus) draws no slots, and says so.
        let rows = if capacity == Some(0) && items.is_empty() {
            0
        } else {
            slots.div_ceil(cols).max(1)
        };
        if rows == 0 {
            let note =
                TextStyle::new(Family::Body, 13.0, ctx.colours.dim()).edge(ctx.colours.edge());
            p.text(
                &note,
                grid_x + 4.0 * k,
                y0 + 6.0 * k,
                "This holds no items.",
            );
        }
        // Whole rows only: the grid stops above the foot, and scrolls through the rest.
        let room = (foot_top - y0).max(pitch);
        let shown_rows = ((room + gap) / pitch).floor().max(1.0);
        let area = Rect::new(grid_x, y0, body.right() - grid_x, shown_rows * pitch - gap);
        #[allow(clippy::cast_precision_loss)]
        let offset = kit::scroll(
            p,
            ctx,
            area,
            rows as f32 * pitch - gap,
            &mut self.inventory_scroll,
        );
        // For the pad the pack's contents are what the window is for: it opens inside them, and
        // a step back goes to the pack's icon.
        if rows > 0 {
            let input = &mut *ctx.input;
            input.nav.list_inside(area, shown_icon, &input.occluders);
        }
        // A drop in the grid's empty space goes into this pack.
        if let Some(pack) = pack_id {
            ctx.drops.push((
                area,
                Some(dereth_client_contract::view::DropTarget::Container(pack).into()),
            ));
        }
        let margin = 4.0 * k;
        p.list.push_clip(Rect::new(
            area.x - margin,
            area.y - margin,
            area.w + 2.0 * margin,
            area.h + 2.0 * margin,
        ));
        for i in 0..rows * cols {
            let (c, r) = (i % cols, i / cols);
            #[allow(clippy::cast_precision_loss)]
            let rect = Rect::new(
                x0 + c as f32 * pitch,
                y0 + r as f32 * pitch - offset,
                slot,
                slot + 2.0 * k,
            );
            if rect.bottom() < area.y {
                ctx.input.nav.note_hidden(rect);
                continue;
            }
            if rect.y > area.bottom() {
                ctx.input.nav.note_hidden(rect);
                break;
            }
            if let Some(pack) = pack_id {
                ctx.drops.push((
                    rect,
                    Some(Self::slot_drop(ctx, state, pack, items, i).into()),
                ));
            }
            // The pad's focus starts on the first thing in the pack.
            if i == 0 {
                ctx.input.nav.home(rect);
            }
            self.item_tile(p, ctx, state, rect, items.get(i), out);
        }
        p.list.pop_clip();
        // The splitter, while a stack is selected, wherever it is.
        self.split_row(
            p,
            ctx,
            Rect::new(
                body.x + 8.0 * k,
                body.bottom() - 64.0 * k,
                body.w - 16.0 * k,
                24.0 * k,
            ),
            out,
        );
        // The burden gauge and the count at the foot, clear of the window's frame.
        let small = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let foot = body.bottom() - 30.0 * k;
        p.text(
            &small,
            body.x + 8.0 * k,
            foot,
            &capacity.map_or_else(
                || format!("{} items", items.len()),
                |c| format!("Items {} of {c}", items.len()),
            ),
        );
        if let Some(b) = state.burden {
            p.text(&small, body.right() - 250.0 * k, foot, "Burden");
            // The gauge centred on the label's line.
            let lh = p.line_height(&small);
            let gauge = Rect::new(
                body.right() - 190.0 * k,
                foot + (lh - 12.0 * k) / 2.0,
                180.0 * k,
                12.0 * k,
            );
            kit::gauge(
                p,
                gauge,
                b / 3.0,
                if b > 1.0 { 0xFFFF_9040 } else { 0xFF6E_CB5C },
            );
            if ctx.input.hover(&gauge) {
                self.tip = Some(("Burden".into(), vec![burden_percent(b)]));
            }
        }
    }

    /// Bag `index`'s items and capacity as the game has it: the player's own for the main pack
    /// (bag 0), each side pack's or focus's for itself.
    fn bag<'s>(&self, state: &'s GameState, index: usize) -> (&'s [Item], Option<u32>) {
        if index == 0 {
            return (&state.pack, state.pack_capacity);
        }
        state
            .side_packs
            .get(index - 1)
            .map_or((&[][..], None), |(pack, items)| {
                let capacity = pack
                    .decoration
                    .and_then(|d| u32::try_from(d.items_capacity).ok());
                (items.as_slice(), capacity)
            })
    }

    /// One pack in the pack column: its own picture (the main pack's word on an empty tile),
    /// how full it is along its foot, and a frame while it is the one open. A click opens it, a
    /// drag carries it (to the shortcut bar, a vendor, the salvage window, another pack), a
    /// double-click uses it and a right-click examines it.
    #[allow(clippy::too_many_arguments)]
    fn pack_tile(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        r: Rect,
        index: usize,
        pack: Option<&Item>,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let (items, capacity) = self.bag(state, index);
        let held = items.len();
        // The main pack is the player, drawn as the game's backpack and carried like any pack.
        let pack = pack.or(if index == 0 {
            state.main_pack.as_ref()
        } else {
            None
        });
        let icon: Option<Sprite> = pack.and_then(|i| p.art.ac_item(i));
        // A pack being moved is carried: its place stands empty, and it is drawn lifted where
        // the focus is.
        let moving = pack.is_some_and(|it| self.moving_pack == Some(it.id));
        let pressed = kit::icon_slot(p, ctx, r, icon.as_ref().filter(|_| !moving), WHITE)
            // With the pad, confirming on a pack that holds nothing (a focus) does nothing.
            && !(ctx.input.pad.mode.is_some() && capacity == Some(0) && self.moving_pack.is_none());
        if moving {
            self.moving_icon = icon;
        }
        if pack.is_some_and(items::waiting) {
            p.fill(r, items::WAITING_WASH);
        }
        if icon.is_none() && index == 0 {
            let word =
                TextStyle::new(Family::Body, 12.0, ctx.colours.text()).edge(ctx.colours.edge());
            p.text_in(&word, r, Align::Centre, "Main");
        }
        if let Some(c) = capacity.filter(|c| *c > 0) {
            #[allow(clippy::cast_precision_loss)]
            let full = (held as f32 / c as f32).min(1.0);
            let bar = Rect::new(r.x + 4.0 * k, r.bottom() - 6.0 * k, r.w - 8.0 * k, 3.0 * k);
            p.fill(bar, 0xC000_0000);
            p.fill(
                Rect::new(bar.x, bar.y, bar.w * full, bar.h),
                if full >= 1.0 {
                    0xFFE0_6040
                } else {
                    0xFFC8_B27A
                },
            );
        }
        if index == self.inventory_bag {
            p.outline(r, 2.0 * k, 0xFFF0_C860);
        }

        if let Some(it) = pack {
            if state.target.as_ref().is_some_and(|t| t.id == it.id) {
                kit::selected_tile(p, r);
            }
            if ctx.input.double_clicked(&r) {
                *ctx.drag = None;
                // The main pack is the player, who is not used.
                if index != 0 {
                    out.requests.push(UiRequest::Use(it.id));
                }
            } else if ctx.input.right_clicked(&r) {
                // In gamepad mode, X on a pack opens what can be done with it.
                if ctx.input.pad.mode.is_some() {
                    self.open_item_menu(it, r);
                } else {
                    out.requests.push(UiRequest::Select(it.id));
                    out.requests.push(UiRequest::Examine(it.id));
                }
            }
        }
        // A pack being moved is put down at the place pressed, as a drop on the row of packs.
        if pressed {
            if let Some(moving) = self.moving_pack.take() {
                out.requests.push(pack_move(state, moving, index));
                return;
            }
        }
        if pressed {
            self.inventory_bag = index;
            self.inventory_scroll = 0.0;
            if let Some(it) = pack {
                if state.targeting {
                    out.requests.push(UiRequest::ExecuteTargetItem(it.id));
                } else if ctx.input.pad.mode.is_none() {
                    // With the pad, confirming on a pack shows it; only the mouse drags it.
                    *ctx.drag = Some(crate::ui::Drag {
                        item: it.id,
                        look: Some(it.clone()),
                        spell_look: None,
                        from_shortcut: None,
                        icon: it.ac_icon,
                        origin: ctx.input.mouse,
                        active: false,
                        on_click: Some(UiRequest::Select(it.id)),
                        spell: None,
                        from_spell_slot: None,
                        component: false,
                        stance: None,
                    });
                }
            }
        }
        if ctx.input.hover(&r) {
            let name = if index == 0 {
                "Main Pack".to_owned()
            } else {
                pack.map_or_else(String::new, |it| it.name.clone())
            };
            let line = match capacity.filter(|c| *c > 0) {
                Some(c) => format!("{held} / {c}"),
                None if held == 0 => "Holds nothing".to_owned(),
                None => format!("{held} items"),
            };
            self.tip = Some((name, vec![line]));
        }
    }

    // -----------------------------------------------------------------------------------------
    // Spellbook
    // -----------------------------------------------------------------------------------------

    // -----------------------------------------------------------------------------------------
    // Social, allegiance, map
    // -----------------------------------------------------------------------------------------

    fn map(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, state: &GameState, body: Rect) {
        use dereth_ui_screens::mapradar::map::{
            graphic, place_marker_on_map, SHIPPED_NOTE_BINDING,
        };
        let k = p.scale;
        p.fill(body, 0xFF2A_2418);
        // The picture, as large as the window holds it at its own shape.
        let (iw, ih) = (257.0, 267.0);
        let fit = ((body.w - 24.0 * k) / iw).min((body.h - 48.0 * k) / ih);
        let area = Rect::new(
            body.x + (body.w - iw * fit) / 2.0,
            body.y + 8.0 * k,
            iw * fit,
            ih * fit,
        );
        match p.art.ac_icon(graphic::MAP_IMAGE.0) {
            Some(map) => p.sprite(&map, area, WHITE),
            None => p.fill(area, 0xFFC8_B488),
        }
        // The pad's focus starts on the map itself, none of its places chosen.
        {
            let input = &mut *ctx.input;
            input
                .nav
                .note(area, crate::ui::nav::Kind::Plain, &input.occluders);
            input.nav.home(area);
        }
        let style = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
        // The places the map names: framed and named while the pointer is on one, as the game's
        // map notes are.
        for note in dereth_client_contract::panels::map::notes(state.map_profile) {
            #[allow(clippy::cast_precision_loss)]
            let r = Rect::new(
                area.x + note.x as f32 * fit,
                area.y + note.y as f32 * fit,
                note.w as f32 * fit,
                note.h as f32 * fit,
            );
            if ctx.over(&r) {
                p.fill(r, 0x30FF_E8A0);
                p.outline(r, 1.5 * k, 0xFFF0_C860);
                self.tip = Some((note.name.to_owned(), Vec::new()));
            }
        }
        if let Some((n, e)) = state.coords {
            let (_, _, markers) = SHIPPED_NOTE_BINDING;
            let (mx, my) = place_marker_on_map(markers, e, n, (0, 0));
            #[allow(clippy::cast_precision_loss)]
            let (px, py) = (area.x + mx as f32 * fit, area.y + my as f32 * fit);
            let s = 12.0 * k;
            match p.art.ac_icon(graphic::PLAYER_ICON.0) {
                Some(icon) => p.sprite(&icon, Rect::new(px - s, py - s, 2.0 * s, 2.0 * s), WHITE),
                None => p.fill(Rect::new(px - s / 2.0, py - s / 2.0, s, s), 0xFF40_E040),
            }
            let label = format!(
                "{:.1}{} {:.1}{}",
                n.abs(),
                if n >= 0.0 { "N" } else { "S" },
                e.abs(),
                if e >= 0.0 { "E" } else { "W" }
            );
            p.text_in(
                &style,
                Rect::new(body.x, area.bottom() + 6.0 * k, body.w, 24.0 * k),
                Align::Centre,
                &label,
            );
        }
    }
}

impl Windows {
    /// The character's own options: a switch for each, under its section's heading, each sent to
    /// the server as it changes, with the game's help on hover.
    fn character_options_tab(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let section =
            TextStyle::new(Family::Heading, 18.4, ctx.colours.heading()).edge(ctx.colours.edge());
        let label = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let area = Rect::new(body.x, body.y + 36.0 * k, body.w, body.h - 40.0 * k);
        let row_h = 28.0 * k;
        // The options the world's era has, under the headings that keep any.
        let sections: Vec<(String, Vec<CharacterOption>)> = self
            .character_options
            .iter()
            .map(|(h, options)| {
                let shown: Vec<CharacterOption> = options
                    .iter()
                    .filter(|o| o.needs.met(state.era.as_ref()))
                    .cloned()
                    .collect();
                (h.clone(), shown)
            })
            .filter(|(_, options)| !options.is_empty())
            .collect();
        let rows: usize = sections.iter().map(|(_, o)| o.len() + 1).sum();
        #[allow(clippy::cast_precision_loss)]
        let content = row_h * rows as f32;
        let offset = kit::scroll(p, ctx, area, content, &mut self.character_scroll);
        p.list.push_clip(area);
        let mut y = area.y - offset;
        for (heading, options) in &sections {
            if y + row_h >= area.y && y <= area.bottom() {
                p.text(&section, area.x + 8.0 * k, y + 4.0 * k, heading);
            }
            y += row_h;
            for o in options {
                if y + row_h >= area.y && y <= area.bottom() {
                    let cb = Rect::new(area.x + 16.0 * k, y + 2.0 * k, 24.0 * k, 24.0 * k);
                    if let Some(on) = kit::checkbox(p, ctx, cb, state.option(o.option)) {
                        out.requests.push(UiRequest::SetPlayerOption(o.option, on));
                    }
                    let text = Rect::new(cb.right() + 10.0 * k, y, area.w - 60.0 * k, row_h);
                    p.text_in(&label, text, Align::Left, &o.label);
                    if ctx.input.hover(&text) && !o.help.is_empty() {
                        self.tip = Some((o.label.clone(), vec![o.help.clone()]));
                    }
                }
                y += row_h;
            }
        }
        p.list.pop_clip();
    }
}

#[cfg(test)]
mod layering_tests {
    //! Behaviour: none (experimental Horizon interface)
    use super::*;
    use crate::ui::input::InputFrame;

    #[test]
    fn a_press_on_a_window_under_another_brings_it_to_the_top() {
        let mut w = Windows::default();
        w.open(WindowId::Inventory, 0.0);
        w.open(WindowId::Character, 0.0);
        let rects = w.rects(1.0);
        assert_eq!(rects.last().map(|(id, _)| *id), Some(WindowId::Character));
        let (_, inv) = rects[0];
        let (_, chara) = rects[1];
        // A point of the inventory that the character window does not cover.
        let x = if inv.right() > chara.right() {
            inv.right() - 2.0
        } else {
            inv.x + 2.0
        };
        let input = InputFrame {
            mouse: (x, inv.y + 50.0),
            pressed: [true, false, false],
            ..InputFrame::default()
        };
        w.raise_pressed(&input, 1.0);
        assert_eq!(
            w.rects(1.0).last().map(|(id, _)| *id),
            Some(WindowId::Inventory)
        );
        // A press on no window changes nothing.
        let input = InputFrame {
            mouse: (-500.0, -500.0),
            pressed: [true, false, false],
            ..InputFrame::default()
        };
        w.raise_pressed(&input, 1.0);
        assert_eq!(
            w.rects(1.0).last().map(|(id, _)| *id),
            Some(WindowId::Inventory)
        );
    }
}

/// A stack's count as a tile shows it: whole up to 999, then in thousands (`1.5K`, `25K`) and
/// millions (`1.2M`).
#[must_use]
pub fn short_count(n: u32) -> String {
    let short = |v: u32, unit: u32, suffix: &str| {
        if v < 10 * unit {
            let tenths = v / (unit / 10);
            if tenths.is_multiple_of(10) {
                format!("{}{suffix}", tenths / 10)
            } else {
                format!("{}.{}{suffix}", tenths / 10, tenths % 10)
            }
        } else {
            format!("{}{suffix}", v / unit)
        }
    };
    match n {
        0..=999 => n.to_string(),
        1_000..=999_999 => short(n, 1_000, "K"),
        _ => short(n, 1_000_000, "M"),
    }
}

#[cfg(test)]
mod count_tests {
    //! Behaviour: none (experimental Horizon interface)
    use super::short_count;

    #[test]
    fn a_stack_of_four_digits_or_more_shows_in_thousands_or_millions() {
        for (n, shown) in [
            (1, "1"),
            (999, "999"),
            (1_000, "1K"),
            (1_500, "1.5K"),
            (9_999, "9.9K"),
            (10_000, "10K"),
            (25_431, "25K"),
            (999_999, "999K"),
            (1_234_567, "1.2M"),
            (12_000_000, "12M"),
        ] {
            assert_eq!(short_count(n), shown, "{n}");
        }
    }
}

#[cfg(test)]
mod slot_tests {
    //! Behaviour: none (experimental Horizon interface)
    use super::{placeholder, HELD_SLOTS, LEFT_SLOTS, RIGHT_SLOTS, SIGIL_SLOTS, TOP_SLOTS};
    use dereth_rules::slots::loc;

    fn slots_of(worn: u32) -> Vec<&'static str> {
        LEFT_SLOTS
            .iter()
            .chain(RIGHT_SLOTS.iter().flatten())
            .chain(TOP_SLOTS.iter())
            .chain(HELD_SLOTS.iter())
            .chain(SIGIL_SLOTS.iter())
            .filter(|(_, mask)| worn & mask != 0)
            .map(|(name, _)| *name)
            .collect()
    }

    #[test]
    fn the_slots_are_the_paper_doll_s_locations_and_a_shirt_is_not_chest_armour() {
        let mut ours: Vec<u32> = LEFT_SLOTS
            .iter()
            .chain(RIGHT_SLOTS.iter().flatten())
            .chain(TOP_SLOTS.iter())
            .chain(HELD_SLOTS.iter())
            .chain(SIGIL_SLOTS.iter())
            .map(|(_, m)| *m)
            .collect();
        let mut doll: Vec<u32> = dereth_rules::slots::PAPERDOLL_REGIONS
            .iter()
            .map(|(_, m, _)| *m)
            .collect();
        ours.sort_unstable();
        doll.sort_unstable();
        assert_eq!(ours, doll);
        assert_eq!(slots_of(loc::CHEST_WEAR | loc::UPPER_ARM_WEAR), ["Shirt"]);
        assert_eq!(slots_of(loc::CHEST_ARMOR), ["Chest Armour"]);
    }

    #[test]
    fn every_slot_shows_a_placeholder_while_empty() {
        for (name, mask) in LEFT_SLOTS
            .iter()
            .chain(RIGHT_SLOTS.iter().flatten())
            .chain(TOP_SLOTS.iter())
            .chain(HELD_SLOTS.iter())
            .chain(SIGIL_SLOTS.iter())
        {
            assert!(placeholder(*mask).is_some(), "{name}");
        }
    }
}

/// The request that puts pack `moving` down at place `index` of the pack column (0 the main
/// pack's, then the packs beside it): a drop on the row of packs at that place, as the game takes
/// a pack dragged along it, which asks the server to put the pack in the character at that place
/// among its packs.
#[must_use]
pub fn pack_move(
    state: &GameState,
    moving: dereth_primitives::ObjectId,
    index: usize,
) -> UiRequest {
    let packs: Vec<dereth_primitives::ObjectId> =
        state.side_packs.iter().map(|(p, _)| p.id).collect();
    let at = index.saturating_sub(1).min(packs.len());
    // Moved further along, it takes the place it is put on, the packs between moving up: the
    // game puts a pack in front of the one it is dropped on once it has left its old place.
    let old = packs.iter().position(|p| *p == moving);
    let place = if old.is_some_and(|o| o < at) {
        (at + 1).min(packs.len())
    } else {
        at
    };
    UiRequest::DragDrop {
        item: moving,
        target: dereth_client_contract::view::DropTarget::ItemListSlot {
            container: state.player_id.unwrap_or_default(),
            under: packs.get(at).copied(),
            index: u32::try_from(place).unwrap_or(0),
            num_ui_items: u32::try_from(packs.len()).unwrap_or(0),
            dragged_is_container: true,
            container_list: true,
        },
    }
}
