//! Focus: a hero's third vital, earned by killing creatures worth it and spent by what costs it.
//!
//! **Heroes.** Only a hero (its hero quality true) earns focus (2017: the focus quality was in
//! exactly the 434 character descriptions of heroes). Becoming one sets its focus to its greatest
//! (2017: the one character seen becoming a hero was sent its hero quality and 100 focus at once).
//! Nothing in play makes a hero here: the operator's console, or a test, does. A world's step or
//! give may be for heroes alone: it runs for no one else.
//!
//! **Its greatest** is 100 and the character's own focus quality above it (with what its effects
//! add); an award past it stops there (2017: 12 of 12 sent 100), and an award at it sends nothing
//! (363 of 363), its text still naming the whole part.
//!
//! **A kill** is worth its template's death focus, whatever the killer's level (2017: the whole of
//! it in 401 of 401 solo kills by heroes, up to 16 levels above them; not doubled on the days that
//! doubled experience), paid in parts as its experience is: each damager's part the death focus
//! times the health it took over all taken, rounded to the nearest (803 of 803 parts). In a
//! fellowship that shares experience, a fellow within 100 m of the one whose part it is gets the
//! death focus times the fellowship's share times the part's share, rounded, never a group
//! creature's factor (227 of 264 shares exact, 263 within 1).
//!
//! **Resurrected**, a player with more than ten focus comes back with ten, sent after its health
//! and vigor (2017: 18 of 20 resurrections from above ten; 19 of 19 in that order); ten or less is
//! kept (2 of 2).
//!
//! **Spent** by a skill whose definition costs focus (the heroes' skills): its cost at the user's
//! rating ([`crate::skill_use::focus_cost`]), paid as the use is carried out, after its vigor; a
//! use the player hasn't the focus for is refused, and the client says so.

use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::player::quality;
use crate::{progress, quals, Server};

/// The quality that makes a character a hero.
pub const HERO: u32 = 4000;

/// The focus every hero has at least as its greatest, before its own focus quality.
pub const BASE: i32 = 100;

/// The most focus a player comes back from a resurrection with.
pub const AFTER_RESURRECTION: i32 = 10;

/// How far from the one whose part of a kill it is a fellow shares its focus, in meters.
pub const SHARE_RANGE: f32 = 100.0;

/// Is the character whose qualities these are a hero?
#[must_use]
pub fn is_hero(qualities: &[u8]) -> bool {
    quals::bool(qualities, HERO).unwrap_or(false)
}

/// May a step run for a player, by whether it is for heroes alone?
#[must_use]
pub fn allows_step<S: Store>(
    s: &Server<S>,
    world: ConnId,
    step: &yalain_content::quests::Step,
) -> bool {
    let gates = &s.content.hero_gates;
    gates.is_empty() || !gates.of_step(step) || hero_in_world(s, world)
}

/// May a give take its item from a player, by whether it is for heroes alone?
#[must_use]
pub fn allows_give<S: Store>(
    s: &Server<S>,
    world: ConnId,
    give: &yalain_content::quests::Give,
) -> bool {
    let gates = &s.content.hero_gates;
    gates.is_empty() || !gates.of_give(give) || hero_in_world(s, world)
}

/// Is a player in the world a hero?
fn hero_in_world<S: Store>(s: &Server<S>, world: ConnId) -> bool {
    s.players.get(&world).is_some_and(|p| is_hero(&p.qualities))
}

/// A player's greatest focus.
#[must_use]
pub fn greatest<S: Store>(s: &Server<S>, p: &crate::world::Player) -> i32 {
    crate::effects::max_vital(s, p, quality::FOCUS_MAX)
}

/// A player's focus now (its greatest when it has never been set).
#[must_use]
pub fn current<S: Store>(s: &Server<S>, p: &crate::world::Player) -> i32 {
    quals::int(&p.qualities, quality::FOCUS).unwrap_or_else(|| greatest(s, p))
}

/// A player's focus to spend: a hero's focus now; none for one who is no hero (2017: no character
/// that was no hero held any).
#[must_use]
pub fn spendable<S: Store>(s: &Server<S>, p: &crate::world::Player) -> i32 {
    if is_hero(&p.qualities) {
        current(s, p)
    } else {
        0
    }
}

/// A hero gains focus, up to its greatest: the new focus is sent, nothing when it was already at
/// its greatest.
pub fn gain<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, amount: i32) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let (max, was) = (greatest(s, p), current(s, p));
    if amount <= 0 || was >= max {
        return;
    }
    let now = was.saturating_add(amount).min(max);
    if let Some(p) = s.players.get_mut(&world) {
        quals::set_int(&mut p.qualities, quality::FOCUS, now);
    }
    progress::send_int(net, world, quality::FOCUS, now);
}

/// Back from a resurrection: focus above ten is cut to ten (and sent).
pub fn resurrected(p: &mut crate::world::Player, net: &mut ServerNet, world: ConnId, most: i32) {
    if let Some(f) = quals::int(&p.qualities, quality::FOCUS) {
        if f > most {
            quals::set_int(&mut p.qualities, quality::FOCUS, most);
            progress::send_int(net, world, quality::FOCUS, most);
        }
    }
}

/// The most focus a player comes back with: ten, and a share of its greatest a resurrecting skill
/// lets it keep, up to the next whole point (2017: 20 at 0.0944, 17 at 0.0657, of a greatest of 100).
#[must_use]
pub fn back_with(greatest: i32, kept_share: f32) -> i32 {
    #[allow(clippy::cast_precision_loss)]
    let kept = (greatest.max(0) as f32 * kept_share.max(0.0)).ceil();
    AFTER_RESURRECTION + dereth_primitives::num::to_i32(kept)
}

/// Make a character a hero, or no longer one, by name: in the world, it is sent its hero quality
/// and, made one, its focus at its greatest; out of it, its stored qualities are changed. Returns
/// what was done, as the console reports it.
///
/// # Errors
/// No character is so named, or the store fails.
pub fn set_hero<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    name: &str,
    hero: bool,
) -> Result<String, String> {
    let online = s
        .players
        .iter()
        .find(|(_, p)| p.name.eq_ignore_ascii_case(name))
        .map(|(w, _)| *w);
    let said = if hero {
        "is now a hero"
    } else {
        "is no longer a hero"
    };
    if let Some(world) = online {
        let Some(p) = s.players.get_mut(&world) else {
            return Err(format!("{name} left the world"));
        };
        quals::set_bool(&mut p.qualities, HERO, hero);
        let who = p.name.clone();
        progress::private(
            net,
            world,
            &auberean_protocol::ServerMessage::UpdateBool(
                auberean_protocol::quality::PrivateQuality {
                    quality: HERO,
                    value: i32::from(hero),
                },
            ),
        );
        if hero {
            let Some(p) = s.players.get(&world) else {
                return Err(format!("{name} left the world"));
            };
            let max = greatest(s, p);
            if let Some(p) = s.players.get_mut(&world) {
                quals::set_int(&mut p.qualities, quality::FOCUS, max);
            }
            progress::send_int(net, world, quality::FOCUS, max);
        }
        return Ok(format!("{who} {said}."));
    }
    let c = s
        .store
        .character_named(name)
        .map_err(|e| format!("the store: {e}"))?
        .ok_or_else(|| format!("no character is named {name}"))?;
    let mut st = crate::saves::state_of(s, c.id)
        .map_err(|e| format!("the store: {e}"))?
        .unwrap_or_default();
    if st.qualities.is_empty() {
        // Never in the world yet: the qualities it will first enter with.
        st.qualities = crate::world::first_state(s, &c).qualities;
    }
    set_hero_kept(&mut st.qualities, hero);
    crate::saves::put(s, c.id, st).map_err(|e| format!("the store: {e}"))?;
    Ok(format!("{} {said} (out of the world).", c.name))
}

/// Make a character out of the world a hero, or no longer one, in its kept qualities: made one,
/// its focus is its greatest when it enters (the base and its own focus quality).
pub fn set_hero_kept(qualities: &mut Vec<u8>, hero: bool) {
    quals::set_bool(qualities, HERO, hero);
    if hero {
        let own = quals::int(qualities, quality::FOCUS_MAX).unwrap_or(0);
        quals::set_int(qualities, quality::FOCUS, BASE + own);
    }
}

/// The string table of heroes' titles.
pub const TITLES: u32 = 0x2500_0CE8;

/// Each race's classes as a hero's title names them, by the skill that makes one of the class, in
/// the order they are looked for: the first the hero holds trained is its class (Defenders by either
/// of two, Drudges by their first attack).
pub const TITLE_CLASSES: [(u32, &[(u32, &str)]); 5] = [
    (
        1,
        &[
            (132, "BountyHunter_"),
            (150, "Defender_"),
            (148, "Defender_"),
            (189, "Alchemist_"),
            (235, "Ranger_"),
            (68, "Enchanter_"),
            (116, "Sorcerer_"),
        ],
    ),
    (
        2,
        &[
            (319, "Berserker_"),
            (355, "Juggernaut_"),
            (423, "Raider_"),
            (450, "Tactician_"),
            (270, "Elementalist_"),
            (302, "Sage_"),
        ],
    ),
    (
        4,
        &[
            (697, "FeralIntendant_"),
            (731, "Zealot_"),
            (769, "HiveKeeper_"),
            (763, "ClawBearer_"),
            (642, "Healer_"),
            (654, "Invoker_"),
        ],
    ),
    (
        16,
        &[
            (1929, "Templar_"),
            (1868, "Mentalist_"),
            (1544, "Hieromancer_"),
        ],
    ),
    (8, &[(2112, "Drudge_")]),
];

/// A hero's title, its string in [`TITLES`]: the token `ID_`, its class, its kingdom (`Neutral`,
/// `Shadow`, `Order`, `Dominion`) and its rank, 1 to level 50, one more each ten levels past it, 11 at
/// the highest level (2017: 4,479 examinations of heroes, every title so, a Ranger of no kingdom
/// at level 48 `ID_Ranger_Neutral1`). None for one who is no hero, or holds no class.
#[must_use]
pub fn hero_title<S: Store>(s: &Server<S>, p: &crate::world::Player) -> Option<u32> {
    if !is_hero(&p.qualities) {
        return None;
    }
    let starting = s.start.skills.get(&p.race).cloned().unwrap_or_default();
    let skills = crate::skills::held(&starting, &p.play.skills);
    let trained = |id: u32| {
        skills
            .iter()
            .any(|h| h.skill == id && h.mask & crate::skills::mask::TRAINED != 0)
    };
    let (_, classes) = TITLE_CLASSES.iter().find(|(race, _)| *race == p.race)?;
    let class = classes.iter().find(|(id, _)| trained(*id))?.1;
    let kingdom = match crate::kingdoms::of(&p.qualities) {
        None => "Neutral",
        Some(1) => "Shadow",
        Some(2) => "Order",
        Some(4) => "Dominion",
        Some(_) => return None,
    };
    let level = crate::examine::level_of(&p.qualities);
    let rank = if level <= 50 {
        1
    } else if level >= s.levels.max_level() {
        11
    } else {
        (level - 50) / 10 + 1
    };
    Some(auberean_assets::strings::token(&format!(
        "ID_{class}{kingdom}{rank}"
    )))
}
