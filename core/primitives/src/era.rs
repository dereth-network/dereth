//! The eras a world can play, and the later systems each has.
//!
//! A server names its era and the systems its world plays: the era's table is their default, which
//! the server's configuration may change system by system. The client is told the full set, so a
//! front end can hide a panel the world has no state for (ratings, aetheria, luminance, ...)
//! before any state arrives. Gameplay era changes data and rules; the connection selects its
//! wire dialect separately.

/// An era, in the order of retail history, so "at or before" compares as the ordinal does.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EraId {
    /// November 1999: the experimental launch profile.
    Launch,
    /// February 2005: the Infiltration patch, before Throne of Destiny.
    Infiltration,
    /// The end of retail.
    #[default]
    Eor,
}

impl EraId {
    /// Every era, in order.
    pub const ALL: [Self; 3] = [Self::Launch, Self::Infiltration, Self::Eor];

    /// The era's name as configuration files, command lines and status documents spell it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Launch => "launch",
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
            Self::Launch => EraFeatures::LAUNCH,
            Self::Infiltration => EraFeatures::INFILTRATION,
            Self::Eor => EraFeatures::END_OF_RETAIL,
        }
    }

    /// The dat set the era's world is drawn from: the files from before Throne of Destiny
    /// (`portal.dat`, `cell.dat`) for an era before it, the later `client_*.dat` files otherwise.
    #[must_use]
    pub const fn container_era(self) -> crate::ContainerEra {
        match self {
            Self::Launch | Self::Infiltration => crate::ContainerEra::Classic,
            Self::Eor => crate::ContainerEra::Modern,
        }
    }

    /// The data files the era's world is drawn from, as the files' own iterations tell them
    /// apart from other years' files.
    #[must_use]
    pub const fn expected_dats(self) -> ExpectedDats {
        match self {
            // The 1999 files: the retail CD's (portal 102, cell 67), the October 31 portal (105),
            // and any other before the next files held, August 2000's (portal 124, cell 108).
            Self::Launch => ExpectedDats {
                container: crate::ContainerEra::Classic,
                portal: Some((102, 123)),
                cell: Some((67, 107)),
            },
            // Files later than the last ones held before February 2005, September 2004's
            // (portal 2020, cell 1536); February 2005's are portal 2112 and cell 1593.
            Self::Infiltration => ExpectedDats {
                container: crate::ContainerEra::Classic,
                portal: Some((2021, u32::MAX)),
                cell: Some((1537, u32::MAX)),
            },
            Self::Eor => ExpectedDats {
                container: crate::ContainerEra::Modern,
                portal: None,
                cell: None,
            },
        }
    }

    /// How much experience wins back a point of vitae in the era.
    #[must_use]
    pub const fn vitae_recovery(self) -> VitaeRecovery {
        match self {
            Self::Launch | Self::Infiltration => VitaeRecovery::BeforeThroneOfDestiny,
            Self::Eor => VitaeRecovery::EndOfRetail,
        }
    }
}

/// The data files an era expects: which set, and the range of iterations (both ends included)
/// its portal and cell files may carry. A range of `None` takes any file of the set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpectedDats {
    pub container: crate::ContainerEra,
    pub portal: Option<(u32, u32)>,
    pub cell: Option<(u32, u32)>,
}

impl ExpectedDats {
    /// Whether files of `container` whose portal and cell files carry these iterations are the
    /// era's. An iteration the files do not tell (`None`) fails a range.
    #[must_use]
    pub fn admits(
        &self,
        container: crate::ContainerEra,
        portal: Option<u32>,
        cell: Option<u32>,
    ) -> bool {
        let within = |range: Option<(u32, u32)>, have: Option<u32>| {
            range.is_none_or(|(lo, hi)| have.is_some_and(|n| (lo..=hi).contains(&n)))
        };
        container == self.container && within(self.portal, portal) && within(self.cell, cell)
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
        /// The later systems, actions and rules an era has or lacks. The era's table
        /// ([`EraId::features`]) is only the default: a server may turn any of them on or off for
        /// its world, and announces the set it plays to the client ([`EraFeatureOverrides`]).
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
    /// Damage-resistance, critical, healing, life, damage-over-time and nether ratings. Without
    /// them every such rating is 0 and its modifier 1.
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
    /// makes and teaches it if it is new. It left the game in June 2002, so only Launch enables
    /// it by default. A character's own spell formulas (the components each spell
    /// takes for its account) are not this system: every era keeps them.
    spell_research,
    /// Chess on the game boards.
    chess,
    /// An oath of allegiance costs unassigned experience once a character has broken from a
    /// patron: five percent of its next level's experience, held between 100 and 5,000, a quarter
    /// more for each break. Throne of Destiny removed it, so the end of retail does not have it.
    swear_xp_cost,
    // Version 2 of the table starts here.
    /// A minute of invulnerability after resurrecting at a lifestone, given up by teleporting,
    /// casting or attacking (May 2000).
    lifestone_protection,
    /// Skill credits left unspent at character creation are kept. Without it they are lost when
    /// the character enters the world (November 2002).
    creation_credit_carry_over,
    /// Changing skills after creation: untraining, unspecializing and specializing them with the
    /// items that do it (November 2002).
    skill_alteration,
    /// Buying augmentations with experience. The augmentation a heritage is born with is
    /// [`EraFeatures::innate_augmentations`].
    augmentations,
    /// PK Lite, the optional lighter player-killer status (September 2003).
    pk_lite,
    /// The `@marketplace` recall (January 2003).
    marketplace_recall,
    /// Strength and Endurance raising a player's natural resistances, health regeneration and
    /// stamina savings, and the character sheet's account of them (April 2002).
    endurance_benefits,
    /// `@lifestone` recalls to the lifestone for half the character's mana, and `@die` kills the
    /// character. Without it `@lifestone` asks whether to kill the character and does, and there
    /// is no `@die` (April 2002).
    lifestone_recall,
    /// `@pkarena` and `@pklarena`, the recalls to the player-killer arenas. `@pklarena` also needs
    /// [`EraFeatures::pk_lite`].
    pk_arena_recalls,
    /// A player corpse holding items lasts at least an hour, whatever the character's level
    /// (December 2000). Without it the corpse lasts five minutes a level.
    player_corpse_hour_minimum,
    /// Casting skill above a spell's power lowers the chance of burning its components.
    component_burn_skill_discount,
    /// War and Life Magic projectiles can strike critically (August 2002).
    war_and_life_projectile_criticals,
    /// A player's War Magic projectiles do more damage the further their skill exceeds the
    /// spell's power (August 2002).
    war_magic_skill_damage_bonus,
    /// A transfer spell whose target cannot take its whole benefit costs its source less
    /// (July 2002). Without it the overflow is lost and the source pays in full.
    transfer_overflow_reduces_source,
    /// An item enchantment cast on a creature lands on the items it wears (September 2003), at a
    /// higher mana cost for armor (June 2004). Without it item enchantments take an item as their
    /// target.
    item_enchantment_redirection,
    // Version 3 of the table starts here.
    /// The spellcasting panel's tabs, which sort a character's spells (May 2000). Without them
    /// the panel holds one list.
    spell_tabs,
    /// A fellowship's founder chooses whether it shares experience (December 2000). Without it
    /// every fellowship shares.
    fellowship_options,
    /// `@permit` and `@consent`: letting other players loot your corpse (April 2001).
    corpse_permissions,
    /// The corpse's coordinates, told the player after an outdoor death (May 2001).
    corpse_location_notices,
    /// `@log`, copying the chat window to a file (May 2001).
    chat_log,
    /// `@corpse`, which shows where the last outdoor death was (August 2001).
    corpse_command,
    /// The allegiance's message of the day and its broadcasts (June 2001).
    allegiance_messages,
    /// A patron is asked to accept an oath of allegiance (March 2002). Without it an oath is
    /// accepted at once.
    allegiance_oath_confirmation,
    /// A patron or monarch booting a member out of the allegiance (April 2002).
    allegiance_boot,
    /// Appraising an item needs no appraisal skill (May 2002). Without it the skill for the
    /// item's kind is rolled against the item's difficulty.
    appraisal_without_skills,
    /// A trade note cashes at its face value at any vendor and sells at 15% over it (May 2003).
    /// Without it a note sells at 10% over its value and cashes at face value only at the
    /// vendor that issued it, at 80% elsewhere.
    trade_notes_cash_anywhere,
    /// Allegiance officers and account bans (May 2004).
    allegiance_officers_and_bans,
    /// Naming the allegiance (May 2006).
    allegiance_name,
    // Version 4 of the table starts here.
    /// A fellowship shares its creatures' loot where its founder chooses to (by September 2001).
    /// Without it nobody may loot a fellow's kill as a fellow.
    fellowship_share_loot,
    /// Accepting every fellowship invitation at once, as a character's option (September 2003).
    /// Without it every invitation asks.
    fellowship_auto_accept,
    /// A leader opening a fellowship, so that any member may recruit (October 2003).
    fellowship_open,
    /// The allegiance chat room, `@a` and its options (January 2004). The tells to a patron, a
    /// monarch and vassals, and the allegiance's broadcasts, are older and do not need it.
    allegiance_chat,
    /// The bonus for a skill trained or specialized at character creation is free experience, worth
    /// exactly its 5 or 10 ranks, so the next rank costs what that rank costs on the curve. When it
    /// went away is not known. Without it a specialized skill starts with 10 ranks that no
    /// experience paid for.
    creation_skill_bonus_as_experience,
    /// The game's own chat rooms: General, Trade, LFG, Roleplay, Society and Olthoi, with the
    /// menu entries, options, filters and commands that go with them (September 21, 2005). The
    /// allegiance room is older and a Dereth Network room is the network's: neither needs it.
    global_chat_rooms,
    /// A charge on each spell a character knows, spent by casting it and regained over time, that
    /// scales what the spell does (removed June 13, 2002).
    spell_charge,
    /// A world-wide value on each spell, spent by everyone casting it and regained over hours, that
    /// scales what the spell does and how long it lasts, with a line saying how strong the magic
    /// feels and a duration range on the spell panels (removed June 13, 2002).
    spell_economy,
    // Version 5 of the table starts here.
    /// Officer titles, the allegiance lock with its approved vassal, and gagging a member of the
    /// allegiance room (May 2006). Also needs [`EraFeatures::allegiance_officers_and_bans`].
    allegiance_titles_and_lock,
    /// `@allegiance house`: opening and closing the monarch's villa or mansion to the allegiance,
    /// as guests and for storage (after June 2005). Its storage also needs
    /// [`EraFeatures::apartments`].
    allegiance_house_commands,
}

impl EraFeatures {
    /// Launch (November 1999): spell research, spell charge, the spell economy, the allegiance oath
    /// cost and the creation skill bonus
    /// as experience; none of the later systems, actions or rules.
    pub const LAUNCH: Self = Self {
        spell_research: true,
        swear_xp_cost: true,
        creation_skill_bonus_as_experience: true,
        spell_charge: true,
        spell_economy: true,
        ..Self::NONE
    };

    /// The end of retail: every system but spell research, the oath's experience cost and the
    /// creation skill bonus as experience.
    pub const END_OF_RETAIL: Self = Self {
        spell_research: false,
        swear_xp_cost: false,
        creation_skill_bonus_as_experience: false,
        spell_charge: false,
        spell_economy: false,
        ..Self::ALL
    };

    /// February 2005: none of the systems from ratings to the journal, nor spell research; trade,
    /// housing, apartments, tinkering, cantrips, chess and the oath's experience cost, which that
    /// world had, and the actions and rules of the later tables that it had: not augmentations
    /// (July 2005), the arena recalls (January 2006) or the allegiance's name, titles, lock and
    /// house commands (after June 2005).
    pub const INFILTRATION: Self = Self {
        trade: true,
        housing: true,
        apartments: true,
        tinkering: true,
        cantrips: true,
        chess: true,
        swear_xp_cost: true,
        lifestone_protection: true,
        creation_credit_carry_over: true,
        skill_alteration: true,
        pk_lite: true,
        marketplace_recall: true,
        endurance_benefits: true,
        lifestone_recall: true,
        player_corpse_hour_minimum: true,
        component_burn_skill_discount: true,
        war_and_life_projectile_criticals: true,
        war_magic_skill_damage_bonus: true,
        transfer_overflow_reduces_source: true,
        item_enchantment_redirection: true,
        spell_tabs: true,
        fellowship_options: true,
        corpse_permissions: true,
        corpse_location_notices: true,
        chat_log: true,
        corpse_command: true,
        allegiance_messages: true,
        allegiance_oath_confirmation: true,
        allegiance_boot: true,
        appraisal_without_skills: true,
        trade_notes_cash_anywhere: true,
        allegiance_officers_and_bans: true,
        fellowship_share_loot: true,
        fellowship_auto_accept: true,
        fellowship_open: true,
        allegiance_chat: true,
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
    pub const TABLE_VERSION: u16 = 5;

    /// How many systems each version of the table has, version 1 first.
    pub const COUNT_BY_VERSION: [usize; Self::TABLE_VERSION as usize] = [24, 39, 52, 60, 62];

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
/// (`3:ffff5fffffff0f` is the end of retail).
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EraFeatureOverrides {
    values: [Option<bool>; EraFeatures::COUNT],
}

impl Default for EraFeatureOverrides {
    fn default() -> Self {
        Self {
            values: [None; EraFeatures::COUNT],
        }
    }
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
    fn every_era_round_trips_its_name_and_the_end_of_retail_has_every_system_but_three_early_ones()
    {
        for e in EraId::ALL {
            assert_eq!(EraId::parse(e.name()), Some(e));
        }
        assert_eq!(EraId::parse(" INFILTRATION "), Some(EraId::Infiltration));
        assert_eq!(EraId::parse("tod"), None);
        assert!(EraId::Infiltration < EraId::Eor);
        for (name, on) in EraId::Eor.features().iter() {
            assert_eq!(
                on,
                ![
                    "spell_research",
                    "swear_xp_cost",
                    "creation_skill_bonus_as_experience",
                    "spell_charge",
                    "spell_economy"
                ]
                .contains(&name),
                "{name}"
            );
        }
        assert_eq!(EraFeatures::default(), EraId::Eor.features());
    }

    /// February 2005 has trade, housing, apartments, tinkering, cantrips, chess and the oath's
    /// experience cost, and neither spell research nor any of the later systems; of the later
    /// tables' actions and rules it has those dated before it.
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
        for (i, (name, value)) in f.iter().enumerate() {
            let later_table = i >= EraFeatures::count_at_version(1);
            let later_rule = [
                "augmentations",
                "pk_arena_recalls",
                "allegiance_name",
                "creation_skill_bonus_as_experience",
                "global_chat_rooms",
                "spell_charge",
                "spell_economy",
                "allegiance_titles_and_lock",
                "allegiance_house_commands",
            ]
            .contains(&name);
            assert_eq!(
                value,
                !later_rule && (later_table || on.contains(&name)),
                "{name}"
            );
        }
        assert_eq!(EraFeatures::NAMES.len(), 62);
        assert_eq!(f, f.implied());
    }

    /// Launch has spell research and the oath's experience cost, and nothing else of either table.
    #[test]
    fn launch_has_spell_research_the_oath_cost_the_creation_bonus_and_nothing_later() {
        let on: Vec<_> = EraId::Launch
            .features()
            .iter()
            .filter(|(_, on)| *on)
            .map(|(name, _)| name)
            .collect();
        assert_eq!(
            on,
            [
                "spell_research",
                "swear_xp_cost",
                "creation_skill_bonus_as_experience",
                "spell_charge",
                "spell_economy"
            ]
        );
    }

    #[test]
    fn launch_takes_only_the_1999_files_and_infiltration_only_files_after_september_2004() {
        use crate::ContainerEra::{Classic, Modern};
        let launch = EraId::Launch.expected_dats();
        assert!(
            launch.admits(Classic, Some(105), Some(67)),
            "the launch pair"
        );
        assert!(
            launch.admits(Classic, Some(102), Some(67)),
            "the retail CD's pair"
        );
        assert!(!launch.admits(Classic, Some(124), Some(108)), "August 2000");
        assert!(
            !launch.admits(Classic, Some(2112), Some(1593)),
            "February 2005"
        );
        assert!(
            !launch.admits(Classic, Some(105), Some(1593)),
            "a later cell file"
        );
        assert!(
            !launch.admits(Classic, Some(2112), Some(67)),
            "a later portal file"
        );
        assert!(
            !launch.admits(Classic, Some(105), None),
            "a cell file that tells nothing"
        );
        assert!(
            !launch.admits(Modern, Some(2072), Some(982)),
            "the end of retail"
        );
        let feb = EraId::Infiltration.expected_dats();
        assert!(feb.admits(Classic, Some(2112), Some(1593)));
        assert!(
            !feb.admits(Classic, Some(2020), Some(1536)),
            "September 2004"
        );
        assert!(!feb.admits(Classic, Some(105), Some(67)));
        let eor = EraId::Eor.expected_dats();
        assert!(eor.admits(Modern, None, None));
        assert!(!eor.admits(Classic, Some(2112), Some(1593)));
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
        assert!(text.contains(",chess=true,swear_xp_cost=true,"), "{text}");
        assert!(
            text.ends_with(
                ",spell_economy=false,allegiance_titles_and_lock=false,allegiance_house_commands=false"
            ),
            "{text}"
        );
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
        const VERSION_2: [&str; 15] = [
            "lifestone_protection",
            "creation_credit_carry_over",
            "skill_alteration",
            "augmentations",
            "pk_lite",
            "marketplace_recall",
            "endurance_benefits",
            "lifestone_recall",
            "pk_arena_recalls",
            "player_corpse_hour_minimum",
            "component_burn_skill_discount",
            "war_and_life_projectile_criticals",
            "war_magic_skill_damage_bonus",
            "transfer_overflow_reduces_source",
            "item_enchantment_redirection",
        ];
        const VERSION_3: [&str; 13] = [
            "spell_tabs",
            "fellowship_options",
            "corpse_permissions",
            "corpse_location_notices",
            "chat_log",
            "corpse_command",
            "allegiance_messages",
            "allegiance_oath_confirmation",
            "allegiance_boot",
            "appraisal_without_skills",
            "trade_notes_cash_anywhere",
            "allegiance_officers_and_bans",
            "allegiance_name",
        ];
        let two = VERSION_1.len() + VERSION_2.len();
        assert_eq!(EraFeatures::NAMES[..VERSION_1.len()], VERSION_1);
        assert_eq!(EraFeatures::NAMES[VERSION_1.len()..two], VERSION_2);
        assert_eq!(EraFeatures::NAMES[two..two + VERSION_3.len()], VERSION_3);
        assert_eq!(
            EraFeatures::COUNT_BY_VERSION[..3],
            [VERSION_1.len(), two, two + VERSION_3.len()]
        );
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
            "5:ffff5fffffffff32"
        );
        assert_eq!(
            EraFeatureBits::of(EraFeatures::INFILTRATION).to_string(),
            "5:0000dff7fefff700"
        );
        assert_eq!(
            EraFeatureBits::of(EraFeatures::LAUNCH).to_string(),
            "5:0000a0000000000d"
        );
    }

    /// A peer built against the first table reads a second-table announcement as it read a first
    /// one: the first three bytes are the bytes the first table wrote for every era, and the bits
    /// past its 24 are skipped. A first-table announcement read here leaves every second-table
    /// rule to the era's table.
    #[test]
    fn a_first_table_peer_reads_the_second_table_as_it_read_the_first() {
        let first_table = [
            (EraId::Eor, [0xff, 0xff, 0x5f]),
            (EraId::Infiltration, [0x00, 0x00, 0xdf]),
            (EraId::Launch, [0x00, 0x00, 0xa0]),
        ];
        for (era, v1) in first_table {
            let bits = EraFeatureBits::of(era.features());
            assert_eq!(bits.bytes[..3], v1, "{era}");
            // What a first-table reader keeps: the bits both tables name, read the same way.
            let old_reader = |i: usize| bits.bytes.get(i / 8).map(|b| b & (1 << (i % 8)) != 0);
            for (i, (name, on)) in era.features().iter().enumerate().take(24) {
                assert_eq!(old_reader(i), Some(on), "{era} {name}");
            }

            // A first-table writer, read here, even one that sends a padding byte or two past its
            // table.
            let old_writer = EraFeatureBits {
                table_version: 1,
                bytes: [&v1[..], &[0xff, 0xff]].concat(),
            };
            let over = old_writer.overrides();
            for name in &EraFeatures::NAMES[24..] {
                assert_eq!(over.get(name), None, "{era} {name} is unknown, never off");
            }
            assert_eq!(over.apply(era.features()), era.features(), "{era}");
        }
    }

    /// A peer built against the second table reads a third-table announcement as it read a
    /// second one: the first 39 bits are the ones the second table wrote for every era. A
    /// second-table announcement read here leaves every third-table system to the era's table.
    #[test]
    fn a_second_table_peer_reads_the_third_table_as_it_read_the_second() {
        let second_table = [
            (EraId::Eor, [0xff, 0xff, 0x5f, 0xff, 0x7f]),
            (EraId::Infiltration, [0x00, 0x00, 0xdf, 0xf7, 0x7e]),
            (EraId::Launch, [0x00, 0x00, 0xa0, 0x00, 0x00]),
        ];
        for (era, v2) in second_table {
            let bits = EraFeatureBits::of(era.features());
            let old_reader = |i: usize| bits.bytes.get(i / 8).map(|b| b & (1 << (i % 8)) != 0);
            let old_bits = |i: usize| v2[i / 8] & (1 << (i % 8)) != 0;
            for i in 0..39 {
                assert_eq!(old_reader(i), Some(old_bits(i)), "{era} bit {i}");
            }
            let old_writer = EraFeatureBits {
                table_version: 2,
                bytes: v2.to_vec(),
            };
            let over = old_writer.overrides();
            for name in &EraFeatures::NAMES[39..] {
                assert_eq!(over.get(name), None, "{era} {name} is unknown, never off");
            }
            assert_eq!(over.apply(era.features()), era.features(), "{era}");
        }
    }

    /// A third-table peer reads a fourth-table announcement as it read a third one, and a
    /// third-table announcement read here leaves the fourth table's systems to the era's table:
    /// an older server playing Launch has none of them, one playing a later era all of them.
    #[test]
    fn a_third_table_peer_reads_the_fourth_table_as_it_read_the_third() {
        let third_table = [
            (EraId::Eor, [0xff, 0xff, 0x5f, 0xff, 0xff, 0xff, 0x0f]),
            (
                EraId::Infiltration,
                [0x00, 0x00, 0xdf, 0xf7, 0xfe, 0xff, 0x07],
            ),
            (EraId::Launch, [0x00, 0x00, 0xa0, 0x00, 0x00, 0x00, 0x00]),
        ];
        for (era, v3) in third_table {
            let bits = EraFeatureBits::of(era.features());
            for i in 0..52 {
                let old = v3[i / 8] & (1 << (i % 8)) != 0;
                assert_eq!(bits.bit(i), Some(old), "{era} bit {i}");
            }
            let over = EraFeatureBits {
                table_version: 3,
                bytes: v3.to_vec(),
            }
            .overrides();
            for name in &EraFeatures::NAMES[52..] {
                assert_eq!(over.get(name), None, "{era} {name} is unknown, never off");
            }
            assert_eq!(over.apply(era.features()), era.features(), "{era}");
        }
    }

    /// A fourth-table peer reads a fifth-table announcement as it read a fourth one, and a
    /// fourth-table announcement read here leaves the fifth table's systems to the era's table.
    #[test]
    fn a_fourth_table_peer_reads_the_fifth_table_as_it_read_the_fourth() {
        let fourth_table = [
            (EraId::Eor, [0xff, 0xff, 0x5f, 0xff, 0xff, 0xff, 0xff, 0x02]),
            (
                EraId::Infiltration,
                [0x00, 0x00, 0xdf, 0xf7, 0xfe, 0xff, 0xf7, 0x00],
            ),
            (
                EraId::Launch,
                [0x00, 0x00, 0xa0, 0x00, 0x00, 0x00, 0x00, 0x0d],
            ),
        ];
        for (era, v4) in fourth_table {
            let bits = EraFeatureBits::of(era.features());
            for i in 0..60 {
                let old = v4[i / 8] & (1 << (i % 8)) != 0;
                assert_eq!(bits.bit(i), Some(old), "{era} bit {i}");
            }
            let over = EraFeatureBits {
                table_version: 4,
                bytes: v4.to_vec(),
            }
            .overrides();
            for name in &EraFeatures::NAMES[60..] {
                assert_eq!(over.get(name), None, "{era} {name} is unknown, never off");
            }
            assert_eq!(over.apply(era.features()), era.features(), "{era}");
        }
    }

    /// A newer table's bits past this build's are skipped; an older table's missing systems,
    /// and bits not sent, are unknown and left to the era's table.
    #[test]
    fn unknown_bits_are_unknown_never_off() {
        // A newer writer: two more systems, both on, past this table's last bit.
        let newer = EraFeatureBits {
            table_version: EraFeatures::TABLE_VERSION + 1,
            bytes: vec![0xff, 0xff, 0x5f, 0xff, 0xff, 0xff, 0xff, 0xf2],
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
        assert_eq!(EraFeatures::count_at_version(2), 39);
        assert_eq!(EraFeatures::count_at_version(3), 52);
        assert_eq!(EraFeatures::count_at_version(4), 60);
        assert_eq!(EraFeatures::count_at_version(5), 62);
        assert_eq!(EraFeatures::count_at_version(u16::MAX), EraFeatures::COUNT);
    }

    #[test]
    fn the_bitfield_text_refuses_what_is_not_one() {
        for bad in ["ffff5f", "0:ffff5f", "x:ff", "1:fff", "1:zz", "-1:ff"] {
            assert!(EraFeatureBits::parse(bad).is_err(), "{bad}");
        }
        assert_eq!(
            EraFeatureBits::parse(" 5:FFff5FfFfFfFfF32 "),
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
