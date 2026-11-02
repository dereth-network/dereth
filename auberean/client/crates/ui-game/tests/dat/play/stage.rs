//! A play screen in the world for the trade, shop, book, pet, story and chat panels' tests: the
//! player with a sword and a stack of ore in the main pack, the clock moving a twentieth of a
//! second a frame; and the install's records with a string table the data lacks.

use std::sync::{Arc, Mutex};

use auberean_text::StringInfo;
use auberean_ui::types::button_id;
use auberean_ui::{Assets, Handle, Token, Ui};
use auberean_ui_game::gameplay::{GamePlay, Notice, Record, Request, View};
use auberean_ui_game::inventory::{Container, Contents, Facts, ItemFacts};
use auberean_ui_game::shop::Facts as ShopFacts;
use auberean_ui_game::toolbar::action as tb;

use crate::common::records;

pub const PLAYER: u64 = 0x5000_0000_0000_0001;
pub const PARTNER: u64 = 0x5000_0000_0000_0002;
pub const SWORD: u64 = 0x5000_0000_0000_0010;
pub const ORE: u64 = 0x5000_0000_0000_0011;
pub const ICON: u32 = 0x4100_0ADE;
pub const ORE_RECORD: u32 = 0x4700_0123;

fn item(name: &str, quantity: u32, stack_size: u32) -> ItemFacts {
    ItemFacts {
        quantity,
        icon: ICON,
        stackable: stack_size > 1,
        stack_size,
        name: name.to_owned(),
        had: true,
        ..ItemFacts::default()
    }
}

/// The player's pack: a sword, twelve ore (stacks of up to fifty).
#[must_use]
pub fn inventory() -> Facts {
    let pack = vec![SWORD, ORE];
    let slots = vec![(0, 2), (2, 0), (2, 0), (2, 0)];
    let segments = slots
        .iter()
        .map(|(first, count)| Contents {
            items: pack.clone(),
            first: *first,
            count: *count,
        })
        .collect();
    let mut f = Facts {
        version: 1,
        player: PLAYER,
        segments: slots.clone(),
        coin_stack: 1000,
        ..Facts::default()
    };
    f.containers.insert(
        PLAYER,
        Container {
            size: 78,
            segments,
            slots,
        },
    );
    f.items.insert(SWORD, item("Sword", 1, 1));
    let mut ore = item("Ore", 12, 50);
    ore.data_id = ORE_RECORD;
    f.items.insert(ORE, ore);
    f
}

/// A 64-bit id as the scripts' notices hold one: high word, then low.
#[must_use]
pub fn long(v: u64) -> [u32; 2] {
    let hi = u32::try_from(v >> 32).unwrap_or(0);
    let lo = u32::try_from(v & 0xFFFF_FFFF).unwrap_or(0);
    [hi, lo]
}

/// A notice about the player.
#[must_use]
pub fn said(native: u32, words: Vec<u32>, records: Vec<Option<Record>>) -> Notice {
    Notice {
        native,
        about: Some(PLAYER),
        words,
        records,
    }
}

/// A question answered.
#[must_use]
pub fn answer(id: u32, yes: bool) -> Notice {
    Notice {
        native: auberean_ui_game::dialog::notice::CLOSE_CONFIRMATION_DIALOG,
        words: vec![id, u32::from(yes)],
        ..Notice::default()
    }
}

/// A dialog the panels asked for: its number and its text.
#[must_use]
pub fn dialog(out: &[Request]) -> Option<(u32, String)> {
    out.iter().find_map(|r| match r {
        Request::Dialog(d) => Some((d.id, d.text.clone())),
        _ => None,
    })
}

/// The install's records, with one string table the data lacks: a reference to it waits until
/// the test says the table came, then writes as the server's text.
pub struct Lacking {
    records: Arc<auberean_ui::DatAssets>,
    waiting: Mutex<bool>,
}

pub const LACKING: u32 = 0x2502_0001;

impl Lacking {
    /// The install's records, the table not yet come.
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            records: records(),
            waiting: Mutex::new(true),
        })
    }

    /// The table came.
    pub fn arrive(&self) {
        *self.waiting.lock().unwrap() = false;
    }
}

impl Assets for Lacking {
    fn layout(&self, id: u32) -> Option<Arc<auberean_assets::ui::UiLayout>> {
        self.records.layout(id)
    }
    fn image(&self, id: u32) -> Option<Arc<auberean_ui::Image>> {
        self.records.image(id)
    }
    fn font(&self, id: u32) -> Option<Arc<auberean_assets::ui::Font>> {
        self.records.font(id)
    }
    fn string(&self, table: u32, token: u32) -> Option<String> {
        self.records.string(table, token)
    }
    fn data_id_by_enum(&self, value: u32, mapping: u32) -> u32 {
        self.records.data_id_by_enum(value, mapping)
    }
    fn text(&self, table: u32, token: u32) -> Option<String> {
        self.records.text(table, token)
    }
    fn write(&self, info: &StringInfo) -> Option<String> {
        self.records.write(info)
    }
    fn write_valid(&self, info: &StringInfo) -> Option<String> {
        if info.table == LACKING {
            return Some(String::new());
        }
        self.records.write_valid(info)
    }
    fn element_text(&self, info: &StringInfo) -> Vec<u16> {
        if info.table == LACKING {
            return "From the server\n".encode_utf16().collect();
        }
        self.records.element_text(info)
    }
    fn start_string_download(&self, info: &StringInfo) -> Option<u32> {
        (info.table == LACKING).then_some(1)
    }
    fn string_download_waiting(&self, _download: u32) -> bool {
        *self.waiting.lock().unwrap()
    }
}

pub struct World {
    pub ui: Ui,
    pub play: GamePlay,
    pub t: f64,
    pub inventory: Facts,
    pub shop: ShopFacts,
    pub options: auberean_ui_game::options::Values,
}

impl World {
    #[must_use]
    pub fn new() -> Self {
        Self::over(records())
    }

    /// The same over other records.
    #[must_use]
    pub fn over(assets: Arc<dyn Assets + Send + Sync>) -> Self {
        let mut ui = Ui::new(assets, 1024, 768);
        let mut out = Vec::new();
        let mut play = GamePlay::new(&mut ui, &mut out);
        play.login_finished(&mut ui, &mut out);
        let mut w = Self {
            ui,
            play,
            t: 0.0,
            inventory: inventory(),
            shop: ShopFacts::default(),
            options: auberean_ui_game::options::Values::default(),
        };
        w.tick(&[]);
        w
    }

    /// A frame: the clock on, the notices heard, what the panels asked.
    pub fn tick(&mut self, notices: &[Notice]) -> Vec<Request> {
        self.t += 0.05;
        self.ui.use_time(self.t);
        let view = View {
            inventory: Some(&self.inventory),
            shop: Some(&self.shop),
            options: Some(self.options),
            everyone: notices,
            ..View::default()
        };
        let mut out = Vec::new();
        self.play.tick(&mut self.ui, &view, notices, &mut out);
        out
    }

    /// Frames for `seconds` of the clock, what the panels asked.
    pub fn wait(&mut self, seconds: f64) -> Vec<Request> {
        let mut out = Vec::new();
        let until = self.t + seconds;
        while self.t < until {
            out.extend(self.tick(&[]));
        }
        out
    }

    #[must_use]
    pub fn el(&self, name: &str) -> Handle {
        self.ui
            .element(Token::of(name))
            .unwrap_or_else(|| panic!("{name} made"))
    }

    #[must_use]
    pub fn text(&self, h: Handle) -> String {
        self.ui.text(h).unwrap_or_default()
    }

    #[must_use]
    pub fn middle(&self, h: Handle) -> (i32, i32) {
        let r = self.ui.screen_rect(h).expect("placed");
        ((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2)
    }

    /// A press with a button and its release on an element.
    pub fn press(&mut self, h: Handle, button: u32) -> Vec<Request> {
        let (x, y) = self.middle(h);
        self.ui.pointer_move(x, y);
        let mut out = self.tick(&[]);
        for down in [true, false] {
            self.ui.pointer_button(button, down);
            out.extend(self.tick(&[]));
        }
        out
    }

    pub fn click(&mut self, h: Handle) -> Vec<Request> {
        self.press(h, button_id::LEFT)
    }

    /// An element dragged onto another.
    pub fn drag(&mut self, from: Handle, to: Handle) -> Vec<Request> {
        let (x, y) = self.middle(from);
        self.ui.pointer_move(x, y);
        let mut out = self.tick(&[]);
        self.ui.pointer_button(button_id::LEFT, true);
        out.extend(self.tick(&[]));
        self.ui.pointer_move(x + 8, y + 8);
        out.extend(self.tick(&[]));
        assert!(self.ui.dragged().is_some(), "dragging");
        let (tx, ty) = self.middle(to);
        self.ui.pointer_move(tx, ty);
        out.extend(self.tick(&[]));
        self.ui.pointer_button(button_id::LEFT, false);
        out.extend(self.tick(&[]));
        out
    }

    /// The inventory window opened (unless it shows), and an item's icon in the main pack.
    pub fn pack_icon(&mut self, item: u64) -> Handle {
        let open = self
            .ui
            .element(Token::of("InventoryField"))
            .is_some_and(|h| self.ui.shown(h));
        if !open {
            self.ui.key(tb::INVENTORY_PANEL, true);
            self.tick(&[]);
            self.ui.key(tb::INVENTORY_PANEL, false);
            self.tick(&[]);
        }
        self.play
            .child::<auberean_ui_game::toolbar::Toolbar>()
            .and_then(|t| t.child::<auberean_ui_game::inventory::Inventory>())
            .and_then(|i| i.backpack())
            .and_then(|b| b.packs()[0].item_display(item))
            .and_then(auberean_ui_game::inventory::item::ItemCell::icon)
            .expect("the item's icon")
    }

    /// The stack splitter asking how many: shown, its label; answered with an amount (typed, then
    /// OK), what the panels asked.
    pub fn split(&mut self, amount: u32) -> (String, Vec<Request>) {
        let field = self.el("StackableField");
        assert!(self.shown(field), "the stack splitter asks how many");
        let label = self.text(self.under(field, "Label"));
        let entry = self.under(field, "AmountEntry");
        self.ui.set_text(entry, &amount.to_string());
        let out = self.click(self.under(field, "StackOkButton"));
        (label, out)
    }

    #[must_use]
    pub fn shown(&self, h: Handle) -> bool {
        self.ui.shown(h)
    }

    #[must_use]
    pub fn state(&self, h: Handle) -> Token {
        self.ui.node(h).map_or(Token(0), |n| n.state)
    }

    #[must_use]
    pub fn ghosted(&self, h: Handle) -> bool {
        self.ui
            .node(h)
            .is_some_and(|n| n.bool_attr(auberean_ui::token::attr::GHOSTED))
    }

    /// An element under another, by name.
    #[must_use]
    pub fn under(&self, h: Handle, name: &str) -> Handle {
        self.ui
            .child_recursive(h, Token::of(name))
            .unwrap_or_else(|| panic!("{name} under it"))
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}
