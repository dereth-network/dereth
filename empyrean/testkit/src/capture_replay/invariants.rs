//! Stream invariants, checked on a recording (ACE's truth) and on our replay with the same code.
//!
//! Each invariant was first asserted on every ACE recording. Where ACE breaks one, the invariant is
//! either narrowed to what ACE does (an **exemption**, named with the ACE member responsible, and
//! reported as *documented* rather than violated) or dropped and only counted (an
//! **observation**).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::recording::Msg;
use super::wire::{self, u16_at, u32_at};

/// The invariants, by id, with what each asserts.
pub const INVARIANTS: [(&str, &str); 9] = [
    ("event-sequence", "game events are numbered 1, 2, 3, ... from each login, in send order"),
    ("event-target", "every game event is addressed to the logged-in character"),
    ("known-object", "every object update names an object created earlier on the connection and not deleted since"),
    ("delete-known", "every DeleteObject names an object created earlier and not deleted since"),
    ("object-sequences", "per object, since its create: position, motion, state and vector sequences only move forward"),
    ("private-sequences", "per private property, since the login: the update sequence only moves forward"),
    ("use-done", "UseDone never outnumbers the requests that end with one (Use, UseWithTarget, casts, Buy, Sell)"),
    ("vitals", "UpdateHealth is a fraction in [0, 1]; vital currents are non-negative"),
    ("login-shape", "each login sends one PlayerDescription, then one PlayerCreate, then the character's CreateObject"),
];

/// The exemptions: an invariant ACE breaks in a narrow, explained way.
pub const EXEMPTIONS: [(&str, &str, &str); 3] = [
    (
        "early-announce",
        "known-object",
        "an update whose object's CreateObject follows within 2 s: WorldObject.EnterWorld broadcasts PlayScript.Create, \
         Creature.TryEquipObjectWithBroadcasting a ParentEvent and Creature.LaunchProjectile a PlayerKillerStatus update \
         before the viewers' ObjectMaint sends the create",
    ),
    (
        "health-after-logoff",
        "event-target",
        "UpdateHealth addressed to 0 after the character logged off: Creature.OnHealthUpdate keeps the logged-off \
         player's selection and sends through its Session, whose Player is gone",
    ),
    (
        "death-vitals",
        "private-sequences",
        "a vital update one sequence behind the previous one in the same send: Player.ThreadSafeTeleportOnDeath builds \
         its three vital messages, then UpdateVital sends newer ones before them",
    ),
];

/// Observations: properties ACE does not have, counted only.
pub const OBSERVATIONS: [(&str, &str); 2] = [
    (
        "created-again",
        "a CreateObject for an object the client already has (no DeleteObject between): ACE re-sends creates when an \
         object comes back into view (the client culls objects that leave its range; ObjectMaint re-adds them) and \
         when a container is opened",
    ),
    ("action-sequence", "the client's game-action sequence restarts or repeats (a client property; not the server's)"),
];

/// One violation (or one documented exemption hit): the invariant, the message index, and what was
/// wrong, as opcodes, guids and numbers only.
#[derive(Debug, Clone)]
pub struct Hit {
    pub invariant: &'static str,
    pub index: usize,
    pub t: f64,
    pub detail: String,
}

/// The outcome of checking one stream.
#[derive(Debug, Clone, Default)]
pub struct InvariantReport {
    /// How many times each invariant was checked.
    pub checks: BTreeMap<&'static str, usize>,
    pub violations: Vec<Hit>,
    /// Exemption hits, by exemption id.
    pub documented: BTreeMap<&'static str, usize>,
    /// Observation counts, by id.
    pub observed: BTreeMap<&'static str, usize>,
}

impl InvariantReport {
    /// Violation counts by invariant.
    #[must_use]
    pub fn violation_counts(&self) -> BTreeMap<&'static str, usize> {
        let mut m = BTreeMap::new();
        for v in &self.violations {
            *m.entry(v.invariant).or_insert(0) += 1;
        }
        m
    }
}

/// The early-announce window: how long after an update its object's create may follow.
pub const EARLY_ANNOUNCE_S: f64 = 2.0;

/// Checks one stream (see [`INVARIANTS`]).
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn check(msgs: &[Msg]) -> InvariantReport {
    let mut r = InvariantReport::default();
    let bump = |r: &mut InvariantReport, id: &'static str| *r.checks.entry(id).or_insert(0) += 1;

    // the index of every create, by guid, for the early-announce look-ahead
    let mut creates: HashMap<u32, Vec<usize>> = HashMap::new();
    for (i, m) in msgs.iter().enumerate() {
        if !m.c2s && matches!(m.opcode(), wire::CREATE_OBJECT | wire::UPDATE_OBJECT) {
            if let Some(g) = u32_at(&m.payload, 4) {
                creates.entry(g).or_default().push(i);
            }
        }
    }
    let created_soon = |g: u32, i: usize, t: f64| {
        creates.get(&g).is_some_and(|v| {
            v.iter()
                .any(|&j| j > i && msgs[j].t - t <= EARLY_ANNOUNCE_S)
        })
    };

    let mut player: Option<u32> = None;
    let mut logged_off = false;
    let mut next_event: Option<u32> = None;
    let mut known: BTreeSet<u32> = BTreeSet::new();
    let mut obj_seq: HashMap<(u32, u8), u16> = HashMap::new();
    let mut private_seq: HashMap<(u32, u32), (u8, f64)> = HashMap::new();
    let (mut use_requests, mut use_done) = (0usize, 0usize);
    let mut last_action: Option<u32> = None;
    // login shape: (PlayerDescription seen, PlayerCreate seen, own create seen)
    let mut login: Option<(usize, usize, bool)> = None;

    let finish_login =
        |r: &mut InvariantReport, login: Option<(usize, usize, bool)>, i: usize, t: f64| {
            if let Some((pd, pc, own)) = login {
                *r.checks.entry("login-shape").or_insert(0) += 1;
                if pd != 1 || pc != 1 || !own {
                    r.violations.push(Hit {
                        invariant: "login-shape",
                        index: i,
                        t,
                        detail: format!(
                            "PlayerDescription x{pd}, PlayerCreate x{pc}, own create {own}"
                        ),
                    });
                }
            }
        };

    for (i, m) in msgs.iter().enumerate() {
        let p = &m.payload;
        let op = m.opcode();
        let hit = |r: &mut InvariantReport, invariant: &'static str, detail: String| {
            r.violations.push(Hit {
                invariant,
                index: i,
                t: m.t,
                detail,
            });
        };
        if m.c2s {
            match op {
                wire::ENTER_WORLD => {
                    finish_login(&mut r, login.take(), i, m.t);
                    player = u32_at(p, 4);
                    logged_off = false;
                    next_event = Some(1);
                    private_seq.clear();
                    login = Some((0, 0, false));
                }
                wire::GAME_ACTION => {
                    if wire::action_type(p).is_some_and(|a| wire::USE_DONE_REQUESTS.contains(&a)) {
                        use_requests += 1;
                    }
                    let stamp = u32_at(p, 4);
                    if let (Some(a), Some(b)) = (stamp, last_action) {
                        if a <= b {
                            *r.observed.entry("action-sequence").or_insert(0) += 1;
                        }
                    }
                    last_action = stamp;
                }
                _ => {}
            }
            continue;
        }

        match op {
            wire::CREATE_OBJECT | wire::UPDATE_OBJECT => {
                if let Some(g) = u32_at(p, 4) {
                    if op == wire::CREATE_OBJECT && !known.insert(g) {
                        *r.observed.entry("created-again").or_insert(0) += 1;
                    }
                    known.insert(g);
                    obj_seq.retain(|k, _| k.0 != g);
                    if Some(g) == player {
                        if let Some(l) = login.as_mut() {
                            l.2 = l.1 > 0;
                        }
                    }
                }
            }
            wire::PLAYER_CREATE => {
                if let Some(l) = login.as_mut() {
                    if l.0 == 0 {
                        // a PlayerCreate before the PlayerDescription
                        l.0 = 99;
                    }
                    l.1 += 1;
                }
            }
            wire::DELETE_OBJECT => {
                if let Some(g) = u32_at(p, 4) {
                    bump(&mut r, "delete-known");
                    if !known.remove(&g) {
                        hit(&mut r, "delete-known", format!("DeleteObject {g:08X}"));
                    }
                }
            }
            wire::LOGOFF => {
                logged_off = true;
                finish_login(&mut r, login.take(), i, m.t);
            }
            _ => {}
        }

        // known-object
        if wire::is_object_update(op) {
            for g in wire::subject(p).into_iter().chain(wire::other_named(p)) {
                bump(&mut r, "known-object");
                if Some(g) == player || known.contains(&g) {
                    continue;
                }
                if created_soon(g, i, m.t) {
                    *r.documented.entry("early-announce").or_insert(0) += 1;
                } else {
                    hit(
                        &mut r,
                        "known-object",
                        format!("{} about {g:08X}", wire::label(p)),
                    );
                }
            }
        }

        // object-sequences
        let seq = match op {
            wire::UPDATE_POSITION => p
                .len()
                .checked_sub(6)
                .and_then(|at| u16_at(p, at))
                .map(|s| (0u8, s)),
            wire::UPDATE_MOTION => u16_at(p, 10).map(|s| (1, s)),
            wire::SET_STATE => u16_at(p, 14).map(|s| (2, s)),
            wire::VECTOR_UPDATE => u16_at(p, 34).map(|s| (3, s)),
            _ => None,
        };
        if let (Some((kind, s)), Some(g)) = (seq, u32_at(p, 4)) {
            bump(&mut r, "object-sequences");
            if let Some(prev) = obj_seq.insert((g, kind), s) {
                if !wire::newer16(s, prev) {
                    hit(
                        &mut r,
                        "object-sequences",
                        format!("{} {g:08X}: {s} after {prev}", wire::label(p)),
                    );
                }
            }
        }

        // private-sequences
        if wire::PRIVATE_UPDATES.contains(&op) {
            if let (Some(&s), Some(key)) = (p.get(4), u32_at(p, 5)) {
                bump(&mut r, "private-sequences");
                if let Some((prev, prev_t)) = private_seq.insert((op, key), (s, m.t)) {
                    if !wire::newer8(s, prev) {
                        if op == 0x02E9 && s.wrapping_add(1) == prev && (m.t - prev_t).abs() < 0.05
                        {
                            *r.documented.entry("death-vitals").or_insert(0) += 1;
                        } else {
                            hit(
                                &mut r,
                                "private-sequences",
                                format!("{} key {key}: {s} after {prev}", wire::label(p)),
                            );
                        }
                    }
                }
            }
        }

        // vitals
        if op == 0x02E9 || op == 0x02E7 {
            let at = if op == 0x02E9 { 9 } else { 21 };
            if let Some(cur) = u32_at(p, at) {
                bump(&mut r, "vitals");
                if cur > 0x7FFF_FFFF {
                    hit(
                        &mut r,
                        "vitals",
                        format!("{} current {}", wire::label(p), cur.cast_signed()),
                    );
                }
            }
        }

        // game events
        if let Some(e) = wire::event_type(p) {
            bump(&mut r, "event-sequence");
            let n = u32_at(p, 8).unwrap_or(0);
            if let Some(expected) = next_event {
                if n != expected {
                    hit(
                        &mut r,
                        "event-sequence",
                        format!("ev {e:04X} numbered {n}, expected {expected}"),
                    );
                }
            }
            next_event = Some(n.wrapping_add(1));

            bump(&mut r, "event-target");
            let target = u32_at(p, 4).unwrap_or(0);
            if player.is_some_and(|pl| pl != target) {
                if logged_off && target == 0 && e == wire::EV_UPDATE_HEALTH {
                    *r.documented.entry("health-after-logoff").or_insert(0) += 1;
                } else {
                    hit(
                        &mut r,
                        "event-target",
                        format!("ev {e:04X} to {target:08X}"),
                    );
                }
            }

            if e == wire::EV_PLAYER_DESCRIPTION {
                if let Some(l) = login.as_mut() {
                    l.0 += 1;
                }
            }
            if e == wire::EV_USE_DONE {
                use_done += 1;
                bump(&mut r, "use-done");
                if use_done > use_requests {
                    hit(
                        &mut r,
                        "use-done",
                        format!("UseDone #{use_done} after {use_requests} requests"),
                    );
                }
            }
            if e == wire::EV_UPDATE_HEALTH {
                if let Some(f) = wire::f32_at(p, 20) {
                    bump(&mut r, "vitals");
                    if !(0.0..=1.0).contains(&f) {
                        hit(&mut r, "vitals", format!("UpdateHealth {f}"));
                    }
                }
            }
        }
    }
    finish_login(
        &mut r,
        login.take(),
        msgs.len(),
        msgs.last().map_or(0.0, |m| m.t),
    );
    r
}
