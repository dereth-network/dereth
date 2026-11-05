//! The inventory: the toolbar's inventory window (the pack in four packs, the money bag, what the
//! player wears on its paper doll), the window an opened container shows (a corpse, a chest), the
//! panel that splits a stack, and dragging items between them and into the world.
//!
//! The panels, as the play screen and the toolbar make them:
//!
//! - [`Inventory`]: the toolbar's inventory window: shown and hidden by its button, its key and
//!   its close button, brought in front of the other toolbar windows when it shows or is pressed;
//!   it holds the [`paper_doll`], the [`backpack`], the [`money_bag`] and the [`items_3d`] panel
//!   (items dropped in the world).
//! - [`external::ExternalContainer`]: an opened container's window, under the container on the
//!   screen, with its take-all and melt-all buttons.
//! - [`stack::StackSplitter`]: how many of a stack to move.
//!
//! What each shows comes from the player's scripts ([`Facts`]: what they answer about the pack,
//! the open containers and each item in them) and their notices about the inventory; what the
//! player does with them becomes the scripts' inventory functions ([`Request`]). The panels tell
//! each other things through the play screen (a stack to split, the split's amount, a drag from
//! another panel): [`notice`].
//!
//! - [`item::ItemCell`]: an item in a cell: its icon, its name as the tooltip, waiting for the
//!   server, selected, and a click on it selects, uses or examines it.
//! - [`container::ContainerCells`]: a container's cells: its items in their slots, an item
//!   dropped on a cell moved there (a stack split first when it stacks), coins dropped taken out of
//!   the money bag.

use std::any::Any;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};

use auberean_ui::framework::FrameId;
use auberean_ui::token::msg;
use auberean_ui::{Handle, Mail, Token, Ui};

use crate::gameplay::{self, Child, Notice, View};

pub mod backpack;
pub mod container;
pub mod external;
pub mod item;
pub mod items_3d;
pub mod money_bag;
pub mod paper_doll;
pub mod stack;

/// The toolbar's layout's enumeration value (the inventory window, its cells, the paper doll).
pub const TOOLBAR_LAYOUT_ENUM: u32 = crate::shortcuts::TOOLBAR_LAYOUT_ENUM;
/// The play layout's (the external container, the stack splitter).
pub const PLAY_LAYOUT_ENUM: u32 = gameplay::PLAY_LAYOUT_ENUM;

/// The kind of shortcut an item's icon is (what a drop checks).
pub const ITEM_SHORTCUT: u32 = 0x4000_0002;

/// No object.
pub const NO_OBJECT: u64 = 0;

/// The attributes the panels give an item's icon and read from a dropped one.
pub mod attr {
    use auberean_ui::Token;
    /// What kind of shortcut it is, and the object it stands for.
    pub const SHORTCUT_TYPE: Token = Token::of("ShortcutType");
    pub const SHORTCUT_DATA: Token = Token::of("ShortcutData");
    /// The container it came from, whether it is worn, its slot there.
    pub const FROM_ID: Token = Token::of("FromID");
    pub const EQUIPPED: Token = Token::of("Equipped");
    pub const OLD_INDEX: Token = Token::of("OldIndex");
    /// The panel it was dragged out of, when that is not a container (the shortcut bar).
    pub const SOURCE_CONTEXT: Token = Token::of("SourceContext");
    /// How many slots a container shows (on the container's element).
    pub const NUM_SLOTS: Token = Token::of("num_slots");
    /// The cursor an item's icon shows while an item waits to be used on a target.
    pub const USAGE_CURSOR: Token = Token::of("usage_cursor");
    pub const USAGE_CURSOR_X: Token = Token::of("usage_cursor_x");
    pub const USAGE_CURSOR_Y: Token = Token::of("usage_cursor_y");
}

/// The messages the panels hear besides the framework's own.
pub mod message {
    use auberean_ui::Token;
    /// An item dropped on a container (the parameter: 1 taken, 0 not), and an element not
    /// dragable pressed and pulled.
    pub const STOP_DRAG_ITEM: Token = Token::of("stop_drag_item");
    pub const NOT_DRAGABLE: Token = Token::of("not_dragable");
    /// A toolbar window comes in front of the others.
    pub const BRING_TO_FRONT: Token = Token::of("bring_to_front");
}

/// The notices the panels hear: the player's scripts' (by the native that sends each, its package
/// then its number) and the panels' own to each other (numbered below `0x10000`).
pub mod notice {
    /// The player's worn items changed: all of them (`0x4000_0001`), one worn, one taken off, a
    /// request about one failed, one under way.
    pub const UPDATE_INVENTORY: u32 = 0x008B << 16 | 0x0001;
    /// A container's contents changed: all of them, one added at a slot, one removed, a request
    /// about one failed, one under way.
    pub const UPDATE_CONTAINER: u32 = 0x008B << 16 | 0x0002;
    /// A container opened (its state, not 0) or closed for the player.
    pub const CONTAINER_STATE_UPDATE: u32 = 0x008B << 16 | 0x0003;
    /// An item changed.
    pub const ITEM_UPDATE: u32 = 0x008B << 16 | 0x0004;
    /// The pack grew or shrank.
    pub const UPDATE_BACKPACK: u32 = 0x008B << 16 | 0x0005;
    /// The player's money changed (the amount).
    pub const UPDATE_MONEY: u32 = 0x008C << 16 | 0x0002;
    /// The player's selection changed (the new one).
    pub const SELECTION_CHANGED: u32 = 0x0085 << 16 | 0x0003;
    /// An item waits to be used on a target (1) or no longer does (0).
    pub const SET_TARGETING_POINTER: u32 = 0x0092 << 16 | 0x0001;
    /// The crafting panel opens.
    pub const OPEN_INGREDIENT_PANEL: u32 = 0x0088 << 16 | 0x0005;

    /// The panels' own: a confirmation closed (the dialog, yes or no).
    pub const CLOSE_CONFIRMATION: u32 = 0x436;
    /// Split a stack: the context, the amount offered and the most, the object to watch (high and
    /// low words), the other object (high and low), the item whose name labels it (high and low;
    /// none for coins).
    // D2-00317
    pub const INIT_STACK: u32 = 0x458;
    /// The split asked for in a context is called off.
    // D2-00310
    pub const CLOSE_STACK: u32 = 0x459;
    /// The split in a context is done: the amount.
    // D2-00324
    pub const STACK_DONE: u32 = 0x45A;
    /// The examination panel opens.
    pub const OPEN_EXAMINATION: u32 = 0x45F;
    /// An item from another panel dropped in a slot of the pack: the item (high and low), the
    /// slot, the panel it came from.
    // D2-00316
    pub const DRAG_FROM: u32 = 0x493;
}

/// The string table the panels' messages are in, by enumeration value.
pub const MESSAGES: u32 = crate::flow::MESSAGES;

/// What the inventory panels ask of the client: each is one of the player's scripts' inventory
/// functions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Move `quantity` of an item from slot `old_slot` of `from` to slot `new_slot` of `to`.
    Put {
        item: u64,
        from: u64,
        to: u64,
        old_slot: u32,
        new_slot: u32,
        quantity: u32,
    },
    /// Wear an item at a place.
    Wear {
        item: u64,
        location: u32,
    },
    /// Take a worn item off into a slot of the pack.
    TakeOff {
        item: u64,
        slot: u32,
    },
    /// Melt `quantity` of an item (from its slot in `from`) into money.
    IntoMoney {
        item: u64,
        quantity: u32,
        slot: u32,
        from: u64,
    },
    /// Take coins out of the money bag into a slot of a container.
    OutOfMoney {
        amount: u32,
        slot: u32,
        container: u64,
    },
    /// Take everything out of a container; melt everything in it.
    TakeAll {
        container: u64,
    },
    MeltAll {
        container: u64,
    },
    /// A container closed for the player.
    Close {
        container: u64,
    },
    /// Select an object.
    Select(u64),
    /// The help panel at a page (a string table's entry and the table's enumeration value).
    Help {
        entry: Token,
        table: u32,
    },
    /// Find the part of the paper doll's figure under the pointer; the answer comes in
    /// [`Facts::picked`].
    Pick,
    /// Find what is under the pointer in the world; the answer comes in [`Facts::picked`].
    PickWorld,
    /// The player's figure as it looks, in a viewport's scene: the whole figure (the paper doll),
    /// or its head and shoulders (`portrait`, a consignment's).
    ShowDoll {
        viewport: Handle,
        portrait: bool,
    },
}

/// What the player's scripts answer about the inventory: the player's pack in its segments and
/// what it wears, the containers it has open, each item in them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    /// Counts the times it changed.
    pub version: u64,
    pub player: u64,
    /// What the player and each open container holds.
    pub containers: BTreeMap<u64, Container>,
    /// The pack's segments: first slot and size.
    pub segments: Vec<(u32, u32)>,
    /// The worn items by place.
    pub worn: Vec<(u32, u64)>,
    pub items: BTreeMap<u64, ItemFacts>,
    pub money: i32,
    /// The most coins one stack holds.
    pub coin_stack: u32,
    /// Melting items into money goes without asking.
    pub no_melt_question: bool,
    /// The player's selection.
    pub selection: u64,
    /// What a pick in the world found (none: nothing yet; `Some(0)`: nothing there).
    pub picked: Option<u64>,
    /// Where open containers stand on the screen, when they are in view.
    pub screen: BTreeMap<u64, (i32, i32)>,
}

/// What a container holds: how many items at most, and in each segment its items, the
/// segment's first slot and size.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Container {
    pub size: u32,
    pub segments: Vec<Contents>,
    /// Each segment's first slot and size, as the container says.
    pub slots: Vec<(u32, u32)>,
}

/// What a container holds in one segment.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Contents {
    pub items: Vec<u64>,
    pub first: u32,
    pub count: u32,
}

/// What the panels show of an item.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemFacts {
    pub quantity: u32,
    /// Its icon (a picture), and the layers of the icon its look makes (each a picture and the
    /// color it is tinted by, bottom first), which it shows instead when it has them.
    pub icon: u32,
    pub icon_layers: Vec<(u32, u32)>,
    pub stackable: bool,
    pub stack_size: u32,
    /// Its name for its quantity.
    pub name: String,
    /// Its wear and greatest wear, when it wears.
    pub wear: Option<(u32, u32)>,
    /// Where it is worn by preference (0: nowhere).
    pub preferred_location: u32,
    /// The player has it; wears it.
    pub had: bool,
    pub worn: bool,
    /// Dropped on the money bag it melts without a question.
    pub melts_at_once: bool,
    /// Its record: what kind of thing it is.
    pub data_id: u32,
}

impl Facts {
    /// Whether an object is here (the client knows it).
    #[must_use]
    pub fn here(&self, id: u64) -> bool {
        id != NO_OBJECT
            && (self.items.contains_key(&id)
                || self.containers.contains_key(&id)
                || id == self.player)
    }

    /// An item's facts.
    #[must_use]
    pub fn item(&self, id: u64) -> Option<&ItemFacts> {
        self.items.get(&id)
    }

    /// What a container holds in a segment.
    #[must_use]
    pub fn contents(&self, container: u64, segment: u32) -> Option<&Contents> {
        let s = usize::try_from(segment).ok()?;
        self.containers.get(&container)?.segments.get(s)
    }

    /// A segment of the pack: its first slot and size.
    #[must_use]
    pub fn segment(&self, segment: u32) -> (u32, u32) {
        usize::try_from(segment)
            .ok()
            .and_then(|s| self.segments.get(s))
            .copied()
            .unwrap_or((0, 0))
    }

    /// How many items a container holds at most.
    #[must_use]
    pub fn size(&self, container: u64) -> u32 {
        self.containers.get(&container).map_or(0, |c| c.size)
    }
}

/// The context numbers the stack splitter's users know their splits by (one count for every
/// panel).
static STACK_CONTEXT: AtomicU32 = AtomicU32::new(0);

/// A new stack-split context.
pub fn next_stack_context() -> u32 {
    STACK_CONTEXT
        .fetch_add(1, Ordering::Relaxed)
        .wrapping_add(1)
}

/// Ask the stack splitter for an amount: the context, the amount offered, the most, the object
/// whose loss calls it off, another object, and the item named on it (none: coins).
#[must_use]
pub fn init_stack(
    context: u32,
    amount: u32,
    max: u32,
    watch: u64,
    other: u64,
    label: u64,
) -> Notice {
    let split = |v: u64| [(v >> 32) as u32, (v & 0xFFFF_FFFF) as u32];
    let mut words = vec![context, amount, max];
    words.extend(split(watch));
    words.extend(split(other));
    words.extend(split(label));
    said(notice::INIT_STACK, &words)
}

/// A notice of the panels' own with plain words.
#[must_use]
pub fn said(native: u32, words: &[u32]) -> Notice {
    Notice {
        native,
        words: words.to_vec(),
        ..Notice::default()
    }
}

/// A string-table entry written with its variables.
pub(crate) fn write(ui: &Ui, entry: &str, vars: &[(&str, auberean_text::Value)]) -> String {
    let table = ui
        .assets()
        .data_id_by_enum(MESSAGES, crate::flow::TABLE_MAPPING);
    let mut info = auberean_text::StringInfo::entry(table, Token::of(entry).raw());
    for (name, v) in vars {
        info.variables.push(auberean_text::Variable {
            id: Token::of(name).raw(),
            value: v.clone(),
        });
    }
    ui.assets().write(&info).unwrap_or_default()
}

/// The mail a framework heard, taken.
pub(crate) fn take(ui: &mut Ui, frame: FrameId) -> Vec<Mail> {
    ui.frame_listener(frame)
        .map(|l| ui.take_mail(l))
        .unwrap_or_default()
}

mod name {
    use auberean_ui::Token;
    pub const INVENTORY_FIELD: Token = Token::of("InventoryField");
    pub const INVENTORY_BUTTON: Token = Token::of("InventoryButton");
    pub const INVENTORY_CLOSE_BUTTON: Token = Token::of("InventoryCloseButton");
    pub const INVENTORY_HELP_BUTTON: Token = Token::of("InventoryHelpButton");
    pub const INVENTORY_PANEL_TOP: Token = Token::of("InventoryPanelTop");
    /// The toolbar windows' fields, which come to the front.
    pub const WINDOWS: [Token; 7] = [
        Token::of("ExaminationField"),
        INVENTORY_FIELD,
        Token::of("NavigationField"),
        Token::of("OptionField"),
        Token::of("QuestField"),
        Token::of("SkillField"),
        Token::of("SocialField"),
    ];
    /// The help page.
    pub const HELP: Token = Token::of("ID_Help_Inventory");
}

/// The help pages' string table, by enumeration value.
pub const HELP_TABLE: u32 = 0x4100_004E;

/// The depth a toolbar window comes to the front at, and the depths the others are pushed back
/// within.
pub const FRONT: i32 = 10;
pub const BEHIND: (i32, i32) = (11, 18);

/// A toolbar window's depth when a window comes to the front: the one coming to the front at
/// the front, any other one step further back, within the range behind it.
#[must_use]
pub fn depth_after(me: bool, now: i32) -> i32 {
    if me {
        FRONT
    } else {
        (now + 1).clamp(BEHIND.0, BEHIND.1)
    }
}

/// One of the inventory window's panels.
#[derive(Debug)]
pub enum Part {
    PaperDoll(paper_doll::PaperDoll),
    Backpack(backpack::Backpack),
    MoneyBag(money_bag::MoneyBag),
    Items3d(items_3d::Items3d),
}

impl Part {
    fn child(&mut self) -> &mut dyn Child {
        match self {
            Self::PaperDoll(p) => p,
            Self::Backpack(p) => p,
            Self::MoneyBag(p) => p,
            Self::Items3d(p) => p,
        }
    }
}

/// The toolbar's inventory window.
#[derive(Debug)]
// D2-02898
pub struct Inventory {
    frame: FrameId,
    /// The window's field (the toolbar made it).
    field: Option<Handle>,
    /// Its panels, in its list (the last made first).
    parts: Vec<Part>,
}

impl Inventory {
    /// The window under the toolbar's framework: its field found; its button, close button, help
    /// button and title heard, and every toolbar window coming to the front; its panels; hidden.
    // D2-02897
    pub fn new(ui: &mut Ui, parent: FrameId, facts: Option<&Facts>) -> Self {
        let frame = ui.new_child_frame(parent);
        let field = ui.element(name::INVENTORY_FIELD);
        if let Some(l) = ui.frame_listener(frame) {
            ui.listen_for_message(l, name::INVENTORY_BUTTON, msg::BUTTON_RELEASE_ON);
            ui.listen_for_message(l, name::INVENTORY_CLOSE_BUTTON, msg::BUTTON_RELEASE_ON);
            ui.listen_for_message(l, name::INVENTORY_HELP_BUTTON, msg::BUTTON_RELEASE_ON);
            ui.listen_for_message(l, name::INVENTORY_PANEL_TOP, msg::MOUSE_PRESS);
            ui.listen_for_message(l, name::INVENTORY_PANEL_TOP, msg::MOUSE_RELEASE);
            for w in name::WINDOWS {
                ui.listen_for_message(l, w, message::BRING_TO_FRONT);
            }
        }
        let mut me = Self {
            frame,
            field,
            parts: Vec::new(),
        };
        me.setup_children(ui, facts);
        me.show(ui, false, &mut Vec::new());
        me
    }

    /// The panels: the paper doll, the pack, the money bag, items dropped in the world; each put
    /// at the front of the list.
    // D2-02895
    pub fn setup_children(&mut self, ui: &mut Ui, facts: Option<&Facts>) {
        let f = self.frame;
        let doll = Part::PaperDoll(paper_doll::PaperDoll::new(ui, f, facts));
        let pack = Part::Backpack(backpack::Backpack::new(ui, f, facts));
        let bag = Part::MoneyBag(money_bag::MoneyBag::new(ui, f, facts));
        let world = Part::Items3d(items_3d::Items3d::new(ui, f, facts));
        for p in [doll, pack, bag, world] {
            self.parts.insert(0, p);
        }
    }

    /// The panels go.
    // D2-02395
    pub fn cleanup_children(&mut self, ui: &mut Ui, out: &mut Vec<gameplay::Request>) {
        for p in self.parts.drain(..) {
            match p {
                Part::PaperDoll(p) => p.close_with(ui, out),
                Part::Backpack(p) => p.close_with(ui, out),
                Part::MoneyBag(p) => p.close_with(ui, out),
                Part::Items3d(p) => p.close_with(ui, out),
            }
        }
    }

    /// The pack.
    #[must_use]
    pub fn backpack(&self) -> Option<&backpack::Backpack> {
        self.parts.iter().find_map(|p| match p {
            Part::Backpack(b) => Some(b),
            _ => None,
        })
    }

    /// The money bag.
    #[must_use]
    pub fn money_bag(&self) -> Option<&money_bag::MoneyBag> {
        self.parts.iter().find_map(|p| match p {
            Part::MoneyBag(b) => Some(b),
            _ => None,
        })
    }

    /// The paper doll.
    #[must_use]
    pub fn paper_doll(&self) -> Option<&paper_doll::PaperDoll> {
        self.parts.iter().find_map(|p| match p {
            Part::PaperDoll(b) => Some(b),
            _ => None,
        })
    }
}

impl Inventory {
    /// The notices its panels register for as they are made, in the order they are made.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::union([
            backpack::Backpack::notices_of(),
            paper_doll::PaperDoll::notices_of(),
            money_bag::MoneyBag::notices_of(),
            items_3d::Items3d::notices_of(),
        ])
    }
}

impl Child for Inventory {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    /// Shown or hidden: brought to the front, its field shown as the window is.
    // D2-02893
    fn show(&mut self, ui: &mut Ui, on: bool, out: &mut Vec<gameplay::Request>) {
        ui.frame_show(self.frame, on);
        for p in &mut self.parts {
            gameplay::hear(p.child(), ui, out);
        }
        if let Some(f) = self.field {
            ui.broadcast(f, message::BRING_TO_FRONT, 0);
            let shown = ui.frame_shown(self.frame);
            ui.set_visible(f, shown);
        }
    }

    /// A toolbar window coming to the front (this one at the front, the others pushed back); the
    /// title pressed (to the front) and let go (the windows' places kept); the button (shown or
    /// hidden), the close button (hidden), the help button (the help panel at the inventory's
    /// page).
    // D2-02894
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<gameplay::Request>) {
        let Mail::Element {
            from,
            element,
            what,
            ..
        } = *mail
        else {
            return;
        };
        if what == message::BRING_TO_FRONT {
            if let Some(f) = self.field {
                let now = ui.node(f).map_or(0, |n| n.z);
                ui.set_z_level(f, depth_after(from == f, now));
            }
        }
        if element == name::INVENTORY_PANEL_TOP {
            if what == msg::MOUSE_PRESS {
                if let Some(f) = self.field {
                    ui.broadcast(f, message::BRING_TO_FRONT, 0);
                }
            }
            if what == msg::MOUSE_RELEASE {
                gameplay::GamePlay::save_locations_changed(out);
            }
        }
        if element == name::INVENTORY_BUTTON {
            let shown = self.shown(ui);
            self.show(ui, !shown, out);
        } else if element == name::INVENTORY_CLOSE_BUTTON {
            self.show(ui, false, out);
        } else if element == name::INVENTORY_HELP_BUTTON {
            out.push(gameplay::Request::Inventory(Request::Help {
                entry: name::HELP,
                table: HELP_TABLE,
            }));
        }
    }

    fn notice(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<gameplay::Request>) {
        for p in &mut self.parts {
            gameplay::hand(p.child(), ui, n, out);
        }
    }

    fn update(&mut self, ui: &mut Ui, view: &View<'_>, out: &mut Vec<gameplay::Request>) {
        for p in &mut self.parts {
            gameplay::drive(p.child(), ui, view, &[], out);
        }
        for m in take(ui, self.frame) {
            match m {
                Mail::Show { shown } => self.show(ui, shown, out),
                m => self.listen(ui, &m, out),
            }
        }
    }

    fn close(mut self: Box<Self>, ui: &mut Ui) {
        self.cleanup_children(ui, &mut Vec::new());
        ui.drop_frame(self.frame);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
