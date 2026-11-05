//! What a front end (a window, a headless driver, a test) asks the client to do: the action
//! vocabulary.
//!
//! **Depends on** `auberean-primitives` only. **Used by** the runtime, which carries the actions out,
//! and every front end.
//!
//! **Must never** name a device's input: an [`Action`] is a player's intent; a key or a click
//! becomes an action in the front end's input layer, and the runtime knows nothing of keys
//! (`cargo xtask seams`).

use auberean_primitives::Vec3;

/// One thing the player asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Enter the world as this character.
    PlayCharacter { character: u64 },
    /// Leave the world for the character list.
    LogOff,
    /// Make a character.
    CreateCharacter(NewCharacter),
    /// Delete a character.
    DeleteCharacter { character: u64 },
    /// Bring back a deleted character still greyed out.
    RestoreCharacter { character: u64, name: String },
    /// A chat service packet (the chat service's own bytes).
    Chat(Vec<u8>),
    /// A call into the server's scripts, its arguments packed, sent as it is.
    ServerCall { function: u32, args: Vec<u8> },
    /// Look at an object (or at nothing, 0): the message alone.
    LookAt { target: u64 },
    /// Select an object (or nothing, 0), as a click on it in the world does: the player's
    /// scripts take the selection, examine it while the examination panel is open, and the player
    /// looks at it.
    Select { target: u64 },
    /// Use what is selected: a double click on it, or the use key.
    UseSelected,
    /// Use what a toolbar slot holds (a skill or an attack on the selection, an item, a line of
    /// text, an emote, a recipe): a click on the slot, or its key.
    UseToolbarSlot { slot: u32 },
    /// Move `quantity` of an item from its slot `old_slot` in the container `from` to slot
    /// `new_slot` of the container `to` (less than it holds splits the stack).
    PutItem {
        item: u64,
        from: u64,
        to: u64,
        old_slot: u32,
        new_slot: u32,
        quantity: u32,
    },
    /// Wear an item at a place.
    Wear { item: u64, location: u32 },
    /// Take a worn item off into a slot of the pack.
    TakeOff { item: u64, slot: u32 },
    /// Melt `quantity` of an item, from its slot in the container `from`, into money.
    IntoMoney {
        item: u64,
        quantity: u32,
        slot: u32,
        from: u64,
    },
    /// Take `amount` coins out of the money bag into a slot of a container.
    OutOfMoney {
        amount: u32,
        slot: u32,
        container: u64,
    },
    /// Take everything out of a container.
    TakeAll { container: u64 },
    /// Melt everything in a container into money.
    MeltAll { container: u64 },
    /// A container closed (or opened) for the player.
    SetOpen { container: u64, open: bool },
    /// Start attacking what is selected, or stop.
    Attack { start: bool },
    /// Open the examination panel, asking for these tabs, or close it (`tabs` none).
    Examination { tabs: Option<u32> },
    /// Give up a quest under way.
    CancelQuest { quest: u32 },
    /// Be brought back to life.
    Resurrect,
    /// Turn down someone's offer to bring the player back to life.
    DeclineResurrection,
    /// Get down from a mount.
    Dismount,
    /// Send the player's options to the server to keep.
    KeepOptions,
    /// Yes: the effect the server asked about may go on, from `source` to `target`.
    ApplyTotem {
        target: u64,
        source: u64,
        effect: u32,
    },
    /// Ask something of a skill, as the skill window does.
    Skill(SkillAction),
    /// Ask something of crafting, as the crafting page and the ingredient panel do.
    Craft(CraftAction),
    /// Ask something of a trade, as the trade window does.
    Trade(TradeAction),
    /// Ask something of a shop, a catalog or the player's consignments, as their windows do.
    Store(StoreAction),
    /// Command the player's pets, as the pet panel does.
    Pet(PetAction),
    /// The story's scenes are done: the scenes played, in their packed form as the scripts
    /// handed them.
    PlayScenesDone { scenes: Vec<u8> },
    /// Show the player's consignments that wait to be collected.
    SaleReminders,
    /// Select something (or nothing, 0) and have the examination ask for what the request names
    /// (its kind, then the datum the kind uses: a skill or panel, an item, a record).
    Examine {
        kind: u32,
        data_enum: u32,
        item: u64,
        data_id: u32,
        selection: u64,
    },
    /// Say something aloud: a line typed into the chat box.
    Say { text: String },
    /// Turn the head toward a direction.
    TurnHead { x: f32, y: f32 },
    /// Ask the server for a data record the client lacks.
    RequestData { kind: u32, id: u32 },
    /// Ask the player's body to move: the asked motion and facing.
    Move(Motion),
    /// Jump, with this velocity.
    Jump { velocity: Vec3 },
    /// Put a shortcut in a toolbar slot; one of kind 0 empties the slot.
    SetToolbarSlot {
        slot: u32,
        shortcut: ToolbarShortcut,
    },
    /// Show another set of ten toolbar slots (from 1).
    ToolbarSet { set: u32 },
    /// Set the player's options as the options window applies them: each change in turn, then
    /// (with `tell`) the panels told the options changed, then the options sent to the server to
    /// keep.
    SetOptions {
        changes: Vec<(PlayerOption, u32)>,
        tell: bool,
    },
    /// Show or hide the map coordinates on the radar, as its button does.
    ShowRadarCoordinates { on: bool },
    /// Ask something of the player's fellowship or allegiance, as the social window does.
    Social(SocialAction),
    /// Play an emote: a string reference (its string and its string table).
    Emote { string: u32, table: u32 },
    /// Start a tell to someone by name, as pressing a fellow's row twice does.
    AssistedTell { name: String },
}

/// What the social window asks of the player's fellowship and allegiance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SocialAction {
    /// Make a fellowship: its name (as UTF-16 units), its experience setting, how loot is shared.
    CreateFellowship {
        name: Vec<u16>,
        social: bool,
        looting: u32,
    },
    /// Break the fellowship up, or leave it.
    DisbandFellowship,
    LeaveFellowship,
    /// Ask someone into the fellowship, or send one out.
    Recruit(u64),
    Dismiss(u64),
    /// Take the place offered in a fellowship, or turn it down with a reason's number.
    AcceptRecruitment,
    DeclineRecruitment(u32),
    /// Ask for the allegiance's profile about a member (0: no longer).
    QueryAllegiance(u64),
    /// Swear to the one selected; break with a patron or a vassal.
    Swear,
    Break(u64),
    /// Take an oath offered, or turn it down with a reason's number.
    TakeOath,
    RefuseOath(u32),
    /// The friends page asks the friends service: add a name (as UTF-16 units) to the friends,
    /// take one off, the list again, the player's own state (1 available, 2 away, 4 not to be
    /// disturbed) and where it is.
    AddFriend(Vec<u16>),
    RemoveFriend(Vec<u16>),
    GetFriends,
    SetState {
        mode: u32,
        location: Vec<u16>,
    },
}

/// One of the player's options the options window sets; its value is a word: 1 or 0 for a
/// yes-or-no option, a 32-bit float's bits for the range, the number of rows for the height.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerOption {
    AutoTarget,
    TakeAllegianceOffers,
    TakeFellowshipOffers,
    TakeTradeOffers,
    DamageNumbers,
    OthersDamageNumbers,
    PlayerKillerDamage,
    ShortcutNumbers,
    ShortcutLocked,
    ShortcutInstantScroll,
    /// Ask before melting an item into money (kept as its opposite: melt without asking).
    AskBeforeMelting,
    LatencyGraph,
    FellowsOnMap,
    PetPanel,
    ClassicExamination,
    /// The radar shows monsters, people, players, portals, lifestones, items (its mask's bits 1
    /// to 0x20), and the map coordinates.
    RadarMonsters,
    RadarPeople,
    RadarPlayers,
    RadarPortals,
    RadarLifestones,
    RadarItems,
    RadarCoordinates,
    /// How far away others' damage shows (a float's bits).
    OthersDamageRange,
    /// The shortcut bar's rows.
    ShortcutHeight,
}

/// What a toolbar slot holds: its kind (a skill, an item, a line of text, an emote, a recipe; 0
/// for nothing) and the one datum its kind uses.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ToolbarShortcut {
    pub kind: u32,
    /// An item's instance id.
    pub item: u64,
    /// A skill's or an emote's enumerated value.
    pub data_enum: u32,
    /// A recipe's data id.
    pub data_id: u32,
    /// A line of text, as UTF-16 units.
    pub text: Vec<u16>,
}

/// What the skill window asks of a skill.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillAction {
    /// Train a skill, untrain it, raise it.
    Train(u32),
    Untrain(u32),
    Raise(u32),
    /// Stop the untraining under way.
    CancelUntrain,
}

/// What the crafting page and the ingredient panel ask of crafting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CraftAction {
    /// Open a recipe (the scripts open the ingredient panel on it).
    Open(u32),
    /// Raise a craft skill.
    RaiseCraftSkill(u32),
    /// Put an item in a recipe's ingredient slot, at a spinner value.
    AddIngredient {
        recipe: u32,
        ordinal: u32,
        item: u64,
        spinner: i32,
    },
    /// Make a recipe at a spinner value from the items in its slots (by ordinal).
    ExecuteCraft {
        recipe: u32,
        spinner: i32,
        ingredients: Vec<(u32, u64)>,
    },
    /// Read a recipe at a spinner value, as the ingredient panel does (recipe 0: the panel is
    /// closed).
    Inquire { recipe: u32, spinner: i32 },
}

/// What the trade window asks of a trade.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TradeAction {
    /// Open the trade with the partner who asked.
    Open(u64),
    Close,
    Accept,
    Decline,
    /// Offer so many of an item.
    Offer {
        item: u64,
        quantity: u32,
    },
    /// Take an offered item back.
    Revoke(u64),
}

/// What the shop windows ask of a shop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreAction {
    /// Buy so many of a sale's item into a slot of the pack (0: wherever).
    Purchase {
        item: u64,
        quantity: u32,
        slot: u32,
    },
    /// A purchase confirmed, or not.
    Confirm(bool),
    LeaveStore,
    LeaveCatalog,
    LeaveConsignment,
    EnterConsignment,
    /// Enter a catalog's shop.
    EnterShop(u32),
    NextSales,
    PrevSales,
    /// The page from a sale on.
    Sales(u32),
    /// Add so many of an item to a consignment already up.
    Restock {
        item: u64,
        quantity: u32,
        sale: u32,
    },
    /// Put so many of an item up for a price.
    Sell {
        item: u64,
        quantity: u32,
        price: u32,
    },
    /// Collect a consignment: its coins (sold), or its item.
    Collect {
        sale: u32,
        sold: bool,
    },
}

/// What the pet panel asks of the player's pets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PetAction {
    /// Attack an object.
    Attack(u64),
    Return,
    Buff,
    Heal,
    /// A pet to take a mode.
    Mode {
        pet: u64,
        mode: u32,
    },
    /// A pet renamed.
    Name {
        pet: u64,
        name: String,
    },
}

/// The motion the player asks for.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Motion {
    /// Forward (positive) or back, -1 to 1.
    pub forward: f32,
    /// Right (positive) or left, -1 to 1.
    pub sideways: f32,
    /// The heading the body faces, degrees clockwise from north.
    pub heading: f32,
    /// Whether any movement or turning is asked this frame.
    pub active: bool,
}

/// A new character's choices.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NewCharacter {
    /// The name, as UTF-16 units.
    pub name: Vec<u16>,
    pub entity: u32,
    pub start_area: u32,
    pub race: u32,
    pub sex: u32,
    pub class: u32,
    pub implement: u32,
    /// The physique choices, keyed, in the order they are laid out, with the table's size class.
    pub physique: Vec<(u32, f32)>,
    pub physique_size_class: u8,
}
