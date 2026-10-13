//! The character directory's words: a listed character's profile and the bounds of each of its
//! fields, the three messages a world and the hub exchange about listings, and the six fields a
//! character's own "Allow others to see your ..." options keep out of its profile.
//!
//! A character is listed while it is on the network (`/dn on`). Its world sends its profile to the
//! hub when it changes, and the hub publishes it at once in the network's directory
//! (`docs/network/01-addresses-and-settings.md` section 2, `02-link-protocol.md` section 7).
//!
//! The directory's messages are a family of their own: [`DirectoryMessage`] reads them, and
//! [`crate::Message`] skips them as types it does not know, as every reader of a link skips a type
//! it does not know. A world or a hub that knows nothing of the directory carries on unchanged.

use serde::{Deserialize, Serialize};

use crate::address::Address;
use crate::limits::Rate;
use crate::wire::{WireError, MAX_ID_CHARS, MAX_MESSAGE_BYTES};

/// The highest level a profile may give: well above the final retail game's 275, for worlds that
/// raise it.
pub const MAX_LEVEL: u32 = 100_000;

/// The heritage groups, numbered as the game numbers them, 1 (Aluvian) to 13.
pub const MAX_HERITAGE: u8 = 13;

/// The longest title a profile shows, in characters.
pub const MAX_TITLE_CHARS: usize = 64;

/// The longest allegiance name, in characters.
pub const MAX_ALLEGIANCE_NAME_CHARS: usize = 40;

/// The highest attribute, vital or skill a profile gives.
pub const MAX_STAT: u32 = 1_000_000;

/// The most skills one profile names.
pub const MAX_SKILLS: usize = 64;

/// The highest skill number.
pub const MAX_SKILL_ID: u32 = 255;

/// The most augmentations one profile names.
pub const MAX_AUGMENTATIONS: usize = 64;

/// The highest augmentation number.
pub const MAX_AUGMENTATION_ID: u32 = 65_535;

/// The most times one augmentation may have been taken.
pub const MAX_AUGMENTATION_TIMES: u32 = 1_000;

/// The largest amount a profile gives (experience, luminance, a time in seconds): 2^53 - 1, the
/// largest whole number a page's script reads exactly.
pub const MAX_AMOUNT: u64 = (1 << 53) - 1;

/// The largest count a profile gives (skill credits, deaths, the chess rank, the fishing skill,
/// titles).
pub const MAX_COUNT: u32 = 1_000_000;

/// The largest allegiance.
pub const MAX_MEMBERS: u32 = 100_000;

/// The highest allegiance rank.
pub const MAX_RANK: u32 = 100;

/// The shortest and longest allegiance key, in characters of `A-Z`, `a-z`, `0-9`, `-` and `_`.
pub const MIN_ALLEGIANCE_KEY_CHARS: usize = 8;
/// See [`MIN_ALLEGIANCE_KEY_CHARS`].
pub const MAX_ALLEGIANCE_KEY_CHARS: usize = 64;

/// The least time between two listings a world sends for one character, in seconds, but for one at
/// its logout and one that takes it out of the directory, which go at once. Changes in between
/// wait, and only the latest goes.
pub const LISTING_EVERY_SECS: f64 = 300.0;

/// Listings of one character the hub takes; one that takes the character out is not held to it.
/// The burst leaves room for a player going on the network, off and on again.
pub const LISTINGS_PER_CHARACTER: Rate = Rate {
    burst: 5.0,
    per_sec: 1.0 / 60.0,
};

/// Listings of one world's characters the hub takes, all of them together.
pub const LISTINGS_PER_WORLD: Rate = Rate {
    burst: 50.0,
    per_sec: 10.0,
};

/// A listed character's profile, as its world sends it and the directory publishes it. Every
/// field but the first three may be left out; the hub publishes only these fields.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub level: u32,
    /// The heritage group, 1 to [`MAX_HERITAGE`].
    pub heritage: u8,
    /// 1, male, or 2, female.
    pub gender: u8,
    /// The title the character shows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub experience: Option<Experience>,
    /// Skill credits not yet spent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credits: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub luminance: Option<Luminance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attributes: Option<Attributes>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vitals: Option<Vitals>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<Skill>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub augmentations: Vec<Augmentation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allegiance: Option<Allegiance>,
    /// When the character was made, in seconds since 1970: only while its options let others see
    /// its date of birth ([`OthersSee`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub birth: Option<u64>,
    /// How long it has been played, in seconds: only while others may see its age.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub age: Option<u64>,
    /// How many times it has died: only while others may see its number of deaths.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deaths: Option<u32>,
    /// Its chess rank: only while others may see it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chess: Option<u32>,
    /// Its fishing skill: only while others may see it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fishing: Option<u32>,
    /// How many titles it has earned: only while others may see its number of titles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub titles: Option<u32>,
}

/// Experience earned in all, and not yet spent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Experience {
    pub total: u64,
    pub unassigned: u64,
}

/// Luminance to spend, and the most the character may hold.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Luminance {
    pub available: u64,
    pub maximum: u64,
}

/// One attribute: the value the character was made with, and its value with the ranks raised
/// since, unbuffed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attribute {
    pub innate: u32,
    pub base: u32,
}

/// The six attributes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attributes {
    pub strength: Attribute,
    pub endurance: Attribute,
    pub coordination: Attribute,
    pub quickness: Attribute,
    pub focus: Attribute,
    #[serde(rename = "self")]
    pub self_: Attribute,
}

/// One vital's maximum, unbuffed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vital {
    pub base: u32,
}

/// The three vitals.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vitals {
    pub health: Vital,
    pub stamina: Vital,
    pub mana: Vital,
}

/// How far a skill is trained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Training {
    Untrained,
    Trained,
    Specialized,
}

/// One skill: its number, as the game numbers its skills, how far it is trained, and its level
/// unbuffed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skill {
    pub id: u32,
    pub training: Training,
    pub base: u32,
}

/// One augmentation: the number of the integer property the game keeps it in, and how many times
/// the character has taken it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Augmentation {
    pub id: u32,
    pub times: u32,
}

/// The character's allegiance, as far as the directory shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Allegiance {
    /// The allegiance's opaque key: stable while its monarch is, and derived from nothing a player
    /// could see in game.
    pub key: String,
    /// The allegiance's name, only when its monarch gave it one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// How many characters the allegiance holds, listed or not.
    pub members: u32,
    /// The character's rank in it.
    pub rank: u32,
    /// The character's patron, only while that character is listed too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patron: Option<Address>,
    /// The allegiance's monarch, only while that character is listed too, and never the
    /// character itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monarch: Option<Address>,
}

/// The final retail client's "Allow others to see your ..." character options, as a character has
/// them set. Each lets others see one of its fields when they appraise it, and the directory
/// follows them: a field its option keeps hidden is never in its profile.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OthersSee {
    /// Allow others to see your date of birth.
    pub date_of_birth: bool,
    /// Allow others to see your age.
    pub age: bool,
    /// Allow others to see your chess rank.
    pub chess_rank: bool,
    /// Allow others to see your fishing skill.
    pub fishing_skill: bool,
    /// Allow others to see your number of deaths.
    pub number_of_deaths: bool,
    /// Allow others to see your number of titles.
    pub number_of_titles: bool,
}

/// The six fields [`OthersSee`] governs, as the world keeps them for a character.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Seeable {
    pub birth: Option<u64>,
    pub age: Option<u64>,
    pub chess: Option<u32>,
    pub fishing: Option<u32>,
    pub deaths: Option<u32>,
    pub titles: Option<u32>,
}

impl Profile {
    /// Sets the six fields a character's options govern: each to its value where `allowed` lets
    /// others see it, and out of the profile where it does not.
    pub fn set_seeable(&mut self, values: Seeable, allowed: OthersSee) {
        self.birth = values.birth.filter(|_| allowed.date_of_birth);
        self.age = values.age.filter(|_| allowed.age);
        self.chess = values.chess.filter(|_| allowed.chess_rank);
        self.fishing = values.fishing.filter(|_| allowed.fishing_skill);
        self.deaths = values.deaths.filter(|_| allowed.number_of_deaths);
        self.titles = values.titles.filter(|_| allowed.number_of_titles);
    }

    /// The bounds of every field, for a profile of `who`: why it is out of them, if it is.
    ///
    /// # Errors
    /// The first field out of its bounds.
    pub fn check(&self, who: &Address) -> Result<(), &'static str> {
        if !(1..=MAX_LEVEL).contains(&self.level) {
            return Err("level");
        }
        if !(1..=MAX_HERITAGE).contains(&self.heritage) {
            return Err("heritage");
        }
        if !(1..=2).contains(&self.gender) {
            return Err("gender");
        }
        if self
            .title
            .as_deref()
            .is_some_and(|t| !is_shown_text(t, MAX_TITLE_CHARS))
        {
            return Err("title");
        }
        if self
            .experience
            .is_some_and(|e| e.total > MAX_AMOUNT || e.unassigned > MAX_AMOUNT)
        {
            return Err("experience");
        }
        if self
            .luminance
            .is_some_and(|l| l.available > MAX_AMOUNT || l.maximum > MAX_AMOUNT)
        {
            return Err("luminance");
        }
        if let Some(a) = &self.attributes {
            let all = [
                a.strength,
                a.endurance,
                a.coordination,
                a.quickness,
                a.focus,
                a.self_,
            ];
            if all.iter().any(|x| x.innate > MAX_STAT || x.base > MAX_STAT) {
                return Err("attributes");
            }
        }
        if let Some(v) = &self.vitals {
            if [v.health, v.stamina, v.mana]
                .iter()
                .any(|x| x.base > MAX_STAT)
            {
                return Err("vitals");
            }
        }
        if self.skills.len() > MAX_SKILLS
            || self
                .skills
                .iter()
                .any(|s| !(1..=MAX_SKILL_ID).contains(&s.id) || s.base > MAX_STAT)
            || !distinct(self.skills.iter().map(|s| s.id))
        {
            return Err("skills");
        }
        if self.augmentations.len() > MAX_AUGMENTATIONS
            || self.augmentations.iter().any(|a| {
                !(1..=MAX_AUGMENTATION_ID).contains(&a.id)
                    || !(1..=MAX_AUGMENTATION_TIMES).contains(&a.times)
            })
            || !distinct(self.augmentations.iter().map(|a| a.id))
        {
            return Err("augmentations");
        }
        if let Some(a) = &self.allegiance {
            a.check(who)?;
        }
        if [self.birth, self.age]
            .iter()
            .flatten()
            .any(|&n| n > MAX_AMOUNT)
        {
            return Err("birth or age");
        }
        if [
            self.credits,
            self.deaths,
            self.chess,
            self.fishing,
            self.titles,
        ]
        .iter()
        .flatten()
        .any(|&n| n > MAX_COUNT)
        {
            return Err("a count");
        }
        Ok(())
    }
}

impl Allegiance {
    fn check(&self, who: &Address) -> Result<(), &'static str> {
        if !is_allegiance_key(&self.key) {
            return Err("allegiance key");
        }
        if self
            .name
            .as_deref()
            .is_some_and(|n| !is_shown_text(n, MAX_ALLEGIANCE_NAME_CHARS))
        {
            return Err("allegiance name");
        }
        if !(1..=MAX_MEMBERS).contains(&self.members) || !(1..=MAX_RANK).contains(&self.rank) {
            return Err("allegiance size or rank");
        }
        // Allegiances never span worlds, and a character is neither its own patron nor its own
        // monarch.
        let other_member =
            |a: &Address| !a.is_channel() && a.world() == who.world() && !a.same(who);
        if !self.patron.iter().chain(&self.monarch).all(other_member) {
            return Err("allegiance patron or monarch");
        }
        Ok(())
    }
}

/// Whether every value `values` gives is different.
fn distinct(values: impl Iterator<Item = u32>) -> bool {
    let mut seen = std::collections::BTreeSet::new();
    values.into_iter().all(|v| seen.insert(v))
}

/// Whether `s` is an allegiance key: [`MIN_ALLEGIANCE_KEY_CHARS`] to [`MAX_ALLEGIANCE_KEY_CHARS`]
/// of `A-Z`, `a-z`, `0-9`, `-` and `_`.
#[must_use]
pub fn is_allegiance_key(s: &str) -> bool {
    (MIN_ALLEGIANCE_KEY_CHARS..=MAX_ALLEGIANCE_KEY_CHARS).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Whether `s` may be shown as a title or an allegiance name: 1 to `max` characters, with no white
/// space at either end, no hidden character ([`crate::text::is_hidden`]) and no angle bracket.
/// Text that breaks the rule is refused, never cut or cleaned.
#[must_use]
pub fn is_shown_text(s: &str, max: usize) -> bool {
    let n = s.chars().count();
    (1..=max).contains(&n)
        && s.trim() == s
        && s.chars()
            .all(|c| !crate::text::is_hidden(c) && c != '<' && c != '>')
}

/// How a listing ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListingOutcome {
    /// Done: the profile is published as sent, or the character is out of the directory.
    Ok,
    /// The hub will not list this character now; `reason` says why.
    Refused,
    /// Over the hub's pace for listings: the world sends it again later.
    RateLimited,
    /// This hub keeps no directory.
    Unavailable,
}

/// Why the hub refused a listing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    /// The hub's operator took the character out of the directory, for a while.
    TakenDown,
    /// The directory holds as many of the world's characters as it may.
    Full,
}

/// Who took a character out of the directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OffBy {
    /// The character's person, forgetting itself on the site: its world takes the character off
    /// the network, as `@dn off` does. A world of the network's first version takes it out of the
    /// directory alone.
    Person,
    /// The hub's operator: its world holds the character out of the directory until its next
    /// login, and it stays on the network.
    Operator,
}

/// One of the directory's link messages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DirectoryMessage {
    /// World to hub: `who` is listed, with its profile as it is now (`on`), or it is listed no
    /// longer. A listing sets what the directory holds, so one repeated changes nothing.
    #[serde(rename = "listing")]
    Listing {
        id: String,
        who: Address,
        on: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        profile: Option<Box<Profile>>,
    },
    /// Hub to world: how a listing ended, and the character's page in the directory once it is
    /// listed.
    #[serde(rename = "listing.result")]
    ListingResult {
        id: String,
        outcome: ListingOutcome,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<Refusal>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
    /// Hub to world: `who` was taken out of the directory, so its world sends its listing no more
    /// for now (taken off the network, by its person), and says so with a listing that is not
    /// `on`.
    #[serde(rename = "listing.off")]
    ListingOff { who: Address, by: OffBy },
}

/// Every `type` of the directory's messages.
pub const TYPES: &[&str] = &["listing", "listing.result", "listing.off"];

impl DirectoryMessage {
    /// Reads one frame.
    ///
    /// # Errors
    /// [`WireError::Unknown`] for a type that is not one of the directory's,
    /// [`WireError::Malformed`] for anything else that is not one of its messages.
    pub fn from_json(text: &str) -> Result<Self, WireError> {
        if text.len() > MAX_MESSAGE_BYTES {
            return Err(WireError::Malformed("longer than the limit".into()));
        }
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|e| WireError::Malformed(e.to_string()))?;
        let Some(kind) = value.get("type").and_then(serde_json::Value::as_str) else {
            return Err(WireError::Malformed("no type".into()));
        };
        if !TYPES.contains(&kind) {
            return Err(WireError::Unknown(kind.to_owned()));
        }
        let kind = kind.to_owned();
        let m: Self = serde_json::from_value(value)
            .map_err(|e| WireError::Malformed(format!("{kind}: {e}")))?;
        m.check()
            .map_err(|why| WireError::Malformed(format!("{kind}: {why}")))?;
        Ok(m)
    }

    /// Writes one frame.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| String::from("{}"))
    }

    /// The message's `type`.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Listing { .. } => "listing",
            Self::ListingResult { .. } => "listing.result",
            Self::ListingOff { .. } => "listing.off",
        }
    }

    /// The range checks serde does not make.
    fn check(&self) -> Result<(), &'static str> {
        let id_ok = |id: &str| !id.is_empty() && id.chars().count() <= MAX_ID_CHARS;
        match self {
            Self::Listing {
                id,
                who,
                on,
                profile,
            } => {
                if !id_ok(id) {
                    return Err("id");
                }
                if who.is_channel() {
                    return Err("a channel is not a character");
                }
                match (on, profile) {
                    (true, Some(p)) => p.check(who),
                    (true, None) => Err("a listing that is on carries the profile"),
                    (false, Some(_)) => Err("a listing that is off carries no profile"),
                    (false, None) => Ok(()),
                }
            }
            Self::ListingResult {
                id,
                outcome,
                reason,
                url,
            } => {
                if !id_ok(id) {
                    return Err("id");
                }
                if reason.is_some() && *outcome != ListingOutcome::Refused {
                    return Err("a reason is given for a refusal only");
                }
                if url
                    .as_deref()
                    .is_some_and(|u| !crate::person::is_page_url(u))
                {
                    return Err("url");
                }
                Ok(())
            }
            Self::ListingOff { who, .. } => {
                if who.is_channel() {
                    return Err("a channel is not a character");
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Message;

    fn a(s: &str) -> Address {
        Address::parse(s).expect("an address")
    }

    /// A profile with a value in every field.
    fn full() -> Profile {
        let attribute = Attribute {
            innate: 100,
            base: 290,
        };
        Profile {
            level: 126,
            heritage: 1,
            gender: 1,
            title: Some("the Bloodletter".into()),
            experience: Some(Experience {
                total: 191_226_310_247,
                unassigned: 1_000,
            }),
            credits: Some(0),
            luminance: Some(Luminance {
                available: 5_000,
                maximum: 1_500_000,
            }),
            attributes: Some(Attributes {
                strength: attribute,
                endurance: attribute,
                coordination: attribute,
                quickness: attribute,
                focus: attribute,
                self_: attribute,
            }),
            vitals: Some(Vitals {
                health: Vital { base: 400 },
                stamina: Vital { base: 500 },
                mana: Vital { base: 450 },
            }),
            skills: vec![Skill {
                id: 6,
                training: Training::Specialized,
                base: 420,
            }],
            augmentations: vec![Augmentation { id: 218, times: 1 }],
            allegiance: Some(Allegiance {
                key: "k3Yq0pW9xLmN".into(),
                name: Some("Order of the Dawn".into()),
                members: 41,
                rank: 3,
                patron: Some(a("Cara@coldeve")),
                monarch: Some(a("Eli@coldeve")),
            }),
            birth: Some(1_083_412_800),
            age: Some(3_600_000),
            deaths: Some(12),
            chess: Some(1_400),
            fishing: Some(10),
            titles: Some(37),
        }
    }

    fn listing(profile: Profile) -> DirectoryMessage {
        DirectoryMessage::Listing {
            id: "l1".into(),
            who: a("Sir Bob@coldeve"),
            on: true,
            profile: Some(Box::new(profile)),
        }
    }

    fn malformed(m: &DirectoryMessage) -> bool {
        matches!(
            DirectoryMessage::from_json(&m.to_json()),
            Err(WireError::Malformed(_))
        )
    }

    /// Spec: link messages: every directory message round-trips through its JSON form, a profile's
    /// every field with it.
    #[test]
    fn every_directory_message_round_trips_through_its_json() {
        let all = [
            listing(full()),
            DirectoryMessage::Listing {
                id: "l2".into(),
                who: a("Sir Bob@coldeve"),
                on: false,
                profile: None,
            },
            DirectoryMessage::ListingResult {
                id: "l1".into(),
                outcome: ListingOutcome::Ok,
                reason: None,
                url: Some(
                    "https://dereth.network/directory/character/?a=Sir%20Bob%40coldeve".into(),
                ),
            },
            DirectoryMessage::ListingResult {
                id: "l1".into(),
                outcome: ListingOutcome::Refused,
                reason: Some(Refusal::TakenDown),
                url: None,
            },
            DirectoryMessage::ListingOff {
                who: a("Sir Bob@coldeve"),
                by: OffBy::Operator,
            },
            DirectoryMessage::ListingOff {
                who: a("Sir Bob@coldeve"),
                by: OffBy::Person,
            },
        ];
        for m in all {
            let json = m.to_json();
            assert!(
                json.contains(&format!("\"type\":\"{}\"", m.kind())),
                "{json}"
            );
            assert_eq!(DirectoryMessage::from_json(&json), Ok(m), "{json}");
        }
    }

    /// Spec: link messages: a reader that knows only the other link messages skips a directory
    /// message as a type it does not know, and the directory's reader skips theirs.
    #[test]
    fn a_reader_without_the_directory_skips_its_messages() {
        let frame = listing(full()).to_json();
        assert_eq!(
            Message::from_json(&frame),
            Err(WireError::Unknown("listing".into()))
        );
        assert_eq!(
            DirectoryMessage::from_json(r#"{"type":"tell.result","id":"t1","outcome":"offline"}"#),
            Err(WireError::Unknown("tell.result".into()))
        );
    }

    /// Spec: link messages: a profile out of any field's bounds is malformed, refused and never
    /// cut.
    #[test]
    fn a_profile_out_of_its_bounds_is_malformed() {
        type Change = Box<dyn Fn(&mut Profile)>;
        let out_of_bounds: Vec<(&str, Change)> = vec![
            ("level 0", Box::new(|p| p.level = 0)),
            ("level", Box::new(|p| p.level = MAX_LEVEL + 1)),
            ("heritage", Box::new(|p| p.heritage = MAX_HERITAGE + 1)),
            ("gender", Box::new(|p| p.gender = 3)),
            (
                "a long title",
                Box::new(|p| p.title = Some("x".repeat(MAX_TITLE_CHARS + 1))),
            ),
            (
                "a title with a hidden character",
                Box::new(|p| p.title = Some("the \u{202E}Bold".into())),
            ),
            (
                "a title with markup",
                Box::new(|p| p.title = Some("<Tell:1>x".into())),
            ),
            (
                "a title with a space at its end",
                Box::new(|p| p.title = Some("the Bold ".into())),
            ),
            (
                "an empty title",
                Box::new(|p| p.title = Some(String::new())),
            ),
            (
                "experience",
                Box::new(|p| {
                    p.experience = Some(Experience {
                        total: MAX_AMOUNT + 1,
                        unassigned: 0,
                    });
                }),
            ),
            (
                "an attribute",
                Box::new(|p| {
                    if let Some(a) = p.attributes.as_mut() {
                        a.self_.base = MAX_STAT + 1;
                    }
                }),
            ),
            (
                "a skill number",
                Box::new(|p| {
                    p.skills.push(Skill {
                        id: 0,
                        training: Training::Trained,
                        base: 10,
                    });
                }),
            ),
            (
                "a skill twice",
                Box::new(|p| {
                    p.skills.push(Skill {
                        id: 6,
                        training: Training::Trained,
                        base: 10,
                    });
                }),
            ),
            (
                "too many skills",
                Box::new(|p| {
                    p.skills = (1..=u32::try_from(MAX_SKILLS + 1).unwrap_or(0))
                        .map(|id| Skill {
                            id,
                            training: Training::Trained,
                            base: 10,
                        })
                        .collect();
                }),
            ),
            (
                "an augmentation taken no times",
                Box::new(|p| p.augmentations[0].times = 0),
            ),
            (
                "an allegiance key",
                Box::new(|p| {
                    if let Some(al) = p.allegiance.as_mut() {
                        al.key = "short".into();
                    }
                }),
            ),
            (
                "a patron on another world",
                Box::new(|p| {
                    if let Some(al) = p.allegiance.as_mut() {
                        al.patron = Some(a("Cara@frost"));
                    }
                }),
            ),
            (
                "the character its own monarch",
                Box::new(|p| {
                    if let Some(al) = p.allegiance.as_mut() {
                        al.monarch = Some(a("sir bob@coldeve"));
                    }
                }),
            ),
            (
                "an allegiance of nobody",
                Box::new(|p| {
                    if let Some(al) = p.allegiance.as_mut() {
                        al.members = 0;
                    }
                }),
            ),
            ("deaths", Box::new(|p| p.deaths = Some(MAX_COUNT + 1))),
            ("birth", Box::new(|p| p.birth = Some(MAX_AMOUNT + 1))),
        ];
        assert!(!malformed(&listing(full())), "the full profile is within");
        for (what, change) in out_of_bounds {
            let mut p = full();
            change(&mut p);
            assert!(malformed(&listing(p)), "{what}");
        }
    }

    /// Spec: link messages: a listing that is on carries the profile and one that is off carries
    /// none; a result gives a reason for a refusal only; a channel is never listed.
    #[test]
    fn a_listing_carries_a_profile_only_while_it_is_on() {
        let off_with_profile = DirectoryMessage::Listing {
            id: "l".into(),
            who: a("Bob@coldeve"),
            on: false,
            profile: Some(Box::new(full())),
        };
        let on_without = DirectoryMessage::Listing {
            id: "l".into(),
            who: a("Bob@coldeve"),
            on: true,
            profile: None,
        };
        let reason_for_ok = DirectoryMessage::ListingResult {
            id: "l".into(),
            outcome: ListingOutcome::Ok,
            reason: Some(Refusal::Full),
            url: None,
        };
        for m in [off_with_profile, on_without, reason_for_ok] {
            assert!(malformed(&m), "{m:?}");
        }
        for bad in [
            r#"{"type":"listing","id":"l","who":"oldguard@channel","on":false}"#,
            r#"{"type":"listing.off","who":"oldguard@channel","by":"operator"}"#,
            r#"{"type":"listing.off","who":"Bob@coldeve","by":"someone"}"#,
            r#"{"type":"listing.result","id":"l","outcome":"refused","reason":"unlisted"}"#,
            r#"{"type":"listing.result","id":"l","outcome":"ok","url":"https://x/ <b>"}"#,
            r#"{"type":"listing","id":"","who":"Bob@coldeve","on":false}"#,
        ] {
            assert!(
                matches!(
                    DirectoryMessage::from_json(bad),
                    Err(WireError::Malformed(_))
                ),
                "{bad}"
            );
        }
    }

    /// Spec: link messages: the largest listing within every bound, in the letters JSON writes
    /// longest, fits one frame.
    #[test]
    fn the_largest_listing_fits_one_frame() {
        let longest = |tag: &str| {
            let name = format!("{}{tag}", "\u{160}".repeat(32 - tag.len()));
            a(&format!("{name}@abcdefghijkl"))
        };
        let mut p = full();
        p.level = MAX_LEVEL;
        p.title = Some("\u{20AC}".repeat(MAX_TITLE_CHARS));
        p.experience = Some(Experience {
            total: MAX_AMOUNT,
            unassigned: MAX_AMOUNT,
        });
        p.luminance = Some(Luminance {
            available: MAX_AMOUNT,
            maximum: MAX_AMOUNT,
        });
        p.skills = (1..=u32::try_from(MAX_SKILLS).unwrap_or(0))
            .map(|id| Skill {
                id: MAX_SKILL_ID + 1 - id,
                training: Training::Specialized,
                base: MAX_STAT,
            })
            .collect();
        p.augmentations = (1..=u32::try_from(MAX_AUGMENTATIONS).unwrap_or(0))
            .map(|id| Augmentation {
                id: MAX_AUGMENTATION_ID + 1 - id,
                times: MAX_AUGMENTATION_TIMES,
            })
            .collect();
        p.allegiance = Some(Allegiance {
            key: "k".repeat(MAX_ALLEGIANCE_KEY_CHARS),
            name: Some("\u{20AC}".repeat(MAX_ALLEGIANCE_NAME_CHARS)),
            members: MAX_MEMBERS,
            rank: MAX_RANK,
            patron: Some(longest("a")),
            monarch: Some(longest("b")),
        });
        let m = DirectoryMessage::Listing {
            id: "i".repeat(MAX_ID_CHARS),
            who: longest("c"),
            on: true,
            profile: Some(Box::new(p)),
        };
        let json = m.to_json();
        assert!(json.len() <= MAX_MESSAGE_BYTES, "{} bytes", json.len());
        assert_eq!(DirectoryMessage::from_json(&json), Ok(m));
    }

    /// Spec: settings: a field a character's "Allow others to see your ..." option keeps hidden is
    /// never in its profile; one it allows is.
    #[test]
    fn a_field_its_option_hides_is_never_in_the_profile() {
        let values = Seeable {
            birth: Some(1_083_412_800),
            age: Some(3_600_000),
            chess: Some(1_400),
            fishing: Some(10),
            deaths: Some(12),
            titles: Some(37),
        };
        let mut p = full();
        p.set_seeable(values, OthersSee::default());
        let none = serde_json::to_value(&p).expect("written");
        for field in ["birth", "age", "chess", "fishing", "deaths", "titles"] {
            assert!(none.get(field).is_none(), "{field} with every option off");
        }
        let one_each = [
            (
                "birth",
                OthersSee {
                    date_of_birth: true,
                    ..OthersSee::default()
                },
            ),
            (
                "age",
                OthersSee {
                    age: true,
                    ..OthersSee::default()
                },
            ),
            (
                "chess",
                OthersSee {
                    chess_rank: true,
                    ..OthersSee::default()
                },
            ),
            (
                "fishing",
                OthersSee {
                    fishing_skill: true,
                    ..OthersSee::default()
                },
            ),
            (
                "deaths",
                OthersSee {
                    number_of_deaths: true,
                    ..OthersSee::default()
                },
            ),
            (
                "titles",
                OthersSee {
                    number_of_titles: true,
                    ..OthersSee::default()
                },
            ),
        ];
        for (field, allowed) in one_each {
            let mut p = full();
            p.set_seeable(values, allowed);
            let v = serde_json::to_value(&p).expect("written");
            let present: Vec<&str> = ["birth", "age", "chess", "fishing", "deaths", "titles"]
                .into_iter()
                .filter(|f| v.get(*f).is_some())
                .collect();
            assert_eq!(present, [field], "only what its option allows");
        }
    }
}
