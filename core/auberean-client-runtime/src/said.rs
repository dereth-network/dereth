//! The panels' notices read as the contract carries them ([`auberean_client_contract::notice`]):
//! each notice's words named as the retail panels read them (an object's id as its high word and
//! its low, a flag, a number), and the records its words name (an examination profile or
//! request, the player's options, a quest and the quest list, a string reference, a shop and a
//! catalog's shops, the story's scenes, a fellowship and a fellow, an allegiance's profile, a
//! friend's state and the friends list, a hit's damage) as plain data, read as the panels read
//! them.
//!
//! A front end takes a frame's notices read with [`crate::Client::take_notices`].

use std::collections::BTreeMap;

use auberean_client_contract::notice::{
    Allegiance, AllegianceNotice, BookNotice, ChatNotice, CombatNotice, CraftNotice, DialogNotice,
    ExaminationLine, ExaminationNotice, ExaminationProfile, ExaminationRequest, Fellow, Fellowship,
    FellowshipNotice, FriendState, FriendsNotice, HitText, InterfaceNotice, InventoryNotice,
    Keyframe, MapNotice, Member, Notice, PetNotice, PlayerNotice, Quest, QuestNotice, Said, Sale,
    Scene, Scenes, SelectionNotice, SelectionVitals, ShortcutsNotice, SkillsNotice, Store,
    StoreNotice, TradeNotice, WorldNotice,
};
use auberean_client_contract::text::TextRef;
use auberean_script::native_object::{native_type, Field, NativeObject, QuestInfo};

use crate::examination::{kind, Profile, Request};
use crate::host::PanelNotice;
use crate::text::{from_script, to_view};

/// The notices read, by the native that sends each (its package, then its number), and the
/// client's own.
pub mod native {
    const fn id(package: u32, number: u32) -> u32 {
        package << 16 | number
    }
    pub const ALLEGIANCE_PROFILE: u32 = id(0x84, 0x01);
    pub const OATH_OFFERED: u32 = id(0x84, 0x02);
    pub const OATH_ANSWERED: u32 = id(0x84, 0x03);
    pub const VITAL: u32 = id(0x85, 0x01);
    pub const EFFECTS: u32 = id(0x85, 0x02);
    pub const SELECTED: u32 = id(0x85, 0x03);
    pub const SELECTABLE: u32 = id(0x85, 0x04);
    pub const SKILL_POINTS: u32 = id(0x85, 0x05);
    pub const SKILL: u32 = id(0x85, 0x06);
    pub const RESET_TIMES: u32 = id(0x85, 0x07);
    pub const DEATH: u32 = id(0x85, 0x08);
    pub const OPTIONS_READ: u32 = id(0x85, 0x09);
    pub const LOOK: u32 = id(0x85, 0x0A);
    pub const SELECTION_VITALS: u32 = id(0x85, 0x0B);
    pub const MESSAGE: u32 = id(0x85, 0x0C);
    pub const CONFLICT: u32 = id(0x85, 0x0D);
    pub const VITAE: u32 = id(0x85, 0x0E);
    pub const SKILL_POINTS_WHOLE: u32 = id(0x85, 0x0F);
    pub const RESURRECTION_OFFERED: u32 = id(0x85, 0x10);
    pub const MOUNTED: u32 = id(0x85, 0x11);
    pub const HERO: u32 = id(0x85, 0x12);
    pub const PULSE_MISSED: u32 = id(0x85, 0x13);
    pub const MARKER_ADDED: u32 = id(0x85, 0x14);
    pub const MARKERS_CLEARED: u32 = id(0x85, 0x15);
    pub const MARKER_REMOVED: u32 = id(0x85, 0x16);
    pub const PET_ADDED: u32 = id(0x85, 0x17);
    pub const PET_REMOVED: u32 = id(0x85, 0x18);
    pub const PET_NAMED: u32 = id(0x85, 0x19);
    pub const PET_MODE: u32 = id(0x85, 0x1A);
    pub const PET_ATTACKED: u32 = id(0x85, 0x1B);
    pub const SAVE_LAYOUT: u32 = id(0x85, 0x1C);
    pub const LOAD_LAYOUT: u32 = id(0x85, 0x1D);
    pub const ATTACKING: u32 = id(0x86, 0x01);
    pub const SPECIAL_ASKED: u32 = id(0x86, 0x02);
    pub const SPECIAL_STARTED: u32 = id(0x86, 0x03);
    pub const SPECIAL_ENDED: u32 = id(0x86, 0x04);
    pub const SPECIAL_TOGGLED: u32 = id(0x86, 0x05);
    pub const START_TELL: u32 = id(0x87, 0x02);
    pub const CHAT_SETTINGS_READ: u32 = id(0x87, 0x04);
    pub const CRAFT_PANEL: u32 = id(0x88, 0x01);
    pub const INGREDIENT_ADDED: u32 = id(0x88, 0x02);
    pub const MADE: u32 = id(0x88, 0x03);
    pub const CRAFT_OPEN: u32 = id(0x88, 0x05);
    pub const CRAFT_SKILL: u32 = id(0x88, 0x06);
    pub const PROFILE: u32 = id(0x89, 0x01);
    pub const REQUEST_CHANGED: u32 = id(0x89, 0x02);
    pub const FELLOWSHIP: u32 = id(0x8A, 0x01);
    pub const FELLOWSHIP_CLEARED: u32 = id(0x8A, 0x02);
    pub const FELLOW: u32 = id(0x8A, 0x03);
    pub const FELLOW_GONE: u32 = id(0x8A, 0x04);
    pub const FELLOW_LEVEL: u32 = id(0x8A, 0x05);
    pub const FELLOWSHIP_OFFERED: u32 = id(0x8A, 0x06);
    pub const FELLOWSHIP_OFFER_EXPIRED: u32 = id(0x8A, 0x07);
    pub const FELLOW_HEALTH: u32 = id(0x8A, 0x08);
    pub const FELLOW_MAX_HEALTH: u32 = id(0x8A, 0x09);
    pub const FELLOW_VIGOR: u32 = id(0x8A, 0x0A);
    pub const FELLOW_MAX_VIGOR: u32 = id(0x8A, 0x0B);
    pub const LEADER: u32 = id(0x8A, 0x0C);
    pub const FELLOW_CELL: u32 = id(0x8A, 0x0D);
    pub const WORN: u32 = id(0x8B, 0x01);
    pub const CONTENTS: u32 = id(0x8B, 0x02);
    pub const CONTAINER_STATE: u32 = id(0x8B, 0x03);
    pub const ITEM: u32 = id(0x8B, 0x04);
    pub const PACK: u32 = id(0x8B, 0x05);
    pub const MONEY: u32 = id(0x8C, 0x02);
    pub const DAMAGE: u32 = id(0x8D, 0x01);
    pub const JOURNAL: u32 = id(0x8E, 0x01);
    pub const ACTS: u32 = id(0x8E, 0x02);
    pub const SCENES: u32 = id(0x8E, 0x03);
    pub const SHORTCUTS: u32 = id(0x8F, 0x01);
    pub const RECIPE_GONE: u32 = id(0x8F, 0x02);
    pub const TRADE_REGISTERED: u32 = id(0x90, 0x01);
    pub const TRADE_OPENED: u32 = id(0x90, 0x02);
    pub const TRADE_CLOSED: u32 = id(0x90, 0x03);
    pub const TRADE_ACCEPTED: u32 = id(0x90, 0x04);
    pub const TRADE_DECLINED: u32 = id(0x90, 0x05);
    pub const TRADE_EMPTIED: u32 = id(0x90, 0x06);
    pub const TRADE_ACCEPTANCE_CLEARED: u32 = id(0x90, 0x07);
    pub const TRADE_OFFERED: u32 = id(0x90, 0x08);
    pub const TRADE_WITHDRAWN: u32 = id(0x90, 0x09);
    pub const PORTAL_SPACE: u32 = id(0x91, 0x01);
    pub const OPEN_EXAMINATION: u32 = id(0x91, 0x02);
    pub const PORTAL_SCENE: u32 = id(0x91, 0x03);
    pub const TARGETING: u32 = id(0x92, 0x01);
    pub const BOOK: u32 = id(0x92, 0x02);
    pub const EFFECT_QUESTION: u32 = id(0x92, 0x03);
    pub const FRIENDS: u32 = id(0x1AD2, 0x01);
    pub const FRIEND_ADDED: u32 = id(0x1AD2, 0x02);
    pub const FRIEND_REMOVED: u32 = id(0x1AD2, 0x03);
    pub const FRIEND_STATE: u32 = id(0x1AD2, 0x04);
    pub const OWN_STATE: u32 = id(0x1AD2, 0x05);
    pub const CATALOG: u32 = id(0x1AD3, 0x01);
    pub const CATALOG_LEFT: u32 = id(0x1AD3, 0x02);
    pub const STORE: u32 = id(0x1AD3, 0x03);
    pub const STORE_CHANGED: u32 = id(0x1AD3, 0x04);
    pub const STOCK: u32 = id(0x1AD3, 0x05);
    pub const STORE_LEFT: u32 = id(0x1AD3, 0x06);
    pub const PURCHASE: u32 = id(0x1AD3, 0x07);
    pub const CONSIGNMENTS: u32 = id(0x1AD3, 0x08);
    pub const CONSIGNMENTS_CHANGED: u32 = id(0x1AD3, 0x09);
    pub const CONSIGNMENTS_LEFT: u32 = id(0x1AD3, 0x0A);
    pub const SALE_WAITING: u32 = id(0x1AD3, 0x0B);
    /// The client's own: the chat service closed the connection; it took the ticket again (or
    /// not); the options window changed the player's options.
    pub const CHAT_SERVICE_LOST: u32 = crate::friends::notice::CONNECTION_LOST;
    pub const CHAT_SERVICE_TICKET: u32 = crate::friends::notice::AUTHENTICATED;
    pub const OPTIONS_CHANGED: u32 = crate::OPTIONS_CHANGED;
}

/// A record a notice's word names, as the panels read it.
#[derive(Clone, Debug, PartialEq)]
enum Record {
    Profile(Box<ExaminationProfile>),
    Request(ExaminationRequest),
    OptionBits(u32),
    Quest(Box<Quest>),
    Quests(Vec<Quest>),
    Scenes(Scenes),
    Text(TextRef),
    Store(Box<Store>),
    Stores(Vec<Store>),
    Fellowship(Box<Fellowship>),
    Fellow(Box<Fellow>),
    Allegiance(Box<Allegiance>),
    Friends(Vec<FriendState>),
    Friend(FriendState),
    Damage(HitText),
}

/// A notice as the contract carries it: its words named, its records read.
#[must_use]
pub fn read(n: &PanelNotice) -> Notice {
    let records = records_of(n);
    let r = Reader {
        words: &n.words,
        records: &records,
    };
    Notice {
        about: n.about,
        said: said(n, &r).unwrap_or_else(|| Said::Unread {
            native: n.native,
            words: n.words.clone(),
        }),
    }
}

/// A notice's words and records, read by position.
struct Reader<'a> {
    words: &'a [u32],
    records: &'a [Option<Record>],
}

impl Reader<'_> {
    fn word(&self, at: usize) -> u32 {
        self.words.get(at).copied().unwrap_or(0)
    }

    /// A 64-bit value (an instance id): its high word at `at`, its low next.
    fn long(&self, at: usize) -> u64 {
        u64::from(self.word(at)) << 32 | u64::from(self.word(at + 1))
    }

    /// A flag: a word not 0.
    fn yes(&self, at: usize) -> bool {
        self.word(at) != 0
    }

    /// A one-byte flag: the word's low byte not 0.
    fn flag(&self, at: usize) -> bool {
        self.word(at) & 0xFF != 0
    }

    fn float(&self, at: usize) -> f32 {
        f32::from_bits(self.word(at))
    }

    fn record(&self, at: usize) -> Option<&Record> {
        self.records.get(at).and_then(Option::as_ref)
    }

    fn text(&self, at: usize) -> Option<TextRef> {
        match self.record(at) {
            Some(Record::Text(t)) => Some(t.clone()),
            _ => None,
        }
    }

    /// The first record of a kind, wherever it is.
    fn find<T>(&self, f: impl Fn(&Record) -> Option<T>) -> Option<T> {
        self.records.iter().flatten().find_map(f)
    }

    fn store(&self) -> Option<Box<Store>> {
        self.find(|r| match r {
            Record::Store(s) => Some(s.clone()),
            _ => None,
        })
    }

    fn options(&self) -> Option<u32> {
        self.find(|r| match r {
            Record::OptionBits(b) => Some(*b),
            _ => None,
        })
    }

    fn scenes(&self) -> Option<Scenes> {
        self.find(|r| match r {
            Record::Scenes(s) => Some(s.clone()),
            _ => None,
        })
    }
}

/// What a notice says, read as the panels that hear it read it; `None` for one no panel reads.
#[allow(clippy::too_many_lines)]
fn said(n: &PanelNotice, r: &Reader<'_>) -> Option<Said> {
    use self::native as k;
    let w = |at| r.word(at);
    Some(match n.native {
        k::VITAL => Said::Player(PlayerNotice::Vital {
            vital: w(0),
            current: r.long(1),
            max: r.long(3),
        }),
        k::HERO => Said::Player(PlayerNotice::Hero {
            hero: r.yes(0),
            focus: r.long(1),
            max_focus: r.long(3),
        }),
        k::VITAE => Said::Player(PlayerNotice::Vitae {
            kept: r.float(0),
            regained: r.float(1),
            experience: r.float(2),
        }),
        k::CONFLICT => Said::Player(PlayerNotice::Conflict { kind: w(0) }),
        k::DEATH => Said::Player(PlayerNotice::Death { dead: r.yes(0) }),
        k::MOUNTED => Said::Player(PlayerNotice::Mounted { mounted: r.yes(0) }),
        k::EFFECTS => Said::Player(PlayerNotice::EffectsChanged),
        k::PULSE_MISSED => Said::Player(PlayerNotice::PulseMissed { effect: w(0) }),
        k::OPTIONS_READ => Said::Player(PlayerNotice::OptionsRead {
            options: r.options(),
        }),
        k::OPTIONS_CHANGED => Said::Player(PlayerNotice::OptionsChanged {
            options: r.options(),
        }),
        k::SELECTED => Said::Selection(SelectionNotice::Changed {
            selected: r.long(0),
        }),
        k::SELECTABLE => Said::Selection(SelectionNotice::Selectable {
            selectable: r.yes(0),
        }),
        k::SELECTION_VITALS => Said::Selection(SelectionNotice::Vitals(
            n.selections
                .first()
                .copied()
                .flatten()
                .map(|s| SelectionVitals {
                    kind: s.kind,
                    health: s.cur_health,
                    health_taken_by_players: s.pk_damage,
                    max_health: s.max_health,
                    vigor: s.cur_vigor,
                    vigor_taken_by_players: s.pk_vigor_loss,
                    max_vigor: s.max_vigor,
                }),
        )),
        k::TARGETING => Said::Selection(SelectionNotice::Targeting { waiting: r.yes(0) }),
        k::PROFILE => Said::Examination(ExaminationNotice::Profile(r.find(|x| match x {
            Record::Profile(p) => Some(p.as_ref().clone()),
            _ => None,
        }))),
        k::REQUEST_CHANGED => {
            Said::Examination(ExaminationNotice::RequestChanged(r.find(|x| match x {
                Record::Request(q) => Some(*q),
                _ => None,
            })))
        }
        k::OPEN_EXAMINATION => Said::Examination(ExaminationNotice::Open),
        k::START_TELL => Said::Chat(ChatNotice::StartTell { to: r.text(0) }),
        k::CHAT_SETTINGS_READ => Said::Chat(ChatNotice::SettingsRead),
        k::MESSAGE => Said::Dialog(DialogNotice::Message {
            text: r.text(0),
            at_once: r.yes(1),
        }),
        k::RESURRECTION_OFFERED => {
            Said::Dialog(DialogNotice::ResurrectionOffered { by: r.text(0) })
        }
        k::EFFECT_QUESTION => Said::Dialog(DialogNotice::EffectQuestion {
            target: r.long(0),
            source: r.long(2),
            effect: w(4),
            question: r.text(5),
        }),
        k::LOOK => Said::World(WorldNotice::LookChanged),
        k::ITEM => Said::World(WorldNotice::ItemChanged),
        k::DAMAGE => Said::World(WorldNotice::Damage(match r.record(0) {
            Some(Record::Damage(d)) => Some(d.clone()),
            _ => None,
        })),
        k::ATTACKING => Said::Combat(CombatNotice::Attacking {
            attacking: r.flag(0),
        }),
        k::SPECIAL_ASKED => Said::Combat(CombatNotice::SpecialAsked {
            skill: w(0),
            special: w(1),
        }),
        k::SPECIAL_STARTED => Said::Combat(CombatNotice::SpecialStarted { special: w(0) }),
        k::SPECIAL_ENDED => Said::Combat(CombatNotice::SpecialEnded { special: w(0) }),
        k::SPECIAL_TOGGLED => Said::Combat(CombatNotice::SpecialToggled {
            skill: w(0),
            on: r.flag(1),
        }),
        k::SKILL => Said::Skills(SkillsNotice::Changed { skill: w(0) }),
        k::SKILL_POINTS | k::SKILL_POINTS_WHOLE => Said::Skills(SkillsNotice::Points {
            experience: r.long(0),
            credits: w(2),
            hero_credits: w(3),
            whole: n.native == k::SKILL_POINTS_WHOLE,
        }),
        k::RESET_TIMES => Said::Skills(SkillsNotice::ResetTimes),
        k::SHORTCUTS => Said::Shortcuts(ShortcutsNotice::Reloaded),
        k::RECIPE_GONE => Said::Shortcuts(ShortcutsNotice::RecipeGone { recipe: w(0) }),
        k::WORN => Said::Inventory(InventoryNotice::Worn {
            change: w(0),
            item: r.long(1),
            location: w(3),
        }),
        k::CONTENTS => Said::Inventory(InventoryNotice::Contents {
            container: r.long(0),
            change: w(2),
            item: r.long(3),
            slot: w(5),
        }),
        k::CONTAINER_STATE => Said::Inventory(InventoryNotice::ContainerState {
            container: r.long(0),
            state: w(2),
        }),
        k::PACK => Said::Inventory(InventoryNotice::PackChanged),
        k::MONEY => Said::Inventory(InventoryNotice::Money { amount: w(0) }),
        k::FELLOWSHIP => Said::Fellowship(FellowshipNotice::Changed(match r.record(0) {
            Some(Record::Fellowship(f)) => Some(f.clone()),
            _ => None,
        })),
        k::FELLOWSHIP_CLEARED => Said::Fellowship(FellowshipNotice::Cleared),
        k::FELLOW => Said::Fellowship(FellowshipNotice::Fellow {
            fellow: r.long(0),
            record: match r.record(2) {
                Some(Record::Fellow(f)) => Some(f.clone()),
                _ => None,
            },
        }),
        k::FELLOW_GONE => Said::Fellowship(FellowshipNotice::FellowGone { fellow: r.long(0) }),
        k::FELLOW_LEVEL => Said::Fellowship(FellowshipNotice::Level {
            fellow: r.long(0),
            level: w(2),
        }),
        k::FELLOW_HEALTH => Said::Fellowship(FellowshipNotice::Health {
            fellow: r.long(0),
            health: w(2),
            taken_by_players: w(3),
        }),
        k::FELLOW_MAX_HEALTH => Said::Fellowship(FellowshipNotice::MaxHealth {
            fellow: r.long(0),
            max: w(2),
        }),
        k::FELLOW_VIGOR => Said::Fellowship(FellowshipNotice::Vigor {
            fellow: r.long(0),
            vigor: w(2),
            taken_by_players: w(3),
        }),
        k::FELLOW_MAX_VIGOR => Said::Fellowship(FellowshipNotice::MaxVigor {
            fellow: r.long(0),
            max: w(2),
        }),
        k::LEADER => Said::Fellowship(FellowshipNotice::Leader { leader: r.long(0) }),
        k::FELLOW_CELL => Said::Fellowship(FellowshipNotice::Cell {
            fellow: r.long(0),
            cell: w(2),
        }),
        k::FELLOWSHIP_OFFERED => Said::Fellowship(FellowshipNotice::Offered {
            leader: r.long(0),
            leader_name: r.text(2),
            fellowship: r.text(3),
        }),
        k::FELLOWSHIP_OFFER_EXPIRED => {
            Said::Fellowship(FellowshipNotice::OfferExpired { leader: r.long(0) })
        }
        k::ALLEGIANCE_PROFILE => Said::Allegiance(AllegianceNotice::Changed(match r.record(0) {
            Some(Record::Allegiance(a)) => Some(a.clone()),
            _ => None,
        })),
        k::OATH_OFFERED => Said::Allegiance(AllegianceNotice::OathOffered {
            vassal: r.long(0),
            name: r.text(2),
        }),
        k::OATH_ANSWERED => Said::Allegiance(AllegianceNotice::OathAnswered {
            patron: r.long(0),
            answer: w(2),
        }),
        k::FRIENDS => Said::Friends(FriendsNotice::List(match r.record(0) {
            Some(Record::Friends(f)) => f.clone(),
            _ => Vec::new(),
        })),
        k::FRIEND_ADDED => Said::Friends(FriendsNotice::Added { name: r.text(0) }),
        k::FRIEND_REMOVED => Said::Friends(FriendsNotice::Removed { name: r.text(0) }),
        k::FRIEND_STATE => Said::Friends(FriendsNotice::State(match r.record(0) {
            Some(Record::Friend(f)) => Some(f.clone()),
            _ => None,
        })),
        k::OWN_STATE => Said::Friends(FriendsNotice::OwnState { mode: w(0) }),
        k::TRADE_REGISTERED => Said::Trade(TradeNotice::Registered {
            asker: r.long(0),
            asked: r.long(2),
            partner: r.text(4),
        }),
        k::TRADE_OPENED => Said::Trade(TradeNotice::Opened),
        k::TRADE_CLOSED => Said::Trade(TradeNotice::Closed {
            by: r.long(0),
            reason: w(2),
        }),
        k::TRADE_ACCEPTED => Said::Trade(TradeNotice::Accepted { by: r.long(0) }),
        k::TRADE_DECLINED => Said::Trade(TradeNotice::Declined { by: r.long(0) }),
        k::TRADE_EMPTIED => Said::Trade(TradeNotice::Emptied),
        k::TRADE_ACCEPTANCE_CLEARED => Said::Trade(TradeNotice::AcceptanceCleared),
        k::TRADE_OFFERED => Said::Trade(TradeNotice::Offered {
            by: r.long(0),
            item: r.long(2),
            quantity: w(4),
        }),
        k::TRADE_WITHDRAWN => Said::Trade(TradeNotice::Withdrawn {
            by: r.long(0),
            item: r.long(2),
        }),
        k::PET_ADDED => Said::Pet(PetNotice::Added {
            pet: r.long(0),
            name: r.text(2),
            icon: w(3),
            mode: w(4),
        }),
        k::PET_REMOVED => Said::Pet(PetNotice::Removed { pet: r.long(0) }),
        k::PET_NAMED => Said::Pet(PetNotice::Named {
            pet: r.long(0),
            name: r.text(2),
        }),
        k::PET_MODE => Said::Pet(PetNotice::Mode {
            pet: r.long(0),
            mode: w(2),
        }),
        k::PET_ATTACKED => Said::Pet(PetNotice::Attacked { pet: r.long(0) }),
        k::MARKER_ADDED => Said::Map(MapNotice::MarkerAdded {
            kind: w(0),
            object: r.long(1),
            cell: w(3),
            picture: w(4),
            label: r.text(5),
        }),
        k::MARKER_REMOVED => Said::Map(MapNotice::MarkerRemoved {
            kind: w(0),
            object: r.long(1),
        }),
        k::MARKERS_CLEARED => Said::Map(MapNotice::MarkersCleared { kind: w(0) }),
        k::CATALOG => Said::Store(StoreNotice::Catalog {
            header: r.text(0),
            shops: match r.record(1) {
                Some(Record::Stores(s)) => s.clone(),
                _ => Vec::new(),
            },
        }),
        k::CATALOG_LEFT => Said::Store(StoreNotice::CatalogLeft),
        k::STORE => Said::Store(StoreNotice::Shown(r.store())),
        k::STORE_CHANGED => Said::Store(StoreNotice::Changed(r.store())),
        k::STOCK => Said::Store(StoreNotice::Stock {
            item: r.long(0),
            stock: w(2).cast_signed(),
        }),
        k::STORE_LEFT => Said::Store(StoreNotice::Left),
        k::PURCHASE => Said::Store(StoreNotice::Purchase {
            product: r.text(0),
            quantity: w(1),
            cost: r.text(2),
            cost_quantity: w(3),
        }),
        k::CONSIGNMENTS => Said::Store(StoreNotice::Consignments(r.store())),
        k::CONSIGNMENTS_CHANGED => Said::Store(StoreNotice::ConsignmentsChanged(r.store())),
        k::CONSIGNMENTS_LEFT => Said::Store(StoreNotice::ConsignmentsLeft),
        k::SALE_WAITING => Said::Store(StoreNotice::SaleWaiting { waiting: r.yes(0) }),
        k::CRAFT_PANEL => Said::Craft(CraftNotice::PanelChanged),
        k::INGREDIENT_ADDED => Said::Craft(CraftNotice::IngredientAdded {
            recipe: w(0),
            slot: w(1),
            item: r.long(2),
            amount: w(4),
            error: w(5),
        }),
        k::MADE => Said::Craft(CraftNotice::Made {
            recipe: w(0),
            error: w(1),
        }),
        k::CRAFT_OPEN => Said::Craft(CraftNotice::Open { recipe: w(0) }),
        k::CRAFT_SKILL => Said::Craft(CraftNotice::SkillChanged { skill: w(0) }),
        k::JOURNAL => Said::Quest(QuestNotice::Journal {
            change: w(0),
            quest: match r.record(1) {
                Some(Record::Quest(q)) => Some(q.as_ref().clone()),
                _ => None,
            },
            all: r.find(|x| match x {
                Record::Quests(l) => Some(l.clone()),
                _ => None,
            }),
        }),
        k::ACTS => Said::Quest(QuestNotice::Acts {
            change: w(0),
            act: w(1),
        }),
        k::SCENES => Said::Quest(QuestNotice::Scenes(r.scenes())),
        k::BOOK => Said::Book(BookNotice::Open {
            controls: r.yes(0),
            look: w(1),
            title: r.text(2),
            text: r.text(3),
            picture: w(4),
        }),
        k::PORTAL_SPACE => Said::Interface(InterfaceNotice::ThroughPortal { inside: r.yes(0) }),
        k::PORTAL_SCENE => Said::Interface(InterfaceNotice::PortalScene {
            playing: r.yes(0),
            scenes: r.scenes(),
        }),
        k::SAVE_LAYOUT => Said::Interface(InterfaceNotice::SaveLayout { name: r.text(0) }),
        k::LOAD_LAYOUT => Said::Interface(InterfaceNotice::LoadLayout { name: r.text(0) }),
        k::CHAT_SERVICE_LOST => Said::Interface(InterfaceNotice::ChatServiceLost),
        k::CHAT_SERVICE_TICKET => Said::Interface(InterfaceNotice::ChatServiceTicket {
            taken: r.word(0) == 1,
        }),
        _ => return None,
    })
}

/// The records a notice's words name: an examination request whole when the scripts held one, a
/// profile with its effect lines' own lines, a list as the friends or a catalog's shops, else as
/// [`record`] reads it.
// D2-00898, D2-00925
fn records_of(n: &PanelNotice) -> Vec<Option<Record>> {
    n.objects
        .iter()
        .enumerate()
        .map(|(i, o)| {
            if let Some(r) = n.requests.get(i).copied().flatten() {
                return Some(Record::Request(request(&r)));
            }
            let listed = n.listed.get(i).map_or(&[][..], Vec::as_slice);
            // The friends list: every entry a friend's state (an empty list too, for its notice).
            let friends: Vec<_> = listed.iter().flatten().filter_map(friend).collect();
            if friends.len() == listed.len() && (!friends.is_empty() || n.native == native::FRIENDS)
            {
                return Some(Record::Friends(friends));
            }
            if !listed.is_empty() {
                let shops: Vec<_> = listed.iter().flatten().filter_map(store).collect();
                if !shops.is_empty() {
                    return Some(Record::Stores(shops));
                }
            }
            let mut record = record(o.as_ref());
            if let Some(Record::Profile(p)) = record.as_mut() {
                for (k, bytes) in n.effect_lines.get(i).into_iter().flatten() {
                    if let (Some(line), Ok(sub)) = (p.lines.get_mut(*k), Profile::unpack(bytes)) {
                        line.effect = lines(&sub);
                    }
                }
            }
            record
        })
        .collect()
}

/// The record a notice's word names, when it is one a panel reads.
fn record(o: Option<&NativeObject>) -> Option<Record> {
    match o? {
        o @ (NativeObject::Packed { .. } | NativeObject::Profile { .. })
            if Profile::of(o).is_some() =>
        {
            Profile::of(o).map(|p| Record::Profile(Box::new(profile(&p))))
        }
        NativeObject::Packed { ty, bytes } if *ty == native_type::EXAMINATION_REQUEST => {
            Some(Record::Request(request(&Request::unpack(bytes))))
        }
        NativeObject::Options(o) => Some(Record::OptionBits(o.bits)),
        NativeObject::TextInfo(t) => Some(Record::Text(script_text(t))),
        NativeObject::Text(t) => {
            let mut s = auberean_text::StringInfo::default();
            s.set_literal(t, false);
            Some(Record::Text(to_view(&s)))
        }
        o @ NativeObject::Adaptor { ty, .. } if *ty == SHOP => {
            store(o).map(|s| Record::Store(Box::new(s)))
        }
        NativeObject::Scenes(v) => Some(Record::Scenes(scenes(v))),
        NativeObject::Packed { ty, bytes } if *ty == native_type::SCENE_INFO_LIST => {
            let mut r = auberean_script::wire::In::new(bytes);
            auberean_script::story::read_scenes(&mut r)
                .ok()
                .map(|v| Record::Scenes(scenes(&v)))
        }
        NativeObject::Quest(q) => Some(Record::Quest(Box::new(quest(q)))),
        NativeObject::DamageText(d) => Some(Record::Damage(HitText {
            text: String::from_utf16_lossy(&d.text),
            color: d.color.map(f32::from_bits),
            seconds: f32::from_bits(d.duration),
            scale: f32::from_bits(d.scale),
        })),
        NativeObject::QuestList(l) => Some(Record::Quests(l.iter().map(quest).collect())),
        o if friend(o).is_some() => friend(o).map(Record::Friend),
        NativeObject::Adaptor { ty, fields } => match *ty {
            adaptor::FELLOWSHIP => Some(Record::Fellowship(Box::new(fellowship(fields)))),
            adaptor::FELLOW => Some(Record::Fellow(Box::new(fellow(fields)))),
            adaptor::ALLEGIANCE_PROFILE => Some(Record::Allegiance(Box::new(allegiance(fields)))),
            _ => None,
        },
        _ => None,
    }
}

/// A script's string reference as the contract carries it.
fn script_text(t: &auberean_script::native_object::TextInfo) -> TextRef {
    to_view(&from_script(t))
}

/// The native type of a shop's adaptor, and of a sale's.
const SHOP: u32 = 0x1AD6;

/// A friend's state, as the friends page reads it.
fn friend(o: &NativeObject) -> Option<FriendState> {
    let s = crate::friends::UserStatus::of(o)?;
    Some(FriendState {
        name: String::from_utf16_lossy(&s.name),
        location: String::from_utf16_lossy(&s.location),
        mode: s.mode,
    })
}

type Fields = BTreeMap<String, Field>;

/// A field of an adaptor: a word, a 64-bit value, a string reference (an empty one when never
/// set).
fn word(f: &Fields, k: &str) -> u32 {
    match f.get(k) {
        Some(Field::Word(w)) => *w,
        _ => 0,
    }
}

fn long(f: &Fields, k: &str) -> u64 {
    match f.get(k) {
        Some(Field::Long(l)) => *l,
        _ => 0,
    }
}

fn text(f: &Fields, k: &str) -> TextRef {
    text_of(f.get(k))
}

fn text_of(f: Option<&Field>) -> TextRef {
    match f {
        Some(Field::Value(v)) => match v.as_ref() {
            NativeObject::TextInfo(t) => script_text(t),
            _ => TextRef::default(),
        },
        _ => TextRef::default(),
    }
}

/// An icon description's layers, each a picture and its color, in order.
fn icon_layers(f: &Fields, k: &str) -> Vec<(u32, u32)> {
    let Some(Field::Value(v)) = f.get(k) else {
        return Vec::new();
    };
    let NativeObject::Packed { bytes, .. } = v.as_ref() else {
        return Vec::new();
    };
    let w: Vec<u32> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect();
    let n = w.first().copied().unwrap_or(0) as usize;
    w.get(1..)
        .unwrap_or(&[])
        .as_chunks::<3>()
        .0
        .iter()
        .take(n)
        .map(|l| (l[1], l[2]))
        .collect()
}

/// A shop's adaptor as the shop windows read it: its sales' adaptors in order.
fn store(o: &NativeObject) -> Option<Store> {
    let NativeObject::Adaptor { ty, fields } = o else {
        return None;
    };
    if *ty != SHOP {
        return None;
    }
    let sales = match fields.get("Sale") {
        Some(Field::Many(v)) => v
            .iter()
            .filter_map(|s| match s {
                Field::Value(a) => match a.as_ref() {
                    NativeObject::Adaptor { fields: f, .. } => Some(Sale {
                        item: long(f, "ItemIID"),
                        item_data: word(f, "ItemDID"),
                        icon: word(f, "ItemIconID"),
                        icon_layers: icon_layers(f, "ItemIconDesc"),
                        description: text(f, "ItemDesc"),
                        cost_description: text(f, "CostDesc"),
                        cost_icon: word(f, "CostIcon"),
                        cost_quantity: f32::from_bits(word(f, "CostQuantity")),
                        stock: word(f, "StockQuantity").cast_signed(),
                        ordinal: word(f, "Ordinal"),
                        state: word(f, "State"),
                    }),
                    _ => None,
                },
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    Some(Store {
        id: word(fields, "ID"),
        name: text(fields, "Name"),
        description: text(fields, "Desc"),
        sorter: text(fields, "Sorter"),
        portrait: word(fields, "Portrait"),
        sales,
        position: word(fields, "Position").cast_signed(),
        size: word(fields, "StoreSize").cast_signed(),
    })
}

/// Scenes as the story plays them, and their packed form to give back.
fn scenes(v: &[auberean_script::story::SceneInfo]) -> Scenes {
    use auberean_script::story::track;
    let mut w = auberean_script::wire::Out::new();
    auberean_script::story::write_scenes(&mut w, v);
    Scenes {
        scenes: v
            .iter()
            .map(|s| Scene {
                id: s.id,
                playable: s.playable == 1,
                keyframes: s
                    .keyframes
                    .iter()
                    .map(|k| Keyframe {
                        picture: k.has(track::VIDEO).then_some(k.video),
                        music: k.has(track::AUDIO).then_some(k.audio),
                        voice: k.has(track::VOICE).then_some(k.voice),
                        subtitle: k.has(track::SUBTITLE).then(|| script_text(&k.subtitle)),
                        seconds: k.has_duration().then(|| f64::from_bits(k.duration)),
                    })
                    .collect(),
            })
            .collect(),
        packed: w.buf,
    }
}

/// The interface adaptors' native types.
mod adaptor {
    pub const ALLEGIANCE_PROFILE: u32 = 0x83;
    pub const FELLOW: u32 = 0x93;
    pub const FELLOWSHIP: u32 = 0x94;
}

/// The fields of an adaptor an adaptor holds (none when never set).
fn inner(f: Option<&Field>) -> Fields {
    match f {
        Some(Field::Value(v)) => match v.as_ref() {
            NativeObject::Adaptor { fields, .. } => fields.clone(),
            _ => Fields::new(),
        },
        _ => Fields::new(),
    }
}

/// A fellow as its adaptor holds it.
fn fellow(f: &Fields) -> Fellow {
    Fellow {
        name: text_of(f.get("Name")),
        level: word(f, "Level"),
        health: word(f, "Health"),
        max_health: word(f, "MaxHealth"),
        vigor: word(f, "Vigor"),
        max_vigor: word(f, "MaxVigor"),
        health_taken_by_players: word(f, "PKDamage"),
        vigor_taken_by_players: word(f, "PKVigorloss"),
        cell: word(f, "Cell"),
    }
}

/// A fellowship as its adaptor holds it: its fellows in the order added.
#[allow(clippy::cast_possible_wrap)]
fn fellowship(f: &Fields) -> Fellowship {
    let fellows = match f.get("Fellow") {
        Some(Field::Many(all)) => all
            .iter()
            .filter_map(|e| match e {
                Field::Many(pair) => match pair.as_slice() {
                    [Field::Long(id), a] => Some((*id, fellow(&inner(Some(a))))),
                    _ => None,
                },
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    let name = match f.get("Name") {
        Some(Field::Value(v)) => match v.as_ref() {
            NativeObject::Text(units) => String::from_utf16_lossy(units),
            _ => String::new(),
        },
        _ => String::new(),
    };
    Fellowship {
        fellows,
        name,
        leader: long(f, "Leader"),
        social: word(f, "Social") as i32,
        looting: word(f, "LootingMethod") as i32,
    }
}

/// An allegiance member as its adaptor holds one.
fn member(f: &Fields) -> Member {
    Member {
        id: long(f, "ID"),
        name: text_of(f.get("Name")),
        title: text_of(f.get("Title")),
        experience_passed: long(f, "XPPassed"),
        species: word(f, "Species"),
        level: word(f, "Level"),
        gender: word(f, "Gender"),
        officer: word(f, "Officer") != 0,
        faction: word(f, "Faction"),
        rank: word(f, "Rank"),
    }
}

/// A member and whether online, as a profile holds each (none when never set).
fn member_flag(f: Option<&Field>) -> (Member, bool) {
    match f {
        Some(Field::Many(pair)) => match pair.as_slice() {
            [a, Field::Word(on)] => (member(&inner(Some(a))), *on != 0),
            _ => (Member::default(), false),
        },
        _ => (Member::default(), false),
    }
}

/// An allegiance's profile as its adaptor holds it.
fn allegiance(f: &Fields) -> Allegiance {
    let vassals = match f.get("Vassal") {
        Some(Field::Many(all)) => all.iter().map(|v| member_flag(Some(v))).collect(),
        _ => Vec::new(),
    };
    let officers = match f.get("Officer") {
        Some(Field::Many(all)) => all
            .iter()
            .filter_map(|o| match o {
                Field::Long(id) => Some(*id),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    Allegiance {
        monarch: member_flag(f.get("Monarch")),
        patron: member_flag(f.get("Patron")),
        member: member_flag(f.get("Member")),
        vassals,
        total: word(f, "Total"),
        message: text_of(f.get("MotD")),
        name: text_of(f.get("AllegianceName")),
        faction: word(f, "FactionType"),
        officers,
    }
}

fn request(r: &Request) -> ExaminationRequest {
    ExaminationRequest {
        kind: r.kind,
        object: r.object,
        tabs: r.current,
        data_enum: r.data_enum,
        data_id: r.data_id,
    }
}

/// A profile's lines as the examination window lays them out: a string reference written by the
/// window, an icon by its layers' pictures and colors.
fn profile(p: &Profile) -> ExaminationProfile {
    ExaminationProfile {
        request: ExaminationRequest {
            tabs: p.asked(),
            ..request(&Request::unpack(&p.request))
        },
        tabs: p.tabs,
        lines: lines(p),
    }
}

fn lines(p: &Profile) -> Vec<ExaminationLine> {
    p.nodes
        .iter()
        .map(|n| ExaminationLine {
            kind: n.kind,
            tabs: n.tabs,
            appearance: n.appearance,
            value: n.value,
            time: n.time,
            text: (n.kind == kind::STRING).then(|| script_text(&n.text)),
            icon: n.icon.iter().map(|l| (l[1], l[2])).collect(),
            effect: Vec::new(),
        })
        .collect()
}

pub(crate) fn quest(q: &QuestInfo) -> Quest {
    Quest {
        id: q.id,
        name: script_text(&q.name),
        description: script_text(&q.description),
        journal: script_text(&q.journal),
        status: q.status,
        challenge: q.challenge,
        given: f64::from_bits(q.bestowed),
        maxed_out: q.maxed_out(),
        seconds_remaining: q.seconds_remaining(),
        cancellable: q.cancellable != 0,
    }
}

impl crate::Client {
    /// The notices the scripts sent the panels since they were last taken, in order, read as the
    /// contract carries them; [`crate::Client::notices`] is empty after.
    pub fn take_notices(&mut self) -> Vec<Notice> {
        self.views.notices_asked = 0;
        self.targeting.taken(&self.notices);
        let notices: Vec<Notice> = std::mem::take(&mut self.notices).iter().map(read).collect();
        if self.hook.is_some() {
            self.hook_notices.taken(&notices);
        }
        notices
    }
}
