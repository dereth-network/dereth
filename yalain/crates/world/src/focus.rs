//! Focus: a hero's third vital, earned by killing creatures worth it and spent by what costs it.
//!
//! **Heroes.** Only a hero (its hero quality true) earns focus (2017: the focus quality was in
//! exactly the 434 character descriptions of heroes). Becoming one sets its focus to its greatest
//! (2017: the one character seen becoming a hero was sent its hero quality and 100 focus at once).
//! Nothing in play makes a hero here: the operator's console, or a test, does.
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
pub fn resurrected(p: &mut crate::world::Player, net: &mut ServerNet, world: ConnId) {
    if let Some(f) = quals::int(&p.qualities, quality::FOCUS) {
        if f > AFTER_RESURRECTION {
            quals::set_int(&mut p.qualities, quality::FOCUS, AFTER_RESURRECTION);
            progress::send_int(net, world, quality::FOCUS, AFTER_RESURRECTION);
        }
    }
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
