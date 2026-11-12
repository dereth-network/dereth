//! What effects place in the world where their own data says nothing: a world's `[[effect_rule]]`
//! `places` row makes an object of a template standing at its player as the effect is put on.
//!
//! **Which effects.** An effect with a saved state does so wherever it is put on a player (an
//! item's use, a step, an event). An effect the install names (an item's use naming it) but holds
//! no saved state of does so as an item of a template naming it is used, after the use is
//! answered, as what a world's `[[death_spawn]]` makes does.
//!
//! **Where it stands.** At the row's offset from the player as it faces (`[across, ahead]` in
//! meters, the player's feet when left out), on the ground outdoors and at the player's height
//! indoors, facing as the player faces. It is made for every client that sees its place, and is
//! no one's: it can't be taken, and a use of it does nothing, unless its template is a chest's:
//! then it is a chest anyone may open, filled from its profile ([`crate::usable::chests`]).
//!
//! **How long.** For its row's `lasts` seconds, else until a tally's reset clears its template,
//! or the world stops: what is placed is not kept across a restart. No more than [`MOST`] of one
//! template placed so stand at once; a further one is not made.

use yalain_content::effect_rules::Placing;
use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::things::{Kind, Thing};
use crate::{presence, Clock, Server, Timer};

/// The most objects of one template effects have placed that stand at once.
pub const MOST: usize = 100;

/// Place an object as a rule says, at a player. Returns its id, if it was made.
pub fn place<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    p: Placing,
    clock: Clock,
) -> Option<u64> {
    let from = s.players.get(&world)?.position;
    let at = crate::death_spawn::at_offset(s, &from, p.offset);
    place_at(s, net, (p.template, at), p.lasts, clock)
}

/// Place an object of a template at a place, standing `lasts` seconds (until cleared, when none).
/// Returns its id, if it was made: not past [`MOST`] of its template.
pub fn place_at<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    (template, at): (u32, yalain_store::StoredPosition),
    lasts: Option<u32>,
    clock: Clock,
) -> Option<u64> {
    let of = s.catalog.get(template).cloned()?;
    let standing = s
        .things
        .by_id
        .values()
        .filter(|t| t.kind == Kind::Placed && t.template == template)
        .count();
    if standing >= MOST {
        return None;
    }
    let id = s.things.new_id();
    let mut t = Thing::of(id, &of, Kind::Placed);
    t.at = Some(at);
    t.fresh = true;
    s.things.insert(t);
    presence::thing_appears(s, net, id);
    if let Some(seconds) = lasts {
        s.after(clock, f64::from(seconds), Timer::Vanish { id });
    }
    Some(id)
}

/// The rows of the effects a template names for its use that the install holds no state of.
fn named<S: Store>(s: &Server<S>, props: &yalain_content::Props) -> Vec<Placing> {
    let (name, _) = crate::item_use::USED;
    (1..=8)
        .filter_map(|i| props.text(&format!("{name}{i}")))
        .filter_map(|e| s.effect_table.placings.get(e).copied())
        .collect()
}

/// Does a template name for its use an effect a world's row says places something?
#[must_use]
pub fn names_one<S: Store>(s: &Server<S>, props: &yalain_content::Props) -> bool {
    !named(s, props).is_empty()
}

/// Can this world make everything a template's use places by those rows?
#[must_use]
pub fn can_make_all<S: Store>(s: &Server<S>, props: &yalain_content::Props) -> bool {
    named(s, props)
        .iter()
        .all(|p| s.catalog.get(p.template).is_some())
}

/// What an item's use places at its user, by the effects its template names.
pub fn used<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    user: ConnId,
    props: &yalain_content::Props,
    clock: Clock,
) {
    for p in named(s, props) {
        place(s, net, user, p, clock);
    }
}
