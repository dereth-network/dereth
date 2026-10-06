//! A world's rules: the values a world may set over the end of retail's, which the client and a
//! server compute alike.
//!
//! The end-of-retail client hard-codes a handful of rules a few worlds play differently: the burden
//! a character can carry, how fast it runs and how high it jumps, what an appraisal shows in a
//! shield's slot. [`WorldRules`] holds a world's value for each, and its [`Default`] is the end of
//! retail's, so a world that sets nothing plays exactly as the final client does.
//!
//! This is plain data. Which rules exist, their bounds and the named sets a world can be given
//! (its profile) are `dereth_rules::world`'s, which builds the value; every crate that applies a
//! rule reads it from here, so the two interfaces cannot disagree.

use std::borrow::Cow;

/// Which curve wins back a point of vitae.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VitaeCurve {
    /// The era's own ([`crate::EraId::vitae_recovery`]).
    #[default]
    Era,
    /// This curve whatever the era.
    Fixed(crate::VitaeRecovery),
}

/// A piece of interface text a world may word differently. Only the texts named here can be
/// replaced, so a world can never rewrite arbitrary text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TextKey {
    /// The appraisal's shield-slot line for a shield whose level is not known.
    AppraisalShieldUnknown,
    /// The appraisal's burden line when the burden is not known (`"Burden: Unknown"`).
    AppraisalBurdenUnknown,
    /// The special property `Nether Rending`.
    AppraisalNetherRending,
    /// The special property `+1 Magic Defense`.
    AppraisalMagicDefense,
    /// The special property `+1 Missile Defense`.
    AppraisalMissileDefense,
    /// The special property `+1 Melee Defense`.
    AppraisalMeleeDefense,
    /// An appraised item's sentence for a portal a lite player killer may not use.
    AppraisalPkLitePortal,
    /// A character appraisal's armour footnote (`"* = Unenchantable"`).
    AppraisalUnenchantableNote,
    /// A creature appraisal's player-killer-lite status.
    AppraisalPkLiteStatus,
    /// The skill name `Void Magic`.
    SkillVoidMagic,
    /// The magic school name `Void Magic`.
    SchoolVoidMagic,
    /// The fourth starting town's name on character creation's summary.
    ChargenFourthTown,
    /// The question a house purchase asks.
    HouseBuyConfirmation,
    /// The refusal of a portal to a lite player killer.
    FailurePkLitePortal,
    /// The refusal of a command only lite player killers may use.
    FailurePkLiteCommand,
}

impl TextKey {
    /// Every key, in declaration order.
    pub const ALL: [Self; 15] = [
        Self::AppraisalShieldUnknown,
        Self::AppraisalBurdenUnknown,
        Self::AppraisalNetherRending,
        Self::AppraisalMagicDefense,
        Self::AppraisalMissileDefense,
        Self::AppraisalMeleeDefense,
        Self::AppraisalPkLitePortal,
        Self::AppraisalUnenchantableNote,
        Self::AppraisalPkLiteStatus,
        Self::SkillVoidMagic,
        Self::SchoolVoidMagic,
        Self::ChargenFourthTown,
        Self::HouseBuyConfirmation,
        Self::FailurePkLitePortal,
        Self::FailurePkLiteCommand,
    ];

    /// The key's stable name, as a rule set spells it (`text.` and this).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::AppraisalShieldUnknown => "appraisal.shield_unknown",
            Self::AppraisalBurdenUnknown => "appraisal.burden_unknown",
            Self::AppraisalNetherRending => "appraisal.nether_rending",
            Self::AppraisalMagicDefense => "appraisal.magic_defense",
            Self::AppraisalMissileDefense => "appraisal.missile_defense",
            Self::AppraisalMeleeDefense => "appraisal.melee_defense",
            Self::AppraisalPkLitePortal => "appraisal.pk_lite_portal",
            Self::AppraisalUnenchantableNote => "appraisal.unenchantable_note",
            Self::AppraisalPkLiteStatus => "appraisal.pk_lite_status",
            Self::SkillVoidMagic => "skill.void_magic",
            Self::SchoolVoidMagic => "school.void_magic",
            Self::ChargenFourthTown => "chargen.fourth_town",
            Self::HouseBuyConfirmation => "housing.buy_confirmation",
            Self::FailurePkLitePortal => "failure.pk_lite_portal",
            Self::FailurePkLiteCommand => "failure.pk_lite_command",
        }
    }

    /// The key named `name`, as [`Self::name`] spells it.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name() == name)
    }
}

/// A world's rules. [`Default`] is the end of retail's for every one.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct WorldRules {
    /// The profile these came from, when they came from one.
    pub profile: Option<Cow<'static, str>>,
    /// Added to Strength before burden capacity is computed. 0 at the end of retail.
    pub burden_strength_bonus: i32,
    /// The run rate's divisor. 1 at the end of retail; a larger one runs slower.
    pub run_scale: f32,
    /// The jump height's divisor. 1 at the end of retail.
    pub jump_scale: f32,
    /// The vitae recovery curve.
    pub vitae: VitaeCurve,
    /// Whether the power bar shows the Recklessness marker to a character trained in it.
    pub recklessness_marker: bool,
    /// Whether a shield's appraisal shows its shield level (the end of retail) or, with this on,
    /// the item's level in that slot, with [`TextKey::AppraisalShieldUnknown`] when it has none.
    pub appraisal_shield_shows_level: bool,
    /// Whether the appraisal's damage range prints its low end as a whole number.
    pub appraisal_whole_damage: bool,
    /// Whether character creation's random choice of heritage is limited to the three original
    /// heritages whatever the account holds.
    pub random_original_heritages_only: bool,
    /// Whether a missile launcher, a two-handed weapon or a caster rules out a shield. On at the
    /// end of retail.
    pub weapons_block_shields: bool,
    /// Whether wielding a trinket while wearing one asks the server to wield the new one in its
    /// place, rather than refusing with "You're already wearing a trinket."
    pub trinket_replaces_worn: bool,
    /// Whether an enchantment's expiry writes "`<spell>` has expired." to chat. On at the end of
    /// retail.
    pub enchantment_expiry_line: bool,
    /// Whether selecting any object that is not a creature or a player, or a stack, asks the
    /// server for its mana, rather than only an item the player owns. With it the selection's
    /// stop request follows the health meter, and a reply with no mana asks for nothing more.
    pub selection_asks_any_mana: bool,
    /// Whether buying an apartment pays at once, as the end of retail's does; off, every purchase
    /// asks first.
    pub apartment_buys_without_asking: bool,
    /// The texts the world words differently, one entry per key ([`Self::set_text`] keeps it
    /// so); read them with [`Self::text`].
    pub texts: Vec<(TextKey, Cow<'static, str>)>,
}

impl Default for WorldRules {
    fn default() -> Self {
        Self {
            profile: None,
            burden_strength_bonus: 0,
            run_scale: 1.0,
            jump_scale: 1.0,
            vitae: VitaeCurve::Era,
            recklessness_marker: true,
            appraisal_shield_shows_level: false,
            appraisal_whole_damage: false,
            random_original_heritages_only: false,
            weapons_block_shields: true,
            trinket_replaces_worn: false,
            enchantment_expiry_line: true,
            selection_asks_any_mana: false,
            apartment_buys_without_asking: true,
            texts: Vec::new(),
        }
    }
}

impl PartialEq for WorldRules {
    fn eq(&self, other: &Self) -> bool {
        self.profile == other.profile
            && self.burden_strength_bonus == other.burden_strength_bonus
            && self.run_scale.to_bits() == other.run_scale.to_bits()
            && self.jump_scale.to_bits() == other.jump_scale.to_bits()
            && self.vitae == other.vitae
            && self.recklessness_marker == other.recklessness_marker
            && self.appraisal_shield_shows_level == other.appraisal_shield_shows_level
            && self.appraisal_whole_damage == other.appraisal_whole_damage
            && self.random_original_heritages_only == other.random_original_heritages_only
            && self.weapons_block_shields == other.weapons_block_shields
            && self.trinket_replaces_worn == other.trinket_replaces_worn
            && self.enchantment_expiry_line == other.enchantment_expiry_line
            && self.selection_asks_any_mana == other.selection_asks_any_mana
            && self.apartment_buys_without_asking == other.apartment_buys_without_asking
            && self.texts == other.texts
    }
}

impl Eq for WorldRules {}

impl WorldRules {
    /// The world's wording of `key`, when it words it differently.
    #[must_use]
    pub fn text(&self, key: TextKey) -> Option<&str> {
        self.texts
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, t)| t.as_ref())
    }

    /// The world's wording of `key`, else `default`.
    #[must_use]
    pub fn text_or<'a>(&'a self, key: TextKey, default: &'a str) -> &'a str {
        self.text(key).unwrap_or(default)
    }

    /// Word `key` as `text` for this world, replacing any wording it had.
    pub fn set_text(&mut self, key: TextKey, text: impl Into<Cow<'static, str>>) {
        let text = text.into();
        if let Some(slot) = self.texts.iter_mut().find(|(k, _)| *k == key) {
            slot.1 = text;
        } else {
            self.texts.push((key, text));
            self.texts.sort_by_key(|(k, _)| *k);
        }
    }

    /// The vitae curve in `era`.
    #[must_use]
    pub fn vitae_recovery(&self, era: crate::EraId) -> crate::VitaeRecovery {
        match self.vitae {
            VitaeCurve::Era => era.vitae_recovery(),
            VitaeCurve::Fixed(curve) => curve,
        }
    }

    /// Whether every rule is the end of retail's.
    #[must_use]
    pub fn is_end_of_retail(&self) -> bool {
        Self {
            profile: self.profile.clone(),
            ..Self::default()
        } == *self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_rules_are_the_end_of_retails() {
        let r = WorldRules::default();
        assert!(r.is_end_of_retail());
        assert_eq!(r.text(TextKey::AppraisalShieldUnknown), None);
        assert_eq!(
            r.text_or(TextKey::SkillVoidMagic, "Void Magic"),
            "Void Magic"
        );
        assert_eq!(
            r.vitae_recovery(crate::EraId::Eor),
            crate::VitaeRecovery::EndOfRetail
        );
    }

    #[test]
    fn a_text_set_twice_keeps_the_later_wording_and_every_key_round_trips_its_name() {
        let mut r = WorldRules::default();
        r.set_text(TextKey::SkillVoidMagic, "Any");
        r.set_text(TextKey::SkillVoidMagic, "Either");
        assert_eq!(r.text(TextKey::SkillVoidMagic), Some("Either"));
        assert!(!r.is_end_of_retail());
        for k in TextKey::ALL {
            assert_eq!(TextKey::parse(k.name()), Some(k));
        }
        let fixed = WorldRules {
            vitae: VitaeCurve::Fixed(crate::VitaeRecovery::BeforeThroneOfDestiny),
            ..WorldRules::default()
        };
        assert_eq!(
            fixed.vitae_recovery(crate::EraId::Eor),
            crate::VitaeRecovery::BeforeThroneOfDestiny
        );
    }
}
