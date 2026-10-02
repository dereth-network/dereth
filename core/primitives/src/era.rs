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
}

impl EraFeatures {
    /// The end of retail: every system but spell research.
    pub const END_OF_RETAIL: Self = Self {
        spell_research: false,
        ..Self::ALL
    };

    /// February 2005: none of the systems from ratings to the journal, nor spell research; trade,
    /// housing, apartments, tinkering, cantrips and chess, which that world had.
    pub const INFILTRATION: Self = Self {
        trade: true,
        housing: true,
        apartments: true,
        tinkering: true,
        cantrips: true,
        chess: true,
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
    fn every_era_round_trips_its_name_and_the_end_of_retail_has_every_system_but_research() {
        for e in EraId::ALL {
            assert_eq!(EraId::parse(e.name()), Some(e));
        }
        assert_eq!(EraId::parse(" INFILTRATION "), Some(EraId::Infiltration));
        assert_eq!(EraId::parse("tod"), None);
        assert!(EraId::Infiltration < EraId::Eor);
        for (name, on) in EraId::Eor.features().iter() {
            assert_eq!(on, name != "spell_research", "{name}");
        }
        assert_eq!(EraFeatures::default(), EraId::Eor.features());
    }

    /// February 2005 has trade, housing, apartments, tinkering, cantrips and chess, and neither
    /// spell research nor any of the later systems.
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
        ];
        for (name, value) in f.iter() {
            assert_eq!(value, on.contains(&name), "{name}");
        }
        assert_eq!(EraFeatures::NAMES.len(), 23);
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
        assert!(text.ends_with(",chess=true"), "{text}");
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
}
