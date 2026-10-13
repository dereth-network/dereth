//! Not ACE: the network's directory, the world's half (`docs/network/03-world-behaviour.md`
//! section 10).
//!
//! A character on the network has its profile in the network's directory. The world composes the
//! profile, sends it to the hub when it changes, and takes it out when the player takes the
//! character off the network, or the hub's operator takes it out:
//!
//! - **The profile** is the character's own: level, heritage, gender, title, experience, skill
//!   credits, luminance, attributes, vitals, skills and augmentations, all without the effects of
//!   spells, and its allegiance. The six fields the character's "Allow others to see your ..."
//!   options govern go only while those options allow, the test appraising it makes.
//! - **The allegiance** goes under a key the world keeps for each monarch, a random value derived
//!   from nothing seen in game; with its name only when the monarch gave it one, never the
//!   monarch's name in its place; and with its patron and monarch only while each is listed too.
//! - **When.** At `/dn on` and at each login, at once; a change while the character plays at most
//!   once every [`LISTING_EVERY_SECS`], the latest profile then (its age, which grows every second
//!   it plays, goes with its next other change); at logout at once, if it changed. The listings of
//!   each character and of the world together are paced under the hub's paces for listings and the
//!   link's own. When a character comes onto the network or leaves it, the listings of its listed
//!   vassals (and, for a monarch, of every listed member) go again, so that their links to it
//!   appear or go.
//! - **Held out.** A character the hub's operator took out, or one the directory has no room for,
//!   is sent no more until its next login, when the world asks again; the hub refuses it while it
//!   holds it out. The player is told once: when the operator takes it out (at once, or at its next
//!   login), or when `/dn on` is refused.
//! - **What is kept.** Each listed character's latest profile, whether the hub has answered it,
//!   and every listing that takes a character out until the hub answers it, are saved with the
//!   network's records ([`super::NetworkStore`]), so that a lost link, a restart or a crash never
//!   leaves a character in the directory its player took off the network. They go again after the
//!   next welcome, the ones that take characters out first.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use dereth_network_protocol::directory::{
    self as dir, Allegiance, Attribute, Attributes, Augmentation, DirectoryMessage, Experience,
    ListingOutcome, Luminance, OffBy, OthersSee, Profile, Refusal, Seeable, Skill, Training, Vital,
    Vitals, LISTINGS_PER_CHARACTER, LISTINGS_PER_WORLD, LISTING_EVERY_SECS,
};
use dereth_network_protocol::limits::{Bucket, Rate};
use dereth_network_protocol::Address;
use empyrean_entity::enums::{
    CharacterOption, CharacterTitle, PropertyAttribute, PropertyAttribute2nd, PropertyInt,
    PropertyInt64, PropertyString, SkillAdvancementClass,
};
use empyrean_entity::ObjectGuid;
use serde::{Deserialize, Serialize};

use super::{enabled, existing_record_mut, fresh_id, now, own_address, tell_player};
use crate::entity::i_player;
use crate::managers::player_manager;
use crate::world_objects::allegiance;
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::world_object_networking::shims;
use crate::World;

/// How often the profiles of the listed characters online are looked over for a change, in
/// seconds. A change shows at most this long after the character's five minutes are up.
pub const LOOK_EVERY_SECS: f64 = 30.0;

/// How long a listing the hub held back to its pace waits before it goes again, in seconds.
pub const AGAIN_SECS: f64 = 60.0;

/// The integer properties the game keeps the augmentations in, whose numbers a profile's
/// augmentations carry: the attribute, skill, resistance, combat and magic augmentations, and the
/// ones bought with luminance.
pub const AUGMENTATIONS: &[std::ops::RangeInclusive<u16>] = &[
    218..=238,
    240..=246,
    293..=302,
    309..=310,
    326..=328,
    333..=345,
    365..=365,
];

/// The line for a character the hub's operator took out of the directory, or refused to list.
pub const TAKEN_DOWN: &str =
    "The Dereth Network's operator has taken this character out of the directory for now.";

/// The line for a character its player took off the network by asking the Dereth Network to forget
/// them.
pub const FORGOTTEN: &str = "You asked the Dereth Network to forget you, so this character is off the network now. Type @dn on to join it again.";

/// The line for a character the directory has no room for.
pub const FULL: &str =
    "The Dereth Network's directory holds as many of this world's characters as it may.";

/// The line for a listing on a hub that keeps no directory.
pub const UNAVAILABLE: &str =
    "This network's hub keeps no directory, so your profile is not in one.";

/// What the world keeps of a listed character, saved with its record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Listing {
    /// The address it is listed under.
    pub who: Address,
    /// The latest profile sent, or the one due to go.
    pub profile: Profile,
    /// The hub has not yet answered this profile.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub owed: bool,
    /// The character's page in the directory, as the hub last named it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
}

/// Why a character was taken out of the directory while it was away, to tell it at its next
/// login. A world that kept other reasons before reads them as the operator's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TakenOut {
    /// The hub's operator took it out, for a while.
    #[serde(alias = "taken_down", alias = "person")]
    Operator,
    /// The directory holds as many of this world's characters as it may.
    Full,
    /// Its player asked the Dereth Network to forget them: it is off the network.
    Forgotten,
}

impl TakenOut {
    fn line(self) -> &'static str {
        match self {
            Self::Operator => TAKEN_DOWN,
            Self::Full => FULL,
            Self::Forgotten => FORGOTTEN,
        }
    }
}

/// When a listed character's profile goes next.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Due {
    /// Not before this.
    at: f64,
    /// It answers `/dn on`: the hub's answer is shown.
    announce: bool,
}

/// A listing handed to the link and not yet answered.
#[derive(Debug, Clone)]
struct Sent {
    /// The character it lists; none for one that takes a character out.
    character: Option<u32>,
    /// The identity key of the address it names.
    key: String,
    announce: bool,
}

/// The directory's state on the world, beside what is saved.
#[derive(Debug, Default)]
pub(super) struct DirectoryState {
    /// The characters whose profile is due.
    due: BTreeMap<u32, Due>,
    /// Listings handed to the link and not yet answered, by id.
    sent: HashMap<String, Sent>,
    /// The id of each character's latest listing.
    latest: HashMap<u32, String>,
    /// Listings that take a character out, on the link and not yet answered, by identity key.
    removing: BTreeSet<String>,
    /// Those the hub held back to its pace, and when they may go again.
    removals_wait: HashMap<String, f64>,
    /// When each character's last listing went.
    last: HashMap<u32, f64>,
    /// Each character's listings, paced as the hub paces them.
    paces: HashMap<u32, Bucket>,
    /// The world's listings, paced as the hub paces them.
    pace: Option<Bucket>,
    next_look: f64,
    /// The characters the hub holds out of the directory, sent no listing until their next login.
    held_out: BTreeSet<u32>,
}

/// A tenth under the hub's own `rate`, so the two clocks never disagree at the edge.
fn under(rate: Rate) -> Rate {
    Rate {
        burst: (rate.burst * 0.9).max(1.0),
        per_sec: rate.per_sec * 0.9,
    }
}

/// Whether `character` is listed: on the network, and not held out by the hub.
fn listed(w: &World, character: u32) -> bool {
    !w.federation.directory.held_out.contains(&character)
        && w.federation
            .store
            .characters
            .get(&character)
            .is_some_and(|r| r.settings.on)
}

/// `character`'s address on this world, online or not.
fn address_of(w: &World, character: u32) -> Option<Address> {
    let p = player_manager::find_by_guid(w, character).0?;
    let name = i_player::name(w, p)?;
    Address::new(&name, &w.federation.settings.slug).ok()
}

/// The character of this world `who` names, online or not.
fn character_named(w: &World, who: &Address) -> Option<u32> {
    if who.world() != w.federation.settings.slug {
        return None;
    }
    player_manager::find_by_name(w, who.name())
        .0
        .map(|p| p.guid().full())
}

/// The key the directory knows `monarch`'s allegiance by: a random value the world keeps while the
/// monarch is the same, made the first time it is needed. None when the system's random source
/// fails, so that no key is ever made from anything else.
fn allegiance_key(w: &mut World, monarch: u32) -> Option<String> {
    if let Some(k) = w.federation.store.allegiance_keys.get(&monarch) {
        return Some(k.clone());
    }
    let mut bytes = [0u8; 16];
    if getrandom::fill(&mut bytes).is_err() {
        log::warn!("Dereth Network: the system's random source failed; no allegiance key made");
        return None;
    }
    let key: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    w.federation
        .store
        .allegiance_keys
        .insert(monarch, key.clone());
    w.federation.dirty = true;
    Some(key)
}

/// `character`'s allegiance as the directory shows it, from the allegiance the world has loaded:
/// its key, its name when its monarch gave it one, its size, the character's rank, and its patron
/// and monarch while each is listed. None for a character in no loaded allegiance.
fn allegiance_of(w: &mut World, character: u32) -> Option<Allegiance> {
    let g = ObjectGuid::new(character);
    let ag = *w.allegiance_manager.players.get(&g)?;
    let (monarch, patron, rank, members) = {
        let f = allegiance::fields_of(w, ag)?;
        let node = f.tree.node(*f.members.as_ref()?.get(&g)?);
        let monarch = f.tree.node(f.monarch?).player_guid.full();
        let patron = node.patron.map(|p| f.tree.node(p).player_guid.full());
        (
            monarch,
            patron,
            node.rank,
            u32::try_from(f.total_members()).ok()?,
        )
    };
    // The name the monarch gave it, never one made up for an unnamed allegiance.
    let name = w
        .objects
        .get(ag)
        .and_then(|o| o.get_property(PropertyString::AllegianceName))
        .filter(|n| dir::is_shown_text(n, dir::MAX_ALLEGIANCE_NAME_CHARS));
    let key = allegiance_key(w, monarch)?;
    let named = |w: &World, other: u32| {
        (other != character && listed(w, other))
            .then(|| address_of(w, other))
            .flatten()
    };
    let a = Allegiance {
        key,
        name,
        members,
        rank,
        patron: patron.and_then(|p| named(w, p)),
        monarch: named(w, monarch),
    };
    ((1..=dir::MAX_MEMBERS).contains(&a.members) && (1..=dir::MAX_RANK).contains(&a.rank))
        .then_some(a)
}

/// Whether a player lets others see each of the six fields its options govern, as appraising it
/// does: a character the world has no options for lets nobody see any.
fn others_see(w: &World, player: ObjectGuid) -> OthersSee {
    let has_options = w
        .objects
        .get(player)
        .is_some_and(|o| shims::player_character(o).is_some());
    if !has_options {
        return OthersSee::default();
    }
    let option = |o| crate::world_objects::player_character::get_character_option(w, player, o);
    OthersSee {
        date_of_birth: option(CharacterOption::AllowOthersToSeeYourDateOfBirth),
        age: option(CharacterOption::AllowOthersToSeeYourAge),
        chess_rank: option(CharacterOption::AllowOthersToSeeYourChessRank),
        fishing_skill: option(CharacterOption::AllowOthersToSeeYourFishingSkill),
        number_of_deaths: option(CharacterOption::AllowOthersToSeeYourNumberOfDeaths),
        number_of_titles: option(CharacterOption::AllowOthersToSeeYourNumberOfTitles),
    }
}

/// The profile of a player in the world, as its own state gives it now. None for one that cannot
/// be listed: no level, heritage or gender within the directory's bounds.
fn compose(w: &mut World, player: ObjectGuid) -> Option<Profile> {
    let allegiance = allegiance_of(w, player.full());
    let w: &World = w;
    let o = w.objects.get(player)?;
    let level = u32::try_from(o.get_property(PropertyInt::Level)?).ok()?;
    let heritage = u8::try_from(o.get_property(PropertyInt::HeritageGroup)?).ok()?;
    let gender = u8::try_from(o.get_property(PropertyInt::Gender)?).ok()?;
    let amount = |p| {
        o.get_property(p)
            .and_then(|n: i64| u64::try_from(n).ok())
            .filter(|&n| n <= dir::MAX_AMOUNT)
    };
    let count = |p| {
        o.get_property(p)
            .and_then(|n: i32| u32::try_from(n).ok())
            .filter(|&n| n <= dir::MAX_COUNT)
    };
    let secs = |p| o.get_property(p).and_then(|n: i32| u64::try_from(n).ok());
    let title = o
        .get_property(PropertyInt::CharacterTitleId)
        .and_then(|id| u32::try_from(id).ok())
        .filter(|&id| id > 0)
        .and_then(|id| crate::world_objects::player_character::get_title(w, CharacterTitle(id)))
        .filter(|t| dir::is_shown_text(t, dir::MAX_TITLE_CHARS));
    let mut profile = Profile {
        level,
        heritage,
        gender,
        title,
        experience: amount(PropertyInt64::TotalExperience).map(|total| Experience {
            total,
            unassigned: amount(PropertyInt64::AvailableExperience).unwrap_or(0),
        }),
        credits: count(PropertyInt::AvailableSkillCredits),
        luminance: amount(PropertyInt64::MaximumLuminance).map(|maximum| Luminance {
            available: amount(PropertyInt64::AvailableLuminance).unwrap_or(0),
            maximum,
        }),
        attributes: attributes(o),
        vitals: vitals(w, o),
        skills: skills(w, o),
        augmentations: augmentations(o),
        allegiance,
        ..Profile::default()
    };
    profile.set_seeable(
        Seeable {
            birth: secs(PropertyInt::CreationTimestamp),
            age: secs(PropertyInt::Age),
            chess: count(PropertyInt::ChessRank),
            fishing: count(PropertyInt::FakeFishingSkill),
            deaths: count(PropertyInt::NumDeaths),
            titles: count(PropertyInt::NumCharacterTitles),
        },
        others_see(w, player),
    );
    Some(profile)
}

/// The six attributes, as made and as raised since, without spells; none unless a creature has
/// all six within bounds.
fn attributes(o: &WorldObject) -> Option<Attributes> {
    o.creature.as_ref()?;
    let one = |a| {
        let c = o.attributes().get(&a)?;
        let v = Attribute {
            innate: c.starting_value(o),
            base: c.base(o),
        };
        (v.innate <= dir::MAX_STAT && v.base <= dir::MAX_STAT).then_some(v)
    };
    Some(Attributes {
        strength: one(PropertyAttribute::Strength)?,
        endurance: one(PropertyAttribute::Endurance)?,
        coordination: one(PropertyAttribute::Coordination)?,
        quickness: one(PropertyAttribute::Quickness)?,
        focus: one(PropertyAttribute::Focus)?,
        self_: one(PropertyAttribute::Self_)?,
    })
}

/// The three vitals' maximums without spells; none unless a creature has all three.
fn vitals(w: &World, o: &WorldObject) -> Option<Vitals> {
    o.creature.as_ref()?;
    let one = |v| {
        let base = o.vitals().get(&v)?.base(w, o);
        (base <= dir::MAX_STAT).then_some(Vital { base })
    };
    Some(Vitals {
        health: one(PropertyAttribute2nd::MaxHealth)?,
        stamina: one(PropertyAttribute2nd::MaxStamina)?,
        mana: one(PropertyAttribute2nd::MaxMana)?,
    })
}

/// The character's skills, untrained, trained or specialized, with their levels without spells,
/// in the order of their numbers.
fn skills(w: &World, o: &WorldObject) -> Vec<Skill> {
    if o.creature.is_none() {
        return Vec::new();
    }
    let mut out: Vec<Skill> = o
        .skills()
        .iter()
        .filter_map(|(s, c)| {
            let id = u32::try_from(s.0)
                .ok()
                .filter(|id| (1..=dir::MAX_SKILL_ID).contains(id))?;
            let training = match c.advancement_class(o) {
                SkillAdvancementClass::Untrained => Training::Untrained,
                SkillAdvancementClass::Trained => Training::Trained,
                SkillAdvancementClass::Specialized => Training::Specialized,
                _ => return None,
            };
            let base = c.base(w, o);
            (base <= dir::MAX_STAT).then_some(Skill { id, training, base })
        })
        .collect();
    out.sort_by_key(|s| s.id);
    out.dedup_by_key(|s| s.id);
    out.truncate(dir::MAX_SKILLS);
    out
}

/// The augmentations the character has taken, each with how many times.
fn augmentations(o: &WorldObject) -> Vec<Augmentation> {
    AUGMENTATIONS
        .iter()
        .flat_map(Clone::clone)
        .filter_map(|p| {
            let times = o
                .get_property(PropertyInt(p))
                .and_then(|n| u32::try_from(n).ok())
                .filter(|t| (1..=dir::MAX_AUGMENTATION_TIMES).contains(t))?;
            Some(Augmentation {
                id: u32::from(p),
                times,
            })
        })
        .collect()
}

/// A profile kept for a character not in the world, with its allegiance as it is now: from the
/// allegiance loaded, or failing that the kept one, naming a patron or monarch only while it is
/// still listed.
fn refreshed(w: &mut World, character: u32, mut profile: Profile) -> Profile {
    if w.allegiance_manager
        .players
        .contains_key(&ObjectGuid::new(character))
    {
        profile.allegiance = allegiance_of(w, character);
    } else if let Some(a) = profile.allegiance.as_mut() {
        let still = |w: &World, x: &Option<Address>| {
            x.clone()
                .filter(|x| character_named(w, x).is_some_and(|c| listed(w, c)))
        };
        a.patron = still(w, &a.patron);
        a.monarch = still(w, &a.monarch);
    }
    profile
}

/// The profile to list `character` with now, and the address it goes under: composed for a
/// character in the world, the kept one for one away. None for a character that is not listed
/// or cannot be.
fn listing_of(w: &mut World, character: u32) -> Option<(Address, Profile)> {
    if !listed(w, character) {
        return None;
    }
    let in_world = player_manager::get_online_player(w, character);
    let who = match in_world {
        Some(g) => own_address(w, g)?,
        None => address_of(w, character)?,
    };
    let kept = w
        .federation
        .store
        .characters
        .get(&character)
        .and_then(|r| r.listing.clone());
    // A character renamed since it was listed is taken out under the name it had, and listed under
    // its new one.
    let kept = match kept {
        Some(k) if !k.who.same(&who) => {
            let old = k.who.clone();
            forget(w, character);
            owe_removal(w, old);
            None
        }
        kept => kept,
    };
    let profile = match in_world {
        Some(g) => compose(w, g)?,
        None => refreshed(w, character, kept?.profile),
    };
    if let Err(why) = profile.check(&who) {
        log::warn!("Dereth Network: {who}'s profile is out of the directory's bounds ({why}); it is not listed");
        return None;
    }
    Some((who, profile))
}

/// Owes the hub a listing that takes `who` out of the directory, saved until the hub answers it.
fn owe_removal(w: &mut World, who: Address) {
    let key = who.key();
    w.federation.store.unlistings.insert(key.clone(), who);
    w.federation.directory.removals_wait.remove(&key);
    w.federation.dirty = true;
}

/// Forgets `character`'s listing: nothing of it is due or awaited.
fn forget(w: &mut World, character: u32) {
    if let Some(r) = existing_record_mut(w, character) {
        r.listing = None;
    }
    w.federation.dirty = true;
    w.federation.directory.due.remove(&character);
    w.federation.directory.latest.remove(&character);
}

/// The hub holds `character` out of the directory: its listing is forgotten, and none is sent until
/// its next login. Its followers' links to it go.
fn hold_out(w: &mut World, character: u32) {
    let was = listed(w, character);
    forget(w, character);
    w.federation.directory.held_out.insert(character);
    if was {
        links_changed(w, character);
    }
}

/// Shows `character` why it was taken out of the directory: at once while it is in the world, else
/// at its next login.
fn tell_taken_out(w: &mut World, character: u32, why: TakenOut) {
    match player_manager::get_online_player(w, character) {
        Some(g) => tell_player(w, g, why.line()),
        None => {
            if let Some(r) = existing_record_mut(w, character) {
                r.taken_out = Some(why);
                w.federation.dirty = true;
            }
        }
    }
}

/// Marks `character`'s profile due at `at`, unless it is due sooner.
fn due(w: &mut World, character: u32, at: f64, announce: bool) {
    let d = &mut w.federation.directory;
    let e = d.due.entry(character).or_insert(Due { at, announce });
    e.at = e.at.min(at);
    e.announce |= announce;
}

/// `character` came onto the directory or left it: the listings of its listed vassals, and of every
/// listed member when it is a monarch, go again, each within its own pace, so that their links to it appear or
/// go. Only an allegiance the world has loaded is known.
fn links_changed(w: &mut World, character: u32) {
    let g = ObjectGuid::new(character);
    let Some(&ag) = w.allegiance_manager.players.get(&g) else {
        return;
    };
    let followers: Vec<u32> = {
        let Some(f) = allegiance::fields_of(w, ag) else {
            return;
        };
        let Some(members) = f.members.as_ref() else {
            return;
        };
        let monarch = f.monarch.map(|m| f.tree.node(m).player_guid);
        members
            .iter()
            .filter(|(m, id)| {
                **m != g
                    && (monarch == Some(g)
                        || f.tree
                            .node(**id)
                            .patron
                            .is_some_and(|p| f.tree.node(p).player_guid == g))
            })
            .map(|(m, _)| m.full())
            .collect()
    };
    let t = now(w);
    for m in followers {
        if listed(w, m) {
            let at = w
                .federation
                .directory
                .last
                .get(&m)
                .map_or(t, |&l| t.max(l + LISTING_EVERY_SECS));
            due(w, m, at, false);
        }
    }
}

// ---- what the world's characters do -------------------------------------------------------------

/// `/dn on` for `player`: its listing goes at once, and the hub's answer is shown (its page, or why
/// it is not listed).
pub(super) fn announce(w: &mut World, player: ObjectGuid) {
    let character = player.full();
    // A listing supersedes one still owed that takes the character out.
    if let Some(who) = own_address(w, player) {
        w.federation.store.unlistings.remove(&who.key());
    }
    w.federation.directory.held_out.remove(&character);
    let t = now(w);
    due(w, character, t, true);
}

/// A character came online: it is told why it was taken out of the directory, or off the network,
/// while it was away, on the network or off it now.
pub(super) fn told_at_login(w: &mut World, player: ObjectGuid) {
    let told = existing_record_mut(w, player.full()).and_then(|r| r.taken_out.take());
    if let Some(why) = told {
        w.federation.dirty = true;
        tell_player(w, player, why.line());
    }
}

/// A character on the network came online, or onto the network: it is listed at once (the hub
/// refuses it while it holds it out).
pub(super) fn arrived(w: &mut World, player: ObjectGuid) {
    let character = player.full();
    w.federation.directory.held_out.remove(&character);
    if listed(w, character) {
        let t = now(w);
        due(w, character, t, false);
    }
}

/// `player` came back onto the network (`/dn on`): its followers' links to it appear.
pub(super) fn back_on(w: &mut World, player: ObjectGuid) {
    if listed(w, player.full()) {
        links_changed(w, player.full());
    }
}

/// A listed character left the world: its profile goes at once if it changed since its last
/// listing.
pub(super) fn left(w: &mut World, player: ObjectGuid) {
    let character = player.full();
    if !listed(w, character) {
        return;
    }
    let Some(who) = own_address(w, player) else {
        return;
    };
    let Some(profile) = compose(w, player) else {
        return;
    };
    let changed = w
        .federation
        .store
        .characters
        .get(&character)
        .and_then(|r| r.listing.as_ref())
        .is_none_or(|l| l.profile != profile || !l.who.same(&who));
    if !changed {
        return;
    }
    if let Some(r) = existing_record_mut(w, character) {
        let page = r
            .listing
            .as_ref()
            .filter(|l| l.who.same(&who))
            .and_then(|l| l.page.clone());
        r.listing = Some(Listing {
            who,
            profile,
            owed: true,
            page,
        });
    }
    w.federation.dirty = true;
    let t = now(w);
    due(w, character, t, false);
}

/// `player` went off the network (`/dn off`), its record already saying so: it is taken out of the
/// directory at once.
pub(super) fn went_off(w: &mut World, player: ObjectGuid) {
    let character = player.full();
    w.federation.directory.held_out.remove(&character);
    let who = w
        .federation
        .store
        .characters
        .get(&character)
        .and_then(|r| r.listing.as_ref().map(|l| l.who.clone()))
        .or_else(|| own_address(w, player));
    if let Some(r) = existing_record_mut(w, character) {
        r.listing = None;
    }
    w.federation.directory.due.remove(&character);
    if let Some(who) = who {
        owe_removal(w, who);
    }
    links_changed(w, character);
}

/// A character of this world was deleted (or is waiting to be): a listed one is taken out of the
/// directory.
pub fn deleted(w: &mut World, character: u32) {
    if !enabled(w) || !listed(w, character) {
        return;
    }
    let who = w
        .federation
        .store
        .characters
        .get(&character)
        .and_then(|r| r.listing.as_ref().map(|l| l.who.clone()))
        .or_else(|| address_of(w, character));
    forget(w, character);
    if let Some(who) = who {
        owe_removal(w, who);
    }
    links_changed(w, character);
}

/// A character of this world was renamed from `old_name`: a listed one is taken out of the
/// directory under its old name, and listed under its new one when it is next in the world.
pub fn renamed(w: &mut World, character: u32, old_name: &str) {
    if !enabled(w) || !listed(w, character) {
        return;
    }
    let who = w
        .federation
        .store
        .characters
        .get(&character)
        .and_then(|r| r.listing.as_ref().map(|l| l.who.clone()))
        .or_else(|| Address::new(old_name, &w.federation.settings.slug).ok());
    forget(w, character);
    if let Some(who) = who {
        owe_removal(w, who);
    }
    links_changed(w, character);
}

// ---- the link -----------------------------------------------------------------------------------

/// The link is welcomed: every listed character whose latest profile the hub has not answered is
/// due again (what takes characters out goes first, as it always does).
pub(super) fn welcomed(w: &mut World) {
    let t = now(w);
    let owed: Vec<u32> = w
        .federation
        .store
        .characters
        .iter()
        .filter(|(_, r)| r.listing.as_ref().is_some_and(|l| l.owed))
        .map(|(c, _)| *c)
        .collect();
    for c in owed {
        if listed(w, c) {
            due(w, c, t, false);
        }
    }
}

/// The link is lost: what was on it and not yet answered is owed again, on the next link.
pub(super) fn link_down(w: &mut World) {
    let d = &mut w.federation.directory;
    d.sent.clear();
    d.latest.clear();
    d.removing.clear();
    d.removals_wait.clear();
    d.pace = None;
}

/// The next listing for the link at `t`, if one may go: what takes a character out first, then the
/// profiles due, each within its character's pace. None leaves the rest for later.
fn next(w: &mut World, t: f64) -> Option<String> {
    let removal = {
        let d = &w.federation.directory;
        w.federation
            .store
            .unlistings
            .iter()
            .find(|(k, _)| {
                !d.removing.contains(*k) && d.removals_wait.get(*k).is_none_or(|&at| at <= t)
            })
            .map(|(k, a)| (k.clone(), a.clone()))
    };
    if let Some((key, who)) = removal {
        let id = fresh_id(w, 'l');
        let d = &mut w.federation.directory;
        d.removing.insert(key.clone());
        d.removals_wait.remove(&key);
        d.sent.insert(
            id.clone(),
            Sent {
                character: None,
                key,
                announce: false,
            },
        );
        return Some(
            DirectoryMessage::Listing {
                id,
                who,
                on: false,
                profile: None,
            }
            .to_json(),
        );
    }
    let ready: Vec<u32> = w
        .federation
        .directory
        .due
        .iter()
        .filter(|(_, d)| d.at <= t)
        .map(|(c, _)| *c)
        .collect();
    let rate = under(LISTINGS_PER_CHARACTER);
    for c in ready {
        let paced = w
            .federation
            .directory
            .paces
            .entry(c)
            .or_insert_with(|| Bucket::new(rate, t))
            .has(t);
        if !paced {
            continue;
        }
        let Some(due) = w.federation.directory.due.remove(&c) else {
            continue;
        };
        let Some((who, profile)) = listing_of(w, c) else {
            continue;
        };
        let id = fresh_id(w, 'l');
        let key = who.key();
        if let Some(r) = existing_record_mut(w, c) {
            let page = r
                .listing
                .as_ref()
                .filter(|l| l.who.same(&who))
                .and_then(|l| l.page.clone());
            r.listing = Some(Listing {
                who: who.clone(),
                profile: profile.clone(),
                owed: true,
                page,
            });
        }
        w.federation.dirty = true;
        let d = &mut w.federation.directory;
        if let Some(p) = d.paces.get_mut(&c) {
            p.take(t);
        }
        d.last.insert(c, t);
        d.latest.insert(c, id.clone());
        d.sent.insert(
            id.clone(),
            Sent {
                character: Some(c),
                key,
                announce: due.announce,
            },
        );
        return Some(
            DirectoryMessage::Listing {
                id,
                who,
                on: true,
                profile: Some(Box::new(profile)),
            }
            .to_json(),
        );
    }
    None
}

/// The listings to send now, after everything else the link carries, as far as the link's pace
/// (`w.federation.pace`) and the hub's pace for this world's listings allow.
pub(super) fn take(w: &mut World, t: f64, out: &mut Vec<String>) {
    let rate = under(LISTINGS_PER_WORLD);
    loop {
        let link_has = w.federation.pace.is_some_and(|p| p.has(t));
        let world_has = w
            .federation
            .directory
            .pace
            .get_or_insert_with(|| Bucket::new(rate, t))
            .has(t);
        if !link_has || !world_has {
            return;
        }
        let Some(frame) = next(w, t) else {
            return;
        };
        if let Some(p) = w.federation.pace.as_mut() {
            p.take(t);
        }
        if let Some(p) = w.federation.directory.pace.as_mut() {
            p.take(t);
        }
        out.push(frame);
    }
}

/// The passage of time: every [`LOOK_EVERY_SECS`], each listed character in the world whose last
/// listing is [`LISTING_EVERY_SECS`] old is looked at, and one whose profile changed is due.
pub(super) fn tick(w: &mut World) {
    let t = now(w);
    if t < w.federation.directory.next_look {
        return;
    }
    w.federation.directory.next_look = t + LOOK_EVERY_SECS;
    w.federation.directory.paces.retain(|_, b| !b.full_at(t));
    for player in player_manager::get_all_online(w) {
        let c = player.full();
        let d = &w.federation.directory;
        if !listed(w, c)
            || d.due.contains_key(&c)
            || d.last.get(&c).is_some_and(|&l| t - l < LISTING_EVERY_SECS)
        {
            continue;
        }
        let (Some(who), Some(profile)) = (own_address(w, player), compose(w, player)) else {
            continue;
        };
        let changed = w
            .federation
            .store
            .characters
            .get(&c)
            .and_then(|r| r.listing.as_ref())
            .is_none_or(|l| differs_while_playing(&l.profile, &profile) || !l.who.same(&who));
        if changed {
            due(w, c, t, false);
        }
    }
}

/// Whether two profiles of a character in the world differ in more than its age's value, which
/// grows every second it plays: the age alone goes with its next other change, or at its logout
/// (its option turned on or off is a change of its own).
fn differs_while_playing(a: &Profile, b: &Profile) -> bool {
    let ageless = |p: &Profile| Profile {
        age: p.age.map(|_| 0),
        ..p.clone()
    };
    ageless(a) != ageless(b)
}

/// One of the directory's frames from the hub.
pub(super) fn on_message(w: &mut World, m: DirectoryMessage) {
    match m {
        DirectoryMessage::ListingResult {
            id,
            outcome,
            reason,
            url,
        } => on_result(w, &id, outcome, reason, url),
        DirectoryMessage::ListingOff { who, by } => on_off(w, &who, by),
        DirectoryMessage::Listing { .. } => {}
    }
}

/// How a listing ended.
fn on_result(
    w: &mut World,
    id: &str,
    outcome: ListingOutcome,
    reason: Option<Refusal>,
    url: Option<String>,
) {
    let t = now(w);
    let Some(sent) = w.federation.directory.sent.remove(id) else {
        return;
    };
    let Some(character) = sent.character else {
        let d = &mut w.federation.directory;
        d.removing.remove(&sent.key);
        if outcome == ListingOutcome::RateLimited {
            d.removals_wait.insert(sent.key, t + AGAIN_SECS);
        } else {
            w.federation.store.unlistings.remove(&sent.key);
            w.federation.dirty = true;
        }
        return;
    };
    if w.federation
        .directory
        .latest
        .get(&character)
        .map(String::as_str)
        != Some(id)
    {
        // An earlier listing of the character: the latest one's answer is the one that counts.
        return;
    }
    w.federation.directory.latest.remove(&character);
    let online = player_manager::get_online_player(w, character);
    let answered = |w: &mut World, page: Option<String>| {
        if let Some(l) = existing_record_mut(w, character).and_then(|r| r.listing.as_mut()) {
            l.owed = false;
            if page.is_some() {
                l.page = page;
            }
        }
        w.federation.dirty = true;
    };
    match (outcome, reason) {
        (ListingOutcome::Ok, _) => {
            if let (true, Some(g), Some(u)) = (sent.announce, online, url.as_deref()) {
                tell_player(w, g, &format!("Your directory page: {u}"));
            }
            answered(w, url);
        }
        (ListingOutcome::RateLimited, _) => {
            due(w, character, t + AGAIN_SECS, sent.announce);
        }
        (ListingOutcome::Unavailable, _) => {
            answered(w, None);
            if let (true, Some(g)) = (sent.announce, online) {
                tell_player(w, g, UNAVAILABLE);
            }
        }
        // Held out by the hub: none is sent until the character's next login. Only the `/dn on`
        // that asked is told why; the hub's `listing.off` tells the rest.
        (ListingOutcome::Refused, _) => {
            hold_out(w, character);
            if let (true, Some(g)) = (sent.announce, online) {
                let why = match reason {
                    Some(Refusal::Full) => FULL,
                    _ => TAKEN_DOWN,
                };
                tell_player(w, g, why);
            }
        }
    }
}

/// `who` was taken out of the directory: a listing that is not on says the world has heard, and
/// the player is told, at once or at its next login. Taken out by the hub's operator, none of its
/// listings is sent until its next login, and it stays on the network. Taken out by its person,
/// forgetting itself on the site, it goes off the network, as `/dn off` takes it.
fn on_off(w: &mut World, who: &Address, by: OffBy) {
    if who.world() != w.federation.settings.slug {
        return;
    }
    owe_removal(w, who.clone());
    let Some(character) = character_named(w, who) else {
        return;
    };
    if !super::is_on(w, character) {
        return;
    }
    match by {
        OffBy::Operator => {
            hold_out(w, character);
            tell_taken_out(w, character, TakenOut::Operator);
        }
        OffBy::Person => {
            match player_manager::get_online_player(w, character) {
                Some(g) => super::commands::go_off(w, g),
                None => taken_off(w, character),
            }
            tell_taken_out(w, character, TakenOut::Forgotten);
        }
    }
}

/// A character away goes off the network: its setting says so, and its listing is forgotten, the
/// listing that takes it out owed already. Its followers' links to it go.
fn taken_off(w: &mut World, character: u32) {
    let was = listed(w, character);
    if let Some(r) = existing_record_mut(w, character) {
        r.settings = dereth_network_protocol::CharacterSettings::OFF;
    }
    forget(w, character);
    if was {
        links_changed(w, character);
    }
}
