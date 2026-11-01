//! The login connection, as the January 2017 login server ran it.
//!
//! - **Login.** The account is checked ([`AccountPolicy`]); the server sends the world's name and
//!   asks which data the client holds.
//! - **The character list.** When the client answers, the server says the download is over and
//!   sends the account's characters twice: by name and id ([`CharacterNames`], those in play), then
//!   with their looks ([`CharacterList`], deleted ones still greyed out included).
//! - **Data requests** are answered from the server's data files ([`crate::download`]).
//! - **Creating** a character answers with its identity and look, or the reason it was not made.
//! - **Deleting** greys the character out for an hour, then it is gone; the lists are sent again.
//! - **Entering** the world sends a referral to the world service; the client's world connection
//!   then belongs to that character.
//! - **Leaving** the world answers that the character has left, and the lists again; a character
//!   in the world lingers in it for about nineteen seconds first ([`fn@crate::world::log_off`]).
//! - **Logging in again.** An account has one character in the world at a time: entering is
//!   refused (reason 11) while one of its characters is in the world or on its way in. A new login
//!   of an account whose character is still in the world (its client gone without logging off)
//!   logs that character off, and the new login enters once it has left. A client that goes away
//!   with its character in the world leaves it lingering about fifteen seconds
//!   ([`crate::world::dropped`]).

use auberean_protocol::login::{
    CharacterCreated, CharacterDeleted, CharacterList, CharacterNames, CharacterRefused,
    CreateCharacter, DataQuery, Empty, Notice, Text,
};
use auberean_protocol::text::{NarrowString, StringInfo, WideString};
use auberean_protocol::types::{CharacterIdentity, VisualDesc};
use auberean_protocol::wire::{Field, Reader, Writer};
use auberean_protocol::{message_type, queue, ClientBlob, ClientMessage, ServerMessage};
use sha2::{Digest, Sha256};
use yalain_net::login::GameLogin;
use yalain_net::{BlobKind, CloseReason, ConnId, ServerNet};
use yalain_store::{Account, Character, Store};

use crate::{AccountPolicy, Clock, Server, Session, Stage};

/// The high word of every player's object id.
pub const PLAYER_ID_BASE: u64 = 0x2130_0000_0000_0000;

/// The data query the January 2017 login servers sent: region 1, names in English, and the six
/// languages they served.
#[must_use]
pub fn retail_data_query() -> DataQuery {
    DataQuery {
        region: 1,
        name_language: 0x8000_0001,
        languages: vec![
            0x4100_0001,
            0x8100_0001,
            0x8100_0003,
            0x8100_0004,
            0x8100_0005,
            0x8000_0001,
        ],
    }
}

/// The refusal reason for an account that may not log in.
pub const ACCOUNT_INVALID: u32 = 7;
/// A character answer for choices that make no character (an unknown race or sex).
pub const CORRUPT: u32 = 5;
/// A character answer when the character could not be kept.
pub const DATABASE_DOWN: u32 = 6;

fn send(net: &mut ServerNet, conn: ConnId, queue: u16, kind: BlobKind, m: &ServerMessage) {
    let bytes = m.encode();
    tracing::debug!(
        ?conn,
        queue,
        len = bytes.len(),
        "sent {:#x}",
        m.message_type()
    );
    net.send(conn, queue, kind, bytes);
}

/// A credential check: a salt, then the hash of the salt and the ticket.
#[must_use]
pub fn secret_for(name: &str, ticket: &[u8], now: i64) -> Vec<u8> {
    let salt: Vec<u8> = Sha256::new()
        .chain_update(b"salt")
        .chain_update(name.as_bytes())
        .chain_update(now.to_le_bytes())
        .finalize()[..16]
        .to_vec();
    let mut out = salt.clone();
    out.extend_from_slice(
        &Sha256::new()
            .chain_update(&salt)
            .chain_update(ticket)
            .finalize(),
    );
    out
}

/// Does a ticket match a credential check?
#[must_use]
pub fn ticket_matches(secret: &[u8], ticket: &[u8]) -> bool {
    let Some((salt, hash)) = secret.split_at_checked(16) else {
        return false;
    };
    Sha256::new()
        .chain_update(salt)
        .chain_update(ticket)
        .finalize()
        .as_slice()
        == hash
}

/// Make an account beforehand (for [`AccountPolicy::Listed`]).
///
/// # Errors
/// The store's.
pub fn add_account<S: Store>(
    store: &mut S,
    name: &str,
    ticket: &[u8],
    now: i64,
) -> yalain_store::StoreResult<Account> {
    store.create_account(name, Some(secret_for(name, ticket, now)), now)
}

/// The account a character made outside a client's login (a server command) goes on: the account
/// of that name; else, on an open world, one made with no ticket, which takes the first ticket it
/// logs in with. A world of listed accounts makes none, since an account with no ticket could
/// never log in to it.
///
/// # Errors
/// What the operator should do instead, or the store's failure.
pub fn account_for_command<S: Store>(
    policy: AccountPolicy,
    store: &mut S,
    name: &str,
    now: i64,
) -> Result<Account, String> {
    match store
        .account_by_name(name)
        .map_err(|e| format!("account {name}: {e}"))?
    {
        Some(a) => Ok(a),
        None if policy == AccountPolicy::Open => store
            .create_account(name, None, now)
            .map_err(|e| format!("account {name}: {e}")),
        None => Err(format!(
            "no account {name}: this world admits only listed accounts, so make it with its \
             ticket first (--add-account {name}:TICKET)"
        )),
    }
}

/// The account a login of this name and ticket is admitted as, by the world's rule: on an open
/// world the account is made the first time its name logs in, with that ticket, and admitted from
/// then on with that ticket only (an account made with none takes the first ticket it logs in
/// with); on a world of listed accounts only an account made beforehand, with its own ticket. The
/// friends service checks its tickets by the same rule.
pub fn admits<S: Store>(
    policy: AccountPolicy,
    store: &mut S,
    name: &str,
    ticket: &[u8],
    now: i64,
) -> Option<Account> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    match (policy, store.account_by_name(name)) {
        (AccountPolicy::Open, Ok(Some(mut a))) => match a.secret.as_deref() {
            Some(sec) => ticket_matches(sec, ticket).then_some(a),
            None => {
                let secret = secret_for(&a.name, ticket, now);
                store.set_account_secret(a.id, secret.clone()).ok()?;
                a.secret = Some(secret);
                Some(a)
            }
        },
        (AccountPolicy::Open, Ok(None)) => store
            .create_account(name, Some(secret_for(name, ticket, now)), now)
            .ok(),
        (AccountPolicy::Listed, Ok(Some(a))) => a
            .secret
            .as_deref()
            .is_some_and(|sec| ticket_matches(sec, ticket))
            .then_some(a),
        (_, Ok(None) | Err(_)) => None,
    }
}

pub(crate) fn logged_in<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    conn: ConnId,
    login: &GameLogin,
    clock: Clock,
) {
    let name = login.account.trim().to_string();
    let account = admits(
        s.cfg.accounts,
        &mut s.store,
        &name,
        &login.ticket,
        clock.unix,
    );
    let Some(account) = account else {
        tracing::info!(?conn, "login refused");
        let m = ServerMessage::CharacterRefused(CharacterRefused {
            reason: ACCOUNT_INVALID,
        });
        send(net, conn, queue::EVENT, BlobKind::LOGIN, &m);
        return;
    };
    tracing::info!(?conn, account = %account.name, version = %login.version, "logged in");
    logged_in_again(s, net, account.id, clock);
    s.sessions.insert(
        conn,
        Session {
            account,
            stage: Stage::Selecting,
        },
    );
    let name = ServerMessage::WorldName(Text(StringInfo::literal(&s.cfg.world_name)));
    send(net, conn, queue::EVENT, BlobKind::LOGIN, &name);
    send(
        net,
        conn,
        queue::DATABASE,
        BlobKind::LOGIN,
        &ServerMessage::DataQuery(retail_data_query()),
    );
}

/// An account logs in again while an earlier login of it still has a character in the world (its
/// client gone without logging off): that character is logged off, lingering as any logging off
/// does, and an entry the earlier login had asked for is forgotten (its referral spent).
fn logged_in_again<S: Store>(s: &mut Server<S>, net: &mut ServerNet, account: u64, clock: Clock) {
    let earlier: Vec<(ConnId, Stage)> = s
        .sessions
        .iter()
        .filter(|(_, x)| x.account.id == account)
        .map(|(c, x)| (*c, x.stage))
        .collect();
    for (conn, stage) in earlier {
        match stage {
            Stage::InWorld { world, .. } => {
                tracing::info!(
                    ?conn,
                    ?world,
                    "logged in again: the character in the world logs off"
                );
                crate::world::log_off(s, net, world, clock);
            }
            Stage::Entering { .. } => {
                if let Some(x) = s.sessions.get_mut(&conn) {
                    x.stage = Stage::Selecting;
                }
            }
            Stage::Selecting => {}
        }
    }
}

/// Decode a stored look.
fn look_of(c: &Character) -> VisualDesc {
    VisualDesc::read(&mut Reader::new(&c.look)).unwrap_or_default()
}

/// Encode a look for keeping.
fn look_bytes(v: &VisualDesc) -> Vec<u8> {
    let mut w = Writer::default();
    v.write(&mut w);
    w.finish()
}

/// Seconds a deleted character has left in the list; `None` once it is gone.
fn greyed_for(cfg_seconds: u32, deleted_at: Option<i64>, now: i64) -> Option<u32> {
    match deleted_at {
        None => Some(0),
        Some(at) => {
            let left = i64::from(cfg_seconds) - (now - at);
            (left > 0).then(|| u32::try_from(left).unwrap_or(0))
        }
    }
}

/// Send the account's characters: by name, then with their looks (wearing what they wear).
pub(crate) fn send_lists<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    conn: ConnId,
    clock: Clock,
) {
    let Some(session) = s.sessions.get(&conn) else {
        return;
    };
    let account = session.account.clone();
    let mut names = CharacterNames {
        account: NarrowString::from(account.name.as_str()),
        ..CharacterNames::default()
    };
    let mut list = CharacterList {
        status: 0,
        slots: s.cfg.slots,
        account: NarrowString::from(account.name.as_str()),
        uses_chat_service: 1,
        has_expansion: 1,
        unknown: 1,
        ..CharacterList::default()
    };
    for c in s.store.characters(account.id).unwrap_or_default() {
        let Some(greyed) = greyed_for(s.cfg.delete_seconds, c.deleted_at, clock.unix) else {
            let _ = s.store.purge_character(c.id);
            continue;
        };
        if greyed == 0 {
            names.names.push(WideString::from(c.name.as_str()));
            names.ids.push(c.id);
        }
        // Drawn wearing what it wore when it was last kept.
        let items = s
            .store
            .world_state(c.id)
            .ok()
            .flatten()
            .and_then(|st| crate::state::PlayState::from_bytes(&st.play).ok())
            .map(|p| p.items)
            .unwrap_or_default();
        let visual = crate::equip::dressed(&look_of(&c), &s.catalog, &items, c.race, c.sex);
        list.characters.push(CharacterIdentity {
            id: c.id,
            name: WideString::from(c.name.as_str()),
            seconds_greyed_out: greyed,
            visual,
        });
    }
    send(
        net,
        conn,
        queue::EVENT,
        BlobKind::LOGIN,
        &ServerMessage::CharacterNames(names),
    );
    send(
        net,
        conn,
        queue::EVENT,
        BlobKind::LOGIN,
        &ServerMessage::CharacterList(list),
    );
}

fn create<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    conn: ConnId,
    c: &CreateCharacter,
    clock: Clock,
) {
    let Some(session) = s.sessions.get(&conn) else {
        return;
    };
    let account = session.account.clone();
    let typed = c.name.text();
    let answer = |net: &mut ServerNet, response: u32, identity: CharacterIdentity| {
        let m = ServerMessage::CharacterCreated(CharacterCreated {
            response,
            identity,
            result: 0,
        });
        send(net, conn, queue::EVENT, BlobKind::LOGIN, &m);
    };
    let name = match s.names.check(&typed) {
        Ok(name) => name,
        Err(why) => {
            tracing::info!(?conn, name = %typed, ?why, "character name refused");
            return answer(
                net,
                CharacterCreated::NAME_REFUSED,
                CharacterIdentity::default(),
            );
        }
    };
    if s.store.name_taken(&name).unwrap_or(true) {
        return answer(
            net,
            CharacterCreated::NAME_IN_USE,
            CharacterIdentity::default(),
        );
    }
    let active = s
        .store
        .characters(account.id)
        .unwrap_or_default()
        .iter()
        .filter(|ch| ch.deleted_at.is_none())
        .count();
    if active >= s.cfg.slots as usize {
        return answer(
            net,
            CharacterCreated::NO_FREE_SLOT,
            CharacterIdentity::default(),
        );
    }
    let Some(look) = s.rules.look(c.race, c.sex, &c.physique) else {
        return answer(net, CORRUPT, CharacterIdentity::default());
    };
    let Ok(n) = s.store.next_character_number() else {
        return answer(net, DATABASE_DOWN, CharacterIdentity::default());
    };
    let id = PLAYER_ID_BASE | n;
    let record = Character {
        id,
        account: account.id,
        name: name.clone(),
        race: c.race,
        sex: c.sex,
        class: c.class,
        implement: c.implement,
        start_area: c.start_area,
        entity_did: c.entity_did,
        physique: c.physique.entries.clone(),
        look: look_bytes(&look),
        created_at: clock.unix,
        deleted_at: None,
        last_played: clock.unix,
    };
    if s.store.insert_character(&record).is_err() {
        return answer(net, DATABASE_DOWN, CharacterIdentity::default());
    }
    tracing::info!(?conn, character = %name, id = format_args!("{id:#x}"), "character created");
    answer(
        net,
        CharacterCreated::OK,
        CharacterIdentity {
            id,
            name: WideString::from(name.as_str()),
            seconds_greyed_out: 0,
            visual: look,
        },
    );
}

fn owned<S: Store>(s: &mut Server<S>, conn: ConnId, character: u64) -> Option<Character> {
    let account = s.sessions.get(&conn)?.account.id;
    s.store
        .character(character)
        .ok()
        .flatten()
        .filter(|c| c.account == account && c.deleted_at.is_none())
}

fn in_world_elsewhere<S: Store>(s: &Server<S>, character: u64) -> bool {
    s.players.values().any(|p| p.character == character)
        || s.sessions.values().any(|x| match x.stage {
            Stage::Entering { character: c, .. } | Stage::InWorld { character: c, .. } => {
                c == character
            }
            Stage::Selecting => false,
        })
}

/// Has the account a character in the world (lingering included), or on its way in?
fn account_in_world<S: Store>(s: &mut Server<S>, account: u64) -> bool {
    if s.sessions
        .values()
        .any(|x| x.account.id == account && !matches!(x.stage, Stage::Selecting))
    {
        return true;
    }
    let present: Vec<u64> = s.players.values().map(|p| p.character).collect();
    present.into_iter().any(|character| {
        s.store
            .character(character)
            .ok()
            .flatten()
            .is_some_and(|c| c.account == account)
    })
}

fn delete<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    conn: ConnId,
    character: u64,
    clock: Clock,
) {
    if owned(s, conn, character).is_none() || in_world_elsewhere(s, character) {
        s.unhandled
            .note("deleting a character not on the account or in the world");
        return;
    }
    if s.store.set_deleted(character, Some(clock.unix)).is_err() {
        return;
    }
    tracing::info!(
        ?conn,
        id = format_args!("{character:#x}"),
        "character deleted"
    );
    send(
        net,
        conn,
        queue::EVENT,
        BlobKind::LOGIN,
        &ServerMessage::CharacterDeleted(CharacterDeleted { character }),
    );
    send_lists(s, net, conn, clock);
}

fn refuse(net: &mut ServerNet, conn: ConnId, reason: u32) {
    send(
        net,
        conn,
        queue::EVENT,
        BlobKind::LOGIN,
        &ServerMessage::CharacterRefused(CharacterRefused { reason }),
    );
}

fn enter<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    conn: ConnId,
    character: u64,
    clock: Clock,
) {
    let Some(c) = owned(s, conn, character) else {
        return refuse(net, conn, CharacterRefused::NOT_OWNED);
    };
    if account_in_world(s, c.account) {
        return refuse(net, conn, CharacterRefused::IN_WORLD);
    }
    if !s.cfg.referral {
        if let Some(session) = s.sessions.get_mut(&conn) {
            session.stage = Stage::InWorld {
                character,
                world: conn,
            };
        }
        tracing::info!(
            ?conn,
            id = format_args!("{character:#x}"),
            "entering the world on the login connection"
        );
        crate::world::enter(s, net, conn, conn, character, clock);
        return;
    }
    let Some(cookie) = net.refer(conn, clock.local) else {
        return;
    };
    if let Some(session) = s.sessions.get_mut(&conn) {
        session.stage = Stage::Entering { character, cookie };
    }
    tracing::info!(
        ?conn,
        id = format_args!("{character:#x}"),
        "entering the world: referral sent"
    );
}

/// A player logs off: a character in the world first lingers in it (its leaving ends in
/// [`logged_off`]); any other leaves at once. At the character list there is nothing to leave and
/// nothing is answered (the client logs off so as it leaves its play screen after a refused entry).
fn log_off<S: Store>(s: &mut Server<S>, net: &mut ServerNet, conn: ConnId, clock: Clock) {
    match s.sessions.get(&conn).map(|x| x.stage) {
        Some(Stage::InWorld { world, .. }) => {
            if crate::world::log_off(s, net, world, clock) {
                return;
            }
        }
        Some(Stage::Selecting) | None => return,
        Some(Stage::Entering { .. }) => {}
    }
    leave(s, net, conn, clock);
}

/// A character that logged off (or whose client went away) is out of the world: it leaves, and its
/// login, when it is still there, is told so with the character list.
pub(crate) fn logged_off<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    login: ConnId,
    world: ConnId,
    clock: Clock,
) {
    let still = s
        .sessions
        .get(&login)
        .is_some_and(|x| matches!(x.stage, Stage::InWorld { world: w, .. } if w == world));
    if still {
        return leave(s, net, login, clock);
    }
    let character = s.players.get(&world).map(|p| p.character);
    crate::world::leave(s, net, world);
    s.worlds.remove(&world);
    net.close(world);
    if let Some(character) = character {
        let _ = s.store.set_last_played(character, clock.unix);
    }
}

fn leave<S: Store>(s: &mut Server<S>, net: &mut ServerNet, conn: ConnId, clock: Clock) {
    let Some(stage) = s.sessions.get(&conn).map(|x| x.stage) else {
        return;
    };
    let played = match stage {
        Stage::InWorld { character, world } => {
            crate::world::leave(s, net, world);
            if world != conn {
                s.worlds.remove(&world);
                net.close(world);
            }
            Some(character)
        }
        Stage::Entering { character, .. } => Some(character),
        Stage::Selecting => None,
    };
    if let Some(session) = s.sessions.get_mut(&conn) {
        session.stage = Stage::Selecting;
    }
    // The list orders characters by when they last left the world: this one goes last.
    if let Some(character) = played {
        let _ = s.store.set_last_played(character, clock.unix);
    }
    send(
        net,
        conn,
        queue::EVENT,
        BlobKind::UNORDERED,
        &ServerMessage::CharExitGame(Empty),
    );
    send_lists(s, net, conn, clock);
}

/// A message on a login connection.
pub(crate) fn message<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    conn: ConnId,
    q: u16,
    bytes: &[u8],
    clock: Clock,
) {
    tracing::debug!(
        ?conn,
        queue = q,
        len = bytes.len(),
        "received {:#x}",
        message_type(bytes).unwrap_or(0)
    );
    if !s.sessions.contains_key(&conn) {
        s.unhandled.note("a message before the login was accepted");
        return;
    }
    let m = match ClientBlob::decode(q, bytes) {
        Ok(ClientBlob::Message(m)) => m,
        Ok(ClientBlob::Secure { queue, .. }) => {
            s.unhandled
                .note(format!("an encrypted message on queue {queue}"));
            return;
        }
        Err(e) => {
            s.unhandled
                .note(format!("a message that does not read ({e})"));
            return;
        }
    };
    match m {
        ClientMessage::DataHeld(_) => {
            send(
                net,
                conn,
                queue::DATABASE,
                BlobKind::LOGIN,
                &ServerMessage::DownloadEnds(Empty),
            );
            send_lists(s, net, conn, clock);
        }
        ClientMessage::DataRequest(r) => crate::download::answer(s, net, conn, r),
        ClientMessage::CreateCharacter(c) => create(s, net, conn, &c, clock),
        ClientMessage::DeleteCharacter(d) => delete(s, net, conn, d.character, clock),
        ClientMessage::PlayCharacter(p) => enter(s, net, conn, p.character, clock),
        ClientMessage::LogOff(_) => log_off(s, net, conn, clock),
        ClientMessage::DownloadEnds(_) => {}
        ClientMessage::Chat(p) => crate::chat::service(s, net, conn, &p.0),
        other => s.unhandled.note(format!(
            "message {:#x} on the login connection",
            other.message_type()
        )),
    }
}

/// A client reached the world.
pub(crate) fn world_joined<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    login: ConnId,
    cookie: u64,
    clock: crate::Clock,
) {
    let Some(session) = s.sessions.get_mut(&login) else {
        net.close(world);
        return;
    };
    match session.stage {
        Stage::Entering {
            character,
            cookie: c,
        } if c == cookie => {
            session.stage = Stage::InWorld { character, world };
            s.worlds.insert(world, login);
            tracing::info!(
                ?world,
                ?login,
                id = format_args!("{character:#x}"),
                "world connection up"
            );
            crate::world::enter(s, net, world, login, character, clock);
        }
        _ => net.close(world),
    }
}

/// Is a message on a login connection one of the world's (it carries the world when the server
/// sends no referral)?
pub(crate) fn is_world_bound(q: u16, bytes: &[u8]) -> bool {
    matches!(ClientBlob::decode(q, bytes), Ok(ClientBlob::Message(m)) if crate::world::is_world_message(&m))
}

/// A connection ended.
pub(crate) fn closed<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    conn: ConnId,
    why: CloseReason,
    clock: Clock,
) {
    if let Some(&login) = s.worlds.get(&conn) {
        tracing::info!(?conn, ?why, "world connection closed");
        // Its character lingers, and leaves as one whose client went away.
        if crate::world::dropped(s, net, conn, clock) {
            return;
        }
        s.worlds.remove(&conn);
        if let Some(session) = s.sessions.get_mut(&login) {
            if let Stage::InWorld { world, character } = session.stage {
                if world == conn {
                    session.stage = Stage::Selecting;
                    let _ = s.store.set_last_played(character, clock.unix);
                }
            }
        }
        return;
    }
    if let Some(session) = s.sessions.remove(&conn) {
        tracing::info!(?conn, ?why, account = %session.account.name, "login connection closed");
        if let Stage::InWorld { world, character } = session.stage {
            if crate::world::dropped(s, net, world, clock) {
                return;
            }
            if world != conn {
                s.worlds.remove(&world);
                net.close(world);
            }
            let _ = s.store.set_last_played(character, clock.unix);
        }
    }
}

/// Show a notice (the kind the 2017 servers used for system text) on a connection. The client
/// adds each notice to the chat window as it comes, and the client's own strings end their lines,
/// so a literal is given a line end unless it has one.
pub fn notice(net: &mut ServerNet, conn: ConnId, text: &str) {
    let mut line = text.to_owned();
    if !line.ends_with('\n') {
        line.push('\n');
    }
    let m = ServerMessage::Notice(Notice {
        kind: 256,
        text: StringInfo::literal(&line),
    });
    send(net, conn, queue::EVENT, BlobKind::UNORDERED, &m);
}

/// Make a character outside a client's login (a server command): the account's, with this name,
/// race and sex, and a middling physique (every slider at its middle, build and height 1).
///
/// # Errors
/// The name is not a name or is taken, there is no body for the race and sex, or the store
/// failed.
pub fn seed_character<S: Store>(
    store: &mut S,
    rules: &crate::CharGenRules,
    account: &Account,
    name: &str,
    race: u32,
    sex: u32,
    now: i64,
) -> Result<Character, String> {
    let name = crate::names::NameRules::default()
        .check(name)
        .map_err(|why| format!("the name {name} is not a name ({why:?})"))?;
    if store.name_taken(&name).unwrap_or(true) {
        return Err(format!("the name {name} is not free"));
    }
    let mut physique = vec![
        (crate::chargen::BUILD, 1.0f32),
        (crate::chargen::HEIGHT, 1.0),
    ];
    for e in rules
        .physique
        .get(&(race, sex))
        .map_or(&[][..], Vec::as_slice)
    {
        physique.push((e.physique_type, 0.5));
    }
    let table = auberean_protocol::table::Table::from_inserted(
        auberean_protocol::table::SMALL,
        physique.clone(),
    );
    let look = rules
        .look(race, sex, &table)
        .ok_or_else(|| format!("no body for race {race}, sex {sex:#x}"))?;
    let n = store.next_character_number().map_err(|e| e.to_string())?;
    let record = Character {
        id: PLAYER_ID_BASE | n,
        account: account.id,
        name,
        race,
        sex,
        class: 0,
        implement: 0,
        start_area: 0,
        entity_did: 0,
        physique,
        look: look_bytes(&look),
        created_at: now,
        deleted_at: None,
        last_played: now,
    };
    store.insert_character(&record).map_err(|e| e.to_string())?;
    Ok(record)
}
