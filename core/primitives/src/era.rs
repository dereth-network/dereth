//! The eras a world can play, and the later systems each has.
//!
//! A server names its era and the systems its world plays: the era's table is their default, which
//! the server's configuration may change system by system. The client is told the full set, so a
//! front end can hide a panel the world has no state for (ratings, aetheria, luminance, ...)
//! before any state arrives. Every era speaks the end-of-retail client protocol: an era changes data and rules,
//! never the wire.

/// An era, in the order of retail history, so "at or before" compares as the ordinal does.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EraId {
    /// February 2005: the Infiltration patch, before Throne of Destiny.
    Infiltration,
    /// The end of retail.
    #[default]
    Eor,
}

impl EraId {
    /// Every era, in order.
    pub const ALL: [Self; 2] = [Self::Infiltration, Self::Eor];

    /// The era's name as configuration files, command lines and status documents spell it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Infiltration => "infiltration",
            Self::Eor => "eor",
        }
    }

    /// The era named `name` (as [`Self::name`] spells it, any case, surrounding space ignored).
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|e| e.name().eq_ignore_ascii_case(name.trim()))
    }

    /// The later systems the era has.
    #[must_use]
    pub const fn features(self) -> EraFeatures {
        match self {
            Self::Infiltration => EraFeatures::INFILTRATION,
            Self::Eor => EraFeatures::END_OF_RETAIL,
        }
    }

    /// The dat set the era's world is drawn from: the files from before Throne of Destiny
    /// (`portal.dat`, `cell.dat`) for an era before it, the later `client_*.dat` files otherwise.
    #[must_use]
    pub const fn container_era(self) -> crate::ContainerEra {
        match self {
            Self::Infiltration => crate::ContainerEra::Classic,
            Self::Eor => crate::ContainerEra::Modern,
        }
    }

    /// How much experience wins back a point of vitae in the era.
    #[must_use]
    pub const fn vitae_recovery(self) -> VitaeRecovery {
        match self {
            Self::Infiltration => VitaeRecovery::BeforeThroneOfDestiny,
            Self::Eor => VitaeRecovery::EndOfRetail,
        }
    }
}

/// The experience needed to win back one point (1%) of vitae.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VitaeRecovery {
    /// `(level^2.5 * 2.5 + 20) * vitae^5 + 0.5`.
    EndOfRetail,
    /// `(level^2 * 5 + 20) * vitae^5 + 0.5`, at most 12,500: the formula from launch, and the cap
    /// Throne of Destiny's level-cap update announced (it says the formula had not changed since
    /// 1999).
    BeforeThroneOfDestiny,
}

impl core::fmt::Display for EraId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.name())
    }
}

/// Declares [`EraFeatures`]: one `bool` per system, with its name as configuration files, command
/// lines and status documents spell it (the field's own name).
macro_rules! era_features {
    ($( $(#[doc = $doc:literal])* $name:ident, )*) => {
        /// The later systems an era has or lacks. The era's table ([`EraId::features`]) is only the
        /// default: a server may turn any of them on or off for its world, and announces the set it
        /// plays to the client ([`EraFeatureOverrides`]).
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[allow(clippy::struct_excessive_bools)]
        pub struct EraFeatures {
            $( $(#[doc = $doc])* pub $name: bool, )*
        }

        impl EraFeatures {
            /// How many systems there are.
            pub const COUNT: usize = [$(stringify!($name)),*].len();
            /// Every system's name, in declaration order.
            pub const NAMES: [&'static str; Self::COUNT] = [$(stringify!($name)),*];
            /// Every system.
            pub const ALL: Self = Self { $($name: true,)* };
            /// None of them.
            pub const NONE: Self = Self { $($name: false,)* };

            /// Whether the system named `name` is on; `None` for a name that is no system's.
            #[must_use]
            pub fn get(&self, name: &str) -> Option<bool> {
                match name {
                    $(stringify!($name) => Some(self.$name),)*
                    _ => None,
                }
            }

            /// Turns the system named `name` on or off. Answers whether `name` is a system's.
            pub fn set(&mut self, name: &str, on: bool) -> bool {
                match name {
                    $(stringify!($name) => { self.$name = on; true })*
                    _ => false,
                }
            }

            /// Every system's name and whether it is on, in declaration order.
            pub fn iter(&self) -> impl Iterator<Item = (&'static str, bool)> {
                [$((stringify!($name), self.$name)),*].into_iter()
            }
        }
    };
}

era_features! {
    /// Damage-resistance, critical, healing, life, damage-over-time and nether ratings (and
    /// Recklessness and Sneak Attack). Without them every such rating is 0 and its modifier 1.
    ratings,
    /// The 2012 consolidation of the weapon skills: a player's old weapon skill (Axe, Sword, Bow,
    /// ...) is read as Heavy, Light, Finesse or Missile Weapons. Without it the old skill is used.
    consolidated_weapon_skills,
    /// The weapon item spells that became auras on the wielder. Without it they are item spells.
    item_spell_auras,
    /// Assessing a player or an unattackable creature shows its armour levels and ratings.
    assessed_armor_and_ratings,
    /// A character may swear allegiance to a patron of lower level.
    swear_to_lower_level,
    /// The pre-order gifts handed out at login, and rare items dropped by creatures.
    pre_order_items_and_rares,
    /// Wielding a second weapon in the off hand.
    dual_wield,
    /// The heritage weapon masteries: a bonus with the heritage's weapon types.
    weapon_masteries,
    /// The augmentation each heritage is born with (Jack of All Trades for the three original
    /// heritages, ...).
    innate_augmentations,
    /// Aetheria and its sigil slots.
    aetheria,
    /// Luminance and the luminance auras.
    luminance,
    /// The contract tracker: the quests a character has taken on, with their timers.
    contracts,
    /// Character titles: the titles a character earns and the one it displays.
    titles,
    /// Cloaks, and the cloak slot they are worn in.
    cloaks,
    /// The trinket slot.
    trinkets,
    /// The journal: the quest page, its notebook and the toolbar button that opens it.
    journal,
    /// Secure trade between players: the trade window and its button.
    trade,
    /// Houses: cottages, villas and mansions, bought and rented from their deeds.
    housing,
    /// Apartments, and with them the house recalls and allegiance storage. An era with apartments
    /// has housing ([`EraFeatures::implied`]).
    apartments,
    /// Tinkering: the tinkering skills (in place of the appraisal skills), salvaging into salvage
    /// bags, and the tinkering recipes that apply salvage to items.
    tinkering,
    /// Cantrips on generated loot.
    cantrips,
    /// Learning a spell by researching its formula, as the early clients' research page did: a
    /// formula of carried components tested on a target in magic mode, which casts the spell it
    /// makes and teaches it if it is new. It was gone by the middle of 2002, so no era here has it
    /// unless the world turns it on. A character's own spell formulas (the components each spell
    /// takes for its account) are not this system: every era keeps them.
    spell_research,
    /// Chess on the game boards.
    chess,
    /// An oath of allegiance costs unassigned experience once a character has broken from a
    /// patron: five percent of its next level's experience, held between 100 and 5,000, a quarter
    /// more for each break. Throne of Destiny removed it, so the end of retail does not have it.
    swear_xp_cost,
}

impl EraFeatures {
    /// The end of retail: every system but spell research and the oath's experience cost.
    pub const END_OF_RETAIL: Self = Self {
        spell_research: false,
        swear_xp_cost: false,
        ..Self::ALL
    };

    /// February 2005: none of the systems from ratings to the journal, nor spell research; trade,
    /// housing, apartments, tinkering, cantrips, chess and the oath's experience cost, which that
    /// world had.
    pub const INFILTRATION: Self = Self {
        trade: true,
        housing: true,
        apartments: true,
        tinkering: true,
        cantrips: true,
        chess: true,
        swear_xp_cost: true,
        ..Self::NONE
    };

    /// The set with the systems one implies turned on: apartments imply housing.
    #[must_use]
    pub const fn implied(mut self) -> Self {
        if self.apartments {
            self.housing = true;
        }
        self
    }
}

impl Default for EraFeatures {
    fn default() -> Self {
        EraId::default().features()
    }
}

impl EraFeatures {
    /// The version of the systems' table, as [`EraFeatureBits`] carries it. Systems are only ever
    /// appended to the table, never moved or removed, so each version's systems are the first
    /// ones of every later version's; the version goes up by one whenever one is appended.
    pub const TABLE_VERSION: u16 = 1;

    /// How many systems each version of the table has, version 1 first.
    pub const COUNT_BY_VERSION: [usize; Self::TABLE_VERSION as usize] = [24];

    /// How many systems version `version` of the table has, as far as this build can say: its own
    /// count for a version it knows, every system it has for a later one (whose first systems are
    /// these), none for version 0.
    #[must_use]
    pub fn count_at_version(version: u16) -> usize {
        match version {
            0 => 0,
            v if v > Self::TABLE_VERSION => Self::COUNT,
            v => Self::COUNT_BY_VERSION[usize::from(v) - 1],
        }
    }
}

/// A world's full set of systems as a bitfield: bit `i` (byte `i / 8`, bit `i % 8`, lowest
/// first) is the `i`-th system of [`EraFeatures::NAMES`], with the version of the table it was
/// written against. The server's status reply, the launcher and the client's `--era-features`
/// all carry it in this form.
///
/// A reader takes only the bits its own table and the writer's both name: a system the writer's
/// table does not have (an older writer), or whose bit the writer did not send, is unknown, never
/// off, and a bit past the reader's table (a newer writer) is skipped.
///
/// Text form: the table version in decimal, a colon, and the bytes in hex, first byte first
/// (`1:ffff5f` is the end of retail).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EraFeatureBits {
    /// The version of the table the bits were written against.
    pub table_version: u16,
    /// The bits, lowest system first.
    pub bytes: Vec<u8>,
}

impl EraFeatureBits {
    /// The most bytes a bitfield is read with: room for 2,040 systems.
    pub const MAX_BYTES: usize = 255;

    /// `features` against this build's table.
    #[must_use]
    pub fn of(features: EraFeatures) -> Self {
        let mut bytes = vec![0u8; EraFeatures::COUNT.div_ceil(8)];
        for (i, (_, on)) in features.iter().enumerate() {
            if on {
                bytes[i / 8] |= 1 << (i % 8);
            }
        }
        Self {
            table_version: EraFeatures::TABLE_VERSION,
            bytes,
        }
    }

    /// The bit of the `i`-th system, `None` when it is unknown: past the writer's table, or past
    /// the bytes sent.
    #[must_use]
    pub fn bit(&self, i: usize) -> Option<bool> {
        if i >= EraFeatures::count_at_version(self.table_version) {
            return None;
        }
        let byte = self.bytes.get(i / 8)?;
        Some(byte & (1 << (i % 8)) != 0)
    }

    /// Every system whose bit is known, set to it; the rest unset, so the era's table supplies
    /// them ([`EraFeatureOverrides::apply`]).
    #[must_use]
    pub fn overrides(&self) -> EraFeatureOverrides {
        let mut o = EraFeatureOverrides::default();
        for (i, name) in EraFeatures::NAMES.iter().enumerate() {
            if let Some(on) = self.bit(i) {
                o.set(name, on);
            }
        }
        o
    }

    /// Reads the text form.
    ///
    /// # Errors
    /// No colon, a version that is not a number from 1 to 65,535, or hex that is not whole bytes,
    /// or more than [`Self::MAX_BYTES`] of them.
    pub fn parse(text: &str) -> Result<Self, String> {
        let (version, hex) = text
            .trim()
            .split_once(':')
            .ok_or_else(|| format!("{text:?} is not <table version>:<hex>"))?;
        let table_version = version
            .trim()
            .parse::<u16>()
            .ok()
            .filter(|v| *v > 0)
            .ok_or_else(|| format!("{text:?}: the table version is not a number from 1"))?;
        let hex = hex.trim();
        if hex.len() % 2 != 0 || hex.len() / 2 > Self::MAX_BYTES {
            return Err(format!("{text:?}: the bits are not whole hex bytes"));
        }
        let bytes = (0..hex.len())
            .step_by(2)
            .map(|i| {
                hex.get(i..i + 2)
                    .and_then(|b| u8::from_str_radix(b, 16).ok())
            })
            .collect::<Option<Vec<u8>>>()
            .ok_or_else(|| format!("{text:?}: the bits are not hex"))?;
        Ok(Self {
            table_version,
            bytes,
        })
    }
}

impl core::fmt::Display for EraFeatureBits {
    /// The text form [`EraFeatureBits::parse`] reads.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}:", self.table_version)?;
        for b in &self.bytes {
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

/// The systems a world turns on or off over its era's table: each value set replaces the table's.
/// A server's configuration gives them, and the client is told the world's full set in the same
/// form.
///
/// Text form: `name=true,name=false,...` (the names of [`EraFeatures::NAMES`]; `on`/`off` and
/// `1`/`0` also read). A name this build does not know is skipped, so a client older than its
/// server reads the systems it knows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EraFeatureOverrides {
    values: [Option<bool>; EraFeatures::COUNT],
}

impl EraFeatureOverrides {
    /// Every system set to its value in `features`: the full set a server announces.
    #[must_use]
    pub fn all_of(features: EraFeatures) -> Self {
        let mut o = Self::default();
        for (name, on) in features.iter() {
            o.set(name, on);
        }
        o
    }

    /// Sets the system named `name`. Answers whether `name` is a system's.
    pub fn set(&mut self, name: &str, on: bool) -> bool {
        match EraFeatures::NAMES.iter().position(|n| *n == name) {
            Some(i) => {
                self.values[i] = Some(on);
                true
            }
            None => false,
        }
    }

    /// The value set for the system named `name`, if any.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<bool> {
        let i = EraFeatures::NAMES.iter().position(|n| *n == name)?;
        self.values[i]
    }

    /// Whether nothing is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.iter().all(Option::is_none)
    }

    /// Each system set, by name, in declaration order.
    pub fn iter(&self) -> impl Iterator<Item = (&'static str, bool)> + '_ {
        EraFeatures::NAMES
            .iter()
            .zip(self.values)
            .filter_map(|(n, v)| v.map(|v| (*n, v)))
    }

    /// `base` with these values over it. Housing turned off without a word about apartments
    /// turns apartments off too; then apartments imply housing ([`EraFeatures::implied`]).
    #[must_use]
    pub fn apply(&self, base: EraFeatures) -> EraFeatures {
        let mut f = base;
        for (name, on) in self.iter() {
            f.set(name, on);
        }
        if self.get("housing") == Some(false) && self.get("apartments").is_none() {
            f.apartments = false;
        }
        f.implied()
    }

    /// Reads the text form. Answers the values and the names it skipped as no system's.
    ///
    /// # Errors
    /// An entry that is not `name=value`, or a value that is not `true`/`false` (`on`/`off`,
    /// `1`/`0`).
    pub fn parse(text: &str) -> Result<(Self, Vec<String>), String> {
        let mut o = Self::default();
        let mut unknown = Vec::new();
        for entry in text.split(',').map(str::trim).filter(|e| !e.is_empty()) {
            let (name, value) = entry
                .split_once('=')
                .ok_or_else(|| format!("{entry:?} is not name=true or name=false"))?;
            let on = match value.trim().to_ascii_lowercase().as_str() {
                "true" | "on" | "1" => true,
                "false" | "off" | "0" => false,
                _ => return Err(format!("{entry:?}: the value is not true or false")),
            };
            let name = name.trim().to_ascii_lowercase();
            if !o.set(&name, on) {
                unknown.push(name);
            }
        }
        Ok((o, unknown))
    }
}

impl core::fmt::Display for EraFeatureOverrides {
    /// The text form [`EraFeatureOverrides::parse`] reads.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for (i, (name, on)) in self.iter().enumerate() {
            if i > 0 {
                f.write_str(",")?;
            }
            write!(f, "{name}={on}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_era_round_trips_its_name_and_the_end_of_retail_has_every_system_but_two_early_ones() {
        for e in EraId::ALL {
            assert_eq!(EraId::parse(e.name()), Some(e));
        }
        assert_eq!(EraId::parse(" INFILTRATION "), Some(EraId::Infiltration));
        assert_eq!(EraId::parse("tod"), None);
        assert!(EraId::Infiltration < EraId::Eor);
        for (name, on) in EraId::Eor.features().iter() {
            assert_eq!(
                on,
                !["spell_research", "swear_xp_cost"].contains(&name),
                "{name}"
            );
        }
        assert_eq!(EraFeatures::default(), EraId::Eor.features());
    }

    /// February 2005 has trade, housing, apartments, tinkering, cantrips, chess and the oath's
    /// experience cost, and neither spell research nor any of the later systems.
    #[test]
    fn infiltration_has_the_systems_february_2005_had_and_none_of_the_later_ones() {
        let f = EraId::Infiltration.features();
        let on = [
            "trade",
            "housing",
            "apartments",
            "tinkering",
            "cantrips",
            "chess",
            "swear_xp_cost",
        ];
        for (name, value) in f.iter() {
            assert_eq!(value, on.contains(&name), "{name}");
        }
        assert_eq!(EraFeatures::NAMES.len(), 24);
        assert_eq!(f, f.implied());
    }

    #[test]
    fn apartments_imply_housing() {
        let f = EraFeatures {
            apartments: true,
            ..EraFeatures::NONE
        };
        assert!(f.implied().housing);
        let f = EraFeatures {
            housing: true,
            ..EraFeatures::NONE
        };
        assert!(!f.implied().apartments, "housing alone has no apartments");
    }

    #[test]
    fn overrides_replace_the_eras_values_and_housing_off_takes_apartments_with_it() {
        let (o, unknown) =
            EraFeatureOverrides::parse("aetheria=on, trade=false,housing=0").expect("parses");
        assert!(unknown.is_empty());
        let f = o.apply(EraFeatures::ALL);
        assert!(f.aetheria && !f.trade && !f.housing && !f.apartments);
        // Apartments named on keep housing on.
        let (o, _) = EraFeatureOverrides::parse("housing=false,apartments=true").expect("parses");
        let f = o.apply(EraFeatures::NONE);
        assert!(f.housing && f.apartments);
        // Apartments off leave housing alone.
        let (o, _) = EraFeatureOverrides::parse("apartments=false").expect("parses");
        let f = o.apply(EraFeatures::ALL);
        assert!(f.housing && !f.apartments);
        assert_eq!(
            EraFeatureOverrides::default().apply(EraFeatures::INFILTRATION),
            EraFeatures::INFILTRATION
        );
    }

    #[test]
    fn the_text_form_round_trips_and_skips_names_it_does_not_know() {
        let all = EraFeatureOverrides::all_of(EraFeatures::INFILTRATION);
        let text = all.to_string();
        assert!(text.starts_with("ratings=false,"), "{text}");
        assert!(text.ends_with(",chess=true,swear_xp_cost=true"), "{text}");
        assert_eq!(
            EraFeatureOverrides::parse(&text).expect("parses"),
            (all, Vec::new())
        );
        assert_eq!(all.apply(EraFeatures::ALL), EraFeatures::INFILTRATION);
        let (o, unknown) =
            EraFeatureOverrides::parse("Chess=false,spell_credits=true").expect("parses");
        assert_eq!(o.get("chess"), Some(false));
        assert_eq!(unknown, ["spell_credits"]);
        assert!(EraFeatureOverrides::parse("chess").is_err());
        assert!(EraFeatureOverrides::parse("chess=maybe").is_err());
        assert!(EraFeatureOverrides::parse("").expect("parses").0.is_empty());
    }

    /// The first table's systems, in order. A system is only ever appended: this list never
    /// changes, and a new system bumps [`EraFeatures::TABLE_VERSION`].
    #[test]
    fn the_first_tables_systems_keep_their_bits_and_a_new_one_bumps_the_version() {
        const VERSION_1: [&str; 24] = [
            "ratings",
            "consolidated_weapon_skills",
            "item_spell_auras",
            "assessed_armor_and_ratings",
            "swear_to_lower_level",
            "pre_order_items_and_rares",
            "dual_wield",
            "weapon_masteries",
            "innate_augmentations",
            "aetheria",
            "luminance",
            "contracts",
            "titles",
            "cloaks",
            "trinkets",
            "journal",
            "trade",
            "housing",
            "apartments",
            "tinkering",
            "cantrips",
            "spell_research",
            "chess",
            "swear_xp_cost",
        ];
        assert_eq!(EraFeatures::NAMES[..VERSION_1.len()], VERSION_1);
        assert_eq!(
            EraFeatures::COUNT_BY_VERSION.last().copied(),
            Some(EraFeatures::COUNT),
            "the latest version counts every system"
        );
        assert!(EraFeatures::COUNT_BY_VERSION
            .windows(2)
            .all(|w| w[0] < w[1]));
    }

    #[test]
    fn the_bitfield_round_trips_every_era_and_its_text() {
        for era in EraId::ALL {
            let f = era.features();
            let bits = EraFeatureBits::of(f);
            assert_eq!(bits.table_version, EraFeatures::TABLE_VERSION);
            assert_eq!(bits.bytes.len(), EraFeatures::COUNT.div_ceil(8));
            assert_eq!(bits.overrides().apply(EraFeatures::NONE), f, "{era}");
            assert_eq!(bits.overrides().apply(EraFeatures::ALL), f, "{era}");
            let text = bits.to_string();
            assert_eq!(EraFeatureBits::parse(&text), Ok(bits), "{text}");
        }
        assert_eq!(
            EraFeatureBits::of(EraFeatures::END_OF_RETAIL).to_string(),
            "1:ffff5f"
        );
        assert_eq!(
            EraFeatureBits::of(EraFeatures::INFILTRATION).to_string(),
            "1:0000df"
        );
    }

    /// A newer table's bits past this build's are skipped; an older table's missing systems,
    /// and bits not sent, are unknown and left to the era's table.
    #[test]
    fn unknown_bits_are_unknown_never_off() {
        // A newer writer: two more systems, both on, in a fourth byte.
        let newer = EraFeatureBits {
            table_version: EraFeatures::TABLE_VERSION + 1,
            bytes: vec![0xff, 0xff, 0x5f, 0x03],
        };
        assert_eq!(
            newer.overrides().apply(EraFeatures::NONE),
            EraFeatures::END_OF_RETAIL
        );
        assert_eq!(newer.bit(EraFeatures::COUNT), None);

        // Bits not sent: the systems past the bytes keep the table's value.
        let short = EraFeatureBits {
            table_version: EraFeatures::TABLE_VERSION,
            bytes: vec![0x00],
        };
        let f = short.overrides().apply(EraFeatures::ALL);
        assert!(!f.ratings && !f.weapon_masteries, "the first byte is read");
        assert!(f.innate_augmentations, "the ninth system is past it");
        assert!(f.aetheria && f.trade && f.chess, "the rest are unknown");

        // Version 0 names no table: nothing is known.
        let none = EraFeatureBits {
            table_version: 0,
            bytes: vec![0; 3],
        };
        assert!(none.overrides().is_empty());
        assert_eq!(EraFeatures::count_at_version(0), 0);
        assert_eq!(EraFeatures::count_at_version(1), 24);
        assert_eq!(EraFeatures::count_at_version(u16::MAX), EraFeatures::COUNT);
    }

    #[test]
    fn the_bitfield_text_refuses_what_is_not_one() {
        for bad in ["ffff5f", "0:ffff5f", "x:ff", "1:fff", "1:zz", "-1:ff"] {
            assert!(EraFeatureBits::parse(bad).is_err(), "{bad}");
        }
        assert_eq!(
            EraFeatureBits::parse(" 1:FFff5F "),
            Ok(EraFeatureBits::of(EraFeatures::END_OF_RETAIL))
        );
        assert_eq!(
            EraFeatureBits::parse("2:"),
            Ok(EraFeatureBits {
                table_version: 2,
                bytes: Vec::new()
            })
        );
    }
}
