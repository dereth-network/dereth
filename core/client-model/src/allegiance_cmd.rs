//! `@allegiance`, `@motd`, `@alh`/`@ah` and `@ab` — the **only** producer in the retail client of
//! twenty-four allegiance client-to-server events.
//!
//! # There is no button for any of these, and that is measured, not assumed
//!
//! Every place the retail client can raise each allegiance event was accounted for. For each of the twenty-four below, the caller set contains exactly one
//! communication-system chat-command handler, and the allegiance UI appears in
//! none of them:
//!
//! The command handlers cover hometown and house travel, chat moderation, bans,
//! member information, officers and their titles, allegiance naming and locking,
//! message-of-the-day maintenance, and member booting. Together they select twenty-four
//! client-to-server requests; no allegiance-panel button emits any of them.
//!
//! The allegiance panel's only outbound events are update requests (`0x001F`, five sites),
//! swearing allegiance (`0x001D`, from the notice-dialog close path), and
//! breaking allegiance (`0x001E`, from both confirmation-close paths). So
//! **none** of the twenty-four needs a button arm, a context menu or a confirmation dialog: the
//! chat entry is the whole of their production path, and a build with no `@allegiance` handler
//! cannot send any of them at all. The command table ([`crate::cmd::table`]) has four
//! corresponding command entries.
//!
//! **The capture corpus is not an oracle here.** The three recorded fellowship sessions carry
//! 2,920 client game actions and, of the twenty-seven allegiance client-to-server opcodes, only
//! `0x001D` (7), `0x001E` (5) and `0x001F` (31) — the three the panel sends. Nobody typed an
//! `@allegiance` sub-command while the proxy was recording. The oracle for the bodies is
//! therefore [`dereth_protocol::social`]'s writers (round-tripped, with each request
//! checked against the event's encoded bytes), while parsing follows the client's
//! command handlers above.
//!
//! # The three shared argument helpers
//!
//! - The token helper pops one token and decrements `argc`.
//! - The join helper re-joins the rest with a **single** space — original spacing is lost.
//! - The name helper joins the arguments, then applies `trim(both, whitespace)` and
//!   `trim(leading, "+")` — the `+` that marks the player's own characters in a name list.
//!
//! Every sub-command comparison requests case-insensitive matching. Equality takes the matching
//! branch; a mismatch falls through to the next literal. The comparison is true only for
//! equality, so the chain continues only when the current literal did not match.

use dereth_protocol::social as s;

use crate::world::World;
use crate::{Request, RequestSink};

// ---------------------------------------------------------------------------------------------
// The command literals, taken directly from the client
// ---------------------------------------------------------------------------------------------

/// The missing-name refusal shared by the gag, ungag, ban, info and boot commands.
pub const PLEASE_SPECIFY_AN_ACTUAL_NAME: &str = "Please specify an actual name.";
/// The officer command's missing-member refusal.
pub const PLEASE_SPECIFY_AN_ALLEGIANCE_MEMBER: &str =
    "Please specify the name of an allegiance member.";
/// The officer command's invalid-level refusal, including the help-files sentence.
pub const PLEASE_SPECIFY_A_VALID_OFFICER_LEVEL_LONG: &str =
    "Please specify a valid officer level as a number between 1 and 3. Check the game help files \
     for more information on officer levels.";
/// The officer-title command's shorter invalid-level refusal, which stops at the full stop.
pub const PLEASE_SPECIFY_A_VALID_OFFICER_LEVEL: &str =
    "Please specify a valid officer level as a number between 1 and 3.";
/// The hometown command's only refusal.
pub const THIS_COMMAND_TAKES_NO_ARGUMENTS: &str = "This command takes no arguments!";
/// The command tail printed for an unknown sub-command **and** for a sub-handler that returned
/// `false`.
pub const PLEASE_SEE_HELP_ALLEGIANCE: &str =
    "Please see @help Allegiance for more information on how to use this command.";
/// The chat-kick command's default reason when the line carries no comma.
pub const NO_REASON_GIVEN: &str = "No reason given.";
/// The boot acknowledgement. The optional account suffix is selected before the line is formatted
/// and printed on chat type 0.
pub const ATTEMPTING_TO_BOOT: &str = "Attempting to boot {name}{account}...\n";
/// The suffix appended when the `-account` flag was present.
pub const ACCOUNT_SUFFIX: &str = " (Account)";
/// The `-account` flag the boot command looks for inside the joined name.
pub const ACCOUNT_FLAG: &str = "-account";

/// The chat type these handlers print refusals with (`add_text_to_scroll(text, 0x1A, true, …)`) — the refusal type
/// every one of these handlers uses, and the one no window's default filter accepts.
pub const REFUSAL_CHAT_TYPE: u32 = 0x1A;
/// [`World::do_allegiance_boot`]'s acknowledgement goes out on type **0**, not `0x1A`.
pub const BOOT_CHAT_TYPE: u32 = 0;

/// The preference update hands off to the player-option change handler with option ordinal
/// **27**, `HearAllegianceChat`.
pub const HEAR_ALLEGIANCE_CHAT_ORDINAL: usize = 27;

/// `Request::ChannelBroadcast(0x2000000, text)` — the Allegiance Broadcast channel,
/// the same bit `dereth_client::chat::channel_broadcast_line` prints `[Allegiance Broadcast]` for.
pub const ALLEGIANCE_BROADCAST_CHANNEL: u32 = 0x0200_0000;

/// `AllegianceLockAction`, the `Request::AllegianceDoLockAction` argument. The numbers are
/// the client's own constants and ACE's `AllegianceLockAction.cs` agrees on
/// every one.
pub mod lock_action {
    /// `@allegiance lock off`.
    pub const OFF: u32 = 1;
    /// `@allegiance lock on`.
    pub const ON: u32 = 2;
    /// `@allegiance lock toggle`.
    pub const TOGGLE: u32 = 3;
    /// `@allegiance lock`, and `@allegiance lock check`.
    pub const CHECK: u32 = 4;
    /// `@allegiance lock bypass` with no name — ACE calls it `CheckApproved`.
    pub const CHECK_APPROVED: u32 = 5;
    /// `@allegiance lock bypass clear` — ACE's `ClearApproved`.
    pub const CLEAR_APPROVED: u32 = 6;
}

/// `AllegianceHouseAction`, the `Request::AllegianceDoHouseAction` argument.
/// The client's constants; ACE's `AllegianceHouseAction.cs` agrees.
pub mod house_action {
    /// `@allegiance house` with fewer than two arguments — ACE calls it `Help`/`CheckStatus`.
    pub const HELP: u32 = 1;
    /// `@allegiance house guest open`.
    pub const GUEST_OPEN: u32 = 2;
    /// `@allegiance house guest close`.
    pub const GUEST_CLOSE: u32 = 3;
    /// `@allegiance house storage open`.
    pub const STORAGE_OPEN: u32 = 4;
    /// `@allegiance house storage close`.
    pub const STORAGE_CLOSE: u32 = 5;
}

// ---------------------------------------------------------------------------------------------
// What a handler decided
// ---------------------------------------------------------------------------------------------

/// One handler's whole visible effect, apart from the requests it has already put in the sink.
///
/// `handled` is the C++ `bool` the handler returns, and it is load-bearing twice:
/// the dispatcher prints [`PLEASE_SEE_HELP_ALLEGIANCE`] when a sub-handler answers
/// `false`, and raises failure event `0x26` — *"That is not a valid
/// command."* — when a **table** entry's handler does.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AllegianceCommand {
    /// `add_text_to_scroll(text, ty, true, …)` lines, in the order retail prints them. The window
    /// is the caller's because every one of these sites prints to the window the command came
    /// from.
    pub lines: Vec<(String, u32)>,
    /// The handler's own return value.
    pub handled: bool,
    /// How many allegiance events went into the [`RequestSink`]. Counted rather than
    /// inferred, because several arms print *and* send and several print *instead of* sending.
    pub sent: u32,
    /// Behavior: `@allegiance chat on|off` and nothing
    /// else. The caller applies it, because `PlayerSystem::set_option` needs the clock and this
    /// module does not have one.
    pub hear_allegiance_chat: Option<bool>,
}

impl AllegianceCommand {
    /// A handler that returned `false`. Its caller decides what that means.
    fn refused() -> Self {
        Self::default()
    }

    /// A scroll print on `0x1A` at the source window and `return true` — the shape all six of these
    /// handlers' refusals take.
    fn say(text: impl Into<String>, ty: u32) -> Self {
        Self {
            lines: vec![(text.into(), ty)],
            handled: true,
            ..Self::default()
        }
    }

    fn sent() -> Self {
        Self {
            handled: true,
            sent: 1,
            ..Self::default()
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The argument helpers
// ---------------------------------------------------------------------------------------------

/// Behavior: pop one token, or the empty string when `argc` is already 0.
fn next_arg(args: &[String]) -> (&str, &[String]) {
    match args.split_first() {
        Some((first, rest)) => (first.as_str(), rest),
        None => ("", &[]),
    }
}

/// Re-joins the arguments with single spaces.
fn join_args(args: &[String]) -> String {
    args.join(" ")
}

/// `strtol(s, 0, 0)` — base 0, so `0x2` is 2 and a trailing non-digit stops the scan rather than
/// failing it. Retail hands the result straight to a `1 <= n <= 3` test, and a string with no
/// digits at all gives 0, which that test rejects.
fn strtol(s: &str) -> i64 {
    let t = s.trim_start();
    let (neg, t) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let (radix, digits) = if let Some(r) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        (16, r)
    } else if t.starts_with('0') && t.len() > 1 {
        (8, &t[1..])
    } else {
        (10, t)
    };
    let mut acc: i64 = 0;
    for c in digits.chars() {
        match c.to_digit(radix) {
            Some(d) => {
                acc = acc
                    .saturating_mul(i64::from(radix))
                    .saturating_add(i64::from(d))
            }
            None => break,
        }
    }
    if neg {
        -acc
    } else {
        acc
    }
}

/// The client's `-account` excision: find the first case-insensitive
/// occurrence of `-account` inside the joined name, remove it, then
/// trim whitespace at both ends.
///
/// Retail's replacement scan receives the stored string length, which counts the terminator, so
/// the replaced run may be one character longer than the flag. For every line of the documented
/// form (`-account Bob`, `Bob -account`), the trailing whitespace trim makes the two identical,
/// which is why this takes the eight characters and says so rather than guessing the off-by-one.
#[must_use]
pub fn strip_account_flag(joined: &str) -> (String, bool) {
    let lower = joined.to_ascii_lowercase();
    match lower.find(ACCOUNT_FLAG) {
        Some(at) => {
            let mut out = String::with_capacity(joined.len());
            out.push_str(&joined[..at]);
            out.push_str(&joined[at + ACCOUNT_FLAG.len()..]);
            (out.trim().to_owned(), true)
        }
        None => (joined.trim().to_owned(), false),
    }
}

// ---------------------------------------------------------------------------------------------
// The handlers
// ---------------------------------------------------------------------------------------------

impl World {
    /// Handle `@allegiance` and `@all`.
    ///
    /// Fifteen literal comparisons in one chain, in this order, then the help hint. Two pairs are
    /// aliases (`chat`/`ch`, `broadcast`/`br`) and `hometown`/`ho` is handled **inline** by both
    /// matching branches rather than through
    /// the `@allegiance hometown` handler, which is why `@allegiance hometown extra junk` sends and
    /// `@ah extra junk` refuses.
    ///
    /// **It always returns `true`**, so an unknown sub-command never reaches the command
    /// dispatcher's failure event `0x26`; it gets [`PLEASE_SEE_HELP_ALLEGIANCE`] instead.
    pub fn do_allegiance(
        &mut self,
        req: &mut dyn RequestSink,
        args: &[String],
    ) -> AllegianceCommand {
        let (sub, rest) = next_arg(args);
        let eq = |lit: &str| sub.eq_ignore_ascii_case(lit);
        let mut out = if eq("boot") {
            self.do_allegiance_boot(req, rest)
        } else if eq("info") {
            self.do_allegiance_info(req, rest)
        } else if eq("chat") || eq("ch") {
            self.do_allegiance_chat(req, rest)
        } else if eq("broadcast") || eq("br") {
            self.do_allegiance_broadcast(req, rest)
        } else if eq("ban") {
            self.do_allegiance_ban(req, rest)
        } else if eq("officer") {
            self.do_allegiance_officer(req, rest)
        } else if eq("title") {
            self.do_allegiance_officer_title(req, rest)
        } else if eq("hometown") || eq("ho") {
            // The `hometown` and `ho` aliases send the hometown-recall event directly.
            req.send(Request::AllegianceRecallHometown(
                s::AllegianceRecallAllegianceHometown,
            ));
            AllegianceCommand::sent()
        } else if eq("motd") {
            self.do_motd(req, rest)
        } else if eq("name") {
            // The literal here is `"name"`; it selects the allegiance-name handler.
            self.do_allegiance_name(req, rest)
        } else if eq("lock") {
            self.do_allegiance_lock(req, rest)
        } else if eq("house") {
            self.do_allegiance_house(req, rest)
        } else {
            AllegianceCommand::refused()
        };
        if !out.handled {
            out.lines
                .push((PLEASE_SEE_HELP_ALLEGIANCE.to_owned(), REFUSAL_CHAT_TYPE));
            out.handled = true;
        }
        out
    }

    /// Behavior: the `@alh` / `@ah` table
    /// entries. Any argument refuses; otherwise `Request::AllegianceRecallHometown` (`0x02AB`).
    pub fn do_allegiance_hometown(
        &mut self,
        req: &mut dyn RequestSink,
        args: &[String],
    ) -> AllegianceCommand {
        if !args.is_empty() {
            return AllegianceCommand::say(THIS_COMMAND_TAKES_NO_ARGUMENTS, REFUSAL_CHAT_TYPE);
        }
        req.send(Request::AllegianceRecallHometown(
            s::AllegianceRecallAllegianceHometown,
        ));
        AllegianceCommand::sent()
    }

    /// Handle allegiance broadcast commands `@ab` and
    /// `@allegiance broadcast`. Not one of the twenty-four: it is
    /// channel-broadcast event `0x0147` with channel mask `0x2000000`, on the
    /// Allegiance Broadcast channel. Empty text returns **false**.
    pub fn do_allegiance_broadcast(
        &mut self,
        req: &mut dyn RequestSink,
        args: &[String],
    ) -> AllegianceCommand {
        let text = join_args(args);
        if text.is_empty() {
            return AllegianceCommand::refused();
        }
        req.send(Request::ChannelBroadcast(
            dereth_protocol::comms::CommunicationChannelBroadcast {
                channel: ALLEGIANCE_BROADCAST_CHANNEL,
                message: text,
            },
        ));
        AllegianceCommand::sent()
    }

    /// Handle allegiance boot command `0x0277`.
    ///
    /// ```text
    ///   join and normalize the remaining arguments as a name
    ///   test the stored length           ; one terminator means empty
    ///   literal                          ; L"Please specify an actual name."
    ///   print the refusal on chat type 0x1A for the current source
    ///   return true
    ///   flag literal                     ; "-account"
    ///   find the flag case-insensitively within the name
    ///   remove the matching substring
    ///   trim whitespace at both ends
    ///   choose " (Account)" when the flag was present, otherwise the empty string
    ///   format                           ; "Attempting to boot %s%s...\n"
    ///   print the acknowledgement on chat type 0 for the current source
    ///   send the break-allegiance boot event with name and account
    /// ```
    ///
    /// The acknowledgement is printed **before** the send and on chat type **0**, which is the one
    /// asymmetry in this family — every other line these handlers print is a `0x1A` refusal.
    pub fn do_allegiance_boot(
        &mut self,
        req: &mut dyn RequestSink,
        args: &[String],
    ) -> AllegianceCommand {
        let name = crate::friends::join_args_as_name(args);
        if name.is_empty() {
            return AllegianceCommand::say(PLEASE_SPECIFY_AN_ACTUAL_NAME, REFUSAL_CHAT_TYPE);
        }
        let (name, account) = strip_account_flag(&name);
        let line = format!(
            "Attempting to boot {name}{}...\n",
            if account { ACCOUNT_SUFFIX } else { "" }
        );
        req.send(Request::AllegianceBreakAllegianceBoot(
            s::AllegianceBreakAllegianceBoot {
                name,
                account_boot: i32::from(account),
            },
        ));
        AllegianceCommand {
            lines: vec![(line, BOOT_CHAT_TYPE)],
            handled: true,
            sent: 1,
            hear_allegiance_chat: None,
        }
    }

    /// Handle allegiance-info command `0x027B` with one name.
    pub fn do_allegiance_info(
        &mut self,
        req: &mut dyn RequestSink,
        args: &[String],
    ) -> AllegianceCommand {
        let name = crate::friends::join_args_as_name(args);
        if name.is_empty() {
            return AllegianceCommand::say(PLEASE_SPECIFY_AN_ACTUAL_NAME, REFUSAL_CHAT_TYPE);
        }
        req.send(Request::AllegianceInfoRequest(s::AllegianceInfoRequest {
            name,
        }));
        AllegianceCommand::sent()
    }

    /// Handle allegiance-chat commands `0x0041` and `0x02A0`, plus the
    /// local `HearAllegianceChat` toggle.
    ///
    /// | sub-command | effect |
    /// |---|---|
    /// | `on` / `off` | set the hear-allegiance-chat option; no packet of its own |
    /// | `kick <name>[, <reason>]` | send a chat-boot request — `0x02A0` |
    /// | `gag <name>` | send a chat-gag request with state 1 — `0x0041` |
    /// | `ungag <name>` | send a chat-gag request with state 0 |
    /// | anything else | `false` |
    ///
    /// Three deliberate transcriptions of retail's own inconsistency:
    ///
    /// 1. `gag`/`ungag` use the plain join helper, **not** the name helper — no trim or `+` strip.
    /// 2. `kick` has **no empty-name check at all**, so `@allegiance chat kick` on its own puts an
    ///    empty name on the wire. Both `gag` arms do check.
    /// 3. The reason is split at the **first comma**; the name keeps whatever is before it
    ///    untrimmed; only the reason has whitespace trimmed from its
    ///    buffer, while the name does not.
    pub fn do_allegiance_chat(
        &mut self,
        req: &mut dyn RequestSink,
        args: &[String],
    ) -> AllegianceCommand {
        let (sub, rest) = next_arg(args);
        if sub.eq_ignore_ascii_case("on") || sub.eq_ignore_ascii_case("off") {
            let on = sub.eq_ignore_ascii_case("on");
            return AllegianceCommand {
                handled: true,
                hear_allegiance_chat: Some(on),
                ..AllegianceCommand::default()
            };
        }
        if sub.eq_ignore_ascii_case("kick") {
            let joined = join_args(rest);
            let (name, reason) = match joined.find(',') {
                Some(at) => (joined[..at].to_owned(), joined[at + 1..].trim().to_owned()),
                None => (joined, NO_REASON_GIVEN.to_owned()),
            };
            req.send(Request::AllegianceChatBoot(s::AllegianceChatBoot {
                name,
                reason,
            }));
            return AllegianceCommand::sent();
        }
        let gag = if sub.eq_ignore_ascii_case("gag") {
            1
        } else if sub.eq_ignore_ascii_case("ungag") {
            0
        } else {
            return AllegianceCommand::refused();
        };
        let name = join_args(rest);
        if name.is_empty() {
            return AllegianceCommand::say(PLEASE_SPECIFY_AN_ACTUAL_NAME, REFUSAL_CHAT_TYPE);
        }
        req.send(Request::AllegianceChatGag(s::AllegianceChatGag {
            name,
            gagged: gag,
        }));
        AllegianceCommand::sent()
    }

    /// Handle allegiance-ban commands `0x02A1`, `0x02A2`, and `0x02A3`.
    ///
    /// `list` is tested **before** the name is joined, so `@allegiance ban list` never reaches the
    /// empty-name refusal; every other sub-command needs a name first and only then is `add` /
    /// `remove` decided. An unknown sub-command with a name returns `false`.
    pub fn do_allegiance_ban(
        &mut self,
        req: &mut dyn RequestSink,
        args: &[String],
    ) -> AllegianceCommand {
        let (sub, rest) = next_arg(args);
        if sub.eq_ignore_ascii_case("list") {
            req.send(Request::AllegianceListBans(s::AllegianceListAllegianceBans));
            return AllegianceCommand::sent();
        }
        let name = crate::friends::join_args_as_name(rest);
        if name.is_empty() {
            return AllegianceCommand::say(PLEASE_SPECIFY_AN_ACTUAL_NAME, REFUSAL_CHAT_TYPE);
        }
        if sub.eq_ignore_ascii_case("add") {
            req.send(Request::AllegianceAddBan(s::AllegianceAddAllegianceBan {
                name,
            }));
            return AllegianceCommand::sent();
        }
        if sub.eq_ignore_ascii_case("remove") {
            req.send(Request::AllegianceRemoveBan(
                s::AllegianceRemoveAllegianceBan { name },
            ));
            return AllegianceCommand::sent();
        }
        AllegianceCommand::refused()
    }

    /// Handle allegiance-officer commands `0x003B`, `0x02A5`, `0x02A6`,
    /// `0x02A7`.
    ///
    /// `argc == 0` lists, and so does `list`. `set` and `add` are the same arm: either literal sets
    /// the "set" flag, then reads a level with
    /// `strtol(arg, 0, 0)` and refuses anything outside `1..=3` with the long sentence.
    pub fn do_allegiance_officer(
        &mut self,
        req: &mut dyn RequestSink,
        args: &[String],
    ) -> AllegianceCommand {
        if args.is_empty() {
            req.send(Request::AllegianceListOfficers(
                s::AllegianceListAllegianceOfficers,
            ));
            return AllegianceCommand::sent();
        }
        let (sub, rest) = next_arg(args);
        if sub.eq_ignore_ascii_case("list") {
            req.send(Request::AllegianceListOfficers(
                s::AllegianceListAllegianceOfficers,
            ));
            return AllegianceCommand::sent();
        }
        if sub.eq_ignore_ascii_case("clear") {
            req.send(Request::AllegianceClearOfficers(
                s::AllegianceClearAllegianceOfficers,
            ));
            return AllegianceCommand::sent();
        }
        if sub.eq_ignore_ascii_case("remove") {
            let name = crate::friends::join_args_as_name(rest);
            if name.is_empty() {
                return AllegianceCommand::say(
                    PLEASE_SPECIFY_AN_ALLEGIANCE_MEMBER,
                    REFUSAL_CHAT_TYPE,
                );
            }
            req.send(Request::AllegianceRemoveOfficer(
                s::AllegianceRemoveAllegianceOfficer { name },
            ));
            return AllegianceCommand::sent();
        }
        if !(sub.eq_ignore_ascii_case("set") || sub.eq_ignore_ascii_case("add")) {
            return AllegianceCommand::refused();
        }
        let (level_word, rest) = next_arg(rest);
        let level = strtol(level_word);
        if !(1..=3).contains(&level) {
            return AllegianceCommand::say(
                PLEASE_SPECIFY_A_VALID_OFFICER_LEVEL_LONG,
                REFUSAL_CHAT_TYPE,
            );
        }
        let name = crate::friends::join_args_as_name(rest);
        if name.is_empty() {
            return AllegianceCommand::say(PLEASE_SPECIFY_AN_ALLEGIANCE_MEMBER, REFUSAL_CHAT_TYPE);
        }
        req.send(Request::AllegianceSetOfficer(
            s::AllegianceSetAllegianceOfficer {
                name,
                level: u32::try_from(level).unwrap_or(0),
            },
        ));
        AllegianceCommand::sent()
    }

    /// Handle allegiance-officer-title commands `0x003C`, `0x003D`,
    /// `0x003E`.
    ///
    /// `set` takes the level **first** and the title after it, which is also the wire order:
    /// The request writer stores opcode `0x3C`, then the
    /// level dword and packs the string after it. There is **no empty-title check**,
    /// so `@allegiance title set 2` sends an empty title — which is how a title is cleared one
    /// level at a time.
    pub fn do_allegiance_officer_title(
        &mut self,
        req: &mut dyn RequestSink,
        args: &[String],
    ) -> AllegianceCommand {
        if args.is_empty() {
            req.send(Request::AllegianceListOfficerTitles(
                s::AllegianceListAllegianceOfficerTitles,
            ));
            return AllegianceCommand::sent();
        }
        let (sub, rest) = next_arg(args);
        if sub.eq_ignore_ascii_case("list") {
            req.send(Request::AllegianceListOfficerTitles(
                s::AllegianceListAllegianceOfficerTitles,
            ));
            return AllegianceCommand::sent();
        }
        if sub.eq_ignore_ascii_case("clear") {
            req.send(Request::AllegianceClearOfficerTitles(
                s::AllegianceClearAllegianceOfficerTitles,
            ));
            return AllegianceCommand::sent();
        }
        if !sub.eq_ignore_ascii_case("set") {
            return AllegianceCommand::refused();
        }
        let (level_word, rest) = next_arg(rest);
        let level = strtol(level_word);
        if !(1..=3).contains(&level) {
            return AllegianceCommand::say(PLEASE_SPECIFY_A_VALID_OFFICER_LEVEL, REFUSAL_CHAT_TYPE);
        }
        req.send(Request::AllegianceSetOfficerTitle(
            s::AllegianceSetAllegianceOfficerTitle {
                level: u32::try_from(level).unwrap_or(0),
                title: crate::friends::join_args_as_name(rest),
            },
        ));
        AllegianceCommand::sent()
    }

    /// Handle allegiance-name commands `0x0030`, `0x0031`, and `0x0033`.
    ///
    /// No arguments query the current name. `set` joins the rest and trims whitespace at both ends
    /// using the plain join path — **not**
    /// the name-normalization helper — so a leading `+` survives into an allegiance name.
    pub fn do_allegiance_name(
        &mut self,
        req: &mut dyn RequestSink,
        args: &[String],
    ) -> AllegianceCommand {
        if args.is_empty() {
            req.send(Request::AllegianceQueryName(
                s::AllegianceQueryAllegianceName,
            ));
            return AllegianceCommand::sent();
        }
        let (sub, rest) = next_arg(args);
        if sub.eq_ignore_ascii_case("set") {
            req.send(Request::AllegianceSetName(s::AllegianceSetAllegianceName {
                name: join_args(rest).trim().to_owned(),
            }));
            return AllegianceCommand::sent();
        }
        if sub.eq_ignore_ascii_case("clear") {
            req.send(Request::AllegianceClearName(
                s::AllegianceClearAllegianceName,
            ));
            return AllegianceCommand::sent();
        }
        AllegianceCommand::refused()
    }

    /// Handle allegiance-lock commands `0x003F` and `0x0040`.
    ///
    /// `argc == 0` is the same as `check`. `bypass` with no name asks
    /// ([`lock_action::CHECK_APPROVED`]), `bypass clear` clears
    /// ([`lock_action::CLEAR_APPROVED`], compared with a bare `_stricmp` rather than the usual
    /// string wrapper), and any other name is
    /// `Request::AllegianceSetApprovedVassal(name)`.
    pub fn do_allegiance_lock(
        &mut self,
        req: &mut dyn RequestSink,
        args: &[String],
    ) -> AllegianceCommand {
        let lock = |req: &mut dyn RequestSink, action: u32| {
            req.send(Request::AllegianceDoLockAction(
                s::AllegianceDoAllegianceLockAction { action },
            ));
            AllegianceCommand::sent()
        };
        if args.is_empty() {
            return lock(req, lock_action::CHECK);
        }
        let (sub, rest) = next_arg(args);
        if sub.eq_ignore_ascii_case("off") {
            return lock(req, lock_action::OFF);
        }
        if sub.eq_ignore_ascii_case("on") {
            return lock(req, lock_action::ON);
        }
        if sub.eq_ignore_ascii_case("toggle") {
            return lock(req, lock_action::TOGGLE);
        }
        if sub.eq_ignore_ascii_case("check") {
            return lock(req, lock_action::CHECK);
        }
        if !sub.eq_ignore_ascii_case("bypass") {
            return AllegianceCommand::refused();
        }
        let name = crate::friends::join_args_as_name(rest);
        if name.is_empty() {
            return lock(req, lock_action::CHECK_APPROVED);
        }
        if name.eq_ignore_ascii_case("clear") {
            return lock(req, lock_action::CLEAR_APPROVED);
        }
        req.send(Request::AllegianceSetApprovedVassal(
            s::AllegianceSetAllegianceApprovedVassal { name },
        ));
        AllegianceCommand::sent()
    }

    /// Handle allegiance-house command `0x0042` with one dword.
    ///
    /// `if (argc < 2)` is [`house_action::HELP`], so `@allegiance house guest` on its own — one
    /// argument — asks for the status rather than refusing. Two arguments then pick one of the
    /// four `guest`/`storage` × `open`/`close` actions, and anything else returns `false`.
    pub fn do_allegiance_house(
        &mut self,
        req: &mut dyn RequestSink,
        args: &[String],
    ) -> AllegianceCommand {
        let house = |req: &mut dyn RequestSink, action: u32| {
            req.send(Request::AllegianceDoHouseAction(
                s::AllegianceDoAllegianceHouseAction { action },
            ));
            AllegianceCommand::sent()
        };
        if args.len() < 2 {
            return house(req, house_action::HELP);
        }
        let (what, rest) = next_arg(args);
        let (how, _) = next_arg(rest);
        let action = if what.eq_ignore_ascii_case("guest") {
            if how.eq_ignore_ascii_case("open") {
                house_action::GUEST_OPEN
            } else if how.eq_ignore_ascii_case("close") {
                house_action::GUEST_CLOSE
            } else {
                return AllegianceCommand::refused();
            }
        } else if what.eq_ignore_ascii_case("storage") {
            if how.eq_ignore_ascii_case("open") {
                house_action::STORAGE_OPEN
            } else if how.eq_ignore_ascii_case("close") {
                house_action::STORAGE_CLOSE
            } else {
                return AllegianceCommand::refused();
            }
        } else {
            return AllegianceCommand::refused();
        };
        house(req, action)
    }

    /// Handle message-of-the-day commands `0x0254`, `0x0255`, and `0x0256`. Reached from
    /// the `@motd` table entry **and** from `@allegiance motd`.
    ///
    /// `set` uses a bare re-join (`join_args`) with **no trim** — the one place in this family
    /// where the text is sent exactly as re-joined — so `@motd set   hello` sends `"  hello"` after
    /// the first space is eaten by the tokeniser.
    pub fn do_motd(&mut self, req: &mut dyn RequestSink, args: &[String]) -> AllegianceCommand {
        if args.is_empty() {
            req.send(Request::AllegianceQueryMotd(s::AllegianceQueryMotd));
            return AllegianceCommand::sent();
        }
        let (sub, rest) = next_arg(args);
        if sub.eq_ignore_ascii_case("set") {
            req.send(Request::AllegianceSetMotd(s::AllegianceSetMotd {
                motd: join_args(rest),
            }));
            return AllegianceCommand::sent();
        }
        if sub.eq_ignore_ascii_case("clear") {
            req.send(Request::AllegianceClearMotd(s::AllegianceClearMotd));
            return AllegianceCommand::sent();
        }
        AllegianceCommand::refused()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RecordingRequests;

    fn argv(line: &str) -> Vec<String> {
        line.split_whitespace().map(ToOwned::to_owned).collect()
    }

    fn run(line: &str) -> (AllegianceCommand, Vec<Request>) {
        let mut w = World::new();
        let mut req = RecordingRequests::default();
        let out = w.do_allegiance(&mut req, &argv(line));
        (out, req.0)
    }

    /// Oracle: `strtol(s, 0, 0)`, base 0. The officer arms hand it straight to `1 <= n <= 3`.
    #[test]
    fn the_officer_level_is_read_with_base_zero_strtol() {
        assert_eq!(strtol("2"), 2);
        assert_eq!(strtol("0x3"), 3);
        assert_eq!(
            strtol("03"),
            3,
            "a leading zero is octal, and 03 is 3 either way"
        );
        assert_eq!(
            strtol("2x"),
            2,
            "the scan stops at the first non-digit rather than failing"
        );
        assert_eq!(
            strtol("bob"),
            0,
            "no digits at all is 0, which the 1..=3 test rejects"
        );
        assert_eq!(strtol(""), 0);
        assert_eq!(
            strtol("-1"),
            -1,
            "and the test is signed, so -1 is refused rather than huge"
        );
    }

    /// The account flag is found case-insensitively as a substring, removed, and only
    /// then is the remaining name trimmed.
    #[test]
    fn the_account_flag_is_a_substring_and_is_case_insensitive() {
        assert_eq!(strip_account_flag("-account Bob"), ("Bob".into(), true));
        assert_eq!(strip_account_flag("Bob -account"), ("Bob".into(), true));
        assert_eq!(strip_account_flag("Bob -ACCOUNT"), ("Bob".into(), true));
        assert_eq!(strip_account_flag("Bob"), ("Bob".into(), false));
    }

    /// Oracle: the client's tail — an unknown sub-command, and a **bare**
    /// `@allegiance`, print the help hint and send nothing.
    #[test]
    fn an_unknown_sub_command_prints_the_hint_and_sends_nothing() {
        for line in ["", "wibble", "  "] {
            let (out, sent) = run(line);
            assert!(
                out.handled,
                "{line:?}: the @allegiance handler always returns true"
            );
            assert_eq!(
                out.lines,
                vec![(PLEASE_SEE_HELP_ALLEGIANCE.to_owned(), REFUSAL_CHAT_TYPE)],
                "{line:?}"
            );
            assert!(sent.is_empty(), "{line:?}");
        }
    }

    /// Every one of the twenty-four, from the sub-command a player types to the typed request.
    /// The table is the acceptance: a missing arm shows up as a hint line instead of a request.
    #[test]
    fn every_sub_command_reaches_its_own_request() {
        let cases: Vec<(&str, Request)> = vec![
            (
                "name",
                Request::AllegianceQueryName(s::AllegianceQueryAllegianceName),
            ),
            (
                "name set The Hand",
                Request::AllegianceSetName(s::AllegianceSetAllegianceName {
                    name: "The Hand".into(),
                }),
            ),
            (
                "name clear",
                Request::AllegianceClearName(s::AllegianceClearAllegianceName),
            ),
            ("motd", Request::AllegianceQueryMotd(s::AllegianceQueryMotd)),
            (
                "motd set be excellent",
                Request::AllegianceSetMotd(s::AllegianceSetMotd {
                    motd: "be excellent".into(),
                }),
            ),
            (
                "motd clear",
                Request::AllegianceClearMotd(s::AllegianceClearMotd),
            ),
            (
                "officer",
                Request::AllegianceListOfficers(s::AllegianceListAllegianceOfficers),
            ),
            (
                "officer list",
                Request::AllegianceListOfficers(s::AllegianceListAllegianceOfficers),
            ),
            (
                "officer clear",
                Request::AllegianceClearOfficers(s::AllegianceClearAllegianceOfficers),
            ),
            (
                "officer add 2 Bob",
                Request::AllegianceSetOfficer(s::AllegianceSetAllegianceOfficer {
                    name: "Bob".into(),
                    level: 2,
                }),
            ),
            (
                "officer set 3 Bob",
                Request::AllegianceSetOfficer(s::AllegianceSetAllegianceOfficer {
                    name: "Bob".into(),
                    level: 3,
                }),
            ),
            (
                "officer remove Bob",
                Request::AllegianceRemoveOfficer(s::AllegianceRemoveAllegianceOfficer {
                    name: "Bob".into(),
                }),
            ),
            (
                "title",
                Request::AllegianceListOfficerTitles(s::AllegianceListAllegianceOfficerTitles),
            ),
            (
                "title list",
                Request::AllegianceListOfficerTitles(s::AllegianceListAllegianceOfficerTitles),
            ),
            (
                "title clear",
                Request::AllegianceClearOfficerTitles(s::AllegianceClearAllegianceOfficerTitles),
            ),
            (
                "title set 1 Speaker",
                Request::AllegianceSetOfficerTitle(s::AllegianceSetAllegianceOfficerTitle {
                    level: 1,
                    title: "Speaker".into(),
                }),
            ),
            (
                "ban list",
                Request::AllegianceListBans(s::AllegianceListAllegianceBans),
            ),
            (
                "ban add Bob",
                Request::AllegianceAddBan(s::AllegianceAddAllegianceBan { name: "Bob".into() }),
            ),
            (
                "ban remove Bob",
                Request::AllegianceRemoveBan(s::AllegianceRemoveAllegianceBan {
                    name: "Bob".into(),
                }),
            ),
            (
                "lock",
                Request::AllegianceDoLockAction(s::AllegianceDoAllegianceLockAction {
                    action: lock_action::CHECK,
                }),
            ),
            (
                "lock on",
                Request::AllegianceDoLockAction(s::AllegianceDoAllegianceLockAction {
                    action: lock_action::ON,
                }),
            ),
            (
                "lock off",
                Request::AllegianceDoLockAction(s::AllegianceDoAllegianceLockAction {
                    action: lock_action::OFF,
                }),
            ),
            (
                "lock toggle",
                Request::AllegianceDoLockAction(s::AllegianceDoAllegianceLockAction {
                    action: lock_action::TOGGLE,
                }),
            ),
            (
                "lock bypass",
                Request::AllegianceDoLockAction(s::AllegianceDoAllegianceLockAction {
                    action: lock_action::CHECK_APPROVED,
                }),
            ),
            (
                "lock bypass clear",
                Request::AllegianceDoLockAction(s::AllegianceDoAllegianceLockAction {
                    action: lock_action::CLEAR_APPROVED,
                }),
            ),
            (
                "lock bypass Bob",
                Request::AllegianceSetApprovedVassal(s::AllegianceSetAllegianceApprovedVassal {
                    name: "Bob".into(),
                }),
            ),
            (
                "house",
                Request::AllegianceDoHouseAction(s::AllegianceDoAllegianceHouseAction {
                    action: house_action::HELP,
                }),
            ),
            (
                "house guest open",
                Request::AllegianceDoHouseAction(s::AllegianceDoAllegianceHouseAction {
                    action: house_action::GUEST_OPEN,
                }),
            ),
            (
                "house guest close",
                Request::AllegianceDoHouseAction(s::AllegianceDoAllegianceHouseAction {
                    action: house_action::GUEST_CLOSE,
                }),
            ),
            (
                "house storage open",
                Request::AllegianceDoHouseAction(s::AllegianceDoAllegianceHouseAction {
                    action: house_action::STORAGE_OPEN,
                }),
            ),
            (
                "house storage close",
                Request::AllegianceDoHouseAction(s::AllegianceDoAllegianceHouseAction {
                    action: house_action::STORAGE_CLOSE,
                }),
            ),
            (
                "info Bob",
                Request::AllegianceInfoRequest(s::AllegianceInfoRequest { name: "Bob".into() }),
            ),
            (
                "hometown",
                Request::AllegianceRecallHometown(s::AllegianceRecallAllegianceHometown),
            ),
            (
                "ho",
                Request::AllegianceRecallHometown(s::AllegianceRecallAllegianceHometown),
            ),
            (
                "chat gag Bob",
                Request::AllegianceChatGag(s::AllegianceChatGag {
                    name: "Bob".into(),
                    gagged: 1,
                }),
            ),
            (
                "chat ungag Bob",
                Request::AllegianceChatGag(s::AllegianceChatGag {
                    name: "Bob".into(),
                    gagged: 0,
                }),
            ),
            (
                "chat kick Bob",
                Request::AllegianceChatBoot(s::AllegianceChatBoot {
                    name: "Bob".into(),
                    reason: NO_REASON_GIVEN.into(),
                }),
            ),
            (
                "boot Bob",
                Request::AllegianceBreakAllegianceBoot(s::AllegianceBreakAllegianceBoot {
                    name: "Bob".into(),
                    account_boot: 0,
                }),
            ),
            (
                "boot -account Bob",
                Request::AllegianceBreakAllegianceBoot(s::AllegianceBreakAllegianceBoot {
                    name: "Bob".into(),
                    account_boot: 1,
                }),
            ),
        ];
        for (line, want) in cases {
            let (out, sent) = run(line);
            assert_eq!(sent, vec![want], "@allegiance {line}");
            assert_eq!(out.sent, 1, "@allegiance {line}");
            assert!(
                !out.lines
                    .iter()
                    .any(|(t, _)| t == PLEASE_SEE_HELP_ALLEGIANCE),
                "@allegiance {line} must not reach the help hint"
            );
        }
    }

    /// The comma split, which is the only free-text parse in the family.
    #[test]
    fn a_chat_kick_splits_the_reason_at_the_first_comma_and_trims_only_the_reason() {
        let (_, sent) = run("chat kick Bob Jones,   being rude, again");
        assert_eq!(
            sent,
            vec![Request::AllegianceChatBoot(s::AllegianceChatBoot {
                name: "Bob Jones".into(),
                reason: "being rude, again".into(),
            })]
        );
    }

    /// `@allegiance chat on|off` sends no allegiance event at all; it
    /// sets the hear-allegiance-chat player option, ordinal 27.
    #[test]
    fn chat_on_and_off_toggle_the_option_and_send_no_allegiance_event() {
        for (line, want) in [("chat on", true), ("chat off", false)] {
            let (out, sent) = run(line);
            assert!(sent.is_empty(), "{line}");
            assert_eq!(out.hear_allegiance_chat, Some(want), "{line}");
            assert!(out.lines.is_empty(), "{line}");
        }
    }

    /// The four refusals that print instead of sending, each with the handler that owns it.
    #[test]
    fn the_named_refusals_print_and_send_nothing() {
        for (line, want) in [
            ("info", PLEASE_SPECIFY_AN_ACTUAL_NAME),
            ("boot", PLEASE_SPECIFY_AN_ACTUAL_NAME),
            ("ban add", PLEASE_SPECIFY_AN_ACTUAL_NAME),
            ("chat gag", PLEASE_SPECIFY_AN_ACTUAL_NAME),
            ("officer remove", PLEASE_SPECIFY_AN_ALLEGIANCE_MEMBER),
            ("officer set 2", PLEASE_SPECIFY_AN_ALLEGIANCE_MEMBER),
            (
                "officer set 9 Bob",
                PLEASE_SPECIFY_A_VALID_OFFICER_LEVEL_LONG,
            ),
            ("title set 0 x", PLEASE_SPECIFY_A_VALID_OFFICER_LEVEL),
        ] {
            let (out, sent) = run(line);
            assert!(sent.is_empty(), "@allegiance {line}");
            assert_eq!(
                out.lines,
                vec![(want.to_owned(), REFUSAL_CHAT_TYPE)],
                "@allegiance {line}"
            );
        }
    }

    /// The `@allegiance boot` handler's acknowledgement, on chat type **0** and before the send.
    #[test]
    fn a_boot_prints_its_acknowledgement_on_chat_type_zero() {
        let (out, sent) = run("boot -account Bob");
        assert_eq!(
            out.lines,
            vec![(
                "Attempting to boot Bob (Account)...\n".to_owned(),
                BOOT_CHAT_TYPE
            )]
        );
        assert_eq!(sent.len(), 1);
        let (out, _) = run("boot Bob");
        assert_eq!(
            out.lines,
            vec![("Attempting to boot Bob...\n".to_owned(), BOOT_CHAT_TYPE)]
        );
    }

    /// `@alh` / `@ah` — their own table entry, and the one handler that refuses arguments.
    #[test]
    fn the_hometown_alias_takes_no_arguments() {
        let mut w = World::new();
        let mut req = RecordingRequests::default();
        let out = w.do_allegiance_hometown(&mut req, &[]);
        assert_eq!(
            req.0,
            vec![Request::AllegianceRecallHometown(
                s::AllegianceRecallAllegianceHometown
            )]
        );
        assert!(out.handled);

        let mut req = RecordingRequests::default();
        let out = w.do_allegiance_hometown(&mut req, &argv("now"));
        assert!(req.0.is_empty());
        assert_eq!(
            out.lines,
            vec![(
                THIS_COMMAND_TAKES_NO_ARGUMENTS.to_owned(),
                REFUSAL_CHAT_TYPE
            )]
        );
    }

    /// `@ab` and `@allegiance broadcast` are `0x0147` on `0x2000000`, and empty text is a
    /// **refusal** rather than an empty broadcast.
    #[test]
    fn a_broadcast_is_a_channel_message_and_an_empty_one_is_refused() {
        let mut w = World::new();
        let mut req = RecordingRequests::default();
        let out = w.do_allegiance_broadcast(&mut req, &argv("hello all"));
        assert!(out.handled);
        assert_eq!(
            req.0,
            vec![Request::ChannelBroadcast(
                dereth_protocol::comms::CommunicationChannelBroadcast {
                    channel: ALLEGIANCE_BROADCAST_CHANNEL,
                    message: "hello all".into(),
                }
            )]
        );
        let mut req = RecordingRequests::default();
        assert!(!w.do_allegiance_broadcast(&mut req, &[]).handled);
        assert!(req.0.is_empty());
    }
}
