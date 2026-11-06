//! Mounts: a saddle or a mount's egg used from the pack puts its user on a beast. The rider is
//! one object still: the player takes the mount's look, runs at its speed, and is mounted (a quality
//! of its own) until it dismounts, the effect runs out, it goes indoors, it is resurrected or it
//! leaves the world.
//!
//! **What mounts a player.** An item whose use puts on a mount effect (its effects as items' uses
//! resolve them, [`crate::item_use`]): the effect's record is the ride. A saddle's lasts its duration (an
//! ephemeral one fifteen minutes) and is spent; an egg's lasts until taken off and is kept.
//!
//! **Mounting**, in one instant: the player is told it is mounted (quality 2002); the mount's
//! appearance plays for all who see it; the record is added (cast by the player from the item,
//! its speed the player's times the mount's); the item's other effects go on (an egg's focus
//! cost); the item is spent if it is spent on use; the use is
//! answered (control flags 828, and the spent bit); the mounted look is shown to all who see it.
//!
//! **The mounted look** is the mount table's description for the mount's type and the rider's
//! race and sex, without its own id, carrying the rider's own appearance: each of the rider's
//! appearance entries over its body's defaults for that entry.
//!
//! **Refused** as any item's use is ([`crate::item_use`]): for a mount, moving (10055), something in
//! hand (10045), already mounted (10047), indoors (10046), in that order, the answer alone.
//!
//! **Riding.** Portal trips keep the ride. A pet attack order is refused with its own text while
//! mounted; other orders go on.
//!
//! **The ride's end**, in one instant: the player is told it is no longer mounted, its own look is
//! shown, the record taken off (its speed back), the mount's appearance stopped and its ending
//! appearance played. Asked for (a dismount, which first ends a follow), the record's time up, a
//! resurrection, or the rider indoors: a few seconds after it walks in, or after it arrives inside
//! by portal. A player not mounted asking to dismount is not answered.

use std::collections::HashMap;

use auberean_protocol::table::Table;
use auberean_protocol::types::{FloatTable, PartGroupDesc, VisualDesc};
use auberean_protocol::wire::{Field, Reader};
use auberean_protocol::ServerMessage;
use auberean_script::graph::Graph;
use auberean_script::library::Library;
use auberean_script::native_object::Item;
use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::effects::{self, Cast, On, Shape};
use crate::places::Use;
use crate::start::Nav;
use crate::{progress, Clock, Server, Timer};

/// The player's quality that says it is mounted (a private bool).
pub const MOUNTED: u32 = 2002;

/// The call a player's client makes to dismount.
pub const DISMOUNT: u32 = 0x8212_0030;

/// The use answer's control flags for a mount item (the spent bit added for a saddle).
pub const ANSWER_FLAGS: u32 = 828;
pub const SPENT: u32 = 0x1;

/// A mount record's flags: a timed one (it runs out, it goes at logout) and one held until taken
/// off.
pub mod record {
    use crate::effects::record_flag;
    pub const TIMED: u32 = record_flag::NORMAL | record_flag::LOGOUT | record_flag::TRANSIENT;
    pub const UNTIL_OFF: u32 = record_flag::INFINITE | record_flag::LOGOUT | record_flag::TRANSIENT;
}

/// The text a pet attack order is refused with while mounted, and the kind it is shown as.
pub const PET_ORDER_REFUSED: (u32, u32) = (0x2DF0_E54D, 0x2500_000B);
pub const PET_ORDER_SHOWN_AS: u32 = 1;

/// How long a rider may stay indoors before the ride ends, in seconds: walked in (2017: 3.5 to
/// 4.1 s, 5 rides) and arrived by portal (1.2 s, 1 ride).
pub const WALKED_IN: f64 = 3.8;
pub const ARRIVED_IN: f64 = 1.2;

/// The mount items the world's catalog always holds: the ephemeral, steady and lasting saddles of
/// the five Ataurs and the Crystalline Tyrant Eggs (2017: 168 saddle mounts, 103 egg mounts).
pub const ITEMS: [u32; 18] = [
    0x4700_089E,
    0x4700_089F,
    0x4700_08A0,
    0x4700_08A2,
    0x4700_08A3,
    0x4700_08A4,
    0x4700_08A6,
    0x4700_08A7,
    0x4700_08A8,
    0x4700_08AA,
    0x4700_08AB,
    0x4700_08AC,
    0x4700_08AE,
    0x4700_08AF,
    0x4700_08B0,
    0x4700_14CA,
    0x4700_14CB,
    0x4700_14CC,
];

/// The script package mount effects derive from, and the singleton of the mount table.
pub const MOUNT_EFFECT: &str = "MountEffect";
pub const MOUNT_TABLE: &str = "MountTable";

/// What a mount effect is beside its numbers: its type (the mount table's key) and the
/// appearance its end plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MountEffect {
    pub mount_type: u32,
    pub ending_fx: u32,
}

/// The mounts the world knows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mounts {
    /// The mount effects, by saved state.
    pub effects: HashMap<u32, MountEffect>,
    /// The item templates whose use mounts their user, and the mount effect each puts on.
    pub items: HashMap<u32, u32>,
    /// The mount table: (mount type, race, sex) to the mounted look's description.
    pub looks: HashMap<(u32, u32, u32), VisualDesc>,
    /// The appearance each body's own description gives, by the description.
    pub bodies: HashMap<u32, Table<FloatTable>>,
}

/// A ride under way: its record, its effect, the look the rider shows and its speed.
#[derive(Debug, Clone, PartialEq)]
pub struct Riding {
    pub eid: u32,
    pub state: u32,
    pub look: VisualDesc,
    pub speed: f32,
    /// A check of the rider indoors is due.
    pub checking: bool,
}

impl Mounts {
    /// The mounts of an install: the mount effects among these saved states (each with its
    /// package), the mount table's looks (`record` gives a record's bytes) and the default
    /// appearance of these bodies.
    #[must_use]
    pub fn read(
        lib: &Library,
        states: &[(u32, u32)],
        graph: &dyn Fn(u32) -> Option<Graph>,
        record: &dyn Fn(u32) -> Option<Vec<u8>>,
        bodies: &[u32],
    ) -> Self {
        let mut m = Self::default();
        let Some(base) = lib.package_id(MOUNT_EFFECT) else {
            return m;
        };
        for &(state, package) in states {
            if !lib.derives_from(package, base) {
                continue;
            }
            let Some(g) = graph(state) else { continue };
            let nav = Nav::new(lib, &g);
            let Some(root) = g.root().and_then(|r| nav.object(r).ok()) else {
                continue;
            };
            let (Ok(mount_type), Ok(ending_fx)) = (
                nav.field(root, "m_mountType"),
                nav.field(root, "m_endingFxID"),
            ) else {
                continue;
            };
            m.effects.insert(
                state,
                MountEffect {
                    mount_type,
                    ending_fx,
                },
            );
        }
        if let Some(g) = lib
            .package_id(MOUNT_TABLE)
            .and_then(|p| graph(0x7000_0000 | p))
        {
            for ((mount_type, race, sex), did) in table_of(lib, &g) {
                let look = record(did).and_then(|b| VisualDesc::read(&mut Reader::new(&b)).ok());
                if let Some(look) = look {
                    m.looks.insert((mount_type, race, sex), look);
                }
            }
        }
        for &body in bodies {
            let look = record(body).and_then(|b| VisualDesc::read(&mut Reader::new(&b)).ok());
            if let Some(look) = look.filter(|l| l.global.bits & PartGroupDesc::APPEARANCE != 0) {
                m.bodies.insert(body, look.global.appearance);
            }
        }
        m
    }

    /// Note the item templates whose use puts on a mount effect (their effects as the items'
    /// uses resolve them), and the mount effect each puts on.
    pub fn note_items(&mut self, uses: &crate::item_use::ItemUses) {
        for (template, list) in uses {
            if let Some(e) = list.iter().find(|e| self.effects.contains_key(&e.state)) {
                self.items.insert(*template, e.state);
            }
        }
    }

    /// The look of a rider of this race and sex, whose own look is `own`, on a mount of this type.
    #[must_use]
    pub fn mounted_look(
        &self,
        mount_type: u32,
        race: u32,
        sex: u32,
        own: &VisualDesc,
    ) -> Option<VisualDesc> {
        let mut look = self.looks.get(&(mount_type, race, sex))?.clone();
        look.bits &= !VisualDesc::DID;
        look.did = 0;
        let defaults = self.bodies.get(&own.parent);
        let entries = own
            .global
            .appearance
            .entries
            .iter()
            .map(|(key, values)| {
                let mut merged: Vec<(u32, f32)> = defaults
                    .and_then(|d| d.get(*key))
                    .map(|d| d.entries.clone())
                    .unwrap_or_default();
                for (k, v) in &values.entries {
                    match merged.iter_mut().find(|(m, _)| m == k) {
                        Some(e) => e.1 = *v,
                        None => merged.push((*k, *v)),
                    }
                }
                (*key, Table::from_inserted(values.size_class, merged))
            })
            .collect();
        look.global.appearance = Table {
            size_class: own.global.appearance.size_class,
            entries,
        };
        look.global.bits |= PartGroupDesc::APPEARANCE;
        Some(look)
    }
}

/// The mount table's entries: (mount type, race, sex) and the look's description.
fn table_of(lib: &Library, g: &Graph) -> Vec<((u32, u32, u32), u32)> {
    let nav = Nav::new(lib, g);
    let Some(root) = g.root().and_then(|r| nav.object(r).ok()) else {
        return Vec::new();
    };
    let Ok(table) = nav.field(root, "mMountTable") else {
        return Vec::new();
    };
    let word = |i: Item| Nav::word(i).ok();
    let mut out = Vec::new();
    for (k, v) in nav.entries(table).unwrap_or_default() {
        let (Some(mount_type), Some(races)) = (word(k), v.reference()) else {
            continue;
        };
        for (k, v) in nav.entries(races).unwrap_or_default() {
            let (Some(race), Some(sexes)) = (word(k), v.reference()) else {
                continue;
            };
            for (k, v) in nav.entries(sexes).unwrap_or_default() {
                if let (Some(sex), Some(did)) = (word(k), word(v)) {
                    out.push(((mount_type, race, sex), did));
                }
            }
        }
    }
    out
}

/// Is the player on a mount?
#[must_use]
pub fn mounted(p: &crate::world::Player) -> bool {
    p.riding.is_some()
}

/// The player's own quality that says whether it is mounted.
fn flag(on: bool) -> ServerMessage {
    ServerMessage::UpdateBool(auberean_protocol::quality::PrivateQuality {
        quality: MOUNTED,
        value: i32::from(on),
    })
}

/// A player uses an item it holds: a mount item mounts it, or the use is refused. Returns false
/// for any other item.
pub fn used<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: &Use,
    clock: Clock,
) -> bool {
    let Some(template) = s.things.get(u.item).map(|t| t.template) else {
        return false;
    };
    let Some(state) = s.mounts.items.get(&template).copied() else {
        return false;
    };
    let Some(props) = s.catalog.get(template).map(|t| t.props.clone()) else {
        return false;
    };
    if crate::item_use::refuse(s, net, world, u, &props, clock) {
        return true;
    }
    mount(s, net, world, u, template, state, clock);
    true
}

/// Put a player on the mount of an item's effect.
fn mount<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    u: &Use,
    template: u32,
    state: u32,
    clock: Clock,
) {
    let (Some(def), Some(kind)) = (
        s.effect_table.get(state).cloned(),
        s.mounts.effects.get(&state).copied(),
    ) else {
        s.unhandled.note(format!(
            "a mount effect {state:08X} the server does not know"
        ));
        return;
    };
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let (me, race, sex, own) = (p.character, p.race, p.sex, p.look.clone());
    let Some(look) = s.mounts.mounted_look(kind.mount_type, race, sex, &own) else {
        s.unhandled.note(format!(
            "no mounted look for mount type {:08X}, race {race}, sex {sex:#x}",
            kind.mount_type
        ));
        return;
    };
    progress::private(net, world, &flag(true));
    if def.fx != 0 {
        effects::show_fxs(s, net, world, &[def.fx]);
    }
    let lasts = !def.duration.is_empty();
    let shape = Shape {
        flags: if lasts {
            record::TIMED
        } else {
            record::UNTIL_OFF
        },
        categories: Some(effects::category::ORDINARY),
        timeout: (!lasts).then_some(f32::MAX),
        acting: 0,
    };
    let cast = Cast {
        caster: me,
        spellcraft: 1.0,
        pk: false,
        skill: 0,
        item: u.item,
        giver: None,
        answers: false,
    };
    let Some(eid) = effects::add_quietly(s, net, On::Player(world), &def, cast, shape, clock)
    else {
        return;
    };
    // The item's other effects (an egg's focus cost) as the mount goes on.
    let others: Vec<crate::item_use::UsageEffect> = s
        .item_uses
        .get(&template)
        .map(|l| l.iter().filter(|e| e.state != state).copied().collect())
        .unwrap_or_default();
    for e in others {
        crate::item_use::put_on(s, net, world, me, u.item, e, clock);
    }
    let spent = s
        .catalog
        .get(template)
        .is_some_and(|t| t.props.flag("Item_IsDestroyOnUse"));
    if spent {
        crate::craft::use_up(s, net, world, u.item, 1);
    }
    let speed = effects::player_speed(s, world).0;
    let flags = ANSWER_FLAGS | if spent { SPENT } else { 0 };
    let done = crate::places::use_done(u, me, flags, false, 0.0);
    progress::private(net, world, &done);
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    p.riding = Some(Riding {
        eid,
        state,
        look: look.clone(),
        speed,
        checking: false,
    });
    let shown = ServerMessage::UpdateVisualDesc(crate::combat::player_event(p, look));
    crate::combat::to_all_seeing(s, net, me, &shown);
    crate::world::keep_player(s, world);
}

/// A player asks to dismount: a follow ends, then the ride. Not mounted, it is not answered.
pub fn dismount<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    if !s.players.get(&world).is_some_and(mounted) {
        return;
    }
    crate::follow::stop(s, net, world);
    end(s, net, world);
}

/// The ride ends: the player told it is not mounted, its own look shown, the record taken off (its
/// speed back), the mount's appearance stopped and its ending appearance played.
pub fn end<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let Some(ride) = p.riding.take() else {
        return;
    };
    let me = p.character;
    progress::private(net, world, &flag(false));
    let shown = ServerMessage::UpdateVisualDesc(crate::combat::player_event(p, p.look.clone()));
    crate::combat::to_all_seeing(s, net, me, &shown);
    if let Some(Some(fx)) = effects::take_off(s, net, On::Player(world), ride.eid) {
        effects::stop_fx(s, net, world, &[fx]);
    }
    let ending = s.mounts.effects.get(&ride.state).map_or(0, |m| m.ending_fx);
    if ending != 0 {
        effects::show_fxs(s, net, world, &[ending]);
    }
    crate::world::keep_player(s, world);
}

/// A player's record's time is up: a ride's ends the ride. Returns false for any other record.
pub fn record_ends<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    eid: u32,
) -> bool {
    let riding = s
        .players
        .get(&world)
        .and_then(|p| p.riding.as_ref())
        .is_some_and(|r| r.eid == eid);
    if riding {
        end(s, net, world);
    }
    riding
}

/// A rider reported where it is: indoors, the ride ends a few seconds on unless it is out again.
pub fn moved<S: Store>(s: &mut Server<S>, world: ConnId, clock: Clock) {
    check_indoors(s, world, WALKED_IN, clock);
}

/// A rider came out of portal space: inside, the ride ends a moment later.
pub fn arrived<S: Store>(s: &mut Server<S>, world: ConnId, clock: Clock) {
    check_indoors(s, world, ARRIVED_IN, clock);
}

fn check_indoors<S: Store>(s: &mut Server<S>, world: ConnId, wait: f64, clock: Clock) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let inside = crate::presence::indoors(p.position.cell);
    let Some(ride) = p.riding.as_mut().filter(|r| inside && !r.checking) else {
        return;
    };
    ride.checking = true;
    let eid = ride.eid;
    s.after(clock, wait, Timer::MountIndoors { world, eid });
}

/// A rider's check indoors is due: still riding the same mount and still indoors, the ride ends.
pub fn indoors_due<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, eid: u32) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let inside = crate::presence::indoors(p.position.cell);
    let Some(ride) = p.riding.as_mut().filter(|r| r.eid == eid) else {
        return;
    };
    ride.checking = false;
    if inside {
        end(s, net, world);
    }
}

/// A pet attack order from a mounted player is refused, with its text. Returns whether it was.
pub fn refuses_pet_order<S: Store>(s: &Server<S>, net: &mut ServerNet, world: ConnId) -> bool {
    if !s.players.get(&world).is_some_and(mounted) {
        return false;
    }
    let said = crate::chat::show_text(PET_ORDER_SHOWN_AS, progress::text(PET_ORDER_REFUSED));
    progress::private(net, world, &said);
    true
}

/// The look a player shows others: a rider's mounted look, else its own.
#[must_use]
pub fn shown_look(p: &crate::world::Player) -> &VisualDesc {
    p.riding.as_ref().map_or(&p.look, |r| &r.look)
}

/// The speed scale a rider shows others as they first see it.
#[must_use]
pub fn shown_speed(p: &crate::world::Player) -> Option<f32> {
    p.riding.as_ref().map(|r| r.speed)
}

/// Keep the ride across a refresh of its record's look (an item worn while riding changes the
/// rider's own look, and so the mounted one).
pub fn look_changed<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let Some(ride) = p.riding.as_ref() else {
        return;
    };
    let Some(kind) = s.mounts.effects.get(&ride.state).copied() else {
        return;
    };
    let Some(look) = s
        .mounts
        .mounted_look(kind.mount_type, p.race, p.sex, &p.look)
    else {
        return;
    };
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let me = p.character;
    if let Some(r) = p.riding.as_mut() {
        r.look = look.clone();
    }
    let shown = ServerMessage::UpdateVisualDesc(crate::combat::player_event(p, look));
    crate::combat::to_all_seeing(s, net, me, &shown);
}
