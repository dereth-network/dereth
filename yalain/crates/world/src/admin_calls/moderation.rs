//! An admin's commands over accounts and what players are told: boots, bans, who a character is,
//! the world's limit, and the broadcasts.

use yalain_net::ConnId;
use yalain_store::Store;

use super::{answer, Args, Cx};
use crate::admin::{self, BootBy, Command};
use crate::login::notice;

/// The calls, by their function ids.
pub mod function {
    pub const SERVER_LIST: u32 = 0x8201_002F;
    pub const BOOT: u32 = 0x8201_0092;
    pub const BAN: u32 = 0x8201_0016;
    pub const FINGER_CHARACTER: u32 = 0x8201_001B;
    pub const FINGER_ID: u32 = 0x8201_0023;
    pub const FINGER_ACCOUNT: u32 = 0x8201_0027;
    pub const USER_LIMIT: u32 = 0x8201_008B;
    pub const ADMIN_TABLE: u32 = 0x8201_0075;
    pub const BROADCAST: u32 = 0x8201_0073;
    pub const GAMECAST: u32 = 0x8201_0134;
    pub const GAMECAST_LOCAL: u32 = 0x8201_0127;
    pub const SEND_TEXT: u32 = 0x8201_013F;
    pub const SEND_TEXT_ID: u32 = 0x8201_0121;
    pub const ADMIN_DIALOG: u32 = 0x8201_0074;
    pub const RENAME: u32 = 0x8201_0130;
    pub const SUFFIX: u32 = 0x8201_0113;
    pub const SERVER_TELL: u32 = 0x8201_0084;
    pub const CLOAK: u32 = 0x8201_0017;
    pub const UNCLOAK: u32 = 0x8201_0061;
    pub const CLOAK_STATUS: u32 = 0x8201_0009;
    pub const SEES_HIDDEN: u32 = 0x8201_0088;
}

/// The line a `/admindialog` with no words of its own shows its target, the client's table's words.
const DIALOG_LINE: &str = "A Game Administrator needs to talk with you.";

/// A call of this kind: its answer; `None` for any other.
pub(super) fn call<S: Store>(cx: &mut Cx<'_, S>, f: u32, a: &Args<'_>) -> Option<Vec<String>> {
    Some(match f {
        function::BOOT => boot(cx, a),
        function::BAN => ban(cx, a),
        function::FINGER_CHARACTER => finger_character(cx, a),
        function::FINGER_ID => finger_id(cx, a),
        function::FINGER_ACCOUNT => finger_account(cx, a),
        function::USER_LIMIT => user_limit(cx, a),
        function::ADMIN_TABLE => crate::admin_groups::table(cx.s),
        function::SERVER_LIST => server_list(cx),
        function::BROADCAST => broadcast(cx, a.text(0)),
        function::GAMECAST => broadcast(cx, a.text(0)),
        function::GAMECAST_LOCAL => local(cx, a),
        function::SEND_TEXT => send_text(cx, a),
        function::SEND_TEXT_ID => send_text_id(cx, a),
        function::ADMIN_DIALOG => dialog(cx, a),
        function::RENAME => rename(cx, a),
        function::SUFFIX => suffix(cx, a),
        function::SERVER_TELL => server_tell(cx, a),
        function::CLOAK => cloak(cx, true),
        function::UNCLOAK => cloak(cx, false),
        function::CLOAK_STATUS => vec![if cx.state().cloaked {
            "You are cloaked.".to_owned()
        } else {
            "You are not cloaked.".to_owned()
        }],
        function::SEES_HIDDEN => {
            let on = a.int(0).unwrap_or(0) != 0;
            cx.state().sees_hidden = on;
            crate::presence::refresh(cx.s, cx.net, cx.world);
            vec![format!(
                "You {} what is hidden.",
                if on { "now see" } else { "no longer see" }
            )]
        }
        _ => return None,
    })
}

/// Run one of the console's commands for this admin.
fn run<S: Store>(cx: &mut Cx<'_, S>, c: &Command) -> Vec<String> {
    admin::run(cx.s, cx.net, cx.clock, cx.by, c)
}

fn boot<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let (account, character, id) = (a.text(1), a.text(2), a.long(3).unwrap_or(0));
    let c = if let Some(who) = character.filter(|w| !w.is_empty()) {
        Command::Boot {
            by: BootBy::Character,
            who,
            reason: None,
        }
    } else if let Some(who) = account.filter(|w| !w.is_empty()) {
        Command::Boot {
            by: BootBy::Account,
            who,
            reason: None,
        }
    } else if id != 0 {
        Command::Boot {
            by: BootBy::Id,
            who: format!("{id:#x}"),
            reason: None,
        }
    } else {
        return vec!["Name a character, an account or an id to boot.".to_owned()];
    };
    run(cx, &c)
}

fn ban<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let Some(name) = a.text(1) else {
        return vec!["Name an account to ban.".to_owned()];
    };
    let n = |i| f64::from(a.signed(i).unwrap_or(0));
    run(
        cx,
        &Command::Ban {
            name,
            days: n(2),
            hours: n(3),
            minutes: n(4),
            reason: None,
        },
    )
}

/// A character's line: the account behind it.
fn finger_line<S: Store>(cx: &mut Cx<'_, S>, c: &yalain_store::Character) -> String {
    let account =
        cx.s.store
            .accounts()
            .ok()
            .and_then(|all| all.into_iter().find(|a| a.id == c.account))
            .map_or_else(|| "(none)".to_owned(), |a| a.name);
    format!("{} ({:#x}) is on account {account}.", c.name, c.id)
}

fn finger_character<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let name = a.text(1).unwrap_or_default();
    match cx.s.store.character_named(&name) {
        Ok(Some(c)) => vec![finger_line(cx, &c)],
        _ => vec![format!("No character is named {name}.")],
    }
}

fn finger_id<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let id = a.long(1).unwrap_or(0);
    match cx.s.store.character(id) {
        Ok(Some(c)) => vec![finger_line(cx, &c)],
        _ => vec![format!("No character has the id {id:#x}.")],
    }
}

fn finger_account<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let name = a.text(1).unwrap_or_default();
    let Ok(Some(account)) = cx.s.store.account_by_name(&name) else {
        return vec![format!("No account is named {name}.")];
    };
    let characters = cx.s.store.characters(account.id).unwrap_or_default();
    let mut lines = vec![format!(
        "Account {} ({}) has {} characters.",
        account.name,
        account.id,
        characters.len()
    )];
    lines.extend(characters.iter().map(|c| {
        format!(
            "{} ({:#x}){}",
            c.name,
            c.id,
            if c.deleted_at.is_some() {
                ", deleted"
            } else {
                ""
            }
        )
    }));
    lines
}

fn user_limit<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let Some(n) = a.int(1) else {
        return vec!["Give the number of logins the world holds.".to_owned()];
    };
    cx.s.cfg.max_sessions = n;
    vec![format!("The world now holds {n} logins.")]
}

fn server_list<S: Store>(cx: &Cx<'_, S>) -> Vec<String> {
    vec![format!(
        "{}: {} logins, {} characters in the world.",
        cx.s.cfg.world_name,
        cx.s.sessions.len(),
        cx.s.players.len()
    )]
}

/// Show every player in the world a line from this admin.
fn broadcast<S: Store>(cx: &mut Cx<'_, S>, message: Option<String>) -> Vec<String> {
    let Some(message) = message.filter(|m| !m.is_empty()) else {
        return vec!["Give the words to show.".to_owned()];
    };
    run(cx, &Command::Gamecast { message })
}

/// Everyone within the admin's landblock.
fn near<S: Store>(cx: &Cx<'_, S>) -> Vec<ConnId> {
    let Some(me) = cx.s.players.get(&cx.world) else {
        return Vec::new();
    };
    let block = me.position.cell >> 16;
    let mut near: Vec<ConnId> =
        cx.s.players
            .iter()
            .filter(|(_, p)| p.position.cell >> 16 == block)
            .map(|(w, _)| *w)
            .collect();
    near.sort_unstable();
    near
}

fn local<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let Some(message) = a.text(0).filter(|m| !m.is_empty()) else {
        return vec!["Give the words to show.".to_owned()];
    };
    let from = cx.name_of(cx.world);
    let line = format!("Broadcast from {from}> {message}");
    let near = near(cx);
    for w in &near {
        notice(cx.net, *w, &line);
    }
    vec![format!("Shown to {} nearby.", near.len())]
}

fn send_text<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let name = a.text(0).unwrap_or_default();
    let Some(w) = cx.player_named(&name) else {
        return vec![format!("{name} is not in the world.")];
    };
    let line = a.text(1).unwrap_or_default();
    notice(cx.net, w, &line);
    vec![format!("Shown to {}.", cx.name_of(w))]
}

fn send_text_id<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let w = match cx.player(a.long(0).unwrap_or(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let line = a.text(1).unwrap_or_default();
    notice(cx.net, w, &line);
    vec![format!("Shown to {}.", cx.name_of(w))]
}

fn dialog<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let w = match cx.player(a.long(0).unwrap_or(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let words = a.text(1).filter(|t| !t.is_empty());
    notice(cx.net, w, words.as_deref().unwrap_or(DIALOG_LINE));
    vec![format!("{} has been told.", cx.name_of(w))]
}

fn rename<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let w = match cx.player(a.long(0).unwrap_or(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let typed = a.text(1).unwrap_or_default();
    let name = match cx.s.names.check(&typed) {
        Ok(name) => name,
        Err(why) => return vec![format!("{typed} may not be a name: {why:?}.")],
    };
    if cx.s.store.name_taken(&name).unwrap_or(true) {
        return vec![format!("The name {name} is taken.")];
    }
    let Some(p) = cx.s.players.get_mut(&w) else {
        return vec!["That is not a player in the world.".to_owned()];
    };
    let id = p.character;
    if let Err(e) = cx.s.store.rename_character(id, &name) {
        return vec![format!("{name} was not kept: {e}")];
    }
    let Some(p) = cx.s.players.get_mut(&w) else {
        return Vec::new();
    };
    let old = std::mem::replace(&mut p.name, name.clone());
    cx.s.renames.push(crate::player::Renamed {
        character: id,
        old: old.clone(),
        new: name.clone(),
    });
    crate::player::show_name(cx.s, cx.net, w);
    crate::world::keep_player(cx.s, w);
    answer(cx.net, w, &format!("You are now named {name}."));
    vec![format!("{old} is now {name}.")]
}

/// The longest suffix a name may have.
pub const MAX_SUFFIX: usize = 24;

/// `/suffix` puts words after a player's name, kept with it; none takes them off. Letters, digits,
/// spaces and a little punctuation: nothing that reads as the client's mark for the name's grammar.
/// Where the original showed one isn't held: here, after the name, before that mark.
fn suffix<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let w = match cx.player(a.long(0).unwrap_or(0)) {
        Ok(w) => w,
        Err(why) => return vec![why],
    };
    let typed = a.text(1).unwrap_or_default();
    let words = typed.trim().trim_matches('"').trim();
    let fits = |c: char| c.is_alphanumeric() || matches!(c, ' ' | '\'' | '-' | '.' | ',');
    if words.chars().count() > MAX_SUFFIX || !words.chars().all(fits) {
        return vec![format!(
            "A suffix is up to {MAX_SUFFIX} letters, digits, spaces and ' - . ,"
        )];
    }
    let Some(p) = cx.s.players.get_mut(&w) else {
        return Vec::new();
    };
    words.clone_into(&mut p.play.suffix);
    let shown = p.shown_name();
    crate::player::show_name(cx.s, cx.net, w);
    crate::world::keep_player(cx.s, w);
    vec![format!("Shown as {shown}.")]
}

/// `/gunservertell`: a line to a player by name, from no one.
fn server_tell<S: Store>(cx: &mut Cx<'_, S>, a: &Args<'_>) -> Vec<String> {
    let (target, words) = (a.text(0).unwrap_or_default(), a.text(1).unwrap_or_default());
    let Some(w) = cx.player_named(&target) else {
        return vec![format!("{target} is not in the world.")];
    };
    notice(cx.net, w, &words);
    vec![format!("Shown to {}.", cx.name_of(w))]
}

fn cloak<S: Store>(cx: &mut Cx<'_, S>, on: bool) -> Vec<String> {
    cx.state().cloaked = on;
    crate::presence::cloak_changed(cx.s, cx.net, cx.world);
    vec![if on {
        "You are cloaked: players no longer see you.".to_owned()
    } else {
        "You are no longer cloaked.".to_owned()
    }]
}
