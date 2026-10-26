//! What a weenie is, read from its data: its weenie type, its item type, whether it can be
//! attacked and whether it spawns things. Each kind has the sections its Definition pane and its
//! editor show first; a section another kind has shows too when the weenie sets something in it.

use lyceum_foundry::content::{self, PropertyKind, SqlDoc};

/// What a weenie is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Creature,
    /// A creature that cannot be attacked.
    Npc,
    Vendor,
    MeleeWeapon,
    MissileWeapon,
    /// Arrows, bolts, darts and thrown weapons.
    Ammo,
    Caster,
    Armor,
    Clothing,
    Jewelry,
    Portal,
    Door,
    Chest,
    Container,
    /// Something that spawns others and is no item itself.
    Generator,
    Switch,
    PressurePlate,
    Hotspot,
    Food,
    Gem,
    Key,
    Healer,
    Scroll,
    Book,
    /// A house, its hooks, its slum lord and its deed.
    House,
    /// Any other item or device.
    Item,
}

/// The weenie types, as the server numbers them.
mod wt {
    pub const GENERIC: u32 = 1;
    pub const CLOTHING: u32 = 2;
    pub const MISSILE_LAUNCHER: u32 = 3;
    pub const MISSILE: u32 = 4;
    pub const AMMUNITION: u32 = 5;
    pub const MELEE_WEAPON: u32 = 6;
    pub const PORTAL: u32 = 7;
    pub const BOOK: u32 = 8;
    pub const CREATURE: u32 = 10;
    pub const ADMIN: u32 = 11;
    pub const VENDOR: u32 = 12;
    pub const HOT_SPOT: u32 = 13;
    pub const COW: u32 = 15;
    pub const FOOD: u32 = 18;
    pub const DOOR: u32 = 19;
    pub const CHEST: u32 = 20;
    pub const CONTAINER: u32 = 21;
    pub const KEY: u32 = 22;
    pub const PRESSURE_PLATE: u32 = 24;
    pub const SWITCH: u32 = 26;
    pub const HEALER: u32 = 28;
    pub const SCROLL: u32 = 34;
    pub const CASTER: u32 = 35;
    pub const GEM: u32 = 38;
    pub const SENTINEL: u32 = 41;
    pub const HOUSE: u32 = 53;
    pub const DEED: u32 = 54;
    pub const SLUM_LORD: u32 = 55;
    pub const HOOK: u32 = 56;
    pub const STORAGE: u32 = 57;
    pub const BOOT_SPOT: u32 = 58;
    pub const HOUSE_PORTAL: u32 = 59;
    pub const GAME: u32 = 60;
    pub const GAME_PIECE: u32 = 61;
    pub const HOOKER: u32 = 64;
    pub const PET: u32 = 69;
    pub const COMBAT_PET: u32 = 71;
}

/// The item type bits a kind is told apart by.
mod it {
    pub const ARMOR: u32 = 0x2;
    pub const JEWELRY: u32 = 0x8;
    pub const LIFE_STONE: u32 = 0x1000_0000;
}

impl Kind {
    pub const ALL: [Self; 26] = [
        Self::Creature,
        Self::Npc,
        Self::Vendor,
        Self::MeleeWeapon,
        Self::MissileWeapon,
        Self::Ammo,
        Self::Caster,
        Self::Armor,
        Self::Clothing,
        Self::Jewelry,
        Self::Portal,
        Self::Door,
        Self::Chest,
        Self::Container,
        Self::Generator,
        Self::Switch,
        Self::PressurePlate,
        Self::Hotspot,
        Self::Food,
        Self::Gem,
        Self::Key,
        Self::Healer,
        Self::Scroll,
        Self::Book,
        Self::House,
        Self::Item,
    ];

    /// Its name's key in the text, `kinds.<key>`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Creature => "creature",
            Self::Npc => "npc",
            Self::Vendor => "vendor",
            Self::MeleeWeapon => "melee_weapon",
            Self::MissileWeapon => "missile_weapon",
            Self::Ammo => "ammo",
            Self::Caster => "caster",
            Self::Armor => "armor",
            Self::Clothing => "clothing",
            Self::Jewelry => "jewelry",
            Self::Portal => "portal",
            Self::Door => "door",
            Self::Chest => "chest",
            Self::Container => "container",
            Self::Generator => "generator",
            Self::Switch => "switch",
            Self::PressurePlate => "pressure_plate",
            Self::Hotspot => "hotspot",
            Self::Food => "food",
            Self::Gem => "gem",
            Self::Key => "key",
            Self::Healer => "healer",
            Self::Scroll => "scroll",
            Self::Book => "book",
            Self::House => "house",
            Self::Item => "item",
        }
    }

    /// What a weenie of weenie type `ty` and item type `item` is: `attackable` as it sets it
    /// (`None` unset), `generates` whether it has generator rows or a most-at-once.
    #[must_use]
    pub fn of_parts(ty: Option<u32>, item: u32, attackable: Option<bool>, generates: bool) -> Self {
        match ty.unwrap_or(0) {
            wt::VENDOR => Self::Vendor,
            wt::CREATURE
            | wt::COW
            | wt::PET
            | wt::COMBAT_PET
            | wt::ADMIN
            | wt::SENTINEL
            | wt::GAME
            | wt::GAME_PIECE => {
                if attackable == Some(false) {
                    Self::Npc
                } else {
                    Self::Creature
                }
            }
            wt::CLOTHING if item & it::ARMOR != 0 => Self::Armor,
            wt::CLOTHING if item & it::JEWELRY != 0 => Self::Jewelry,
            wt::CLOTHING => Self::Clothing,
            wt::MELEE_WEAPON => Self::MeleeWeapon,
            wt::MISSILE_LAUNCHER => Self::MissileWeapon,
            wt::MISSILE | wt::AMMUNITION => Self::Ammo,
            wt::CASTER => Self::Caster,
            wt::PORTAL | wt::HOUSE_PORTAL => Self::Portal,
            wt::DOOR => Self::Door,
            wt::CHEST => Self::Chest,
            wt::CONTAINER | wt::STORAGE => Self::Container,
            wt::SWITCH => Self::Switch,
            wt::PRESSURE_PLATE => Self::PressurePlate,
            wt::HOT_SPOT => Self::Hotspot,
            wt::FOOD => Self::Food,
            wt::GEM => Self::Gem,
            wt::KEY => Self::Key,
            wt::HEALER => Self::Healer,
            wt::SCROLL => Self::Scroll,
            wt::BOOK => Self::Book,
            wt::HOUSE | wt::DEED | wt::SLUM_LORD | wt::HOOK | wt::HOOKER | wt::BOOT_SPOT => {
                Self::House
            }
            wt::GENERIC if generates && (item == 0 || item == it::LIFE_STONE) => Self::Generator,
            wt::GENERIC if item & it::JEWELRY != 0 => Self::Jewelry,
            _ => Self::Item,
        }
    }

    /// What the weenie of `doc` is.
    #[must_use]
    pub fn of(doc: &SqlDoc) -> Self {
        let ty = doc
            .rows("weenie")
            .first()
            .and_then(|r| doc.cell(*r, "type"))
            .and_then(|v| lyceum_foundry::sql_ids::number(&v));
        let item = doc
            .property(PropertyKind::Int, 1)
            .and_then(|v| content::bare(&v).trim().parse::<i64>().ok())
            .map_or(0, int_bits);
        let attackable = doc
            .property(PropertyKind::Bool, 19)
            .map(|v| super::fields::yes_no(Some(&v), true));
        let generates = !doc.rows("weenie_properties_generator").is_empty()
            || doc.property(PropertyKind::Int, 81).is_some();
        Self::of_parts(ty, item, attackable, generates)
    }

    /// The sections its pane and its editor show, whether the weenie sets anything in them or
    /// not, in the order they are shown.
    #[must_use]
    pub const fn sections(self) -> &'static [Section] {
        use Section::{
            Activation, Appearance, Attributes, Basic, BodyParts, Creature, Emotes, Holds, Lock,
            Magic, Npc, Portal, Protection, Skills, Spawns, Trade, Weapon, Wearing,
        };
        match self {
            Self::Creature => &[
                Basic, Appearance, Creature, Attributes, Skills, BodyParts, Protection, Magic,
                Holds, Emotes,
            ],
            // An NPC's Creature is what it wields, and anything of a fight it sets.
            Self::Npc => &[Basic, Appearance, Creature, Npc, Emotes],
            Self::Vendor => &[Basic, Appearance, Npc, Trade, Emotes],
            Self::MeleeWeapon => &[Basic, Appearance, Wearing, Weapon, Magic],
            Self::MissileWeapon | Self::Caster => &[Basic, Wearing, Weapon, Magic],
            Self::Ammo => &[Basic, Wearing, Weapon],
            Self::Armor | Self::Clothing => &[Basic, Appearance, Wearing, Magic],
            Self::Jewelry => &[Basic, Wearing, Magic],
            Self::Portal => &[Basic, Portal],
            Self::Door => &[Basic, Lock],
            Self::Chest => &[Basic, Lock, Holds],
            Self::Container => &[Basic, Holds],
            Self::Generator => &[Basic, Spawns],
            Self::Switch | Self::PressurePlate | Self::Hotspot => &[Basic, Activation],
            Self::Food | Self::Key | Self::Healer | Self::Item => &[Basic, Activation],
            Self::Gem => &[Basic, Activation, Magic],
            Self::Scroll => &[Basic, Magic],
            Self::Book | Self::House => &[Basic],
        }
    }

    /// The section a field of section `s` is shown in for a weenie of this kind: protection
    /// with the equipment of a kind that is worn or wielded, else its own.
    #[must_use]
    pub fn home(self, s: Section) -> Section {
        match s {
            Section::Protection if self.sections().contains(&Section::Wearing) => Section::Wearing,
            _ => s,
        }
    }

    /// Whether a section it does not have shows when the weenie sets something in it: for every
    /// kind but a house's parts, whose data stays under every property.
    #[must_use]
    pub const fn shows_what_it_sets(self) -> bool {
        !matches!(self, Self::House)
    }

    /// Whether it is a creature, which draws its treasure by chance.
    #[must_use]
    pub const fn is_creature(self) -> bool {
        matches!(self, Self::Creature | Self::Npc | Self::Vendor)
    }
}

/// An int property's value as the bits it is written as (a negative number is its high bit set).
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a 32-bit property's bits
pub const fn int_bits(n: i64) -> u32 {
    n as i32 as u32
}

/// A section of a weenie's fields: what they are for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Section {
    /// What it is: its name, its looks, its size, its worth.
    Basic,
    Appearance,
    /// What it is in a fight, and what it wields.
    Creature,
    /// Its attributes and vitals.
    Attributes,
    Skills,
    /// Its body's parts: how each hits and is hit.
    BodyParts,
    Npc,
    /// What it buys and sells, and at what price.
    Trade,
    /// Where it is worn or wielded, what that needs, and for armor and clothing, how it
    /// protects.
    Wearing,
    Weapon,
    /// How a creature is protected.
    Protection,
    /// What it does when used, stepped on or switched.
    Activation,
    Magic,
    Portal,
    Lock,
    /// What it holds and how much, and what it drops when it dies.
    Holds,
    /// What it spawns, how many and how often.
    Spawns,
    Emotes,
}

impl Section {
    /// Every section, in the order a weenie's are shown.
    pub const ALL: [Self; 18] = [
        Self::Basic,
        Self::Appearance,
        Self::Creature,
        Self::Attributes,
        Self::Skills,
        Self::BodyParts,
        Self::Npc,
        Self::Trade,
        Self::Wearing,
        Self::Weapon,
        Self::Protection,
        Self::Activation,
        Self::Magic,
        Self::Portal,
        Self::Lock,
        Self::Holds,
        Self::Spawns,
        Self::Emotes,
    ];

    /// Its key in the text, `sections.<key>`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Basic => "basic",
            Self::Appearance => "appearance",
            Self::Creature => "creature",
            Self::Attributes => "attributes",
            Self::Skills => "skills",
            Self::BodyParts => "body_parts",
            Self::Npc => "npc",
            Self::Trade => "trade",
            Self::Wearing => "wearing",
            Self::Weapon => "weapon",
            Self::Protection => "protection",
            Self::Activation => "activation",
            Self::Magic => "magic",
            Self::Portal => "portal",
            Self::Lock => "lock",
            Self::Holds => "holds",
            Self::Spawns => "spawns",
            Self::Emotes => "emotes",
        }
    }

    /// Its name's key in the text.
    #[must_use]
    pub fn name_key(self) -> String {
        format!("sections.{}", self.key())
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's weenie kinds)
    use super::*;

    /// A weenie of type `ty` with `rows` (`(table, type, value)` property rows) after it.
    fn weenie(ty: u32, rows: &[(&str, u32, &str)]) -> SqlDoc {
        let mut text = format!(
            "INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)\nVALUES (9, 'thing', {ty}, '2005-02-09 10:00:00');\n"
        );
        for (table, id, value) in rows {
            text.push_str(&format!(
                "INSERT INTO `{table}` (`object_Id`, `type`, `value`)\nVALUES (9, {id}, {value});\n"
            ));
        }
        SqlDoc::parse(&text)
    }

    const INT: &str = "weenie_properties_int";
    const BOOL: &str = "weenie_properties_bool";

    #[test]
    fn a_weenies_kind_is_read_from_its_type_its_item_type_and_what_it_sets() {
        let cases: &[(SqlDoc, Kind)] = &[
            (weenie(10, &[]), Kind::Creature),
            (weenie(10, &[(BOOL, 19, "True")]), Kind::Creature),
            (weenie(10, &[(BOOL, 19, "False")]), Kind::Npc),
            (weenie(15, &[(BOOL, 19, "0")]), Kind::Npc),
            (weenie(12, &[(BOOL, 19, "False")]), Kind::Vendor),
            (weenie(6, &[(INT, 1, "1")]), Kind::MeleeWeapon),
            (weenie(3, &[(INT, 1, "256")]), Kind::MissileWeapon),
            (weenie(5, &[]), Kind::Ammo),
            (weenie(35, &[(INT, 1, "32768")]), Kind::Caster),
            (weenie(2, &[(INT, 1, "2")]), Kind::Armor),
            (weenie(2, &[(INT, 1, "4")]), Kind::Clothing),
            (weenie(2, &[(INT, 1, "8")]), Kind::Jewelry),
            (weenie(1, &[(INT, 1, "8")]), Kind::Jewelry),
            (weenie(7, &[]), Kind::Portal),
            (weenie(59, &[]), Kind::Portal),
            (weenie(19, &[]), Kind::Door),
            (weenie(20, &[(INT, 81, "3")]), Kind::Chest),
            (weenie(21, &[]), Kind::Container),
            (weenie(1, &[(INT, 81, "4")]), Kind::Generator),
            (weenie(1, &[(INT, 1, "128"), (INT, 81, "4")]), Kind::Item),
            (weenie(1, &[(INT, 1, "128")]), Kind::Item),
            (weenie(1, &[]), Kind::Item),
            (weenie(26, &[]), Kind::Switch),
            (weenie(24, &[]), Kind::PressurePlate),
            (weenie(13, &[]), Kind::Hotspot),
            (weenie(18, &[]), Kind::Food),
            (weenie(38, &[]), Kind::Gem),
            (weenie(22, &[]), Kind::Key),
            (weenie(28, &[]), Kind::Healer),
            (weenie(34, &[]), Kind::Scroll),
            (weenie(8, &[]), Kind::Book),
            (weenie(56, &[]), Kind::House),
            (weenie(55, &[]), Kind::House),
            (weenie(44, &[]), Kind::Item),
        ];
        for (doc, want) in cases {
            assert_eq!(Kind::of(doc), *want, "{}", doc.render());
        }
        // A generic weenie with generator rows and no item type spawns things.
        let camp = SqlDoc::parse(
            "INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)\nVALUES (9, 'camp', 1, '2005-02-09 10:00:00');\nINSERT INTO `weenie_properties_generator` (`object_Id`, `probability`, `weenie_Class_Id`, `delay`, `init_Create`, `max_Create`, `when_Create`, `where_Create`, `stack_Size`, `palette_Id`, `shade`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`)\nVALUES (9, -1, 7, 180, 1, 1, 2, 4, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL);\n",
        );
        assert_eq!(Kind::of(&camp), Kind::Generator);
        // A high item type bit, written as the negative number it is stored as.
        assert_eq!(int_bits(-2_147_483_648), 0x8000_0000);
        assert_eq!(int_bits(-1), u32::MAX);
    }

    #[test]
    fn each_kinds_sections_are_in_the_order_every_weenies_are_shown_and_each_once() {
        for kind in Kind::ALL {
            let at: Vec<usize> = kind
                .sections()
                .iter()
                .map(|s| Section::ALL.iter().position(|a| a == s).unwrap())
                .collect();
            assert!(at.windows(2).all(|w| w[0] < w[1]), "{kind:?}: {at:?}");
            assert_eq!(kind.sections().first(), Some(&Section::Basic), "{kind:?}");
        }
    }

    #[test]
    fn every_kind_and_section_has_a_name_in_the_text() {
        let t = crate::text::read(crate::text::BUILT_IN).unwrap();
        for kind in Kind::ALL {
            let k = format!("kinds.{}", kind.key());
            assert!(t.contains_key(&k), "{k}");
            for s in Section::ALL {
                let k = s.name_key();
                assert!(t.contains_key(&k), "{k}");
            }
        }
    }
}
