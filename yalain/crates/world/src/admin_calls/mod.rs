//! What an admin character sends: the client's admin commands arrive as plain script calls, and each
//! is handled here, restricted to admins.
//!
//! **The gate.** Every call of the admin family is checked before anything is read of it, on the
//! server's own record: the account's flag at the time of the call, and that the character entered
//! as an admin character ([`allowed`]). Nothing the client says counts. Anyone else's call does
//! nothing and is logged and counted ([`AdminCalls::refused`]).
//!
//! **What the game kept and what this is.** No recording holds an admin's session and the original
//! server's handlers are not held, so each call's behavior is this server's own, from what the call
//! carries and what the client's help says of it. The client shows every answer: the one answer the
//! original had is a line for the admin's log and chat window ([`answer`]).
//!
//! The calls are by kind: the account's and the players' ([`moderation`]), moving ([`movement`]),
//! objects and items ([`objects`]), a character's state ([`character`]), effects and looks
//! ([`effects`]), factions and creatures ([`factions`]), quests and the story ([`quests`]), chat rooms
//! of an admin's own ([`rooms`]). A call
//! with no counterpart in this server is answered that it has none ([`none`]).

mod args;
mod character;
mod effects;
mod factions;
mod moderation;
mod movement;
mod none;
mod objects;
mod quests;
pub(crate) mod rooms;

use std::collections::BTreeMap;

use auberean_protocol::ServerMessage;
use auberean_script::args::Arg;
use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::admin::Issuer;
use crate::calls::Value;
use crate::{chat, progress, session, Clock, Server};

pub use args::Args;
pub use factions::{ethereal_value, Attackable, Ethereality};
pub use rooms::Rooms;

/// The call family only admin characters send.
const ADMIN_FAMILY: u32 = 0x8201;

/// The client function an admin's answer goes to.
pub const LOG_EVENT: u32 = 0x8201_00EE;

/// The answer's log type: every line goes to the one channel.
pub const LOG_TYPE: u32 = 1;

/// The calls of other families that only an admin's commands send.
pub mod other {
    /// Creatures' debugging and how they treat the admin.
    pub const AI_DUMP: u32 = 0x8200_0006;
    pub const AI_CLIQUE: u32 = 0x8200_0008;
    pub const AI_TRIGGER: u32 = 0x8200_000E;
    pub const AI_ATTACKABLE: u32 = 0x8200_0017;
    /// Factions and player killing.
    pub const FACTION_SHOW: u32 = 0x8210_0002;
    pub const PK_STATUS: u32 = 0x8210_0004;
    pub const FACTION_TYPE: u32 = 0x8210_0006;
    pub const PK_OVERRIDE: u32 = 0x8210_000C;
    pub const FACTION_OWNER: u32 = 0x8210_0018;
    pub const ETHEREALITY: u32 = 0x8210_001A;
    /// Death's values, of the admin or of another; killing.
    pub const DEATH_XP_SET: u32 = 0x8207_0005;
    pub const CHALLENGE_SET: u32 = 0x8207_0008;
    pub const SMITE: u32 = 0x8207_000B;
    pub const VITAE_LOWER: u32 = 0x8207_000C;
    pub const CHALLENGE_OF: u32 = 0x8207_000F;
    pub const DEATH_XP_OF: u32 = 0x8207_0014;
    pub const DEATH_XP_SET_OF: u32 = 0x8207_0018;
    pub const VITAE_RAISE: u32 = 0x8207_001D;
    pub const VITAE_SET: u32 = 0x8207_0024;
    pub const CHALLENGE_SET_OF: u32 = 0x8207_0025;
    /// Money made from nothing.
    pub const MONEY: u32 = 0x8217_001A;
    /// The admin's own player store.
    pub const STORE_EXPIRE: u32 = 0x9B5E_001B;
    pub const STORE_OPEN: u32 = 0x9B5E_0020;
    pub const STORE_CLOSE: u32 = 0x9B5E_003D;

    /// Every one of them.
    pub const ALL: [u32; 24] = [
        AI_DUMP,
        AI_CLIQUE,
        AI_TRIGGER,
        AI_ATTACKABLE,
        FACTION_SHOW,
        PK_STATUS,
        FACTION_TYPE,
        PK_OVERRIDE,
        FACTION_OWNER,
        ETHEREALITY,
        DEATH_XP_SET,
        CHALLENGE_SET,
        SMITE,
        VITAE_LOWER,
        CHALLENGE_OF,
        DEATH_XP_OF,
        DEATH_XP_SET_OF,
        VITAE_RAISE,
        VITAE_SET,
        CHALLENGE_SET_OF,
        MONEY,
        STORE_EXPIRE,
        STORE_OPEN,
        STORE_CLOSE,
    ];
}

/// Is this call one only an admin may make?
#[must_use]
pub fn is_admin_call(function: u32) -> bool {
    function >> 16 == ADMIN_FAMILY || other::ALL.contains(&function)
}

/// How many times each admin call was refused.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AdminCalls {
    pub refused: BTreeMap<u32, u64>,
    /// How many refusals were logged: one a character in each stay in the world.
    pub logged: u64,
    /// How many command lines typed by anyone but an admin were refused.
    pub refused_lines: u64,
    /// How many of those were logged: one a character in each stay in the world.
    pub logged_lines: u64,
}

/// What an admin's commands have set on it. A cloak is kept when it leaves the world: one that
/// silently ended would show an admin who thinks it is hidden. Nothing else is: what keeps it alive
/// or lets it pass and outrun everyone is better lost than forgotten about, and its loss shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AdminState {
    /// Its health never falls below one.
    pub never_say_die: bool,
    /// How creatures treat it.
    pub attackable: factions::Attackable,
    /// Hidden from players.
    pub cloaked: bool,
    /// It sees what is hidden.
    pub sees_hidden: bool,
    /// Its movement scaled, 1 for none.
    pub run: f32,
    /// Its ethereality override.
    pub ethereality: factions::Ethereality,
}

impl Default for AdminState {
    fn default() -> Self {
        Self {
            never_say_die: false,
            attackable: factions::Attackable::Normal,
            cloaked: false,
            sees_hidden: false,
            run: 1.0,
            ethereality: factions::Ethereality::Normal,
        }
    }
}

/// May the sender of a call on this world connection make admin calls: its account's flag now, and
/// its character an admin character? Whose call it is, then.
#[must_use]
pub fn allowed<S: Store>(s: &Server<S>, world: ConnId) -> Option<Issuer> {
    let p = s.players.get(&world)?;
    let account = &s.sessions.get(&p.login)?.account;
    (account.admin && p.admin_character).then(|| Issuer::Account {
        name: account.name.clone(),
        admin: true,
        groups: account.groups.clone(),
        character: p.name.clone(),
    })
}

/// A character in the world is no longer an admin's: what its commands set on it ends at once. It is
/// seen again, can die, creatures notice it, it passes through what it should and runs at its own
/// speed, it no longer sees what is hidden, and it is in no admin's room. A suffix on its name, which
/// may say it is staff, goes too.
pub(crate) fn revoke<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let was = std::mem::take(&mut p.admin);
    let me = p.character;
    let suffixed = !std::mem::take(&mut p.play.suffix).is_empty();
    if was.run.to_bits() != AdminState::default().run.to_bits() {
        let m = ServerMessage::VelocityScale(crate::places::player_event(
            p,
            auberean_protocol::physics::Scale(AdminState::default().run),
        ));
        crate::places::to_all_knowing(s, net, me, &m);
    }
    if suffixed {
        crate::player::show_name(s, net, world);
    }
    rooms::leave_all(s, me);
    crate::presence::cloak_changed(s, net, world);
    crate::presence::stops_seeing_hidden(s, net, world);
    crate::kingdoms::settle_ethereality(s, net, world);
    crate::world::keep_player(s, world);
}

/// A character not in the world is no longer an admin's: a cloak it left with is forgotten, so that
/// it does not come back should it be made an admin's again, and a suffix on its name goes.
pub(crate) fn forget_kept<S: Store>(s: &mut Server<S>, character: u64) {
    if s.players.values().any(|p| p.character == character) {
        return;
    }
    let Ok(Some(mut st)) = crate::saves::state_of(s, character) else {
        return;
    };
    let Ok(mut play) = crate::state::PlayState::from_bytes(&st.play) else {
        return;
    };
    let cloaked = std::mem::take(&mut play.cloaked);
    let suffixed = !std::mem::take(&mut play.suffix).is_empty();
    if !cloaked && !suffixed {
        return;
    }
    if suffixed {
        if let Ok(Some(c)) = s.store.character(character) {
            crate::player::set_name(&mut st.qualities, &c.name, c.sex);
        }
    }
    st.play = play.to_bytes();
    if let Err(e) = crate::saves::put(s, character, st) {
        tracing::warn!(
            id = format_args!("{character:#x}"),
            "what its commands set was not forgotten: {e}"
        );
    }
}

/// An admin character entering the world still cloaked is told so.
pub(crate) fn entered<S: Store>(s: &Server<S>, net: &mut ServerNet, world: ConnId) {
    if s.players.get(&world).is_some_and(|p| p.admin.cloaked) {
        crate::login::notice(net, world, STILL_CLOAKED);
    }
}

/// What an admin entering the world still cloaked is told.
pub const STILL_CLOAKED: &str =
    "You are still cloaked: players do not see you. /cloak off shows you.";

/// Refuse a call: counted, and nothing is answered. The first from a character in each stay in the
/// world is logged, and those after it only counted: a client can send them as fast as it can send.
fn refuse<S: Store>(s: &mut Server<S>, world: ConnId, function: u32) {
    *s.admin_calls.refused.entry(function).or_default() += 1;
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    if std::mem::replace(&mut p.refused_admin_call, true) {
        return;
    }
    s.admin_calls.logged += 1;
    let p = &s.players[&world];
    let account = s
        .sessions
        .get(&p.login)
        .map_or("", |x| x.account.name.as_str());
    let character = p.name.as_str();
    tracing::warn!(
        account,
        character,
        function = format_args!("{function:#010x}"),
        "refused: an admin call from anyone but an admin character of an admin's account (any more \
         from it this stay are only counted)"
    );
}

/// The answer to an admin's call: a line for its log and chat window.
#[must_use]
pub fn answer_message(text: &str) -> ServerMessage {
    session::call(LOG_EVENT, &[chat::string_arg(text), Arg::Int(LOG_TYPE)]).expect("packs")
}

/// Answer an admin's call.
pub fn answer(net: &mut ServerNet, world: ConnId, text: &str) {
    progress::private(net, world, &answer_message(text));
}

/// An admin call: the gate, then what it does. Every call let through is logged with its issuer.
pub fn call<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    function: u32,
    bytes: &[u8],
    clock: Clock,
) {
    let Some(by) = allowed(s, world) else {
        return refuse(s, world, function);
    };
    let values = match crate::calls::values(bytes) {
        Ok(v) => v,
        Err(e) => {
            tracing::info!(issuer = %by, function = format_args!("{function:#010x}"), "an admin call that does not read: {e}");
            return answer(net, world, "That command's values do not read.");
        }
    };
    tracing::info!(issuer = %by, function = format_args!("{function:#010x}"), "admin call");
    if let Some(word) = crate::admin_groups::word_of(function) {
        if let Some(refused) = crate::admin_groups::refusal(s, &by, word) {
            *s.admin_calls.refused.entry(function).or_default() += 1;
            tracing::warn!(issuer = %by, "refused: {refused}");
            return answer(net, world, &refused);
        }
    }
    let lines = dispatch(s, net, world, &by, function, &Args::of(&values), clock);
    for line in &lines {
        tracing::info!(issuer = %by, "admin: {line}");
    }
    // One message: separate ones of a long answer may overtake each other on the wire.
    if !lines.is_empty() {
        answer(net, world, &lines.join("\n"));
    }
}

/// A call's handler: the lines that answer it.
fn dispatch<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    by: &Issuer,
    f: u32,
    a: &Args<'_>,
    clock: Clock,
) -> Vec<String> {
    let cx = &mut Cx {
        s,
        net,
        world,
        by,
        clock,
    };
    moderation::call(cx, f, a)
        .or_else(|| movement::call(cx, f, a))
        .or_else(|| objects::call(cx, f, a))
        .or_else(|| character::call(cx, f, a))
        .or_else(|| effects::call(cx, f, a))
        .or_else(|| factions::call(cx, f, a))
        .or_else(|| quests::call(cx, f, a))
        .or_else(|| rooms::call(cx, f, a))
        .unwrap_or_else(|| none::call(cx, f))
}

/// What a handler works with.
pub(crate) struct Cx<'a, S: Store> {
    pub s: &'a mut Server<S>,
    pub net: &'a mut ServerNet,
    pub world: ConnId,
    pub by: &'a Issuer,
    pub clock: Clock,
}

/// The one a call names: a player in the world, or a thing of the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Target {
    Player(ConnId),
    Thing(u64),
}

impl<S: Store> Cx<'_, S> {
    /// The admin's own state.
    pub fn state(&mut self) -> &mut AdminState {
        &mut self
            .s
            .players
            .get_mut(&self.world)
            .expect("an allowed call's player is in the world")
            .admin
    }

    /// The admin's character.
    pub fn me(&self) -> u64 {
        self.s.players.get(&self.world).map_or(0, |p| p.character)
    }

    /// The one an id names; 0 is the admin's selection, or the admin with none.
    pub fn target(&self, id: u64) -> Option<Target> {
        let id = if id == 0 {
            self.s.players.get(&self.world).map_or(0, |p| {
                if p.selection != 0 {
                    p.selection
                } else {
                    p.character
                }
            })
        } else {
            id
        };
        if let Some((w, _)) = self.s.players.iter().find(|(_, p)| p.character == id) {
            return Some(Target::Player(*w));
        }
        self.s.things.get(id).map(|_| Target::Thing(id))
    }

    /// A player in the world an id names; 0 is the admin's selection when that is a player, else the
    /// admin. The answer for one that is none.
    pub fn player(&self, id: u64) -> Result<ConnId, String> {
        match self.target(id) {
            Some(Target::Player(w)) => Ok(w),
            _ => Err("That is not a player in the world.".to_owned()),
        }
    }

    /// A player in the world by name.
    pub fn player_named(&self, name: &str) -> Option<ConnId> {
        crate::presence::player_named(self.s, &crate::chat::bare_name(name), |_, _| true)
    }

    /// A character's name by world connection.
    pub fn name_of(&self, w: ConnId) -> String {
        self.s
            .players
            .get(&w)
            .map_or_else(String::new, |p| p.name.clone())
    }
}

/// Every call of the admin family a client can send, by the low word of its function id: for the
/// gate's tests and the operator's dumps.
#[must_use]
pub fn admin_family() -> Vec<u32> {
    (0..=0x143u32).map(|low| ADMIN_FAMILY << 16 | low).collect()
}

/// A float value's bits as the float.
#[must_use]
pub(crate) fn float_of(v: &Value) -> Option<f32> {
    match v {
        Value::Float(b) => Some(f32::from_bits(*b)),
        _ => None,
    }
}
