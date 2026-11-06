//! The weapons creatures wield, as objects of their own (2017: 4,412 of 4,413 creatures that wield
//! implements and name a weapon had one made when seen fighting).
//!
//! - **Made with the creature.** A creature whose template wields implements is given its weapon
//!   (`AI_Inventory1`), twice when it holds two and wields two at once, and the item beside it
//!   (`AI_Inventory2`) when it uses a shield with its weapon or wields two; each an object held by
//!   the creature, its ids the ones after the creature's. A weapon whose template the server does
//!   not know is not made.
//! - **Shown when drawn.** In peace a creature's weapons are never sent. As it goes after a
//!   player, every client that sees it is sent each weapon hanging from its hand (a client that
//!   does not know it: its object made hung; one that does: the weapon attached again), and its
//!   combat mode; a client it comes into view of while it fights gets them with it.
//! - **Put away.** As it gives up, each weapon is let go of (the second first), and it is at peace;
//!   its weapons are not taken out of the world. Its death changes nothing; as it leaves the world
//!   its weapons go with it.
//! - **Taken up again.** Going after a player again, a client that knew a weapon is told, weapon by
//!   weapon, that no one holds it, that the creature wields and holds it again, and then that it
//!   is attached (2017: 720 of 761 weapons taken up again).
//! - **Where they hang.** A weapon or a shield at its kind's first place; of two weapons, the
//!   second at its kind's second place. The places are the template's when it names them, else
//!   its kind's (2017: one-handed weapons 1, the second `0x40000006`; handblades 1 and `0x11`;
//!   shields, bows and wands 2; javelins 4; vials 0x13).
//! - Each attaching and letting go counts on a number the messages carry (the weapon's made hung
//!   carries the count it is at).
//!
//! A creature's missile story names the weapon it holds.

use auberean_protocol::physics::{Attach, ObjectEvent};
use auberean_protocol::types::{ObjectStamp, ParentLink, PhysicsDesc, WeenieDesc};
use auberean_protocol::ServerMessage;
use yalain_net::{BlobKind, ServerNet};
use yalain_store::Store;

use crate::things::{Kind, Thing};
use crate::Server;

/// The places a weapon hangs from: first and second.
pub const FIRST: u32 = 1;
pub const SECOND: u32 = 0x4000_0006;

/// The place a creature's weapon of this name hangs from (a name's first word gives its kind),
/// as the first item held or as the second of two weapons; the template's own places first.
#[must_use]
pub fn place_of(name: &str, props: Option<&yalain_content::Props>, second: bool) -> u32 {
    let own = props.and_then(|p| {
        p.word(if second {
            "Item_SecondaryParentingLocation"
        } else {
            "Item_PrimaryParentingLocation"
        })
    });
    if let Some(p) = own.filter(|p| *p != 0) {
        return p;
    }
    let word = name
        .char_indices()
        .skip(1)
        .find(|(_, c)| c.is_ascii_uppercase())
        .map_or(name, |(i, _)| &name[..i]);
    match (word, second) {
        ("Shield" | "Bow" | "Wand", _) => 2,
        ("Javelin", _) => 4,
        ("Vial" | "Phial" | "Potion", _) => 0x13,
        ("Handblade", true) => 0x11,
        (_, true) => SECOND,
        _ => FIRST,
    }
}

/// The names of what a creature of these numbers holds: its weapon (twice when it holds two and
/// wields two), and the item beside it when it uses a shield with its weapon or wields two; none
/// when it does not wield implements.
#[must_use]
pub fn held_names(stats: &yalain_content::CreatureStats) -> Vec<(String, bool)> {
    let Some(weapon) = stats.weapon.clone().filter(|_| stats.wields) else {
        return Vec::new();
    };
    let mut out = vec![(weapon.clone(), false)];
    let shield = stats
        .beside
        .as_deref()
        .is_some_and(|b| b.starts_with("Shield"));
    if stats.dual_wields && stats.weapons_held >= 2 {
        out.push((weapon, true));
    } else if let Some(b) = stats.beside.clone() {
        if (shield && stats.weapon_and_shield) || (!shield && stats.dual_wields) {
            out.push((b, !shield));
        }
    }
    out
}

/// Make a new creature's weapons, held by it (each with the place it hangs from).
pub fn arm<S: Store>(s: &mut Server<S>, creature: u64) {
    let Some(stats) = s
        .things
        .get(creature)
        .and_then(|t| t.creature.as_ref())
        .map(|c| c.stats.clone())
    else {
        return;
    };
    let mut held = Vec::new();
    let names = held_names(&stats);
    // Two of one weapon: given two at the share the world database says, else always.
    let pair = names.len() == 2 && names[0].0 == names[1].0;
    for (name, second) in names {
        let Some(template) = s.creature_weapons.get(&name).copied() else {
            continue;
        };
        if second && pair {
            let share = s
                .creature_weapon_rules
                .get(&name)
                .and_then(|r| r.two_share)
                .unwrap_or(1.0);
            if !s.rng.chance(share) {
                continue;
            }
        }
        let Some(tpl) = s.catalog.get(template).cloned() else {
            continue;
        };
        let place = place_of(&name, Some(&tpl.props), second);
        let id = s.things.new_id();
        let mut t = Thing::of(id, &tpl, Kind::Item);
        t.container = Some((creature, 0));
        t.wielded = Some((creature, place));
        s.things.insert(t);
        held.push(id);
    }
    if let Some(c) = s.things.get_mut(creature).and_then(|t| t.creature.as_mut()) {
        c.weapons = held;
    }
}

/// The stances a weapon is fought with, alone, with a shield and two at once: its template's
/// own; else the world database's; else its kind's, read off the first word of its name.
#[must_use]
pub fn stances(
    name: &str,
    props: Option<&yalain_content::Props>,
    rule: Option<&yalain_content::WeaponRule>,
) -> [u32; 3] {
    use crate::combat::mode;
    let word = |k: &str| props.and_then(|p| p.word(k)).filter(|m| *m != 0);
    let kind = crate::combat::kind_stance(name);
    let fallback = |both: u32| rule.and_then(|r| r.stance).or(kind).unwrap_or(both);
    [
        word("Weapon_SingleWeaponStance").unwrap_or_else(|| fallback(mode::ONE_HANDED)),
        word("Weapon_WithShieldStance").unwrap_or_else(|| fallback(mode::WITH_SHIELD)),
        word("Weapon_DualWieldStance").unwrap_or_else(|| fallback(mode::DUAL)),
    ]
}

/// The stance a creature takes as it goes after a player: by the weapons it holds (two, a weapon
/// and a shield, or one), the first one's; a creature holding none fights by its template alone
/// (2017: the stance followed the items made, 4,411 of 4,412 wielding creatures).
#[must_use]
pub fn stance<S: Store>(s: &Server<S>, creature: u64) -> u32 {
    let Some(c) = s.things.get(creature).and_then(|t| t.creature.as_ref()) else {
        return crate::combat::mode::MONSTER;
    };
    let held: Vec<&Thing> = c.weapons.iter().filter_map(|w| s.things.get(*w)).collect();
    let (Some(first), Some(name)) = (held.first(), c.stats.weapon.as_deref()) else {
        return crate::combat::creature_mode(&c.stats);
    };
    let props = s.catalog.get(first.template).map(|t| &t.props);
    let [one, shield, two] = stances(name, props, s.creature_weapon_rules.get(name));
    match held.get(1) {
        None => one,
        Some(_) if c.stats.weapons_held >= 2 && c.stats.dual_wields => two,
        Some(_)
            if c.stats
                .beside
                .as_deref()
                .is_some_and(|b| b.starts_with("Shield")) =>
        {
            shield
        }
        Some(_) => two,
    }
}

/// The slider a creature's weapons are drawn and put away with, and its value and time (2017:
/// 0 over a tenth of a second both ways, 8,170 of 8,170 sliders of creatures that wield).
pub const DRAWN_SLIDER: auberean_protocol::physics::Slide = auberean_protocol::physics::Slide {
    slider: crate::equip::WEAPON_DRAWN,
    value: 0.0,
    time: 0.1,
};

/// The slider message of a creature drawing or putting away its weapons.
fn slider<S: Store>(s: &mut Server<S>, creature: u64) -> ServerMessage {
    ServerMessage::MoveSlider(ObjectEvent {
        sender: crate::inventory::visual(s, creature),
        action: DRAWN_SLIDER,
    })
}

/// A creature's weapons.
fn weapons_of<S: Store>(s: &Server<S>, creature: u64) -> Vec<u64> {
    s.things
        .get(creature)
        .and_then(|t| t.creature.as_ref())
        .map(|c| c.weapons.clone())
        .unwrap_or_default()
}

/// A creature's weapon made hanging from its hand: held, in peace, the count of its attachings at
/// its first stamp, no name.
#[must_use]
pub fn hung<S: Store>(s: &Server<S>, item: u64) -> Option<ServerMessage> {
    let t = s.things.get(item)?;
    let (creature, place) = t.wielded?;
    let c = s.things.get(creature)?;
    let count = c.creature.as_ref().map_or(1, |c| c.weapon_count);
    let mut o = crate::inventory::item_create(t);
    o.weenie.mask |= WeenieDesc::OBJECT_BITS[1];
    o.weenie.objects[1] = creature;
    o.weenie.mask &= !WeenieDesc::NAME;
    o.physics.bits |= PhysicsDesc::PARENT;
    o.physics.mode = crate::combat::mode::PEACE;
    o.physics.parent = ParentLink {
        parent: creature,
        location: place,
        stamp: c.instance,
    };
    o.physics.stamps = [count, 0, 0, 0];
    Some(ServerMessage::CreateObject(o))
}

/// What a client is told of a creature's weapons as the creature comes into its view: each made
/// hung while it fights, nothing in peace.
#[must_use]
pub fn seen_with<S: Store>(s: &Server<S>, creature: u64) -> Vec<(u64, ServerMessage)> {
    let drawn = s
        .things
        .get(creature)
        .and_then(|t| t.creature.as_ref())
        .is_some_and(|c| c.mode == crate::things::Mode::Chasing);
    if !drawn {
        return Vec::new();
    }
    weapons_of(s, creature)
        .into_iter()
        .filter_map(|i| hung(s, i).map(|m| (i, m)))
        .collect()
}

/// A creature draws its weapons as it goes after a player: each client that sees it and does not
/// know a weapon gets it made hung; then every client that sees it gets the slider and the
/// creature's combat mode `stance`; one that knew a weapon is told it is held and attached again
/// after them.
pub fn draw<S: Store>(s: &mut Server<S>, net: &mut ServerNet, creature: u64, stance: u32) {
    let weapons = weapons_of(s, creature);
    let watchers: Vec<(yalain_net::ConnId, Vec<bool>)> = s
        .players
        .iter()
        .filter(|(_, p)| p.known.contains_key(&creature))
        .map(|(c, p)| {
            (
                *c,
                weapons.iter().map(|w| p.known.contains_key(w)).collect(),
            )
        })
        .collect();
    // Drawn before, its weapons are attached again: one more to the count.
    let again = s
        .things
        .get(creature)
        .and_then(|t| t.creature.as_ref())
        .is_some_and(|c| c.weapon_count > 1);
    if again && !weapons.is_empty() {
        bump(s, creature);
    }
    let held: Vec<Vec<Vec<u8>>> = weapons
        .iter()
        .enumerate()
        .map(|(i, w)| {
            if !watchers.iter().any(|(_, known)| known[i]) {
                return Vec::new();
            }
            held_again(s, creature, *w)
                .iter()
                .map(ServerMessage::encode)
                .collect()
        })
        .collect();
    let attached: Vec<Option<Vec<u8>>> = weapons
        .iter()
        .map(|w| attach(s, *w).map(|m| m.encode()))
        .collect();
    // Made after those stamps are taken: a client made one now takes the next.
    let made: Vec<Option<ServerMessage>> = weapons.iter().map(|w| hung(s, *w)).collect();
    let slide = (!weapons.is_empty()).then(|| slider(s, creature).encode());
    let then = ServerMessage::DoMode(ObjectEvent {
        sender: crate::inventory::visual(s, creature),
        action: auberean_protocol::physics::Word(stance),
    })
    .encode();
    for (conn, known) in watchers {
        for (i, w) in weapons.iter().enumerate() {
            if !known[i] {
                if let Some(m) = &made[i] {
                    crate::presence::make_for(s, net, conn, *w, m);
                }
            }
        }
        if let Some(m) = &slide {
            net.send(
                conn,
                auberean_protocol::queue::EVENT,
                BlobKind::VISUAL,
                m.clone(),
            );
        }
        net.send(
            conn,
            auberean_protocol::queue::EVENT,
            BlobKind::VISUAL,
            then.clone(),
        );
        for (i, ms) in held.iter().enumerate() {
            if known.get(i).copied().unwrap_or(false) {
                for m in ms {
                    net.send(
                        conn,
                        auberean_protocol::queue::EVENT,
                        BlobKind::VISUAL,
                        m.clone(),
                    );
                }
                if let Some(m) = &attached[i] {
                    crate::presence::on_its_stream(s, net, conn, weapons[i], m.clone());
                }
            }
        }
    }
}

/// A weapon taken up again: held by no one and wielded by no one, then wielded and held by its
/// creature, each a visual update of its own.
fn held_again<S: Store>(s: &mut Server<S>, creature: u64, item: u64) -> Vec<ServerMessage> {
    use crate::equip::q::{CONTAINER, EQUIPPER};
    if s.things.get(item).and_then(|t| t.wielded).is_none() {
        return Vec::new();
    }
    [
        (EQUIPPER, 0),
        (CONTAINER, 0),
        (EQUIPPER, creature),
        (CONTAINER, creature),
    ]
    .into_iter()
    .map(|(quality, value)| {
        ServerMessage::UpdateInstanceIdVisual(auberean_protocol::quality::VisualQuality {
            object: crate::inventory::visual(s, item),
            quality,
            value,
        })
    })
    .collect()
}

/// Count one more attaching or letting go of a creature's weapons.
fn bump<S: Store>(s: &mut Server<S>, creature: u64) {
    if let Some(c) = s.things.get_mut(creature).and_then(|t| t.creature.as_mut()) {
        c.weapon_count = c.weapon_count.wrapping_add(1);
    }
}

/// A weapon attached to the creature that holds it again.
fn attach<S: Store>(s: &Server<S>, item: u64) -> Option<ServerMessage> {
    let t = s.things.get(item)?;
    let (creature, place) = t.wielded?;
    let c = s.things.get(creature)?;
    let count = c.creature.as_ref().map_or(0, |c| c.weapon_count);
    Some(ServerMessage::Parent(ObjectEvent {
        sender: ObjectStamp {
            id: item,
            instance_stamp: t.instance,
            other_stamp: 0,
        },
        action: Attach {
            parent: ObjectStamp {
                id: creature,
                instance_stamp: c.instance,
                other_stamp: count,
            },
            location: place,
            orientation: 0,
        },
    }))
}

/// A creature puts its weapons away as it gives up: those who know each are told the creature
/// lets go of it, the second first; then everyone who sees it, the slider.
pub fn put_away<S: Store>(s: &mut Server<S>, net: &mut ServerNet, creature: u64) {
    let weapons = weapons_of(s, creature);
    if weapons.is_empty() {
        return;
    }
    bump(s, creature);
    let Some(c) = s.things.get(creature) else {
        return;
    };
    let count = c.creature.as_ref().map_or(0, |c| c.weapon_count);
    let sender = ObjectStamp {
        id: creature,
        instance_stamp: c.instance,
        other_stamp: 0,
    };
    for w in weapons.iter().rev() {
        let Some(t) = s.things.get(*w) else { continue };
        let m = ServerMessage::DeParent(ObjectEvent {
            sender,
            action: ObjectStamp {
                id: *w,
                instance_stamp: t.instance,
                other_stamp: count,
            },
        })
        .encode();
        for (conn, p) in &s.players {
            if p.known.contains_key(w) {
                net.send(
                    *conn,
                    auberean_protocol::queue::EVENT,
                    BlobKind::UNORDERED,
                    m.clone(),
                );
            }
        }
    }
    let slide = slider(s, creature);
    crate::combat::to_all_seeing(s, net, creature, &slide);
}

/// A creature leaves the world: its weapons go with it, out of the world.
pub fn take_out<S: Store>(s: &mut Server<S>, weapons: &[u64]) -> Vec<(u64, u16)> {
    weapons
        .iter()
        .filter_map(|w| s.things.remove(*w).map(|t| (*w, t.instance)))
        .collect()
}

/// The weapon a creature holds, for its missiles' stories to name (none without one).
#[must_use]
pub fn held<S: Store>(s: &Server<S>, creature: u64) -> Option<u64> {
    weapons_of(s, creature).first().copied()
}
