//! A weenie's fields: one table the Definition pane and the editor both draw from. Each field is
//! one idea by its plain name (a property, several properties that go together, some of a
//! property's bits, where it leads, a list of rows), in a section, shown for the kinds it is for
//! and for any weenie that sets it, or while another field has a value it goes with (a
//! generator's set times while it runs between them). Its value is chosen rather than typed as a
//! number where it can be; a set of bits is ticked in a dropdown of check boxes. The server's
//! names for what a field writes are in its tooltip, but for those under a portal's "Usable by",
//! which have none.
//!
//! The editor draws a weenie's sections as pages, a list of them on the left and Advanced after
//! them: every property and row the other pages do not show, or with "Show all" every one. For
//! the pack's change of a server's weenie, each field that differs from the server's is marked,
//! says the server's value, and is put back by a right click.

use lyceum_foundry::content::{self, PropertyKind, Schema, SqlDoc};
use lyceum_foundry::edit::Weenie;

use super::collections;
use super::definition::Drill;
use super::form::FormEdit;
use super::kinds::{int_bits, Kind, Section};
use super::{EditorApp, UiAction};

/// Where a field's value lives in the weenie's SQL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Property(PropertyKind, u32),
    /// Some of a property's bits, those of the mask: a part of what it says, edited apart.
    Bits(PropertyKind, u32, u32),
    /// Several properties edited as one idea, each a part of it.
    Parts(&'static [Part]),
    /// Where it sends who uses it.
    Destination,
    /// Rows of a table: what it wields, holds or sells, its spells, what it spawns, its emotes,
    /// its skills, its body's parts.
    List(Drill),
    /// Its six attributes, and its three vitals.
    Attributes,
    Vitals,
}

/// One property of a field made of several.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Part {
    /// Its plain name's key in the text, `fields.<key>`; none for a part shown by its value alone.
    pub key: Option<&'static str>,
    pub kind: PropertyKind,
    pub id: u32,
    pub widget: Widget,
}

/// How a field's value is chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Widget {
    Text,
    /// Words players read as a paragraph: several lines, wrapped.
    Lines,
    Whole,
    Decimal,
    /// A decimal in meters, or in seconds.
    Meters,
    Seconds,
    /// Yes or no; the value the server takes while the weenie sets none.
    YesNo(bool),
    /// Yes or no, the opposite of the property: yes while it is no. The property's value the
    /// server takes while the weenie sets none.
    Opposite(bool),
    /// Yes or no, written to each of its parts alike: yes while any part is. The value the server
    /// takes while the weenie sets none.
    Together(bool),
    /// One of these values, each by its key in the text (`fields.<key>`).
    Choice(&'static [(i64, &'static str)]),
    /// Unset, by the first key, or one of these values, each by its key: the server runs any
    /// other value (0 among them) as unset, and it reads so, with its number but for 0.
    ChoiceOrUnset(&'static str, &'static [(i64, &'static str)]),
    /// A date and time in UTC, written as seconds since 1970 began; unset or 0 is none.
    DateTime,
    /// Any of these bits, each a check box in a dropdown; closed, it names those ticked.
    Menu(&'static [(i64, &'static str)]),
    /// Any of the bits of the server's enum of this name, each a check box in a dropdown, by the
    /// server's names.
    NamedMenu(&'static str),
    /// Any of these bits, each a check box ticked while its bit is clear: what it allows.
    Allow(&'static [(i64, &'static str)]),
    /// One of these values of the field's bits, each by its key in the text.
    OneOf(&'static [(i64, &'static str)]),
    /// One value of the server's enum of this name, by the server's names.
    Named(&'static str),
    /// One of the server's properties of this type, by its name.
    PropertyName(PropertyKind),
    /// One of the world's loot tables of this kind, by its id or a pack's symbol.
    Treasure(lyceum_foundry::loot::LootKind),
    /// A data file's id, or an object's.
    DataId,
    /// The model it is drawn with, or its icon, chosen from pictures.
    Model,
    Icon,
    /// A base palette, a sound table or a motion table, chosen from a list of those weenies use
    /// (the model's first) and the files' others.
    Table(super::pickers::Picker),
    /// Its clothing look, chosen from pictures of it in each; its color scheme, from the colors
    /// of each; and its shade, on a slider.
    ClothingLook,
    ColorScheme,
    Shade,
    /// A spell, by its id.
    Spell,
    /// Each of its parts, side by side.
    Parts,
    /// What wielding it needs: a requirement, what it is of, and how much.
    Wield,
    /// A list of rows.
    List,
}

/// A field of a weenie's form.
#[derive(Debug)]
pub struct Field {
    /// Its plain name's key in the text, `fields.<key>`; its description is `fields.<key>_about`.
    pub key: &'static str,
    pub section: Section,
    pub target: Target,
    pub widget: Widget,
    /// The kinds it is shown for, set or not; every kind when empty. Any weenie that sets it shows
    /// it too.
    pub kinds: &'static [Kind],
    /// Whether it is shown only for a weenie that sets it, whatever its kind: one few weenies set.
    pub when_set: bool,
    /// The property, and its value, the field is shown for: after the field that writes that
    /// property, while the weenie has the value. One shown only when set shows when set too. None
    /// for a field shown whatever the weenie's other values.
    pub shown_while: Option<(PropertyKind, u32, i64)>,
}

const fn field(
    key: &'static str,
    section: Section,
    target: Target,
    widget: Widget,
    kinds: &'static [Kind],
) -> Field {
    Field {
        key,
        section,
        target,
        widget,
        kinds,
        when_set: false,
        shown_while: None,
    }
}

/// A field shown only for a weenie that sets it.
const fn seldom(key: &'static str, section: Section, target: Target, widget: Widget) -> Field {
    Field {
        key,
        section,
        target,
        widget,
        kinds: &[],
        when_set: true,
        shown_while: None,
    }
}

/// A field shown after the field of property `kind` `id` while the weenie's is `is`.
const fn shown_while(
    key: &'static str,
    section: Section,
    target: Target,
    widget: Widget,
    (kind, id, is): (PropertyKind, u32, i64),
) -> Field {
    Field {
        key,
        section,
        target,
        widget,
        kinds: &[],
        when_set: false,
        shown_while: Some((kind, id, is)),
    }
}

const fn part(key: &'static str, kind: PropertyKind, id: u32, widget: Widget) -> Part {
    Part {
        key: Some(key),
        kind,
        id,
        widget,
    }
}

/// A part shown without a name of its own.
const fn unnamed(kind: PropertyKind, id: u32, widget: Widget) -> Part {
    Part {
        key: None,
        kind,
        id,
        widget,
    }
}

const USABLE: &[(i64, &str)] = &[
    (0x01, "usable_no"),
    (0x20, "usable_remote"),
    (0x08, "usable_contained"),
    (0x04, "usable_wielded"),
    (0x30, "usable_viewed_remote"),
    (0x08_0008, "usable_on_contained"),
    (0x20_0008, "usable_on_remote"),
];

/// The item types, each one bit.
const ITEM_TYPES: &[(i64, &str)] = &[
    (0x0000_0001, "item_melee_weapon"),
    (0x0000_0002, "item_armor"),
    (0x0000_0004, "item_clothing"),
    (0x0000_0008, "item_jewelry"),
    (0x0000_0010, "item_creature"),
    (0x0000_0020, "item_food"),
    (0x0000_0040, "item_money"),
    (0x0000_0080, "item_misc"),
    (0x0000_0100, "item_missile_weapon"),
    (0x0000_0200, "item_container"),
    (0x0000_0400, "item_useless"),
    (0x0000_0800, "item_gem"),
    (0x0000_1000, "item_spell_components"),
    (0x0000_2000, "item_writable"),
    (0x0000_4000, "item_key"),
    (0x0000_8000, "item_caster"),
    (0x0001_0000, "item_portal"),
    (0x0002_0000, "item_lockable"),
    (0x0004_0000, "item_trade_note"),
    (0x0008_0000, "item_mana_stone"),
    (0x0010_0000, "item_service"),
    (0x0020_0000, "item_magic_wieldable"),
    (0x0040_0000, "item_cooking_base"),
    (1 << 23, "item_alchemy_base"),
    (0x0100_0000, "item_fletching_base"),
    (0x0400_0000, "item_alchemy_intermediate"),
    (0x0800_0000, "item_fletching_intermediate"),
    (0x1000_0000, "item_life_stone"),
    (0x2000_0000, "item_tinkering_tool"),
    (0x4000_0000, "item_tinkering_material"),
    (0x8000_0000, "item_gameboard"),
];

const MERCHANDISE: &[(i64, &str)] = &[
    (0x0000_0101, "buys_weapons"),
    (0x0000_0002, "buys_armor"),
    (0x0000_0004, "buys_clothing"),
    (0x0000_0008, "buys_jewelry"),
    (0x0000_0020, "buys_food"),
    (0x0000_0800, "buys_gems"),
    (0x0000_1000, "buys_components"),
    (0x0000_2000, "buys_writing"),
    (0x0000_4000, "buys_keys"),
    (0x0000_8000, "buys_casters"),
    (0x0008_0000, "buys_mana_stones"),
    (0x0000_0080, "buys_misc"),
];

const BONDED: &[(i64, &str)] = &[
    (0, "bonded_normal"),
    (1, "bonded_bonded"),
    (2, "bonded_sticky"),
    (-1, "bonded_slippery"),
    (-2, "bonded_destroy"),
];

const ATTUNED: &[(i64, &str)] = &[
    (0, "attuned_normal"),
    (1, "attuned_attuned"),
    (2, "attuned_sticky"),
];

/// Where a thing is worn or held.
const WORN_ON: &[(i64, &str)] = &[
    (0x0000_0001, "body_head"),
    (0x0000_0002, "body_chest_under"),
    (0x0000_0004, "body_abdomen_under"),
    (0x0000_0008, "body_upper_arms_under"),
    (0x0000_0010, "body_lower_arms_under"),
    (0x0000_0020, "body_hands"),
    (0x0000_0040, "body_upper_legs_under"),
    (0x0000_0080, "body_lower_legs_under"),
    (0x0000_0100, "body_feet"),
    (0x0000_0200, "body_chest"),
    (0x0000_0400, "body_abdomen"),
    (0x0000_0800, "body_upper_arms"),
    (0x0000_1000, "body_lower_arms"),
    (0x0000_2000, "body_upper_legs"),
    (0x0000_4000, "body_lower_legs"),
    (0x0000_8000, "slot_neck"),
    (0x0001_0000, "slot_left_wrist"),
    (0x0002_0000, "slot_right_wrist"),
    (0x0004_0000, "slot_left_finger"),
    (0x0008_0000, "slot_right_finger"),
    (0x0010_0000, "slot_melee"),
    (0x0020_0000, "slot_shield"),
    (0x0040_0000, "slot_missile"),
    (1 << 23, "slot_ammo"),
    (0x0100_0000, "slot_held"),
    (0x0200_0000, "slot_two_handed"),
    (0x0400_0000, "slot_trinket"),
    (0x0800_0000, "slot_cloak"),
    (0x1000_0000, "slot_sigil_one"),
    (0x2000_0000, "slot_sigil_two"),
    (0x4000_0000, "slot_sigil_three"),
];

/// The parts of the body a thing covers.
const COVERS: &[(i64, &str)] = &[
    (0x0000_0002, "body_upper_legs_under"),
    (0x0000_0004, "body_lower_legs_under"),
    (0x0000_0008, "body_chest_under"),
    (0x0000_0010, "body_abdomen_under"),
    (0x0000_0020, "body_upper_arms_under"),
    (0x0000_0040, "body_lower_arms_under"),
    (0x0000_0100, "body_upper_legs"),
    (0x0000_0200, "body_lower_legs"),
    (0x0000_0400, "body_chest"),
    (0x0000_0800, "body_abdomen"),
    (0x0000_1000, "body_upper_arms"),
    (0x0000_2000, "body_lower_arms"),
    (0x0000_4000, "body_head"),
    (0x0000_8000, "body_hands"),
    (0x0001_0000, "body_feet"),
];

const DAMAGE_TYPES: &[(i64, &str)] = &[
    (0x001, "damage_slash"),
    (0x002, "damage_pierce"),
    (0x004, "damage_bludgeon"),
    (0x008, "damage_cold"),
    (0x010, "damage_fire"),
    (0x020, "damage_acid"),
    (0x040, "damage_lightning"),
    (0x400, "damage_nether"),
    (0x080, "vital_health"),
    (0x100, "vital_stamina"),
    (0x200, "vital_mana"),
];

const AMMO: &[(i64, &str)] = &[
    (0x000, "ammo_none"),
    (0x001, "ammo_arrows"),
    (0x002, "ammo_quarrels"),
    (0x004, "ammo_darts"),
    (0x008, "ammo_crystal_arrows"),
    (0x010, "ammo_crystal_quarrels"),
    (0x020, "ammo_crystal_darts"),
    (0x040, "ammo_chorizite_arrows"),
    (0x080, "ammo_chorizite_quarrels"),
    (0x100, "ammo_chorizite_darts"),
];

/// The vitals, as the server numbers their current values.
const VITALS: &[(i64, &str)] = &[(2, "vital_health"), (4, "vital_stamina"), (6, "vital_mana")];

/// What wielding a thing can need.
const WIELD_NEEDS: &[(i64, &str)] = &[
    (0, "wield_nothing"),
    (1, "wield_skill"),
    (2, "wield_base_skill"),
    (3, "wield_attribute"),
    (4, "wield_base_attribute"),
    (5, "wield_vital"),
    (6, "wield_base_vital"),
    (7, "wield_level"),
    (8, "wield_training"),
    (9, "wield_number"),
    (10, "wield_yes_no"),
    (11, "wield_creature_kind"),
    (12, "wield_heritage"),
];

/// Who may use a portal, by the bits that turn each away: its player killer statuses, those
/// with vitae, new accounts, and Olthoi players; and whether it can be summoned, and tied or
/// recalled to.
mod portal {
    pub const ANYONE: u32 = 0x001;
    pub const PK: &[(i64, &str)] = &[
        (0x002, "allow_pk"),
        (0x004, "allow_pkl"),
        (0x008, "allow_npk"),
    ];
    pub const VITAE: &[(i64, &str)] = &[(0x100, "allow_vitae")];
    pub const NEW_ACCOUNTS: &[(i64, &str)] = &[(0x200, "allow_new_accounts")];
    pub const OLTHOI: &[(i64, &str)] = &[
        (0, "olthoi_allowed"),
        (0x080, "olthoi_not_allowed"),
        (0x040, "olthoi_only"),
    ];
    pub const MAGIC: &[(i64, &str)] = &[(0x010, "can_be_summoned"), (0x020, "can_be_recalled")];
    /// Every player killer status turned away: no player can use it.
    pub const NOBODY: u32 = 0x00E;
}

/// The fields under "Usable by" on a portal's page, the first after its heading; none of them
/// has a tooltip.
const USABLE_BY: &[&str] = &[
    "pk_status",
    "vitae_penalty",
    "account_status",
    "olthoi",
    "min_level",
    "max_level",
    "needs_quest",
    "portal_magic",
];

/// When a generator runs: always while unset, else between its set times, during its event, at
/// night or by day. The server runs any other value always.
const GENERATOR_TIME: &[(i64, &str)] = &[
    (RUNS_BETWEEN, "runs_between"),
    (RUNS_EVENT, "runs_event"),
    (4, "runs_night"),
    (5, "runs_day"),
];

/// When a generator runs (Int 142): between its set times, and during its event.
const RUNS_BETWEEN: i64 = 1;
const RUNS_EVENT: i64 = 3;

/// What using, switching or stepping on it does: any of these together.
const ACTIVATION: &[(i64, &str)] = &[
    (0x2, "activation_use"),
    (0x4, "activation_animate"),
    (0x10, "activation_talk"),
    (0x800, "activation_emote"),
    (0x1000, "activation_cast"),
    (0x1_0000, "activation_generate"),
];

use PropertyKind::{Bool, DataId, Float, Int, String as Str};
use Section as S;
use Widget as W;

const WIELD: &[Part] = &[
    unnamed(Int, 158, W::Choice(WIELD_NEEDS)),
    unnamed(Int, 159, W::Whole),
    part("wield_at_least", Int, 160, W::Whole),
];

const PROTECTION: &[Part] = &[
    part("damage_slash", Float, 13, W::Decimal),
    part("damage_pierce", Float, 14, W::Decimal),
    part("damage_bludgeon", Float, 15, W::Decimal),
    part("damage_cold", Float, 16, W::Decimal),
    part("damage_fire", Float, 17, W::Decimal),
    part("damage_acid", Float, 18, W::Decimal),
    part("damage_lightning", Float, 19, W::Decimal),
    part("damage_nether", Float, 165, W::Decimal),
];

const RESISTS: &[Part] = &[
    part("damage_slash", Float, 64, W::Decimal),
    part("damage_pierce", Float, 65, W::Decimal),
    part("damage_bludgeon", Float, 66, W::Decimal),
    part("damage_cold", Float, 68, W::Decimal),
    part("damage_fire", Float, 67, W::Decimal),
    part("damage_acid", Float, 69, W::Decimal),
    part("damage_lightning", Float, 70, W::Decimal),
    part("damage_nether", Float, 166, W::Decimal),
];

const REGENERATION: &[Part] = &[
    part("vital_health", Float, 3, W::Decimal),
    part("vital_stamina", Float, 4, W::Decimal),
    part("vital_mana", Float, 5, W::Decimal),
];

const MANA: &[Part] = &[
    part("mana_most", Int, 108, W::Whole),
    part("mana_now", Int, 107, W::Whole),
];

const USES: &[Part] = &[
    part("uses_most", Int, 91, W::Whole),
    part("uses_left", Int, 92, W::Whole),
];

const RESTORES: &[Part] = &[
    unnamed(Int, 89, W::Choice(VITALS)),
    unnamed(Int, 90, W::Whole),
];

/// Whether it is locked when it appears, and whether it locks again when it resets: one switch,
/// as the server runs a door or a chest locked by either.
const LOCKED: &[Part] = &[
    unnamed(Bool, 3, W::YesNo(false)),
    unnamed(Bool, 35, W::YesNo(false)),
];

/// The skill a player needs to use it, and at least how much.
const SKILL_TO_USE: &[Part] = &[
    unnamed(DataId, 37, W::Named("Skill")),
    part("wield_at_least", Int, 115, W::Whole),
];

/// The kind of creature it does more damage to, and how much more.
const SLAYER: &[Part] = &[
    unnamed(Int, 166, W::Named("CreatureType")),
    unnamed(Float, 138, W::Decimal),
];

/// The spell it may cast when it hits, and the chance it does.
const CAST_ON_HIT: &[Part] = &[
    unnamed(DataId, 55, W::Spell),
    part("cast_chance", Float, 156, W::Decimal),
];

/// How long before it can be used again, and the group of things whose wait it shares.
const COOLDOWN: &[Part] = &[
    unnamed(Float, 167, W::Seconds),
    part("cooldown_group", Int, 280, W::Whole),
];

/// What using it makes, by its class id, and how many.
const MAKES_WHEN_USED: &[Part] = &[
    unnamed(DataId, 38, W::Whole),
    part("how_many", Int, 269, W::Whole),
];

/// A vendor's happiness: its mean, and how far it strays.
const HAPPINESS: &[Part] = &[
    part("happiness_mean", Int, 126, W::Whole),
    part("happiness_variance", Int, 127, W::Whole),
];

use Kind::{
    Ammo, Armor, Book, Caster, Chest, Clothing, Container, Creature, Door, Food, Gem, Generator,
    Healer, Hotspot, Item, Jewelry, Key, MeleeWeapon, MissileWeapon, Npc, Portal, PressurePlate,
    Scroll, Switch, Vendor,
};

/// Every item: what is picked up, carried, worn or wielded.
const ITEMS: &[Kind] = &[
    MeleeWeapon,
    MissileWeapon,
    Ammo,
    Caster,
    Armor,
    Clothing,
    Jewelry,
    Food,
    Gem,
    Key,
    Healer,
    Scroll,
    Book,
    Item,
];
const WORN_OR_WIELDED: &[Kind] = &[
    MeleeWeapon,
    MissileWeapon,
    Ammo,
    Caster,
    Armor,
    Clothing,
    Jewelry,
];
const MAGIC_ITEMS: &[Kind] = &[MeleeWeapon, MissileWeapon, Caster, Armor, Clothing, Jewelry];

/// Every field, by section, each section's in the order they are shown.
pub static FIELDS: &[Field] = &[
    // Basic
    field("name", S::Basic, Target::Property(Str, 1), W::Text, &[]),
    field(
        "description",
        S::Basic,
        Target::Property(Str, 16),
        W::Lines,
        &[
            MeleeWeapon,
            MissileWeapon,
            Ammo,
            Caster,
            Armor,
            Clothing,
            Jewelry,
            Food,
            Gem,
            Key,
            Healer,
            Scroll,
            Book,
            Item,
            Npc,
            Vendor,
            Portal,
            Door,
            Chest,
            Container,
        ],
    ),
    field(
        "use_message",
        S::Basic,
        Target::Property(Str, 14),
        W::Lines,
        &[
            Food, Gem, Key, Healer, Scroll, Book, Item, Door, Chest, Container,
        ],
    ),
    field(
        "model",
        S::Basic,
        Target::Property(DataId, 1),
        W::Model,
        &[],
    ),
    field("icon", S::Basic, Target::Property(DataId, 8), W::Icon, &[]),
    field(
        "item_type",
        S::Basic,
        Target::Property(Int, 1),
        W::Choice(ITEM_TYPES),
        &[],
    ),
    field(
        "usable",
        S::Basic,
        Target::Property(Int, 16),
        W::Choice(USABLE),
        &[],
    ),
    field(
        "value",
        S::Basic,
        Target::Property(Int, 19),
        W::Whole,
        ITEMS,
    ),
    field(
        "weight",
        S::Basic,
        Target::Property(Int, 5),
        W::Whole,
        ITEMS,
    ),
    field(
        "scale",
        S::Basic,
        Target::Property(Float, 39),
        W::Decimal,
        &[],
    ),
    field(
        "hidden",
        S::Basic,
        Target::Property(Bool, 18),
        W::YesNo(false),
        &[Generator, Switch, PressurePlate, Hotspot],
    ),
    field(
        "bonded",
        S::Basic,
        Target::Property(Int, 33),
        W::Choice(BONDED),
        ITEMS,
    ),
    field(
        "attuned",
        S::Basic,
        Target::Property(Int, 114),
        W::Choice(ATTUNED),
        ITEMS,
    ),
    field(
        "sellable",
        S::Basic,
        Target::Property(Bool, 69),
        W::YesNo(true),
        ITEMS,
    ),
    field(
        "stacks_to",
        S::Basic,
        Target::Property(Int, 11),
        W::Whole,
        &[Ammo, Food, Gem, Item],
    ),
    // Appearance
    field(
        "clothing_base",
        S::Appearance,
        Target::Property(DataId, 7),
        W::ClothingLook,
        &[Armor, Clothing, MeleeWeapon, Creature, Npc, Vendor],
    ),
    field(
        "palette",
        S::Appearance,
        Target::Property(Int, 3),
        W::ColorScheme,
        &[Armor, Clothing, MeleeWeapon, Creature, Npc, Vendor],
    ),
    field(
        "shade",
        S::Appearance,
        Target::Property(Float, 12),
        W::Shade,
        &[Armor, Clothing, Creature, Npc, Vendor],
    ),
    // Creature
    field(
        "level",
        S::Creature,
        Target::Property(Int, 25),
        W::Whole,
        &[Creature],
    ),
    field(
        "attackable",
        S::Creature,
        Target::Property(Bool, 19),
        W::YesNo(true),
        &[Creature],
    ),
    field(
        "creature_kind",
        S::Creature,
        Target::Property(Int, 2),
        W::Named("CreatureType"),
        &[Creature],
    ),
    field(
        "xp",
        S::Creature,
        Target::Property(Int, 146),
        W::Whole,
        &[Creature],
    ),
    field(
        "awareness",
        S::Creature,
        Target::Property(Float, 31),
        W::Meters,
        &[Creature],
    ),
    // Only a fight reads it: a kind that never fights shows it when it sets it. Yes: it never
    // moves from where it stands.
    field(
        "stays_put",
        S::Creature,
        Target::Property(Bool, 52),
        W::YesNo(false),
        &[Creature],
    ),
    field(
        "regeneration",
        S::Creature,
        Target::Parts(REGENERATION),
        W::Parts,
        &[Creature],
    ),
    field(
        "resists",
        S::Creature,
        Target::Parts(RESISTS),
        W::Parts,
        &[Creature],
    ),
    // Attributes, skills and body parts
    field(
        "attributes",
        S::Attributes,
        Target::Attributes,
        W::List,
        &[Creature],
    ),
    field(
        "vitals",
        S::Attributes,
        Target::Vitals,
        W::List,
        &[Creature],
    ),
    field(
        "skills",
        S::Skills,
        Target::List(Drill::Skills),
        W::List,
        &[Creature],
    ),
    field(
        "body_parts",
        S::BodyParts,
        Target::List(Drill::BodyParts),
        W::List,
        &[Creature],
    ),
    // NPC
    field(
        "title",
        S::Npc,
        Target::Property(Str, 5),
        W::Text,
        &[Npc, Vendor],
    ),
    // Trade: what a vendor buys, then what it sells
    field(
        "buys",
        S::Trade,
        Target::Property(Int, 74),
        W::Menu(MERCHANDISE),
        &[Vendor],
    ),
    field(
        "pays_most",
        S::Trade,
        Target::Property(Int, 76),
        W::Whole,
        &[Vendor],
    ),
    field(
        "buy_rate",
        S::Trade,
        Target::Property(Float, 37),
        W::Decimal,
        &[Vendor],
    ),
    field(
        "sell_rate",
        S::Trade,
        Target::Property(Float, 38),
        W::Decimal,
        &[Vendor],
    ),
    field(
        "trades_magic",
        S::Trade,
        Target::Property(Bool, 39),
        W::YesNo(false),
        &[Vendor],
    ),
    // Wearing and wielding: its equipment
    field(
        "worn_on",
        S::Wearing,
        Target::Property(Int, 9),
        W::Menu(WORN_ON),
        WORN_OR_WIELDED,
    ),
    field(
        "covers",
        S::Wearing,
        Target::Property(Int, 4),
        W::Menu(COVERS),
        &[Armor, Clothing],
    ),
    field(
        "wield_needs",
        S::Wearing,
        Target::Parts(WIELD),
        W::Wield,
        WORN_OR_WIELDED,
    ),
    field(
        "arcane_lore",
        S::Wearing,
        Target::Property(Int, 109),
        W::Whole,
        MAGIC_ITEMS,
    ),
    // Weapon
    field(
        "damage",
        S::Weapon,
        Target::Property(Int, 44),
        W::Whole,
        &[MeleeWeapon, MissileWeapon, Ammo],
    ),
    field(
        "damage_type",
        S::Weapon,
        Target::Property(Int, 45),
        W::Menu(DAMAGE_TYPES),
        &[MeleeWeapon, Ammo, Caster],
    ),
    field(
        "damage_spread",
        S::Weapon,
        Target::Property(Float, 22),
        W::Decimal,
        &[MeleeWeapon, Ammo],
    ),
    field(
        "speed",
        S::Weapon,
        Target::Property(Int, 49),
        W::Whole,
        &[MeleeWeapon, MissileWeapon],
    ),
    field(
        "weapon_skill",
        S::Weapon,
        Target::Property(Int, 48),
        W::Named("Skill"),
        &[MeleeWeapon, MissileWeapon],
    ),
    field(
        "ammo",
        S::Weapon,
        Target::Property(Int, 50),
        W::Choice(AMMO),
        &[MissileWeapon, Ammo],
    ),
    field(
        "attack_bonus",
        S::Weapon,
        Target::Property(Float, 62),
        W::Decimal,
        &[MeleeWeapon, MissileWeapon, Caster],
    ),
    field(
        "defense_bonus",
        S::Weapon,
        Target::Property(Float, 29),
        W::Decimal,
        &[MeleeWeapon, MissileWeapon, Caster],
    ),
    field(
        "damage_multiplier",
        S::Weapon,
        Target::Property(Float, 63),
        W::Decimal,
        &[MissileWeapon],
    ),
    // Protection: an item's, with its equipment
    field(
        "armor_level",
        S::Protection,
        Target::Property(Int, 28),
        W::Whole,
        &[Armor, Clothing],
    ),
    field(
        "protection",
        S::Protection,
        Target::Parts(PROTECTION),
        W::Parts,
        &[Armor, Clothing, Creature],
    ),
    // Activation: what using it does
    field(
        "used_on",
        S::Activation,
        Target::Property(Int, 94),
        W::Menu(ITEM_TYPES),
        &[Key, Gem, Item],
    ),
    field(
        "restores",
        S::Activation,
        Target::Parts(RESTORES),
        W::Parts,
        &[Food, Healer],
    ),
    field(
        "uses",
        S::Activation,
        Target::Parts(USES),
        W::Parts,
        &[Key, Healer, Item],
    ),
    field(
        "opens_locks",
        S::Activation,
        Target::Property(Str, 13),
        W::Text,
        &[Key],
    ),
    // Magic
    field(
        "spellcraft",
        S::Magic,
        Target::Property(Int, 106),
        W::Whole,
        MAGIC_ITEMS,
    ),
    field("mana", S::Magic, Target::Parts(MANA), W::Parts, MAGIC_ITEMS),
    field(
        "mana_drain",
        S::Magic,
        Target::Property(Float, 5),
        W::Decimal,
        MAGIC_ITEMS,
    ),
    field(
        "casts",
        S::Magic,
        Target::Property(DataId, 28),
        W::Spell,
        &[Caster, Gem, Scroll],
    ),
    field(
        "spells",
        S::Magic,
        Target::List(Drill::Spells),
        W::List,
        &[
            MeleeWeapon,
            MissileWeapon,
            Caster,
            Armor,
            Clothing,
            Jewelry,
            Creature,
        ],
    ),
    // Portal
    field(
        "destination",
        S::Portal,
        Target::Destination,
        W::Text,
        &[Portal],
    ),
    field(
        "shows_destination",
        S::Portal,
        Target::Property(Bool, 88),
        W::YesNo(true),
        &[Portal],
    ),
    // Usable by: who may use it, then whether it can be summoned and recalled to
    field(
        "pk_status",
        S::Portal,
        Target::Bits(Int, 111, 0x00E),
        W::Allow(portal::PK),
        &[Portal],
    ),
    field(
        "vitae_penalty",
        S::Portal,
        Target::Bits(Int, 111, 0x100),
        W::Allow(portal::VITAE),
        &[Portal],
    ),
    field(
        "account_status",
        S::Portal,
        Target::Bits(Int, 111, 0x200),
        W::Allow(portal::NEW_ACCOUNTS),
        &[Portal],
    ),
    field(
        "olthoi",
        S::Portal,
        Target::Bits(Int, 111, 0x0C0),
        W::OneOf(portal::OLTHOI),
        &[Portal],
    ),
    field(
        "min_level",
        S::Portal,
        Target::Property(Int, 86),
        W::Whole,
        &[Portal],
    ),
    field(
        "max_level",
        S::Portal,
        Target::Property(Int, 87),
        W::Whole,
        &[Portal],
    ),
    field(
        "needs_quest",
        S::Portal,
        Target::Property(Str, 37),
        W::Text,
        &[Portal],
    ),
    field(
        "portal_magic",
        S::Portal,
        Target::Bits(Int, 111, 0x030),
        W::Allow(portal::MAGIC),
        &[Portal],
    ),
    // Lock
    field(
        "locked",
        S::Lock,
        Target::Parts(LOCKED),
        W::Together(false),
        &[Door, Chest],
    ),
    field(
        "lock_difficulty",
        S::Lock,
        Target::Property(Int, 38),
        W::Whole,
        &[Door, Chest],
    ),
    field(
        "lock_code",
        S::Lock,
        Target::Property(Str, 12),
        W::Text,
        &[Door, Chest],
    ),
    field(
        "starts_open",
        S::Lock,
        Target::Property(Bool, 34),
        W::YesNo(false),
        &[Door, Chest],
    ),
    field(
        "resets_after",
        S::Lock,
        Target::Property(Float, 11),
        W::Seconds,
        &[Door, Chest],
    ),
    // Holds: what it holds, and what it drops when it dies
    field(
        "holds",
        S::Holds,
        Target::Property(Int, 6),
        W::Whole,
        &[Chest, Container],
    ),
    field(
        "packs_inside",
        S::Holds,
        Target::Property(Int, 7),
        W::Whole,
        &[Chest, Container],
    ),
    field(
        "holds_weight",
        S::Holds,
        Target::Property(Int, 96),
        W::Whole,
        &[Chest, Container],
    ),
    field(
        "contents",
        S::Holds,
        Target::List(Drill::Holds),
        W::List,
        &[Chest, Container],
    ),
    // Spawns: the generator's own, then what it spawns
    field(
        "most_at_once",
        S::Spawns,
        Target::Property(Int, 81),
        W::Whole,
        &[Generator],
    ),
    field(
        "at_start",
        S::Spawns,
        Target::Property(Int, 82),
        W::Whole,
        &[Generator],
    ),
    field(
        "respawns_every",
        S::Spawns,
        Target::Property(Float, 41),
        W::Seconds,
        &[Generator],
    ),
    field(
        "spread",
        S::Spawns,
        Target::Property(Float, 43),
        W::Meters,
        &[Generator],
    ),
    field(
        "starts_after",
        S::Spawns,
        Target::Property(Float, 121),
        W::Seconds,
        &[Generator],
    ),
    field(
        "runs",
        S::Spawns,
        Target::Property(Int, 142),
        W::ChoiceOrUnset("runs_always", GENERATOR_TIME),
        &[Generator],
    ),
    // Between set times: from when, until when, each none while unset.
    shown_while(
        "runs_from",
        S::Spawns,
        Target::Property(Int, 143),
        W::DateTime,
        (Int, 142, RUNS_BETWEEN),
    ),
    shown_while(
        "runs_until",
        S::Spawns,
        Target::Property(Int, 144),
        W::DateTime,
        (Int, 142, RUNS_BETWEEN),
    ),
    Field {
        shown_while: Some((Int, 142, RUNS_EVENT)),
        ..seldom(
            "generator_event",
            S::Spawns,
            Target::Property(Str, 34),
            W::Text,
        )
    },
    seldom(
        "when_destroyed",
        S::Spawns,
        Target::Property(Int, 103),
        W::Named("GeneratorDestruct"),
    ),
    seldom(
        "when_ended",
        S::Spawns,
        Target::Property(Int, 145),
        W::Named("GeneratorDestruct"),
    ),
    field(
        "spawns",
        S::Spawns,
        Target::List(Drill::Spawns),
        W::List,
        &[Generator],
    ),
    // Activation: what a switch, a pressure plate or a hotspot does
    field(
        "switched_on",
        S::Activation,
        Target::Property(Int, 119),
        W::YesNo(true),
        &[Switch, PressurePlate, Hotspot],
    ),
    field(
        "when_used",
        S::Activation,
        Target::Property(Int, 83),
        W::Menu(ACTIVATION),
        &[Switch, PressurePlate],
    ),
    field(
        "says_when_triggered",
        S::Activation,
        Target::Property(Str, 17),
        W::Lines,
        &[Hotspot, PressurePlate, Switch],
    ),
    // What its kind sets most often besides: each after its section's own.
    // Basic
    field(
        "plural_name",
        S::Basic,
        Target::Property(Str, 20),
        W::Text,
        &[Food, Ammo, Gem, Item],
    ),
    field(
        "short_description",
        S::Basic,
        Target::Property(Str, 15),
        W::Lines,
        &[Scroll],
    ),
    seldom(
        "icon_overlay",
        S::Basic,
        Target::Property(DataId, 50),
        W::DataId,
    ),
    field(
        "use_radius",
        S::Basic,
        Target::Property(Float, 54),
        W::Meters,
        &[Portal, Vendor, Chest, Door],
    ),
    field(
        "inscribable",
        S::Basic,
        Target::Property(Bool, 22),
        W::YesNo(false),
        MAGIC_ITEMS,
    ),
    field(
        "destroy_on_sell",
        S::Basic,
        Target::Property(Bool, 23),
        W::YesNo(false),
        &[Scroll],
    ),
    field(
        "stack_size",
        S::Basic,
        Target::Property(Int, 12),
        W::Whole,
        &[Ammo, Food],
    ),
    field(
        "stack_unit_value",
        S::Basic,
        Target::Property(Int, 15),
        W::Whole,
        &[Ammo, Food],
    ),
    field(
        "stack_unit_weight",
        S::Basic,
        Target::Property(Int, 13),
        W::Whole,
        &[Ammo, Food],
    ),
    seldom("quest", S::Basic, Target::Property(Str, 33), W::Text),
    field(
        "lights",
        S::Basic,
        Target::Property(Bool, 15),
        W::YesNo(false),
        &[Portal],
    ),
    field(
        "radar",
        S::Basic,
        Target::Property(Int, 133),
        W::Named("RadarBehavior"),
        &[Creature, Npc, Vendor, Portal],
    ),
    field(
        "radar_color",
        S::Basic,
        Target::Property(Int, 95),
        W::Named("RadarColor"),
        &[Npc],
    ),
    // Appearance
    field(
        "palette_base",
        S::Appearance,
        Target::Property(DataId, 6),
        W::Table(super::pickers::Picker::Palette),
        &[Armor, Clothing],
    ),
    seldom(
        "translucency",
        S::Appearance,
        Target::Property(Float, 76),
        W::Decimal,
    ),
    field(
        "motion_table",
        S::Appearance,
        Target::Property(DataId, 2),
        W::Table(super::pickers::Picker::Motion),
        &[Creature, Npc, Vendor],
    ),
    field(
        "sound_table",
        S::Appearance,
        Target::Property(DataId, 3),
        W::Table(super::pickers::Picker::Sound),
        &[Creature, Npc, Vendor],
    ),
    // Creature
    field(
        "targeting",
        S::Creature,
        Target::Property(Int, 68),
        W::NamedMenu("TargetingTactic"),
        &[Creature],
    ),
    field(
        "fights_with",
        S::Creature,
        Target::Property(Int, 101),
        W::NamedMenu("CombatStyle"),
        &[Creature],
    ),
    seldom(
        "tolerance",
        S::Creature,
        Target::Property(Int, 67),
        W::NamedMenu("Tolerance"),
    ),
    seldom(
        "friendly_with",
        S::Creature,
        Target::Property(Int, 72),
        W::Named("CreatureType"),
    ),
    seldom(
        "no_corpse",
        S::Creature,
        Target::Property(Bool, 29),
        W::YesNo(false),
    ),
    seldom(
        "kill_quest",
        S::Creature,
        Target::Property(Str, 45),
        W::Text,
    ),
    // NPC
    field(
        "accepts_items",
        S::Npc,
        Target::Property(Bool, 8),
        W::YesNo(false),
        &[Npc],
    ),
    field(
        "looks_like_object",
        S::Npc,
        Target::Property(Bool, 83),
        W::YesNo(false),
        &[Npc],
    ),
    field(
        "player_killer_status",
        S::Npc,
        Target::Property(Int, 134),
        W::Named("PlayerKillerStatus"),
        &[Npc, Vendor],
    ),
    seldom(
        "gender",
        S::Npc,
        Target::Property(Int, 113),
        W::Named("Gender"),
    ),
    seldom(
        "heritage",
        S::Npc,
        Target::Property(Int, 188),
        W::Named("HeritageGroup"),
    ),
    // Vendor
    field(
        "pays_least",
        S::Trade,
        Target::Property(Int, 75),
        W::Whole,
        &[Vendor],
    ),
    field(
        "happiness",
        S::Trade,
        Target::Parts(HAPPINESS),
        W::Parts,
        &[Vendor],
    ),
    field(
        "town",
        S::Trade,
        Target::Property(Str, 24),
        W::Text,
        &[Vendor],
    ),
    seldom(
        "sells_services",
        S::Trade,
        Target::Property(Bool, 51),
        W::YesNo(false),
    ),
    // Equipment
    seldom(
        "skill_to_use",
        S::Wearing,
        Target::Parts(SKILL_TO_USE),
        W::Parts,
    ),
    field(
        "hook_type",
        S::Wearing,
        Target::Property(Int, 151),
        W::NamedMenu("HookType"),
        &[MeleeWeapon, MissileWeapon, Caster],
    ),
    seldom(
        "equipment_set",
        S::Wearing,
        Target::Property(Int, 265),
        W::Named("EquipmentSet"),
    ),
    // Weapon
    field(
        "combat_style",
        S::Weapon,
        Target::Property(Int, 46),
        W::Named("CombatStyle"),
        &[MeleeWeapon, MissileWeapon, Caster, Ammo],
    ),
    field(
        "attack_type",
        S::Weapon,
        Target::Property(Int, 47),
        W::NamedMenu("AttackType"),
        &[MeleeWeapon],
    ),
    field(
        "weapon_type",
        S::Weapon,
        Target::Property(Int, 353),
        W::Named("WeaponType"),
        &[MeleeWeapon, MissileWeapon],
    ),
    seldom(
        "critical_chance",
        S::Weapon,
        Target::Property(Float, 147),
        W::Decimal,
    ),
    seldom(
        "critical_multiplier",
        S::Weapon,
        Target::Property(Float, 136),
        W::Decimal,
    ),
    seldom("slayer", S::Weapon, Target::Parts(SLAYER), W::Parts),
    seldom(
        "elemental_bonus",
        S::Weapon,
        Target::Property(Int, 204),
        W::Whole,
    ),
    seldom(
        "elemental_modifier",
        S::Weapon,
        Target::Property(Float, 152),
        W::Decimal,
    ),
    field(
        "mana_conversion",
        S::Weapon,
        Target::Property(Float, 144),
        W::Decimal,
        &[Caster],
    ),
    seldom(
        "cast_on_hit",
        S::Weapon,
        Target::Parts(CAST_ON_HIT),
        W::Parts,
    ),
    // Protection
    seldom(
        "resist_magic",
        S::Protection,
        Target::Property(Int, 36),
        W::Whole,
    ),
    // Lock
    field(
        "open",
        S::Lock,
        Target::Property(Bool, 2),
        W::YesNo(false),
        &[Door, Chest],
    ),
    // Items
    seldom(
        "refills_on_close",
        S::Holds,
        Target::Property(Bool, 86),
        W::YesNo(false),
    ),
    // Activation
    field(
        "cooldown",
        S::Activation,
        Target::Parts(COOLDOWN),
        W::Parts,
        &[Gem],
    ),
    field(
        "heal_bonus",
        S::Activation,
        Target::Property(Float, 100),
        W::Decimal,
        &[Healer],
    ),
    seldom(
        "makes_when_used",
        S::Activation,
        Target::Parts(MAKES_WHEN_USED),
        W::Parts,
    ),
    seldom(
        "linked_portal",
        S::Activation,
        Target::Property(DataId, 31),
        W::Whole,
    ),
    field(
        "failure_message",
        S::Activation,
        Target::Property(Str, 22),
        W::Lines,
        &[Switch],
    ),
    field(
        "hot",
        S::Activation,
        Target::Property(Bool, 55),
        W::YesNo(false),
        &[Hotspot],
    ),
    field(
        "hot_cycle",
        S::Activation,
        Target::Property(Float, 105),
        W::Seconds,
        &[Hotspot],
    ),
    // Holds: a creature's death treasure
    field(
        "loot",
        S::Holds,
        Target::Property(DataId, 35),
        W::Treasure(lyceum_foundry::loot::LootKind::Death),
        &[Creature],
    ),
    // Creature: the wielded treasure it is made with, then what it wields
    field(
        "wielded_loot",
        S::Creature,
        Target::Property(DataId, 32),
        W::Treasure(lyceum_foundry::loot::LootKind::Wielded),
        &[Creature, Npc],
    ),
    field(
        "wields",
        S::Creature,
        Target::List(Drill::Wields),
        W::List,
        &[Creature, Npc],
    ),
    // Trade: what it sells
    field(
        "sells",
        S::Trade,
        Target::List(Drill::Sells),
        W::List,
        &[Vendor],
    ),
    // Emotes
    field(
        "emotes",
        S::Emotes,
        Target::List(Drill::Emotes),
        W::List,
        &[Creature, Npc, Vendor],
    ),
];

/// The key field `f` is known by in the form: a property's by the property, as the full form
/// knows it, else the field's own.
#[must_use]
pub fn field_key(f: &Field) -> String {
    match f.target {
        Target::Property(kind, id) => property_key(kind, id),
        _ => format!("flow:{}", f.key),
    }
}

/// The key a property is known by in the form, `<type>:<id>`.
#[must_use]
pub fn property_key(kind: PropertyKind, id: u32) -> String {
    format!("{}:{id}", kind.key())
}

/// The properties field `f` writes.
#[must_use]
pub fn properties(f: &Field) -> Vec<(PropertyKind, u32)> {
    match f.target {
        Target::Property(k, id) | Target::Bits(k, id, _) => vec![(k, id)],
        Target::Parts(parts) => parts.iter().map(|p| (p.kind, p.id)).collect(),
        _ => Vec::new(),
    }
}

/// A property of bits read as the server reads it, as written (`raw`): a portal's unset is
/// anyone (its first bit), a portal's 0 no player at all (as if each player killer status were
/// turned away), and unset activation is use.
#[must_use]
pub fn bits_read(kind: PropertyKind, id: u32, raw: Option<&str>) -> u32 {
    let written = raw.and_then(|v| {
        let v = content::bare(v);
        let v = v.trim();
        v.parse::<i64>()
            .ok()
            .or_else(|| lyceum_foundry::sql_ids::number(v).map(i64::from))
            .map(int_bits)
    });
    match ((kind, id), written) {
        ((Int, 111), None) => portal::ANYONE,
        ((Int, 111), Some(0)) => portal::NOBODY,
        ((Int, 83), None) => 0x2,
        (_, n) => n.unwrap_or(0),
    }
}

/// The value bits `v` of property `kind` `id` are written as: a portal never writes 0, which
/// would turn every player away, but anyone (its first bit) for no bit at all.
#[must_use]
pub fn bits_value(kind: PropertyKind, id: u32, v: u32) -> String {
    let v = match (kind, id) {
        (Int, 111) if v == 0 => portal::ANYONE,
        _ => v,
    };
    v.cast_signed().to_string()
}

/// The value written when the bits of mask `mask` of property `kind` `id` (written `raw`) are
/// set to `to`, the others kept as they read.
#[must_use]
pub fn bits_set(kind: PropertyKind, id: u32, raw: Option<&str>, mask: u32, to: u32) -> String {
    let now = bits_read(kind, id, raw);
    bits_value(kind, id, (now & !mask) | (to & mask))
}

/// A yes-or-no property's value as the server takes it: as written (`value`), else `unset`, the
/// server's value while the weenie sets none. A whole number is yes when it is not 0.
#[must_use]
pub fn yes_no(value: Option<&str>, unset: bool) -> bool {
    value.map_or(unset, |v| {
        let v = content::bare(v);
        match v.trim().parse::<i64>() {
            Ok(n) => n != 0,
            Err(_) => matches!(v.as_str(), "True" | "true" | "\u{1}"),
        }
    })
}

/// What choosing yes (`v`) on a yes-or-no `widget` writes to a property of type `kind`: the
/// opposite for a field that reads its property backwards.
#[must_use]
pub fn yes_no_written(kind: PropertyKind, widget: Widget, v: bool) -> &'static str {
    match (kind, v != matches!(widget, Widget::Opposite(_))) {
        (PropertyKind::Bool, true) => "True",
        (PropertyKind::Bool, false) => "False",
        (_, true) => "1",
        (_, false) => "0",
    }
}

/// How many rows of `doc` list `d` has.
#[must_use]
pub fn count(doc: &SqlDoc, d: Drill) -> usize {
    match d {
        Drill::Emotes => doc.emote_sets().len(),
        Drill::Appears | Drill::UsedBy => 0,
        _ => collections::list_of(doc, d).len(),
    }
}

/// The row of `doc` saying where it sends who uses it.
fn destination_row(doc: &SqlDoc) -> Option<content::RowRef> {
    doc.rows("weenie_properties_position")
        .into_iter()
        .find(|r| doc.cell(*r, "position_Type").as_deref() == Some("2"))
}

/// The properties whose value -1 says nothing is set: what a creature carries.
const MINUS_ONE_UNSET: &[(PropertyKind, u32)] = &[(Int, 6), (Int, 7)];

/// Whether `doc` sets property `id` of type `kind`: -1 is unset for those it means that for.
fn sets(doc: &SqlDoc, kind: PropertyKind, id: u32) -> bool {
    doc.property(kind, id).is_some_and(|v| {
        !(MINUS_ONE_UNSET.contains(&(kind, id)) && content::bare(&v).trim() == "-1")
    })
}

/// Whether `doc` sets anything field `f` shows.
#[must_use]
pub fn has_data(f: &Field, doc: &SqlDoc) -> bool {
    match f.target {
        Target::Property(k, id) | Target::Bits(k, id, _) => sets(doc, k, id),
        Target::Parts(parts) => parts.iter().any(|p| sets(doc, p.kind, p.id)),
        Target::Destination => destination_row(doc).is_some(),
        Target::List(d) => count(doc, d) > 0,
        Target::Attributes => !doc.rows(collections::ATTRIBUTES).is_empty(),
        Target::Vitals => !doc.rows(collections::VITALS).is_empty(),
    }
}

/// Whether fields `a` and `b` write a property in common: two of its bits, only a bit in common.
fn overlaps(a: &Field, b: &Field) -> bool {
    if let (Target::Bits(ka, ia, ma), Target::Bits(kb, ib, mb)) = (a.target, b.target) {
        return (ka, ia) == (kb, ib) && ma & mb != 0;
    }
    let pa = properties(a);
    properties(b).iter().any(|p| pa.contains(p))
}

/// The sections a weenie of `kind` read from `doc` shows, in order, each with its fields: those
/// of its kind's sections that are for it, and any other the weenie sets (every one of Spawns
/// when it spawns things), unless one shown already writes the same property. A field shown
/// while another's property has a value is shown when that field is and the weenie has it. A
/// field is in its section's home for the kind ([`Kind::home`]).
#[must_use]
pub fn shown(kind: Kind, doc: &SqlDoc) -> Vec<(Section, Vec<&'static Field>)> {
    let home = |f: &Field| kind.home(f.section);
    let mine = |f: &Field| {
        !f.when_set
            && f.shown_while.is_none()
            && kind.sections().contains(&home(f))
            && (f.kinds.is_empty() || f.kinds.contains(&kind))
    };
    let mut chosen: Vec<&'static Field> = FIELDS.iter().filter(|f| mine(f)).collect();
    let is_chosen =
        |chosen: &[&'static Field], f: &Field| chosen.iter().any(|c| std::ptr::eq(*c, f));
    if kind.shows_what_it_sets() {
        let generates = !doc.rows("weenie_properties_generator").is_empty()
            || doc.property(PropertyKind::Int, 81).is_some();
        for f in FIELDS {
            if is_chosen(&chosen, f) {
                continue;
            }
            // One shown for another field's value is shown by that value, below; here only when
            // set, if it is one few weenies set.
            let set = match f.shown_while {
                Some(_) => f.when_set && has_data(f, doc),
                None => {
                    has_data(f, doc) || (f.section == Section::Spawns && generates && !f.when_set)
                }
            };
            if set && !chosen.iter().any(|c| overlaps(c, f)) {
                chosen.push(f);
            }
        }
    }
    // Those shown for another field's value, while that field is shown and has it.
    for f in FIELDS {
        let Some((k, id, is)) = f.shown_while else {
            continue;
        };
        let after = chosen.iter().any(|c| properties(c).contains(&(k, id)));
        if after
            && !is_chosen(&chosen, f)
            && int_of(doc, k, id) == Some(is)
            && !chosen.iter().any(|c| overlaps(c, f))
        {
            chosen.push(f);
        }
    }
    Section::ALL
        .iter()
        .filter_map(|s| {
            let fields: Vec<&'static Field> = FIELDS
                .iter()
                .filter(|f| home(f) == *s && chosen.iter().any(|c| std::ptr::eq(*c, *f)))
                .collect();
            (!fields.is_empty()).then_some((*s, fields))
        })
        .collect()
}

/// The pages the editor lists for a weenie of `kind` read from `doc`: its sections as shown,
/// and Emotes for every weenie, so its first emote can be added.
#[must_use]
pub fn pages(kind: Kind, doc: &SqlDoc) -> Vec<(Section, Vec<&'static Field>)> {
    let mut sections = shown(kind, doc);
    if !sections.iter().any(|(s, _)| *s == Section::Emotes) {
        let emotes = FIELDS
            .iter()
            .filter(|f| f.target == Target::List(Drill::Emotes))
            .collect();
        sections.push((Section::Emotes, emotes));
    }
    sections
}

/// What a weenie's pages show of `doc`, which its Advanced page leaves out until it is asked to
/// show everything: each property a field of them writes, and each table a list of them shows
/// every row of (its emotes and their quests by the emotes' table).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct OnPages {
    pub properties: Vec<(PropertyKind, u32)>,
    pub tables: Vec<&'static str>,
}

impl OnPages {
    /// Whether Advanced leaves property `id` of type `kind` out.
    #[must_use]
    pub fn has_property(&self, kind: PropertyKind, id: u32) -> bool {
        self.properties.contains(&(kind, id))
    }

    /// Whether Advanced leaves table `table` out.
    #[must_use]
    pub fn has_table(&self, table: &str) -> bool {
        self.tables.contains(&table)
    }
}

/// The emote sets' table: what Advanced lists of the emotes and their quests goes by.
pub const EMOTE_TABLE: &str = "weenie_properties_emote";

/// What the pages of the weenie of `doc` show ([`OnPages`]).
#[must_use]
pub fn on_pages(doc: &SqlDoc) -> OnPages {
    let kind = Kind::of(doc);
    let mut out = OnPages::default();
    let mut lists: Vec<Drill> = Vec::new();
    let mut destination = false;
    for (_, fields) in pages(kind, doc) {
        for f in fields {
            for p in properties(f) {
                if !out.properties.contains(&p) {
                    out.properties.push(p);
                }
            }
            match f.target {
                Target::List(d) => lists.push(d),
                Target::Destination => destination = true,
                _ => {}
            }
        }
    }
    // The create list, when every row of it is in a list shown.
    let creature = kind.is_creature();
    let wares = [Drill::Wields, Drill::Holds, Drill::Sells];
    let shows = |r: &collections::TableRow| {
        lists
            .iter()
            .any(|d| wares.contains(d) && collections::member(*d, creature)(r))
    };
    if lists.iter().any(|d| wares.contains(d))
        && collections::rows_of(doc, collections::CREATE_LIST)
            .iter()
            .all(shows)
    {
        out.tables.push(collections::CREATE_LIST);
    }
    for (d, table) in [
        (Drill::Spells, collections::SPELL_BOOK),
        (Drill::Spawns, collections::GENERATOR),
        (Drill::Emotes, EMOTE_TABLE),
    ] {
        if lists.contains(&d) {
            out.tables.push(table);
        }
    }
    // Its positions, when every one is where it sends who uses it.
    if destination
        && collections::rows_of(doc, collections::POSITIONS)
            .iter()
            .all(collections::is_destination)
    {
        out.tables.push(collections::POSITIONS);
    }
    out
}

/// The key of field `f`'s name on its page: what a creature wields reads as what it has
/// equipped, every other field by its own name.
#[must_use]
pub fn list_heading(f: &Field) -> String {
    match f.target {
        Target::List(Drill::Wields) => "sections.carries".to_owned(),
        _ => label_key(f),
    }
}

/// The key of field `f`'s name in the text: the wielded loot table's among the Loot tab's words,
/// every other field's among the fields'.
#[must_use]
pub fn label_key(f: &Field) -> String {
    match f.widget {
        Widget::Treasure(lyceum_foundry::loot::LootKind::Wielded) => {
            "loot.wielded_field".to_owned()
        }
        _ => format!("fields.{}", f.key),
    }
}

/// `raw` as a value of a property of type `kind` compares: a number by its value, a yes or no by
/// which, words without their quotes.
fn same_value(kind: PropertyKind) -> impl Fn(&str) -> String {
    move |raw: &str| {
        let v = content::bare(raw);
        let v = v.trim();
        match kind {
            PropertyKind::Int | PropertyKind::Int64 => v
                .parse::<i64>()
                .map_or_else(|_| v.to_owned(), |n| n.to_string()),
            PropertyKind::Float => v
                .parse::<f64>()
                .map_or_else(|_| v.to_owned(), |n| n.to_string()),
            PropertyKind::Bool => yes_no(Some(v), false).to_string(),
            PropertyKind::String => content::unquote(v),
            PropertyKind::DataId | PropertyKind::InstanceId => {
                lyceum_foundry::sql_ids::number(v).map_or_else(|| v.to_owned(), |n| n.to_string())
            }
        }
    }
}

/// The columns of an emote set that say what it is.
const EMOTE_COLUMNS: &[&str] = &[
    "category",
    "probability",
    "weenie_Class_Id",
    "style",
    "substyle",
    "quest",
    "vendor_Type",
    "min_Health",
    "max_Health",
];

/// What field `f` of `doc` is, as a value two weenies' fields are compared by.
#[must_use]
pub fn compared(f: &Field, doc: &SqlDoc) -> Vec<String> {
    match f.target {
        Target::Bits(k, id, mask) => {
            vec![(bits_read(k, id, doc.property(k, id).as_deref()) & mask).to_string()]
        }
        Target::Property(_, _) | Target::Parts(_) => properties(f)
            .into_iter()
            .map(|(k, id)| {
                doc.property(k, id)
                    .map_or_else(|| "-".to_owned(), |v| same_value(k)(&v))
            })
            .collect(),
        Target::Destination => destination_row(doc).map_or_else(Vec::new, |r| {
            [
                "obj_Cell_Id",
                "origin_X",
                "origin_Y",
                "origin_Z",
                "angles_W",
                "angles_X",
                "angles_Y",
                "angles_Z",
            ]
            .iter()
            .map(|c| doc.cell(r, c).unwrap_or_default())
            .collect()
        }),
        Target::List(Drill::Emotes) => {
            let mut out = Vec::new();
            for set in doc.emote_sets() {
                out.push(
                    EMOTE_COLUMNS
                        .iter()
                        .map(|c| doc.cell(set.emote, c).unwrap_or_else(|| "NULL".into()))
                        .collect::<Vec<_>>()
                        .join("|"),
                );
                for a in doc.actions(&set) {
                    out.push(
                        content::ACTION_COLUMNS
                            .iter()
                            .skip(1)
                            .map(|c| doc.cell(a, c).unwrap_or_else(|| "NULL".into()))
                            .collect::<Vec<_>>()
                            .join("|"),
                    );
                }
            }
            out
        }
        Target::List(d) => collections::list_of(doc, d)
            .iter()
            .map(collections::row_words)
            .collect(),
        Target::Attributes => collections::rows_of(doc, collections::ATTRIBUTES)
            .iter()
            .map(collections::row_words)
            .collect(),
        Target::Vitals => collections::rows_of(doc, collections::VITALS)
            .iter()
            .map(collections::row_words)
            .collect(),
    }
}

/// Whether field `f` of `doc` differs from the same field of `server`.
#[must_use]
pub fn changed(f: &Field, doc: &SqlDoc, server: &SqlDoc) -> bool {
    compared(f, doc) != compared(f, server)
}

/// The edits that put field `f` of `doc` back as `server` has it, as one step; none for a field
/// that is not put back that way (words the server's has none of).
#[must_use]
pub fn put_back(f: &Field, doc: &SqlDoc, server: &SqlDoc) -> Vec<FormEdit> {
    let whole = |table: &str| FormEdit::Rows {
        table: table.to_owned(),
        rows: collections::rows_of(server, table),
    };
    match f.target {
        // Its bits as the server's has them, the others as they are: the server's value as
        // written once every bit is the same.
        Target::Bits(kind, id, mask) => {
            let mine = bits_read(kind, id, doc.property(kind, id).as_deref());
            let theirs = bits_read(kind, id, server.property(kind, id).as_deref());
            if mine & mask == theirs & mask {
                return Vec::new();
            }
            let written = bits_value(kind, id, (mine & !mask) | (theirs & mask));
            let value = if bits_read(kind, id, Some(&written)) == theirs {
                server.property(kind, id)
            } else {
                Some(written)
            };
            vec![FormEdit::Property { kind, id, value }]
        }
        // Each part that differs: one both leave unset has nothing to put back.
        Target::Property(_, _) | Target::Parts(_) => properties(f)
            .into_iter()
            .filter(|(kind, id)| {
                let same = same_value(*kind);
                doc.property(*kind, *id).map(|v| same(&v))
                    != server.property(*kind, *id).map(|v| same(&v))
            })
            .map(|(kind, id)| FormEdit::Property {
                kind,
                id,
                value: server.property(kind, id),
            })
            .collect(),
        Target::Destination => {
            let theirs: Vec<collections::TableRow> =
                collections::rows_of(server, collections::POSITIONS)
                    .into_iter()
                    .filter(collections::is_destination)
                    .collect();
            vec![FormEdit::Rows {
                table: collections::POSITIONS.to_owned(),
                rows: collections::splice(
                    &collections::rows_of(doc, collections::POSITIONS),
                    collections::is_destination,
                    &theirs,
                ),
            }]
        }
        Target::List(Drill::Emotes) => {
            vec![FormEdit::Emotes(super::emotes::emotes_of(server))]
        }
        Target::List(Drill::Appears | Drill::UsedBy) => Vec::new(),
        Target::List(d) => {
            let Some(table) = collections::table_of(d) else {
                return Vec::new();
            };
            let creature = Kind::of(doc).is_creature();
            vec![FormEdit::Rows {
                table: table.to_owned(),
                rows: collections::splice(
                    &collections::rows_of(doc, table),
                    collections::member(d, creature),
                    &collections::list_of(server, d),
                ),
            }]
        }
        Target::Attributes => vec![whole(collections::ATTRIBUTES)],
        Target::Vitals => vec![whole(collections::VITALS)],
    }
}

/// The server's name of property type `kind`, as its tooltips give it.
const fn type_word(kind: PropertyKind) -> &'static str {
    match kind {
        PropertyKind::Int => "Int",
        PropertyKind::Int64 => "Int64",
        PropertyKind::Bool => "Bool",
        PropertyKind::Float => "Float",
        PropertyKind::String => "String",
        PropertyKind::DataId => "DataId",
        PropertyKind::InstanceId => "InstanceId",
    }
}

/// The server's names for what field `f` writes, a line each ("Int 25 Level").
#[must_use]
pub fn server_names(f: &Field, schema: Option<&Schema>) -> Vec<String> {
    let named = |line: String, name: Option<&str>| match name {
        Some(n) => format!("{line} {n}"),
        None => line,
    };
    match f.target {
        Target::Destination => vec![named(
            "Position 2".to_owned(),
            schema
                .and_then(|s| s.properties.get("position"))
                .and_then(|m| m.get(&2))
                .map(String::as_str),
        )],
        _ => properties(f)
            .into_iter()
            .map(|(k, id)| {
                named(
                    format!("{} {id}", type_word(k)),
                    schema.and_then(|s| s.property(k, id)),
                )
            })
            .collect(),
    }
}

/// Field `f`'s tooltip: what it is, when the text says, then the server's names for what it
/// writes; empty when it has neither.
#[must_use]
pub fn tooltip(f: &Field, t: &crate::text::Table, schema: Option<&Schema>) -> String {
    if USABLE_BY.contains(&f.key) {
        return String::new();
    }
    let about = format!("{}_about", label_key(f));
    let about = t.has(&about).then(|| t.get(&about).to_owned());
    let names = server_names(f, schema).join("\n");
    match about {
        Some(a) if !names.is_empty() => format!("{a}\n\n{names}"),
        Some(a) => a,
        None => names,
    }
}

/// `r` with `tip` shown on hover, when there is one.
pub(super) fn with_tip(r: egui::Response, tip: &str) -> egui::Response {
    if tip.is_empty() {
        r
    } else {
        r.on_hover_text(tip)
    }
}

/// A server's name for an enum value in plain words: `HeavyWeapons` as "Heavy weapons".
#[must_use]
pub fn spaced(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut words: Vec<String> = Vec::new();
    let mut word = String::new();
    for (i, c) in chars.iter().enumerate() {
        let next_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
        let prev = i.checked_sub(1).map(|p| chars[p]);
        let starts = (c.is_uppercase()
            && prev.is_some_and(|p| {
                p.is_lowercase() || p.is_ascii_digit() || (p.is_uppercase() && next_lower)
            }))
            || (c.is_ascii_digit() && prev.is_some_and(char::is_alphabetic));
        if (starts || *c == '_') && !word.is_empty() {
            words.push(std::mem::take(&mut word));
        }
        if *c != '_' {
            word.push(*c);
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
        .iter()
        .enumerate()
        .map(|(i, w)| {
            // A word all capitals (an initialism) stays as it is.
            if i == 0 || w.chars().all(|c| c.is_uppercase() || c.is_ascii_digit()) {
                w.clone()
            } else {
                w.to_lowercase()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The values of the server's enum `name`, each with its plain words.
#[must_use]
pub fn enum_values(schema: Option<&Schema>, name: &str) -> Vec<(u32, String)> {
    schema
        .and_then(|s| s.enums.get(name))
        .map(|m| m.iter().map(|(k, v)| (*k, spaced(v))).collect())
        .unwrap_or_default()
}

/// The enum the "what of" of wielding requirement `what` is a value of, when it is one.
#[must_use]
pub const fn wield_of(what: i64) -> Option<&'static str> {
    match what {
        1 | 2 | 8 => Some("Skill"),
        3 | 4 => Some("Attribute"),
        5 | 6 => Some("Vital"),
        _ => None,
    }
}

/// The type of the player's property wielding requirement `what` names by its "of", when it
/// names one: a whole number the player needs at least so much of, or a yes or no the player's
/// must match.
#[must_use]
pub const fn wield_property(what: i64) -> Option<PropertyKind> {
    match what {
        9 => Some(PropertyKind::Int),
        10 => Some(PropertyKind::Bool),
        _ => None,
    }
}

/// The widget the "of" of wielding requirement `what` is chosen by: a property's name, a value
/// of the enum it names, else a number.
#[must_use]
pub const fn wield_of_widget(what: i64) -> Widget {
    match (wield_property(what), wield_of(what)) {
        (Some(k), _) => Widget::PropertyName(k),
        (None, Some(e)) => Widget::Named(e),
        (None, None) => Widget::Whole,
    }
}

/// The server's properties of type `kind`, each id with its name, by name.
#[must_use]
pub fn property_names(schema: Option<&Schema>, kind: PropertyKind) -> Vec<(u32, String)> {
    let mut all: Vec<(u32, String)> = schema
        .and_then(|s| s.properties.get(kind.key()))
        .map(|m| m.iter().map(|(k, v)| (*k, v.clone())).collect())
        .unwrap_or_default();
    all.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
    all
}

/// Whether wielding requirement `what` names what the player must be (a creature kind, a
/// heritage) rather than how much they need: the server reads it from the requirement's "at
/// least", and its "of" goes unread.
#[must_use]
pub const fn wield_is(what: i64) -> bool {
    matches!(what, 11 | 12)
}

/// The least a skill's training must be, for a requirement that it be trained: untrained,
/// trained or specialized.
const TRAINING: &[(i64, &str)] = &[
    (1, "wield_untrained"),
    (2, "wield_trained"),
    (3, "wield_specialized"),
];

/// The heritages a wielder can be, by the names players read.
const HERITAGES: &[(i64, &str)] = &[
    (1, "heritage_aluvian"),
    (2, "heritage_gharundim"),
    (3, "heritage_sho"),
    (4, "heritage_viamontian"),
    (5, "heritage_umbraen"),
    (6, "heritage_gear_knight"),
    (7, "heritage_tumerok"),
    (8, "heritage_lugian"),
    (9, "heritage_empyrean"),
    (10, "heritage_penumbraen"),
    (11, "heritage_undead"),
    (12, "heritage_olthoi"),
    (13, "heritage_olthoi_spitter"),
];

/// The widget the "at least" (or "must be") of wielding requirement `what` is chosen by: a
/// training, a creature kind, a heritage or a yes or no by name, else a number.
#[must_use]
pub const fn wield_value_widget(what: i64) -> Widget {
    match what {
        8 => Widget::Choice(TRAINING),
        11 => Widget::Named("CreatureType"),
        12 => Widget::Choice(HERITAGES),
        10 => Widget::YesNo(false),
        _ => Widget::Whole,
    }
}

/// An int property's value as written, as a number.
fn int_of(doc: &SqlDoc, kind: PropertyKind, id: u32) -> Option<i64> {
    let v = content::bare(&doc.property(kind, id)?);
    let v = v.trim();
    v.parse::<i64>()
        .ok()
        .or_else(|| lyceum_foundry::sql_ids::number(v).map(i64::from))
}

/// The words of value `n` of a choice of `choices` or unset (by key `unset`), as the server runs
/// it: a value not among them runs as unset, and reads so with its number, but for 0.
fn choice_or_unset_words(
    unset: &str,
    choices: &[(i64, &str)],
    n: i64,
    t: &crate::text::Table,
) -> String {
    let unset = t.get(&format!("fields.{unset}")).to_owned();
    match choices.iter().find(|(v, _)| *v == n) {
        Some((_, k)) => t.get(&format!("fields.{k}")).to_owned(),
        None if n == 0 => unset,
        None => t.fill("fields.as_unset", &[("what", &unset), ("n", &n)]),
    }
}

/// The date and time `secs` seconds after 1970 began, in UTC, as `2001-12-31 18:00`: with its
/// seconds when it has any.
#[must_use]
pub fn date_words(secs: i64) -> String {
    let b = dereth_client_contract::ctime::broken_down(secs, 0);
    let mut s = format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        b.year, b.month, b.day, b.hour, b.minute
    );
    if b.second != 0 {
        s.push_str(&format!(":{:02}", b.second));
    }
    s
}

/// The days from 1970-01-01 to day `d` of month `m` of year `y`.
const fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// What a date and time typed in UTC as `2001-12-31 18:00` is written as: the seconds since 1970
/// began, or none for nothing typed (or 1970's first moment). Its time, or its seconds, may be
/// left off, and a `T` may part it from its date. `None` for what is no date, or a date a whole
/// number property cannot hold (before 1970, or after 2038-01-19 03:14:07).
#[must_use]
pub fn date_written(typed: &str) -> Option<Option<i64>> {
    let s = typed.trim();
    let s = s.strip_suffix("UTC").unwrap_or(s).trim_end();
    if s.is_empty() {
        return Some(None);
    }
    let (date, time) = s
        .split_once([' ', 'T'])
        .map_or((s, ""), |(d, t)| (d, t.trim()));
    let numbers = |s: &str, by: char| -> Option<Vec<i64>> {
        s.split(by)
            .map(|p| {
                (!p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
                    .then(|| p.parse::<i64>().ok())
                    .flatten()
            })
            .collect()
    };
    let [y, m, d] = numbers(date, '-')?[..] else {
        return None;
    };
    let (h, min, sec) = match numbers(time, ':') {
        _ if time.is_empty() => (0, 0, 0),
        Some(hms) => match hms[..] {
            [h, min] => (h, min, 0),
            [h, min, sec] => (h, min, sec),
            _ => return None,
        },
        None => return None,
    };
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || h > 23 || min > 59 || sec > 59 {
        return None;
    }
    let secs = days_from_civil(y, m, d) * 86_400 + h * 3600 + min * 60 + sec;
    // A day its month does not have (the 30th of February) comes back another.
    let b = dereth_client_contract::ctime::broken_down(secs, 0);
    if (b.year, b.month, b.day) != (y, m, d) {
        return None;
    }
    match secs {
        0 => Some(None),
        n if n > 0 && n <= i64::from(i32::MAX) => Some(Some(n)),
        _ => None,
    }
}

/// The edit that writes date-and-time property `kind` `id` as typed (`typed`): removed for
/// nothing typed, refused saying `refused` for what is no date.
#[must_use]
pub fn date_edit(kind: PropertyKind, id: u32, typed: &str, refused: &str) -> FormEdit {
    match date_written(typed) {
        Some(secs) => FormEdit::Property {
            kind,
            id,
            value: secs.map(|n| n.to_string()),
        },
        None => FormEdit::Refused(refused.to_owned()),
    }
}

/// The words of one property of `doc` chosen by `widget`, from `t` and `schema`; empty for one
/// the weenie leaves unset (a yes or no shows the server's value then).
#[must_use]
pub fn property_words(
    kind: PropertyKind,
    id: u32,
    widget: Widget,
    doc: &SqlDoc,
    t: &crate::text::Table,
    schema: Option<&Schema>,
) -> String {
    let raw = doc.property(kind, id);
    match widget {
        Widget::YesNo(unset) | Widget::Opposite(unset) => {
            let yes = yes_no(raw.as_deref(), unset) != matches!(widget, Widget::Opposite(_));
            t.get(if yes { "fields.yes" } else { "fields.no" })
                .to_owned()
        }
        Widget::Choice(choices) => int_of(doc, kind, id).map_or_else(String::new, |n| {
            choices
                .iter()
                .find(|(v, _)| int_bits(*v) == int_bits(n))
                .map_or_else(
                    || n.to_string(),
                    |(_, k)| t.get(&format!("fields.{k}")).to_owned(),
                )
        }),
        Widget::ChoiceOrUnset(unset, choices) => int_of(doc, kind, id)
            .map_or_else(String::new, |n| choice_or_unset_words(unset, choices, n, t)),
        // Before 1970 began is none, as is 0, to the server.
        Widget::DateTime => int_of(doc, kind, id)
            .filter(|n| *n > 0)
            .map_or_else(String::new, |n| {
                t.fill("fields.utc_time", &[("time", &date_words(n))])
            }),
        Widget::Menu(_) | Widget::NamedMenu(_) => {
            let now = bits_read(kind, id, raw.as_deref());
            if raw.is_none() && now == 0 {
                return String::new();
            }
            let on = match widget {
                Widget::NamedMenu(e) => {
                    let named = named_bits(schema, e);
                    // Without the server's names, the number.
                    if named.is_empty() {
                        return now.cast_signed().to_string();
                    }
                    named
                        .into_iter()
                        .filter(|(b, _)| now & b == *b)
                        .map(|(_, n)| n)
                        .collect()
                }
                Widget::Menu(bits) => ticked(bits, now, t),
                _ => Vec::new(),
            };
            if on.is_empty() {
                t.get("fields.none").to_owned()
            } else {
                on.join(", ")
            }
        }
        Widget::Named(e) => int_of(doc, kind, id).map_or_else(String::new, |n| {
            u32::try_from(n)
                .ok()
                .and_then(|n| schema.and_then(|s| s.value(e, n)))
                .map_or_else(|| n.to_string(), spaced)
        }),
        Widget::PropertyName(of) => int_of(doc, kind, id).map_or_else(String::new, |n| {
            u32::try_from(n)
                .ok()
                .and_then(|n| schema.and_then(|s| s.property(of, n)))
                .map_or_else(|| n.to_string(), str::to_owned)
        }),
        Widget::ColorScheme => {
            property_words(kind, id, Widget::Named("PaletteTemplate"), doc, t, schema)
        }
        Widget::DataId | Widget::Model | Widget::Icon | Widget::Table(_) | Widget::ClothingLook => {
            raw.as_deref()
                .and_then(|v| lyceum_foundry::sql_ids::number(&content::bare(v)))
                .map(|n| format!("0x{n:08X}"))
                .unwrap_or_default()
        }
        Widget::Spell => raw
            .as_deref()
            .and_then(|v| lyceum_foundry::sql_ids::number(&content::bare(v)))
            .map(|n| n.to_string())
            .unwrap_or_default(),
        // A table by its id, or a pack's by its symbol's token as written.
        Widget::Treasure(_) => raw
            .as_deref()
            .map(|v| {
                let v = content::bare(v);
                lyceum_foundry::sql_ids::number(&v).map_or(v, |n| n.to_string())
            })
            .unwrap_or_default(),
        Widget::Meters | Widget::Seconds => raw.map_or_else(String::new, |v| {
            let key = if widget == Widget::Meters {
                "fields.meters"
            } else {
                "fields.seconds"
            };
            t.fill(key, &[("n", &content::bare(&v))])
        }),
        _ => raw
            .map(|v| match kind {
                PropertyKind::String => content::unquote(&v),
                _ => content::bare(&v),
            })
            .unwrap_or_default(),
    }
}

/// The names of the bits of `bits` that `now` has, in order.
fn ticked(bits: &[(i64, &str)], now: u32, t: &crate::text::Table) -> Vec<String> {
    bits.iter()
        .filter(|(b, _)| now & int_bits(*b) == int_bits(*b))
        .map(|(_, k)| t.get(&format!("fields.{k}")).to_owned())
        .collect()
}

/// What a closed dropdown of check boxes says of those ticked, `on`: none, the names of one or
/// two, else how many.
#[must_use]
pub fn chosen_words(on: &[String], t: &crate::text::Table) -> String {
    match on.len() {
        0 => t.get("fields.none").to_owned(),
        1 | 2 => on.join(", "),
        n => t.fill("fields.chosen_n", &[("n", &n)]),
    }
}

/// The bits of the server's enum `name`, each one bit, with their plain words; none without the
/// server's names.
#[must_use]
pub fn named_bits(schema: Option<&Schema>, name: &str) -> Vec<(u32, String)> {
    enum_values(schema, name)
        .into_iter()
        .filter(|(v, _)| v.is_power_of_two())
        .collect()
}

/// The words of field bits `mask` of property `kind` `id` of `doc` chosen by `widget`: what they
/// allow, or the choice they make (each that matches, when more than one does); empty while the
/// weenie sets none.
#[must_use]
pub fn bits_words(
    kind: PropertyKind,
    id: u32,
    mask: u32,
    widget: Widget,
    doc: &SqlDoc,
    t: &crate::text::Table,
) -> String {
    let raw = doc.property(kind, id);
    if raw.is_none() {
        return String::new();
    }
    let now = bits_read(kind, id, raw.as_deref()) & mask;
    let words: Vec<String> = match widget {
        Widget::Allow(bits) => bits
            .iter()
            .filter(|(b, _)| now & int_bits(*b) == 0)
            .map(|(_, k)| t.get(&format!("fields.{k}")).to_owned())
            .collect(),
        Widget::OneOf(choices) => one_of(choices, now)
            .into_iter()
            .map(|k| t.get(&format!("fields.{k}")).to_owned())
            .collect(),
        _ => Vec::new(),
    };
    if words.is_empty() {
        t.get("fields.none").to_owned()
    } else {
        words.join(", ")
    }
}

/// The keys of the choices of `choices` bits `now` makes: the one whose value they are, else
/// each whose bits they have (more than one where they set two choices' bits at once).
fn one_of(choices: &[(i64, &'static str)], now: u32) -> Vec<&'static str> {
    if let Some((_, k)) = choices.iter().find(|(v, _)| int_bits(*v) == now) {
        return vec![k];
    }
    choices
        .iter()
        .filter(|(v, _)| *v != 0 && now & int_bits(*v) == int_bits(*v))
        .map(|(_, k)| *k)
        .collect()
}

/// What wielding the weenie of `doc` needs, in words; empty when it needs nothing.
#[must_use]
pub fn wield_words(doc: &SqlDoc, t: &crate::text::Table, schema: Option<&Schema>) -> String {
    let what = int_of(doc, Int, 158).unwrap_or(0);
    if what == 0 {
        return String::new();
    }
    let what_words = property_words(Int, 158, WIELD[0].widget, doc, t, schema);
    if wield_is(what) {
        let is = property_words(Int, 160, wield_value_widget(what), doc, t, schema);
        return t.fill(
            "fields.wield_words_is",
            &[("what", &what_words), ("n", &is)],
        );
    }
    // A yes or no the player's property must be.
    if wield_property(what) == Some(PropertyKind::Bool) {
        let which = property_words(Int, 159, wield_of_widget(what), doc, t, schema);
        let is = property_words(Int, 160, wield_value_widget(what), doc, t, schema);
        return t.fill(
            "fields.wield_words_is",
            &[("what", &format!("{what_words} {which}")), ("n", &is)],
        );
    }
    let at_least = match wield_value_widget(what) {
        Widget::Whole => int_of(doc, Int, 160).unwrap_or(0).to_string(),
        widget => property_words(Int, 160, widget, doc, t, schema),
    };
    let which = matches!(
        wield_of_widget(what),
        Widget::Named(_) | Widget::PropertyName(_)
    )
    .then(|| property_words(Int, 159, wield_of_widget(what), doc, t, schema));
    match which.filter(|w| !w.is_empty()) {
        Some(which) => t.fill(
            "fields.wield_words",
            &[("what", &what_words), ("which", &which), ("n", &at_least)],
        ),
        None => t.fill(
            "fields.wield_words_plain",
            &[("what", &what_words), ("n", &at_least)],
        ),
    }
}

/// The words of field `f` of `doc`, from `t` and `schema`; empty for a field the weenie leaves
/// unset (a yes or no shows the server's value then). A death treasure, a spell, a destination
/// and a list are worded by the pane, which knows them.
#[must_use]
pub fn plain_words(
    f: &Field,
    doc: &SqlDoc,
    t: &crate::text::Table,
    schema: Option<&Schema>,
) -> String {
    match (f.target, f.widget) {
        (Target::Property(kind, id), widget) => property_words(kind, id, widget, doc, t, schema),
        (Target::Bits(kind, id, mask), widget) => bits_words(kind, id, mask, widget, doc, t),
        (Target::Parts(_), Widget::Wield) => wield_words(doc, t, schema),
        (Target::Parts(parts), Widget::Together(unset)) => t
            .get(if together(parts, doc, unset) {
                "fields.yes"
            } else {
                "fields.no"
            })
            .to_owned(),
        (Target::Parts(parts), _) => {
            let words: Vec<String> = parts
                .iter()
                .map(|p| property_words(p.kind, p.id, p.widget, doc, t, schema))
                .collect();
            // A field that says its parts its own way, when every part is set.
            let own = format!("fields.{}_words", f.key);
            if t.has(&own) && words.iter().all(|w| !w.is_empty()) {
                let mut s = t.get(&own).to_owned();
                for (i, w) in words.iter().enumerate() {
                    s = s.replace(&format!("{{{i}}}"), w);
                }
                return s;
            }
            parts
                .iter()
                .zip(words)
                .filter(|(_, w)| !w.is_empty())
                .map(|(p, w)| match p.key {
                    Some(k) => format!("{} {w}", t.get(&format!("fields.{k}"))),
                    None => w,
                })
                .collect::<Vec<_>>()
                .join(", ")
        }
        (Target::Attributes, _) => collections::attribute_words(doc, t),
        (Target::Vitals, _) => collections::vital_words(doc, t),
        (Target::Destination | Target::List(_), _) => String::new(),
    }
}

/// Whether yes-or-no parts `parts` of `doc` together say yes: any of them does (`unset` the
/// value of one the weenie sets none of).
#[must_use]
pub fn together(parts: &[Part], doc: &SqlDoc, unset: bool) -> bool {
    parts
        .iter()
        .any(|p| yes_no(doc.property(p.kind, p.id).as_deref(), unset))
}

/// The edits that write yes (`v`) to every one of `parts`, as one step.
#[must_use]
pub fn together_written(parts: &[Part], v: bool) -> Vec<FormEdit> {
    parts
        .iter()
        .map(|p| FormEdit::Property {
            kind: p.kind,
            id: p.id,
            value: Some(yes_no_written(p.kind, p.widget, v).to_owned()),
        })
        .collect()
}

/// A page of the editor: one of a weenie's sections, or every property and row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Page {
    Section(Section),
    Advanced,
}

impl Default for Page {
    fn default() -> Self {
        Self::Section(Section::Basic)
    }
}

/// The list field an entry's key (`list:wields:0:stack`, `spawn:2:share`, `action:0:1`) is a
/// row of, by its key; a field's own key is its own.
#[must_use]
pub fn list_field_of(key: &str) -> &str {
    let prefix = key.split(':').next().unwrap_or("");
    match prefix {
        "list" => match key.split(':').nth(1) {
            Some("wields") => "flow:wields",
            Some("holds") => "flow:contents",
            Some("sells") => "flow:sells",
            _ => key,
        },
        "spawn" => "flow:spawns",
        "spell" => "flow:spells",
        "skill" => "flow:skills",
        "part" => "flow:body_parts",
        "attribute" => "flow:attributes",
        "vital" => "flow:vitals",
        "emote" | "action" => "flow:emotes",
        _ => key,
    }
}

/// The page field `key` of a weenie whose sections are `sections` is on: its section's (a list
/// entry's, its list's), else Advanced, where every property is.
#[must_use]
pub fn page_of(key: &str, sections: &[(Section, Vec<&'static Field>)]) -> Page {
    let key = list_field_of(key);
    sections
        .iter()
        .find(|(_, fields)| {
            fields.iter().any(|f| {
                field_key(f) == key
                    || properties(f)
                        .iter()
                        .any(|(k, id)| property_key(*k, *id) == key)
            })
        })
        .map_or(Page::Advanced, |(s, _)| Page::Section(*s))
}

/// The width of the editor's list of pages, in points.
const PAGES_WIDTH: f32 = 130.0;

/// The width of a field of words players read as a paragraph, in points.
pub(super) const LINES_WIDTH: f32 = 320.0;

/// How far the fields stand off the divider at their left, in points.
pub(super) const FIELDS_MARGIN: i8 = 8;

impl EditorApp {
    /// The server's class the weenie `w` is the pack's change of, as the server has it: `None`
    /// for any other weenie, and while it is being read.
    fn server_doc(&mut self, ui: &egui::Ui, w: &Weenie) -> Option<std::rc::Rc<SqlDoc>> {
        let Weenie::Class(c) = w else { return None };
        self.session.weenie_file(w)?;
        match self.server_doc_of(*c) {
            None => {
                ui.ctx().request_repaint();
                None
            }
            Some(found) => found,
        }
    }

    /// The editor's form over `doc`: the weenie's pages listed on the left, a mark on each with a
    /// field changed from the server's, and the page chosen on the right.
    pub(super) fn editor_form(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        doc: &SqlDoc,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let kind = Kind::of(doc);
        let sections = pages(kind, doc);
        let server = self.server_doc(ui, w);
        // The field asked for, on its page.
        if let Some((fw, key)) = self.editor.focus.clone() {
            if fw == *w {
                self.editor.page = page_of(&key, &sections);
            }
        }
        // A page the weenie does not have: its first.
        if let Page::Section(s) = self.editor.page {
            if !sections.iter().any(|(x, _)| *x == s) {
                self.editor.page = sections
                    .first()
                    .map_or(Page::Advanced, |(s, _)| Page::Section(*s));
            }
        }
        self.editor.page_rects.clear();
        let marker = ui.visuals().warn_fg_color;
        egui::Panel::left(egui::Id::new("editor pages"))
            .resizable(false)
            .exact_size(PAGES_WIDTH)
            .show(ui, |ui| {
                for (s, fields) in &sections {
                    let changed = server
                        .as_ref()
                        .is_some_and(|sv| fields.iter().any(|f| changed(f, doc, sv)));
                    let name = t.get(&s.name_key()).to_owned();
                    let words: egui::WidgetText = if changed {
                        let mut job = egui::text::LayoutJob::default();
                        for (piece, color) in [(name.as_str(), None), (" ●", Some(marker))] {
                            let mut text = egui::RichText::new(piece);
                            if let Some(c) = color {
                                text = text.color(c);
                            }
                            text.append_to(
                                &mut job,
                                ui.style(),
                                egui::FontSelection::Default,
                                egui::Align::Center,
                            );
                        }
                        job.into()
                    } else {
                        name.into()
                    };
                    let page = Page::Section(*s);
                    let r = ui.selectable_label(self.editor.page == page, words);
                    self.editor.page_rects.push((page, r.rect));
                    if r.clicked() {
                        self.editor.page = page;
                    }
                }
                ui.separator();
                let r = ui
                    .selectable_label(self.editor.page == Page::Advanced, t.get("weenie.advanced"));
                self.editor.page_rects.push((Page::Advanced, r.rect));
                if r.clicked() {
                    self.editor.page = Page::Advanced;
                }
                // Advanced shows what the other pages do too, when asked.
                if self.editor.page == Page::Advanced {
                    let r = ui.checkbox(&mut self.editor.advanced_all, t.get("weenie.show_all"));
                    self.editor.show_all_switch = Some(r.rect);
                }
            });
        egui::CentralPanel::no_frame().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt(("weenie form", self.editor.page))
                .auto_shrink(false)
                .show(ui, |ui| {
                    // Off the pages' divider.
                    egui::Frame::NONE
                        .inner_margin(egui::Margin {
                            left: FIELDS_MARGIN,
                            ..egui::Margin::ZERO
                        })
                        .show(ui, |ui| match self.editor.page {
                            Page::Advanced => self.weenie_form(ui, w, doc, out),
                            Page::Section(s) => {
                                let fields = sections
                                    .iter()
                                    .find(|(x, _)| *x == s)
                                    .map(|(_, f)| f.clone())
                                    .unwrap_or_default();
                                self.section_page(ui, w, &fields, doc, server.as_deref(), out);
                                if s == Section::Basic {
                                    self.appears_section(ui, w, out);
                                }
                            }
                        });
                });
        });
    }

    /// A section's fields: each its name, its value chosen, and for the pack's change of a
    /// server's weenie, a mark when it differs from the server's. A list is drawn under them,
    /// the width of the page.
    fn section_page(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        fields: &[&'static Field],
        doc: &SqlDoc,
        server: Option<&SqlDoc>,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let schema = self.schema.clone();
        let wide = |f: &Field| {
            matches!(
                f.target,
                Target::List(_) | Target::Attributes | Target::Vitals
            )
        };
        egui::Grid::new(("section fields", w.clone()))
            .num_columns(3)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                let mut usable_by = false;
                for f in fields.iter().filter(|f| !wide(f)) {
                    // A portal's "Usable by" under a heading of its own.
                    if USABLE_BY.contains(&f.key) && !usable_by {
                        usable_by = true;
                        ui.strong(t.get("fields.who_can_use"));
                        ui.end_row();
                    }
                    let label = with_tip(
                        ui.label(t.get(&label_key(f))),
                        &tooltip(f, &t, schema.as_ref()),
                    );
                    match server {
                        Some(sv) if changed(f, doc, sv) => {
                            self.changed_mark(ui, w, f, doc, sv, out);
                        }
                        _ => {
                            ui.label("");
                        }
                    }
                    self.field_widget(ui, w, f, doc, out);
                    // The field asked for when the editor opened (or a part of it): shown.
                    let asked = self.editor.focus_on(w, &field_key(f))
                        || properties(f)
                            .iter()
                            .any(|(k, id)| self.editor.focus_on(w, &property_key(*k, *id)));
                    if asked {
                        label.scroll_to_me(Some(egui::Align::Center));
                    }
                    ui.end_row();
                }
            });
        // A list of rows the width of the page, under its name and its mark when changed.
        for f in fields.iter().filter(|f| wide(f)) {
            ui.add_space(8.0);
            let marked = server.filter(|sv| changed(f, doc, sv));
            ui.horizontal(|ui| {
                with_tip(
                    ui.strong(t.get(&list_heading(f))),
                    &tooltip(f, &t, schema.as_ref()),
                );
                if let Some(sv) = marked {
                    self.changed_mark(ui, w, f, doc, sv, out);
                }
            });
            self.field_widget(ui, w, f, doc, out);
        }
    }

    /// The mark of field `f` changed from the server's `server`: the server's value on hover, and
    /// a right click to put it back.
    fn changed_mark(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        f: &Field,
        doc: &SqlDoc,
        server: &SqlDoc,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let was = self.field_words(f, server);
        let hover = if matches!(
            f.target,
            Target::List(_) | Target::Attributes | Target::Vitals | Target::Destination
        ) {
            t.get("editor.changed").to_owned()
        } else if was.is_empty() {
            t.get("editor.server_unset").to_owned()
        } else {
            t.fill("editor.server_value", &[("value", &was)])
        };
        let edits = put_back(f, doc, server);
        let r = ui
            .add(
                egui::Label::new(egui::RichText::new("●").color(ui.visuals().warn_fg_color))
                    .sense(egui::Sense::click()),
            )
            .on_hover_text(hover);
        self.editor.changed_marks.push((field_key(f), r.rect));
        if !edits.is_empty() {
            r.context_menu(|ui| {
                let b = ui.button(t.get("editor.put_back_field"));
                self.editor.put_back_button = Some(b.rect);
                if b.clicked() {
                    out.push(UiAction::Forms(w.clone(), edits.clone()));
                    ui.close();
                }
            });
        }
    }

    /// A list field's rows: what it wields, holds or sells by name; its spells, what it spawns
    /// and its emotes as their rows.
    fn list_field(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        d: Drill,
        key: &str,
        doc: &SqlDoc,
        out: &mut Vec<UiAction>,
    ) {
        let _ = key;
        match d {
            Drill::Wields | Drill::Holds | Drill::Sells => {
                self.create_list_editor(ui, w, doc, d, out);
            }
            Drill::Spells => self.spell_book_editor(ui, w, doc, out),
            Drill::Spawns => self.generator_editor(ui, w, doc, out),
            Drill::Skills => self.skills_editor(ui, w, doc, out),
            Drill::BodyParts => self.body_parts_editor(ui, w, doc, out),
            Drill::Emotes => {
                self.emote_editor(ui, w, doc, out);
                ui.add_space(8.0);
                self.quests_section(ui, w, doc, true, out);
            }
            Drill::Appears | Drill::UsedBy => {}
        }
    }

    /// One field's value, chosen.
    fn field_widget(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        f: &Field,
        doc: &SqlDoc,
        out: &mut Vec<UiAction>,
    ) {
        let key = field_key(f);
        match (f.target, f.widget) {
            (Target::Property(kind, id), widget) => {
                self.property_widget(ui, w, kind, id, widget, doc, out);
            }
            (Target::Bits(kind, id, mask), widget) => {
                self.bits_widget(ui, w, (kind, id, mask), widget, doc, out);
            }
            (Target::Parts(_), Widget::Wield) => self.wield_widget(ui, w, doc, out),
            (Target::Parts(parts), Widget::Together(unset)) => {
                let t = self.text.table();
                let now = together(parts, doc, unset);
                ui.horizontal(|ui| {
                    for (v, label) in [(true, "yes"), (false, "no")] {
                        let r = ui.selectable_label(now == v, t.get(&format!("fields.{label}")));
                        self.editor
                            .row_buttons
                            .push((format!("{key}:{label}"), r.rect));
                        if r.clicked() && v != now {
                            out.push(UiAction::Forms(w.clone(), together_written(parts, v)));
                        }
                    }
                });
            }
            (Target::Parts(parts), _) => {
                let t = self.text.table();
                // Parts without names side by side; named, many in rows of four, a few side by
                // side, each after its name.
                if parts.iter().all(|p| p.key.is_none()) {
                    ui.horizontal(|ui| {
                        for p in parts {
                            self.property_widget(ui, w, p.kind, p.id, p.widget, doc, out);
                        }
                    });
                    return;
                }
                let across = if parts.len() > 3 { 4 } else { parts.len() };
                egui::Grid::new(("parts", f.key, w.clone()))
                    .num_columns(across * 2)
                    .show(ui, |ui| {
                        for (i, p) in parts.iter().enumerate() {
                            ui.label(p.key.map_or_else(String::new, |k| {
                                t.get(&format!("fields.{k}")).to_owned()
                            }));
                            self.property_widget(ui, w, p.kind, p.id, p.widget, doc, out);
                            if (i + 1) % across == 0 {
                                ui.end_row();
                            }
                        }
                    });
            }
            (Target::Destination, _) => self.destination_field(ui, w, doc, &key, out),
            (Target::List(d), _) => self.list_field(ui, w, d, &key, doc, out),
            (Target::Attributes, _) => self.attributes_editor(ui, w, doc, out),
            (Target::Vitals, _) => self.vitals_editor(ui, w, doc, out),
        }
    }

    /// Some of a property's bits (`kind`, `id` and the mask of them), chosen by `widget`: a check
    /// box for each it allows, or a dropdown of the choices they make. Each change is a step of
    /// its own.
    fn bits_widget(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        (kind, id, mask): (PropertyKind, u32, u32),
        widget: Widget,
        doc: &SqlDoc,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let raw = doc.property(kind, id);
        let now = bits_read(kind, id, raw.as_deref());
        let key = format!("{}:{mask:03X}", property_key(kind, id));
        let set = |to: u32| {
            UiAction::Form(
                w.clone(),
                FormEdit::Property {
                    kind,
                    id,
                    value: Some(bits_set(kind, id, raw.as_deref(), mask, to)),
                },
            )
        };
        match widget {
            Widget::Allow(bits) => {
                ui.horizontal(|ui| {
                    for (bit, k) in bits {
                        let bit = int_bits(*bit);
                        let mut allowed = now & bit == 0;
                        let r = ui.checkbox(&mut allowed, t.get(&format!("fields.{k}")));
                        self.editor
                            .picker_rows
                            .push((format!("{key}:{bit:03X}"), r.rect));
                        if r.changed() {
                            out.push(set(if allowed { now & !bit } else { now | bit }));
                        }
                    }
                });
            }
            Widget::OneOf(choices) => {
                let mine = now & mask;
                let shown: Vec<String> = one_of(choices, mine)
                    .into_iter()
                    .map(|k| t.get(&format!("fields.{k}")).to_owned())
                    .collect();
                let combo = egui::ComboBox::from_id_salt(("one of", &key, w.clone()))
                    .selected_text(shown.join(", "))
                    .show_ui(ui, |ui| {
                        for (v, k) in choices {
                            let v = int_bits(*v);
                            let r = ui.selectable_label(mine == v, t.get(&format!("fields.{k}")));
                            self.editor
                                .picker_rows
                                .push((format!("{key}:{v:03X}"), r.rect));
                            if r.clicked() && mine != v {
                                out.push(set(v));
                            }
                        }
                    });
                self.editor
                    .add_buttons
                    .push((key.clone(), combo.response.rect));
            }
            _ => {}
        }
    }

    /// One property's value, chosen by `widget`.
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn property_widget(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        kind: PropertyKind,
        id: u32,
        widget: Widget,
        doc: &SqlDoc,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let key = property_key(kind, id);
        let set = |value: Option<String>| {
            UiAction::Form(w.clone(), FormEdit::Property { kind, id, value })
        };
        match widget {
            Widget::YesNo(unset) | Widget::Opposite(unset) => {
                let opposite = matches!(widget, Widget::Opposite(_));
                let now = yes_no(doc.property(kind, id).as_deref(), unset) != opposite;
                ui.horizontal(|ui| {
                    for (v, label) in [(true, "fields.yes"), (false, "fields.no")] {
                        if ui.selectable_label(now == v, t.get(label)).clicked() && v != now {
                            out.push(set(Some(yes_no_written(kind, widget, v).into())));
                        }
                    }
                });
            }
            Widget::Choice(choices) => {
                let now = int_of(doc, kind, id);
                let shown = property_words(kind, id, widget, doc, &t, None);
                egui::ComboBox::from_id_salt(("choice", &key, w.clone()))
                    .selected_text(shown)
                    .height(300.0)
                    .show_ui(ui, |ui| {
                        for (v, k) in choices {
                            let on = now.is_some_and(|n| int_bits(n) == int_bits(*v));
                            if ui
                                .selectable_label(on, t.get(&format!("fields.{k}")))
                                .clicked()
                                && !on
                            {
                                out.push(set(Some(int_bits(*v).cast_signed().to_string())));
                            }
                        }
                    });
            }
            // Unset first, chosen too while the server runs the value as unset: choosing it
            // takes any value away.
            Widget::ChoiceOrUnset(unset, choices) => {
                let now = int_of(doc, kind, id);
                let shown = now.map_or_else(
                    || t.get(&format!("fields.{unset}")).to_owned(),
                    |n| choice_or_unset_words(unset, choices, n, &t),
                );
                let as_unset = now.is_none_or(|n| !choices.iter().any(|(v, _)| *v == n));
                egui::ComboBox::from_id_salt(("choice", &key, w.clone()))
                    .selected_text(shown)
                    .height(300.0)
                    .show_ui(ui, |ui| {
                        let r = ui.selectable_label(as_unset, t.get(&format!("fields.{unset}")));
                        self.editor
                            .picker_rows
                            .push((format!("{key}:unset"), r.rect));
                        if r.clicked() && now.is_some() {
                            out.push(set(None));
                        }
                        for (v, k) in choices {
                            let on = now == Some(*v);
                            let r = ui.selectable_label(on, t.get(&format!("fields.{k}")));
                            self.editor.picker_rows.push((format!("{key}:{v}"), r.rect));
                            if r.clicked() && !on {
                                out.push(set(Some(v.to_string())));
                            }
                        }
                    });
            }
            Widget::DateTime => {
                let now = int_of(doc, kind, id)
                    .filter(|n| *n > 0)
                    .map(date_words)
                    .unwrap_or_default();
                let refused = t.get("messages.not_a_date").to_owned();
                ui.horizontal(|ui| {
                    self.field_hinted(
                        ui,
                        w,
                        &key,
                        &now,
                        130.0,
                        t.get("fields.date_hint"),
                        move |v| date_edit(kind, id, &v, &refused),
                        out,
                    );
                    ui.weak(t.get("fields.utc"));
                });
            }
            Widget::Menu(_) | Widget::NamedMenu(_) => {
                let now = bits_read(kind, id, doc.property(kind, id).as_deref());
                let rows: Vec<(u32, String)> = match widget {
                    Widget::Menu(bits) => bits
                        .iter()
                        .map(|(b, k)| (int_bits(*b), t.get(&format!("fields.{k}")).to_owned()))
                        .collect(),
                    Widget::NamedMenu(e) => named_bits(self.schema.as_ref(), e),
                    _ => Vec::new(),
                };
                // Without the server's names, the number.
                if rows.is_empty() {
                    let shown = doc
                        .property(kind, id)
                        .map(|v| content::bare(&v))
                        .unwrap_or_default();
                    self.field(
                        ui,
                        w,
                        &key,
                        &shown,
                        80.0,
                        move |v| FormEdit::Property {
                            kind,
                            id,
                            value: Some(v),
                        },
                        out,
                    );
                    return;
                }
                let on: Vec<String> = rows
                    .iter()
                    .filter(|(b, _)| now & b == *b)
                    .map(|(_, n)| n.clone())
                    .collect();
                // It stays open while boxes are ticked, each tick a step of its own.
                let combo = egui::ComboBox::from_id_salt(("menu", &key, w.clone()))
                    .selected_text(chosen_words(&on, &t))
                    .height(320.0)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .show_ui(ui, |ui| {
                        for (bit, words) in &rows {
                            let bit = *bit;
                            let mut on = now & bit == bit;
                            let r = ui.checkbox(&mut on, words);
                            self.editor
                                .picker_rows
                                .push((format!("{key}:{bit:08X}"), r.rect));
                            if r.changed() {
                                let v = if on { now | bit } else { now & !bit };
                                out.push(set(Some(bits_value(kind, id, v))));
                            }
                        }
                    });
                self.editor
                    .add_buttons
                    .push((key.clone(), combo.response.rect));
            }
            Widget::Named(_) | Widget::PropertyName(_) => {
                let values = match widget {
                    Widget::Named(e) => enum_values(self.schema.as_ref(), e),
                    Widget::PropertyName(of) => property_names(self.schema.as_ref(), of),
                    _ => Vec::new(),
                };
                let now = int_of(doc, kind, id);
                if values.is_empty() {
                    let now = now.map(|n| n.to_string()).unwrap_or_default();
                    self.field(
                        ui,
                        w,
                        &key,
                        &now,
                        80.0,
                        move |v| FormEdit::Property {
                            kind,
                            id,
                            value: Some(v),
                        },
                        out,
                    );
                } else {
                    let shown = property_words(kind, id, widget, doc, &t, self.schema.as_ref());
                    egui::ComboBox::from_id_salt(("named", &key, w.clone()))
                        .selected_text(shown)
                        .height(300.0)
                        .show_ui(ui, |ui| {
                            for (v, words) in &values {
                                let on = now == Some(i64::from(*v));
                                if ui.selectable_label(on, words).clicked() && !on {
                                    out.push(set(Some(v.to_string())));
                                }
                            }
                        });
                }
            }
            Widget::Treasure(loot) => {
                let written = doc.property(kind, id).filter(|v| !v.trim().is_empty());
                let now = written.as_deref().and_then(|v| self.session.loot_id_of(v));
                let shown = written.as_deref().map_or_else(
                    || t.get("fields.none").to_owned(),
                    |v| self.loot_field_words(loot, v),
                );
                let all = self.loot_choices(loot);
                egui::ComboBox::from_id_salt(("treasure", &key, w.clone()))
                    .selected_text(shown)
                    .height(240.0)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(written.is_none(), t.get("fields.none"))
                            .clicked()
                        {
                            out.push(set(None));
                        }
                        for (n, words, as_written) in all {
                            if ui.selectable_label(now == Some(n), words).clicked() {
                                out.push(set(Some(as_written)));
                            }
                        }
                    });
                // The table, opened in the Loot tab.
                if let Some(n) = now {
                    let r = ui
                        .link(t.get("loot.tab"))
                        .on_hover_text(t.get("loot.open_in_tab"));
                    self.editor.links.push((key.clone(), r.rect));
                    if r.clicked() {
                        out.push(UiAction::Loot(super::LootAction::Open(Some((loot, n)))));
                    }
                }
            }
            Widget::DataId => {
                let now = property_words(kind, id, widget, doc, &t, None);
                self.field(
                    ui,
                    w,
                    &key,
                    &now,
                    100.0,
                    move |v| FormEdit::Property {
                        kind,
                        id,
                        value: Some(v),
                    },
                    out,
                );
            }
            Widget::Model => self.picker(ui, w, super::pickers::Picker::Model, doc, out),
            Widget::Table(which) => self.table_picker(ui, w, which, doc, out),
            Widget::ClothingLook => self.clothing_widget(ui, w, doc, out),
            Widget::ColorScheme => {
                if !self.scheme_widget(ui, w, doc, out) {
                    let named = Widget::Named("PaletteTemplate");
                    self.property_widget(ui, w, kind, id, named, doc, out);
                }
            }
            Widget::Shade => self.shade_widget(ui, w, doc, out),
            Widget::Icon => self.picker(ui, w, super::pickers::Picker::Icon, doc, out),
            Widget::Spell => {
                let now = int_of(doc, kind, id).and_then(|n| u32::try_from(n).ok());
                let named = now.and_then(|n| self.spell_name(n));
                ui.horizontal(|ui| {
                    self.field(
                        ui,
                        w,
                        &key,
                        &now.map(|n| n.to_string()).unwrap_or_default(),
                        70.0,
                        move |v| FormEdit::Property {
                            kind,
                            id,
                            value: (!v.trim().is_empty()).then(|| v.trim().to_owned()),
                        },
                        out,
                    );
                    if let Some(n) = named {
                        ui.weak(n);
                    }
                    let salt = format!("spell:{key}");
                    let chosen = self.spell_picker(ui, &salt, t.get("collections.choose_spell"));
                    if let Some(spell) = chosen {
                        out.push(UiAction::Form(
                            w.clone(),
                            FormEdit::Property {
                                kind,
                                id,
                                value: Some(spell.to_string()),
                            },
                        ));
                    }
                });
            }
            Widget::Lines => {
                let now = doc
                    .property(kind, id)
                    .map(|v| content::unquote(&v))
                    .unwrap_or_default();
                self.field_lines(
                    ui,
                    w,
                    &key,
                    &now,
                    LINES_WIDTH,
                    move |v| FormEdit::Property {
                        kind,
                        id,
                        value: Some(v),
                    },
                    out,
                );
            }
            Widget::Meters | Widget::Seconds => {
                let now = doc
                    .property(kind, id)
                    .map(|v| content::bare(&v))
                    .unwrap_or_default();
                ui.horizontal(|ui| {
                    self.field(
                        ui,
                        w,
                        &key,
                        &now,
                        80.0,
                        move |v| FormEdit::Property {
                            kind,
                            id,
                            value: Some(v),
                        },
                        out,
                    );
                    ui.weak(t.get(if widget == Widget::Meters {
                        "fields.meters_unit"
                    } else {
                        "fields.seconds_unit"
                    }));
                });
            }
            _ => {
                let now = doc
                    .property(kind, id)
                    .map(|v| match kind {
                        PropertyKind::String => content::unquote(&v),
                        _ => content::bare(&v),
                    })
                    .unwrap_or_default();
                let width = match (kind, widget) {
                    (PropertyKind::String, _) => 220.0,
                    (_, Widget::Decimal) => 60.0,
                    _ => 80.0,
                };
                self.field(
                    ui,
                    w,
                    &key,
                    &now,
                    width,
                    move |v| FormEdit::Property {
                        kind,
                        id,
                        value: Some(v),
                    },
                    out,
                );
            }
        }
    }

    /// What wielding it needs: the requirement, what it is of (a skill, an attribute, a vital
    /// or a creature kind by name, else a number) and how much.
    fn wield_widget(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        doc: &SqlDoc,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let what = int_of(doc, Int, 158).unwrap_or(0);
        ui.horizontal(|ui| {
            self.property_widget(ui, w, Int, 158, WIELD[0].widget, doc, out);
            if what == 0 {
                return;
            }
            if wield_is(what) {
                ui.label(t.get("fields.wield_must_be"));
                self.property_widget(ui, w, Int, 160, wield_value_widget(what), doc, out);
                return;
            }
            if what != 7 {
                self.property_widget(ui, w, Int, 159, wield_of_widget(what), doc, out);
            }
            // A yes or no property must be yes or no; anything else, at least so much.
            let must_be = wield_property(what) == Some(PropertyKind::Bool);
            ui.label(t.get(if must_be {
                "fields.wield_must_be"
            } else {
                "fields.wield_at_least"
            }));
            self.property_widget(ui, w, Int, 160, wield_value_widget(what), doc, out);
        });
    }

    /// A portal's destination: shown by place, chosen from the dungeons or set to where the
    /// camera is.
    fn destination_field(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        doc: &SqlDoc,
        key: &str,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let dungeons = self.dungeons();
        let shown = self.destination_words(doc);
        ui.vertical(|ui| {
            ui.label(shown);
            ui.horizontal(|ui| {
                if ui.small_button(t.get("fields.destination_here")).clicked() {
                    out.push(UiAction::DestinationHere(w.clone()));
                }
                egui::ComboBox::from_id_salt(key)
                    .selected_text(t.get("fields.destination_dungeon"))
                    .height(240.0)
                    .show_ui(ui, |ui| {
                        for d in &dungeons {
                            if ui.selectable_label(false, self.dungeon_name(d)).clicked() {
                                out.push(UiAction::DestinationDungeon(w.clone(), d.landblock));
                            }
                        }
                    });
            });
        });
    }

    /// Where the weenie appears in the world: each spawn, flown to by a click.
    fn appears_section(&mut self, ui: &mut egui::Ui, w: &Weenie, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let n = self.appears_of(w).len();
        ui.add_space(8.0);
        egui::CollapsingHeader::new(t.fill("flows.appears", &[("n", &n)]))
            .id_salt("appears")
            .default_open(false)
            .show(ui, |ui| self.appears_list(ui, w, out));
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's weenie fields)
    use super::*;

    /// A weenie of type `ty` with `rows` (`(table, type, value)` property rows) after it.
    fn weenie(ty: u32, rows: &[(&str, u32, &str)]) -> SqlDoc {
        let mut text = format!(
            "INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)\nVALUES (9, 'thing', {ty}, '2005-02-09 10:00:00');\n"
        );
        for (table, id, value) in rows {
            text.push_str(&format!(
                "INSERT INTO `weenie_properties_{table}` (`object_Id`, `type`, `value`)\nVALUES (9, {id}, {value});\n"
            ));
        }
        SqlDoc::parse(&text)
    }

    fn sections_of(doc: &SqlDoc) -> Vec<Section> {
        shown(Kind::of(doc), doc)
            .into_iter()
            .map(|(s, _)| s)
            .collect()
    }

    fn keys_in(doc: &SqlDoc, section: Section) -> Vec<&'static str> {
        shown(Kind::of(doc), doc)
            .into_iter()
            .find(|(s, _)| *s == section)
            .map(|(_, f)| f.iter().map(|f| f.key).collect())
            .unwrap_or_default()
    }

    fn f(key: &str) -> &'static Field {
        FIELDS.iter().find(|f| f.key == key).unwrap()
    }

    /// A few of the server's names, as `empyrean-import schema` writes them.
    fn schema() -> Schema {
        Schema::read(
            r#"{"properties":{"int":{"25":"Level","158":"WieldRequirements"},"float":{"13":"ArmorModVsSlash"},"position":{"2":"Destination"}},"enums":{"Skill":{"44":"HeavyWeapons"},"CreatureType":{"3":"Drudge","68":"OlthoiLarvae"}}}"#,
        )
        .unwrap()
    }

    #[test]
    fn every_field_its_choices_and_its_parts_have_plain_words_in_the_text() {
        let t = crate::text::read(crate::text::BUILT_IN).unwrap();
        let mut need: Vec<String> = Vec::new();
        let words_of = |w: Widget, need: &mut Vec<String>| {
            if let Widget::ChoiceOrUnset(unset, _) = w {
                need.push(format!("fields.{unset}"));
            }
            if let Widget::Choice(c)
            | Widget::ChoiceOrUnset(_, c)
            | Widget::Menu(c)
            | Widget::Allow(c)
            | Widget::OneOf(c) = w
            {
                need.extend(c.iter().map(|(_, k)| format!("fields.{k}")));
            }
        };
        for what in 0..=12 {
            words_of(wield_value_widget(what), &mut need);
        }
        for field in FIELDS {
            need.push(list_heading(field));
            words_of(field.widget, &mut need);
            if let Target::Parts(parts) = field.target {
                for p in parts {
                    need.extend(p.key.map(|k| format!("fields.{k}")));
                    words_of(p.widget, &mut need);
                }
            }
        }
        for k in [
            "fields.meters",
            "fields.seconds",
            "fields.meters_unit",
            "fields.seconds_unit",
            "fields.wield_words",
            "fields.wield_words_plain",
            "fields.wield_words_is",
            "fields.wield_must_be",
            "fields.chosen_n",
            "fields.who_can_use",
            "fields.as_unset",
            "fields.utc_time",
            "editor.changed",
            "editor.server_value",
            "editor.server_unset",
            "editor.put_back_field",
        ] {
            need.push(k.to_owned());
        }
        let missing: Vec<&String> = need.iter().filter(|k| !t.contains_key(*k)).collect();
        assert!(missing.is_empty(), "{missing:?}");
    }

    #[test]
    fn each_field_is_in_one_of_its_kinds_sections_and_each_kinds_section_has_a_field_for_it() {
        let mut keys: Vec<&str> = FIELDS.iter().map(|f| f.key).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), FIELDS.len(), "each field's key once");
        for field in FIELDS {
            for k in field.kinds {
                assert!(
                    k.sections().contains(&k.home(field.section)),
                    "{} for {k:?}",
                    field.key
                );
            }
        }
        let mine = |kind: Kind| -> Vec<&'static Field> {
            FIELDS
                .iter()
                .filter(|f| {
                    !f.when_set
                        && f.shown_while.is_none()
                        && kind.sections().contains(&kind.home(f.section))
                        && (f.kinds.is_empty() || f.kinds.contains(&kind))
                })
                .collect()
        };
        for kind in Kind::ALL {
            let fields = mine(kind);
            for s in kind.sections() {
                assert!(
                    fields.iter().any(|f| kind.home(f.section) == *s),
                    "{kind:?} {s:?}"
                );
            }
            // Two fields of one property are never both a kind's own.
            for (i, a) in fields.iter().enumerate() {
                for b in &fields[i + 1..] {
                    assert!(!overlaps(a, b), "{kind:?}: {} and {}", a.key, b.key);
                }
            }
        }
    }

    #[test]
    fn a_weenie_shows_its_kinds_sections_and_any_other_it_sets_something_in() {
        // A creature shows its own, whether it sets them or not.
        let drudge = weenie(10, &[]);
        assert_eq!(sections_of(&drudge), Kind::Creature.sections());
        // A chest that spawns shows a generator's, its spawns with them.
        let chest = weenie(20, &[("int", 81, "3"), ("int", 82, "1")]);
        assert_eq!(
            sections_of(&chest),
            [
                Section::Basic,
                Section::Lock,
                Section::Holds,
                Section::Spawns
            ]
        );
        assert_eq!(
            keys_in(&chest, Section::Spawns),
            [
                "most_at_once",
                "at_start",
                "respawns_every",
                "spread",
                "starts_after",
                "runs",
                "spawns"
            ]
        );
        // An NPC's Creature is what it wields; with a level, that much of a fight's too.
        let npc = weenie(10, &[("bool", 19, "False"), ("int", 25, "5")]);
        assert!(sections_of(&npc).contains(&Section::Creature));
        assert_eq!(
            keys_in(&npc, Section::Creature),
            ["level", "attackable", "wielded_loot", "wields"]
        );
        assert_eq!(
            keys_in(&weenie(10, &[("bool", 19, "False")]), Section::Creature),
            ["attackable", "wielded_loot", "wields"]
        );
        // A house's part shows its basics only, whatever it sets.
        let hook = weenie(56, &[("int", 6, "10"), ("int", 25, "5")]);
        assert_eq!(sections_of(&hook), [Section::Basic]);
        // A creature's mana regenerates with its health and stamina: no item's mana drain.
        let regen = weenie(10, &[("float", 5, "2")]);
        assert!(keys_in(&regen, Section::Creature).contains(&"regeneration"));
        assert!(!keys_in(&regen, Section::Magic).contains(&"mana_drain"));
        // A sword's mana drains.
        let sword = weenie(6, &[("int", 1, "1"), ("float", 5, "-0.05")]);
        assert!(keys_in(&sword, Section::Magic).contains(&"mana_drain"));
    }

    #[test]
    fn merged_sections_hold_what_their_parts_held() {
        // Armor's equipment: where it is worn, what it covers, what wearing it needs, and how it
        // protects; no Protection of its own. A creature's protection is its own.
        let armor = weenie(2, &[("int", 1, "2")]);
        assert_eq!(
            sections_of(&armor),
            [
                Section::Basic,
                Section::Appearance,
                Section::Wearing,
                Section::Magic
            ]
        );
        assert_eq!(
            keys_in(&armor, Section::Wearing),
            [
                "worn_on",
                "covers",
                "wield_needs",
                "arcane_lore",
                "armor_level",
                "protection"
            ]
        );
        assert_eq!(
            keys_in(&weenie(10, &[]), Section::Protection),
            ["protection"]
        );
        // A weapon's equipment reads as an armor's does.
        assert_eq!(Section::Wearing.name_key(), "sections.wearing");
        // A vendor's trade: what it buys, at what price, and what it sells.
        let vendor = weenie(12, &[("bool", 19, "False")]);
        assert_eq!(keys_in(&vendor, Section::Trade).last(), Some(&"sells"));
        // A generator's own and what it spawns, in one section.
        let camp = weenie(1, &[("int", 81, "4")]);
        assert_eq!(sections_of(&camp), [Section::Basic, Section::Spawns]);
        assert_eq!(keys_in(&camp, Section::Spawns).last(), Some(&"spawns"));
        // A key's use and a switch's activation are one section.
        let key = weenie(22, &[]);
        assert_eq!(
            keys_in(&key, Section::Activation),
            ["used_on", "uses", "opens_locks"]
        );
        let switch = weenie(26, &[]);
        assert_eq!(
            keys_in(&switch, Section::Activation),
            [
                "switched_on",
                "when_used",
                "says_when_triggered",
                "failure_message"
            ]
        );
        // A creature's death treasure is among its items; what it wields is in Creature, under
        // the heading of what it has equipped.
        let drudge = weenie(10, &[]);
        assert_eq!(keys_in(&drudge, Section::Holds), ["loot"]);
        let creature = keys_in(&drudge, Section::Creature);
        assert_eq!(creature[creature.len() - 2..], ["wielded_loot", "wields"]);
        assert_eq!(list_heading(f("wields")), "sections.carries");
        assert_eq!(list_heading(f("wielded_loot")), "loot.wielded_field");
        assert_eq!(list_heading(f("spells")), "fields.spells");
    }

    #[test]
    fn the_editor_lists_emotes_for_every_weenie() {
        let pages_of = |doc: &SqlDoc| -> Vec<Section> {
            pages(Kind::of(doc), doc)
                .into_iter()
                .map(|(s, _)| s)
                .collect()
        };
        // A sword with no emotes: Emotes last, its emotes field on it.
        let sword = weenie(6, &[("int", 1, "1")]);
        assert_eq!(pages_of(&sword).last(), Some(&Section::Emotes));
        assert!(
            !sections_of(&sword).contains(&Section::Emotes),
            "not in the pane"
        );
        let (_, fields) = pages(Kind::of(&sword), &sword).pop().unwrap();
        assert_eq!(fields.iter().map(|f| f.key).collect::<Vec<_>>(), ["emotes"]);
        // A creature's own, once; a house's part, after its basics.
        let drudge = weenie(10, &[]);
        assert_eq!(
            pages_of(&drudge)
                .iter()
                .filter(|s| **s == Section::Emotes)
                .count(),
            1
        );
        assert_eq!(
            pages_of(&weenie(56, &[])),
            [Section::Basic, Section::Emotes]
        );
    }

    #[test]
    fn stays_put_is_among_a_creatures_fight_fields_and_one_that_never_fights_shows_it_when_set() {
        let anywhere = |doc: &SqlDoc| {
            shown(Kind::of(doc), doc)
                .iter()
                .any(|(_, fields)| fields.iter().any(|f| f.key == "stays_put"))
        };
        // A creature: with what it does in a fight, set or not.
        let drudge = weenie(10, &[]);
        let fight = keys_in(&drudge, Section::Creature);
        let at = |k: &str| fight.iter().position(|f| *f == k).unwrap();
        assert_eq!(at("stays_put"), at("awareness") + 1, "{fight:?}");
        // An NPC and a vendor never fight: nothing shown while they leave it unset.
        let npc = weenie(10, &[("bool", 19, "False")]);
        let vendor = weenie(12, &[("bool", 19, "False")]);
        assert!(!anywhere(&npc) && !anywhere(&vendor));
        // Set, it shows with a creature's fields, never among an NPC's own.
        for ty in [10, 12] {
            let set = weenie(ty, &[("bool", 19, "False"), ("bool", 52, "True")]);
            assert!(keys_in(&set, Section::Creature).contains(&"stays_put"));
            assert!(!keys_in(&set, Section::Npc).contains(&"stays_put"));
        }
    }

    #[test]
    fn immobile_reads_never_moving_as_it_is_and_is_put_back_exactly_unset_included() {
        let t = crate::text::Text::built_in().table();
        let immobile = f("stays_put");
        assert_eq!(t.get("fields.stays_put"), "Immobile");
        let words = |rows: &[(&str, u32, &str)]| plain_words(immobile, &weenie(10, rows), &t, None);
        // Unset or false: it moves; true: it never moves from where it stands.
        assert_eq!(words(&[]), t.get("fields.no"));
        assert_eq!(words(&[("bool", 52, "False")]), t.get("fields.no"));
        assert_eq!(words(&[("bool", 52, "True")]), t.get("fields.yes"));
        // Choosing yes writes that it never moves, no that it does.
        assert_eq!(yes_no_written(Bool, immobile.widget, true), "True");
        assert_eq!(yes_no_written(Bool, immobile.widget, false), "False");
        // Put back: as the server's had it, unset, false or true.
        for (server, mine) in [
            (weenie(10, &[]), "True"),
            (weenie(10, &[("bool", 52, "False")]), "True"),
            (weenie(10, &[("bool", 52, "True")]), "False"),
        ] {
            let mut doc = weenie(10, &[("bool", 52, mine)]);
            assert!(changed(immobile, &doc, &server));
            for e in &put_back(immobile, &doc, &server) {
                assert!(super::super::form::apply_edit(&mut doc, e, None));
            }
            assert_eq!(
                doc.property(Bool, 52).map(|v| content::bare(&v)),
                server.property(Bool, 52).map(|v| content::bare(&v))
            );
        }
        // Chosen no over a server's unset: written false, still a change from unset.
        let server = weenie(10, &[]);
        let doc = weenie(10, &[("bool", 52, "False")]);
        assert!(changed(immobile, &doc, &server));
        assert_eq!(words(&[("bool", 52, "False")]), words(&[]));
    }

    #[test]
    fn a_creature_that_carries_nothing_shows_only_its_loot_and_what_npcs_say_is_their_emotes() {
        // Room inside and packs inside of -1 are nothing set: the creature's Holds is its loot.
        let drudge = weenie(10, &[("int", 6, "-1"), ("int", 7, "-1")]);
        assert_eq!(keys_in(&drudge, Section::Holds), ["loot"]);
        let npc = weenie(10, &[("bool", 19, "False"), ("int", 6, "-1")]);
        assert!(!sections_of(&npc).contains(&Section::Holds));
        // Room for something is set.
        let pack = weenie(10, &[("int", 6, "24"), ("int", 7, "-1")]);
        assert_eq!(keys_in(&pack, Section::Holds), ["holds", "loot"]);
        // A vendor's and an NPC's words are their emotes, set or not.
        for ty in [10, 12] {
            let talker = weenie(ty, &[("bool", 19, "False")]);
            assert_eq!(keys_in(&talker, Section::Emotes), ["emotes"]);
            assert_eq!(sections_of(&talker).last(), Some(&Section::Emotes));
        }
    }

    #[test]
    fn a_fields_value_is_shown_in_plain_words_and_unset_is_nothing_but_a_yes_or_no() {
        let t = crate::text::Text::built_in().table();
        let doc = weenie(
            10,
            &[
                ("bool", 19, "True"),
                ("int", 25, "5"),
                ("int", 16, "32"),
                ("d_i_d", 1, "0x020007DD"),
                ("int", 2, "68"),
                ("float", 31, "20"),
            ],
        );
        assert_eq!(plain_words(f("level"), &doc, &t, None), "5");
        assert_eq!(
            plain_words(f("attackable"), &doc, &t, None),
            t.get("fields.yes")
        );
        assert_eq!(plain_words(f("model"), &doc, &t, None), "0x020007DD");
        assert_eq!(
            plain_words(f("usable"), &doc, &t, None),
            t.get("fields.usable_remote")
        );
        assert_eq!(plain_words(f("awareness"), &doc, &t, None), "20 m");
        // A creature kind by the server's name, in plain words; a number without the names.
        let s = schema();
        assert_eq!(
            plain_words(f("creature_kind"), &doc, &t, Some(&s)),
            "Olthoi larvae"
        );
        assert_eq!(plain_words(f("creature_kind"), &doc, &t, None), "68");
        // Unset: a yes or no is the server's value, anything else nothing.
        assert_eq!(plain_words(f("locked"), &doc, &t, None), t.get("fields.no"));
        assert_eq!(
            plain_words(f("switched_on"), &doc, &t, None),
            t.get("fields.yes")
        );
        assert_eq!(plain_words(f("scale"), &doc, &t, None), "");
        // A whole number as a yes or no: anything but 0 is yes.
        let plate = weenie(24, &[("int", 119, "65535")]);
        assert_eq!(
            plain_words(f("switched_on"), &plate, &t, None),
            t.get("fields.yes")
        );
        let off = weenie(24, &[("int", 119, "0")]);
        assert_eq!(
            plain_words(f("switched_on"), &off, &t, None),
            t.get("fields.no")
        );
    }

    #[test]
    fn the_fields_next_in_line_are_on_their_kinds_pages_by_the_servers_names() {
        let t = crate::text::Text::built_in().table();
        let s = Schema::read(
            r#"{"enums":{"HookType":{"0":"Undef","1":"Floor","2":"Wall","8":"Yard"},"CombatStyle":{"2":"OneHanded","8":"TwoHanded"},"Gender":{"1":"Male","2":"Female"}}}"#,
        )
        .unwrap();
        // A sword that hangs on floors and in yards, held in one hand.
        let sword = weenie(6, &[("int", 1, "1"), ("int", 151, "9"), ("int", 46, "2")]);
        assert_eq!(
            plain_words(f("hook_type"), &sword, &t, Some(&s)),
            "Floor, Yard"
        );
        assert_eq!(
            plain_words(f("combat_style"), &sword, &t, Some(&s)),
            "One handed"
        );
        // Without the server's names, their numbers.
        assert_eq!(plain_words(f("hook_type"), &sword, &t, None), "9");
        assert_eq!(plain_words(f("combat_style"), &sword, &t, None), "2");
        // Each kind shows those it sets most often, set or not.
        let npc = weenie(10, &[("bool", 19, "False"), ("int", 113, "2")]);
        assert_eq!(
            keys_in(&npc, Section::Npc),
            [
                "title",
                "accepts_items",
                "looks_like_object",
                "player_killer_status",
                "gender"
            ]
        );
        assert_eq!(plain_words(f("gender"), &npc, &t, Some(&s)), "Female");
        // One few weenies set shows only where it is set.
        let plain = weenie(6, &[("int", 1, "1")]);
        assert!(!keys_in(&plain, Section::Weapon).contains(&"slayer"));
        let slayer = weenie(6, &[("int", 166, "3"), ("float", 138, "1.5")]);
        assert!(keys_in(&slayer, Section::Weapon).contains(&"slayer"));
        assert_eq!(plain_words(f("slayer"), &slayer, &t, None), "3 ×1.5");
        // A generator's few shows them only when set too.
        let camp = weenie(1, &[("int", 81, "4")]);
        assert!(!keys_in(&camp, Section::Spawns).contains(&"generator_event"));
        let event = weenie(1, &[("int", 81, "4"), ("string", 34, "'Halloween'")]);
        assert!(keys_in(&event, Section::Spawns).contains(&"generator_event"));
        // Advanced leaves them out once a page shows them.
        assert!(on_pages(&sword).has_property(Int, 151));
    }

    #[test]
    fn a_portals_ticks_write_its_bits_both_ways_unset_and_none_included() {
        let t = crate::text::Text::built_in().table();
        let name = |k: &str| t.get(&format!("fields.{k}")).to_owned();
        let portal = |v: &str| weenie(7, &[("int", 111, v)]);
        let words = |key: &str, doc: &SqlDoc| plain_words(f(key), doc, &t, None);
        // Under "Usable by", in this order, none with a tooltip.
        let fields = keys_in(&weenie(7, &[]), Section::Portal);
        assert_eq!(
            fields[2..],
            [
                "pk_status",
                "vitae_penalty",
                "account_status",
                "olthoi",
                "min_level",
                "max_level",
                "needs_quest",
                "portal_magic"
            ]
        );
        for key in &fields[2..] {
            assert_eq!(tooltip(f(key), &t, Some(&schema())), "", "{key}");
        }
        // Unset is anyone: each tick is written out with its first bit.
        assert_eq!(bits_read(Int, 111, None), 0x001);
        assert_eq!(bits_set(Int, 111, None, 0x00E, 0x002), "3");
        assert_eq!(bits_set(Int, 111, None, 0x030, 0x010), "17");
        assert_eq!(bits_set(Int, 111, None, 0x100, 0x100), "257");
        assert_eq!(bits_set(Int, 111, None, 0x200, 0x200), "513");
        // 49: anyone, but neither summoned nor recalled to; ticked back one at a time.
        let common = portal("49");
        assert_eq!(
            words("pk_status", &common),
            [name("allow_pk"), name("allow_pkl"), name("allow_npk")].join(", ")
        );
        assert_eq!(words("portal_magic", &common), name("none"));
        assert_eq!(bits_set(Int, 111, Some("49"), 0x030, 0x020), "33");
        assert_eq!(bits_set(Int, 111, Some("49"), 0x030, 0), "1");
        // A value without its first bit keeps it so: 22 turns away PK and PK lite.
        assert_eq!(words("pk_status", &portal("22")), name("allow_npk"));
        assert_eq!(bits_set(Int, 111, Some("22"), 0x00E, 0x004), "20");
        // 0, no player at all: every status turned away, and allowing one lets it through.
        assert_eq!(bits_read(Int, 111, Some("0")), 0x00E);
        assert_eq!(words("pk_status", &portal("0")), name("none"));
        assert_eq!(bits_set(Int, 111, Some("0"), 0x00E, 0x006), "6");
        // Allowing every status of one that allowed none lets anyone through: never 0.
        assert_eq!(bits_set(Int, 111, Some("0"), 0x00E, 0), "1");
        assert_eq!(bits_set(Int, 111, Some("14"), 0x00E, 0), "1");
        // Olthoi: allowed, not allowed, only; both bits, both named.
        assert_eq!(words("olthoi", &portal("1")), name("olthoi_allowed"));
        assert_eq!(words("olthoi", &portal("129")), name("olthoi_not_allowed"));
        assert_eq!(words("olthoi", &portal("65")), name("olthoi_only"));
        assert_eq!(
            words("olthoi", &portal("193")),
            format!("{}, {}", name("olthoi_not_allowed"), name("olthoi_only"))
        );
        assert_eq!(bits_set(Int, 111, Some("193"), 0x0C0, 0x040), "65");
        // Each row is its own change, and is put back alone; the server's value as written once
        // every row matches.
        let server = portal("49");
        let mut doc = portal("35");
        assert!(changed(f("pk_status"), &doc, &server));
        assert!(changed(f("portal_magic"), &doc, &server));
        assert!(!changed(f("vitae_penalty"), &doc, &server));
        let apply = |doc: &mut SqlDoc, key: &str| {
            for e in &put_back(f(key), doc, &server) {
                assert!(super::super::form::apply_edit(doc, e, None), "{e:?}");
            }
        };
        apply(&mut doc, "portal_magic");
        assert_eq!(doc.property(Int, 111).as_deref(), Some("51"));
        apply(&mut doc, "pk_status");
        assert_eq!(doc.property(Int, 111).as_deref(), Some("49"));
        // Over an unset server's: unset again, from a change or from 0.
        let server = weenie(7, &[]);
        for v in ["3", "0"] {
            let mut doc = portal(v);
            for e in &put_back(f("pk_status"), &doc, &server) {
                assert!(super::super::form::apply_edit(&mut doc, e, None));
            }
            assert_eq!(doc.property(Int, 111), None, "{v}");
        }
    }

    #[test]
    fn one_locked_tick_writes_both_locks_and_reads_locked_when_either_is() {
        let t = crate::text::Text::built_in().table();
        let locked = f("locked");
        let words = |rows: &[(&str, u32, &str)]| plain_words(locked, &weenie(19, rows), &t, None);
        assert_eq!(words(&[]), t.get("fields.no"));
        assert_eq!(words(&[("bool", 3, "True")]), t.get("fields.yes"));
        assert_eq!(words(&[("bool", 35, "True")]), t.get("fields.yes"));
        assert_eq!(
            words(&[("bool", 3, "False"), ("bool", 35, "False")]),
            t.get("fields.no")
        );
        // A tick writes both, as one step.
        let Target::Parts(parts) = locked.target else {
            panic!("{:?}", locked.target)
        };
        let mut door = weenie(19, &[("bool", 3, "True")]);
        let off = together_written(parts, false);
        assert_eq!(off.len(), 2);
        for e in &off {
            assert!(super::super::form::apply_edit(&mut door, e, None));
        }
        assert_eq!(door.property(Bool, 3).as_deref(), Some("False"));
        assert_eq!(door.property(Bool, 35).as_deref(), Some("False"));
        // Put back as the server has each: locked now, never set to lock again.
        let server = weenie(19, &[("bool", 3, "True")]);
        for e in &put_back(locked, &door, &server) {
            assert!(super::super::form::apply_edit(&mut door, e, None));
        }
        assert!(!changed(locked, &door, &server));
        assert_eq!(door.property(Bool, 35), None);
        // One field for both: none of its own for locking again.
        assert_eq!(
            keys_in(&weenie(19, &[]), Section::Lock),
            [
                "locked",
                "lock_difficulty",
                "lock_code",
                "starts_open",
                "resets_after",
                "open"
            ]
        );
    }

    #[test]
    fn a_wield_need_of_a_players_property_names_it_and_a_yes_or_no_must_match() {
        let t = crate::text::Text::built_in().table();
        let s = Schema::read(
            r#"{"properties":{"int":{"25":"Level","287":"SocietyRankCelhan"},"bool":{"1":"Stuck","120":"IsPsr"}}}"#,
        )
        .unwrap();
        // Its "of" by the names of the player's properties of its type.
        assert_eq!(wield_of_widget(9), Widget::PropertyName(Int));
        assert_eq!(wield_of_widget(10), Widget::PropertyName(Bool));
        assert_eq!(wield_of_widget(2), Widget::Named("Skill"));
        assert_eq!(wield_of_widget(7), Widget::Whole);
        assert_eq!(
            property_names(Some(&s), Int),
            [
                (25, "Level".to_owned()),
                (287, "SocietyRankCelhan".to_owned())
            ]
        );
        assert_eq!(
            property_names(Some(&s), Bool),
            [(120, "IsPsr".to_owned()), (1, "Stuck".to_owned())]
        );
        let sword = |rows: &[(&str, u32, &str)]| weenie(6, rows);
        let rank = sword(&[("int", 158, "9"), ("int", 159, "287"), ("int", 160, "601")]);
        assert_eq!(
            plain_words(f("wield_needs"), &rank, &t, Some(&s)),
            format!(
                "{} SocietyRankCelhan, at least 601",
                t.get("fields.wield_number")
            )
        );
        // A yes or no: what the player's must be.
        for (v, yes) in [("1", "fields.yes"), ("0", "fields.no")] {
            let psr = sword(&[("int", 158, "10"), ("int", 159, "120"), ("int", 160, v)]);
            assert_eq!(
                plain_words(f("wield_needs"), &psr, &t, Some(&s)),
                format!("{} IsPsr: {}", t.get("fields.wield_yes_no"), t.get(yes))
            );
        }
        // Without the server's names, its number.
        assert_eq!(
            plain_words(f("wield_needs"), &rank, &t, None),
            format!("{} 287, at least 601", t.get("fields.wield_number"))
        );
    }

    #[test]
    fn a_choice_reads_negative_values_and_flags_read_the_high_bit() {
        let t = crate::text::Text::built_in().table();
        let ring = weenie(2, &[("int", 33, "-2"), ("int", 1, "-2147483648")]);
        assert_eq!(
            plain_words(f("bonded"), &ring, &t, None),
            t.get("fields.bonded_destroy")
        );
        assert_eq!(
            plain_words(f("item_type"), &ring, &t, None),
            t.get("fields.item_gameboard")
        );
        let key = weenie(22, &[("int", 94, "640")]);
        assert_eq!(
            plain_words(f("used_on"), &key, &t, None),
            format!(
                "{}, {}",
                t.get("fields.item_misc"),
                t.get("fields.item_container")
            )
        );
        // Bits written back as the property's own signed number.
        let mut doc = weenie(1, &[]);
        let high = FormEdit::Property {
            kind: PropertyKind::Int,
            id: 1,
            value: Some(int_bits(0x8000_0000).cast_signed().to_string()),
        };
        assert!(super::super::form::apply_edit(&mut doc, &high, None));
        assert_eq!(
            plain_words(f("item_type"), &doc, &t, None),
            t.get("fields.item_gameboard")
        );
    }

    #[test]
    fn a_field_of_several_properties_says_them_together() {
        let t = crate::text::Text::built_in().table();
        let s = schema();
        let armor = weenie(2, &[("float", 13, "0.8"), ("float", 14, "0.7")]);
        assert_eq!(
            plain_words(f("protection"), &armor, &t, None),
            "Slashing 0.8, Piercing 0.7"
        );
        let wand = weenie(35, &[("int", 108, "3000"), ("int", 107, "1500")]);
        assert_eq!(plain_words(f("mana"), &wand, &t, None), "1500 / 3000");
        // Only one set: each said by its name, or alone when it has none.
        let half = weenie(35, &[("int", 108, "3000")]);
        assert_eq!(plain_words(f("mana"), &half, &t, None), "Max 3000");
        let kit = weenie(28, &[("int", 89, "2"), ("int", 90, "10")]);
        assert_eq!(plain_words(f("restores"), &kit, &t, None), "Health: 10");
        let some = weenie(28, &[("int", 90, "10")]);
        assert_eq!(plain_words(f("restores"), &some, &t, None), "10");
        let level = weenie(
            6,
            &[("int", 158, "7"), ("int", 159, "1"), ("int", 160, "50")],
        );
        assert_eq!(
            plain_words(f("wield_needs"), &level, &t, None),
            "Level, at least 50"
        );
        let skill = weenie(
            6,
            &[("int", 158, "2"), ("int", 159, "44"), ("int", 160, "250")],
        );
        assert_eq!(
            plain_words(f("wield_needs"), &skill, &t, Some(&s)),
            "Skill (Base) Heavy weapons, at least 250"
        );
        assert_eq!(plain_words(f("wield_needs"), &weenie(6, &[]), &t, None), "");
        // A creature kind is what the player must be, read from its "at least".
        let kind = weenie(
            2,
            &[("int", 158, "11"), ("int", 159, "0"), ("int", 160, "3")],
        );
        assert_eq!(
            plain_words(f("wield_needs"), &kind, &t, Some(&s)),
            "Creature type: Drudge"
        );
    }

    #[test]
    fn a_training_or_heritage_wield_need_reads_its_value_by_name_and_keeps_an_unknown_number() {
        let t = crate::text::Text::built_in().table();
        let s = schema();
        let need = |what: &str, of: &str, n: &str| {
            plain_words(
                f("wield_needs"),
                &weenie(6, &[("int", 158, what), ("int", 159, of), ("int", 160, n)]),
                &t,
                Some(&s),
            )
        };
        // Training: the least advancement class, 1 untrained to 3 specialized.
        assert_eq!(wield_value_widget(8), Widget::Choice(TRAINING));
        assert_eq!(
            TRAINING.iter().map(|(v, _)| *v).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(
            need("8", "44", "2"),
            "Skill Trained Heavy weapons, at least Trained"
        );
        assert_eq!(
            need("8", "44", "3"),
            "Skill Trained Heavy weapons, at least Specialized"
        );
        // Heritage: the group the wielder must be, its "of" unread.
        assert_eq!(wield_value_widget(12), Widget::Choice(HERITAGES));
        assert_eq!(
            HERITAGES.iter().map(|(v, _)| *v).collect::<Vec<_>>(),
            (1..=13).collect::<Vec<_>>()
        );
        assert_eq!(need("12", "0", "2"), "Heritage: Gharu'ndim");
        assert_eq!(need("12", "0", "13"), "Heritage: Olthoi Spitter");
        // A value outside the list reads as its number.
        assert_eq!(need("12", "0", "40"), "Heritage: 40");
        assert_eq!(
            need("8", "44", "7"),
            "Skill Trained Heavy weapons, at least 7"
        );
        // The others keep their numbers.
        assert_eq!(wield_value_widget(7), Widget::Whole);
        assert_eq!(wield_value_widget(1), Widget::Whole);
    }

    #[test]
    fn a_fields_tooltip_names_what_it_writes_by_the_servers_names() {
        let s = schema();
        assert_eq!(server_names(f("level"), Some(&s)), ["Int 25 Level"]);
        assert_eq!(server_names(f("level"), None), ["Int 25"]);
        let protection = server_names(f("protection"), Some(&s));
        assert_eq!(protection.len(), 8);
        assert_eq!(protection[0], "Float 13 ArmorModVsSlash");
        assert_eq!(
            server_names(f("destination"), Some(&s)),
            ["Position 2 Destination"]
        );
        assert!(server_names(f("emotes"), Some(&s)).is_empty());
        let t = crate::text::Text::built_in().table();
        assert_eq!(
            tooltip(f("hidden"), &t, None),
            format!("{}\n\nBool 18", t.get("fields.hidden_about"))
        );
        // A field whose words say nothing more of it: the server's names alone.
        assert_eq!(tooltip(f("level"), &t, Some(&s)), "Int 25 Level");
        assert!(!t.has("fields.name_about"));
        assert_eq!(tooltip(f("name"), &t, None), "String 1");
    }

    #[test]
    fn the_servers_names_read_as_plain_words() {
        assert_eq!(spaced("HeavyWeapons"), "Heavy weapons");
        assert_eq!(spaced("DarkAquaMetal"), "Dark aqua metal");
        assert_eq!(spaced("Drudge"), "Drudge");
        assert_eq!(spaced("PKLite"), "PK lite");
        assert_eq!(spaced("Sigil2"), "Sigil 2");
    }

    #[test]
    fn a_field_changed_from_the_servers_is_found_and_put_back_as_the_server_has_it() {
        let server = weenie(
            10,
            &[("int", 25, "5"), ("float", 64, "1"), ("float", 65, "0.5")],
        );
        let mut doc = weenie(
            10,
            &[
                ("int", 25, "6"),
                ("float", 64, "1.0"),
                ("float", 65, "0.5"),
                ("float", 66, "0.75"),
            ],
        );
        assert!(changed(f("level"), &doc, &server));
        assert!(!changed(f("attackable"), &doc, &server));
        assert!(changed(f("resists"), &doc, &server), "one part added");
        // Put back: each part that differs as the server has it, one it lacks taken away.
        let edits = put_back(f("resists"), &doc, &server);
        assert_eq!(
            edits,
            [FormEdit::Property {
                kind: PropertyKind::Float,
                id: 66,
                value: None
            }]
        );
        let level = put_back(f("level"), &doc, &server);
        for e in edits.iter().chain(&level) {
            assert!(super::super::form::apply_edit(&mut doc, e, None), "{e:?}");
        }
        assert!(!changed(f("resists"), &doc, &server));
        assert!(!changed(f("level"), &doc, &server));
        // A value written another way is the same value.
        let one = weenie(10, &[("float", 64, "1"), ("bool", 19, "1")]);
        let other = weenie(10, &[("float", 64, "1.000"), ("bool", 19, "True")]);
        assert!(!changed(f("resists"), &one, &other));
        assert!(!changed(f("attackable"), &one, &other));
        // Emotes are put back whole.
        assert!(matches!(
            put_back(f("emotes"), &doc, &server)[..],
            [FormEdit::Emotes(_)]
        ));
    }

    #[test]
    fn a_grouped_field_the_server_leaves_unset_is_put_back_unset_every_part_of_it_taking() {
        // A sword that needs level 3, of a server sword that needs nothing: the requirement and
        // its level taken away, and its "of", which neither sets, left alone.
        let server = weenie(6, &[("int", 1, "1")]);
        let mut doc = weenie(6, &[("int", 1, "1"), ("int", 158, "7"), ("int", 160, "3")]);
        assert!(changed(f("wield_needs"), &doc, &server));
        let edits = put_back(f("wield_needs"), &doc, &server);
        assert_eq!(edits.len(), 2, "{edits:?}");
        for e in &edits {
            assert!(super::super::form::apply_edit(&mut doc, e, None), "{e:?}");
        }
        assert!(!changed(f("wield_needs"), &doc, &server));
        assert!(doc.property(PropertyKind::Int, 158).is_none());
        // Every other grouped field: put back over a server's that sets none of it, each part
        // set on the pack's side, then over one that sets only its first part.
        for field in FIELDS
            .iter()
            .filter(|f| matches!(f.target, Target::Parts(_)))
        {
            let parts = properties(field);
            let value = |k: PropertyKind| match k {
                PropertyKind::Float => "0.5",
                _ => "4",
            };
            for server_sets in [0, 1] {
                let rows: Vec<(&str, u32, &str)> = parts
                    .iter()
                    .take(server_sets)
                    .map(|(k, id)| (table_of(*k), *id, value(*k)))
                    .collect();
                let server = weenie(10, &rows);
                let mine: Vec<(&str, u32, &str)> = parts
                    .iter()
                    .map(|(k, id)| (table_of(*k), *id, if *k == Float { "0.25" } else { "2" }))
                    .collect();
                let mut doc = weenie(10, &mine);
                for e in &put_back(field, &doc, &server) {
                    assert!(
                        super::super::form::apply_edit(&mut doc, e, None),
                        "{}: {e:?}",
                        field.key
                    );
                }
                assert!(!changed(field, &doc, &server), "{}", field.key);
            }
        }
    }

    /// The table a property of type `k` is written in, as `weenie` takes it.
    fn table_of(k: PropertyKind) -> &'static str {
        match k {
            PropertyKind::Int => "int",
            PropertyKind::Int64 => "int64",
            PropertyKind::Bool => "bool",
            PropertyKind::Float => "float",
            PropertyKind::String => "string",
            PropertyKind::DataId => "d_i_d",
            PropertyKind::InstanceId => "i_i_d",
        }
    }

    #[test]
    fn a_closed_dropdown_of_check_boxes_names_one_or_two_and_counts_more() {
        let t = crate::text::Text::built_in().table();
        let name = |k: &str| t.get(&format!("fields.{k}")).to_owned();
        let menu_words = |bits: &[(i64, &str)], now: u32| chosen_words(&ticked(bits, now, &t), &t);
        assert_eq!(menu_words(COVERS, 0), t.get("fields.none"));
        assert_eq!(menu_words(COVERS, 0x4000), name("body_head"));
        assert_eq!(
            menu_words(COVERS, 0x0400 | 0x0800),
            format!("{}, {}", name("body_chest"), name("body_abdomen"))
        );
        assert_eq!(
            menu_words(COVERS, 0x0400 | 0x0800 | 0x1000),
            t.fill("fields.chosen_n", &[("n", &3)])
        );
        // Those that are chosen this way: every field of bits.
        for key in [
            "worn_on",
            "covers",
            "used_on",
            "damage_type",
            "buys",
            "when_used",
        ] {
            assert!(matches!(f(key).widget, Widget::Menu(_)), "{key}");
        }
    }

    #[test]
    fn damage_types_and_what_a_vendor_buys_read_and_write_several_bits_at_once() {
        let t = crate::text::Text::built_in().table();
        let name = |k: &str| t.get(&format!("fields.{k}")).to_owned();
        // A spear that slashes and pierces, and an ammo that does nothing set.
        let spear = weenie(6, &[("int", 45, "3")]);
        assert_eq!(
            plain_words(f("damage_type"), &spear, &t, None),
            format!("{}, {}", name("damage_slash"), name("damage_pierce"))
        );
        assert_eq!(plain_words(f("damage_type"), &weenie(5, &[]), &t, None), "");
        // Weapons are two bits, ticked together; with armor, three.
        let vendor = weenie(12, &[("int", 74, "259")]);
        assert_eq!(
            plain_words(f("buys"), &vendor, &t, None),
            format!("{}, {}", name("buys_weapons"), name("buys_armor"))
        );
        let one = weenie(12, &[("int", 74, "1")]);
        assert_eq!(plain_words(f("buys"), &one, &t, None), name("none"));
        // Written back as the property's own number.
        assert_eq!(bits_value(Int, 45, 0x3 | 0x10), "19");
        assert_eq!(bits_value(Int, 74, 0x101 | 0x2), "259");
    }

    #[test]
    fn a_switchs_actions_are_any_together_and_unset_is_use() {
        let t = crate::text::Text::built_in().table();
        let name = |k: &str| t.get(&format!("fields.{k}")).to_owned();
        let actions = f("when_used");
        let words = |rows: &[(&str, u32, &str)]| plain_words(actions, &weenie(26, rows), &t, None);
        // Unset is use, to the server.
        assert_eq!(bits_read(Int, 83, None), 0x2);
        assert_eq!(words(&[]), name("activation_use"));
        // Spawns and says something, as 18 of the server's weenies do; one uses too.
        assert_eq!(
            words(&[("int", 83, "65552")]),
            format!(
                "{}, {}",
                name("activation_talk"),
                name("activation_generate")
            )
        );
        assert_eq!(
            words(&[("int", 83, "65554")]),
            format!(
                "{}, {}, {}",
                name("activation_use"),
                name("activation_talk"),
                name("activation_generate")
            )
        );
        // No ticks: nothing.
        assert_eq!(words(&[("int", 83, "0")]), name("none"));
        // Ticking says something on an unset one keeps its use.
        let now = bits_read(Int, 83, None);
        assert_eq!(bits_value(Int, 83, now | 0x10), "18");
        assert!(!t.has("fields.activation_nothing"));
    }

    /// A weenie's create list and destination, as rows its SQL writes.
    fn with_rows(head: &SqlDoc, create: &[(u32, u32, &str)], cell: Option<&str>) -> SqlDoc {
        let mut text = head.render();
        if !create.is_empty() {
            text.push_str("INSERT INTO `weenie_properties_create_list` (`object_Id`, `destination_Type`, `weenie_Class_Id`, `stack_Size`, `palette`, `shade`, `try_To_Bond`)\nVALUES ");
            let rows: Vec<String> = create
                .iter()
                .map(|(d, c, shade)| format!("(9, {d}, {c}, 1, 0, {shade}, False)"))
                .collect();
            text.push_str(&rows.join("\n     , "));
            text.push_str(";\n");
        }
        if let Some(cell) = cell {
            text.push_str(&format!("INSERT INTO `weenie_properties_position` (`object_Id`, `position_Type`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`)\nVALUES (9, 2, {cell}, 10, 20, 0, 1, 0, 0, 0)\n     , (9, 1, 0xA9B40001, 1, 1, 1, 1, 0, 0, 0);\n"));
        }
        SqlDoc::parse(&text)
    }

    #[test]
    fn a_list_and_a_destination_changed_from_the_servers_are_put_back_as_the_server_has_them() {
        let head = weenie(10, &[]);
        let server = with_rows(
            &head,
            &[(2, 100, "0"), (10, 1, "0.5"), (10, 2, "0.5"), (1, 7, "0")],
            None,
        );
        // The pack's: a wield taken away, another's chance changed, what it holds kept.
        let mut doc = with_rows(&head, &[(1, 7, "0"), (2, 100, "0"), (10, 1, "1")], None);
        assert!(changed(f("wields"), &doc, &server));
        assert!(!changed(f("contents"), &doc, &server));
        let edits = put_back(f("wields"), &doc, &server);
        assert_eq!(edits.len(), 1);
        for e in &edits {
            assert!(super::super::form::apply_edit(&mut doc, e, None));
        }
        assert!(!changed(f("wields"), &doc, &server), "{}", doc.render());
        assert!(!changed(f("contents"), &doc, &server));
        // What it holds stays first, where it was.
        let first = collections::rows_of(&doc, collections::CREATE_LIST)[0].clone();
        assert_eq!(first.number("weenie_Class_Id"), Some(7.0));
        // A portal's destination: put back, its other positions kept.
        let portal = weenie(7, &[]);
        let theirs = with_rows(&portal, &[], Some("0x019E0100"));
        let mut mine = with_rows(&portal, &[], Some("0x01A00100"));
        assert!(changed(f("destination"), &mine, &theirs));
        for e in &put_back(f("destination"), &mine, &theirs) {
            assert!(super::super::form::apply_edit(&mut mine, e, None));
        }
        assert!(!changed(f("destination"), &mine, &theirs));
        assert_eq!(mine.rows(collections::POSITIONS).len(), 2);
    }

    #[test]
    fn advanced_leaves_out_what_the_other_pages_show_until_it_shows_all() {
        // A drudge: its name and level on its pages, a property no page has in Advanced; what it
        // wields and its treasure, its spells and its emotes all on its pages.
        let drudge = with_rows(
            &weenie(
                10,
                &[
                    ("string", 1, "'Drudge'"),
                    ("int", 25, "5"),
                    ("int", 9999, "1"),
                ],
            ),
            &[(2, 100, "0"), (9, 7, "0.5")],
            None,
        );
        let left = on_pages(&drudge);
        assert!(left.has_property(Str, 1) && left.has_property(Int, 25));
        assert!(left.has_property(DataId, 35), "its loot table's field");
        assert!(!left.has_property(Int, 9999));
        for table in [
            collections::CREATE_LIST,
            collections::SPELL_BOOK,
            EMOTE_TABLE,
        ] {
            assert!(left.has_table(table), "{table}");
        }
        assert!(!left.has_table("weenie_properties_palette"));
        // Show all: nothing left out.
        assert!(!OnPages::default().has_property(Int, 25));
        // A create list with a row no list shows (a house's for sale) stays in Advanced.
        let chest = with_rows(&weenie(20, &[]), &[(1, 7, "0"), (16, 8, "0")], None);
        assert!(!on_pages(&chest).has_table(collections::CREATE_LIST));
        let plain = with_rows(&weenie(20, &[]), &[(1, 7, "0")], None);
        assert!(on_pages(&plain).has_table(collections::CREATE_LIST));
        // A portal's positions: one besides its destination keeps them in Advanced.
        let portal = with_rows(&weenie(7, &[]), &[], Some("0x01A00100"));
        assert!(!on_pages(&portal).has_table(collections::POSITIONS));
        // An NPC with no spells keeps its spell book there, to add the first.
        let npc = weenie(10, &[("bool", 19, "False")]);
        assert!(!on_pages(&npc).has_table(collections::SPELL_BOOK));
        // The bits of a portal's flags shown apart are its one property.
        assert!(on_pages(&weenie(7, &[("int", 111, "49")])).has_property(Int, 111));
    }

    #[test]
    fn a_field_asked_for_opens_its_page_and_a_property_without_one_opens_advanced() {
        let npc = weenie(
            10,
            &[("bool", 19, "False"), ("int", 25, "5"), ("float", 64, "1")],
        );
        let sections = shown(Kind::of(&npc), &npc);
        assert_eq!(
            page_of("int:25", &sections),
            Page::Section(Section::Creature)
        );
        assert_eq!(
            page_of("string:1", &sections),
            Page::Section(Section::Basic)
        );
        // A line of an emote, as what an NPC says: Emotes.
        assert_eq!(
            page_of("action:0:0", &sections),
            Page::Section(Section::Emotes)
        );
        // A part of a field of several: that field's page.
        assert_eq!(
            page_of("float:64", &sections),
            Page::Section(Section::Creature)
        );
        assert_eq!(page_of("int:9999", &sections), Page::Advanced);
    }

    #[test]
    fn runs_reads_each_value_as_the_server_runs_it_and_always_is_unset() {
        let t = crate::text::Text::built_in().table();
        let name = |k: &str| t.get(&format!("fields.{k}")).to_owned();
        let runs = f("runs");
        // Always is unset; each other choice writes the server's value of it.
        let W::ChoiceOrUnset(unset, choices) = runs.widget else {
            panic!("{:?}", runs.widget)
        };
        assert_eq!(unset, "runs_always");
        assert_eq!(
            choices,
            [
                (1, "runs_between"),
                (3, "runs_event"),
                (4, "runs_night"),
                (5, "runs_day")
            ]
        );
        let words = |v: &str| plain_words(runs, &weenie(1, &[("int", 142, v)]), &t, None);
        assert_eq!(words("1"), name("runs_between"));
        assert_eq!(words("3"), name("runs_event"));
        assert_eq!(words("4"), name("runs_night"));
        assert_eq!(words("5"), name("runs_day"));
        // 0 is the server's unset; 2, and any other value, run always too, and say their number.
        assert_eq!(words("0"), name("runs_always"));
        assert_eq!(words("2"), format!("{} (2)", name("runs_always")));
        assert_eq!(words("7"), format!("{} (7)", name("runs_always")));
        assert_eq!(plain_words(runs, &weenie(1, &[]), &t, None), "");
    }

    #[test]
    fn a_set_time_is_written_from_a_date_in_utc_and_read_back_and_nothing_typed_takes_it_away() {
        let t = crate::text::Text::built_in().table();
        assert_eq!(date_written("1999-12-31 20:00"), Some(Some(946_670_400)));
        assert_eq!(
            date_written(" 1999-12-31T20:07 UTC "),
            Some(Some(946_670_820))
        );
        assert_eq!(date_written("1999-12-31"), Some(Some(946_598_400)));
        assert_eq!(date_written("1999-12-31 20:07:30"), Some(Some(946_670_850)));
        assert_eq!(date_written("2024-02-29 00:00"), Some(Some(1_709_164_800)));
        assert_eq!(
            date_written("2038-01-19 03:14:07"),
            Some(Some(i64::from(i32::MAX)))
        );
        // Nothing, or 1970's first moment: no bound.
        assert_eq!(date_written(""), Some(None));
        assert_eq!(date_written("1970-01-01 00:00"), Some(None));
        // No date, a day its month lacks, and what a whole number property cannot hold.
        for typed in [
            "tomorrow",
            "946670400",
            "12/31/1999",
            "1999-12-31 25:00",
            "1999-12-31 20",
            "2023-02-29",
            "1999-13-01",
            "1969-12-31 23:59",
            "2038-01-19 03:14:08",
        ] {
            assert_eq!(date_written(typed), None, "{typed}");
        }
        // Read back as written, its seconds only when it has some.
        assert_eq!(date_words(946_670_400), "1999-12-31 20:00");
        assert_eq!(date_words(946_670_850), "1999-12-31 20:07:30");
        for secs in [
            1,
            946_670_400,
            951_782_400,
            1_709_164_800,
            i64::from(i32::MAX),
        ] {
            assert_eq!(date_written(&date_words(secs)), Some(Some(secs)), "{secs}");
        }
        // The fields: From and Until each write their property, and say it in UTC.
        let from = f("runs_from");
        let until = f("runs_until");
        assert_eq!(from.target, Target::Property(Int, 143));
        assert_eq!(until.target, Target::Property(Int, 144));
        let mut camp = weenie(1, &[("int", 81, "4"), ("int", 142, "1")]);
        for (typed, id) in [("1999-12-31 20:00", 143), ("1999-12-31 20:07", 144)] {
            let e = date_edit(Int, id, typed, "no");
            assert!(super::super::form::apply_edit(&mut camp, &e, None), "{e:?}");
        }
        assert_eq!(camp.property(Int, 143).as_deref(), Some("946670400"));
        assert_eq!(camp.property(Int, 144).as_deref(), Some("946670820"));
        assert_eq!(
            plain_words(from, &camp, &t, None),
            t.fill("fields.utc_time", &[("time", &"1999-12-31 20:00")])
        );
        assert_eq!(plain_words(until, &camp, &t, None), "1999-12-31 20:07 UTC");
        // Nothing typed takes the bound away; what is no date is refused, nothing written.
        let e = date_edit(Int, 143, "  ", "no");
        assert!(super::super::form::apply_edit(&mut camp, &e, None));
        assert_eq!(camp.property(Int, 143), None);
        assert_eq!(plain_words(from, &camp, &t, None), "");
        assert_eq!(
            date_edit(Int, 144, "soon", "no"),
            FormEdit::Refused("no".into())
        );
        // 0, or before 1970 began, is no bound to the server: nothing said.
        for v in ["0", "-5"] {
            let doc = weenie(1, &[("int", 142, "1"), ("int", 143, v)]);
            assert_eq!(plain_words(from, &doc, &t, None), "", "{v}");
        }
    }

    #[test]
    fn set_times_show_after_runs_while_between_set_times_and_its_event_while_during_one() {
        let camp = |rows: &[(&str, u32, &str)]| {
            let mut all = vec![("int", 81, "4")];
            all.extend_from_slice(rows);
            weenie(1, &all)
        };
        let after_runs = |doc: &SqlDoc| -> Vec<&'static str> {
            let keys = keys_in(doc, Section::Spawns);
            let at = keys.iter().position(|k| *k == "runs").unwrap();
            keys[at + 1..keys.len() - 1].to_vec()
        };
        // Always: neither, its event's field only when set.
        assert!(after_runs(&camp(&[])).is_empty());
        assert!(after_runs(&camp(&[("int", 142, "4")])).is_empty());
        // Between set times: From and Until, set or not.
        assert_eq!(
            after_runs(&camp(&[("int", 142, "1")])),
            ["runs_from", "runs_until"]
        );
        // During an event: its name, set or not.
        assert_eq!(after_runs(&camp(&[("int", 142, "3")])), ["generator_event"]);
        assert_eq!(
            after_runs(&camp(&[("int", 142, "3"), ("string", 34, "'Halloween'")])),
            ["generator_event"]
        );
        // A chest that spawns shows them with its own Runs.
        let chest = weenie(20, &[("int", 81, "3"), ("int", 142, "1")]);
        assert!(keys_in(&chest, Section::Spawns).contains(&"runs_from"));
        // A kind that shows only its basics shows none of them.
        let hook = weenie(56, &[("int", 142, "1"), ("int", 143, "946670400")]);
        assert_eq!(sections_of(&hook), [Section::Basic]);
        // The Definition pane shows them as the editor's page does, and Advanced leaves them
        // out while they show; times kept while it runs otherwise stay in Advanced.
        let between = camp(&[("int", 142, "1"), ("int", 143, "946670400")]);
        let left = on_pages(&between);
        assert!(left.has_property(Int, 143) && left.has_property(Int, 144));
        let night = camp(&[("int", 142, "4"), ("int", 143, "946670400")]);
        assert!(!keys_in(&night, Section::Spawns).contains(&"runs_from"));
        assert!(!on_pages(&night).has_property(Int, 143));
        assert!(on_pages(&camp(&[("int", 142, "3")])).has_property(Str, 34));
        // Each is shown after a field of the property it is shown for, in the same section.
        for field in FIELDS {
            if let Some((k, id, _)) = field.shown_while {
                let at = FIELDS.iter().position(|g| std::ptr::eq(g, field)).unwrap();
                assert!(
                    FIELDS[..at]
                        .iter()
                        .any(|g| g.section == field.section && properties(g).contains(&(k, id))),
                    "{}",
                    field.key
                );
            }
        }
    }
}
