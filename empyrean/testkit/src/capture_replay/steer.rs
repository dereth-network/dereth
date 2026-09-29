//! Steering: anchoring the recording's monster deaths in our world.
//!
//! The recorded client attacks a monster only until ACE's rolls kill it. When ours rolled worse,
//! the monster lives on in our world, nothing in the recording attacks it again, and it fights
//! the character for the rest of the session (selection bias, not a server fault). So
//! when the recording shows a monster's death, the translated monster in our world, if it is
//! still alive at that point, is killed there through the server's own death path:
//! `Creature.Smite` with `useTakeDamage`, the character dealing the monster's remaining health
//! through `Creature.TakeDamage`, which runs `OnDeath` and `Die` as a lethal hit does (the
//! killer's notification, XP by damage share, the Dead motion, the corpse).
//!
//! Each anchored death is counted as `steered` and reported per session, so a real defect in our
//! combat cannot hide behind it.
//!
//! **Evidence of a death** in the recording (numbers only, never payload text): the monster's
//! UpdateMotion whose interpreted forward command is `Dead` (`Creature.Die` broadcasts it at the
//! lethal hit), or, when no such motion reached the client, the monster's DeleteObject
//! (`Creature.Die`'s chain destroys it after the corpse is made). The kill event
//! (`KillerNotification`) names no object, so it only confirms. A monster is a creature the
//! recording created (ItemType Creature) that is not a player.
//!
//! **Only translated monsters** are steered: a recorded monster the replay has bound to one of
//! ours (the recorded client named it). A recorded death that names a monster the replay never
//! bound is not anchored: which of ours it would be is a guess.

use std::collections::{HashMap, HashSet};

use empyrean_entity::ObjectGuid;
use empyrean_world::dispatch::{class_of, Class};
use empyrean_world::world_objects::{creature_death, monster_combat};
use empyrean_world::World;

use super::wire::{self, u16_at, u32_at};

/// A recorded death is anchored this long after its recorded time (on our clock): our lethal hit,
/// when the rolls agree, lands within a few 1/60 s steps of ACE's, and must not be pre-empted.
pub const GRACE_S: f64 = 0.5;

/// While one of our projectiles (a spell or a missile) is still in flight at the monster, the
/// anchor waits for it, up to this long past its due time: our lethal hit may be that projectile,
/// landing later than ACE's did (a longer flight).
pub const IN_FLIGHT_S: f64 = 3.0;

/// `MotionCommand.Dead` (0x40000011), as the interpreted motion state carries it (low 16 bits).
const DEAD: u16 = 0x0011;

/// `ItemType.Creature`.
const ITEM_TYPE_CREATURE: u32 = 0x10;

/// Whether a CreateObject's item type is a creature's, from its (weenie, item type, flags).
#[must_use]
pub const fn is_creature_type(obj_type: u32) -> bool {
    obj_type & ITEM_TYPE_CREATURE != 0
}

/// The monster an UpdateMotion shows dying: its interpreted motion's forward command is `Dead`.
/// Layout: opcode, guid, instance sequence (u16), movement data (movement and server-control
/// sequences, autonomous, aligned), movement type at 16 (0: interpreted), then the interpreted
/// state's flags at 20 (bit 0: current style u16, bit 1: forward command u16).
#[must_use]
pub fn dead_motion(p: &[u8]) -> Option<u32> {
    if u32_at(p, 0)? != wire::UPDATE_MOTION || *p.get(16)? != 0 {
        return None;
    }
    let flags = u32_at(p, 20)?;
    if flags & 2 == 0 {
        return None;
    }
    let at = if flags & 1 != 0 { 26 } else { 24 };
    (u16_at(p, at)? == DEAD).then(|| u32_at(p, 4)).flatten()
}

/// The monster a recorded server message shows dying (see the module docs), given the recorded
/// creatures (non-player guids whose CreateObject said Creature).
#[must_use]
pub fn recorded_death(p: &[u8], creatures: &HashSet<u32>) -> Option<u32> {
    let g = match u32_at(p, 0)? {
        wire::UPDATE_MOTION => dead_motion(p)?,
        wire::DELETE_OBJECT => u32_at(p, 4)?,
        _ => return None,
    };
    (creatures.contains(&g) && !wire::is_player_guid(g)).then_some(g)
}

/// A death anchored and not yet applied.
#[derive(Debug, Clone, Copy)]
struct Pending {
    /// Our time at which it is applied.
    due: f64,
    /// Our monster.
    monster: u32,
    /// Our character, the killer.
    killer: u32,
    phase: usize,
}

/// The anchored deaths of one replay.
#[derive(Debug, Default)]
pub struct Steering {
    /// Recorded monsters whose death was seen (a death is anchored once).
    seen: HashSet<u32>,
    pending: Vec<Pending>,
    /// Every death applied: (phase, our monster).
    pub steered: Vec<(usize, u32)>,
    /// Recorded deaths not steered: the monster was not translated, already dead or gone in
    /// ours, or no character of ours was in the world. (phase, recorded monster, why).
    pub not_steered: Vec<(usize, u32, &'static str)>,
}

impl Steering {
    /// A recorded server message: when it shows a monster's death (the first evidence of it), the
    /// death is anchored at `due` (our clock) on the monster `bound` maps it to, with `killer`
    /// (our character in the world) as the killer.
    pub fn observe(
        &mut self,
        p: &[u8],
        creatures: &HashSet<u32>,
        bound: &HashMap<u32, u32>,
        killer: Option<u32>,
        due: f64,
        phase: usize,
    ) {
        let Some(g) = recorded_death(p, creatures) else {
            return;
        };
        if !self.seen.insert(g) {
            return;
        }
        match (bound.get(&g), killer) {
            (Some(&monster), Some(killer)) => self.pending.push(Pending {
                due: due + GRACE_S,
                monster,
                killer,
                phase,
            }),
            (None, _) => self.not_steered.push((phase, g, "untranslated")),
            (Some(_), None) => self.not_steered.push((phase, g, "no character")),
        }
    }

    /// Applies every anchored death that is due by `now`: a monster still alive in our world is
    /// killed by our character through `Creature.Smite(useTakeDamage)`. `ours` names the objects
    /// our server has created (the projectiles among them): while one is in flight at the monster,
    /// the death waits (see [`IN_FLIGHT_S`]).
    pub fn apply(&mut self, w: &mut World, now: f64, ours: impl Iterator<Item = u32> + Clone) {
        if self.pending.iter().all(|p| p.due > now) {
            return;
        }
        let (due, later): (Vec<Pending>, Vec<Pending>) =
            self.pending.drain(..).partition(|p| p.due <= now);
        self.pending = later;
        for p in due {
            let (monster, killer) = (ObjectGuid::new(p.monster), ObjectGuid::new(p.killer));
            let in_flight = ours.clone().any(|g| {
                w.objects
                    .get(ObjectGuid::new(g))
                    .and_then(|o| o.projectile)
                    .is_some_and(|l| l.target == Some(monster))
            });
            if in_flight && now < p.due + IN_FLIGHT_S {
                self.pending.push(p);
                continue;
            }
            let alive = w
                .objects
                .get(monster)
                .is_some_and(|o| !monster_combat::is_dead(o))
                && class_of(w, monster) == Class::Creature;
            if !alive {
                self.not_steered.push((p.phase, p.monster, "dead in ours"));
                continue;
            }
            if !w.objects.contains(killer) {
                self.not_steered.push((p.phase, p.monster, "no character"));
                continue;
            }
            creature_death::smite(w, monster, killer, true);
            self.steered.push((p.phase, p.monster));
        }
    }
}
