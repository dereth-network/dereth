//! The login connection, as the January 2017 login server ran it.
//!
//! - **Login.** The account is checked ([`AccountPolicy`]); the server sends the world's name and
//!   asks which data the client holds. A login refused is answered with the refusal and its
//!   connection closed at once.
//! - **A full world.** A login past the logins the world holds, or past those one address may
//!   hold, is refused unchecked with the client's "server full" reason (19), as the January 2017
//!   login server refused one. An admin's account with its right ticket passes the world's limit
//!   (not its address's).
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
//! - **A banned account** is admitted no further: its login is ended with the client's line that
//!   the account is banned ([`crate::admin`]).
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
use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use yalain_net::login::GameLogin;
use yalain_net::{BlobKind, CloseReason, ConnId, LocalTime, ServerNet};
use yalain_store::{Account, Character, Store};

use crate::call_limit::Bucket;
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
/// How many logins a world holds at once unless its operator says otherwise: Empyrean's 128.
pub const MAX_SESSIONS: u32 = 128;
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

/// Refused logins from one address within [`LOGIN_LOCKOUT`] seconds past which its next logins
/// are refused unchecked for as long, as the friends service counts refused tickets.
pub const LOGIN_REFUSALS: usize = 5;
/// See [`LOGIN_REFUSALS`].
pub const LOGIN_LOCKOUT: f64 = 60.0;
/// The most addresses the login guard remembers refusals for; past it, those with none in the
/// window are forgotten.
const GUARD_ADDRESSES: usize = 4096;

/// How many secrets one address may have checked at once, and a second after that. Checking a
/// secret is slow by design and the world waits for it, so how many are checked is limited. Each
/// of the 198 machines recorded in 2017 logged in at most twice in a minute; this is far past a
/// household's or a shared address's, and the operator can exempt an address.
pub const ADDRESS_CHECKS: (f64, f64) = (16.0, 2.0);
/// How many secrets the world checks a second at [`SECRET_COST`], one at a time with the world's work
/// between them: about a sixth of its time. A higher cost checks fewer, for the same time.
pub const CHECKS_A_SECOND: f64 = 16.0;
/// How many logins may wait for their secrets to be checked: every place in the world logging in
/// together, twice over. Past it, one is told the world has no room.
pub const MOST_LOGINS_WAITING: usize = 256;

/// How many new accounts one address may make at once on an open world, and a second after that:
/// a household's, and each further one a minute. What an open world's accounts come to is bounded
/// by this and [`WORLD_NEW_ACCOUNTS`].
pub const ADDRESS_NEW_ACCOUNTS: (f64, f64) = (8.0, 1.0 / 60.0);
/// How many new accounts the world makes at once, and a second after that.
pub const WORLD_NEW_ACCOUNTS: (f64, f64) = (256.0, 1.0);

/// The refused logins of each address, the addresses refused unchecked until a time, how many secrets
/// each address may yet have checked, and the logins waiting their turn for theirs.
#[derive(Debug, Default)]
pub struct LoginGuard {
    refusals: HashMap<String, Vec<f64>>,
    locked: HashMap<String, f64>,
    checks: HashMap<String, Bucket>,
    new_accounts: HashMap<String, Bucket>,
    world_new_accounts: Option<Bucket>,
    /// When the next secret may be checked.
    next_check: f64,
    waiting: VecDeque<(ConnId, GameLogin)>,
    /// Logins turned away with too many waiting.
    turned_away: u64,
}

/// An address's next check, under its limit; the addresses remembered stay bounded, those with their
/// limit whole being forgotten first, and one past that is not held to it.
fn spend_address(
    buckets: &mut HashMap<String, Bucket>,
    address: &str,
    (burst, per_second): (f64, f64),
    now: LocalTime,
) -> bool {
    if buckets.len() >= GUARD_ADDRESSES && !buckets.contains_key(address) {
        buckets.retain(|_, b| !b.full(now));
        if buckets.len() >= GUARD_ADDRESSES {
            return true;
        }
    }
    let bucket = buckets
        .entry(address.to_owned())
        .or_insert_with(|| Bucket::with(burst, per_second, now));
    let allowed = bucket.allow(now);
    if !allowed && bucket.dropped == 1 {
        tracing::warn!(
            address,
            "this address is logging in faster than {per_second:.0} a second: what is over is told \
             the world has no room"
        );
    }
    allowed
}

impl LoginGuard {
    /// Is an address refused unchecked now?
    pub fn locked(&mut self, address: &str, now: f64) -> bool {
        match self.locked.get(address) {
            Some(&until) if now < until => true,
            Some(_) => {
                self.locked.remove(address);
                false
            }
            None => false,
        }
    }

    /// A login from an address was refused.
    pub fn refused(&mut self, address: &str, now: f64) {
        if self.refusals.len() >= GUARD_ADDRESSES {
            self.refusals
                .retain(|_, t| t.iter().any(|t| now - t < LOGIN_LOCKOUT));
            self.locked.retain(|_, until| now < *until);
        }
        let times = self.refusals.entry(address.to_owned()).or_default();
        times.retain(|t| now - t < LOGIN_LOCKOUT);
        times.push(now);
        if times.len() >= LOGIN_REFUSALS {
            times.clear();
            self.locked.insert(address.to_owned(), now + LOGIN_LOCKOUT);
        }
    }
}

/// The address refusals are counted against: an IPv4 address (one mapped into IPv6 included), or
/// an IPv6 address's /64.
#[must_use]
pub fn address_key(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(v) => v.to_string(),
        IpAddr::V6(v) => match v.to_ipv4_mapped() {
            Some(v4) => v4.to_string(),
            None => {
                let s = v.segments();
                format!("{:x}:{:x}:{:x}:{:x}::/64", s[0], s[1], s[2], s[3])
            }
        },
    }
}

/// The longest account name admitted, in UTF-16 units, as Empyrean admits.
pub const ACCOUNT_NAME_MAX: usize = 50;

/// Is an account name one an account may have: not empty, and no longer than
/// [`ACCOUNT_NAME_MAX`]?
#[must_use]
pub fn account_name_fits(name: &str) -> bool {
    !name.is_empty() && name.encode_utf16().count() <= ACCOUNT_NAME_MAX
}

/// The bcrypt cost new secrets are kept at, unless the operator gives another: Empyrean's
/// default work factor.
pub const SECRET_COST: u32 = 8;
/// The lowest and highest bcrypt costs; a cost outside them is taken as the nearer one.
pub const SECRET_COSTS: std::ops::RangeInclusive<u32> = 4..=31;

/// The length of a secret kept in the bcrypt form (`$2b$`, the cost, the salt and the hash).
const BCRYPT_LEN: usize = 60;
/// The length of a secret earlier versions kept: a 16-byte salt, then the SHA-256 of the salt and
/// the ticket.
const SALTED_SHA256_LEN: usize = 48;

/// A credential check: the ticket's bcrypt hash at `cost` (taken into [`SECRET_COSTS`]), with a
/// fresh salt from the system's random source. `None` when there is no random source to draw it
/// from.
#[must_use]
pub fn secret_for(ticket: &[u8], cost: u32) -> Option<Vec<u8>> {
    let cost = cost.clamp(*SECRET_COSTS.start(), *SECRET_COSTS.end());
    match bcrypt::hash(ticket, cost) {
        Ok(h) => Some(h.into_bytes()),
        Err(e) => {
            tracing::error!("an account's secret could not be made: {e}");
            None
        }
    }
}

/// Does a ticket match a credential check? The check is a bcrypt hash, or the salted SHA-256
/// earlier versions kept; either is compared in constant time. Anything else matches nothing.
#[must_use]
pub fn ticket_matches(secret: &[u8], ticket: &[u8]) -> bool {
    match secret.len() {
        BCRYPT_LEN => std::str::from_utf8(secret)
            .ok()
            .and_then(|h| bcrypt::verify(ticket, h).ok())
            .unwrap_or(false),
        SALTED_SHA256_LEN => {
            use subtle::ConstantTimeEq;
            let (salt, hash) = secret.split_at(16);
            let made = Sha256::new()
                .chain_update(salt)
                .chain_update(ticket)
                .finalize();
            made.as_slice().ct_eq(hash).into()
        }
        _ => false,
    }
}

/// Is a credential check kept as it should be: a bcrypt hash at this cost? One that is not (the
/// salted SHA-256 of earlier versions, or another cost) is kept again once its ticket matches.
#[must_use]
pub fn secret_is_current(secret: &[u8], cost: u32) -> bool {
    let cost = cost.clamp(*SECRET_COSTS.start(), *SECRET_COSTS.end());
    secret.len() == BCRYPT_LEN
        && secret.starts_with(b"$2")
        && std::str::from_utf8(&secret[4..6])
            .ok()
            .and_then(|c| c.parse::<u32>().ok())
            == Some(cost)
}

/// May a login or a friends ticket from this address go on to have its secret checked? Asked only of
/// an address its client has answered from, so that nobody can spend another's. An address the
/// operator exempts, or this machine's own, may. A name with no account on an open world would make
/// one: that has limits of its own, the world's for every address.
pub(crate) fn may_check_secret<S: Store>(s: &mut Server<S>, address: &str, name: &str) -> bool {
    let now = s.saves.now;
    let exempt = address.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
        || address == address_key(IpAddr::V6(std::net::Ipv6Addr::LOCALHOST))
        || s.cfg
            .unlimited_addresses
            .iter()
            .any(|a| address_key(*a) == address);
    if !exempt && !spend_address(&mut s.login_guard.checks, address, ADDRESS_CHECKS, now) {
        return false;
    }
    let makes_one = s.cfg.accounts == AccountPolicy::Open
        && matches!(s.store.account_by_name(name.trim()), Ok(None));
    if !makes_one {
        return true;
    }
    let guard = &mut s.login_guard;
    if !exempt && !spend_address(&mut guard.new_accounts, address, ADDRESS_NEW_ACCOUNTS, now) {
        return false;
    }
    let (burst, per_second) = WORLD_NEW_ACCOUNTS;
    guard
        .world_new_accounts
        .get_or_insert_with(|| Bucket::with(burst, per_second, now))
        .allow(now)
}

/// Is it time to check another secret?
#[must_use]
pub fn secret_check_due<S: Store>(s: &Server<S>) -> bool {
    s.saves.now.0 >= s.login_guard.next_check
}

/// A secret is about to be checked: the next waits its share of the world's time.
pub(crate) fn secret_checked<S: Store>(s: &mut Server<S>) {
    let cost = s
        .cfg
        .secret_cost
        .clamp(*SECRET_COSTS.start(), *SECRET_COSTS.end());
    #[allow(clippy::cast_possible_wrap)]
    // LINT-OK: two small whole numbers, not a float
    let work = 2f64.powi(cost as i32 - SECRET_COST as i32);
    s.login_guard.next_check = s.saves.now.0 + work / CHECKS_A_SECOND;
}

/// The logins whose turn has come.
pub(crate) fn checks_due<S: Store>(s: &mut Server<S>, net: &mut ServerNet, clock: Clock) {
    while secret_check_due(s) {
        let Some((conn, login)) = s.login_guard.waiting.pop_front() else {
            return;
        };
        if net.service(conn).is_some() {
            admit(s, net, conn, &login, clock);
        }
    }
}

/// Make an account beforehand (for [`AccountPolicy::Listed`]), its secret kept at `cost`.
///
/// # Errors
/// The name is empty or too long, the store's, or no secret could be made.
pub fn add_account<S: Store>(
    store: &mut S,
    name: &str,
    ticket: &[u8],
    cost: u32,
    now: i64,
) -> yalain_store::StoreResult<Account> {
    if !account_name_fits(name) {
        return Err(yalain_store::StoreError(format!(
            "an account name is 1 to {ACCOUNT_NAME_MAX} characters"
        )));
    }
    let secret = secret_for(ticket, cost)
        .ok_or_else(|| yalain_store::StoreError("no secret could be made".into()))?;
    store.create_account(name, Some(secret), now)
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
///
/// The ticket is the plain secret, however the client carried it: the game login's is read back to
/// its text, the friends ticket's is its decoded secret. This is the one place a secret is checked
/// or kept. Secrets are kept as bcrypt hashes at `cost`; a matching one kept otherwise (the salted
/// SHA-256 of earlier versions, or another cost) is kept again at `cost`.
pub fn admits<S: Store>(
    policy: AccountPolicy,
    cost: u32,
    store: &mut S,
    name: &str,
    ticket: &[u8],
    now: i64,
) -> Option<Account> {
    let name = name.trim();
    if !account_name_fits(name) {
        return None;
    }
    match (policy, store.account_by_name(name)) {
        // An admin's account is never claimed: with no secret it logs in with none.
        (AccountPolicy::Open, Ok(Some(mut a))) if a.secret.is_none() && !a.admin => {
            let secret = secret_for(ticket, cost)?;
            store.set_account_secret(a.id, secret.clone()).ok()?;
            a.secret = Some(secret);
            Some(a)
        }
        (AccountPolicy::Open | AccountPolicy::Listed, Ok(Some(a))) => {
            let sec = a.secret.as_deref()?;
            if !ticket_matches(sec, ticket) {
                return None;
            }
            Some(kept_current(store, a, ticket, cost))
        }
        (AccountPolicy::Open, Ok(None)) => store
            .create_account(name, Some(secret_for(ticket, cost)?), now)
            .ok(),
        (_, Ok(None) | Err(_)) => None,
    }
}

/// An admitted account whose secret is not kept as it should be ([`secret_is_current`]) has it
/// kept again from the ticket that matched it. Admitted either way: a store that fails here still
/// holds the secret the ticket matched.
fn kept_current<S: Store>(store: &mut S, mut a: Account, ticket: &[u8], cost: u32) -> Account {
    let Some(old) = a.secret.as_deref() else {
        return a;
    };
    if secret_is_current(old, cost) {
        return a;
    }
    let was = if old.len() == BCRYPT_LEN {
        "bcrypt at another cost"
    } else {
        "the older salted SHA-256"
    };
    let Some(secret) = secret_for(ticket, cost) else {
        return a;
    };
    match store.set_account_secret(a.id, secret.clone()) {
        Ok(()) => {
            tracing::info!(account = %a.name, "the account's secret, kept as {was}, is kept as bcrypt at cost {cost} now");
            a.secret = Some(secret);
        }
        Err(e) => {
            tracing::warn!(account = %a.name, "the account's secret could not be kept again: {e}");
        }
    }
    a
}

/// A login whose client has answered its handshake takes its turn: at once if nothing is waiting,
/// its secret checked as its turn comes, with the world's work between. An admin's goes first.
pub(crate) fn logged_in<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    conn: ConnId,
    login: &GameLogin,
    clock: Clock,
) {
    if held_back(s, net, conn, login, clock) {
        return;
    }
    let now = clock.local.0;
    let address = net.peer(conn).map(|a| address_key(a.ip()));
    let locked = address
        .as_deref()
        .is_some_and(|a| s.login_guard.locked(a, now));
    let name = login.account.trim();
    if !locked
        && address
            .as_deref()
            .is_some_and(|a| !may_check_secret(s, a, name))
    {
        tracing::debug!(
            ?conn,
            ?address,
            "login refused unchecked: too many from its address"
        );
        refuse_login(net, conn, CharacterRefused::SERVER_FULL, clock);
        return;
    }
    if s.login_guard.waiting.is_empty() && secret_check_due(s) {
        return admit(s, net, conn, login, clock);
    }
    if names_an_admin(s, login.account.trim()) {
        s.login_guard.waiting.push_front((conn, login.clone()));
    } else if s.login_guard.waiting.len() < MOST_LOGINS_WAITING {
        s.login_guard.waiting.push_back((conn, login.clone()));
    } else {
        s.login_guard.turned_away += 1;
        if s.login_guard.turned_away % 4096 == 1 {
            tracing::warn!(
                turned_away = s.login_guard.turned_away,
                "more logins are waiting than this world checks: what is over is told the world \
                 has no room"
            );
        }
        refuse_login(net, conn, CharacterRefused::SERVER_FULL, clock);
    }
}

/// Is a login refused for the world or its address holding as many as they may? It is told so.
fn held_back<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    conn: ConnId,
    login: &GameLogin,
    clock: Clock,
) -> bool {
    let name = login.account.trim();
    let world_full = holds_as_many_as_it_may(s, conn);
    let why = address_full(s, net, conn)
        .or_else(|| (world_full && !names_an_admin(s, name)).then_some("the world is full"));
    if let Some(why) = why {
        tracing::info!(?conn, "login refused: {why}");
        refuse_login(net, conn, CharacterRefused::SERVER_FULL, clock);
    }
    why.is_some()
}

/// A login's turn: its secret checked, and it is in or refused.
fn admit<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    conn: ConnId,
    login: &GameLogin,
    clock: Clock,
) {
    if held_back(s, net, conn, login, clock) {
        return;
    }
    let name = login.account.trim().to_string();
    let world_full = holds_as_many_as_it_may(s, conn);
    let address = net.peer(conn).map(|a| address_key(a.ip()));
    let now = clock.local.0;
    let locked = address
        .as_deref()
        .is_some_and(|a| s.login_guard.locked(a, now));
    let account = if locked {
        None
    } else {
        secret_checked(s);
        admits(
            s.cfg.accounts,
            s.cfg.secret_cost,
            &mut s.store,
            &name,
            &login.ticket,
            clock.unix,
        )
    };
    let Some(account) = account else {
        if locked {
            tracing::info!(?conn, address = ?address, "login refused unchecked: too many refused from its address");
        } else {
            tracing::info!(?conn, address = ?address, "login refused");
            if let Some(a) = address.as_deref() {
                s.login_guard.refused(a, now);
            }
        }
        // A wrong ticket for an admin's account is told what any other login is, or a full world
        // would say which accounts are admins'.
        let reason = if world_full {
            CharacterRefused::SERVER_FULL
        } else {
            ACCOUNT_INVALID
        };
        refuse_login(net, conn, reason, clock);
        return;
    };
    if crate::admin::banned(&mut s.store, &account, clock.unix) {
        tracing::info!(?conn, account = %account.name, "login refused: the account is banned");
        crate::admin::end_login(s, net, conn, crate::admin::BANNED, clock);
        return;
    }
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

/// Answer a client that told of its data and lacks nothing of the world's: the script library if
/// it's an admin's that lacks that, else the end of the download and its characters.
fn answer_held<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    conn: ConnId,
    held: &auberean_protocol::login::DataHeld,
    clock: Clock,
) {
    let admin = s.sessions.get(&conn).is_some_and(|x| x.account.admin);
    if let Some((begins, records)) = s
        .admin_library
        .as_ref()
        .filter(|_| admin)
        .and_then(|l| l.to_send(&held.files))
    {
        tracing::info!(?conn, "serving an admin's account the script library");
        send(net, conn, queue::DATABASE, BlobKind::LOGIN, begins);
        for record in records {
            send(net, conn, queue::DATABASE, BlobKind::LOGIN, record);
        }
        s.storing_library.insert(conn);
        return;
    }
    send(
        net,
        conn,
        queue::DATABASE,
        BlobKind::LOGIN,
        &ServerMessage::DownloadEnds(Empty),
    );
    send_lists(s, net, conn, clock);
}

/// Does the world hold as many logins as it may, without the new one on `conn`? Counted before the
/// ticket is checked, as Empyrean counts, since checking a ticket is slow by design.
fn holds_as_many_as_it_may<S: Store>(s: &Server<S>, conn: ConnId) -> bool {
    let held = s.sessions.keys().filter(|c| **c != conn).count();
    held >= s.cfg.max_sessions as usize
}

/// Does a login name an admin's account? Those the world's limit does not hold back (the game's
/// `/userlimit` holds back new "non-admin users"), once their ticket has been checked.
fn names_an_admin<S: Store>(s: &mut Server<S>, name: &str) -> bool {
    s.store
        .account_by_name(name)
        .ok()
        .flatten()
        .is_some_and(|a| a.admin)
}

/// Why a new login on `conn` is held back by its address, if it is: as many from it as one address
/// may hold (an address the operator exempts is not). Counted before the ticket is checked.
fn address_full<S: Store>(s: &Server<S>, net: &ServerNet, conn: ConnId) -> Option<&'static str> {
    let held = s.sessions.keys().filter(|c| **c != conn);
    let limit = s.cfg.max_sessions_per_address?;
    let ip = net.peer(conn)?.ip().to_canonical();
    if s.cfg
        .unlimited_addresses
        .iter()
        .any(|a| a.to_canonical() == ip)
    {
        return None;
    }
    let from_it = held
        .filter(|c| net.peer(**c).is_some_and(|a| a.ip().to_canonical() == ip))
        .count();
    (from_it >= limit as usize).then_some("its address holds as many logins as it may")
}

/// The world is stopping: every client is told why, which takes it from the world to its
/// disconnected screen as a connection merely ending does not, and every connection ends.
pub fn let_everyone_go<S: Store>(s: &Server<S>, net: &mut ServerNet, reason: u32, now: LocalTime) {
    let conns: std::collections::BTreeSet<ConnId> =
        s.sessions.keys().chain(s.worlds.keys()).copied().collect();
    let m = ServerMessage::CharacterRefused(CharacterRefused { reason }).encode();
    for conn in conns {
        net.send(conn, queue::EVENT, BlobKind::LOGIN, m.clone());
        net.end(conn, now);
    }
}

/// Refuse a login: the refusal, and the connection closed with it, as the January 2017 login
/// server ended a login it refused for a full world (the refusal and its goodbye together).
fn refuse_login(net: &mut ServerNet, conn: ConnId, reason: u32, clock: Clock) {
    let m = ServerMessage::CharacterRefused(CharacterRefused { reason });
    send(net, conn, queue::EVENT, BlobKind::LOGIN, &m);
    net.end(conn, clock.local);
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
        let play = crate::saves::state_of(s, c.id)
            .ok()
            .flatten()
            .and_then(|st| crate::state::PlayState::from_bytes(&st.play).ok())
            .unwrap_or_default();
        let visual = crate::equip::dressed(
            &look_of(&c),
            &s.catalog,
            &play.items,
            &play.item_states,
            c.race,
            c.sex,
        );
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
    if c.entity_did == crate::player::ADMIN_ENTITY && !account.admin {
        tracing::warn!(?conn, account = %account.name, name = %typed, "refused: an admin character asked for by a player's account");
        return answer(
            net,
            CharacterCreated::ADMIN_REFUSED,
            CharacterIdentity::default(),
        );
    }
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

/// Bring back a deleted character of this account: it is answered as a made character is, its
/// grey gone. The answer says so by itself: no lists follow it.
fn restore<S: Store>(s: &mut Server<S>, net: &mut ServerNet, conn: ConnId, character: u64) {
    let Some(account) = s.sessions.get(&conn).map(|x| x.account.id) else {
        return;
    };
    let answer = |net: &mut ServerNet, response: u32, identity: CharacterIdentity| {
        let m = ServerMessage::CharacterCreated(CharacterCreated {
            response,
            identity,
            result: 0,
        });
        send(net, conn, queue::EVENT, BlobKind::LOGIN, &m);
    };
    let found = s.store.character(character).ok().flatten();
    let Some(c) = found.filter(|c| c.account == account && c.deleted_at.is_some()) else {
        s.unhandled
            .note("restoring a character not on the account or not deleted");
        return answer(net, CORRUPT, CharacterIdentity::default());
    };
    let active = s
        .store
        .characters(account)
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
    if s.store.set_deleted(character, None).is_err() {
        return answer(net, DATABASE_DOWN, CharacterIdentity::default());
    }
    tracing::info!(
        ?conn,
        id = format_args!("{character:#x}"),
        "character restored"
    );
    let play = crate::saves::state_of(s, c.id)
        .ok()
        .flatten()
        .and_then(|st| crate::state::PlayState::from_bytes(&st.play).ok())
        .unwrap_or_default();
    let visual = crate::equip::dressed(
        &look_of(&c),
        &s.catalog,
        &play.items,
        &play.item_states,
        c.race,
        c.sex,
    );
    answer(
        net,
        CharacterCreated::OK,
        CharacterIdentity {
            id: c.id,
            name: WideString::from(c.name.as_str()),
            seconds_greyed_out: 0,
            visual,
        },
    );
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
    let Some(account) = s.sessions.get(&conn).map(|x| x.account.id) else {
        return refuse(net, conn, CharacterRefused::NOT_OWNED);
    };
    let c = match s.store.character(character) {
        Ok(found) => found.filter(|c| c.account == account && c.deleted_at.is_none()),
        Err(e) => {
            tracing::error!(?conn, "the store could not read a character to enter: {e}");
            return refuse(net, conn, CharacterRefused::ERROR);
        }
    };
    let Some(c) = c else {
        return refuse(net, conn, CharacterRefused::NOT_OWNED);
    };
    if account_in_world(s, c.account) {
        return refuse(net, conn, CharacterRefused::IN_WORLD);
    }
    // One that cannot be read is told so before it is sent anywhere.
    if let Err(reason) = crate::world::kept_for_entry(s, &c) {
        return refuse(net, conn, reason);
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
        if let Err(reason) = crate::world::enter(s, net, conn, conn, character, clock) {
            entry_refused(s, net, conn, conn, reason);
        }
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

/// An entry under way is refused after all: back at the character list, with why.
fn entry_refused<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    login: ConnId,
    world: ConnId,
    reason: u32,
) {
    if let Some(session) = s.sessions.get_mut(&login) {
        session.stage = Stage::Selecting;
    }
    if world != login {
        s.worlds.remove(&world);
        net.close(world);
    }
    refuse(net, login, reason);
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
        ClientMessage::OverlayHeld(held) => {
            s.overlay_clients.insert(conn, held);
        }
        ClientMessage::DataHeld(held) => {
            if !crate::world_patch::on_held(s, net, conn, &held, clock) {
                answer_held(s, net, conn, &held, clock);
            }
        }
        ClientMessage::DataRequest(r) => crate::download::answer(s, net, conn, r, clock),
        ClientMessage::CreateCharacter(c) => create(s, net, conn, &c, clock),
        ClientMessage::DeleteCharacter(d) => delete(s, net, conn, d.character, clock),
        ClientMessage::RestoreCharacter(r) => restore(s, net, conn, r.character),
        ClientMessage::PlayCharacter(p) => enter(s, net, conn, p.character, clock),
        ClientMessage::LogOff(_) => log_off(s, net, conn, clock),
        ClientMessage::DownloadEnds(_) => {
            // It has stored the world's records it was sent: on as any client that told of its
            // data.
            if let Some(held) = s.patches.stored(conn) {
                answer_held(s, net, conn, &held, clock);
                return;
            }
            // It has stored the library it was sent; its characters follow.
            if s.storing_library.remove(&conn) {
                tracing::info!(?conn, "an admin's client has stored the script library");
                send(
                    net,
                    conn,
                    queue::DATABASE,
                    BlobKind::LOGIN,
                    &ServerMessage::DownloadEnds(Empty),
                );
                send_lists(s, net, conn, clock);
            }
        }
        ClientMessage::Chat(p) => crate::chat::service(s, net, conn, &p.0, clock),
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
            if let Err(reason) = crate::world::enter(s, net, world, login, character, clock) {
                entry_refused(s, net, login, world, reason);
            }
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
    s.call_limits.closed(conn);
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
    s.storing_library.remove(&conn);
    s.overlay_clients.remove(&conn);
    s.patches.closed(conn);
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

/// Tell a player a whole answer, its lines in their order. It goes as one notice with inner line
/// ends: the transport puts a short message into the room an earlier packet has left, so separate
/// ones sent in a row can overtake each other.
pub fn notices(net: &mut ServerNet, conn: ConnId, lines: &[String]) {
    if !lines.is_empty() {
        notice(net, conn, &lines.join("\n"));
    }
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
    let record = new_character_record(store, rules, account, name, race, sex, now)?;
    store.insert_character(&record).map_err(|e| e.to_string())?;
    Ok(record)
}

/// The record of a character made outside a client's login, as [`seed_character`] makes one, not
/// yet kept: its name checked and free, a middling physique, its look, and a new id.
///
/// # Errors
/// The name is not a name or is taken, there is no body for the race and sex, or the store
/// failed.
pub fn new_character_record<S: Store>(
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
    Ok(record)
}
