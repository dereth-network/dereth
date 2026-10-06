//! A world's rules as data: the rules a world may set, their bounds, and the named profiles the
//! client is given for worlds whose servers cannot say their own.
//!
//! [`VOCABULARY`] is every rule: its dotted key, the kind of value it takes and that value's
//! bounds. A rule set is a list of `(key, value)` entries over the end of retail's rules, read
//! into a [`WorldRules`] by [`rules_of`], which refuses the whole set if any entry names no rule,
//! has the wrong kind of value or is out of bounds. A refused set is never applied in part.
//!
//! [`PROFILES`] are the sets compiled into the client, each named for the worlds it is for and
//! chosen by `--world-profile`. Each reproduces what those worlds' own client changed, so a
//! character there carries, runs, jumps and reads what the world's server expects.
//!
//! [`VOCABULARY`]: crate::world::VOCABULARY
//! [`WorldRules`]: dereth_primitives::WorldRules
//! [`rules_of`]: crate::world::rules_of
//! [`PROFILES`]: crate::world::PROFILES

use std::borrow::Cow;

use dereth_primitives::{TextKey, VitaeCurve, VitaeRecovery, WorldRules};

/// One rule's value in a rule set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RuleValue {
    Switch(bool),
    Whole(i32),
    Real(f32),
    /// One of the rule's own named choices.
    Choice(&'static str),
    /// A text override.
    Text(&'static str),
}

/// The kind of value a rule takes, with its bounds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RuleKind {
    Switch,
    Whole {
        min: i32,
        max: i32,
    },
    Real {
        min: f32,
        max: f32,
    },
    Choice(&'static [&'static str]),
    /// Plain text of at most this many characters.
    Text {
        max_chars: usize,
    },
}

/// One rule.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RuleSpec {
    pub key: &'static str,
    pub kind: RuleKind,
}

/// The longest text a world may put in place of the client's.
pub const MAX_TEXT_CHARS: usize = 200;

const TEXT: RuleKind = RuleKind::Text {
    max_chars: MAX_TEXT_CHARS,
};

/// The vitae curves by name.
pub const VITAE_CURVES: &[&str] = &["era", "end_of_retail", "before_throne_of_destiny"];

/// Every rule a world may set. Rules are only ever added to it; a key is never renamed or given
/// another kind of value.
pub const VOCABULARY: &[RuleSpec] = &[
    RuleSpec {
        key: "burden.strength_bonus",
        kind: RuleKind::Whole {
            min: -100,
            max: 1000,
        },
    },
    RuleSpec {
        key: "movement.run_scale",
        kind: RuleKind::Real {
            min: 0.25,
            max: 4.0,
        },
    },
    RuleSpec {
        key: "movement.jump_scale",
        kind: RuleKind::Real {
            min: 0.25,
            max: 4.0,
        },
    },
    RuleSpec {
        key: "vitae.curve",
        kind: RuleKind::Choice(VITAE_CURVES),
    },
    RuleSpec {
        key: "combat.recklessness_marker",
        kind: RuleKind::Switch,
    },
    RuleSpec {
        key: "appraisal.shield_shows_level",
        kind: RuleKind::Switch,
    },
    RuleSpec {
        key: "appraisal.whole_damage",
        kind: RuleKind::Switch,
    },
    RuleSpec {
        key: "chargen.random_original_heritages_only",
        kind: RuleKind::Switch,
    },
    RuleSpec {
        key: "equip.weapons_block_shields",
        kind: RuleKind::Switch,
    },
    RuleSpec {
        key: "equip.trinket_replaces_worn",
        kind: RuleKind::Switch,
    },
    RuleSpec {
        key: "chat.enchantment_expiry_line",
        kind: RuleKind::Switch,
    },
    RuleSpec {
        key: "selection.asks_any_mana",
        kind: RuleKind::Switch,
    },
    RuleSpec {
        key: "housing.apartment_buys_without_asking",
        kind: RuleKind::Switch,
    },
    RuleSpec {
        key: "text.appraisal.shield_unknown",
        kind: TEXT,
    },
    RuleSpec {
        key: "text.appraisal.burden_unknown",
        kind: TEXT,
    },
    RuleSpec {
        key: "text.appraisal.nether_rending",
        kind: TEXT,
    },
    RuleSpec {
        key: "text.appraisal.magic_defense",
        kind: TEXT,
    },
    RuleSpec {
        key: "text.appraisal.missile_defense",
        kind: TEXT,
    },
    RuleSpec {
        key: "text.appraisal.melee_defense",
        kind: TEXT,
    },
    RuleSpec {
        key: "text.appraisal.pk_lite_portal",
        kind: TEXT,
    },
    RuleSpec {
        key: "text.appraisal.unenchantable_note",
        kind: TEXT,
    },
    RuleSpec {
        key: "text.appraisal.pk_lite_status",
        kind: TEXT,
    },
    RuleSpec {
        key: "text.skill.void_magic",
        kind: TEXT,
    },
    RuleSpec {
        key: "text.school.void_magic",
        kind: TEXT,
    },
    RuleSpec {
        key: "text.chargen.fourth_town",
        kind: TEXT,
    },
    RuleSpec {
        key: "text.housing.buy_confirmation",
        kind: TEXT,
    },
    RuleSpec {
        key: "text.failure.pk_lite_portal",
        kind: TEXT,
    },
    RuleSpec {
        key: "text.failure.pk_lite_command",
        kind: TEXT,
    },
];

/// A named rule set compiled into the client.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Profile {
    /// The name `--world-profile` takes.
    pub name: &'static str,
    /// Who it is for, in a line.
    pub about: &'static str,
    pub entries: &'static [(&'static str, RuleValue)],
}

/// The profile for ClassicACE's Infiltration ruleset.
pub const CLASSICACE_INFILTRATION: &str = "classicace-infiltration";
/// The profile for ClassicACE's CustomDM ruleset (Dekarutide, Unfamiliar Shores).
pub const CLASSICACE_CUSTOMDM: &str = "classicace-customdm";
/// The profile for ClassicDereth's worlds (Seedsow, Snowreap). Its client changes no rule this
/// client knows of, so it sets none; it names the worlds' set so one can be added.
pub const CLASSICDERETH: &str = "classicdereth";

/// The profiles compiled into the client.
pub const PROFILES: &[Profile] = &[
    Profile {
        name: CLASSICACE_INFILTRATION,
        about: "ClassicACE worlds on the Infiltration ruleset",
        entries: &[
            // Both ClassicACE rulesets' client.
            ("vitae.curve", RuleValue::Choice("before_throne_of_destiny")),
            ("combat.recklessness_marker", RuleValue::Switch(false)),
            ("appraisal.shield_shows_level", RuleValue::Switch(true)),
            (
                "text.appraisal.shield_unknown",
                RuleValue::Text("Covers Front"),
            ),
            (
                "chargen.random_original_heritages_only",
                RuleValue::Switch(true),
            ),
            ("text.chargen.fourth_town", RuleValue::Text("Random")),
        ],
    },
    Profile {
        name: CLASSICACE_CUSTOMDM,
        about: "ClassicACE worlds on the CustomDM ruleset",
        entries: &[
            // Both ClassicACE rulesets' client.
            ("vitae.curve", RuleValue::Choice("before_throne_of_destiny")),
            ("combat.recklessness_marker", RuleValue::Switch(false)),
            ("appraisal.shield_shows_level", RuleValue::Switch(true)),
            (
                "text.appraisal.shield_unknown",
                RuleValue::Text("Covers Front"),
            ),
            (
                "chargen.random_original_heritages_only",
                RuleValue::Switch(true),
            ),
            ("text.chargen.fourth_town", RuleValue::Text("Other")),
            ("burden.strength_bonus", RuleValue::Whole(40)),
            ("movement.run_scale", RuleValue::Real(1.5)),
            ("movement.jump_scale", RuleValue::Real(1.5)),
            ("equip.weapons_block_shields", RuleValue::Switch(false)),
            ("equip.trinket_replaces_worn", RuleValue::Switch(true)),
            ("chat.enchantment_expiry_line", RuleValue::Switch(false)),
            ("selection.asks_any_mana", RuleValue::Switch(true)),
            (
                "housing.apartment_buys_without_asking",
                RuleValue::Switch(false),
            ),
            (
                "text.housing.buy_confirmation",
                RuleValue::Text("Are you sure you want to buy this house?"),
            ),
            ("appraisal.whole_damage", RuleValue::Switch(true)),
            (
                "text.appraisal.nether_rending",
                RuleValue::Text("Elem. Rending"),
            ),
            (
                "text.appraisal.magic_defense",
                RuleValue::Text("+3 Magic Defense"),
            ),
            (
                "text.appraisal.missile_defense",
                RuleValue::Text("+3 Missile Defense"),
            ),
            (
                "text.appraisal.melee_defense",
                RuleValue::Text("+3 Melee Defense"),
            ),
            (
                "text.appraisal.burden_unknown",
                RuleValue::Text("Burden: ???"),
            ),
            (
                "text.appraisal.pk_lite_portal",
                RuleValue::Text("Hardcore Killers may not use this portal."),
            ),
            ("text.appraisal.unenchantable_note", RuleValue::Text("")),
            (
                "text.appraisal.pk_lite_status",
                RuleValue::Text("Player Killer"),
            ),
            ("text.skill.void_magic", RuleValue::Text("Any")),
            ("text.school.void_magic", RuleValue::Text("Any")),
            (
                "text.failure.pk_lite_portal",
                RuleValue::Text("Hardcore Killers may not interact with that portal!"),
            ),
            (
                "text.failure.pk_lite_command",
                RuleValue::Text("Only Hardcore Killer characters may use this command!"),
            ),
        ],
    },
    Profile {
        name: CLASSICDERETH,
        about: "ClassicDereth worlds",
        entries: &[],
    },
];

/// Why a rule set was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleError {
    /// No profile has this name.
    UnknownProfile(String),
    /// No rule has this key.
    UnknownRule(String),
    /// The value is not the kind the rule takes.
    WrongKind(String),
    /// The value is outside the rule's bounds, or not one of its choices.
    OutOfBounds(String),
    /// The set names this rule twice.
    Repeated(String),
}

impl core::fmt::Display for RuleError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownProfile(n) => write!(
                f,
                "no world profile is named {n:?} (known: {})",
                PROFILES
                    .iter()
                    .map(|p| p.name)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::UnknownRule(k) => write!(f, "no world rule is named {k:?}"),
            Self::WrongKind(k) => write!(f, "the world rule {k:?} takes another kind of value"),
            Self::OutOfBounds(k) => write!(f, "the world rule {k:?} is out of bounds"),
            Self::Repeated(k) => write!(f, "the world rule {k:?} is set twice"),
        }
    }
}

impl std::error::Error for RuleError {}

/// The rule named `key`.
#[must_use]
pub fn spec(key: &str) -> Option<&'static RuleSpec> {
    VOCABULARY.iter().find(|s| s.key == key)
}

/// The profile named `name` (any case).
#[must_use]
pub fn profile(name: &str) -> Option<&'static Profile> {
    PROFILES
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(name.trim()))
}

/// The rules the profile named `name` sets, over the end of retail's.
///
/// # Errors
/// [`RuleError::UnknownProfile`] for a name no profile has, or the first entry the profile's set
/// refuses for.
pub fn profile_rules(name: &str) -> Result<WorldRules, RuleError> {
    let p = profile(name).ok_or_else(|| RuleError::UnknownProfile(name.to_owned()))?;
    let mut rules = rules_of(p.entries)?;
    rules.profile = Some(Cow::Borrowed(p.name));
    Ok(rules)
}

/// A rule set read over the end of retail's rules, whole or not at all.
///
/// # Errors
/// The first entry that names no rule, takes the wrong kind of value, is out of bounds or
/// repeats a rule.
pub fn rules_of(entries: &[(&'static str, RuleValue)]) -> Result<WorldRules, RuleError> {
    let mut rules = WorldRules::default();
    for (i, (key, value)) in entries.iter().enumerate() {
        if entries[..i].iter().any(|(k, _)| k == key) {
            return Err(RuleError::Repeated((*key).to_owned()));
        }
        apply(&mut rules, key, *value)?;
    }
    Ok(rules)
}

fn check(spec: &RuleSpec, value: RuleValue) -> Result<(), RuleError> {
    let key = || spec.key.to_owned();
    match (spec.kind, value) {
        (RuleKind::Switch, RuleValue::Switch(_)) => Ok(()),
        (RuleKind::Whole { min, max }, RuleValue::Whole(n)) => (min..=max)
            .contains(&n)
            .then_some(())
            .ok_or_else(|| RuleError::OutOfBounds(key())),
        (RuleKind::Real { min, max }, RuleValue::Real(x)) => (x.is_finite()
            && (min..=max).contains(&x))
        .then_some(())
        .ok_or_else(|| RuleError::OutOfBounds(key())),
        (RuleKind::Choice(choices), RuleValue::Choice(c)) => choices
            .contains(&c)
            .then_some(())
            .ok_or_else(|| RuleError::OutOfBounds(key())),
        (RuleKind::Text { max_chars }, RuleValue::Text(t)) => (t.chars().count() <= max_chars
            && !t.chars().any(char::is_control))
        .then_some(())
        .ok_or_else(|| RuleError::OutOfBounds(key())),
        _ => Err(RuleError::WrongKind(key())),
    }
}

/// Set one rule.
///
/// # Errors
/// As [`rules_of`], for this one entry. A refused entry changes nothing.
pub fn apply(rules: &mut WorldRules, key: &str, value: RuleValue) -> Result<(), RuleError> {
    let spec = spec(key).ok_or_else(|| RuleError::UnknownRule(key.to_owned()))?;
    check(spec, value)?;
    if let (Some(name), RuleValue::Text(t)) = (key.strip_prefix("text."), value) {
        let text_key =
            TextKey::parse(name).ok_or_else(|| RuleError::UnknownRule(key.to_owned()))?;
        rules.set_text(text_key, t);
        return Ok(());
    }
    match (key, value) {
        ("burden.strength_bonus", RuleValue::Whole(n)) => rules.burden_strength_bonus = n,
        ("movement.run_scale", RuleValue::Real(x)) => rules.run_scale = x,
        ("movement.jump_scale", RuleValue::Real(x)) => rules.jump_scale = x,
        ("vitae.curve", RuleValue::Choice(c)) => {
            rules.vitae = match c {
                "end_of_retail" => VitaeCurve::Fixed(VitaeRecovery::EndOfRetail),
                "before_throne_of_destiny" => {
                    VitaeCurve::Fixed(VitaeRecovery::BeforeThroneOfDestiny)
                }
                _ => VitaeCurve::Era,
            }
        }
        ("combat.recklessness_marker", RuleValue::Switch(b)) => rules.recklessness_marker = b,
        ("appraisal.shield_shows_level", RuleValue::Switch(b)) => {
            rules.appraisal_shield_shows_level = b;
        }
        ("appraisal.whole_damage", RuleValue::Switch(b)) => rules.appraisal_whole_damage = b,
        ("chargen.random_original_heritages_only", RuleValue::Switch(b)) => {
            rules.random_original_heritages_only = b;
        }
        ("equip.weapons_block_shields", RuleValue::Switch(b)) => rules.weapons_block_shields = b,
        ("equip.trinket_replaces_worn", RuleValue::Switch(b)) => rules.trinket_replaces_worn = b,
        ("chat.enchantment_expiry_line", RuleValue::Switch(b)) => {
            rules.enchantment_expiry_line = b;
        }
        ("selection.asks_any_mana", RuleValue::Switch(b)) => rules.selection_asks_any_mana = b,
        ("housing.apartment_buys_without_asking", RuleValue::Switch(b)) => {
            rules.apartment_buys_without_asking = b;
        }
        _ => return Err(RuleError::UnknownRule(key.to_owned())),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_rule_in_the_vocabulary_can_be_set_and_every_text_key_has_a_rule() {
        for s in VOCABULARY {
            let v = match s.kind {
                RuleKind::Switch => RuleValue::Switch(true),
                RuleKind::Whole { min, .. } => RuleValue::Whole(min),
                RuleKind::Real { max, .. } => RuleValue::Real(max),
                RuleKind::Choice(c) => RuleValue::Choice(c[0]),
                RuleKind::Text { .. } => RuleValue::Text("x"),
            };
            let mut r = WorldRules::default();
            apply(&mut r, s.key, v).unwrap_or_else(|e| panic!("{}: {e}", s.key));
        }
        for k in TextKey::ALL {
            assert!(
                spec(&format!("text.{}", k.name())).is_some(),
                "{} has no rule",
                k.name()
            );
        }
    }

    #[test]
    fn every_compiled_profile_reads_whole_and_names_itself() {
        for p in PROFILES {
            let r = profile_rules(p.name).unwrap_or_else(|e| panic!("{}: {e}", p.name));
            assert_eq!(r.profile.as_deref(), Some(p.name));
        }
        assert_eq!(
            profile_rules("ClassicACE-CustomDM")
                .unwrap()
                .profile
                .as_deref(),
            Some(CLASSICACE_CUSTOMDM),
            "any case"
        );
        assert!(matches!(
            profile_rules("nowhere"),
            Err(RuleError::UnknownProfile(_))
        ));
    }

    /// Behaviour: world.rules.a-world-profile-sets-its-clients-rules-over-the-end-of-retails
    #[test]
    fn the_customdm_profile_sets_its_clients_rules_and_infiltration_only_the_shared_ones() {
        let c = profile_rules(CLASSICACE_CUSTOMDM).unwrap();
        assert_eq!(c.burden_strength_bonus, 40);
        assert_eq!((c.run_scale, c.jump_scale), (1.5, 1.5));
        assert!(!c.weapons_block_shields && c.trinket_replaces_worn);
        assert!(!c.enchantment_expiry_line && c.selection_asks_any_mana);
        assert!(!c.apartment_buys_without_asking && c.appraisal_whole_damage);
        assert!(!c.recklessness_marker && c.appraisal_shield_shows_level);
        assert!(c.random_original_heritages_only);
        assert_eq!(
            c.vitae,
            VitaeCurve::Fixed(VitaeRecovery::BeforeThroneOfDestiny)
        );
        assert_eq!(
            c.text(TextKey::AppraisalShieldUnknown),
            Some("Covers Front")
        );
        assert_eq!(c.text(TextKey::ChargenFourthTown), Some("Other"));
        assert_eq!(c.text(TextKey::SkillVoidMagic), Some("Any"));

        let i = profile_rules(CLASSICACE_INFILTRATION).unwrap();
        assert_eq!(i.text(TextKey::ChargenFourthTown), Some("Random"));
        assert_eq!(i.burden_strength_bonus, 0);
        assert_eq!(i.run_scale, 1.0);
        assert!(i.weapons_block_shields && i.enchantment_expiry_line);
        assert!(!i.recklessness_marker && i.random_original_heritages_only);
        assert_eq!(i.text(TextKey::SkillVoidMagic), None);

        let d = profile_rules(CLASSICDERETH).unwrap();
        assert!(d.is_end_of_retail(), "no rule this client knows of");
    }

    #[test]
    fn a_set_with_any_bad_entry_is_refused_whole() {
        assert_eq!(
            rules_of(&[("burden.strength_bonus", RuleValue::Whole(5000))]),
            Err(RuleError::OutOfBounds("burden.strength_bonus".into()))
        );
        assert_eq!(
            rules_of(&[("movement.run_scale", RuleValue::Switch(true))]),
            Err(RuleError::WrongKind("movement.run_scale".into()))
        );
        assert_eq!(
            rules_of(&[("movement.fly", RuleValue::Switch(true))]),
            Err(RuleError::UnknownRule("movement.fly".into()))
        );
        assert_eq!(
            rules_of(&[("vitae.curve", RuleValue::Choice("sideways"))]),
            Err(RuleError::OutOfBounds("vitae.curve".into()))
        );
        assert_eq!(
            rules_of(&[("text.skill.void_magic", RuleValue::Text("a\nb"))]),
            Err(RuleError::OutOfBounds("text.skill.void_magic".into()))
        );
        assert_eq!(
            rules_of(&[
                ("burden.strength_bonus", RuleValue::Whole(1)),
                ("burden.strength_bonus", RuleValue::Whole(2)),
            ]),
            Err(RuleError::Repeated("burden.strength_bonus".into()))
        );
        assert_eq!(
            rules_of(&[("movement.run_scale", RuleValue::Real(f32::NAN))]),
            Err(RuleError::OutOfBounds("movement.run_scale".into()))
        );
    }
}
