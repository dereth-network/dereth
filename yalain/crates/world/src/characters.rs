//! The operator's characters, as Empyrean's operator has them for its world builder: characters
//! made with a build before they first play, deleted, carried to another world in a file and
//! brought in from one, and sent somewhere while in the world.
//!
//! - **A build** ([`Recipe`], [`make`]): a level or an experience, skills, money, items, quests,
//!   whether the character is a hero, and where it first enters, given to a character that has
//!   never been in the world (made now, or made before and not yet played). The build is what a
//!   player reaching it would hold, kept in the store's own fields, so a made character is like any
//!   other: its experience is the level's own and its credits those the levels gave
//!   ([`crate::progress::level_up`]'s); each skill is trained with its credits and raised with its
//!   experience by the rules a player's training and raising follow
//!   ([`crate::skills::trained`], [`crate::skills::SkillTable::raise`]), and refused as a player
//!   would be; its items are in its pack, its quests done or under way at a phase. Made again, the
//!   build starts over from a new character's. A character that has played is refused: delete it
//!   and make it again.
//! - **A build given to a character that has played** ([`apply`]): out of the world, its
//!   experience and skills raised to the build's by the same rules (never lowered), its pack
//!   holding at least the build's items, its money, quests, hero and place as the build says, and
//!   everything else as it was.
//! - **Deleting** ([`delete`]) marks the character deleted as its player's deletion does: it stays
//!   in its account's list, grayed out, for the deleted list's hour, then goes. One in the world is
//!   booted first, as Empyrean's `deletecharacter` boots it. Deleted at once ([`delete_now`]), it
//!   goes with no hour, and its name is free again straight away.
//! - **Placing** ([`place`]) a character out of the world: where it enters next.
//! - **Taking a character out of the world** ([`force_logoff`]) at once, kept, its login ended:
//!   one a killed client left lingering goes now.
//! - **Export and import** ([`export`], [`import`]): a character's record and what it is in the
//!   world, read whole; brought in on an account as a new character of this world (a new id, and
//!   new ids for its items), with its name or a new one. Its friends, its place on the network,
//!   its allegiance and any gag stay behind. The character and its state are kept in one
//!   transaction.
//! - **Teleporting** ([`teleport`]) a character in the world to another character, to a place the
//!   world names, or to a cell and a point: the ordinary trip through portal space
//!   ([`crate::places::sent_away`]).

use auberean_protocol::text::StringInfo;
use yalain_net::{ConnId, ServerNet};
use yalain_store::{Character, Store, StoredPosition, WorldState};

use crate::player::quality;
use crate::progress::{q, CREDITS_PER_LEVEL};
use crate::skills::{held, kept, refusal, trained};
use crate::state::{Held, PlayState, QuestState};
use crate::{quals, AccountPolicy, Clock, Server, Stage};

/// Where a quest of a build stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestAt {
    /// Completed.
    Done,
    /// Under way, at this phase (1 or more).
    Phase(u32),
}

/// A character's build, made before it first plays.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Recipe {
    pub account: String,
    pub name: String,
    pub race: u32,
    pub sex: u32,
    /// Its level (its experience the level's own), or its total experience (its level the one the
    /// experience reaches); at most one of the two.
    pub level: Option<i32>,
    pub experience: Option<u64>,
    pub money: Option<u32>,
    /// A hero or not; `None` keeps what it was made (a new character is none).
    pub hero: Option<bool>,
    /// Skills to train, each raised to a rating: (skill, rating).
    pub skills: Vec<(u32, u32)>,
    /// Items in its pack: (template, quantity).
    pub items: Vec<(u32, u32)>,
    pub quests: Vec<(u32, QuestAt)>,
    /// Where it first enters; `None` for where it was put, else its start.
    pub place: Option<StoredPosition>,
}

/// A character read whole, to be carried to another world.
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterFile {
    /// The world it was exported from.
    pub world: String,
    pub character: Character,
    pub state: WorldState,
    /// What it holds, in a recipe's terms, for people and world builders to read ([`build_of`]);
    /// none in a file written before files carried it. Bringing the character in reads the state,
    /// never this.
    pub build: Option<Recipe>,
}

/// Where a teleport sends a character.
#[derive(Debug, Clone, PartialEq)]
pub enum Destination {
    /// Beside another character in the world, facing it.
    Character(String),
    /// A place the world names (a city's recall's), by its name in any case.
    Place(String),
    /// A cell and a point in it, with a rotation.
    At(StoredPosition),
}

/// A player's refusal, in words.
fn refusal_words(code: u32) -> &'static str {
    match code {
        refusal::TRAINED => "it is trained already",
        refusal::LEVEL => "the level is below the skill's least",
        refusal::RACE => "it is not for the race",
        refusal::PARENT => "a parent skill is untrained",
        refusal::PREREQUISITE => "a prerequisite is untrained or too low",
        refusal::CREDITS => "the credits are short",
        refusal::UNTRAINED => "it is untrained",
        refusal::CAP => "it is at its cap",
        refusal::EXPERIENCE => "the experience is short",
        refusal::CANNOT_RAISE => "it cannot be raised",
        refusal::NOT_BY_PLAYER => "a player may not train it",
        refusal::HERO => "it is a hero's skill",
        _ => "refused",
    }
}

/// The world connection of a character in the world, by its name in any case.
fn in_world_named<S: Store>(s: &Server<S>, name: &str) -> Option<ConnId> {
    s.players
        .iter()
        .find(|(_, p)| p.name.eq_ignore_ascii_case(name))
        .map(|(w, _)| *w)
}

/// How many characters an account holds, the deleted ones aside.
fn held_by<S: Store>(s: &mut Server<S>, account: u64) -> Result<u32, String> {
    let all = s.store.characters(account).map_err(|e| e.to_string())?;
    Ok(u32::try_from(all.iter().filter(|c| c.deleted_at.is_none()).count()).unwrap_or(u32::MAX))
}

/// The account a made or imported character goes on: by name; on an open world one is made.
fn account_for<S: Store>(
    s: &mut Server<S>,
    name: &str,
    now: i64,
) -> Result<yalain_store::Account, String> {
    let policy: AccountPolicy = s.cfg.accounts;
    let account = crate::login::account_for_command(policy, &mut s.store, name, now)
        .map_err(|e| e.to_string())?;
    if held_by(s, account.id)? >= s.cfg.slots {
        return Err(format!(
            "account {} has no character slot free",
            account.name
        ));
    }
    Ok(account)
}

/// Bring a character's state up to a build: `st` is what it holds (a new character's, for one
/// made now). Its experience and skills are raised to the build's and never lowered, its pack
/// holds at least the build's items, and its money, quests, hero and place are set as the build
/// says; what the build does not name is left as it is.
fn build<S: Store>(
    s: &mut Server<S>,
    c: &Character,
    r: &Recipe,
    st: &mut WorldState,
    now: i64,
) -> Result<(), String> {
    let mut play = PlayState::from_bytes(&st.play)?;
    let had = quals::long(&st.qualities, q::TOTAL_XP).unwrap_or(0);
    let was = quals::int(&st.qualities, q::LEVEL).unwrap_or(1).max(1);
    // Its experience and level, as awards reaching them leave a player.
    let wanted = match (r.level, r.experience) {
        (Some(_), Some(_)) => return Err("give a level or an experience, not both".into()),
        (Some(level), None) => {
            if level < 1 || (level > 1 && level > s.levels.max_level()) {
                return Err(format!(
                    "level {level} is not a level of this world (1 to {})",
                    s.levels.max_level().max(1)
                ));
            }
            s.levels.level(level).map_or(0, |l| l.xp)
        }
        (None, Some(xp)) => xp,
        (None, None) => 0,
    };
    let total = wanted.max(had);
    let level = if s.levels.levels.is_empty() {
        was
    } else {
        s.levels.level_for(total).max(was)
    };
    let mut available = quals::long(&st.qualities, q::AVAILABLE_XP).unwrap_or(0) + (total - had);
    if total > had {
        quals::set_long(&mut st.qualities, q::TOTAL_XP, total);
        quals::set_long(&mut st.qualities, q::AVAILABLE_XP, available);
    }
    if level > was {
        let gained: i32 = (was + 1..=level)
            .map(|l| {
                s.levels
                    .level(l)
                    .map_or(CREDITS_PER_LEVEL, |x| x.skill_credits)
            })
            .sum();
        play.credits = play
            .credits
            .saturating_add(u32::try_from(gained.max(0)).unwrap_or(0));
        let hero: i32 = (was + 1..=level)
            .filter_map(|l| s.levels.level(l).map(|x| x.hero_credits))
            .sum();
        play.hero_credits = play
            .hero_credits
            .saturating_add(u32::try_from(hero.max(0)).unwrap_or(0));
        quals::set_int(&mut st.qualities, q::LEVEL, level);
        if let Some(l) = s.levels.level(level) {
            for (k, v) in [
                (quality::HEALTH_MAX, l.health),
                (quality::VIGOR_MAX, l.vigor),
                (quality::HEALTH, l.health),
                (quality::VIGOR, l.vigor),
            ] {
                quals::set_int(&mut st.qualities, k, v);
            }
        }
    }
    // Its skills: each trained with its credits, then raised with its experience to the rating.
    let starting = s.start.skills.get(&c.race).cloned().unwrap_or_default();
    let mut skills = held(&starting, &play.skills);
    let hero = r
        .hero
        .unwrap_or_else(|| crate::focus::is_hero(&st.qualities));
    for &(skill, rating) in &r.skills {
        let is_trained = skills
            .iter()
            .any(|h| h.skill == skill && h.mask & crate::skills::mask::TRAINED != 0);
        if !is_trained {
            let (_, _, (cost, hero_cost)) = trained(
                &s.skill_table,
                &mut skills,
                skill,
                level,
                c.race,
                crate::skills::Credits {
                    skill: play.credits,
                    hero: hero.then_some(play.hero_credits),
                },
            )
            .map_err(|code| format!("skill {skill} not trained: {}", refusal_words(code)))?;
            play.credits -= cost;
            play.hero_credits -= hero_cost;
        }
        while s.skill_table.rating(&skills, skill, hero) < rating {
            let cost = s
                .skill_table
                .raise(&skills, skill, available, hero)
                .map_err(|code| {
                    format!(
                        "skill {skill} not raised to {rating} (it is {}): {}",
                        s.skill_table.rating(&skills, skill, hero),
                        refusal_words(code)
                    )
                })?;
            if let Some(h) = skills.iter_mut().find(|h| h.skill == skill) {
                h.xp += cost;
            }
            available -= cost;
        }
    }
    if total > 0 {
        quals::set_long(&mut st.qualities, q::AVAILABLE_XP, available);
    }
    play.skills = kept(&starting, &skills);
    if let Some(m) = r.money {
        let m = i32::try_from(m).map_err(|_| format!("money {m} is more than a player holds"))?;
        quals::set_int(&mut st.qualities, crate::store::MONEY, m);
    }
    // Its items, in its pack: as many of each template as the build names, counting those it
    // holds already.
    let mut wanted_items: Vec<(u32, u32)> = Vec::new();
    for &(template, quantity) in &r.items {
        if s.catalog.get(template).is_none() {
            return Err(format!(
                "item {template:08X} is not among the world's templates"
            ));
        }
        if quantity == 0 {
            return Err(format!("item {template:08X}: a quantity of 1 or more"));
        }
        match wanted_items.iter_mut().find(|(t, _)| *t == template) {
            Some((_, n)) => *n = n.saturating_add(quantity),
            None => wanted_items.push((template, quantity)),
        }
    }
    for (template, quantity) in wanted_items {
        let holds: u32 = play
            .items
            .iter()
            .filter(|h| h.template == template)
            .map(|h| h.quantity.max(1))
            .sum();
        if holds >= quantity {
            continue;
        }
        // A template that stacks is one stack; any other is that many items.
        let most = s
            .catalog
            .get(template)
            .and_then(|t| t.props.int("Item_MaxStackSize"))
            .unwrap_or(0);
        let more = quantity - holds;
        let (items, each) = if most > 1 { (1, more) } else { (more, 1) };
        for _ in 0..items {
            if !crate::inventory::fits_in_pack(&play.items, 1) {
                return Err("the items do not fit in the pack".into());
            }
            // Put in the pack as an item given to a player is.
            let slot = crate::inventory::room_for_one(&mut play.items);
            play.items.push(Held {
                id: s.things.new_id(),
                template,
                slot,
                location: 0,
                quantity: each,
            });
        }
    }
    crate::inventory::close_up(&mut play.items);
    // Its quests.
    for &(quest, at) in &r.quests {
        if s.quests.quest(quest).is_none() {
            return Err(format!(
                "quest {quest:08X} is not one of the world's quests"
            ));
        }
        let (status, phase) = match at {
            QuestAt::Done => (crate::quests::status::COMPLETED, 0),
            QuestAt::Phase(0) => return Err(format!("quest {quest:08X}: a phase of 1 or more")),
            QuestAt::Phase(n) => (crate::quests::status::UNDERWAY, n),
        };
        // A quest it has under way already keeps when it was begun.
        let started = play
            .quests
            .iter()
            .find(|x| x.quest == quest)
            .map_or(now, |x| x.started);
        play.quests.retain(|x| x.quest != quest);
        play.quests.push(QuestState {
            quest,
            status,
            phase,
            started,
            updated: now,
        });
    }
    if let Some(hero) = r.hero {
        crate::focus::set_hero_kept(&mut st.qualities, hero);
    }
    if let Some(at) = r.place {
        st.position = Some(at);
    }
    st.play = play.to_bytes();
    Ok(())
}

/// Make a character with a build, or give one made before (and never played) its build anew.
/// Returns what was done, as the console reports it.
///
/// # Errors
/// The build does not hold (nothing is kept then), the character has played or is another's, or
/// the store's.
pub fn make<S: Store>(s: &mut Server<S>, r: &Recipe, now: i64) -> Result<String, String> {
    let found = s
        .store
        .character_named(&r.name)
        .map_err(|e| e.to_string())?;
    let Some(c) = found else {
        let account = account_for(s, &r.account, now)?;
        let c = crate::login::new_character_record(
            &mut s.store,
            &s.rules,
            &account,
            &r.name,
            r.race,
            r.sex,
            now,
        )?;
        let mut st = crate::world::first_state(s, &c);
        build(s, &c, r, &mut st, now)?;
        s.store
            .insert_character_with_state(&c, &st)
            .map_err(|e| e.to_string())?;
        return Ok(format!(
            "character {} made on {} with its build: {:#x}",
            c.name, account.name, c.id
        ));
    };
    let account = s
        .store
        .account_by_name(&r.account)
        .map_err(|e| e.to_string())?;
    if account.is_none_or(|a| a.id != c.account) {
        return Err(format!("{} is another account's character", c.name));
    }
    if c.deleted_at.is_some() {
        return Err(format!("{} is deleted", c.name));
    }
    if in_world_named(s, &c.name).is_some() {
        return Err(format!("{} is in the world", c.name));
    }
    if c.race != r.race || c.sex != r.sex {
        return Err(format!(
            "{} is of another race or sex: delete it and make it again",
            c.name
        ));
    }
    let kept = crate::saves::state_of(s, c.id).map_err(|e| e.to_string())?;
    if kept.as_ref().is_some_and(|k| k.instances > 0) {
        return Err(format!(
            "{} has played: bring it up to the build instead, or delete it at once and make it again",
            c.name
        ));
    }
    let mut st = crate::world::first_state(s, &c);
    if let Some(k) = &kept {
        // Where it was put, and whether it was made a hero, stand unless the build says.
        if k.position.is_some() {
            st.position = k.position;
        }
        if crate::focus::is_hero(&k.qualities) {
            crate::focus::set_hero_kept(&mut st.qualities, true);
        }
    }
    build(s, &c, r, &mut st, now)?;
    crate::saves::put(s, c.id, st).map_err(|e| e.to_string())?;
    Ok(format!("character {} given its build", c.name))
}

/// Delete a character as its player's deletion does: it stays in its account's list, grayed
/// out, for the deleted list's hour. One in the world is booted first. Returns what was done.
///
/// # Errors
/// No such character, one deleted already, or the store's.
pub fn delete<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    clock: Clock,
    name: &str,
) -> Result<String, String> {
    let c = s
        .store
        .character_named(name)
        .map_err(|e| e.to_string())?
        .filter(|c| c.deleted_at.is_none())
        .ok_or_else(|| format!("no character is named {name}"))?;
    let mut booted = "";
    if let Some(login) = s
        .players
        .values()
        .find(|p| p.character == c.id)
        .map(|p| p.login)
    {
        crate::admin::end_login(s, net, login, crate::admin::BOOTED, clock);
        booted = ", booted from the world first";
    }
    s.store
        .set_deleted(c.id, Some(clock.unix))
        .map_err(|e| e.to_string())?;
    // An account at its character list sees it grayed out at once.
    let mut selecting: Vec<ConnId> = s
        .sessions
        .iter()
        .filter(|(_, x)| x.account.id == c.account && x.stage == Stage::Selecting)
        .map(|(conn, _)| *conn)
        .collect();
    selecting.sort_unstable();
    for conn in selecting {
        crate::login::send_lists(s, net, conn, clock);
    }
    Ok(format!(
        "{} deleted{booted}; it stays in its account's list, grayed out, for {} minutes",
        c.name,
        s.cfg.delete_seconds / 60
    ))
}

/// Is a character in the world (lingering included), or on its way in?
fn in_world_or_entering<S: Store>(s: &Server<S>, id: u64) -> bool {
    s.players.values().any(|p| p.character == id)
        || s.sessions.values().any(|x| match x.stage {
            Stage::Entering { character, .. } | Stage::InWorld { character, .. } => character == id,
            Stage::Selecting => false,
        })
}

/// What a character holds out of the world: its kept state, or, never in the world, the state it
/// will first enter with (where it was put, and what it was given, kept).
fn state_out_of_world<S: Store>(s: &mut Server<S>, c: &Character) -> Result<WorldState, String> {
    Ok(
        match crate::saves::state_of(s, c.id).map_err(|e| e.to_string())? {
            Some(st) if !st.qualities.is_empty() => st,
            kept => {
                let mut st = crate::world::first_state(s, c);
                if let Some(k) = kept {
                    st.position = k.position.or(st.position);
                    st.places = k.places;
                    st.play = k.play;
                }
                st
            }
        },
    )
}

/// Bring a character out of the world, played or not, up to a build, keeping what the build does
/// not name: its experience and skills raised to the build's (never lowered), its pack holding
/// at least the build's items, its money, quests, hero and place as the build says. Returns what
/// was done, as the console reports it.
///
/// # Errors
/// The build does not hold (nothing is kept then), no such character, one in the world or on
/// its way in, another account's, another race or sex, or the store's.
pub fn apply<S: Store>(s: &mut Server<S>, r: &Recipe, now: i64) -> Result<String, String> {
    let c = s
        .store
        .character_named(&r.name)
        .map_err(|e| e.to_string())?
        .filter(|c| c.deleted_at.is_none())
        .ok_or_else(|| format!("no character is named {}", r.name))?;
    let account = s
        .store
        .account_by_name(&r.account)
        .map_err(|e| e.to_string())?;
    if account.is_none_or(|a| a.id != c.account) {
        return Err(format!("{} is another account's character", c.name));
    }
    if in_world_or_entering(s, c.id) {
        return Err(format!(
            "{} is in the world: a build is given out of it",
            c.name
        ));
    }
    if c.race != r.race || c.sex != r.sex {
        return Err(format!(
            "{} is of another race or sex: delete it and make it again",
            c.name
        ));
    }
    let mut st = state_out_of_world(s, &c)?;
    build(s, &c, r, &mut st, now)?;
    crate::saves::put(s, c.id, st).map_err(|e| e.to_string())?;
    Ok(format!("{} brought up to its build", c.name))
}

/// Set where a character out of the world enters next: beside a character in the world, at a
/// place the world names, or at a cell and point. Returns what was done.
///
/// # Errors
/// No such character, one in the world or on its way in, a destination that is none, or the
/// store's.
pub fn place<S: Store>(s: &mut Server<S>, who: &str, to: &Destination) -> Result<String, String> {
    let c = s
        .store
        .character_named(who)
        .map_err(|e| e.to_string())?
        .filter(|c| c.deleted_at.is_none())
        .ok_or_else(|| format!("no character is named {who}"))?;
    if in_world_or_entering(s, c.id) {
        return Err(format!("{} is in the world: teleport it instead", c.name));
    }
    let (at, said) = match to {
        Destination::Character(other) => {
            let o = in_world_named(s, other)
                .and_then(|w| s.players.get(&w))
                .ok_or_else(|| format!("{other} is not in the world"))?;
            (
                crate::places::beside(o.position.cell, o.position.origin, &o.position),
                o.name.clone(),
            )
        }
        Destination::Place(name) => {
            let pl = s
                .places_by_name
                .get(&name.to_ascii_lowercase())
                .ok_or_else(|| format!("the world names no place {name}"))?;
            (
                StoredPosition {
                    cell: pl.cell,
                    origin: pl.origin,
                    rotation: pl.rotation,
                },
                name.clone(),
            )
        }
        Destination::At(at) => (*at, format!("{:08X} {:?}", at.cell, at.origin)),
    };
    let mut st = crate::saves::state_of(s, c.id)
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    st.position = Some(at);
    crate::saves::put(s, c.id, st).map_err(|e| e.to_string())?;
    Ok(format!("{} enters at {said} next", c.name))
}

/// Delete a character at once, with no hour in its account's list: its name is free again
/// straight away. One in the world is taken out of it at once (kept no more) and its login
/// booted; one deleted already, and waiting out its hour, goes too. Returns what was done.
///
/// # Errors
/// No such character, one on its way into the world, or the store's.
pub fn delete_now<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    clock: Clock,
    name: &str,
) -> Result<String, String> {
    let c = s
        .store
        .character_named(name)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no character is named {name}"))?;
    let playing = s
        .players
        .iter()
        .find(|(_, p)| p.character == c.id)
        .map(|(w, p)| (*w, p.login));
    if playing.is_none() && in_world_or_entering(s, c.id) {
        return Err(format!("{} is on its way into the world", c.name));
    }
    let mut booted = "";
    if let Some((world, login)) = playing {
        // Out of the world now, as at the end of a leaving, then its login booted.
        crate::login::logged_off(s, net, login, world, clock);
        crate::admin::end_login(s, net, login, crate::admin::BOOTED, clock);
        booted = ", taken out of the world and booted first";
    }
    crate::saves::write_out(s).map_err(|e| e.to_string())?;
    s.store.purge_character(c.id).map_err(|e| e.to_string())?;
    s.saves.forget(c.id);
    crate::market::seller_gone(s, net, c.id);
    // An account at its character list sees it gone at once.
    let mut selecting: Vec<ConnId> = s
        .sessions
        .iter()
        .filter(|(_, x)| x.account.id == c.account && x.stage == Stage::Selecting)
        .map(|(conn, _)| *conn)
        .collect();
    selecting.sort_unstable();
    for conn in selecting {
        crate::login::send_lists(s, net, conn, clock);
    }
    Ok(format!("{} deleted at once{booted}", c.name))
}

/// Take a character out of the world at once and end its login, as Empyrean's `forcelogoff`
/// does, without the leaving a logging off plays: it is kept where it stands, as when it leaves,
/// and its account may enter again straight away. A character whose client was killed, or that
/// is logging off, goes now instead of lingering. Returns what was done.
///
/// # Errors
/// No character of that name is in the world, or it is on its way in.
pub fn force_logoff<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    clock: Clock,
    name: &str,
) -> Result<String, String> {
    let Some(world) = in_world_named(s, name) else {
        let entering = s
            .store
            .character_named(name)
            .ok()
            .flatten()
            .is_some_and(|c| in_world_or_entering(s, c.id));
        return Err(if entering {
            format!("{name} is on its way into the world")
        } else {
            format!("{name} is not in the world")
        });
    };
    let Some((who, login)) = s.players.get(&world).map(|p| (p.name.clone(), p.login)) else {
        return Err(format!("{name} is not in the world"));
    };
    crate::login::logged_off(s, net, login, world, clock);
    if !s.sessions.contains_key(&login) {
        return Ok(format!("{who} taken out of the world"));
    }
    crate::admin::end_login(s, net, login, crate::admin::BOOTED, clock);
    Ok(format!("{who} taken out of the world and booted"))
}

/// Read a character whole for another world. One in the world is kept first, so the file holds
/// it as it is now.
///
/// # Errors
/// No such character, or the store's.
pub fn export<S: Store>(s: &mut Server<S>, name: &str) -> Result<CharacterFile, String> {
    let c = s
        .store
        .character_named(name)
        .map_err(|e| e.to_string())?
        .filter(|c| c.deleted_at.is_none())
        .ok_or_else(|| format!("no character is named {name}"))?;
    if let Some(world) = in_world_named(s, &c.name) {
        crate::world::keep_player(s, world);
    }
    let state = state_out_of_world(s, &c)?;
    let build = build_of(s, &c, &state)?;
    Ok(CharacterFile {
        world: s.cfg.world_name.clone(),
        character: c,
        state,
        build: Some(build),
    })
}

/// What a character holds, as a recipe says it ([`Recipe`], with no account): its level when its
/// experience is the level's own, else its total experience; its money and whether it is a hero;
/// every skill it has trained, with its rating now; its items by template, counted wherever it
/// holds them; its quests done and under way; and where it enters next. Given to
/// [`make`] under another name, it makes a character holding the same.
///
/// # Errors
/// Its play does not read.
pub fn build_of<S: Store>(s: &Server<S>, c: &Character, st: &WorldState) -> Result<Recipe, String> {
    let play = PlayState::from_bytes(&st.play)?;
    let total = quals::long(&st.qualities, q::TOTAL_XP).unwrap_or(0);
    let level = quals::int(&st.qualities, q::LEVEL).unwrap_or(1).max(1);
    let (level, experience) = match s.levels.level(level) {
        Some(l) if l.xp == total => (Some(level), None),
        _ if total == 0 => (None, None),
        _ => (None, Some(total)),
    };
    let starting = s.start.skills.get(&c.race).cloned().unwrap_or_default();
    let skills = held(&starting, &play.skills);
    let hero = crate::focus::is_hero(&st.qualities);
    let mut trained: Vec<(u32, u32)> = skills
        .iter()
        .filter(|h| h.mask & crate::skills::mask::TRAINED != 0)
        .map(|h| (h.skill, s.skill_table.rating(&skills, h.skill, hero)))
        .collect();
    trained.sort_unstable();
    let mut items: Vec<(u32, u32)> = Vec::new();
    for h in &play.items {
        match items.iter_mut().find(|(t, _)| *t == h.template) {
            Some((_, n)) => *n = n.saturating_add(h.quantity.max(1)),
            None => items.push((h.template, h.quantity.max(1))),
        }
    }
    items.sort_unstable();
    let mut quests: Vec<(u32, QuestAt)> = play
        .quests
        .iter()
        .filter_map(|x| match x.status {
            crate::quests::status::COMPLETED => Some((x.quest, QuestAt::Done)),
            crate::quests::status::UNDERWAY => Some((x.quest, QuestAt::Phase(x.phase.max(1)))),
            _ => None,
        })
        .collect();
    quests.sort_unstable_by_key(|(quest, _)| *quest);
    Ok(Recipe {
        account: String::new(),
        name: c.name.clone(),
        race: c.race,
        sex: c.sex,
        level,
        experience,
        money: quals::int(&st.qualities, crate::store::MONEY).map(i32::cast_unsigned),
        hero: Some(crate::focus::is_hero(&st.qualities)),
        skills: trained,
        items,
        quests,
        place: st.position,
    })
}

/// Bring a character from a file onto an account as a new character of this world: a new id, new
/// ids for its items, its name or `new_name`. Kept whole or not at all. Returns what was done.
///
/// # Errors
/// The account cannot hold it, the name is no name or is taken, its state does not read or holds
/// one item id twice, or the store's.
pub fn import<S: Store>(
    s: &mut Server<S>,
    file: &CharacterFile,
    account: &str,
    new_name: Option<&str>,
    now: i64,
) -> Result<String, String> {
    let wanted = new_name.unwrap_or(&file.character.name);
    let name = crate::names::NameRules::default()
        .check(wanted)
        .map_err(|why| format!("the name {wanted} is not a name ({why:?})"))?;
    if s.store.name_taken(&name).unwrap_or(true) {
        return Err(format!(
            "the name {name} is taken: give the character a new one"
        ));
    }
    let mut play = PlayState::from_bytes(&file.state.play)?;
    // Each row becomes an item of its own, so one id held twice would make two of one item.
    let mut seen = std::collections::HashSet::new();
    if let Some(h) = play.items.iter().find(|h| !seen.insert(h.id)) {
        return Err(format!(
            "the file holds item {:#x} twice: nothing was brought",
            h.id
        ));
    }
    let account = account_for(s, account, now)?;
    let n = s.store.next_character_number().map_err(|e| e.to_string())?;
    let old = file.character.id;
    let id = crate::login::PLAYER_ID_BASE | n;
    let c = Character {
        id,
        account: account.id,
        name: name.clone(),
        deleted_at: None,
        last_played: now,
        ..file.character.clone()
    };
    // Its items, given ids of this world's.
    let mut ids = std::collections::HashMap::new();
    for h in &mut play.items {
        let new = s.things.new_id();
        ids.insert(h.id, new);
        h.id = new;
    }
    play.item_states.retain_mut(|k| match ids.get(&k.id) {
        Some(new) => {
            k.id = *new;
            true
        }
        None => false,
    });
    let me = |x: u64| if x == old { id } else { x };
    for k in &mut play.item_states {
        k.own.bound_to = me(k.own.bound_to);
    }
    for e in &mut play.effects {
        e.caster = me(e.caster);
        e.acting = me(e.acting);
        if let Some(new) = ids.get(&e.item) {
            e.item = *new;
        }
    }
    // A cloak and a suffix are this world's own admins' business.
    play.cloaked = false;
    play.suffix.clear();
    let mut state = file.state.clone();
    if !play.items.is_empty() || !file.state.play.is_empty() {
        state.play = play.to_bytes();
    }
    // Its name as the world shows it.
    if !state.qualities.is_empty() {
        let mut q = quals::read(&state.qualities);
        q.weenie.name = StringInfo::literal(&crate::player::world_name(&name, c.sex));
        state.qualities = quals::write(&q);
    }
    s.store
        .insert_character_with_state(&c, &state)
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "character {name} brought from {} onto {}: {id:#x}",
        if file.world.is_empty() {
            "another world"
        } else {
            &file.world
        },
        account.name
    ))
}

/// Send a character in the world somewhere through portal space. Returns what was done.
///
/// # Errors
/// It is not in the world or is on its way already, or the destination is none.
pub fn teleport<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    clock: Clock,
    who: &str,
    to: &Destination,
) -> Result<String, String> {
    let world = in_world_named(s, who).ok_or_else(|| format!("{who} is not in the world"))?;
    let Some(p) = s.players.get(&world) else {
        return Err(format!("{who} is not in the world"));
    };
    if p.trip.is_some() {
        return Err(format!("{} is on its way somewhere already", p.name));
    }
    let (name, from) = (p.name.clone(), p.position);
    let (at, said) = match to {
        Destination::Character(other) => {
            let o = in_world_named(s, other)
                .and_then(|w| s.players.get(&w))
                .ok_or_else(|| format!("{other} is not in the world"))?;
            if o.character == p.character {
                return Err(format!("{name} cannot be sent to itself"));
            }
            (
                crate::places::beside(o.position.cell, o.position.origin, &from),
                o.name.clone(),
            )
        }
        Destination::Place(place) => {
            let pl = s
                .places_by_name
                .get(&place.to_ascii_lowercase())
                .ok_or_else(|| format!("the world names no place {place}"))?;
            (
                StoredPosition {
                    cell: pl.cell,
                    origin: pl.origin,
                    rotation: pl.rotation,
                },
                place.clone(),
            )
        }
        Destination::At(at) => (*at, format!("{:08X} {:?}", at.cell, at.origin)),
    };
    crate::places::sent_away(s, net, world, at, clock);
    Ok(format!("{name} is on its way to {said}"))
}
