//! The `WeenieError` codes that become a chat line.
//!
//! Some lines in retail's chat backlog, such as *"You have entered the Trade channel."* and *"You
//! have entered the LFG channel."*, do not arrive on `0xF7E0 Communication_TextboxString` — they
//! are **`0x028B Communication_WeenieErrorWithString`**, which `dereth_protocol` decodes. ACE
//! sends them from `Player::JoinTurbineChatChannel`
//! (`GameEventWeenieErrorWithString(WeenieErrorWithString.YouHaveEnteredThe_Channel, channelName)`).
//!
//! # The chain
//!
//! The inbound message is filtered on its opcode, handed to the communication system's
//! weenie-error-with-string handler, and from there to one failure-event routine that takes the
//! code and the server's string.
//!
//! That routine is one enormous `switch (errorCode)` whose arms are **hard-coded English
//! literals**, not string-table rows: code `0x51a` is
//! `L"Only the original owner may use that item's magic."` and nothing looks it up. The two
//! channel arms take the server's string as the single `%s` substitution and hand the result
//! to the scroll with **chat type `0`** — `Broadcast`, which the colour table fills green. That
//! is why retail shows these two lines in exactly the same green as ACE's welcome banner.
//!
//! The channel arms' `sprintf` formats are `L"You have entered the %s channel.\n"` and
//! `L"You have left the %s channel.\n"`. Two independent sources agree on them: a retail
//! screenshot of the rendered line ("You have entered the Trade channel."), and ACE's own
//! `WeenieErrorWithString.cs`, which carries the retail text as the enumerator's doc comment,
//! `/// You have entered the  channel.`, whose **two spaces** are where the `%s` was stripped
//! when that file was transcribed. The trailing newline is in the client's bytes; the scroll's
//! trim (`add_text_to_scroll_trim`) takes it off again, so no rendered line changes.
//!
//! # The portal refusal, and why the chat type is per arm
//!
//! A move-to targeting a portal the player cannot use circles it endlessly, in retail as here, but
//! retail prints *"You must complete a quest to interact with that portal."* when the player walks
//! through the portal. It is not a string-table row and it is not server text: the server sends a
//! bare `WeenieError` code and the sentence is the client's own, carried as a wide literal exactly
//! like `0x51A`'s:
//!
//! ```text
//! 0x474  "You must complete a quest to interact with that portal.\n"   chat type 7
//! 0x36   "Action cancelled!"                                           chat type 0x1A
//! ```
//!
//! **The third argument is not `0` for either of them.** `0x474` is chat type **7** (`MAGIC`, light
//! blue) and `0x36` is **`0x1A`** (`LOCAL_ERROR`, red); the two channel arms above are `0`. So
//! `FAILURE_EVENT_CHAT_TYPE` cannot stand for every arm -- it is *the two channel arms'* type,
//! and each other arm carries its own.
//!
//! **`0x36 ACTION_CANCELLED` matters because of what this build's own server does.** `serv-game`'s
//! `use_item` handles `LIFE_STONE` and answers `ACTION_CANCELLED` for every other weenie type,
//! portals included -- so against it a refused portal produces a `0x01C7 Item_UseDone` carrying
//! `0x36`. Against a shard that gates the portal properly the code is `0x474`. Both print.
//!
//! **The trailing newline on `0x474` is retail's own and is kept.** The chat window's one
//! scroll-submission path trims a line's trailing newlines before it reaches the log, so
//! transcribing the literal verbatim costs nothing here.
//!
//! # The fellowship arms
//!
//! These are not string-table rows (`0x23000001`) keyed by the code: the routine is a ladder of
//! bands over switches, and every arm carries its own wide literal. The arms below were each
//! resolved through those switches and then read out:
//!
//! ```text
//! 0x50b  " is now an open fellowship; anyone may recruit new members.\n"  type 0, text + literal
//! 0x50c  " is now a closed fellowship.\n"                                 type 0, text + literal
//! 0x50d  " is now the leader of this fellowship.\n"                       type 0, text + literal
//! 0x50e  "You have passed leadership of the fellowship to %s\n"           type 0, sprintf
//! 0x50f  "You do not belong to a Fellowship."                             type 0x1a
//! 0x518  "This fellowship is locked; " + text + " cannot be recruited into the fellowship.\n"
//! 0x519  "The fellowship is locked, you were not added to the fellowship.\n"  type 0
//! 0x528  "The fellowship is locked; you cannot open locked fellowships.\n"    type 0
//! 0x41d  "You must be the leader of a Fellowship"                         type 0x1a
//! 0x41e  "Your Fellowship is full"                                        type 0x1a
//! 0x41f  "That Fellowship name is not permitted"                          type 0x1a
//! 0x48e  "Your offer of Allegiance has been ignored."                     type 0x1a
//! 0x58b  "As a mindless engine of destruction an Olthoi cannot join a fellowship!\n"  type 7
//! ```
//!
//! The literals keep their trailing newline here, as `0x474`'s does; the scroll trims it as its
//! first act (`dereth_client::chat::add_text_to_scroll_trim`), and the `hud.rs`
//! arms apply that trim on the way to the scroll.
//!
//! # The whole function, and what a miss does
//!
//! Moving during a `/ls` recall animation aborts the teleport, and ACE
//! (`Player_Location.cs:187`) sends `GameEventWeenieError(WeenieError.YouHaveMovedTooFar
//! /* 0x0498 */)`; the client's arm for it is a wide literal -- "You have
//! moved too far!", pushed with chat type `0x1A` (`LOCAL_ERROR`).
//!
//! **The dispatch.** The failure-event routine is a five-level band ladder over four switches,
//! two of them indexed through a byte table. Simulating that ladder over every `u32` the bands can
//! reach gives **344 codes with an arm**, spread over 333 distinct arms (eleven arms are shared
//! -- `0x23`, `0x37`, `0x38` and `0x39` all land on one of them, and so on). Codes below `0x17`
//! and above `0x593` cannot reach a table slot at all.
//!
//! **There is no string table, and no fallback.** A code with no arm is not looked up in the
//! `0x23000005`-style table. The only things this routine calls are the scroll (232 call sites),
//! the string construction / formatting / concatenation / assignment / destruction family, the
//! combat system's auto-attack abort (the preamble that runs for
//! `0x23`/`0x3E`/`0x36`/`0x43`/`0x3F7`),
//! the UI system's ground-object setter (the four "Unable to move to object!" arms), the
//! abuse-report notice (the three abuse codes), the plugin API and two interlocked refcount
//! imports. Every band's upper bound and every table slot for a code with no arm -- `0x417`
//! (`FellowshipIgnoringRequests`), `0x4db`, `0x4dc`, `0x51d` (`TurbineChatIsEnabled`) among the
//! codes ACE sends -- jumps to the function's **epilogue**: the local string's refbuffer is
//! decremented and the function returns.
//!
//! So **a miss prints nothing at all**, and `None` below is retail exactly. The table has all
//! 344 of retail's arms, so no code that retail prints takes the miss exit.
//!
//! **Which surface.** The chat type is the arm's own, and across the 344 arms it takes exactly
//! three values: `0` (Broadcast, the colour table's green fill) on **162** arms, `7` (`MAGIC`,
//! light blue) on **59**, and `0x1A` (`LOCAL_ERROR`, bright red) on **120** -- plus the three
//! abuse arms, which draw no line. The display fan-out then decides the surface, and it decides
//! it by *type alone*: the display-final-string notice offers every line to every registered
//! receiver, the spew box takes it iff `type == 0x1A`, and the chat interface takes it iff the
//! type is active -- and the default text-type filter `0xFBFFFFFF` has bit 26 clear. So the 120
//! red arms draw on the **over-head bubble strip** and are absent from the scrollback unless the
//! player ticks the **Error** group; the other 221 draw in the **chat window**, in their own
//! colour. `0x0498` is one of the 117: "You have moved too far!" is a red bubble, not a
//! scrollback line.
//!
//! **Eight composition shapes, and nothing else.** The arms differ only in how the literal and
//! the server's `%s` combine -- see `Arm`. 219 are a bare literal, 98 a `sprintf` format (the
//! nine with two `%s` push the same pointer twice, so both take the same string), 20 are
//! `text + literal`, and five are one-offs. Three arms (`0x24`, `0x48`, `0x49`) take a
//! *pre-built* string global rather than a literal -- the three can't-jump messages (position, in
//! the air, load), each filled by its own start-up initialiser; their text is transcribed here
//! like any other.
//!
//! **The `0x028B` sibling is the same function.** The weenie-error-with-string handler calls it
//! with the server's text and the bare weenie-error handler calls it with an empty string -- one
//! table, two producers, which is why the `%s` arms are reachable from `0x028A` too and simply
//! substitute nothing.
//!
//! # Where it lives
//!
//! Nothing in this module draws: its production surface is `super::interface::ChatMessage` and
//! nothing else, and `dereth_client::{hud, interaction}` call `handle_failure_event` at nine
//! sites. Its tests live in `dereth_ui_screens::chat::failure`, because four of them assert
//! against `chat::colors` and `chat::interface::ChatInterface`, which are presentation; they read
//! every item here through a `pub use` glob.

use super::interface::ChatMessage;

/// `WeenieErrorWithString::YouHaveEnteredThe_Channel`.
pub const YOU_HAVE_ENTERED_THE_CHANNEL: u32 = 0x051B;
/// `WeenieErrorWithString::YouHaveLeftThe_Channel`.
pub const YOU_HAVE_LEFT_THE_CHANNEL: u32 = 0x051C;

/// `WeenieError::ACTION_CANCELLED`.
pub const ACTION_CANCELLED: u32 = 0x0036;
/// `WeenieError::YouAreNotInAllegiance`.
pub const YOU_ARE_NOT_IN_ALLEGIANCE: u32 = 0x0414;
/// `WeenieError::YouMustCompleteQuestToUsePortal`.
pub const YOU_MUST_COMPLETE_QUEST_TO_USE_PORTAL: u32 = 0x0474;

/// `WeenieError::YouMustBeLeaderOfFellowship`.
pub const YOU_MUST_BE_LEADER_OF_FELLOWSHIP: u32 = 0x041D;
/// `WeenieError::YourFellowshipIsFull`.
pub const YOUR_FELLOWSHIP_IS_FULL: u32 = 0x041E;
/// `WeenieError::FellowshipNameIsNotPermitted`.
pub const FELLOWSHIP_NAME_IS_NOT_PERMITTED: u32 = 0x041F;
/// `WeenieError::YourOfferOfAllegianceWasIgnored` -- seen in a recorded fellowship session.
pub const YOUR_OFFER_OF_ALLEGIANCE_WAS_IGNORED: u32 = 0x048E;
/// `WeenieErrorWithString::_IsNowOpenFellowship`.
pub const IS_NOW_OPEN_FELLOWSHIP: u32 = 0x050B;
/// `WeenieErrorWithString::_IsNowClosedFellowship`.
pub const IS_NOW_CLOSED_FELLOWSHIP: u32 = 0x050C;
/// `WeenieErrorWithString::_IsNowLeaderOfFellowship`.
pub const IS_NOW_LEADER_OF_FELLOWSHIP: u32 = 0x050D;
/// `WeenieErrorWithString::YouHavePassedFellowshipLeadershipTo_`.
pub const YOU_HAVE_PASSED_FELLOWSHIP_LEADERSHIP_TO: u32 = 0x050E;
/// `WeenieError::YouDoNotBelongToAFellowship`.
pub const YOU_DO_NOT_BELONG_TO_A_FELLOWSHIP: u32 = 0x050F;
/// `WeenieErrorWithString::LockedFellowshipCannotRecruit_`.
pub const LOCKED_FELLOWSHIP_CANNOT_RECRUIT: u32 = 0x0518;
/// `WeenieError::LockedFellowshipCannotRecruitYou`.
pub const LOCKED_FELLOWSHIP_CANNOT_RECRUIT_YOU: u32 = 0x0519;
/// `WeenieError::FellowshipIsLocked`.
pub const FELLOWSHIP_IS_LOCKED: u32 = 0x0528;
/// `WeenieError::OlthoiCannotJoinFellowship`.
pub const OLTHOI_CANNOT_JOIN_FELLOWSHIP: u32 = 0x058B;

/// The chat type the scroll is given for the **two channel arms**: `0`, `Broadcast`, drawn
/// in the colour table's green fill.
///
/// **Only those two arms.** It is not the type of every arm: the failure-event routine's third
/// argument is per-arm (`0x474` passes `7`, `0x36` passes `0x1A`) and the module doc carries the
/// listing.
pub const FAILURE_EVENT_CHAT_TYPE: u8 = 0;

/// `dereth_client_model::chat::text_type::MAGIC` -- the type `case 0x474` passes.
pub const MAGIC_CHAT_TYPE: u8 = 7;
/// `dereth_client_model::chat::text_type::LOCAL_ERROR` -- the type `case 0x36` passes.
pub const LOCAL_ERROR_CHAT_TYPE: u8 = 0x1A;

/// `dereth_client_model::chat::text_type::BROADCAST` -- the scroll's type `0`, the colour table's
/// green fill. The same value as `FAILURE_EVENT_CHAT_TYPE`, named for what it *is* rather than
/// for the two channel arms: 162 of the 344 arms push it.
pub const BROADCAST_CHAT_TYPE: u8 = 0;

/// `WeenieError::YouHaveMovedTooFar` -- the code ACE's `Player_Location.cs:187` sends when you
/// move during a recall.
pub const YOU_HAVE_MOVED_TOO_FAR: u32 = 0x0498;

/// How one arm of the failure-event routine combines its literal with the
/// server's string.
///
/// Retail is *not* generic here: there is no string table, no string-by-id lookup, and no
/// default sentence. Each arm is a wide literal plus a chat
/// type pushed at the arm, and the only thing that varies between arms is the composition --
/// of which the whole 344-arm function uses exactly these eight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arm {
    /// The literal alone, handed straight to the scroll; the server's string is present but not
    /// read. **219 arms**, the commonest shape by a wide margin.
    Lit(&'static str),
    /// A format the server's string is substituted into, whether through `sprintf` or through
    /// the formatting constructor. Every `%s` takes the server's string; the nine arms whose
    /// format has two of them push the same pointer twice, so both substitutions are the same
    /// word. **98 arms.**
    Fmt(&'static str),
    /// The server's string first, the literal after it -- either `s = text; s += L"..."` or
    /// `operator+(text, lit)`, which compose the same line. **20 arms.**
    Suffix(&'static str),
    /// `set(L"..."); += text; += L"..."` -- the locked-fellowship recruit refusal (`0x518`)
    /// alone.
    Wrap(&'static str, &'static str),
    /// `text + L"..." + text + L"..."`, three chained appends -- `0x4f8` alone, and the
    /// `operator+` spelling of what `0x4f6` says with a two-`%s` format.
    SuffixMid(&'static str, &'static str),
    /// The server's string, verbatim and alone, straight into the scroll -- `0x55e` (`AFK`)
    /// alone.
    Text,
    /// A format whose substitution falls back to the second literal when the server's string is
    /// empty: the arm tests the string's length and builds `L"item"` instead -- `0x4bf` alone.
    FmtOr(&'static str, &'static str),
    /// An arm that reaches no scroll call on any path: the three abuse-report codes share one
    /// arm, which raises the abuse-report response notice and jumps straight to the epilogue.
    /// Retail draws no chat line for them.
    Silent,
}

impl Arm {
    /// The line this arm composes from the server's string, or `None` when it draws none.
    /// The trailing newline many literals carry is retail's own; the scroll
    /// trims it as its first act (`dereth_client::chat::add_text_to_scroll_trim`), which the
    /// `hud.rs` arms apply on the way to the scroll.
    #[must_use]
    pub fn render(self, text: &str) -> Option<String> {
        Some(match self {
            Arm::Lit(s) => s.to_owned(),
            Arm::Fmt(f) => f.replace("%s", text),
            Arm::Suffix(s) => format!("{text}{s}"),
            Arm::Wrap(a, b) => format!("{a}{text}{b}"),
            Arm::SuffixMid(a, b) => format!("{text}{a}{text}{b}"),
            Arm::Text => text.to_owned(),
            Arm::FmtOr(f, empty) => f.replace("%s", if text.is_empty() { empty } else { text }),
            Arm::Silent => return None,
        })
    }
}

/// **Every** arm of the failure-event routine: the error code, the chat type that arm pushes to,
/// and the composition.
///
/// This is the whole function rather than a selection. Every code the
/// routine can reach is listed with the literal it prints, the string it composes, and the chat
/// type. Nothing is invented: a code absent from this table is a code whose
/// band or table slot points at the epilogue, and retail prints nothing for it.
///
/// **Sorted by code**, which [`arm_for`] binary-searches. The comment above each row is the ACE
/// enumerator (`WeenieError.cs` / `WeenieErrorWithString.cs`).
pub static ARMS: [(u32, u8, Arm); 344] = [
    // `WeenieError::BadMovementEvent`.
    (0x0017, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You failed to go to non-combat mode.")),
    // `WeenieError::YoureTooBusy`.
    (0x001d, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You're too busy!")),
    // `WeenieErrorWithString::_IsTooBusyToAcceptGifts`.
    (0x001e, BROADCAST_CHAT_TYPE, Arm::Suffix(" is too busy to accept gifts right now.\n")),
    // `WeenieError::IllegalInventoryTransaction`.
    (0x0020, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You must control both objects!")),
    // `WeenieError::MotionFailure`.
    (0x0023, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("Unable to move to object!")),
    // `WeenieError::YouCantJumpWhileInTheAir`.
    (0x0024, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You can't jump while in the air")),
    // `WeenieError::ThatIsNotAValidCommand`.
    (0x0026, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("That is not a valid command.")),
    // `WeenieError::Frozen`.
    (0x0028, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("The item is under someone else's control!")),
    // `WeenieError::Stuck`.
    (0x0029, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You cannot pick that up!")),
    // `WeenieError::YouAreTooEncumbered`.
    (0x002a, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You are too encumbered to carry that!")),
    // `WeenieErrorWithString::_CannotCarryAnymore`.
    (0x002b, BROADCAST_CHAT_TYPE, Arm::Suffix(" cannot carry anymore.\n")),
    // `WeenieError::ActionCancelled`.
    (0x0036, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("Action cancelled!")),
    // `WeenieError::ObjectGone`.
    (0x0037, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("Unable to move to object!")),
    // `WeenieError::NoObject`.
    (0x0038, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("Unable to move to object!")),
    // `WeenieError::CantGetThere`.
    (0x0039, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("Unable to move to object!")),
    // `WeenieError::Dead`.
    (0x003a, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You can't do that... you're dead!")),
    // `WeenieError::YouChargedTooFar`.
    (0x003d, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You charged too far!")),
    // `WeenieError::YouAreTooTiredToDoThat`.
    (0x003e, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You are too tired to do that!")),
    // `WeenieError::YouCantJumpFromThisPosition`.
    (0x0048, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You can't jump from this position")),
    // `WeenieError::CantJumpLoadedDown`.
    (0x0049, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You're too loaded down to jump")),
    // `WeenieError::YouKilledYourself`.
    (0x004a, BROADCAST_CHAT_TYPE, Arm::Lit("Ack! You killed yourself!\n")),
    // `WeenieError::InvalidPkStatus`.
    (0x004d, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("Invalid PK status!")),
    // `WeenieErrorWithString::YouFailToAffect_YouCannotAffectAnyone`.
    (0x004e, MAGIC_CHAT_TYPE, Arm::Fmt("You fail to affect %s because you cannot affect anyone!\n")),
    // `WeenieErrorWithString::YouFailToAffect_TheyCannotBeHarmed`.
    (0x004f, MAGIC_CHAT_TYPE, Arm::Fmt("You fail to affect %s because $s cannot be harmed!\n")),
    // `WeenieErrorWithString::YouFailToAffect_WithBeneficialSpells`.
    (0x0050, MAGIC_CHAT_TYPE, Arm::Fmt("You fail to affect %s because beneficial spells do not affect %s!\n")),
    // `WeenieErrorWithString::YouFailToAffect_YouAreNotPK`.
    (0x0051, MAGIC_CHAT_TYPE, Arm::Fmt("You fail to affect %s because you are not a player killer!\n")),
    // `WeenieErrorWithString::YouFailToAffect_TheyAreNotPK`.
    (0x0052, MAGIC_CHAT_TYPE, Arm::Fmt("You fail to affect %s because %s is not a player killer!\n")),
    // `WeenieErrorWithString::YouFailToAffect_NotSamePKType`.
    (0x0053, MAGIC_CHAT_TYPE, Arm::Fmt("You fail to affect %s because you are not the same sort of player killer as %s!\n")),
    // `WeenieErrorWithString::YouFailToAffect_AcrossHouseBoundary`.
    (0x0054, MAGIC_CHAT_TYPE, Arm::Fmt("You fail to affect %s because you are acting across a house boundary!\n")),
    // `WeenieError::TheContainerIsClosed`.
    (0x03ee, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("The container is closed!")),
    // `WeenieErrorWithString::_IsNotAcceptingGiftsRightNow`.
    (0x03ef, BROADCAST_CHAT_TYPE, Arm::Suffix(" is not accepting gifts right now.\n")),
    // `WeenieError::ChangeCombatModeFailure`.
    (0x03f1, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You failed to go to non-combat mode.")),
    // `WeenieError::YouAreTooFatiguedToAttack`.
    (0x03f7, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You are too fatigued to attack!")),
    // `WeenieError::YouAreOutOfAmmunition`.
    (0x03f8, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You are out of ammunition!")),
    // `WeenieError::YourAttackMisfired`.
    (0x03f9, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("Your missile attack misfired!")),
    // `WeenieError::YouveAttemptedAnImpossibleSpellPath`.
    (0x03fa, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You've attempted an impossible spell path!")),
    // `WeenieError::YouDontKnowThatSpell`.
    (0x03fe, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You don't know that spell!")),
    // `WeenieError::IncorrectTargetType`.
    (0x03ff, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("Incorrect target type")),
    // `WeenieError::YouDontHaveAllTheComponents`.
    (0x0400, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You don't have all the components for this spell.")),
    // `WeenieError::YouDontHaveEnoughManaToCast`.
    (0x0401, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You don't have enough Mana to cast this spell.")),
    // `WeenieError::YourSpellFizzled`.
    (0x0402, MAGIC_CHAT_TYPE, Arm::Lit("Your spell fizzled.\n")),
    // `WeenieError::YourSpellTargetIsMissing`.
    (0x0403, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("Your spell's target is missing!")),
    // `WeenieError::YourProjectileSpellMislaunched`.
    (0x0404, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("Your projectile spell mislaunched!")),
    // `WeenieError::YourSpellCannotBeCastOutside`.
    (0x0407, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("Your spell cannot be cast outside")),
    // `WeenieError::YourSpellCannotBeCastInside`.
    (0x0408, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("Your spell cannot be cast inside")),
    // `WeenieError::YouAreUnpreparedToCastASpell`.
    (0x040a, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You are unprepared to cast a spell")),
    // `WeenieError::YouveAlreadySwornAllegiance`.
    (0x040b, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You've already sworn your Allegiance")),
    // `WeenieError::CantSwearAllegianceInsufficientXp`.
    (0x040c, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You don't have enough experience available to swear Allegiance")),
    // `WeenieErrorWithString::_IsAlreadyOneOfYourFollowers`.
    (0x0413, LOCAL_ERROR_CHAT_TYPE, Arm::Fmt("%s is already one of your followers")),
    // `WeenieError::YouAreNotInAllegiance`.
    (0x0414, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You are not in an allegiance!")),
    // `WeenieErrorWithString::_CannotHaveAnyMoreVassals`.
    (0x0416, LOCAL_ERROR_CHAT_TYPE, Arm::Fmt("%s cannot have any more Vassals")),
    // `WeenieError::YouMustBeLeaderOfFellowship`.
    (0x041d, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You must be the leader of a Fellowship")),
    // `WeenieError::YourFellowshipIsFull`.
    (0x041e, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("Your Fellowship is full")),
    // `WeenieError::FellowshipNameIsNotPermitted`.
    (0x041f, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("That Fellowship name is not permitted")),
    // `WeenieError::ThatChannelDoesntExist`.
    (0x0422, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("That channel doesn't exist.")),
    // `WeenieError::YouCantUseThatChannel`.
    (0x0423, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You can't use that channel.")),
    // `WeenieError::YouAreAlreadyOnThatChannel`.
    (0x0424, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You're already on that channel.")),
    // `WeenieError::YouAreNotOnThatChannel`.
    (0x0425, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You're not currently on that channel.")),
    // `WeenieError::YouCannotMergeDifferentStacks`.
    (0x0427, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You cannot merge different stacks!")),
    // `WeenieError::YouCannotMergeEnchantedItems`.
    (0x0428, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You cannot merge enchanted items!")),
    // `WeenieError::YouMustControlAtLeastOneStack`.
    (0x0429, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You must control at least one stack!")),
    // `WeenieError::UnableToMakeCraftReq`.
    (0x0432, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("Your craft attempt fails.")),
    // `WeenieError::CraftAnimationFailed`.
    (0x0433, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("Your craft attempt fails.")),
    // `WeenieError::YouCantCraftWithThatNumberOfItems`.
    (0x0434, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("Given that number of items, you cannot craft anything.")),
    // `WeenieError::CraftGeneralErrorUiMsg`.
    (0x0435, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("Your craft attempt fails.")),
    // `WeenieError::YouDoNotPassCraftingRequirements`.
    (0x0437, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("Either you or one of the items involved does not pass the requirements for this craft interaction.")),
    // `WeenieError::YouDoNotHaveAllTheNecessaryItems`.
    (0x0438, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You do not have all the neccessary items.")),
    // `WeenieError::NotAllTheItemsAreAvailable`.
    (0x0439, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("Not all the items are avaliable.")),
    // `WeenieError::YouMustBeInPeaceModeToTrade`.
    (0x043a, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You must be at rest in peace mode to do trade skills.")),
    // `WeenieError::YouAreNotTrainedInThatTradeSkill`.
    (0x043b, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You are not trained in that trade skill.")),
    // `WeenieError::YourHandsMustBeFree`.
    (0x043c, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("Your hands must be free.")),
    // `WeenieError::YouCannotLinkToThatPortal`.
    (0x043d, MAGIC_CHAT_TYPE, Arm::Lit("You cannot link to that portal!\n")),
    // `WeenieError::YouHaveSolvedThisQuestTooRecently`.
    (0x043e, BROADCAST_CHAT_TYPE, Arm::Lit("You have solved this quest too recently!\n")),
    // `WeenieError::YouHaveSolvedThisQuestTooManyTimes`.
    (0x043f, BROADCAST_CHAT_TYPE, Arm::Lit("You have solved this quest too many times!\n")),
    // `WeenieError::ItemRequiresQuestToBePickedUp`.
    (0x0445, BROADCAST_CHAT_TYPE, Arm::Lit("This item requires you to complete a specific quest before you can pick it up!\n")),
    // `WeenieError::PKsMayNotUsePortal`.
    (0x045c, MAGIC_CHAT_TYPE, Arm::Lit("Player killers may not interact with that portal!\n")),
    // `WeenieError::NonPKsMayNotUsePortal`.
    (0x045d, MAGIC_CHAT_TYPE, Arm::Lit("Non-player killers may not interact with that portal!\n")),
    // `WeenieError::HouseAbandoned`.
    (0x045e, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You do not own a house!")),
    // `WeenieError::HouseEvicted`.
    (0x045f, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You do not own a house!")),
    // `WeenieError::YouMustHaveDarkMajestyToUsePortal`.
    (0x0466, MAGIC_CHAT_TYPE, Arm::Lit("You must purchase Asheron's Call: Dark Majesty to interact with that portal.\n")),
    // `WeenieError::YouHaveUsedAllTheHooks`.
    (0x0469, BROADCAST_CHAT_TYPE, Arm::Lit("You have used all the hooks you are allowed to use for this house.\n")),
    // `WeenieError::TradeAiDoesntWant`.
    (0x046a, BROADCAST_CHAT_TYPE, Arm::Suffix(" doesn't know what to do with that.\n")),
    // `WeenieError::YouMustCompleteQuestToUsePortal`.
    (0x0474, MAGIC_CHAT_TYPE, Arm::Lit("You must complete a quest to interact with that portal.\n")),
    // `WeenieError::YouMustOwnHouseToUseCommand`.
    (0x047f, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You must own a house to use this command.")),
    // `WeenieError::YourMonarchDoesNotOwnAMansionOrVilla`.
    (0x0480, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("Your monarch does not own a mansion or a villa!")),
    // `WeenieError::YourMonarchsHouseIsNotAMansionOrVilla`.
    (0x0481, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("Your monarch does not own a mansion or a villa!")),
    // `WeenieError::YourMonarchHasClosedTheMansion`.
    (0x0482, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("Your monarch has closed the mansion to the Allegiance.")),
    // `WeenieErrorWithString::YouMustBeAboveLevel_ToBuyHouse`.
    (0x0488, BROADCAST_CHAT_TYPE, Arm::Fmt("You must be above level %s to purchase this dwelling.\n")),
    // `WeenieErrorWithString::YouMustBeAtOrBelowLevel_ToBuyHouse`.
    (0x0489, BROADCAST_CHAT_TYPE, Arm::Fmt("You must be at or below level %s to purchase this dwelling.\n")),
    // `WeenieError::YouMustBeMonarchToPurchaseDwelling`.
    (0x048a, BROADCAST_CHAT_TYPE, Arm::Lit("You must be a monarch to purchase this dwelling.\n")),
    // `WeenieErrorWithString::YouMustBeAboveAllegianceRank_ToBuyHouse`.
    (0x048b, BROADCAST_CHAT_TYPE, Arm::Fmt("You must be above allegiance rank %s to purchase this dwelling.\n")),
    // `WeenieErrorWithString::YouMustBeAtOrBelowAllegianceRank_ToBuyHouse`.
    (0x048c, BROADCAST_CHAT_TYPE, Arm::Fmt("You must be at or below allegiance rank %s to purchase this dwelling.\n")),
    // `WeenieError::YourOfferOfAllegianceWasIgnored`.
    (0x048e, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("Your offer of Allegiance has been ignored.")),
    // `WeenieError::ConfirmationInProgress`.
    (0x048f, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You are already involved in something!")),
    // `WeenieError::YouMustBeAMonarchToUseCommand`.
    (0x0490, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You must be a monarch to use this command.")),
    // `WeenieError::YouMustSpecifyCharacterToBoot`.
    (0x0491, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You must specify a character to boot.")),
    // `WeenieError::YouCantBootYourself`.
    (0x0492, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You can't boot yourself!")),
    // `WeenieError::ThatCharacterDoesNotExist`.
    (0x0493, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("That character does not exist.")),
    // `WeenieError::ThatPersonIsNotInYourAllegiance`.
    (0x0494, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("That person is not a member of your Allegiance!")),
    // `WeenieError::CantBreakFromPatronNotInAllegiance`.
    (0x0495, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("No patron from which to break!")),
    // `WeenieError::YourAllegianceHasBeenDissolved`.
    (0x0496, BROADCAST_CHAT_TYPE, Arm::Lit("Your Allegiance has been dissolved!\n")),
    // `WeenieError::YourPatronsAllegianceHasBeenBroken`.
    (0x0497, BROADCAST_CHAT_TYPE, Arm::Lit("Your patron's Allegiance to you has been broken!\n")),
    // `WeenieError::YouHaveMovedTooFar`.
    (0x0498, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You have moved too far!")),
    // `WeenieError::TeleToInvalidPosition`.
    (0x0499, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("That is not a valid destination!")),
    // `WeenieError::MustHaveDarkMajestyToUse`.
    (0x049a, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You must purchase Asheron's Call -- Dark Majesty to use this function.")),
    // `WeenieError::YouFailToLinkWithLifestone`.
    (0x049b, MAGIC_CHAT_TYPE, Arm::Lit("You fail to link with the lifestone!\n")),
    // `WeenieError::YouWanderedTooFarToLinkWithLifestone`.
    (0x049c, MAGIC_CHAT_TYPE, Arm::Lit("You wandered too far to link with the lifestone!\n")),
    // `WeenieError::YouSuccessfullyLinkWithLifestone`.
    (0x049d, MAGIC_CHAT_TYPE, Arm::Lit("You successfully link with the lifestone!\n")),
    // `WeenieError::YouMustLinkToLifestoneToRecall`.
    (0x049e, MAGIC_CHAT_TYPE, Arm::Lit("You must have linked with a lifestone in order to recall to it!\n")),
    // `WeenieError::YouFailToRecallToLifestone`.
    (0x049f, MAGIC_CHAT_TYPE, Arm::Lit("You fail to recall to the lifestone!\n")),
    // `WeenieError::YouFailToLinkWithPortal`.
    (0x04a0, MAGIC_CHAT_TYPE, Arm::Lit("You fail to link with the portal!\n")),
    // `WeenieError::YouSuccessfullyLinkWithPortal`.
    (0x04a1, MAGIC_CHAT_TYPE, Arm::Lit("You successfully link with the portal!\n")),
    // `WeenieError::YouFailToRecallToPortal`.
    (0x04a2, MAGIC_CHAT_TYPE, Arm::Lit("You fail to recall to the portal!\n")),
    // `WeenieError::YouMustLinkToPortalToRecall`.
    (0x04a3, MAGIC_CHAT_TYPE, Arm::Lit("You must have linked with a portal in order to recall to it!\n")),
    // `WeenieError::YouFailToSummonPortal`.
    (0x04a4, MAGIC_CHAT_TYPE, Arm::Lit("You fail to summon the portal!\n")),
    // `WeenieError::YouMustLinkToPortalToSummonIt`.
    (0x04a5, MAGIC_CHAT_TYPE, Arm::Lit("You must have linked with a portal in order to summon it!\n")),
    // `WeenieError::YouFailToTeleport`.
    (0x04a6, MAGIC_CHAT_TYPE, Arm::Lit("You fail to teleport!\n")),
    // `WeenieError::YouHaveBeenTeleportedTooRecently`.
    (0x04a7, MAGIC_CHAT_TYPE, Arm::Lit("You have been teleported too recently!\n")),
    // `WeenieError::YouMustBeAnAdvocateToUsePortal`.
    (0x04a8, MAGIC_CHAT_TYPE, Arm::Lit("You must be an Advocate to interact with that portal.\n")),
    // `WeenieError::PlayersMayNotUsePortal`.
    (0x04aa, MAGIC_CHAT_TYPE, Arm::Lit("Players may not interact with that portal.\n")),
    // `WeenieError::YouAreNotPowerfulEnoughToUsePortal`.
    (0x04ab, MAGIC_CHAT_TYPE, Arm::Lit("You are not powerful enough to interact with that portal!\n")),
    // `WeenieError::YouAreTooPowerfulToUsePortal`.
    (0x04ac, MAGIC_CHAT_TYPE, Arm::Lit("You are too powerful to interact with that portal!\n")),
    // `WeenieError::YouCannotRecallPortal`.
    (0x04ad, MAGIC_CHAT_TYPE, Arm::Lit("You cannot recall to that portal!\n")),
    // `WeenieError::YouCannotSummonPortal`.
    (0x04ae, MAGIC_CHAT_TYPE, Arm::Lit("You cannot summon that portal!\n")),
    // `WeenieError::LockAlreadyUnlocked`.
    (0x04af, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("The lock is already unlocked.")),
    // `WeenieError::YouCannotLockOrUnlockThat`.
    (0x04b0, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You can't lock or unlock that!")),
    // `WeenieError::YouCannotLockWhatIsOpen`.
    (0x04b1, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You can't lock or unlock what is open!")),
    // `WeenieError::KeyDoesntFitThisLock`.
    (0x04b2, BROADCAST_CHAT_TYPE, Arm::Lit("The key doesn't fit this lock.\n")),
    // `WeenieError::LockUsedTooRecently`.
    (0x04b3, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("The lock has been used too recently.")),
    // `WeenieError::YouArentTrainedInLockpicking`.
    (0x04b4, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You aren't trained in lockpicking!")),
    // `WeenieError::AllegianceInfoEmptyName`.
    (0x04b5, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You must specify a character to query.")),
    // `WeenieError::AllegianceInfoSelf`.
    (0x04b6, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("Please use the allegiance panel to view your own information.")),
    // `WeenieError::AllegianceInfoTooRecent`.
    (0x04b7, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You have used that command too recently.")),
    // `WeenieError::AbuseNoSuchCharacter`.
    (0x04b8, LOCAL_ERROR_CHAT_TYPE, Arm::Silent),
    // `WeenieError::AbuseReportedSelf`.
    (0x04b9, LOCAL_ERROR_CHAT_TYPE, Arm::Silent),
    // `WeenieError::AbuseComplaintHandled`.
    (0x04ba, LOCAL_ERROR_CHAT_TYPE, Arm::Silent),
    // `WeenieError::YouDoNotOwnThatSalvageTool`.
    (0x04bd, BROADCAST_CHAT_TYPE, Arm::Lit("You do not own that salvage tool!\n")),
    // `WeenieError::YouDoNotOwnThatItem`.
    (0x04be, BROADCAST_CHAT_TYPE, Arm::Lit("You do not own that item!\n")),
    // `WeenieErrorWithString::The_WasNotSuitableForSalvaging`.
    (0x04bf, LOCAL_ERROR_CHAT_TYPE, Arm::FmtOr("The %s was not suitable for salvaging.", "item")),
    // `WeenieErrorWithString::The_ContainseTheWrongMaterial`.
    (0x04c0, LOCAL_ERROR_CHAT_TYPE, Arm::Fmt("The %s contains the wrong material.")),
    // `WeenieError::MaterialCannotBeCreated`.
    (0x04c1, BROADCAST_CHAT_TYPE, Arm::Lit("The material cannot be created.\n")),
    // `WeenieError::ItemsAttemptingToSalvageIsInvalid`.
    (0x04c2, BROADCAST_CHAT_TYPE, Arm::Lit("The list of items you are attempting to salvage is invalid.\n")),
    // `WeenieError::YouCannotSalvageItemsInTrading`.
    (0x04c3, BROADCAST_CHAT_TYPE, Arm::Lit("You cannot salvage items that you are trading!\n")),
    // `WeenieError::YouMustBeHouseGuestToUsePortal`.
    (0x04c4, MAGIC_CHAT_TYPE, Arm::Lit("You must be a guest in this house to interact with that portal.\n")),
    // `WeenieError::YourAllegianceRankIsTooLowToUseMagic`.
    (0x04c5, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("Your Allegiance Rank is too low to use that item's magic.")),
    // `WeenieErrorWithString::YouMustBe_ToUseItemMagic`.
    (0x04c6, LOCAL_ERROR_CHAT_TYPE, Arm::Fmt("You must be %s to use that item's magic.")),
    // `WeenieError::YourArcaneLoreIsTooLowToUseMagic`.
    (0x04c7, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("Your Arcane Lore skill is too low to use that item's magic.")),
    // `WeenieError::ItemDoesntHaveEnoughMana`.
    (0x04c8, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("That item doesn't have enough Mana.")),
    // `WeenieErrorWithString::Your_IsTooLowToUseItemMagic`.
    (0x04c9, LOCAL_ERROR_CHAT_TYPE, Arm::Fmt("Your %s is too low to use that item's magic.")),
    // `WeenieErrorWithString::Only_MayUseItemMagic`.
    (0x04ca, LOCAL_ERROR_CHAT_TYPE, Arm::Fmt("Only %s may use that item's magic.")),
    // `WeenieErrorWithString::YouMustSpecialize_ToUseItemMagic`.
    (0x04cb, LOCAL_ERROR_CHAT_TYPE, Arm::Fmt("You must have %s specialized to use that item's magic.")),
    // `WeenieError::YouHaveBeenInPKBattleTooRecently`.
    (0x04cc, MAGIC_CHAT_TYPE, Arm::Lit("You have been involved in a player killer battle too recently to do that!\n")),
    // `WeenieErrorWithString::AiRefuseItemDuringEmote`.
    (0x04ce, BROADCAST_CHAT_TYPE, Arm::Suffix(" is too busy to accept gifts right now.\n")),
    // `WeenieErrorWithString::_CannotAcceptStackedItems`.
    (0x04cf, BROADCAST_CHAT_TYPE, Arm::Suffix(" cannot accept stacked objects. Try giving one at a time.\n")),
    // `WeenieError::YouFailToAlterSkill`.
    (0x04d0, BROADCAST_CHAT_TYPE, Arm::Lit("You have failed to alter your skill.\n")),
    // `WeenieErrorWithString::Your_SkillMustBeTrained`.
    (0x04d1, BROADCAST_CHAT_TYPE, Arm::Fmt("Your %s skill must be trained, not untrained or specialized, in order to be altered in this way!\n")),
    // `WeenieErrorWithString::NotEnoughSkillCreditsToSpecialize`.
    (0x04d2, BROADCAST_CHAT_TYPE, Arm::Fmt("You do not have enough skill credits to specialize your %s skill.\n")),
    // `WeenieErrorWithString::TooMuchXPToRecoverFromSkill`.
    (0x04d3, BROADCAST_CHAT_TYPE, Arm::Fmt("You have too many available experience points to be able to absorb the experience points from your %s skill. Please spend some of your experience points and try again.\n")),
    // `WeenieErrorWithString::Your_SkillIsAlreadyUntrained`.
    (0x04d4, BROADCAST_CHAT_TYPE, Arm::Fmt("Your %s skill is already untrained!\n")),
    // `WeenieErrorWithString::CannotLowerSkillWhileWieldingItem`.
    (0x04d5, BROADCAST_CHAT_TYPE, Arm::Fmt("You are currently wielding items which require a certain level of %s.  Your %s skill cannot be lowered while you are wielding these items.  Please remove these items and try again.\n")),
    // `WeenieErrorWithString::YouHaveSucceededSpecializing_Skill`.
    (0x04d6, BROADCAST_CHAT_TYPE, Arm::Fmt("You have succeeded in specializing your %s skill!\n")),
    // `WeenieErrorWithString::YouHaveSucceededUnspecializing_Skill`.
    (0x04d7, BROADCAST_CHAT_TYPE, Arm::Fmt("You have succeeded in lowering your %s skill from specialized to trained!\n")),
    // `WeenieErrorWithString::YouHaveSucceededUntraining_Skill`.
    (0x04d8, BROADCAST_CHAT_TYPE, Arm::Fmt("You have succeeded in untraining your %s skill!\n")),
    // `WeenieErrorWithString::CannotUntrain_SkillButRecoveredXP`.
    (0x04d9, BROADCAST_CHAT_TYPE, Arm::Fmt("Although you cannot untrain your %s skill, you have succeeded in recovering all the experience you had invested in it.\n")),
    // `WeenieErrorWithString::TooManyCreditsInSpecializedSkills`.
    (0x04da, BROADCAST_CHAT_TYPE, Arm::Fmt("You have too many credits invested in specialized skills already! Before you can specialize your %s skill, you will need to unspecialize some other skill.\n")),
    // `WeenieError::YouHaveFailedToAlterAttributes`.
    (0x04dd, BROADCAST_CHAT_TYPE, Arm::Lit("You have failed to alter your attributes.\n")),
    // `WeenieErrorWithString::AttributeTransferFromTooLow`.
    (0x04de, BROADCAST_CHAT_TYPE, Arm::Suffix("\n")),
    // `WeenieErrorWithString::AttributeTransferToTooHigh`.
    (0x04df, BROADCAST_CHAT_TYPE, Arm::Suffix("\n")),
    // `WeenieError::CannotTransferAttributesWhileWieldingItem`.
    (0x04e0, BROADCAST_CHAT_TYPE, Arm::Lit("You are currently wielding items which require a certain level of skill. Your attributes cannot be transferred while you are wielding these items. Please remove these items and try again.\n")),
    // `WeenieError::YouHaveSucceededTransferringAttributes`.
    (0x04e1, BROADCAST_CHAT_TYPE, Arm::Lit("You have succeeded in transferring your attributes!\n")),
    // `WeenieError::HookIsDuplicated`.
    (0x04e2, BROADCAST_CHAT_TYPE, Arm::Lit("This hook is a duplicated housing object. You may not add items to a duplicated housing object. Please empty the hook and allow it to reset.\n")),
    // `WeenieError::ItemIsWrongTypeForHook`.
    (0x04e3, BROADCAST_CHAT_TYPE, Arm::Lit("That item is of the wrong type to be placed on this hook.\n")),
    // `WeenieError::HousingChestIsDuplicated`.
    (0x04e4, BROADCAST_CHAT_TYPE, Arm::Lit("This chest is a duplicated housing object. You may not add items to a duplicated housing object. Please empty everything -- including backpacks -- out of the chest and allow the chest to reset.\n")),
    // `WeenieError::HookWillBeDeleted`.
    (0x04e5, BROADCAST_CHAT_TYPE, Arm::Lit("This hook was a duplicated housing object. Since it is now empty, it will be deleted momentarily. Once it is gone, it is safe to use the other, non-duplicated hook that is here.\n")),
    // `WeenieError::HousingChestWillBeDeleted`.
    (0x04e6, BROADCAST_CHAT_TYPE, Arm::Lit("This chest was a duplicated housing object. Since it is now empty, it will be deleted momentarily. Once it is gone, it is safe to use the other, non-duplicated chest that is here.\n")),
    // `WeenieError::CannotSwearAllegianceWhileOwningMansion`.
    (0x04e7, BROADCAST_CHAT_TYPE, Arm::Lit("You cannot swear allegiance to anyone because you own a monarch-only house. Please abandon your house and try again.\n")),
    // `WeenieErrorWithString::ItemUnusableOnHook_CannotOpen`.
    (0x04e8, BROADCAST_CHAT_TYPE, Arm::Fmt("The %s cannot be used while on a hook and only the owner may open the hook.\n")),
    // `WeenieErrorWithString::ItemUnusableOnHook_CanOpen`.
    (0x04e9, BROADCAST_CHAT_TYPE, Arm::Fmt("The %s cannot be used while on a hook, use the '@house hooks on' command to make the hook openable.\n")),
    // `WeenieErrorWithString::ItemOnlyUsableOnHook`.
    (0x04ea, BROADCAST_CHAT_TYPE, Arm::Fmt("The %s can only be used while on a hook.\n")),
    // `WeenieError::YouCantDoThatWhileInTheAir`.
    (0x04eb, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You can't do that while in the air!")),
    // `WeenieError::CannotChangePKStatusWhileRecovering`.
    (0x04ec, BROADCAST_CHAT_TYPE, Arm::Lit("You cannot modify your player killer status while you are recovering from a PK death.\n")),
    // `WeenieError::AdvocatesCannotChangePKStatus`.
    (0x04ed, BROADCAST_CHAT_TYPE, Arm::Lit("Advocates may not change their player killer status!\n")),
    // `WeenieError::LevelTooLowToChangePKStatusWithObject`.
    (0x04ee, BROADCAST_CHAT_TYPE, Arm::Lit("Your level is too low to change your player killer status with this object.\n")),
    // `WeenieError::LevelTooHighToChangePKStatusWithObject`.
    (0x04ef, BROADCAST_CHAT_TYPE, Arm::Lit("Your level is too high to change your player killer status with this object.\n")),
    // `WeenieError::YouFeelAHarshDissonance`.
    (0x04f0, BROADCAST_CHAT_TYPE, Arm::Lit("You feel a harsh dissonance, and you sense that an act of killing you have committed recently is interfering with the conversion.\n")),
    // `WeenieError::YouArePKAgain`.
    (0x04f1, BROADCAST_CHAT_TYPE, Arm::Lit("Bael'Zharon's power flows through you again. You are once more a player killer.\n")),
    // `WeenieError::YouAreTemporarilyNoLongerPK`.
    (0x04f2, BROADCAST_CHAT_TYPE, Arm::Lit("Bael'Zharon has granted you respite after your moment of weakness. You are temporarily no longer a player killer.\n")),
    // `WeenieError::PKLiteMayNotUsePortal`.
    (0x04f3, MAGIC_CHAT_TYPE, Arm::Lit("Lite Player Killers may not interact with that portal!\n")),
    // `WeenieErrorWithString::_FailsToAffectYou_TheyCannotAffectAnyone`.
    (0x04f4, MAGIC_CHAT_TYPE, Arm::Fmt("%s fails to affect you because $s cannot affect anyone!\n")),
    // `WeenieErrorWithString::_FailsToAffectYou_YouCannotBeHarmed`.
    (0x04f5, MAGIC_CHAT_TYPE, Arm::Fmt("%s fails to affect you because you cannot be harmed!\n")),
    // `WeenieErrorWithString::_FailsToAffectYou_TheyAreNotPK`.
    (0x04f6, MAGIC_CHAT_TYPE, Arm::Fmt("%s fails to affect you because %s is not a player killer!\n")),
    // `WeenieErrorWithString::_FailsToAffectYou_YouAreNotPK`.
    (0x04f7, MAGIC_CHAT_TYPE, Arm::Suffix(" fails to affect you because you are not a player killer!\n")),
    // `WeenieErrorWithString::_FailsToAffectYou_NotSamePKType`.
    (0x04f8, MAGIC_CHAT_TYPE, Arm::SuffixMid(" fails to affect you because you are not the same sort of player killer as ", "!\n")),
    // `WeenieErrorWithString::_FailsToAffectYouAcrossHouseBoundary`.
    (0x04f9, MAGIC_CHAT_TYPE, Arm::Suffix(" fails to affect you across a house boundary!\n")),
    // `WeenieErrorWithString::_IsAnInvalidTarget`.
    (0x04fa, MAGIC_CHAT_TYPE, Arm::Suffix(" is an invalid target.\n")),
    // `WeenieErrorWithString::YouAreInvalidTargetForSpellOf_`.
    (0x04fb, MAGIC_CHAT_TYPE, Arm::Fmt("You are an invalid target for the spell of %s.\n")),
    // `WeenieError::YouArentTrainedInHealing`.
    (0x04fc, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You aren't trained in healing!")),
    // `WeenieError::YouDontOwnThatHealingKit`.
    (0x04fd, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You don't own that healing kit!")),
    // `WeenieError::YouCantHealThat`.
    (0x04fe, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You can't heal that!")),
    // `WeenieErrorWithString::_IsAtFullHealth`.
    (0x04ff, LOCAL_ERROR_CHAT_TYPE, Arm::Suffix(" is already at full health!")),
    // `WeenieError::YouArentReadyToHeal`.
    (0x0500, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You aren't ready to heal!")),
    // `WeenieError::YouCanOnlyHealPlayers`.
    (0x0501, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You can only use Healing Kits on player characters.")),
    // `WeenieError::LifestoneMagicProtectsYou`.
    (0x0502, MAGIC_CHAT_TYPE, Arm::Lit("The Lifestone's magic protects you from the attack!\n")),
    // `WeenieError::PortalEnergyProtectsYou`.
    (0x0503, MAGIC_CHAT_TYPE, Arm::Lit("The portal's residual energy protects you from the attack!\n")),
    // `WeenieError::YouAreNonPKAgain`.
    (0x0504, BROADCAST_CHAT_TYPE, Arm::Lit("You are enveloped in a feeling of warmth as you are brought back into the protection of the Light. You are once again a Non-Player Killer.\n")),
    // `WeenieError::YoureTooCloseToYourSanctuary`.
    (0x0505, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You're too close to your sanctuary!")),
    // `WeenieError::CantDoThatTradeInProgress`.
    (0x0506, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You can't do that -- you're trading!")),
    // `WeenieError::OnlyNonPKsMayEnterPKLite`.
    (0x0507, BROADCAST_CHAT_TYPE, Arm::Lit("Only Non-Player Killers may enter PK Lite. Please see @help pklite for more details about this command.\n")),
    // `WeenieError::YouAreNowPKLite`.
    (0x0508, BROADCAST_CHAT_TYPE, Arm::Lit("A cold wind touches your heart. You are now a Player Killer Lite.\n")),
    // `WeenieErrorWithString::_HasNoSpellTargets`.
    (0x0509, MAGIC_CHAT_TYPE, Arm::Suffix(" has no appropriate targets equipped for this spell.\n")),
    // `WeenieErrorWithString::YouHaveNoTargetsForSpellOf_`.
    (0x050a, MAGIC_CHAT_TYPE, Arm::Fmt("You have no appropriate targets equipped for %s's spell.\n")),
    // `WeenieErrorWithString::_IsNowOpenFellowship`.
    (0x050b, BROADCAST_CHAT_TYPE, Arm::Suffix(" is now an open fellowship; anyone may recruit new members.\n")),
    // `WeenieErrorWithString::_IsNowClosedFellowship`.
    (0x050c, BROADCAST_CHAT_TYPE, Arm::Suffix(" is now a closed fellowship.\n")),
    // `WeenieErrorWithString::_IsNowLeaderOfFellowship`.
    (0x050d, BROADCAST_CHAT_TYPE, Arm::Suffix(" is now the leader of this fellowship.\n")),
    // `WeenieErrorWithString::YouHavePassedFellowshipLeadershipTo_`.
    (0x050e, BROADCAST_CHAT_TYPE, Arm::Fmt("You have passed leadership of the fellowship to %s\n")),
    // `WeenieError::YouDoNotBelongToAFellowship`.
    (0x050f, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You do not belong to a Fellowship.")),
    // `WeenieErrorWithString::MaxNumberOf_Hooked`.
    (0x0510, BROADCAST_CHAT_TYPE, Arm::Fmt("You may not hook any more %s on your house.  You already have the maximum number of %s hooked or you are not permitted to hook any on your type of house.\n")),
    // `WeenieError::YouAreNowUsingMaxHooks`.
    (0x0512, BROADCAST_CHAT_TYPE, Arm::Lit("You are now using the maximum number of hooks.  You cannot use another hook until you take an item off one of your hooks.\n")),
    // `WeenieError::YouAreNoLongerUsingMaxHooks`.
    (0x0513, BROADCAST_CHAT_TYPE, Arm::Lit("You are no longer using the maximum number of hooks.  You may again add items to your hooks.\n")),
    // `WeenieErrorWithString::MaxNumberOf_HookedUntilOneIsRemoved`.
    (0x0514, BROADCAST_CHAT_TYPE, Arm::Fmt("You now have the maximum number of %s hooked.  You cannot hook any additional %s until you remove one or more from your house.\n")),
    // `WeenieErrorWithString::NoLongerMaxNumberOf_Hooked`.
    (0x0515, BROADCAST_CHAT_TYPE, Arm::Fmt("You no longer have the maximum number of %s hooked.  You may hook additional %s.\n")),
    // `WeenieError::YouAreNotPermittedToUseThatHook`.
    (0x0516, BROADCAST_CHAT_TYPE, Arm::Lit("You are not permitted to use that hook.\n")),
    // `WeenieErrorWithString::_IsNotCloseEnoughToYourLevel`.
    (0x0517, BROADCAST_CHAT_TYPE, Arm::Suffix(" is not close enough to your level.\n")),
    // `WeenieErrorWithString::LockedFellowshipCannotRecruit_`.
    (0x0518, BROADCAST_CHAT_TYPE, Arm::Wrap("This fellowship is locked; ", " cannot be recruited into the fellowship.\n")),
    // `WeenieError::LockedFellowshipCannotRecruitYou`.
    (0x0519, BROADCAST_CHAT_TYPE, Arm::Lit("The fellowship is locked, you were not added to the fellowship.\n")),
    // `WeenieError::ActivationNotAllowedNotOwner`.
    (0x051a, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("Only the original owner may use that item's magic.")),
    // `WeenieErrorWithString::YouHaveEnteredThe_Channel`.
    (0x051b, BROADCAST_CHAT_TYPE, Arm::Fmt("You have entered the %s channel.\n")),
    // `WeenieErrorWithString::YouHaveLeftThe_Channel`.
    (0x051c, BROADCAST_CHAT_TYPE, Arm::Fmt("You have left the %s channel.\n")),
    // `WeenieErrorWithString::_WillNotReceiveMessage`.
    (0x051e, BROADCAST_CHAT_TYPE, Arm::Suffix(" will not receive your message, please use urgent assistance to speak with an in-game representative\n")),
    // `WeenieErrorWithString::MessageBlocked_`.
    (0x051f, LOCAL_ERROR_CHAT_TYPE, Arm::Fmt("Message Blocked: %s")),
    // `WeenieError::YouCannotAddPeopleToHearList`.
    (0x0520, BROADCAST_CHAT_TYPE, Arm::Lit("You cannot add anymore people to the list of players that you can hear.\n")),
    // `WeenieErrorWithString::_HasBeenAddedToHearList`.
    (0x0521, BROADCAST_CHAT_TYPE, Arm::Suffix(" has been added to the list of people you can hear.\n")),
    // `WeenieErrorWithString::_HasBeenRemovedFromHearList`.
    (0x0522, BROADCAST_CHAT_TYPE, Arm::Suffix(" has been removed from the list of people you can hear.\n")),
    // `WeenieError::YouAreNowDeafTo_Screams`.
    (0x0523, BROADCAST_CHAT_TYPE, Arm::Lit("You are now deaf to player's screams.\n")),
    // `WeenieError::YouCanHearAllPlayersOnceAgain`.
    (0x0524, BROADCAST_CHAT_TYPE, Arm::Lit("You can hear all players once again.\n")),
    // `WeenieErrorWithString::FailToRemove_FromLoudList`.
    (0x0525, BROADCAST_CHAT_TYPE, Arm::Fmt("You fail to remove %s from your loud list.\n")),
    // `WeenieError::YouChickenOut`.
    (0x0526, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You chicken out.")),
    // `WeenieError::YouCanPossiblySucceed`.
    (0x0527, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You cannot posssibly succeed.")),
    // `WeenieError::FellowshipIsLocked`.
    (0x0528, BROADCAST_CHAT_TYPE, Arm::Lit("The fellowship is locked; you cannot open locked fellowships.\n")),
    // `WeenieError::TradeComplete`.
    (0x0529, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("Trade Complete!")),
    // `WeenieError::NotASalvageTool`.
    (0x052a, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("That is not a salvaging tool.")),
    // `WeenieError::CharacterNotAvailable`.
    (0x052b, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("That person is not available now.")),
    // `WeenieErrorWithString::YouAreNowSnoopingOn_`.
    (0x052c, BROADCAST_CHAT_TYPE, Arm::Fmt("You are now snooping on %s.\n")),
    // `WeenieErrorWithString::YouAreNoLongerSnoopingOn_`.
    (0x052d, BROADCAST_CHAT_TYPE, Arm::Fmt("You are no longer snooping on %s.\n")),
    // `WeenieErrorWithString::YouFailToSnoopOn_`.
    (0x052e, BROADCAST_CHAT_TYPE, Arm::Fmt("You fail to snoop on %s.\n")),
    // `WeenieErrorWithString::_AttemptedToSnoopOnYou`.
    (0x052f, BROADCAST_CHAT_TYPE, Arm::Fmt("%s attempted to snoop on you.\n")),
    // `WeenieErrorWithString::_IsAlreadyBeingSnoopedOn`.
    (0x0530, BROADCAST_CHAT_TYPE, Arm::Fmt("%s is already being snooped on, only one person may snoop on another at a time.\n")),
    // `WeenieErrorWithString::_IsInLimbo`.
    (0x0531, BROADCAST_CHAT_TYPE, Arm::Fmt("%s is in limbo and cannot receive your message.\n")),
    // `WeenieError::YouMustWaitToPurchaseHouse`.
    (0x0532, BROADCAST_CHAT_TYPE, Arm::Lit("You must wait 30 days after purchasing a house before you may purchase another with any character on the same account. This applies to all housing except apartments.\n")),
    // `WeenieErrorWithString::YouHaveBeenBootedFromAllegianceChat`.
    (0x0533, BROADCAST_CHAT_TYPE, Arm::Fmt("You have been booted from your allegiance chat room. Use \"@allegiance chat on\" to rejoin. (%s).\n")),
    // `WeenieErrorWithString::_HasBeenBootedFromAllegianceChat`.
    (0x0534, BROADCAST_CHAT_TYPE, Arm::Fmt("%s has been booted from the allegiance chat room.\n")),
    // `WeenieError::YouDoNotHaveAuthorityInAllegiance`.
    (0x0535, BROADCAST_CHAT_TYPE, Arm::Lit("You do not have the authority within your allegiance to do that.\n")),
    // `WeenieErrorWithString::AccountOf_IsAlreadyBannedFromAllegiance`.
    (0x0536, BROADCAST_CHAT_TYPE, Arm::Fmt("The account of %s is already banned from the allegiance.\n")),
    // `WeenieErrorWithString::AccountOf_IsNotBannedFromAllegiance`.
    (0x0537, BROADCAST_CHAT_TYPE, Arm::Fmt("The account of %s is not banned from the allegiance.\n")),
    // `WeenieErrorWithString::AccountOf_WasNotUnbannedFromAllegiance`.
    (0x0538, BROADCAST_CHAT_TYPE, Arm::Fmt("The account of %s was not unbanned from the allegiance.\n")),
    // `WeenieErrorWithString::AccountOf_IsBannedFromAllegiance`.
    (0x0539, BROADCAST_CHAT_TYPE, Arm::Fmt("The account of %s has been banned from the allegiance.\n")),
    // `WeenieErrorWithString::AccountOf_IsUnbannedFromAllegiance`.
    (0x053a, BROADCAST_CHAT_TYPE, Arm::Fmt("The account of %s is no longer banned from the allegiance.\n")),
    // `WeenieErrorWithString::ListOfBannedCharacters`.
    (0x053b, BROADCAST_CHAT_TYPE, Arm::Lit("Banned Characters: ")),
    // `WeenieErrorWithString::_IsBannedFromAllegiance`.
    (0x053e, BROADCAST_CHAT_TYPE, Arm::Fmt("%s is banned from the allegiance!\n")),
    // `WeenieErrorWithString::YouAreBannedFromAllegiance`.
    (0x053f, BROADCAST_CHAT_TYPE, Arm::Fmt("You are banned from %s's allegiance!\n")),
    // `WeenieError::YouHaveMaxAccountsBanned`.
    (0x0540, BROADCAST_CHAT_TYPE, Arm::Lit("You have the maximum number of accounts banned.!\n")),
    // `WeenieErrorWithString::_IsNowAllegianceOfficer`.
    (0x0541, BROADCAST_CHAT_TYPE, Arm::Fmt("%s is now an allegiance officer.\n")),
    // `WeenieErrorWithString::ErrorSetting_AsAllegianceOfficer`.
    (0x0542, BROADCAST_CHAT_TYPE, Arm::Fmt("An unspecified error occurred while attempting to set %s as an allegiance officer.\n")),
    // `WeenieErrorWithString::_IsNoLongerAllegianceOfficer`.
    (0x0543, BROADCAST_CHAT_TYPE, Arm::Fmt("%s is no longer an allegiance officer.\n")),
    // `WeenieErrorWithString::ErrorRemoving_AsAllegianceOFficer`.
    (0x0544, BROADCAST_CHAT_TYPE, Arm::Fmt("An unspecified error occurred while attempting to remove %s as an allegiance officer.\n")),
    // `WeenieError::YouHaveMaxAllegianceOfficers`.
    (0x0545, BROADCAST_CHAT_TYPE, Arm::Lit("You already have the maximum number of allegiance officers. You must remove some before you add any more.\n")),
    // `WeenieError::YourAllegianceOfficersHaveBeenCleared`.
    (0x0546, BROADCAST_CHAT_TYPE, Arm::Lit("Your allegiance officers have been cleared.\n")),
    // `WeenieErrorWithString::YouMustWait_BeforeCommunicating`.
    (0x0547, BROADCAST_CHAT_TYPE, Arm::Fmt("You must wait %s before communicating again!\n")),
    // `WeenieError::YouCannotJoinChannelsWhileGagged`.
    (0x0548, BROADCAST_CHAT_TYPE, Arm::Lit("You cannot join any chat channels while gagged.\n")),
    // `WeenieErrorWithString::YourAllegianceOfficerStatusChanged`.
    (0x0549, BROADCAST_CHAT_TYPE, Arm::Fmt("Your allegiance officer status has been modified. You now hold the position of: %s.\n")),
    // `WeenieError::YouAreNoLongerAllegianceOfficer`.
    (0x054a, BROADCAST_CHAT_TYPE, Arm::Lit("You are no longer an allegiance officer.\n")),
    // `WeenieErrorWithString::_IsAlreadyAllegianceOfficerOfThatLevel`.
    (0x054b, BROADCAST_CHAT_TYPE, Arm::Fmt("%s is already an allegiance officer of that level.\n")),
    // `WeenieError::YourAllegianceDoesNotHaveHometown`.
    (0x054c, BROADCAST_CHAT_TYPE, Arm::Lit("Your allegiance does not have a hometown.\n")),
    // `WeenieErrorWithString::The_IsCurrentlyInUse`.
    (0x054d, LOCAL_ERROR_CHAT_TYPE, Arm::Fmt("The %s is currently in use.\n")),
    // `WeenieError::HookItemNotUsable_CannotOpen`.
    (0x054e, BROADCAST_CHAT_TYPE, Arm::Lit("The hook does not contain a usable item. You cannot open the hook because you do not own the house to which it belongs.\n")),
    // `WeenieError::HookItemNotUsable_CanOpen`.
    (0x054f, BROADCAST_CHAT_TYPE, Arm::Lit("The hook does not contain a usable item. Use the '@house hooks on'command to make the hook openable.\n")),
    // `WeenieError::MissileOutOfRange`.
    (0x0550, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("Out of Range!")),
    // `WeenieErrorWithString::YouAreNotListeningTo_Channel`.
    (0x0551, BROADCAST_CHAT_TYPE, Arm::Fmt("You are not listening to the %s channel!\n")),
    // `WeenieError::MustPurchaseThroneOfDestinyToUseFunction`.
    (0x0552, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You must purchase Asheron's Call -- Throne of Destiny to use this function.")),
    // `WeenieError::MustPurchaseThroneOfDestinyToUseItem`.
    (0x0553, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You must purchase Asheron's Call -- Throne of Destiny to use this item.")),
    // `WeenieError::MustPurchaseThroneOfDestinyToUsePortal`.
    (0x0554, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You must purchase Asheron's Call -- Throne of Destiny to use this portal.")),
    // `WeenieError::MustPurchaseThroneOfDestinyToAccessQuest`.
    (0x0555, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You must purchase Asheron's Call -- Throne of Destiny to access this quest.")),
    // `WeenieError::YouFailedToCompleteAugmentation`.
    (0x0556, BROADCAST_CHAT_TYPE, Arm::Lit("You have failed to complete the augmentation.\n")),
    // `WeenieError::AugmentationUsedTooManyTimes`.
    (0x0557, BROADCAST_CHAT_TYPE, Arm::Lit("You have used this augmentation too many times already.\n")),
    // `WeenieError::AugmentationTypeUsedTooManyTimes`.
    (0x0558, BROADCAST_CHAT_TYPE, Arm::Lit("You have used augmentations of this type too many times already.\n")),
    // `WeenieError::AugmentationNotEnoughExperience`.
    (0x0559, BROADCAST_CHAT_TYPE, Arm::Lit("You do not have enough unspent experience available to purchase this augmentation.\n")),
    // `WeenieErrorWithString::AugmentationSkillNotTrained`.
    (0x055a, BROADCAST_CHAT_TYPE, Arm::Fmt("%s\n")),
    // `WeenieErrorWithString::YouSuccededAcquiringAugmentation`.
    (0x055b, BROADCAST_CHAT_TYPE, Arm::Fmt("Congratulations! You have succeeded in acquiring the %s augmentation.\n")),
    // `WeenieErrorWithString::YouSucceededRecoveringXPFromSkill_AugmentationNotUntrainable`.
    (0x055c, BROADCAST_CHAT_TYPE, Arm::Fmt("Although your augmentation will not allow you to untrain your %s skill, you have succeeded in recovering all the experience you had invested in it.\n")),
    // `WeenieError::ExitTrainingAcademyToUseCommand`.
    (0x055d, BROADCAST_CHAT_TYPE, Arm::Lit("You must exit the Training Academy before that command will be available to you.\n")),
    // `WeenieErrorWithString::AFK`.
    (0x055e, BROADCAST_CHAT_TYPE, Arm::Text),
    // `WeenieError::OnlyPKsMayUseCommand`.
    (0x055f, BROADCAST_CHAT_TYPE, Arm::Lit("Only Player Killer characters may use this command!\n")),
    // `WeenieError::OnlyPKLiteMayUseCommand`.
    (0x0560, BROADCAST_CHAT_TYPE, Arm::Lit("Only Player Killer Lite characters may use this command!\n")),
    // `WeenieError::MaxFriendsExceeded`.
    (0x0561, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You may only have a maximum of 50 friends at once. If you wish to add more friends, you must first remove some.")),
    // `WeenieErrorWithString::_IsAlreadyOnYourFriendsList`.
    (0x0562, BROADCAST_CHAT_TYPE, Arm::Fmt("%s is already on your friends list!\n")),
    // `WeenieError::ThatCharacterNotOnYourFriendsList`.
    (0x0563, BROADCAST_CHAT_TYPE, Arm::Fmt("That character is not on your friends list!\n")),
    // `WeenieError::OnlyHouseOwnerCanUseCommand`.
    (0x0564, BROADCAST_CHAT_TYPE, Arm::Lit("Only the character who owns the house may use this command.")),
    // `WeenieError::InvalidAllegianceNameCantBeEmpty`.
    (0x0565, BROADCAST_CHAT_TYPE, Arm::Fmt("That allegiance name is invalid because it is empty. Please use the @allegiance name clear command to clear your allegiance name.\n")),
    // `WeenieError::InvalidAllegianceNameTooLong`.
    (0x0566, BROADCAST_CHAT_TYPE, Arm::Fmt("That allegiance name is too long. Please choose another name.\n")),
    // `WeenieError::InvalidAllegianceNameBadCharacters`.
    (0x0567, BROADCAST_CHAT_TYPE, Arm::Fmt("That allegiance name contains illegal characters. Please choose another name using only letters, spaces, - and '.\n")),
    // `WeenieError::InvalidAllegianceNameInappropriate`.
    (0x0568, BROADCAST_CHAT_TYPE, Arm::Fmt("That allegiance name is not appropriate. Please choose another name.\n")),
    // `WeenieError::InvalidAllegianceNameAlreadyInUse`.
    (0x0569, BROADCAST_CHAT_TYPE, Arm::Fmt("That allegiance name is already in use. Please choose another name.\n")),
    // `WeenieErrorWithString::YouMayOnlyChangeAllegianceNameOnceEvery24Hours`.
    (0x056a, BROADCAST_CHAT_TYPE, Arm::Fmt("You may only change your allegiance name once every 24 hours. You may change your allegiance name again in %s.\n")),
    // `WeenieError::AllegianceNameCleared`.
    (0x056b, BROADCAST_CHAT_TYPE, Arm::Fmt("Your allegiance name has been cleared.\n")),
    // `WeenieError::InvalidAllegianceNameSameName`.
    (0x056c, BROADCAST_CHAT_TYPE, Arm::Fmt("That is already the name of your allegiance!\n")),
    // `WeenieErrorWithString::_IsTheMonarchAndCannotBePromotedOrDemoted`.
    (0x056d, BROADCAST_CHAT_TYPE, Arm::Fmt("%s is the monarch and cannot be promoted or demoted.\n")),
    // `WeenieErrorWithString::ThatLevelOfAllegianceOfficerIsNowKnownAs_`.
    (0x056e, BROADCAST_CHAT_TYPE, Arm::Fmt("That level of allegiance officer is now known as: %s.\n")),
    // `WeenieError::InvalidOfficerLevel`.
    (0x056f, BROADCAST_CHAT_TYPE, Arm::Lit("That is an invalid officer level.\n")),
    // `WeenieError::AllegianceOfficerTitleIsNotAppropriate`.
    (0x0570, BROADCAST_CHAT_TYPE, Arm::Lit("That allegiance officer title is not appropriate.\n")),
    // `WeenieError::AllegianceNameIsTooLong`.
    (0x0571, BROADCAST_CHAT_TYPE, Arm::Fmt("That allegiance name is too long. Please choose another name.\n")),
    // `WeenieError::AllegianceOfficerTitlesCleared`.
    (0x0572, BROADCAST_CHAT_TYPE, Arm::Fmt("All of your allegiance officer titles have been cleared.\n")),
    // `WeenieError::AllegianceTitleHasIllegalChars`.
    (0x0573, BROADCAST_CHAT_TYPE, Arm::Fmt("That allegiance title contains illegal characters. Please choose another name using only letters, spaces, - and '.\n")),
    // `WeenieErrorWithString::YourAllegianceIsCurrently_`.
    (0x0574, BROADCAST_CHAT_TYPE, Arm::Fmt("Your allegiance is currently: %s.\n")),
    // `WeenieErrorWithString::YourAllegianceIsNow_`.
    (0x0575, BROADCAST_CHAT_TYPE, Arm::Fmt("Your allegiance is now: %s.\n")),
    // `WeenieErrorWithString::YouCannotAcceptAllegiance_YourAllegianceIsLocked`.
    (0x0576, BROADCAST_CHAT_TYPE, Arm::Fmt("You may not accept the offer of allegiance from %s because your allegiance is locked.\n")),
    // `WeenieErrorWithString::YouCannotSwearAllegiance_AllegianceOf_IsLocked`.
    (0x0577, BROADCAST_CHAT_TYPE, Arm::Fmt("You may not swear allegiance at this time because the allegiance of %s is locked.\n")),
    // `WeenieErrorWithString::YouHavePreApproved_ToJoinAllegiance`.
    (0x0578, BROADCAST_CHAT_TYPE, Arm::Fmt("You have pre-approved %s to join your allegiance.\n")),
    // `WeenieError::YouHaveNotPreApprovedVassals`.
    (0x0579, BROADCAST_CHAT_TYPE, Arm::Lit("You have not pre-approved any vassals to join your allegiance.\n")),
    // `WeenieErrorWithString::_IsAlreadyMemberOfYourAllegiance`.
    (0x057a, BROADCAST_CHAT_TYPE, Arm::Fmt("%s is already a member of your allegiance!\n")),
    // `WeenieErrorWithString::_HasBeenPreApprovedToJoinYourAllegiance`.
    (0x057b, BROADCAST_CHAT_TYPE, Arm::Fmt("%s has been pre-approved to join your allegiance.\n")),
    // `WeenieError::YouHaveClearedPreApprovedVassal`.
    (0x057c, BROADCAST_CHAT_TYPE, Arm::Lit("You have cleared the pre-approved vassal for your allegiance.\n")),
    // `WeenieError::CharIsAlreadyGagged`.
    (0x057d, BROADCAST_CHAT_TYPE, Arm::Lit("That character is already gagged!\n")),
    // `WeenieError::CharIsNotCurrentlyGagged`.
    (0x057e, BROADCAST_CHAT_TYPE, Arm::Lit("That character is not currently gagged!\n")),
    // `WeenieErrorWithString::YourAllegianceChatPrivilegesRemoved`.
    (0x057f, BROADCAST_CHAT_TYPE, Arm::Fmt("Your allegiance chat privileges have been temporarily removed by %s. Until they are restored, you may not view or speak in the allegiance chat channel.")),
    // `WeenieErrorWithString::_IsTemporarilyGaggedInAllegianceChat`.
    (0x0580, BROADCAST_CHAT_TYPE, Arm::Fmt("%s is now temporarily unable to view or speak in allegiance chat. The gag will run out in 5 minutes, or %s may be explicitly ungagged before then.")),
    // `WeenieError::YourAllegianceChatPrivilegesRestored`.
    (0x0581, BROADCAST_CHAT_TYPE, Arm::Lit("Your allegiance chat privileges have been restored.\n")),
    // `WeenieErrorWithString::YourAllegianceChatPrivilegesRestoredBy_`.
    (0x0582, BROADCAST_CHAT_TYPE, Arm::Fmt("Your allegiance chat privileges have been restored by %s.")),
    // `WeenieErrorWithString::YouRestoreAllegianceChatPrivilegesTo_`.
    (0x0583, BROADCAST_CHAT_TYPE, Arm::Fmt("You have restored allegiance chat privileges to %s.")),
    // `WeenieError::TooManyUniqueItems`.
    (0x0584, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You cannot pick up more of that item!")),
    // `WeenieError::HeritageRequiresSpecificArmor`.
    (0x0585, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("You are restricted to clothes and armor created for your race.")),
    // `WeenieError::ArmorRequiresSpecificHeritage`.
    (0x0586, LOCAL_ERROR_CHAT_TYPE, Arm::Lit("That item was specifically created for another race.")),
    // `WeenieError::OlthoiCannotInteractWithThat`.
    (0x0587, MAGIC_CHAT_TYPE, Arm::Lit("Olthoi cannot interact with that!\n")),
    // `WeenieError::OlthoiCannotUseLifestones`.
    (0x0588, MAGIC_CHAT_TYPE, Arm::Lit("Olthoi cannot use regular lifestones! Asheron would not allow it!\n")),
    // `WeenieError::OlthoiVendorLooksInHorror`.
    (0x0589, MAGIC_CHAT_TYPE, Arm::Lit("The vendor looks at you in horror!\n")),
    // `WeenieErrorWithString::_CowersFromYou`.
    (0x058a, BROADCAST_CHAT_TYPE, Arm::Fmt("%s cowers from you!\n")),
    // `WeenieError::OlthoiCannotJoinFellowship`.
    (0x058b, MAGIC_CHAT_TYPE, Arm::Lit("As a mindless engine of destruction an Olthoi cannot join a fellowship!\n")),
    // `WeenieError::OlthoiCannotJoinAllegiance`.
    (0x058c, MAGIC_CHAT_TYPE, Arm::Lit("The Olthoi only have an allegiance to the Olthoi Queen!\n")),
    // `WeenieError::YouCannotUseThatItem`.
    (0x058d, MAGIC_CHAT_TYPE, Arm::Lit("You cannot use that item!\n")),
    // `WeenieError::ThisPersonWillNotInteractWithYou`.
    (0x058e, MAGIC_CHAT_TYPE, Arm::Lit("This person will not interact with you!\n")),
    // `WeenieError::OnlyOlthoiMayUsePortal`.
    (0x058f, MAGIC_CHAT_TYPE, Arm::Lit("Only Olthoi may pass through this portal!\n")),
    // `WeenieError::OlthoiMayNotUsePortal`.
    (0x0590, MAGIC_CHAT_TYPE, Arm::Lit("Olthoi may not pass through this portal!\n")),
    // `WeenieError::YouMayNotUsePortalWithVitae`.
    (0x0591, MAGIC_CHAT_TYPE, Arm::Lit("You may not pass through this portal while Vitae weakens you!\n")),
    // `WeenieError::YouMustBeTwoWeeksOldToUsePortal`.
    (0x0592, MAGIC_CHAT_TYPE, Arm::Lit("This character must be two weeks old or have been created on an account at least two weeks old to use this portal!\n")),
    // `WeenieError::OlthoiCanOnlyRecallToLifestone`.
    (0x0593, MAGIC_CHAT_TYPE, Arm::Lit("Olthoi characters can only use Lifestone and PK Arena recalls!\n")),
];

/// The arm dispatches `error_code` to: its chat type and its
/// composition, or `None` for the miss exit.
///
/// Exposed so a caller that wants the *surface* without the line -- a count, a trace, a test
/// parametrised over the table -- does not have to compose a string to find out.
#[must_use]
pub fn arm_for(error_code: u32) -> Option<(u8, Arm)> {
    let i = ARMS
        .binary_search_by_key(&error_code, |&(code, _, _)| code)
        .ok()?;
    Some((ARMS[i].1, ARMS[i].2))
}

/// The code and the server's `%s` to the line retail draws.
///
/// `None` is a code with no arm, which is retail's epilogue: the local string's refcount is
/// released and the function returns without drawing anything.
#[must_use]
pub fn handle_failure_event(error_code: u32, text: &str) -> Option<ChatMessage> {
    // The scroll gets `(text, <type>, true, 0)` -- the type is the arm's, not the function's.
    let (ty, arm) = arm_for(error_code)?;
    Some(ChatMessage {
        feedback: crate::feedback::Feedback::numeric(error_code),
        ty,
        body: arm.render(text)?,
        prefix: None,
        window: 0,
    })
}
