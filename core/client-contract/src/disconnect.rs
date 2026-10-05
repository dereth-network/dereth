//! What a disconnect says: the string-table ids the character-error and server-died notices
//! name, and the two messages the account-booted and account-banned handlers format themselves.
//!
//! The runtime resolves a disconnect into one of these (`dereth_client_runtime`'s
//! `recv_disconnect_notice`), so every UI shows the same reason; the retail disconnected screen
//! re-exports them.

/// The string table in which the character-error and server-died handlers
/// build their `StringInfo` values — the same table enum every
/// pre-game dialog uses.
pub const STRING_TABLE_ENUM: u32 = 0x1000_0002;

/// The one string of the UI flow's server-died notice.
pub const SERVER_DIED_STRING_ID: &str = "ID_NetErr_ConnectionLost";

// -------------------------------------------------------------------------------------------
// The two messages this screen shows that are **not** in any string table.
// -------------------------------------------------------------------------------------------
//
// The player system's account booted handling and the account banned handling do not
// raise a character error and do not look a token up: each formats its sentence,
// wraps it as a literal `StringInfo`, and
// queues disconnected mode `0x10000002` with that error itself. So the text
// below **is** the message; nothing resolves it.
//
// The four literals are the retail client's own UTF-16LE strings, in the order the two
// handlers use them.

/// The client's format. `%s` is the reason.
pub const BOOTED_FORMAT: &str = "You have been booted from Asheron's Call%s.";

/// What account-booted handling substitutes when the reason failed to unpack or
/// unpacked as empty. The leading space is the client's, not the
/// server's: the server's own reason strings carry one too, because the client always prefixes.
pub const BOOTED_DEFAULT_REASON: &str = " for Code of Conduct Violations";

/// The client's `expiry <= 0` branch. `%s` is the
/// reason, and **there is no default**: a permanent ban with no reason says
/// "You have been banned from Asheron's Call." and nothing more.
pub const BANNED_FORMAT: &str = "You have been banned from Asheron's Call%s.";

/// The account-banned handler's `expiry > 0` branch. The first `%s` is the expiry
/// as `asctime` renders it, the second is the reason; they are adjacent in the format because the
/// reason carries its own leading space.
pub const BANNED_UNTIL_FORMAT: &str =
    "You have been banned until %s%s. For ban appeals, please visit support.turbine.com";

/// The client's `%s`-only formatting behavior, which is
/// all either handler uses.
///
/// Arguments beyond the format's `%s` count are dropped and surplus `%s` are left in place, which
/// is what the CRT would do with a mismatched call; both are unreachable here because every caller
/// is a constant in this file, and the unit tests assert the counts.
fn sprintf(fmt: &str, args: &[&str]) -> String {
    let mut out = String::with_capacity(fmt.len());
    let mut rest = fmt;
    for a in args {
        let Some(at) = rest.find("%s") else { break };
        out.push_str(&rest[..at]);
        out.push_str(a);
        rest = &rest[at + 2..];
    }
    out.push_str(rest);
    out
}

/// The client's whole message, for a `0xF7DC`.
///
/// `None` is a `0xF7DC` with **no body at all** and `Some("")` is one with an empty string; the
/// handler's guard is the unpack failing *or* the unpacked length being 1 (the terminator alone), so
/// both take [`BOOTED_DEFAULT_REASON`] and the two are indistinguishable on screen.
#[must_use]
pub fn account_booted_message(reason: Option<&str>) -> String {
    let reason = match reason {
        Some(r) if !r.is_empty() => r,
        _ => BOOTED_DEFAULT_REASON,
    };
    sprintf(BOOTED_FORMAT, &[reason])
}

/// The account-banned handler's expiry arithmetic: the real time — plain `time(NULL)` —
/// **rounded up to the next whole minute**, plus the message's own `expiry`.
///
/// So `expiry` is a **duration in seconds from now**, not an absolute instant — see the note on
/// [`account_banned_message`]. `i64::rem` matches C's `%` on signed values, so a pre-epoch `now`
/// rounds the same way the client's would.
#[must_use]
pub fn ban_expiry_epoch(expiry: i32, now_unix: i64) -> i64 {
    let mut t = now_unix;
    let rem = t % 60;
    if rem != 0 {
        t += 60 - rem;
    }
    t + i64::from(expiry)
}

/// MSVCR70's `asctime`, minus the trailing newline the handler strips itself.
///
/// This is a re-export of [`crate::ctime::asctime`] so that the one formatter has one home; see
/// that module's header for what the CRT actually does, and note that MSVC's day of the month is
/// **two digits, zero padded** (`Jan 01`), not a width-3 space pad.
///
/// The handler does `asctime(localtime(&t))` and then overwrites the character before the
/// terminator — that is the `'\n'` `asctime` appends. It is the **only** place the client uses
/// `asctime`; every other date the
/// client draws goes through `strftime("%c")`, which is [`crate::ctime::strftime_c`].
pub use crate::ctime::asctime;

/// The client's whole message, for a `0xF7C1`.
///
/// The handler has two arms and they share nothing but the reason:
///
/// * `expiry <= 0` — permanent. [`BANNED_FORMAT`], reason only, **no default substitution**.
/// * `expiry > 0` — timed. [`BANNED_UNTIL_FORMAT`], with [`ban_expiry_epoch`] rendered by
///   [`asctime`] and then the reason.
///
/// # Two things this signature says out loud
///
/// **`now_unix` is an input.** The handler reads the clock at the moment the message lands, so the
/// text is not a pure function of the wire and a test that wants an exact sentence has to supply
/// the instant. Passing it in is what makes the arithmetic testable at all.
///
/// **`utc_offset_secs` is what `localtime` would have added.**
/// `dereth_desktop::platform::local_utc_offset_secs` reads the machine's zone for the instant
/// being formatted and `HudView` passes it; a constant `0` would render the expiry in UTC. It
/// stays a parameter rather than a clock read so that a test
/// can pin a zone instead of inheriting the machine's.
///
/// **A disagreement worth recording**: `dereth_protocol::login::LoginAccountBanned`'s doc
/// and `serv_char`'s `ban_message` both call `expiry` an **absolute** instant, and the client adds
/// it to the current real time. One of the two ends is wrong; this function reproduces the
/// client, because the client is the authority for what the client shows.
#[must_use]
pub fn account_banned_message(
    expiry: i32,
    reason: &str,
    now_unix: i64,
    utc_offset_secs: i32,
) -> String {
    if expiry <= 0 {
        return sprintf(BANNED_FORMAT, &[reason]);
    }
    let at = ban_expiry_epoch(expiry, now_unix);
    sprintf(
        BANNED_UNTIL_FORMAT,
        &[&asctime(at, utc_offset_secs), reason],
    )
}

/// The UI-flow character-error notice's error-code-to-`StringInfo` switch.
///
/// The four codes with no token (0 `UNDEF`, 2 `LOGGED_ON`, 7 `NO_PREMADE`, 22 `CHARACTER_IS_BOOTED`)
/// fall into the `default:` arm, "which leaves the `StringInfo` untouched and **never queues a
/// mode**" — so `None` here means *do not show this screen at all*, not "show it blank".
///
/// Note code 8 `ACCOUNT_IN_USE` deliberately shares `ID_CHAR_ERROR_SERVER_CRASH` with code 4.
#[must_use]
pub fn character_error_string_id(code: u32) -> Option<&'static str> {
    Some(match code {
        1 => "ID_CHAR_ERROR_LOGON",
        3 => "ID_CHAR_ERROR_ACCOUNT_LOGON",
        4 | 8 => "ID_CHAR_ERROR_SERVER_CRASH",
        5 => "ID_CHAR_ERROR_LOGOFF",
        6 => "ID_CHAR_ERROR_DELETE",
        9 => "ID_CHAR_ERROR_ACCOUNT_INVALID",
        10 => "ID_CHAR_ERROR_ACCOUNT_DOESNT_EXIST",
        11 => "ID_CHAR_ERROR_ENTER_GAME_GENERIC",
        12 => "ID_CHAR_ERROR_ENTER_GAME_STRESS_ACCOUNT",
        13 => "ID_CHAR_ERROR_ENTER_GAME_CHARACTER_IN_WORLD",
        14 => "ID_CHAR_ERROR_ENTER_GAME_PLAYER_ACCOUNT_MISSING",
        15 => "ID_CHAR_ERROR_ENTER_GAME_CHARACTER_NOT_OWNED",
        16 => "ID_CHAR_ERROR_ENTER_GAME_CHARACTER_IN_WORLD_SERVER",
        17 => "ID_CHAR_ERROR_ENTER_GAME_OLD_CHARACTER",
        18 => "ID_CHAR_ERROR_ENTER_GAME_CORRUPT_CHARACTER",
        19 => "ID_CHAR_ERROR_ENTER_GAME_START_SERVER_DOWN",
        20 => "ID_CHAR_ERROR_ENTER_GAME_COULDNT_PLACE_CHARACTER",
        21 => "ID_CHAR_ERROR_LOGON_SERVER_FULL",
        23 => "ID_CHAR_ERROR_ENTER_GAME_CHARACTER_LOCKED",
        24 => "ID_CHAR_ERROR_SUBSCRIPTION_EXPIRED",
        _ => return None,
    })
}
