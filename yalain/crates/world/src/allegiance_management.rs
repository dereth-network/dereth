//! Running an allegiance: its message of the day, its officers, its members' titles, its kingdom
//! restriction, finding a member, and messages to a patron, to vassals or to every follower.
//!
//! **The message of the day** is set or cleared by the monarch or an officer; anyone else in the
//! allegiance is refused, and one in none is told so. The setter is told the new message, and
//! every member in the world is sent its profile again, which carries it. Asked to show it, a
//! member is shown the message, or told there is none.
//!
//! **Officers** are appointed and revoked by the monarch alone, by name, from among the
//! allegiance's members: at most [`MOST_OFFICERS`] besides the monarch. The monarch is told whom it
//! appointed or revoked and how many of how many it has; every member in the world is sent its
//! profile again, which flags the officers.
//!
//! **A member's title** is set or removed by an officer of its allegiance (the monarch among
//! them); others' examinations of the member show it after its rank.
//!
//! **A kingdom restriction** is set or lifted by the monarch alone: new members must be of the
//! monarch's kingdom, or of it or neutral ([`crate::allegiance`] refuses the oath).
//!
//! **Finding a member** by name sends the asker that member's profile, for the allegiance page to
//! show, and says it was found; a name outside the allegiance is told so.
//!
//! **Messages**: to the patron (in the world, or the sender is told it is away), to the vassals in
//! the world, or to every follower below the sender in the world. The sender sees what it said;
//! a gagged player is refused as when it talks, and one ignoring the sender never hears it.

use auberean_script::native_object::{TextInfo, Variable};
use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::allegiance::{self, facts, keep, Restriction};
use crate::calls::{Node, Value};
use crate::profile::number;
use crate::social::{name_var, show, world_of, NOTICE, REFUSED};
use crate::{progress, Server};

/// The client's calls this answers.
pub mod function {
    /// The message of the day: set (a string reference), cleared, shown.
    pub const SET_MOTD: u32 = 0x8203_0059;
    pub const CLEAR_MOTD: u32 = 0x8203_0040;
    pub const SHOW_MOTD: u32 = 0x8203_0062;
    /// An officer appointed, revoked (a name).
    pub const APPOINT: u32 = 0x8203_0048;
    pub const REVOKE: u32 = 0x8203_003D;
    /// A member's title set (its title, then its name) or removed (a name).
    pub const SET_TITLE: u32 = 0x8203_005B;
    pub const REMOVE_TITLE: u32 = 0x8203_0054;
    /// The kingdom restriction: whether neutrals are let in, then whether it is on.
    pub const RESTRICT: u32 = 0x8203_004C;
    /// A member looked for by name.
    pub const SEARCH: u32 = 0x8203_004D;
    /// Words to the patron, the vassals, every follower.
    pub const TO_PATRON: u32 = 0x8203_0025;
    pub const TO_VASSALS: u32 = 0x8203_0028;
    pub const TO_FOLLOWERS: u32 = 0x8203_0018;
}

/// Is this one of the calls this answers?
#[must_use]
pub const fn handles(f: u32) -> bool {
    use function::{
        APPOINT, CLEAR_MOTD, REMOVE_TITLE, RESTRICT, REVOKE, SEARCH, SET_MOTD, SET_TITLE,
        SHOW_MOTD, TO_FOLLOWERS, TO_PATRON, TO_VASSALS,
    };
    matches!(
        f,
        SET_MOTD
            | CLEAR_MOTD
            | SHOW_MOTD
            | APPOINT
            | REVOKE
            | SET_TITLE
            | REMOVE_TITLE
            | RESTRICT
            | SEARCH
            | TO_PATRON
            | TO_VASSALS
            | TO_FOLLOWERS
    )
}

/// Does this call carry words a player wrote (held to the length of a line of talk)?
#[must_use]
pub const fn carries_words(f: u32) -> bool {
    use function::{SET_MOTD, SET_TITLE, TO_FOLLOWERS, TO_PATRON, TO_VASSALS};
    matches!(
        f,
        SET_MOTD | SET_TITLE | TO_PATRON | TO_VASSALS | TO_FOLLOWERS
    )
}

/// The most officers a monarch may appoint.
pub const MOST_OFFICERS: u32 = 4;

/// The allegiance's kind of text in the chat window.
pub const ALLEGIANCE: u32 = 0x2_0000;

/// Texts: (string id, table), and their variables.
pub mod text {
    pub const TABLE: u32 = 0x2500_04AB;
    pub const NOT_IN_ALLEGIANCE: (u32, u32) = (0x257A_6C5D, TABLE);
    pub const MONARCH_ONLY: (u32, u32) = (0x6977_DECD, TABLE);
    pub const MONARCH_OR_OFFICER: (u32, u32) = (0x396A_6C1E, TABLE);
    pub const NOT_A_MEMBER: (u32, u32) = (0x61B3_6279, TABLE);
    pub const MOTD_SET: (u32, u32) = (0x1076_92C8, TABLE);
    pub const MOTD_CLEARED: (u32, u32) = (0x1BD5_1A82, TABLE);
    pub const MOTD: (u32, u32) = (0x6E30_FCDD, TABLE);
    pub const NO_MOTD: (u32, u32) = (0x6065_0865, TABLE);
    pub const MOTD_EMPTY: (u32, u32) = (0xF7FD_398A, TABLE);
    pub const SET_NOT_IN_ALLEGIANCE: (u32, u32) = (0xD0C9_742B, TABLE);
    pub const CLEAR_NOT_IN_ALLEGIANCE: (u32, u32) = (0xD037_F0BC, TABLE);
    pub const APPOINTED: (u32, u32) = (0x1DE8_0CBE, TABLE);
    pub const REVOKED: (u32, u32) = (0x05BA_57C5, TABLE);
    pub const OFFICERS_FULL: (u32, u32) = (0xF182_DFA1, TABLE);
    pub const CANNOT_APPOINT: (u32, u32) = (0x0334_A8B6, TABLE);
    pub const CANNOT_REVOKE: (u32, u32) = (0x4F25_F1CC, TABLE);
    pub const TITLE_SET: (u32, u32) = (0xB659_A831, TABLE);
    pub const TITLE_REMOVED: (u32, u32) = (0x7F7B_40A3, TABLE);
    pub const TITLE_UNCHANGED: (u32, u32) = (0x2F9B_B10E, TABLE);
    pub const RESTRICTED: (u32, u32) = (0x8B73_163A, TABLE);
    pub const RESTRICTED_OR_NEUTRAL: (u32, u32) = (0xF0EC_E3F7, TABLE);
    pub const UNRESTRICTED: (u32, u32) = (0x32D8_60CF, TABLE);
    pub const FOUND: (u32, u32) = (0x896F_7E98, TABLE);
    pub const NO_PATRON: (u32, u32) = (0x7BB2_7CDA, TABLE);
    pub const PATRON_AWAY: (u32, u32) = (0xD230_6A3F, TABLE);
    pub const NO_VASSALS: (u32, u32) = (0xE2C1_0CF9, TABLE);
    pub const TOLD_PATRON: (u32, u32) = (0x02CC_FC96, TABLE);
    pub const PATRON_HEARS: (u32, u32) = (0x99E6_8169, TABLE);
    pub const TOLD_VASSALS: (u32, u32) = (0x58E0_51AD, TABLE);
    pub const VASSALS_HEAR: (u32, u32) = (0xF520_6FA9, TABLE);
    pub const TOLD_FOLLOWERS: (u32, u32) = (0x3D23_4621, TABLE);
    pub const FOLLOWERS_HEAR: (u32, u32) = (0x555A_8315, TABLE);
    /// Variables: the message of the day; a name; how many officers, and how many allowed; a
    /// member and its title; a kingdom and the neutral one; a member found; words said.
    pub const MOTD_VAR: u32 = 0xAAF4_1D38;
    pub const NAME: u32 = 0x9D06_6C50;
    pub const COUNT: u32 = 0x9D56_6800;
    pub const ALLOWED: u32 = 0x7250_64FA;
    pub const MEMBER: u32 = 0x9E06_9D38;
    pub const TITLE: u32 = 0x3924_61A0;
    pub const KINGDOM: u32 = 0xBEE1_1D49;
    pub const NEUTRAL: u32 = 0x2C6D_BBF4;
    pub const FOUND_VAR: u32 = 0x2F96_6C28;
    pub const WORDS: u32 = 0x9B4E_3800;
}

/// A call this answers, from a player's client on its world connection. A call that looks among
/// the allegiance's members, or sends them their profiles, reads a store's row for each, so it
/// comes out of the connection's allowance for pages by the allegiance's size, as the tree's export
/// does; past it, the call goes unanswered.
pub fn call<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    f: u32,
    v: &[Value],
    now: yalain_net::LocalTime,
) {
    if !allegiance::ensure(s) {
        return;
    }
    let Some(me) = s.players.get(&world).map(|p| p.character) else {
        return;
    };
    let reads_members = !matches!(
        f,
        function::SHOW_MOTD
            | function::RESTRICT
            | function::TO_PATRON
            | function::TO_VASSALS
            | function::TO_FOLLOWERS
    );
    if reads_members {
        let members = s
            .allegiances
            .monarch(me)
            .map_or(0, |m| s.allegiances.tree(m).len());
        if !crate::market::rows_allowed(s, world, members + 1, now) {
            return;
        }
    }
    match f {
        function::SET_MOTD => set_motd(s, net, world, me, v),
        function::CLEAR_MOTD => clear_motd(s, net, world, me),
        function::SHOW_MOTD => show_motd(s, net, world, me),
        function::APPOINT | function::REVOKE => {
            officer(s, net, world, me, v, f == function::APPOINT);
        }
        function::SET_TITLE | function::REMOVE_TITLE => {
            title(s, net, world, me, v, f == function::SET_TITLE);
        }
        function::RESTRICT => restrict(s, net, world, me, v),
        function::SEARCH => search(s, net, world, me, v),
        _ => message(s, net, world, me, f, v),
    }
}

fn say(net: &mut ServerNet, world: ConnId, kind: u32, id: (u32, u32), vars: Vec<Variable>) {
    show(net, world, kind, progress::text_with(id, vars));
}

fn refuse(net: &mut ServerNet, world: ConnId, id: (u32, u32)) {
    say(net, world, REFUSED, id, Vec::new());
}

/// A literal string reference of words a player typed, its meta marks escaped.
fn typed(words: &str) -> TextInfo {
    let units: Vec<u16> = words.encode_utf16().collect();
    TextInfo {
        has_literal: 1,
        literal: Some(auberean_text::meta::escape(&units)),
        ..TextInfo::default()
    }
}

/// The strings a call carries, in the order they travel.
fn strings_of(v: &[Value]) -> Vec<String> {
    v.iter()
        .filter_map(|x| match x.root() {
            Some(Node::Text(t)) => Some(t.clone()),
            _ => None,
        })
        .collect()
}

/// The words of the call (its only string).
fn words_of(v: &[Value]) -> String {
    strings_of(v).into_iter().next().unwrap_or_default()
}

/// The member of the asker's allegiance with this name (as typed; letter case aside).
fn member_named<S: Store>(s: &mut Server<S>, monarch: u64, typed: &str) -> Option<u64> {
    let want = crate::chat::bare_name(typed);
    if want.is_empty() {
        return None;
    }
    let members = s.allegiances.tree(monarch);
    members
        .into_iter()
        .find(|m| facts(s, *m).plain.to_lowercase() == want)
}

/// Send every member of a monarch's allegiance in the world its profile again.
fn push_profiles<S: Store>(s: &mut Server<S>, net: &mut ServerNet, monarch: u64) {
    for id in s.allegiances.tree(monarch) {
        if let Some(w) = world_of(s, id) {
            let m = allegiance::profile(s, id);
            progress::private(net, w, &m);
        }
    }
}

fn set_motd<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, me: u64, v: &[Value]) {
    let Some(monarch) = s.allegiances.monarch(me) else {
        return refuse(net, world, text::SET_NOT_IN_ALLEGIANCE);
    };
    if !s.allegiances.is_officer(me) {
        return refuse(net, world, text::MONARCH_OR_OFFICER);
    }
    let motd = v.iter().find_map(|x| match x.root() {
        Some(Node::TextInfo(t)) => t.literal.clone(),
        _ => None,
    });
    let Some(motd) = motd.filter(|m| !m.is_empty()) else {
        return refuse(net, world, text::MOTD_EMPTY);
    };
    s.allegiances.charter_mut(monarch).motd = Some(motd.clone());
    keep(s);
    let shown = TextInfo {
        has_literal: 1,
        literal: Some(motd),
        ..TextInfo::default()
    };
    say(
        net,
        world,
        NOTICE,
        text::MOTD_SET,
        vec![progress::nested(text::MOTD_VAR, shown)],
    );
    push_profiles(s, net, monarch);
}

fn clear_motd<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, me: u64) {
    let Some(monarch) = s.allegiances.monarch(me) else {
        return refuse(net, world, text::CLEAR_NOT_IN_ALLEGIANCE);
    };
    if !s.allegiances.is_officer(me) {
        return refuse(net, world, text::MONARCH_OR_OFFICER);
    }
    s.allegiances.charter_mut(monarch).motd = None;
    s.allegiances.tidy_charter(monarch);
    keep(s);
    say(net, world, NOTICE, text::MOTD_CLEARED, Vec::new());
    push_profiles(s, net, monarch);
}

fn show_motd<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, me: u64) {
    let Some(monarch) = s.allegiances.monarch(me) else {
        return refuse(net, world, text::NOT_IN_ALLEGIANCE);
    };
    match s.allegiances.motd(monarch) {
        Some(m) => {
            let shown = TextInfo {
                has_literal: 1,
                literal: Some(m.to_vec()),
                ..TextInfo::default()
            };
            say(
                net,
                world,
                ALLEGIANCE,
                text::MOTD,
                vec![progress::nested(text::MOTD_VAR, shown)],
            );
        }
        None => say(net, world, ALLEGIANCE, text::NO_MOTD, Vec::new()),
    }
}

/// The monarch, in the allegiance, asking: its allegiance's monarch; else it is told why not.
fn monarch_asking<S: Store>(s: &Server<S>, net: &mut ServerNet, world: ConnId, me: u64) -> bool {
    match s.allegiances.monarch(me) {
        None => {
            refuse(net, world, text::NOT_IN_ALLEGIANCE);
            false
        }
        Some(m) if m != me => {
            refuse(net, world, text::MONARCH_ONLY);
            false
        }
        Some(_) => true,
    }
}

fn officer<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    me: u64,
    v: &[Value],
    appoint: bool,
) {
    if !monarch_asking(s, net, world, me) {
        return;
    }
    let name = words_of(v);
    let Some(target) = member_named(s, me, &name) else {
        let var = progress::nested(text::NAME, typed(&name));
        return say(net, world, REFUSED, text::NOT_A_MEMBER, vec![var]);
    };
    let tname = facts(s, target).name;
    let held = s
        .allegiances
        .charters
        .get(&me)
        .is_some_and(|c| c.officers.contains(&target));
    let count = |s: &Server<S>| u32::try_from(s.allegiances.officers(me, false).len()).unwrap_or(0);
    if appoint {
        if target == me || held {
            let var = name_var(text::NAME, &tname);
            return say(net, world, REFUSED, text::CANNOT_APPOINT, vec![var]);
        }
        if count(s) >= MOST_OFFICERS {
            let var = number(text::COUNT, MOST_OFFICERS);
            return say(net, world, REFUSED, text::OFFICERS_FULL, vec![var]);
        }
        s.allegiances.charter_mut(me).officers.insert(target);
    } else {
        if !held {
            let var = name_var(text::NAME, &tname);
            return say(net, world, REFUSED, text::CANNOT_REVOKE, vec![var]);
        }
        s.allegiances.charter_mut(me).officers.remove(&target);
        s.allegiances.tidy_charter(me);
    }
    keep(s);
    let done = if appoint {
        text::APPOINTED
    } else {
        text::REVOKED
    };
    let vars = vec![
        name_var(text::NAME, &tname),
        number(text::COUNT, count(s)),
        number(text::ALLOWED, MOST_OFFICERS),
    ];
    say(net, world, NOTICE, done, vars);
    push_profiles(s, net, me);
}

fn title<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    me: u64,
    v: &[Value],
    set: bool,
) {
    let Some(monarch) = s.allegiances.monarch(me) else {
        return refuse(net, world, text::NOT_IN_ALLEGIANCE);
    };
    if !s.allegiances.is_officer(me) {
        return refuse(net, world, text::MONARCH_OR_OFFICER);
    }
    // The title travels first, then the member's name.
    let strings = strings_of(v);
    let (name, title) = if set {
        (
            strings.get(1).cloned().unwrap_or_default(),
            strings.first().map(|t| t.trim().to_string()),
        )
    } else {
        (strings.first().cloned().unwrap_or_default(), None)
    };
    let Some(target) = member_named(s, monarch, &name) else {
        let var = progress::nested(text::NAME, typed(&name));
        return say(net, world, REFUSED, text::NOT_A_MEMBER, vec![var]);
    };
    let tname = facts(s, target).name;
    match title {
        Some(t) if !t.is_empty() => {
            s.allegiances.titles.insert(target, t.clone());
            keep(s);
            let vars = vec![
                name_var(text::MEMBER, &tname),
                progress::nested(text::TITLE, typed(&t)),
            ];
            say(net, world, NOTICE, text::TITLE_SET, vars);
        }
        _ if set || s.allegiances.titles.remove(&target).is_none() => {
            let var = name_var(text::MEMBER, &tname);
            say(net, world, REFUSED, text::TITLE_UNCHANGED, vec![var]);
        }
        _ => {
            keep(s);
            let var = name_var(text::MEMBER, &tname);
            say(net, world, NOTICE, text::TITLE_REMOVED, vec![var]);
        }
    }
}

fn restrict<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, me: u64, v: &[Value]) {
    if !monarch_asking(s, net, world, me) {
        return;
    }
    // Whether neutrals are let in travels first, then whether the restriction is on.
    let flags: Vec<u32> = v
        .iter()
        .filter_map(|x| match x {
            Value::Int(i) => Some(*i),
            _ => None,
        })
        .collect();
    let (neutrals, on) = (
        flags.first().copied().unwrap_or(0) != 0,
        flags.get(1).copied().unwrap_or(0) != 0,
    );
    s.allegiances.charter_mut(me).restriction = on.then_some(Restriction { neutrals });
    s.allegiances.tidy_charter(me);
    keep(s);
    if !on {
        return say(net, world, NOTICE, text::UNRESTRICTED, Vec::new());
    }
    let ours = facts(s, me)
        .kingdom
        .unwrap_or(crate::kingdoms::kingdom::NEUTRAL);
    let kingdom = |id: u32, f: u32| {
        progress::nested(
            id,
            crate::profile::string(crate::profile::line::KINGDOMS, crate::profile::kingdom(f)),
        )
    };
    if neutrals {
        let vars = vec![
            kingdom(text::KINGDOM, ours),
            kingdom(text::NEUTRAL, crate::kingdoms::kingdom::NEUTRAL),
        ];
        say(net, world, NOTICE, text::RESTRICTED_OR_NEUTRAL, vars);
    } else {
        let vars = vec![kingdom(text::KINGDOM, ours)];
        say(net, world, NOTICE, text::RESTRICTED, vars);
    }
}

fn search<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, me: u64, v: &[Value]) {
    let Some(monarch) = s.allegiances.monarch(me) else {
        return refuse(net, world, text::NOT_IN_ALLEGIANCE);
    };
    let name = words_of(v);
    let Some(found) = member_named(s, monarch, &name) else {
        let var = progress::nested(text::NAME, typed(&name));
        return say(net, world, REFUSED, text::NOT_A_MEMBER, vec![var]);
    };
    let fname = facts(s, found).name;
    say(
        net,
        world,
        NOTICE,
        text::FOUND,
        vec![name_var(text::FOUND_VAR, &fname)],
    );
    let m = allegiance::profile(s, found);
    progress::private(net, world, &m);
}

/// Words to the patron, the vassals or every follower.
fn message<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    me: u64,
    f: u32,
    v: &[Value],
) {
    let words = words_of(v);
    if words.is_empty() {
        return;
    }
    if crate::chat::refuses_talk(s, net, world) {
        return;
    }
    if !s.allegiances.member(me) {
        return refuse(net, world, text::NOT_IN_ALLEGIANCE);
    }
    let (to, told, heard) = match f {
        function::TO_PATRON => {
            let patron = s.allegiances.nodes.get(&me).map_or(0, |n| n.patron);
            if patron == 0 {
                return refuse(net, world, text::NO_PATRON);
            }
            if world_of(s, patron).is_none() {
                return refuse(net, world, text::PATRON_AWAY);
            }
            (vec![patron], text::TOLD_PATRON, text::PATRON_HEARS)
        }
        function::TO_VASSALS => (
            s.allegiances.vassals(me),
            text::TOLD_VASSALS,
            text::VASSALS_HEAR,
        ),
        _ => (
            s.allegiances.tree(me).into_iter().skip(1).collect(),
            text::TOLD_FOLLOWERS,
            text::FOLLOWERS_HEAR,
        ),
    };
    if to.is_empty() {
        return refuse(net, world, text::NO_VASSALS);
    }
    let (my_name, plain) = {
        let f = facts(s, me);
        (f.name, f.plain.to_lowercase())
    };
    let said = typed(&words);
    for id in to {
        let Some(w) = world_of(s, id) else {
            continue;
        };
        let ignores = s
            .ignoring
            .get(&id)
            .is_some_and(|names| names.contains(&plain));
        if ignores {
            continue;
        }
        let vars = vec![
            name_var(text::NAME, &my_name),
            progress::nested(text::WORDS, said.clone()),
        ];
        say(net, w, ALLEGIANCE, heard, vars);
    }
    say(
        net,
        world,
        ALLEGIANCE,
        told,
        vec![progress::nested(text::WORDS, said)],
    );
}
