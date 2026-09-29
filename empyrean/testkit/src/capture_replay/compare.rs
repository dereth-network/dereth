//! The comparison of our stream with the recording's.
//!
//! **Scopes.** Each server message is either *own* (about the character or its possessions, or
//! addressed to it: game events, private updates, chat, the login messages) or *ambient* (about
//! any other object: the world's statics, spawns, monsters, other players). The own scope is what
//! the client's actions cause; it is compared per phase. The ambient scope depends on the world's
//! own clock (generators, AI, other players) and is compared per session as label counts; its
//! static objects are compared as guid sets.
//!
//! **Phases.** Both streams are cut at every client message (the same messages on both sides). A
//! phase opened by a movement message is folded into the phase before it, so a phase is "what the
//! server said after this action, until the next action".
//!
//! **Levels.** Per phase and label the smaller count is *matched*. The rest is classified by the
//! rules in [`RULES`], in order; what no rule explains is *unexplained*, carried as a
//! [`Finding`] keyed by (opening action, label, direction) with the `not_ported!` sites hit in
//! those phases, which name the ACE members responsible.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use dereth_protocol::opcodes::Opcode;

use super::invariants::{self, InvariantReport};
use super::recording::Msg;
use super::replay::ReplayOutcome;
use super::wire::{self, u32_at};

/// The explanation rules, in the order they are tried: (id, what it explains).
pub const RULES: [(&str, &str); 7] = [
    ("tick-boundary", "the same message one phase earlier or later: our tick and ACE's split the reply differently"),
    (
        "recorded-silence",
        "a phase after the recorded server's last message: the recording's server had stopped answering (the error condition the unclean-logout recordings were made for), ours had not",
    ),
    (
        "position-cadence",
        "the character's own UpdatePosition / VectorUpdate count: Player_Tick.UpdatePlayerPosition's cadence and the \
         physics timing",
    ),
    ("vital-tick", "a vital's current value: Creature.VitalHeartBeat regenerates on the world clock, not per action"),
    (
        "other-client",
        "a phase in which the recording shows another player: that player's client is not replayed (fellowship, \
         tells, trade, emotes and their answers)",
    ),
    ("untranslated", "the action names an object the replay could not find in our world (see the notes)"),
    ("recorded-only-login", "a session that never enters the world: its login answer only"),
];

/// The reported categories that are neither explained nor unexplained: what the recording
/// proves another player's client caused (`unreplayed-client`), and what follows a combat or magic
/// roll whose outcome differs from the recording's (`rng-divergent`). Each is counted apart from
/// matched, explained and unexplained.
pub const CATEGORIES: [(&str, &str); 2] = [
    (
        "unreplayed-client",
        "a message ACE sent that names another player's character (its guid in the message), or any \
         difference during a delayed PK logoff (Player.LogOut's PKLogoutActive, set only by \
         player-versus-player damage) in a recording that shows another player",
    ),
    (
        "rng-divergent",
        "in a phase whose roll outcomes differ from the recording's (hit, evade, kill, death, fizzle \
         counts), the differences of the labels that follow from those rolls",
    ),
];

/// A roll's outcome, as the player sees it: its attack hit (`AttackerNotification`), was evaded
/// (`EvasionAttackerNotification`), killed (`VictimNotificationOther`); a monster's attack hit it
/// (`DefenderNotification`) or it evaded (`EvasionDefenderNotification`); it died
/// (`VictimNotificationSelf`). A fizzle (`WeenieError.YourSpellFizzled`) is counted apart.
const ROLL_OUTCOMES: [&str; 6] = [
    "ev 01B1", "ev 01B3", "ev 01AD", "ev 01B2", "ev 01B4", "ev 01AC",
];

/// What follows a roll in the same phase: the outcomes themselves, health (`UpdateHealth`), attack
/// done and commence, XP and skill XP, the swing and death motions, sounds and scripts, combat text,
/// and the corpse's create and the victim's delete.
const ROLL_CONSEQUENCES: [&str; 17] = [
    "ev 01B1", "ev 01B3", "ev 01AD", "ev 01B2", "ev 01B4", "ev 01AC", "ev 01C0", "ev 01A7",
    "ev 01B8", "02CF", "02DD", "F74C", "F750", "F755", "F7E0", "F745", "F747",
];

/// `WeenieError.YourSpellFizzled` in a `GameEventWeenieError`.
const FIZZLED: u32 = 0x0402;

/// The kinds of roll outcome [`roll_outcome`] tells apart: the six of [`ROLL_OUTCOMES`] and the
/// fizzle.
pub const ROLL_KINDS: usize = ROLL_OUTCOMES.len() + 1;

/// Which roll outcome a server message (with its label) is: an index into [`ROLL_OUTCOMES`], or
/// `ROLL_OUTCOMES.len()` for a fizzle. The replay counts them per phase on both sides, to cap its
/// waits in a phase whose outcomes already differ.
#[must_use]
pub fn roll_outcome(p: &[u8], label: &str) -> Option<usize> {
    ROLL_OUTCOMES.iter().position(|l| *l == label).or_else(|| {
        (wire::event_type(p) == Some(0x028A) && u32_at(p, 16) == Some(FIZZLED))
            .then_some(ROLL_OUTCOMES.len())
    })
}

/// The character's own death (`VictimNotificationSelf`).
const OWN_DEATH: &str = "ev 01AC";

/// A delayed PK logoff: the recording's LogOffComplete this long after the request. ACE's plain
/// logoff completes after the logout animation (6.0 s in every recording); `PKLogoutActive` holds
/// it for `pk_timer` (20 s by default).
const PK_LOGOFF_S: f64 = 10.0;

/// Social labels whose count follows another player's actions: speech and emotes, tells,
/// channel broadcasts, fellowship updates, confirmation requests, and system text.
const SOCIAL: [&str; 14] = [
    "02BB", "02BC", "F7DE", "01E0", "01E2", "ev 02BD", "ev 02BE", "ev 02BF", "ev 02C0", "ev 00A3",
    "ev 00A4", "ev 0274", "ev 0147", "F7E0",
];

/// An unexplained difference, aggregated over the phases where it occurred.
#[derive(Debug, Clone, Default)]
pub struct Finding {
    /// The action that opened the phases (`act XXXX name` or an opcode).
    pub opener: String,
    /// The message label that differs.
    pub label: String,
    /// Messages ACE sent that we did not (`missing`), or the reverse (`extra`).
    pub missing: usize,
    pub extra: usize,
    pub phases: usize,
    /// The `not_ported!` sites hit in those phases, with hit counts.
    pub not_ported: BTreeMap<&'static str, u64>,
}

/// The per-session result.
#[derive(Debug, Clone, Default)]
pub struct SessionReport {
    pub name: String,
    pub phases: usize,
    pub own_recorded: usize,
    pub own_ours: usize,
    pub matched: usize,
    pub explained: BTreeMap<&'static str, usize>,
    pub unexplained: usize,
    /// The reported categories ([`CATEGORIES`]), counted apart from explained and unexplained.
    pub categorized: BTreeMap<&'static str, usize>,
    pub findings: Vec<Finding>,
    /// Every unexplained difference: (phase group, opener, label, ours minus ACE).
    pub unexplained_at: Vec<(usize, String, String, i64)>,
    pub ambient_recorded: usize,
    pub ambient_ours: usize,
    /// Ambient label counts (recorded, ours), for the labels that differ.
    pub ambient_diff: BTreeMap<String, (usize, usize)>,
    pub statics_missing: usize,
    pub statics_extra: usize,
    pub ace: InvariantReport,
    pub ours: InvariantReport,
    pub notes: Vec<String>,
    /// `not_ported!` sites hit throughout the session (in a quarter of its phases or more).
    pub background_sites: BTreeMap<&'static str, u64>,
    pub crashed: Option<String>,
    /// Recorded monster deaths anchored in our world (`steered`), and those not anchored, by
    /// reason.
    pub steered: usize,
    pub not_steered: BTreeMap<&'static str, usize>,
    /// Object waits cut short in a phase whose roll outcomes differed, that found no match.
    pub capped_waits: usize,
    /// The character's deaths on our side beyond the recording's (`VictimNotificationSelf`
    /// counts): a death ours has and ACE's did not. Reported, never steered.
    pub unrecorded_deaths: usize,
    pub reconstructed: usize,
    pub virtual_s: f64,
    pub wall_s: f64,
}

impl SessionReport {
    #[must_use]
    pub fn explained_total(&self) -> usize {
        self.explained.values().sum()
    }

    /// One reported category's count.
    #[must_use]
    pub fn category(&self, id: &str) -> usize {
        self.categorized.get(id).copied().unwrap_or(0)
    }

    /// One line per session.
    #[must_use]
    pub fn line(&self) -> String {
        let expl: Vec<String> = self
            .explained
            .iter()
            .map(|(k, v)| format!("{k} {v}"))
            .collect();
        format!(
            "{:<32} phases {:>4} | own ACE {:>5} ours {:>5} | matched {:>5} explained {:>4} ({}) unreplayed-client {:>4} rng-divergent {:>4} unexplained {:>4} | steered {:>3} unrecorded-death {} | ambient ACE {:>5} ours {:>5} | statics -{} +{} | inv ACE {} ours {} | recon {} | {}{:.0} vs in {:.1} s",
            self.name,
            self.phases,
            self.own_recorded,
            self.own_ours,
            self.matched,
            self.explained_total(),
            expl.join(", "),
            self.category("unreplayed-client"),
            self.category("rng-divergent"),
            self.unexplained,
            self.steered,
            self.unrecorded_deaths,
            self.ambient_recorded,
            self.ambient_ours,
            self.statics_missing,
            self.statics_extra,
            self.ace.violations.len(),
            self.ours.violations.len(),
            self.reconstructed,
            self.crashed.as_ref().map_or(String::new(), |c| format!("CRASHED ({c}) ")),
            self.virtual_s,
            self.wall_s,
        )
    }
}

/// A label with its protocol name, when it has one.
#[must_use]
pub fn named(label: &str) -> String {
    let code = label
        .rsplit(' ')
        .next()
        .and_then(|h| u32::from_str_radix(h, 16).ok());
    match code.and_then(|c| Opcode(c).name()) {
        Some(n) => format!("{label} {n}"),
        None => label.to_owned(),
    }
}

/// Splits a stream into own-scope label counts per phase group, and ambient counts, tracking the
/// character and its possessions as the stream goes.
struct Scoped {
    /// Per group: the opening client message's label, and own label counts.
    groups: Vec<(String, BTreeMap<String, usize>)>,
    /// Per group: whether a message about another player occurred.
    other_player: Vec<bool>,
    /// Per group: when its opening client message was sent.
    opened_at: Vec<f64>,
    /// Per group and label: own messages naming another player's character (their guid in the
    /// message).
    names_other: Vec<BTreeMap<String, usize>>,
    /// Per group: the fizzles (`WeenieError.YourSpellFizzled`).
    fizzles: Vec<usize>,
    /// Per group: inside a delayed PK logoff (see [`PK_LOGOFF_S`]).
    pk_logoff: Vec<bool>,
    /// When the server's last message arrived.
    last_s2c: f64,
    ambient: BTreeMap<String, usize>,
    statics: BTreeSet<u32>,
    entered_world: bool,
}

fn scope(msgs: &[Msg]) -> Scoped {
    let mut s = Scoped {
        groups: vec![("login".to_owned(), BTreeMap::new())],
        other_player: vec![false],
        opened_at: vec![0.0],
        names_other: vec![BTreeMap::new()],
        fizzles: vec![0],
        pk_logoff: vec![false],
        last_s2c: 0.0,
        ambient: BTreeMap::new(),
        statics: BTreeSet::new(),
        entered_world: false,
    };
    let mut player: Option<u32> = None;
    let mut mine: HashSet<u32> = HashSet::new();
    // other players' characters seen in the world (any message about them)
    let mut others: HashSet<u32> = HashSet::new();
    // a logoff request not answered yet: (its group, when)
    let mut logoff: Option<(usize, f64)> = None;
    for m in msgs {
        let p = &m.payload;
        if m.c2s {
            if m.opcode() == wire::ENTER_WORLD {
                player = u32_at(p, 4);
                mine.clear();
                s.entered_world = true;
            }
            if !wire::is_movement(p) {
                s.groups.push((wire::label(p), BTreeMap::new()));
                s.other_player.push(false);
                s.opened_at.push(m.t);
                s.names_other.push(BTreeMap::new());
                s.fizzles.push(0);
                s.pk_logoff.push(false);
                if m.opcode() == wire::LOGOFF && logoff.is_none() {
                    logoff = Some((s.groups.len() - 1, m.t));
                }
            }
            continue;
        }
        let op = m.opcode();
        s.last_s2c = m.t;
        if op == wire::LOGOFF {
            // LogOffComplete: a delayed PK logoff marks every phase since the request
            if let Some((g, t)) = logoff.take() {
                if m.t - t > PK_LOGOFF_S && !others.is_empty() {
                    for x in &mut s.pk_logoff[g..] {
                        *x = true;
                    }
                }
            }
        }
        // possessions: created in (or wielded by) the character or a possession, or put there
        if op == wire::CREATE_OBJECT {
            if let Some(c) = super::replay::create_owner(p) {
                if Some(c.1) == player || mine.contains(&c.1) {
                    mine.insert(c.0);
                }
            }
        }
        if wire::event_type(p) == Some(0x0022) {
            if let (Some(item), Some(container)) = (u32_at(p, 16), u32_at(p, 20)) {
                if Some(container) == player || mine.contains(&container) {
                    mine.insert(item);
                }
            }
        }
        let subject = wire::subject(p);
        let own = match subject {
            None => true,
            Some(g) => op == wire::GAME_EVENT || Some(g) == player || mine.contains(&g),
        };
        let label = wire::label(p);
        let last = s.groups.len() - 1;
        if let Some(g) = subject.filter(|g| wire::is_player_guid(*g) && Some(*g) != player) {
            others.insert(g);
        }
        if own {
            // after a game event's header (opcode, target, sequence, type), else after the opcode
            let from = if op == wire::GAME_EVENT { 16 } else { 4 };
            if (from..p.len().saturating_sub(3))
                .step_by(4)
                .any(|at| u32_at(p, at).is_some_and(|g| others.contains(&g)))
            {
                *s.names_other[last].entry(label.clone()).or_insert(0) += 1;
            }
            if wire::event_type(p) == Some(0x028A) && u32_at(p, 16) == Some(FIZZLED) {
                s.fizzles[last] += 1;
            }
            *s.groups[last].1.entry(label).or_insert(0) += 1;
        } else {
            if subject.is_some_and(|g| wire::is_player_guid(g) && Some(g) != player) {
                s.other_player[last] = true;
            }
            if op == wire::CREATE_OBJECT {
                if let Some(g) =
                    subject.filter(|g| !wire::is_dynamic_guid(*g) && !wire::is_player_guid(*g))
                {
                    s.statics.insert(g);
                }
            }
            *s.ambient.entry(label).or_insert(0) += 1;
        }
        if op == wire::DELETE_OBJECT || op == 0x0024 {
            if let Some(g) = subject {
                mine.remove(&g);
            }
        }
    }
    s
}

/// Compares one replay with its recording and checks the invariants on both.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn compare(o: &ReplayOutcome) -> SessionReport {
    let rec = scope(&o.recorded.msgs);
    let ours = scope(&o.ours);
    let mut r = SessionReport {
        name: o.name.clone(),
        phases: rec.groups.len(),
        ace: invariants::check(&o.recorded.msgs),
        ours: invariants::check(&o.ours),
        crashed: o.crashed.clone(),
        steered: o.steered.len(),
        capped_waits: o.capped_waits,
        reconstructed: o.reconstructed,
        virtual_s: o.virtual_s,
        wall_s: o.wall_s,
        ..SessionReport::default()
    };
    r.notes = o
        .notes
        .iter()
        .map(|(p, n)| format!("phase {p}: {n}"))
        .collect();
    for (_, _, why) in &o.not_steered {
        *r.not_steered.entry(why).or_insert(0) += 1;
    }
    let deaths = |s: &Scoped| {
        s.groups
            .iter()
            .map(|g| g.1.get(OWN_DEATH).copied().unwrap_or(0))
            .sum::<usize>()
    };
    r.unrecorded_deaths = deaths(&ours).saturating_sub(deaths(&rec));

    // group index of each phase (client message index -> group), for notes and not_ported
    let mut group_of_phase = vec![0usize];
    {
        let mut g = 0;
        for m in o.recorded.msgs.iter().filter(|m| m.c2s) {
            if !wire::is_movement(&m.payload) {
                g += 1;
            }
            group_of_phase.push(g);
        }
    }
    let n = rec.groups.len().max(ours.groups.len());
    let mut not_ported: Vec<BTreeMap<&'static str, u64>> = vec![BTreeMap::new(); n];
    for (phase, sites) in o.not_ported.iter().enumerate() {
        let g = group_of_phase
            .get(phase)
            .copied()
            .unwrap_or(n - 1)
            .min(n - 1);
        for (k, v) in sites {
            *not_ported[g].entry(*k).or_insert(0) += v;
        }
    }
    // background sites: hit after many different kinds of action (ticks, heartbeats, the world's
    // own work) rather than after one; they are reported once per session, not against each
    // finding. A handler's site is hit only after its own action and stays with the finding.
    let openers_of_group: Vec<&str> = (0..n)
        .map(|g| {
            rec.groups
                .get(g)
                .or(ours.groups.get(g))
                .map_or("", |x| x.0.as_str())
        })
        .collect();
    let all_openers: BTreeSet<&str> = openers_of_group.iter().copied().collect();
    let mut site_openers: BTreeMap<&'static str, BTreeSet<&str>> = BTreeMap::new();
    for (g, sites) in not_ported.iter().enumerate() {
        for k in sites.keys() {
            site_openers
                .entry(*k)
                .or_default()
                .insert(openers_of_group[g]);
        }
    }
    let threshold = 4.max(all_openers.len() / 2);
    let background: BTreeSet<&'static str> = site_openers
        .iter()
        .filter(|(_, o)| o.len() >= threshold)
        .map(|(k, _)| *k)
        .collect();
    for g in &not_ported {
        for (k, v) in g {
            if background.contains(k) {
                *r.background_sites.entry(*k).or_insert(0) += v;
            }
        }
    }
    let mut untranslated = vec![false; n];
    for (phase, note) in &o.notes {
        if note.contains("untranslated") {
            let g = group_of_phase
                .get(*phase)
                .copied()
                .unwrap_or(n - 1)
                .min(n - 1);
            untranslated[g] = true;
        }
    }

    // per group: ours - recorded, per label
    let empty = BTreeMap::new();
    let mut diffs: Vec<BTreeMap<String, i64>> = Vec::with_capacity(n);
    for g in 0..n {
        let a = rec.groups.get(g).map_or(&empty, |x| &x.1);
        let b = ours.groups.get(g).map_or(&empty, |x| &x.1);
        let labels: BTreeSet<&String> = a.keys().chain(b.keys()).collect();
        let mut d = BTreeMap::new();
        for l in labels {
            let (x, y) = (
                a.get(l).copied().unwrap_or(0),
                b.get(l).copied().unwrap_or(0),
            );
            r.own_recorded += x;
            r.own_ours += y;
            r.matched += x.min(y);
            let v = i64::try_from(y).unwrap_or(0) - i64::try_from(x).unwrap_or(0);
            if v != 0 {
                d.insert(l.clone(), v);
            }
        }
        diffs.push(d);
    }

    // tick-boundary: cancel opposite differences in adjacent groups
    for g in 0..n.saturating_sub(1) {
        let labels: Vec<String> = diffs[g].keys().cloned().collect();
        for l in labels {
            let a = diffs[g].get(&l).copied().unwrap_or(0);
            let b = diffs[g + 1].get(&l).copied().unwrap_or(0);
            if a.signum() * b.signum() == -1 {
                let k = a.abs().min(b.abs());
                *r.explained.entry("tick-boundary").or_insert(0) +=
                    usize::try_from(2 * k).unwrap_or(0);
                diffs[g].insert(l.clone(), a - a.signum() * k);
                diffs[g + 1].insert(l.clone(), b - b.signum() * k);
            }
        }
    }

    let mut findings: BTreeMap<(String, String), Finding> = BTreeMap::new();
    let empty_counts = BTreeMap::new();
    for g in 0..n {
        let opener = rec
            .groups
            .get(g)
            .or(ours.groups.get(g))
            .map_or_else(String::new, |x| x.0.clone());
        // the reported categories' evidence in this phase (see [`CATEGORIES`])
        let pk = rec.pk_logoff.get(g).copied().unwrap_or(false);
        let named_other = rec.names_other.get(g).unwrap_or(&empty_counts);
        let outcome_differs = ROLL_OUTCOMES
            .iter()
            .any(|l| diffs[g].get(*l).copied().unwrap_or(0) != 0);
        let fizzles_differ =
            rec.fizzles.get(g).copied().unwrap_or(0) != ours.fizzles.get(g).copied().unwrap_or(0);
        let rolls_differ = outcome_differs || fizzles_differ;
        for (l, &v) in &diffs[g] {
            if v == 0 {
                continue;
            }
            let k = usize::try_from(v.unsigned_abs()).unwrap_or(0);
            let silent = rec
                .opened_at
                .get(g)
                .is_some_and(|t| *t > rec.last_s2c + 1.0);
            let rule = if silent {
                Some("recorded-silence")
            } else if matches!(l.as_str(), "F748" | "F74E") {
                Some("position-cadence")
            } else if l == "02E9" {
                Some("vital-tick")
            } else if SOCIAL.contains(&l.as_str())
                && rec.other_player.get(g).copied().unwrap_or(false)
            {
                Some("other-client")
            } else if untranslated[g] {
                Some("untranslated")
            } else if !rec.entered_world {
                Some("recorded-only-login")
            } else {
                None
            };
            if let Some(rule) = rule {
                *r.explained.entry(rule).or_insert(0) += k;
                continue;
            }
            // what no rule explains: the reported categories, from the recording's evidence
            // unreplayed-client: the whole difference inside a delayed PK logoff; otherwise the
            // messages ACE sent (and ours did not) that name another player's character
            let other = if pk {
                k
            } else if v < 0 {
                k.min(named_other.get(l).copied().unwrap_or(0))
            } else {
                0
            };
            if other > 0 {
                *r.categorized.entry("unreplayed-client").or_insert(0) += other;
            }
            let k = k - other;
            if k == 0 {
                continue;
            }
            if rolls_differ && ROLL_CONSEQUENCES.contains(&l.as_str()) {
                *r.categorized.entry("rng-divergent").or_insert(0) += k;
                continue;
            }
            let v = v.signum() * i64::try_from(k).unwrap_or(0);
            r.unexplained += k;
            r.unexplained_at.push((g, opener.clone(), l.clone(), v));
            let f = findings
                .entry((named(&opener), named(l)))
                .or_insert_with(|| Finding {
                    opener: named(&opener),
                    label: named(l),
                    ..Finding::default()
                });
            if v < 0 {
                f.missing += k;
            } else {
                f.extra += k;
            }
            f.phases += 1;
            for (site, hits) in not_ported[g]
                .iter()
                .filter(|(k, _)| !background.contains(*k))
            {
                *f.not_ported.entry(*site).or_insert(0) += hits;
            }
        }
    }
    r.findings = findings.into_values().collect();

    r.ambient_recorded = rec.ambient.values().sum();
    r.ambient_ours = ours.ambient.values().sum();
    let labels: BTreeSet<&String> = rec.ambient.keys().chain(ours.ambient.keys()).collect();
    for l in labels {
        let (x, y) = (
            rec.ambient.get(l).copied().unwrap_or(0),
            ours.ambient.get(l).copied().unwrap_or(0),
        );
        if x != y {
            r.ambient_diff.insert(named(l), (x, y));
        }
    }
    r.statics_missing = rec.statics.difference(&ours.statics).count();
    r.statics_extra = ours.statics.difference(&rec.statics).count();
    r
}
