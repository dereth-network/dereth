//! What the player's scripts tell the panels, read ([`Notice`]): a player's vital, the selection,
//! an examination's profile, a fellow's health, a shop's sales, and so on, each with its values
//! named and its records (a profile, a fellowship, a shop, a string reference) as plain data.
//!
//! A notice is about an object (the panels listening on that object hear it; most are about the
//! player) or about none (every panel hears it). The runtime hands a frame's notices over in the
//! order the scripts sent them, so a front end that shows the chat window's lines beside them
//! knows which came first. Every enum here may gain variants: a reader matches what it wants and
//! lets the rest go; a notice the runtime does not read yet comes as [`Said::Unread`], its words as
//! the scripts sent them.

use crate::text::TextRef;

/// A notice the scripts sent the panels: the object it is about (none for a notice every panel
/// hears), and what it says.
#[derive(Clone, Debug, PartialEq)]
pub struct Notice {
    pub about: Option<u64>,
    pub said: Said,
}

/// What a notice says, by the part of the game it is about.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Said {
    /// The player: vitals, the vitae, death, mounts, the effects, the options.
    Player(PlayerNotice),
    /// The selection and what may be selected.
    Selection(SelectionNotice),
    /// The examination window.
    Examination(ExaminationNotice),
    /// The chat windows.
    Chat(ChatNotice),
    /// The game's questions and messages for a dialog.
    Dialog(DialogNotice),
    /// The world as the radar and the names over heads show it.
    World(WorldNotice),
    /// Combat: attacking, the special attacks.
    Combat(CombatNotice),
    /// The skills and the skill panels.
    Skills(SkillsNotice),
    /// The shortcut bar.
    Shortcuts(ShortcutsNotice),
    /// The player's items, its containers and its money.
    Inventory(InventoryNotice),
    /// The fellowship.
    Fellowship(FellowshipNotice),
    /// The allegiance.
    Allegiance(AllegianceNotice),
    /// The chat service's friends.
    Friends(FriendsNotice),
    /// A trade with another player.
    Trade(TradeNotice),
    /// The player's pets.
    Pet(PetNotice),
    /// The map's markers.
    Map(MapNotice),
    /// Shops, catalogs and the player's consignments.
    Store(StoreNotice),
    /// Making things.
    Craft(CraftNotice),
    /// Quests, the story's acts and its scenes.
    Quest(QuestNotice),
    /// A book to read.
    Book(BookNotice),
    /// The interface as a whole: portal space, the panels' places kept in a file.
    Interface(InterfaceNotice),
    /// A notice the runtime does not read: its notice number and its words, as the scripts sent
    /// them.
    Unread { native: u32, words: Vec<u32> },
}

/// The player.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum PlayerNotice {
    /// A vital ([`vital`]): its current value and its greatest.
    Vital { vital: u32, current: u64, max: u64 },
    /// Whether the player is a hero, and its focus: current and greatest.
    Hero {
        hero: bool,
        focus: u64,
        max_focus: u64,
    },
    /// The vitae: the share of health kept, the share gained back, the experience still to earn
    /// to work it off.
    Vitae {
        kept: f32,
        regained: f32,
        experience: f32,
    },
    /// The kind of conflict between players the place allows (1 to 5).
    Conflict { kind: u32 },
    /// The player died (`true`) or lives again.
    Death { dead: bool },
    /// The player mounted (`true`) or got off.
    Mounted { mounted: bool },
    /// The effects on the player changed (the view's effects say how).
    EffectsChanged,
    /// A pulsed effect missed its pulse.
    PulseMissed { effect: u32 },
    /// The player's options were read again: their yes-or-no bits, when the scripts had them.
    OptionsRead { options: Option<u32> },
    /// The options window changed the player's options: their bits, when the scripts had them.
    OptionsChanged { options: Option<u32> },
}

/// The vitals a [`PlayerNotice::Vital`] names.
pub mod vital {
    pub const HEALTH: u32 = 0x4000_0001;
    pub const VIGOR: u32 = 0x4000_0002;
    pub const EXPERIENCE: u32 = 0x4000_0003;
    pub const FOCUS: u32 = 0x4000_0004;
    /// The health and the vigor other players took.
    pub const HEALTH_TAKEN_BY_PLAYERS: u32 = 0x4000_0005;
    pub const VIGOR_TAKEN_BY_PLAYERS: u32 = 0x4000_0006;
}

/// The selection.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum SelectionNotice {
    /// The selection changed: what is selected now (0 for nothing).
    Changed { selected: u64 },
    /// The object the notice is about may be selected (`true`), or no longer.
    Selectable { selectable: bool },
    /// The selection's kind and vitals, when the notice named a selection.
    Vitals(Option<SelectionVitals>),
    /// An item waits for its target (`true`, the pointer shows it), or no longer.
    Targeting { waiting: bool },
}

/// A selection's kind and vitals, as the selection's panel shows them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SelectionVitals {
    pub kind: u32,
    pub health: u32,
    /// The health and the vigor other players took.
    pub health_taken_by_players: u32,
    pub max_health: u32,
    pub vigor: u32,
    pub vigor_taken_by_players: u32,
    pub max_vigor: u32,
}

/// The examination window.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ExaminationNotice {
    /// A profile of what is examined.
    Profile(Option<ExaminationProfile>),
    /// What is examined changed.
    RequestChanged(Option<ExaminationRequest>),
    /// The window opens.
    Open,
}

/// The examination's numbers: what a request asks about, the kinds of a profile's lines, and the
/// tabs a line shows on.
pub mod examination {
    /// What an [`super::ExaminationRequest`] asks about ([`crate::Action::Examine`]'s `kind`);
    /// 0 is nothing.
    pub mod kind {
        /// An object, by its id.
        pub const ITEM: u32 = 0x4000_0002;
        /// A skill, by its enumerated value.
        pub const SKILL: u32 = 0x4000_0003;
        /// A skill panel, by its enumerated value.
        pub const PANEL: u32 = 0x4000_0004;
        /// A recipe, by its data id.
        pub const RECIPE: u32 = 0x4000_0007;
        /// The player's vitae.
        pub const VITAE: u32 = 0x4000_000A;
        /// A craft skill, by its data id.
        pub const CRAFT_SKILL: u32 = 0x4000_000B;
        /// A category of recipes, by its enumerated value.
        pub const RECIPE_CATEGORY: u32 = 0x4000_000C;
    }

    /// The kinds of a profile's lines ([`super::ExaminationLine::kind`]).
    pub mod line {
        /// A number (the value's low word, signed), a 32-bit float's bits, yes (not 0) or no.
        pub const INT: u32 = 0x4000_0001;
        pub const FLOAT: u32 = 0x4000_0002;
        pub const BOOL: u32 = 0x4000_0003;
        /// The line's text.
        pub const STRING: u32 = 0x4000_0004;
        /// A picture, by the value.
        pub const IMAGE: u32 = 0x4000_0005;
        /// The rest of the line starts on a new one.
        pub const BREAK: u32 = 0x4000_0006;
        /// The line moves on by the value's number of indents (at least one).
        pub const INDENT: u32 = 0x4000_0007;
        /// The time left until the line's time, on the game's clock.
        pub const COUNTDOWN: u32 = 0x4000_0008;
        /// An icon, its layers.
        pub const ICON: u32 = 0x4000_0009;
        /// A 64-bit number.
        pub const LONG: u32 = 0x4000_000A;
        /// An effect: its own lines.
        pub const EFFECT: u32 = 0x4000_000C;
    }

    /// The tabs a profile's lines show on, as bits; a request asks for some of them.
    pub mod tab {
        /// The first tab, what a request asks for unless told otherwise; every profile shows it.
        pub const GENERAL: u32 = 0x203;
        pub const DETAILS: u32 = 0x18C;
        pub const EFFECTS: u32 = 0x10;
        pub const USAGE: u32 = 0x60;
        /// Every tab.
        pub const ALL: u32 = 0x3FF;
    }
}

/// What an examination asks about: its kind ([`examination::kind`]), the object, the tabs asked
/// for, and the data it names (a skill, an effect, a recipe).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExaminationRequest {
    pub kind: u32,
    pub object: u64,
    pub tabs: u32,
    pub data_enum: u32,
    pub data_id: u32,
}

/// An examination's profile: the request it answers (with the tabs it asks for), its tabs, and
/// its lines in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExaminationProfile {
    pub request: ExaminationRequest,
    pub tabs: u32,
    pub lines: Vec<ExaminationLine>,
}

/// One line of a profile: its kind, its tabs, how it looks, its value, its time, its text (for a
/// line of text), its icon's layers (each a picture and its color), and for an effect, the
/// effect's own lines.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExaminationLine {
    pub kind: u32,
    pub tabs: u32,
    pub appearance: u32,
    pub value: u64,
    pub time: f64,
    pub text: Option<TextRef>,
    pub icon: Vec<(u32, u32)>,
    pub effect: Vec<ExaminationLine>,
}

/// The chat windows.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ChatNotice {
    /// Start a tell to someone in the entry of the window the keys go to: the name.
    StartTell { to: Option<TextRef> },
    /// The chat's part of the player's options was read again.
    SettingsRead,
}

/// The game's questions and messages, each for a dialog.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum DialogNotice {
    /// A message to read: its text, and whether it goes up at once in place of the dialog up.
    Message {
        text: Option<TextRef>,
        at_once: bool,
    },
    /// Someone offers to bring the player back to life: by name.
    ResurrectionOffered { by: Option<TextRef> },
    /// Whether an effect may go on: its target, its source, the effect, and the question.
    EffectQuestion {
        target: u64,
        source: u64,
        effect: u32,
        question: Option<TextRef>,
    },
}

/// The world as the panels show it.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum WorldNotice {
    /// The radar's look of the object the notice is about changed (its name with it).
    LookChanged,
    /// An item changed (its quantity among others).
    ItemChanged,
    /// A hit's damage, to show over the object the notice is about.
    Damage(Option<HitText>),
}

/// A hit's damage as shown over what was hit: the words, their color (red, green, blue, alpha),
/// how long they stay, and their size.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HitText {
    pub text: String,
    pub color: [f32; 4],
    pub seconds: f32,
    pub scale: f32,
}

/// Combat.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum CombatNotice {
    /// The player is attacking (`true`) or not.
    Attacking {
        attacking: bool,
    },
    /// A special attack was asked for: its skill and its number.
    SpecialAsked {
        skill: u32,
        special: u32,
    },
    /// A special attack started, ended: its number.
    SpecialStarted {
        special: u32,
    },
    SpecialEnded {
        special: u32,
    },
    /// A special attack was turned on (`true`) or off: its skill.
    SpecialToggled {
        skill: u32,
        on: bool,
    },
}

/// The skills.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum SkillsNotice {
    /// A skill changed.
    Changed { skill: u32 },
    /// The player's points changed: its experience, its credits, its hero credits; `whole` when
    /// every node of a skill panel is to be shown again in full.
    Points {
        experience: u64,
        credits: u32,
        hero_credits: u32,
        whole: bool,
    },
    /// The times the skills may be reset again changed.
    ResetTimes,
}

/// The shortcut bar.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ShortcutsNotice {
    /// The shortcuts came again (the view's toolbar has them).
    Reloaded,
    /// A recipe is gone: its data id.
    RecipeGone { recipe: u32 },
}

/// The player's items, its containers and its money.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum InventoryNotice {
    /// The player's worn items changed ([`change`]): the item and where it is worn.
    Worn {
        change: u32,
        item: u64,
        location: u32,
    },
    /// A container's contents changed ([`change`]): the container, the item, its slot.
    Contents {
        container: u64,
        change: u32,
        item: u64,
        slot: u32,
    },
    /// A container opened for the player (its state, not 0) or closed (0).
    ContainerState { container: u64, state: u32 },
    /// The pack grew or shrank.
    PackChanged,
    /// The player's money changed: the amount.
    Money { amount: u32 },
}

/// How the items of a [`InventoryNotice::Worn`] or [`InventoryNotice::Contents`] changed.
pub mod change {
    /// All of them.
    pub const ALL: u32 = 0x4000_0001;
    /// One added, one taken away.
    pub const ADDED: u32 = 0x4000_0002;
    pub const REMOVED: u32 = 0x4000_0003;
    /// A request about one failed; one is under way.
    pub const FAILED: u32 = 0x4000_0004;
    pub const UNDER_WAY: u32 = 0x4000_0005;
}

/// The fellowship.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum FellowshipNotice {
    /// The fellowship as it is now.
    Changed(Option<Box<Fellowship>>),
    /// The player is in no fellowship any more.
    Cleared,
    /// A fellow as it is now.
    Fellow {
        fellow: u64,
        record: Option<Box<Fellow>>,
    },
    /// A fellow left.
    FellowGone {
        fellow: u64,
    },
    /// A fellow's level.
    Level {
        fellow: u64,
        level: u32,
    },
    /// A fellow's health and what players took of it; its greatest health.
    Health {
        fellow: u64,
        health: u32,
        taken_by_players: u32,
    },
    MaxHealth {
        fellow: u64,
        max: u32,
    },
    /// A fellow's vigor and what players took of it; its greatest vigor.
    Vigor {
        fellow: u64,
        vigor: u32,
        taken_by_players: u32,
    },
    MaxVigor {
        fellow: u64,
        max: u32,
    },
    /// The fellowship's leader.
    Leader {
        leader: u64,
    },
    /// Where a fellow is: its cell.
    Cell {
        fellow: u64,
        cell: u32,
    },
    /// A fellowship's leader offers the player a place in it: the leader, its name, the
    /// fellowship's name.
    Offered {
        leader: u64,
        leader_name: Option<TextRef>,
        fellowship: Option<TextRef>,
    },
    /// The offer ran out.
    OfferExpired {
        leader: u64,
    },
}

/// A fellowship: its fellows in the order they joined, its name, its leader, and how it shares
/// experience and loot.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fellowship {
    pub fellows: Vec<(u64, Fellow)>,
    pub name: String,
    pub leader: u64,
    pub social: i32,
    pub looting: i32,
}

/// A fellow as the fellowship shows it: its name, its level, its health and vigor (current and
/// greatest, and what players took), its cell.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fellow {
    pub name: TextRef,
    pub level: u32,
    pub health: u32,
    pub max_health: u32,
    pub vigor: u32,
    pub max_vigor: u32,
    pub health_taken_by_players: u32,
    pub vigor_taken_by_players: u32,
    pub cell: u32,
}

/// The allegiance.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum AllegianceNotice {
    /// The player's allegiance as it is now.
    Changed(Option<Box<Allegiance>>),
    /// Another player offers its oath, to be the player's vassal: by id and by name.
    OathOffered { vassal: u64, name: Option<TextRef> },
    /// An oath the player swore was answered: by whom, and how.
    OathAnswered { patron: u64, answer: u32 },
}

/// An allegiance as the player sees it: its monarch, the player's patron, the player, the
/// player's vassals (each with whether it is online), how many it has, its message of the day,
/// its name, its faction, its officers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Allegiance {
    pub monarch: (Member, bool),
    pub patron: (Member, bool),
    pub member: (Member, bool),
    pub vassals: Vec<(Member, bool)>,
    pub total: u32,
    pub message: TextRef,
    pub name: TextRef,
    pub faction: u32,
    pub officers: Vec<u64>,
}

/// One member of an allegiance.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Member {
    pub id: u64,
    pub name: TextRef,
    pub title: TextRef,
    pub experience_passed: u64,
    pub species: u32,
    pub level: u32,
    pub gender: u32,
    pub officer: bool,
    pub faction: u32,
    pub rank: u32,
}

/// The chat service's friends.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum FriendsNotice {
    /// The friends, each with its state.
    List(Vec<FriendState>),
    /// A friend was added; one was taken off: by name.
    Added {
        name: Option<TextRef>,
    },
    Removed {
        name: Option<TextRef>,
    },
    /// A friend's state changed.
    State(Option<FriendState>),
    /// The player's own state on the service: its mode.
    OwnState {
        mode: u32,
    },
}

/// A friend on the chat service: its name, where it is, its mode.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FriendState {
    pub name: String,
    pub location: String,
    pub mode: u32,
}

/// A trade.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum TradeNotice {
    /// A trade was set up: the one who asked, the one asked, and the other side's name.
    Registered {
        asker: u64,
        asked: u64,
        partner: Option<TextRef>,
    },
    /// The trade window opened.
    Opened,
    /// The trade closed: who closed it, and why.
    Closed {
        by: u64,
        reason: u32,
    },
    /// Someone accepted the trade as it stands, or declined it (0: the player's own accept may
    /// be pressed again).
    Accepted {
        by: u64,
    },
    Declined {
        by: u64,
    },
    /// The offers were emptied.
    Emptied,
    /// Both sides' acceptance was cleared.
    AcceptanceCleared,
    /// Someone offered an item: who, the item, how many.
    Offered {
        by: u64,
        item: u64,
        quantity: u32,
    },
    /// Someone took an item back: who, the item.
    Withdrawn {
        by: u64,
        item: u64,
    },
}

/// The player's pets.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum PetNotice {
    /// A pet came: its name, its icon, its mode.
    Added {
        pet: u64,
        name: Option<TextRef>,
        icon: u32,
        mode: u32,
    },
    /// A pet went.
    Removed { pet: u64 },
    /// A pet was given a name.
    Named { pet: u64, name: Option<TextRef> },
    /// A pet's mode changed.
    Mode { pet: u64, mode: u32 },
    /// A pet is under attack.
    Attacked { pet: u64 },
}

/// The map's markers.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum MapNotice {
    /// A marker put on the map: its kind, the object it marks, the cell, its picture, its label.
    MarkerAdded {
        kind: u32,
        object: u64,
        cell: u32,
        picture: u32,
        label: Option<TextRef>,
    },
    /// A marker taken off: its kind, the object it marked.
    MarkerRemoved { kind: u32, object: u64 },
    /// Every marker of a kind taken off.
    MarkersCleared { kind: u32 },
}

/// Shops, catalogs and the player's consignments.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum StoreNotice {
    /// A catalog shown: its header, its shops.
    Catalog {
        header: Option<TextRef>,
        shops: Vec<Store>,
    },
    /// The catalog left.
    CatalogLeft,
    /// A shop shown, or its sales changed.
    Shown(Option<Box<Store>>),
    Changed(Option<Box<Store>>),
    /// How many of a sale are left: the item, how many.
    Stock {
        item: u64,
        stock: i32,
    },
    /// The shop left.
    Left,
    /// A purchase to confirm: the product, how many, what it costs, how much of that.
    Purchase {
        product: Option<TextRef>,
        quantity: u32,
        cost: Option<TextRef>,
        cost_quantity: u32,
    },
    /// The player's consignments shown, or changed (as a shop).
    Consignments(Option<Box<Store>>),
    ConsignmentsChanged(Option<Box<Store>>),
    /// The consignments left.
    ConsignmentsLeft,
    /// A sale of the player's waits to be collected (`true`), or none does.
    SaleWaiting {
        waiting: bool,
    },
}

/// A shop as its window shows it: its id, its name, its description and sorting, its portrait,
/// its sales in order, the page shown and its size.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Store {
    pub id: u32,
    pub name: TextRef,
    pub description: TextRef,
    pub sorter: TextRef,
    pub portrait: u32,
    pub sales: Vec<Sale>,
    pub position: i32,
    pub size: i32,
}

/// One sale of a shop: the item (by instance and by data id), its icon and its layers (each a
/// picture and its color), its description, what it costs (described, its icon, how much), how
/// many are left, its place, its state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sale {
    pub item: u64,
    pub item_data: u32,
    pub icon: u32,
    pub icon_layers: Vec<(u32, u32)>,
    pub description: TextRef,
    pub cost_description: TextRef,
    pub cost_icon: u32,
    pub cost_quantity: f32,
    pub stock: i32,
    pub ordinal: u32,
    pub state: u32,
}

/// Making things.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum CraftNotice {
    /// The craft panel changed (the view's craft says how).
    PanelChanged,
    /// An ingredient was put in, or not: the recipe, its slot, the item, the amount, why it
    /// failed (0: it went).
    IngredientAdded {
        recipe: u32,
        slot: u32,
        item: u64,
        amount: u32,
        error: u32,
    },
    /// The making is done: the recipe, why it failed (0: it went).
    Made { recipe: u32, error: u32 },
    /// Open the ingredient panel on a recipe.
    Open { recipe: u32 },
    /// A craft skill changed.
    SkillChanged { skill: u32 },
}

/// Quests, the story's acts and its scenes.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum QuestNotice {
    /// The journal's quests changed: how, the quest, and for the whole list read again, every
    /// quest.
    Journal {
        change: u32,
        quest: Option<Quest>,
        all: Option<Vec<Quest>>,
    },
    /// The story's acts changed: how, and the act.
    Acts { change: u32, act: u32 },
    /// A story's scenes to play.
    Scenes(Option<Scenes>),
}

/// A quest of the journal: its id, its name, description and journal, its status, how hard it is
/// (-999: any level), when it was given (game seconds), whether it may be taken no more, the
/// seconds left (before it runs out, or may be taken again), whether the player may give it up.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Quest {
    pub id: u32,
    pub name: TextRef,
    pub description: TextRef,
    pub journal: TextRef,
    pub status: u32,
    pub challenge: i32,
    pub given: f64,
    pub maxed_out: bool,
    pub seconds_remaining: i32,
    pub cancellable: bool,
}

/// Scenes of the story, in order, and their packed form (what the client hands back when they
/// are done).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scenes {
    pub scenes: Vec<Scene>,
    pub packed: Vec<u8>,
}

/// A scene: its id, whether it may be played again, its keyframes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub id: u32,
    pub playable: bool,
    pub keyframes: Vec<Keyframe>,
}

/// A keyframe of a scene: its picture, its music, its voice, its subtitle, how long it lasts
/// (each when it has one).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Keyframe {
    pub picture: Option<u32>,
    pub music: Option<u32>,
    pub voice: Option<u32>,
    pub subtitle: Option<TextRef>,
    pub seconds: Option<f64>,
}

/// A book to read.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum BookNotice {
    /// Open it: whether it shows its controls, how it looks, its title, its text, its picture.
    Open {
        controls: bool,
        look: u32,
        title: Option<TextRef>,
        text: Option<TextRef>,
        picture: u32,
    },
}

/// The interface as a whole.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum InterfaceNotice {
    /// The player went into portal space (`true`) or came out.
    ThroughPortal {
        inside: bool,
    },
    /// A scene on the way through a portal: whether one plays, and the scenes.
    PortalScene {
        playing: bool,
        scenes: Option<Scenes>,
    },
    /// The panels' places to be written to a file, or read back from one: its name.
    SaveLayout {
        name: Option<TextRef>,
    },
    LoadLayout {
        name: Option<TextRef>,
    },
    /// The chat service closed the connection.
    ChatServiceLost,
    /// The chat service took the player's ticket again (`true`) or not, after it came back.
    ChatServiceTicket {
        taken: bool,
    },
}
