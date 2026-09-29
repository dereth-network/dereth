// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Strings.cs
//! Port of `Source/ACE.Server/Entity/Strings.cs`.
//!
//! `GetFallMessage` and the death-message tables with `GetDeathMessage` are
//! ported here, with the attack verbs (`GetAttackVerb`).

use empyrean_common::dotnet;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_entity::enums::DamageType;

use crate::entity::death_message::DeathMessage;

// ACE: Strings.GetFallMessage
/// Returns the player message for falling impact damage.
#[must_use]
pub fn get_fall_message(damage: u32, max_health: u32) -> String {
    #[allow(clippy::cast_precision_loss)]
    let percent = damage as f32 / max_health as f32;

    let severity = if percent > 0.5 {
        "massive"
    } else if percent > 0.25 {
        "crushing"
    } else if percent > 0.1 {
        "heavy"
    } else {
        "minor"
    };

    format!(
        "You suffer {} points of {severity} impact damage.",
        dotnet::format(damage, "N0")
    )
}

// ---------------------------------------------------------------------------------------------
// Death messages. All verified in the retail captures: {0} = victim, {1} = killer.
// ---------------------------------------------------------------------------------------------

// ACE: Strings.Slashing
pub static SLASHING: [DeathMessage; 4] = [
    DeathMessage::new(
        "You split {0} apart!",
        "{1} splits you apart!",
        "{1} splits {0} apart!",
    ),
    DeathMessage::new(
        "You cleave {0} in twain!",
        "{1} cleaves you in twain!",
        "{1} cleaves {0} in twain!",
    ),
    DeathMessage::new(
        "{0} is torn to ribbons by your assault!",
        "You are torn to ribbons by {1}'s assault!",
        "{0} is torn to ribbons by {1}'s assault!",
    ),
    DeathMessage::new(
        "Your killing blow nearly turns {0} inside-out!",
        "{1}'s killing blow nearly turns you inside-out!",
        "{1}'s killing blow nearly turns {0} inside-out!",
    ),
];

// ACE: Strings.Piercing
pub static PIERCING: [DeathMessage; 4] = [
    DeathMessage::new(
        "You run {0} through!",
        "{1} runs you through!",
        "{1} runs {0} through!",
    ),
    DeathMessage::new(
        "{0} is fatally punctured!",
        "You are fatally punctured by {1}!",
        "{0} is fatally punctured by {1}!",
    ),
    DeathMessage::new(
        "{0}'s perforated corpse falls before you!",
        "Your perforated corpse falls before {1}!",
        "{0}'s perforated corpse falls before {1}!",
    ),
    DeathMessage::new(
        "{0}'s death is preceded by a sharp, stabbing pain!",
        "Your death is preceded by a sharp, stabbing pain, courtesy of {1}!",
        "{0}'s death is preceded by a sharp, stabbing pain, courtesy of {1}!",
    ),
];

// ACE: Strings.Bludgeoning
pub static BLUDGEONING: [DeathMessage; 4] = [
    DeathMessage::new(
        "You beat {0} to a lifeless pulp!",
        "{1} beats you to a lifeless pulp!",
        "{1} beats {0} to a lifeless pulp!",
    ),
    DeathMessage::new(
        "{0} is shattered by your assault!",
        "Your body is shattered by {1}'s attack!",
        "{0}'s body is shattered by {1}'s attack!",
    ),
    DeathMessage::new(
        "You flatten {0}'s body with the force of your assault!",
        "The force of {1}'s assault flattens you!",
        "The force of {1}'s assault flattens {0}!",
    ),
    DeathMessage::new(
        "The thunder of crushing {0} is followed by the deafening silence of death!",
        "The thunder of {1} crushing {0} is followed by the deafening silence of your death!",
        "The thunder of {1} crushing {0} is followed by the deafening silence of death!",
    ),
];

// ACE: Strings.Fire
pub static FIRE: [DeathMessage; 4] = [
    DeathMessage::new(
        "You bring {0} to a fiery end!",
        "{1} brings you to a fiery end!",
        "{1} brings {0} to a fiery end!",
    ),
    DeathMessage::new(
        "{0} is reduced to cinders!",
        "You are reduced to cinders by {1}!",
        "{1} reduced {0} to cinders!",
    ),
    DeathMessage::new(
        "{0} is incinerated by your assault!",
        "You are incinerated by {1}'s assault!",
        "{0} is incinerated by {1}'s assault!",
    ),
    DeathMessage::new(
        "{0}'s seared corpse smolders before you!",
        "Your seared corpse smolders before {1}!",
        "{0}'s seared corpse smolders before {1}!",
    ),
];

// ACE: Strings.Ice
pub static ICE: [DeathMessage; 3] = [
    DeathMessage::new(
        "Your attack stops {0} cold!",
        "{1}'s attack stops you cold!",
        "{1}'s attack stops {0} cold!",
    ),
    DeathMessage::new(
        "Your assault sends {0} to an icy death!",
        "{1}'s assault sends you to an icy death!",
        "{1}'s assault sends {0} to an icy death!",
    ),
    DeathMessage::new(
        "{0} suffers a frozen fate!",
        "You suffer a frozen fate at the hands of {1}!",
        "{0} suffers a frozen fate at the hands of {1}!",
    ),
];

// ACE: Strings.Acid
pub static ACID: [DeathMessage; 3] = [
    DeathMessage::new(
        "{0} is liquified by your attack!",
        "You are liquified by {1}'s attack!",
        "{0} is liquified by {1}'s attack!",
    ),
    DeathMessage::new(
        "{0}'s last strength dissolves before you!",
        "Your last strength dissolves before {1}!",
        "{0}'s last strength dissolves before {1}!",
    ),
    DeathMessage::new(
        "You reduce {0} to a sizzling, oozing mass!",
        "{1} reduces you to a sizzling, oozing mass!",
        "{1} reduces {0} to a sizzling, oozing mass!",
    ),
];

// ACE: Strings.Lightning
pub static LIGHTNING: [DeathMessage; 3] = [
    DeathMessage::new(
        "Blistered by lightning, {0} falls!",
        "Blistered by {1}'s lightning, you die!",
        "Blistered by {1}'s lightning, {0} dies!",
    ),
    DeathMessage::new(
        "Electricity tears {0} apart!",
        "Electricity from {1}'s attack tears you apart!",
        "Electricity from {1}'s attack tears {0} apart!",
    ),
    DeathMessage::new(
        "Your lightning coruscates over {0}'s mortal remains!",
        "{1}'s lightning coruscates over your mortal remains!",
        "{1}'s lightning coruscates over {0}'s mortal remains!",
    ),
];

// ACE: Strings.Void
pub static VOID: [DeathMessage; 3] = [
    DeathMessage::new(
        "{0} is dessicated by your attack!",
        "You are dessicated by {1}'s attack!",
        "{0} is dessicated by {1}'s attack!",
    ),
    DeathMessage::new(
        "{0}'s last strength withers before you!",
        "Your last strength withers before {1}!",
        "{0}'s last strength withers before {1}!",
    ),
    DeathMessage::new(
        "You reduce {0} to a drained, twisted corpse!",
        "{1} reduces you to a drained, twisted corpse!",
        "{1} reduces {0} to a drained, twisted corpse!",
    ),
];

// ACE: Strings.Critical
pub static CRITICAL: [DeathMessage; 7] = [
    DeathMessage::new(
        "You obliterate {0}!",
        "{1} obliterates you!",
        "{1} obliterates {0}!",
    ),
    DeathMessage::new(
        "You smite {0} mightily!",
        "{1} smites you mightily!",
        "{1} smites {0} mightily!",
    ),
    DeathMessage::new(
        "You knock {0} into next Morningthaw!",
        "{1} knocks you into next Morningthaw!",
        "{1} knocks {0} into next Morningthaw!",
    ),
    DeathMessage::new(
        "{0} is utterly destroyed by your attack!",
        "You are utterly destroyed by {1}'s attack!",
        "{0} is utterly destroyed by {1}'s attack!",
    ),
    DeathMessage::new(
        "{0} catches your attack, with dire consequences!",
        "You catch {1}'s attack, with dire consequences!",
        "{0} catches {1}'s attack, with dire consequences!",
    ),
    DeathMessage::new(
        "You slay {0} viciously enough to impart death several times over!",
        "{1} slays you viciously enough to impart death several times over!",
        "{1} slays {0} viciously enough to impart death several times over!",
    ),
    DeathMessage::new(
        "The deadly force of your attack is so strong that {0}'s ancestors feel it!",
        "The deadly force of {1}'s attack is so strong that your ancestors feel it!",
        "The deadly force of {1}'s attack is so strong that {0}'s ancestors feel it!",
    ),
];

// ACE: Strings.PKCritical
pub static PK_CRITICAL: [DeathMessage; 1] = [DeathMessage::new(
    "You send {0} to death so violently that even the lifestone flinches!",
    "{1} sends you to your death so violently that even the lifestone flinches!",
    "{1} sends {0} to death so violently that even the lifestone flinches!",
)];

// ACE: Strings.General
pub static GENERAL: [DeathMessage; 2] = [
    DeathMessage::new(
        "You killed {0}!",
        "You were killed by {1}!",
        "{0} was killed by {1}!",
    ),
    DeathMessage::new("{0} died!", "You died!", "{0} died!"),
];

// ACE: Strings.DeathMessages, Strings.Strings
/// `DeathMessages.TryGetValue(damageType, out var messages)`: the table the static constructor
/// (`Strings.Strings`) registers for each damage type, or null.
#[must_use]
pub fn death_messages(damage_type: DamageType) -> Option<&'static [DeathMessage]> {
    let messages: &'static [DeathMessage] = match damage_type {
        DamageType::Undef | DamageType::Base | DamageType::Health => &GENERAL,
        DamageType::Slash => &SLASHING,
        DamageType::Pierce => &PIERCING,
        DamageType::Bludgeon => &BLUDGEONING,
        DamageType::Fire => &FIRE,
        DamageType::Cold => &ICE,
        DamageType::Acid => &ACID,
        DamageType::Electric => &LIGHTNING,
        DamageType::Nether => &VOID,
        _ => return None,
    };
    Some(messages)
}

/// `messages[ThreadSafeRandom.Next(0, messages.Count - 1)]`.
fn pick(messages: &'static [DeathMessage]) -> DeathMessage {
    let count = i32::try_from(messages.len()).unwrap_or(i32::MAX);
    let idx = ThreadSafeRandom::next(0, count - 1);
    messages[usize::try_from(idx).unwrap_or(0)]
}

// ACE: Strings.GetDeathMessage
/// A random death message for the damage type (one `ThreadSafeRandom.Next(0, count - 1)` draw),
/// or a random critical one. An unknown damage type draws nothing and answers `General[1]`.
#[must_use]
pub fn get_death_message(damage_type: DamageType, critical_hit: bool) -> DeathMessage {
    if critical_hit {
        pick(&CRITICAL)
    } else {
        //var damageType = killer.GetDamageType();
        let Some(messages) = death_messages(damage_type) else {
            // DIVERGE: ACE writes this line to the console; here it is a log line.
            log::info!(
                "GetDeathMessage({}, {critical_hit}) - unknown damage type",
                damage_type.to_dotnet_string()
            );
            return GENERAL[1];
        };

        pick(messages)
    }
}

// ACE: Strings.GetAttackVerb
/// The (single, plural) attack verb for a damage type and the fraction of health it took: `None`
/// (ACE returns false and leaves both `null`) for a negative fraction.
#[must_use]
pub fn get_attack_verb(
    damage_type: DamageType,
    percent: f32,
) -> Option<(&'static str, &'static str)> {
    if percent < 0.0 {
        return None;
    }

    Some(match damage_type {
        DamageType::Slash => {
            if percent > 0.5 {
                ("mangle", "mangles")
            } else if percent > 0.25 {
                ("slash", "slashes")
            } else if percent > 0.1 {
                ("cut", "cuts")
            } else {
                ("scratch", "scratches")
            }
        }
        DamageType::Pierce => {
            if percent > 0.5 {
                ("gore", "gores")
            } else if percent > 0.25 {
                ("impale", "impales")
            } else if percent > 0.1 {
                ("stab", "stabs")
            } else {
                ("nick", "nicks")
            }
        }
        DamageType::Bludgeon => {
            if percent > 0.5 {
                ("crush", "crushes")
            } else if percent > 0.25 {
                ("smash", "smashes")
            } else if percent > 0.1 {
                ("bash", "bashes")
            } else {
                ("graze", "grazes")
            }
        }
        DamageType::Fire => {
            if percent > 0.5 {
                ("incinerate", "incinerates")
            } else if percent > 0.25 {
                ("burn", "burns")
            } else if percent > 0.1 {
                ("scorch", "scorches")
            } else {
                ("singe", "singes")
            }
        }
        DamageType::Cold => {
            if percent > 0.5 {
                ("freeze", "freezes")
            } else if percent > 0.25 {
                ("frost", "frosts")
            } else if percent > 0.1 {
                ("chill", "chills")
            } else {
                ("numb", "numbs")
            }
        }
        DamageType::Acid => {
            if percent > 0.5 {
                ("dissolve", "dissolves")
            } else if percent > 0.25 {
                ("corrode", "corrodes")
            } else if percent > 0.1 {
                ("sear", "sears")
            } else {
                ("blister", "blisters")
            }
        }
        DamageType::Electric => {
            if percent > 0.5 {
                ("blast", "blasts")
            } else if percent > 0.25 {
                ("jolt", "jolts")
            } else if percent > 0.1 {
                ("shock", "shocks")
            } else {
                ("spark", "sparks")
            }
        }
        DamageType::Nether => {
            if percent > 0.5 {
                ("eradicate", "eradicates")
            } else if percent > 0.25 {
                ("wither", "withers")
            } else if percent > 0.1 {
                ("twist", "twists")
            } else {
                ("scar", "scars")
            }
        }
        DamageType::Health => {
            if percent > 0.5 {
                ("deplete", "depletes")
            } else if percent > 0.25 {
                ("siphon", "siphons")
            } else if percent > 0.1 {
                ("exhaust", "exhausts")
            } else {
                ("drain", "drains")
            }
        }
        _ => ("hit", "hits"),
    })
}
