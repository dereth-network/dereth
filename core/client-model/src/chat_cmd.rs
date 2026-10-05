//! The remaining chat-command handlers — character teleports and
//! queries, the comms toggles and channels, and the consent/permit lists.
//!
//! # Why this file exists
//!
//! The command tables map names and aliases to typed handler meanings. Character queries,
//! communication settings and teleport commands are implemented here; other handlers delegate
//! to their owning models through the runtime's exhaustive command dispatch.
//!
//! The chat entry is the whole production path for every event below. In retail each
//! character, communication, and house-teleport event below has exactly one caller: its
//! corresponding chat-command handler.
//!
//! These handlers query character facts, manage channel membership
//! and global squelch, teleport to the supported destinations, manage player permissions
//! and consent, set away status, and request suicide after confirmation. Local-only
//! commands display derived character information without a
//! request. Each networked command has exactly one request-producing chat entry.
//!
//! # The three shapes, and the `bool` that decides what a refusal looks like
//!
//! The chat dispatcher raises failure event `0x26` — *"That is not a valid command."* —
//! only when a handler returns **false**. Almost every handler below returns `true` even as it
//! refuses, so the player sees the handler's own sentence and nothing else. The exceptions are
//! named on each function.
//!
//! Shape A, "argc must be zero": a non-empty argument list prints
//! *"Please see @help ... for more information on how to use this command."* on chat type `0x1A`
//! at the command's source window, then returns **true**. `@lifestone`, `@marketplace`, `@pkarena`,
//! `@pklarena`, `@pklite`, `@hor`, `@hom` and `@die` all take it.
//!
//! Shape B, "argc must be exactly one": `@chat`, `@notell`,
//! `@clist`, `@on`, `@off`.
//!
//! Shape C, pop a sub-command then a chain of case-insensitive compares: `@afk`, `@consent`, `@permit`.
//!
//! **No datagram leaves this module.** Every send goes into the caller's [`RequestSink`].

use crate::cmd::interp::next_arg;
use crate::{Request, RequestSink, World};

// ---------------------------------------------------------------------------------------------
// Chat types and command literals, taken directly from the client
// ---------------------------------------------------------------------------------------------

/// Chat type `0x1A` at the current command source is the refusal channel every one
/// of these handlers prints on. `0x1A` is `LOCAL_ERROR`, the type the main window's default filter
/// `0xFBFFFFFF` drops and the spew-box strip accepts.
pub const REFUSAL_CHAT_TYPE: u32 = 0x1A;

/// A scroll print on chat type `0`, `Broadcast`, the default green. The
/// acknowledgements and three of the four failure-event arms below all use it.
pub const BROADCAST_CHAT_TYPE: u32 = 0;

/// The lifestone command's wrong-argument help line.
pub const PLEASE_SEE_HELP_LIFESTONE: &str =
    "Please see @help lifestone for more information on how to use this command.";
/// The marketplace command's wrong-argument help line.
pub const PLEASE_SEE_HELP_MARKETPLACE: &str =
    "Please see @help marketplace for more information on how to use this command.";
/// The player-killer arena command's wrong-argument help line.
pub const PLEASE_SEE_HELP_PKARENA: &str =
    "Please see @help pkarena for more information on how to use this command.";
/// The player-killer-lite arena command's wrong-argument help line.
pub const PLEASE_SEE_HELP_PKLARENA: &str =
    "Please see @help pklarena for more information on how to use this command.";
/// The player-killer-lite command's wrong-argument help line.
pub const PLEASE_SEE_HELP_PKLITE: &str =
    "Please see @help pklite for more information on how to use this command.";
/// **One literal shared by the house-recall and house commands.** It has **no trailing full stop**,
/// unlike every one of its five siblings. Transcribed, not tidied.
pub const PLEASE_SEE_HELP_HOUSE: &str =
    "Please see @help House for more information on how to use this command";
/// The suicide command's wrong-argument help line.
pub const PLEASE_SEE_HELP_DIE: &str =
    "Please see @help die for more information on how to use this command.";

/// Failure `0x55F`, the arena command's refusal for a character who is not a player killer. It is
/// printed on chat type 0.
pub const ONLY_PLAYER_KILLERS: &str = "Only Player Killer characters may use this command!\n";
/// Failure `0x560`, the corresponding player-killer-lite refusal, with the same chat-type-0 tail.
pub const ONLY_PLAYER_KILLER_LITES: &str =
    "Only Player Killer Lite characters may use this command!\n";
/// Failure `0x507`, the player-killer-lite command's refusal for a player killer. It uses the same
/// chat-type-0 tail.
pub const ONLY_NON_PLAYER_KILLERS: &str =
    "Only Non-Player Killers may enter PK Lite. Please see @help pklite for more details about this command.\n";
/// Failure `0x422`, the three channel commands' unknown-name refusal. It is the one arm of these
/// four failures printed on chat type `0x1A` rather than 0.
pub const THAT_CHANNEL_DOES_NOT_EXIST: &str = "That channel doesn't exist.";

/// The chat-toggle command's wrong-argument-count refusal.
pub const SPECIFY_CHAT_ON_OR_OFF: &str = "Please specify if you want chat text on or off.";
/// The chat-toggle command's unknown-word refusal.
pub const SPECIFY_ON_OR_OFF: &str = "Please specify on or off.";
/// The tell-toggle command's wrong-argument-count refusal.
pub const SPECIFY_TELLS_ON_OR_OFF: &str = "Please specify if you want tells on or off.";
/// The missing-channel-name refusal shared by the channel-list, channel-on and channel-off
/// commands.
pub const SPECIFY_THE_CHANNEL_NAME: &str = "Please specify the channel name.";

/// The AFK command's default acknowledgement for `msg` with an empty message.
pub const NEW_AFK_MESSAGE_DEFAULT: &str =
    "New AFK message set: I am currently away from the keyboard.";
/// The prefix the non-empty AFK `msg` arm concatenates the text onto.
pub const NEW_AFK_MESSAGE_PREFIX: &str = "New AFK message set: ";
/// `substring(0, 0xBF)`, i.e. 191 characters. The help text says "limited to 192
/// characters", and 191 plus the newline the next step guarantees is that 192.
pub const AFK_MESSAGE_MAX: usize = 0xBF;

/// The acknowledgement used when corpse-looting permissions are enabled; stored narrow and
/// widened before display.
pub const CAN_NOW_ACCEPT_CORPSE_LOOTING: &str =
    "You can now accept corpse looting permissions from other players.\n";
/// The acknowledgement used when corpse-looting permissions are disabled. This one goes through
/// the **narrow** text-display overload, rather than the wide one.
pub const NO_LONGER_ACCEPTING_CORPSE_LOOTING: &str =
    "You are no longer accepting corpse looting permissions from other players.\n";
/// The refusal for `@consent remove` with no name.
pub const SPECIFY_PERSON_TO_REMOVE_FROM_CONSENT: &str =
    "Please specify a person to remove from your consent list.";
/// The refusal for `@consent <anything else>`.
pub const SPECIFY_VALID_CONSENT_COMMAND: &str = "Please specify a valid consent command.";
/// The refusal for `@permit <anything but add/remove>`.
pub const SPECIFY_VALID_PERMIT_COMMAND: &str = "Please specify a valid permit command.";
/// The refusal for `@permit add` or `@permit remove` with no name.
pub const SPECIFY_PERSON_FOR_PERMIT: &str = "Please specify a person for the permit command.";

/// The client's empty-line refusal, printed on chat type `0x1A` in the current command source.
///
/// **The emote handler has no equivalent refusal** and does not trim: two adjacent handlers give
/// different answers to an empty line. See `dereth_client_model::chat::do_emote`.
pub const YOU_MUST_SPECIFY_TEXT_TO_SAY: &str = "You must specify the text you wish to say!";

/// The complete retired-command refusal.
pub const SPEAKER_RETIRED: &str =
    "This command is no longer in use, please see @allegiance officer.\n";

/// The suicide command's callback-dialog prompt (property `0xC5`), with property `0x8E` set to
/// enum 1 for a Yes/No dialog.
pub const DIE_CONFIRMATION: &str =
    "Do you really want to kill your character? You may drop items and accrue a vitae penalty.";

/// The message-type operand for `@chat`'s global-squelch event is literal 2.
pub const GLOBAL_SQUELCH_TYPE_CHAT: u32 = 2;
/// The same operand for `@notell` — literal 3.
pub const GLOBAL_SQUELCH_TYPE_TELL: u32 = 3;

/// Boolean quality `0x6E` (`Afk`), read before the AFK command
/// decides whether to send anything at all.
pub const AFK_BOOL_PROPERTY: u32 = 0x6E;

/// The client's option-changed ordinal: **16**, `AcceptLootPermits`, bit `0x00080000`
/// of the first option word. It is one of the twenty-one auto-saved options, so the write
/// leaves immediately as a `0x0005 Character_PlayerOptionChangedEvent`.
pub const ACCEPT_LOOT_PERMITS_ORDINAL: usize = 16;

/// The endurance command's entire display body: one literal and one text-display call. Transcribed
/// verbatim, newlines included.
pub const ENDURANCE_TEXT: &str = "The endurance attribute has a number of abilities tied to it.\nFirst, some combination of strength and endurance (with endurance being more important) now allows one to regenerate hit points at a faster rate the higher one's endurance is.  This bonus is in addition to any regeneration spells one may have placed upon themselves.  This endurance regeneration bonus caps at around 110%.\nSecond, the higher a player's Endurance, the less stamina one uses while attacking.  This benefit is tied to Endurance only, and it caps out at around 50% less stamina used per attack.  The minimum stamina used per attack remains one.\nThird, the higher a player's Endurance, the more likely they are not to use a point of stamina to successfully evade a missile or melee attack.  A player is required to have Melee Defense for melee attacks or Missile Defense for missile attacks trained or specialized in order for this specific ability to work.  This benefit is tied to Endurance only, and it caps out at around a 75% chance to avoid losing a point of stamina per successful evasion.\nFourth, some combination of strength and endurance (the two are roughly of equivalent importance) now allows one to partially resist drain and harm attacks, up to a maximum of roughly 50%.\nFifth, some combination of strength and endurance (the two are roughly of equivalent importance) now allows one to have a level of \"natural resistances\" to the 7 damage types, the same as a certain level of life protections.  This caps out at a 50% resistance (the equivalent to level 5 life prots) to these damage types.  This resistance is not additive to life protections: higher level life protections will overwrite these natural resistances, although life vulns will take these natural resistances into account, if the player does not have a higher level life protection cast upon him.\nThe natural resistances, drain resistances, and regeneration rate info are now visible on the Character Information Panel, in what was once the Burden panel.  This panel now displays the above three Endurance benefits, the burden info, as well as information about your age, birth date, and number of deaths.\nThe 5 categories for the endurance benefits are, in order from lowest benefit to highest: Poor, Mediocre, Hardy, Resilient, and Indomitable, with each range of benefits divided up equally amongst the 5 (e.g. Poor describes having anywhere from 1-10% resistance against drain health attacks, etc.).\n";

// ---------------------------------------------------------------------------------------------
// What a handler decided
// ---------------------------------------------------------------------------------------------

/// One handler's whole visible effect, apart from the requests it has already put in the sink.
///
/// The same shape [`crate::allegiance_cmd::AllegianceCommand`] uses, for the same reason: the
/// window id is the command's source window, which lives in the client's `Interaction` and not in
/// the model, and `PlayerSystem::set_option` needs a clock this module does not have.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChatCommand {
    /// Scroll prints `(text, ty)` at the command's source window, in print order.
    pub lines: Vec<(String, u32)>,
    /// Scroll prints `(text, ty)` at window **0**, which is what every
    /// failure-event arm passes. Kept apart from [`Self::lines`] because the asymmetry is
    /// retail's and a test that cannot see it cannot catch a regression in it.
    pub failure_lines: Vec<(String, u32)>,
    /// The `bool` the handler returns. `false` is the command dispatcher's failure event `0x26`.
    pub handled: bool,
    /// How many requests went into the [`RequestSink`].
    pub sent: u32,
    /// A player-option change the caller must apply through `PlayerSystem::set_option`.
    pub option: Option<(usize, bool)>,
    /// Create the current-UI callback dialog for the death prompt.
    /// Set by `@die` and nothing else.
    pub die_confirmation: Option<&'static str>,
    /// The current-UI callback dialog with the first house-abandon callback — the
    /// **first** of `@house abandon`'s two prompts. Kept apart from
    /// [`Self::die_confirmation`] because the two callbacks are different functions and the
    /// abandon one raises a *second* question rather than sending.
    pub house_abandon_confirmation: Option<&'static str>,
}

impl ChatCommand {
    /// A handler that did its work and returned `true` with nothing to print.
    fn done() -> Self {
        Self {
            handled: true,
            ..Self::default()
        }
    }

    /// A handler that returned `false`, so the command dispatcher adds *"That is not a valid command."*
    fn refused() -> Self {
        Self::default()
    }

    /// Print `(text, ty)` at the command's source window and `return true`.
    fn say(text: impl Into<String>, ty: u32) -> Self {
        Self {
            lines: vec![(text.into(), ty)],
            handled: true,
            ..Self::default()
        }
    }

    /// Raise the failure event with an empty string and `return true` — window **0**.
    fn fail(text: impl Into<String>, ty: u32) -> Self {
        Self {
            failure_lines: vec![(text.into(), ty)],
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

/// `_stricmp(a, b) == 0`, which is what every sub-command chain in this file tests.
fn eq(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

// ---------------------------------------------------------------------------------------------
// Family 1 — the character teleports, queries, PK-lite and suicide
// ---------------------------------------------------------------------------------------------

impl World {
    /// Handle `@lifestone`, `@lif`, and `@ls`.
    ///
    /// ```text
    ///   zero arguments -> send event 0x0063 (teleport to the lifestone; empty body), return TRUE
    ///   otherwise      -> print "Please see @help lifestone ..." on 0x1A at the source window
    ///                     and return TRUE — no "not a valid command"
    /// ```
    pub fn do_lifestone(&mut self, req: &mut dyn RequestSink, args: &[String]) -> ChatCommand {
        if !args.is_empty() {
            return ChatCommand::say(PLEASE_SEE_HELP_LIFESTONE, REFUSAL_CHAT_TYPE);
        }
        req.send(Request::TeleToLifestone(
            dereth_protocol::combat::CharacterTeleToLifestone,
        ));
        ChatCommand::sent()
    }

    /// Behavior: `@marketplace`, `@mar`, `@mp`. `do_lifestone`'s function with
    /// two operands changed: the literal and `Request::TeleToMarketplace` (`0x028D`).
    pub fn do_marketplace(&mut self, req: &mut dyn RequestSink, args: &[String]) -> ChatCommand {
        if !args.is_empty() {
            return ChatCommand::say(PLEASE_SEE_HELP_MARKETPLACE, REFUSAL_CHAT_TYPE);
        }
        req.send(Request::TeleToMarketplace(
            dereth_protocol::combat::CharacterTeleToMarketplace,
        ));
        ChatCommand::sent()
    }

    /// `do_house_recall` — `@hor`, `@hr`.
    ///
    /// **It does not force a true return on the send path.** The tail returns the *event's* answer,
    /// so a send the network layer refused
    /// would reach the command dispatcher's failure event `0x26`. `do_mansion_recall`, `do_age`,
    /// `do_birth` and `do_channel_index` are the same; the six shape-A handlers around them all
    /// force `true`. Modelled as `handled: true` because a send cannot fail in this build, and
    /// named here so the next reader does not think it was missed.
    pub fn do_house_recall(&mut self, req: &mut dyn RequestSink, args: &[String]) -> ChatCommand {
        if !args.is_empty() {
            return ChatCommand::say(PLEASE_SEE_HELP_HOUSE, REFUSAL_CHAT_TYPE);
        }
        req.send(Request::TeleToHouse(
            dereth_protocol::trade::HouseTeleToHouse,
        ));
        ChatCommand::sent()
    }

    /// Behavior: `@hom`, `@hoa`. Pushes the **same** literal as
    /// `do_house_recall` and sends `Request::TeleToMansion` (`0x0278`).
    pub fn do_mansion_recall(&mut self, req: &mut dyn RequestSink, args: &[String]) -> ChatCommand {
        if !args.is_empty() {
            return ChatCommand::say(PLEASE_SEE_HELP_HOUSE, REFUSAL_CHAT_TYPE);
        }
        req.send(Request::TeleToMansion(
            dereth_protocol::trade::HouseTeleToMansion,
        ));
        ChatCommand::sent()
    }

    /// `do_pk_arena` — `@pkarena`, `@pka`.
    ///
    /// ```text
    ///   look up the player's weenie
    ///   no weenie found        -> send teleport-to-PK-arena event 0x0027
    ///   the player is PK       -> send 0x0027
    ///   otherwise              -> failure event 0x55F, return TRUE, no send
    /// ```
    ///
    /// **A missing weenie sends**, which is the arm a test with no character takes and a test with
    /// a logged-in one cannot. The test is the PK one, not the player-killer or PK-lite one.
    pub fn do_pk_arena(&mut self, req: &mut dyn RequestSink, args: &[String]) -> ChatCommand {
        if !args.is_empty() {
            return ChatCommand::say(PLEASE_SEE_HELP_PKARENA, REFUSAL_CHAT_TYPE);
        }
        if let Some(w) = self.player.and_then(|p| self.weenie(p)) {
            if !w.is_pk() {
                return ChatCommand::fail(ONLY_PLAYER_KILLERS, BROADCAST_CHAT_TYPE);
            }
        }
        req.send(Request::TeleToPkArena(
            dereth_protocol::combat::CharacterTeleToPkArena,
        ));
        ChatCommand::sent()
    }

    /// Behavior: `@pklarena`, `@pla`. `do_pk_arena` with the PK-lite test (`is_pk_lite`),
    /// error `0x560` and `Request::TeleToPklArena` (`0x0026`).
    pub fn do_pkl_arena(&mut self, req: &mut dyn RequestSink, args: &[String]) -> ChatCommand {
        if !args.is_empty() {
            return ChatCommand::say(PLEASE_SEE_HELP_PKLARENA, REFUSAL_CHAT_TYPE);
        }
        if let Some(w) = self.player.and_then(|p| self.weenie(p)) {
            if !w.is_pk_lite() {
                // The same failure event the server's refusal raises, so a world that words it
                // differently words this one too.
                let text = self.world_rules.text_or(
                    dereth_primitives::TextKey::FailurePkLiteCommand,
                    ONLY_PLAYER_KILLER_LITES,
                );
                return ChatCommand::fail(text, BROADCAST_CHAT_TYPE);
            }
        }
        req.send(Request::TeleToPklArena(
            dereth_protocol::combat::CharacterTeleToPklArena,
        ));
        ChatCommand::sent()
    }

    /// `do_pk_lite` — `@pklite`, `@pkl`.
    ///
    /// **The polarity is the other way round from its two neighbours**: the handler calls
    /// the player-killer test and sends when it is **false**. A
    /// player who is already a Player Killer gets failure event `0x507` and no message
    /// leaves. Writing it as "if not PK then refuse", by symmetry with `@pkarena`, would have
    /// inverted the one command in the family whose test is inverted.
    pub fn do_pk_lite(&mut self, req: &mut dyn RequestSink, args: &[String]) -> ChatCommand {
        if !args.is_empty() {
            return ChatCommand::say(PLEASE_SEE_HELP_PKLITE, REFUSAL_CHAT_TYPE);
        }
        if let Some(w) = self.player.and_then(|p| self.weenie(p)) {
            if w.is_player_killer() {
                return ChatCommand::fail(ONLY_NON_PLAYER_KILLERS, BROADCAST_CHAT_TYPE);
            }
        }
        req.send(Request::EnterPkLite(
            dereth_protocol::combat::CharacterEnterPkLite,
        ));
        ChatCommand::sent()
    }

    /// `do_age` — `@age`.
    ///
    /// ```text
    ///   target = 0 (self)
    ///   the player is a PSR     -> target = the selected object, reset to self if not a player
    ///   (ordinary players skip that and query their own age)
    ///   send query-age event 0x01C2
    /// ```
    ///
    /// **The whole selected-target half is restricted to PSRs**, so for every ordinary character
    /// `@age` is `Request::QueryAge(0)` and nothing else. The PSR test is the
    /// only privilege test in the entire command file. The answer, `0x01C3
    /// Character_QueryAgeResponse`, is handled elsewhere; this is the send only.
    /// There is also **no argc test**, so `@age wibble` sends what `@age` does.
    pub fn do_age(&mut self, req: &mut dyn RequestSink, _args: &[String]) -> ChatCommand {
        req.send(Request::QueryAge(
            dereth_protocol::admin::CharacterQueryAge {
                target: dereth_primitives::ObjectId(0),
            },
        ));
        ChatCommand::sent()
    }

    /// Behavior: `@birth` sends `Request::QueryBirth(0)`. It has no argc test, and the return is the
    /// event's.
    pub fn do_birth(&mut self, req: &mut dyn RequestSink, _args: &[String]) -> ChatCommand {
        req.send(Request::QueryBirth(
            dereth_protocol::admin::CharacterQueryBirth {
                target: dereth_primitives::ObjectId(0),
            },
        ));
        ChatCommand::sent()
    }

    /// Handle `@die`.
    ///
    /// Argc must be zero, else [`PLEASE_SEE_HELP_DIE`] on `0x1A`. The body builds a
    /// `PropertyCollection` with `0x8E` = enum 1 (the Yes/No dialog kind) and `0xC5` = the prompt
    /// `StringInfo`, and hands it to the current-UI callback-dialog factory with the die-dialog
    /// callback.
    ///
    /// The dialog callback does one thing: it reads property `0x92` off the collection and sends
    /// the suicide request when it is present and set. When the answer byte is clear it skips the send. **A No is nothing at
    /// all**, exactly like the
    /// three allegiance dialogs, and unlike the gameplay confirmation dialog's close, which
    /// answers the server either way.
    pub fn do_die(&mut self, args: &[String]) -> ChatCommand {
        if !args.is_empty() {
            return ChatCommand::say(PLEASE_SEE_HELP_DIE, REFUSAL_CHAT_TYPE);
        }
        ChatCommand {
            die_confirmation: Some(DIE_CONFIRMATION),
            ..ChatCommand::done()
        }
    }

    /// The client's Yes arm, which is the only arm that sends.
    pub fn confirm_die(&mut self, req: &mut dyn RequestSink) {
        req.send(Request::Suicide(dereth_protocol::combat::CharacterSuicide));
    }
}

// ---------------------------------------------------------------------------------------------
// Family 2 — the comms toggles and the four channel commands
// ---------------------------------------------------------------------------------------------

impl World {
    /// `do_chat_toggle` — `@chat`.
    ///
    /// ```text
    ///   require argc to be exactly one
    ///   display "Please specify if you want chat text on or off." on type 0x1A
    ///   "on"  -> Request::ModifyGlobalSquelch(0, 2)
    ///   "off" -> Request::ModifyGlobalSquelch(1, 2)
    ///   otherwise display "Please specify on or off." on type 0x1A and return true
    /// ```
    ///
    /// The `add` operand is the **inverse** of the word: `on` means "receive chat", which is
    /// `add = 0`, i.e. remove the filter. The request is `Request::ModifyGlobalSquelch(add, msgType)`
    /// with `msgType` **2**.
    pub fn do_chat_toggle(&mut self, req: &mut dyn RequestSink, args: &[String]) -> ChatCommand {
        if args.len() != 1 {
            return ChatCommand::say(SPECIFY_CHAT_ON_OR_OFF, REFUSAL_CHAT_TYPE);
        }
        let add = if eq(&args[0], "on") {
            0
        } else if eq(&args[0], "off") {
            1
        } else {
            return ChatCommand::say(SPECIFY_ON_OR_OFF, REFUSAL_CHAT_TYPE);
        };
        req.send(Request::ModifyGlobalSquelch(
            dereth_protocol::comms::CommunicationModifyGlobalSquelch {
                add,
                msg_type: GLOBAL_SQUELCH_TYPE_CHAT,
            },
        ));
        ChatCommand::sent()
    }

    /// `do_no_tell` — `@notell`.
    ///
    /// Argc must be exactly one, else [`SPECIFY_TELLS_ON_OR_OFF`]. Then:
    ///
    /// ```text
    ///   add = 1 by default
    ///   "on"            -> keep add = 1
    ///   anything not "off" -> keep add = 1
    ///   "off"           -> add = 0 (only "off" clears it)
    ///   Request::ModifyGlobalSquelch(add, 3)
    /// ```
    ///
    /// **There is no unknown-word refusal**, unlike `do_chat_toggle` three functions earlier:
    /// `@notell wibble` sends `(1, 3)`, the same as `@notell on`. Two adjacent handlers, two
    /// different answers to the same input; inventing the refusal here would show a line retail
    /// never shows.
    pub fn do_no_tell(&mut self, req: &mut dyn RequestSink, args: &[String]) -> ChatCommand {
        if args.len() != 1 {
            return ChatCommand::say(SPECIFY_TELLS_ON_OR_OFF, REFUSAL_CHAT_TYPE);
        }
        let add = i32::from(!eq(&args[0], "off"));
        req.send(Request::ModifyGlobalSquelch(
            dereth_protocol::comms::CommunicationModifyGlobalSquelch {
                add,
                msg_type: GLOBAL_SQUELCH_TYPE_TELL,
            },
        ));
        ChatCommand::sent()
    }

    /// Behavior: `@index`. It only sends `Request::ChannelIndex`. No argc test, no refusal, and the return is the event's.
    pub fn do_channel_index(&mut self, req: &mut dyn RequestSink, _args: &[String]) -> ChatCommand {
        req.send(Request::ChannelIndex(
            dereth_protocol::comms::CommunicationChannelIndexRequest,
        ));
        ChatCommand::sent()
    }

    /// `do_channel_list` — `@clist`.
    ///
    /// ```text
    ///   require argc to be exactly one
    ///   wrong argc: display "Please specify the channel name." on type 0x1A and return true
    ///   resolve the channel id
    ///   zero id: failure event 0x422                ; "That channel doesn't exist."
    ///   otherwise send channel-list event 0x0148
    /// ```
    ///
    /// Channel-name lookup is [`crate::chat::get_channel_id`], the same lookup
    /// the pre-Turbine-chat `@a` channel handler uses. `@clist`, `@on` and `@off`
    /// use the **same** wide literal and the same failure code; only the event differs.
    pub fn do_channel_list(&mut self, req: &mut dyn RequestSink, args: &[String]) -> ChatCommand {
        let Some(channel) = channel_arg(args) else {
            return channel_refusal(args);
        };
        req.send(Request::ChannelList(
            dereth_protocol::comms::CommunicationChannelListRequest { channel },
        ));
        ChatCommand::sent()
    }

    /// `do_channel_on` — `@on` -> `Request::AddToChannel` (`0x0145`).
    pub fn do_channel_on(&mut self, req: &mut dyn RequestSink, args: &[String]) -> ChatCommand {
        let Some(channel) = channel_arg(args) else {
            return channel_refusal(args);
        };
        req.send(Request::AddToChannel(
            dereth_protocol::comms::CommunicationAddToChannel { channel },
        ));
        ChatCommand::sent()
    }

    /// `do_channel_off` — `@off` -> `Request::RemoveFromChannel` (`0x0146`).
    pub fn do_channel_off(&mut self, req: &mut dyn RequestSink, args: &[String]) -> ChatCommand {
        let Some(channel) = channel_arg(args) else {
            return channel_refusal(args);
        };
        req.send(Request::RemoveFromChannel(
            dereth_protocol::comms::CommunicationRemoveFromChannel { channel },
        ));
        ChatCommand::sent()
    }

    /// `do_afk` — `@afk`.
    ///
    /// `next_arg` pops the sub-command, then three case-insensitive compare chains. The empty sub-command falls
    /// into the **`on`** arm and sets the same flag as an explicit `"on"`, so a bare `@afk` is
    /// `@afk on`.
    ///
    /// ```text
    /// on  :  query PropertyBool::Afk
    ///         if already AFK, send nothing; otherwise send Request::SetAfkMode(1)
    /// off :  query PropertyBool::Afk
    ///         if not AFK, send nothing; otherwise send Request::SetAfkMode(0)
    /// A message joins all remaining arguments.
    ///         trim(both, " ")                           ; SPACE only, not whitespace
    ///         length > 0xC0 -> substring(0, 0xBF)       ; 191 characters
    ///                 find_substring("\n") == -1 -> append "\n"
    ///                 send Request::SetAfkMessage               ; 0x0010
    ///         empty? L"New AFK message set: I am currently away from the keyboard."
    ///                 else   "New AFK message set: " + text
    /// ```
    ///
    /// **Both mode arms are guarded by the quality**, so `@afk on` twice sends once. The message
    /// arm has **no** guard and no empty-text refusal: an empty `@afk msg`
    /// sends an empty `0x0010`, which is how the help text's *"will set your AFK message back to
    /// the default"* happens, and the acknowledgement it prints is the default sentence.
    pub fn do_afk(&mut self, req: &mut dyn RequestSink, args: &[String]) -> ChatCommand {
        let (sub, rest) = next_arg(args);
        if sub.is_empty() || eq(sub, "on") {
            if self.player_afk() {
                return ChatCommand::done();
            }
            req.send(Request::SetAfkMode(
                dereth_protocol::comms::CommunicationSetAfkMode { afk: 1 },
            ));
            return ChatCommand::sent();
        }
        if eq(sub, "off") {
            if !self.player_afk() {
                return ChatCommand::done();
            }
            req.send(Request::SetAfkMode(
                dereth_protocol::comms::CommunicationSetAfkMode { afk: 0 },
            ));
            return ChatCommand::sent();
        }
        if eq(sub, "msg") {
            let message = afk_message(rest);
            let ack = if message.trim_end_matches('\n').is_empty() {
                NEW_AFK_MESSAGE_DEFAULT.to_owned()
            } else {
                format!("{NEW_AFK_MESSAGE_PREFIX}{message}")
            };
            req.send(Request::SetAfkMessage(
                dereth_protocol::comms::CommunicationSetAfkMessage { message },
            ));
            return ChatCommand {
                lines: vec![(ack, BROADCAST_CHAT_TYPE)],
                handled: true,
                sent: 1,
                ..ChatCommand::default()
            };
        }
        // the chain's fall-through. It prints nothing and returns the flag, which is
        // clear here, so this is the one arm in the family that reaches the command dispatcher's
        // failure event `0x26` and answers *"That is not a valid command."*
        ChatCommand::refused()
    }

    /// Boolean quality `0x6E` (`Afk`) on the player's own qualities.
    ///
    /// Prefers the weenie row and falls back to the parked login `PlayerDesc`, which is the order
    /// this query's own contract requires.
    #[must_use]
    pub fn player_afk(&self) -> bool {
        match self.player_qualities() {
            Some(q) => q.inq_bool(AFK_BOOL_PROPERTY),
            None => self
                .login_player_desc()
                .is_some_and(|q| q.inq_bool(AFK_BOOL_PROPERTY)),
        }
    }
}

/// The shared argc-and-lookup half of `@clist`, `@on` and `@off`.
fn channel_arg(args: &[String]) -> Option<u32> {
    if args.len() != 1 {
        return None;
    }
    match crate::chat::get_channel_id(&args[0]) {
        0 => None,
        id => Some(id),
    }
}

/// Which of the two refusals the three channel commands print. The argc one is the handler's own
/// scroll print at the command's source window; the unknown-name one is
/// failure event `0x422`, whose arm passes window **0**.
fn channel_refusal(args: &[String]) -> ChatCommand {
    if args.len() == 1 {
        ChatCommand::fail(THAT_CHANNEL_DOES_NOT_EXIST, REFUSAL_CHAT_TYPE)
    } else {
        ChatCommand::say(SPECIFY_THE_CHANNEL_NAME, REFUSAL_CHAT_TYPE)
    }
}

/// `do_afk`'s `msg` body: join the arguments, `trim(both, " ")`, truncate to
/// [`AFK_MESSAGE_MAX`], then guarantee a trailing newline.
///
/// The trim string is the one-character literal `" "`, **not** the full whitespace set — so a
/// tab survives where the say command's trim would remove
/// it. The newline test is `find_substring("\n") == -1` over the whole message, so a message that
/// already contains a newline anywhere does not get another one appended.
#[must_use]
pub fn afk_message(args: &[String]) -> String {
    let joined = args.join(" ");
    let trimmed = joined.trim_matches(' ');
    let mut out: String = trimmed.chars().take(AFK_MESSAGE_MAX).collect();
    if !out.contains('\n') {
        out.push('\n');
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Family 3 — consent and permit
// ---------------------------------------------------------------------------------------------

impl World {
    /// `do_consent` — `@consent`.
    ///
    /// Five case-insensitive compares on the `next_arg` sub-command, in this order:
    ///
    /// ```text
    /// "on"     -> set accept-loot-permits true                 ; option 16
    ///             print "You can now accept ..." on type 0 at the source window
    /// "off"    -> set accept-loot-permits false
    ///             print "You are no longer accepting ..." on type 0 at the source window
    /// "who"    -> Request::DisplayPlayerConsentList                       0x0217
    /// "clear"  -> Request::ClearPlayerConsentList                         0x0216
    /// "remove" -> join_args_as_name(rest); empty -> "Please specify a person to remove ..." on 0x1A
    ///             else Request::RemoveFromPlayerConsentList(name)         0x0218
    /// else     -> "Please specify a valid consent command." on 0x1A
    /// ```
    ///
    /// The sub-command is **`who`, not `list`**. `dereth_protocol::admin`'s doc comment on `0x0217`
    /// says `/consent list`, which is
    /// the ACE-side name and not the word retail's parser compares.
    ///
    /// Both `on` and `off` print at chat type **0** and at the command's source window: the caller
    /// passes 0 for the type and the current command source for the window. `off` goes
    /// through the **narrow** scroll-print overload with the same four arguments and the
    /// string already narrow.
    pub fn do_consent(&mut self, req: &mut dyn RequestSink, args: &[String]) -> ChatCommand {
        let (sub, rest) = next_arg(args);
        if eq(sub, "on") {
            return ChatCommand {
                lines: vec![(
                    CAN_NOW_ACCEPT_CORPSE_LOOTING.to_owned(),
                    BROADCAST_CHAT_TYPE,
                )],
                option: Some((ACCEPT_LOOT_PERMITS_ORDINAL, true)),
                ..ChatCommand::done()
            };
        }
        if eq(sub, "off") {
            return ChatCommand {
                lines: vec![(
                    NO_LONGER_ACCEPTING_CORPSE_LOOTING.to_owned(),
                    BROADCAST_CHAT_TYPE,
                )],
                option: Some((ACCEPT_LOOT_PERMITS_ORDINAL, false)),
                ..ChatCommand::done()
            };
        }
        if eq(sub, "who") {
            req.send(Request::DisplayPlayerConsentList(
                dereth_protocol::admin::CharacterDisplayPlayerConsentList,
            ));
            return ChatCommand::sent();
        }
        if eq(sub, "clear") {
            req.send(Request::ClearPlayerConsentList(
                dereth_protocol::admin::CharacterClearPlayerConsentList,
            ));
            return ChatCommand::sent();
        }
        if eq(sub, "remove") {
            let name = crate::friends::join_args_as_name(rest);
            if name.is_empty() {
                return ChatCommand::say(SPECIFY_PERSON_TO_REMOVE_FROM_CONSENT, REFUSAL_CHAT_TYPE);
            }
            req.send(Request::RemoveFromPlayerConsentList(
                dereth_protocol::admin::CharacterRemoveFromPlayerConsentList { name },
            ));
            return ChatCommand::sent();
        }
        ChatCommand::say(SPECIFY_VALID_CONSENT_COMMAND, REFUSAL_CHAT_TYPE)
    }

    /// `do_permit` — `@permit`.
    ///
    /// ```text
    ///   pop the sub-command; compare it with "add", then "remove"
    ///   neither -> display "Please specify a valid permit command." on type 0x1A
    ///   join_args_as_name(rest)
    ///   empty   -> display "Please specify a person for the permit command." on type 0x1A
    ///   compare with "add" again
    ///     "add"    -> Request::AddPlayerPermission(name)      0x0219
    ///     "remove" -> Request::RemovePlayerPermission(name)   0x021A
    /// ```
    ///
    /// **`0x021A`, not `0x0220`** — the catalogue's `0x0220` is a documentation error that
    /// `dereth_protocol::admin`'s own doc comment and `docs/CORRECTIONS.md` already record; the
    /// request retail sends carries opcode `0x021A`.
    ///
    /// Retail compares `"add"` **twice** — once to decide whether the sub-command is legal and
    /// again to pick the event — which is why a sub-command that is neither is rejected before the
    /// name is even joined.
    pub fn do_permit(&mut self, req: &mut dyn RequestSink, args: &[String]) -> ChatCommand {
        let (sub, rest) = next_arg(args);
        let add = eq(sub, "add");
        if !add && !eq(sub, "remove") {
            return ChatCommand::say(SPECIFY_VALID_PERMIT_COMMAND, REFUSAL_CHAT_TYPE);
        }
        let name = crate::friends::join_args_as_name(rest);
        if name.is_empty() {
            return ChatCommand::say(SPECIFY_PERSON_FOR_PERMIT, REFUSAL_CHAT_TYPE);
        }
        if add {
            req.send(Request::AddPlayerPermission(
                dereth_protocol::admin::CharacterAddPlayerPermission { name },
            ));
        } else {
            req.send(Request::RemovePlayerPermission(
                dereth_protocol::admin::CharacterRemovePlayerPermission { name },
            ));
        }
        ChatCommand::sent()
    }
}

// ---------------------------------------------------------------------------------------------
// Family 4 — the purely local prints
// ---------------------------------------------------------------------------------------------

impl World {
    /// Behavior: `@speaker`. One literal and one scroll print on chat type
    /// **0** at the command's source window; the command is retired and sends nothing.
    pub fn do_speaker(&mut self, _args: &[String]) -> ChatCommand {
        ChatCommand::say(SPEAKER_RETIRED, BROADCAST_CHAT_TYPE)
    }

    /// Behavior: `@endurance`. One literal and one
    /// scroll print on chat type **0** at the command's source window.
    pub fn do_endurance(&mut self, _args: &[String]) -> ChatCommand {
        ChatCommand::say(ENDURANCE_TEXT, BROADCAST_CHAT_TYPE)
    }

    /// `do_emote_list` — `@emotes`.
    ///
    /// The text is [`crate::emotes::emote_list_text`], transcribed from the client's
    /// single 438-byte literal. The chat type, the plugin flag and the window are that module's
    /// three constants: chat type 0, the plugin flag set, window 0.
    pub fn do_emote_list(&mut self, _args: &[String]) -> ChatCommand {
        ChatCommand {
            lines: vec![(
                crate::emotes::emote_list_text(),
                crate::emotes::EMOTE_LIST_TEXT_TYPE,
            )],
            ..ChatCommand::done()
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Family 5 — the `@house` command family.
// ---------------------------------------------------------------------------------------------
//
// House-command dispatch is one `_stricmp` ladder over the first
// argument with four sub-handlers hanging off it, and between the five of them they are the
// **only** caller in retail of thirteen house events.
//
// The house command maps its subcommands to the following requests.
// ```text
// open                         -> Request::SetOpenHouseStatus(1)    0x0247
//                              close               -> Request::SetOpenHouseStatus(0)    0x0247
//                              recall | re         -> do_house_recall                   0x0262
//                              mansion_recall |
//                              alleg_recall | ma   -> do_mansion_recall                 0x0278
//                              storage             -> do_house_storage
//                              remove | boot       -> do_house_boot
//                              boot_all |
//                              remove_all          -> Request::BootEveryone             0x025F
//                              guest               -> do_house_guests
//                              abandon             -> the two-stage dialog              0x021F
//                              available           -> do_house_available_list 0x0270
//                              hooks on | off      -> Request::SetHooksVisibility(1|0)  0x0266
// do_house_guests     add <name>          -> Request::AddPermanentGuest        0x0245
//                              remove <name>       -> Request::RemovePermanentGuest     0x0246
//                              remove_all          -> Request::RemoveAllPermanentGuests 0x025E
//                              list | show         -> Request::RequestFullGuestList     0x024D
//                              add_allegiance      -> Request::ModifyAllegianceGuestPermission(1) 0x0267
//                              remove_allegiance   -> Request::ModifyAllegianceGuestPermission(0) 0x0267
// do_house_storage    add <name>          -> Request::ChangeStoragePermission(1) 0x0249
//                              add -all            -> Request::AddAllStoragePermission  0x025C
//                              remove <name>       -> Request::ChangeStoragePermission(0) 0x0249
//                              remove -all |
//                              remove_all          -> Request::RemoveAllStoragePermission 0x024C
//                              list | show         -> Request::RequestFullGuestList     0x024D
//                              add_allegiance      -> Request::ModifyAllegianceStoragePermission(1) 0x0268
//                              remove_allegiance   -> Request::ModifyAllegianceStoragePermission(0) 0x0268
// boot -all                    -> Request::BootEveryone             0x025F
//                              <name>              -> Request::BootSpecificHouseGuest   0x024A
// available-list cottage|villa|
//                              mansion|apartment   -> Request::ListAvailableHouses(1..4) 0x0270
// ```
//
// Runtime validation covered eleven of the thirteen request shapes (`0x0245` x3, `0x0246`,
// `0x0247` x5, `0x0249` x2, `0x024A` x3, `0x024C` x2, `0x024D` x2, `0x025E` x2, `0x0262`,
// `0x0266` x2, `0x0267` x5, `0x0268` x2 and `0x0278` x2). A `0x024D` request was answered 153 ms
// later by a `0x0257 House_UpdateHAR`, whose receiver is in the housing module. The two
// unobserved request shapes are `0x025F BootEveryone` and `0x021F AbandonHouse`.
//
// Every arm compares case-insensitively, so every sub-command is case-insensitive.

/// The house-guest `add`/`remove` refusal when no name is supplied.
pub const SPECIFY_THE_GUESTS_NAME: &str = "Please specify the guest's name.";
/// The house-storage `add`/`remove` refusal when no name is supplied. It is a *different* sentence
/// from its sibling above and is transcribed rather than harmonised.
pub const SPECIFY_AN_ACTUAL_NAME: &str = "Please specify an actual name.";
/// The house-list command's unknown-type refusal. This is the one literal in the family that names
/// `@help hslist` rather than `@help House`, and it has no trailing full stop either.
pub const PLEASE_SEE_HELP_HSLIST: &str =
    "Please see @help hslist for more information on how to use this command";

/// The client's `abandon` arm — property `0xC5` of the first
/// current-UI callback dialog, as a wide string.
pub const HOUSE_ABANDON_FIRST: &str = "Do you really want to abandon your house? Any items in the house (on hooks or in storage) will stay with the house, and you will lose access to them.";
/// The client's Yes arm — the **second** question, and the only
/// path to the second house-abandon callback.
pub const HOUSE_ABANDON_SECOND: &str =
    "Are you absolutely certain you wish to abandon your house? Click yes only if you are sure!";

/// The client's four case-insensitive compares, in the order the function tests them,
/// with the `Request::ListAvailableHouses` operand each one loads.
///
/// ```text
///   "apartment" -> 4
///   "cottage"   -> 1
///   "villa"     -> 2
///   "mansion"   -> 3
/// ```
pub const HOUSE_TYPES: &[(&str, u32)] = &[
    ("apartment", 4),
    ("cottage", 1),
    ("villa", 2),
    ("mansion", 3),
];

impl World {
    /// Handle `@house` and `@hou`.
    ///
    /// The first act is `next_arg`, and **the bare-command test is on the token's length, not on
    /// `argc`**:
    ///
    /// ```text
    ///   word = next_arg
    ///   length including terminator != 1  -> the sub-command ladder
    ///   otherwise display "Please see @help House …" on type 0x1A in the current source
    ///             and return TRUE
    /// ```
    ///
    /// Every exit of this function returns `true`, so the dispatcher never adds *"That is
    /// not a valid command."* on top of the handler's own sentence. The unknown-word arm prints
    /// the same literal the bare command does — the one with **no trailing full
    /// stop**, which `PLEASE_SEE_HELP_HOUSE` carries.
    ///
    /// `hooks` is the one two-token arm: it takes a second `next_arg` and compares it against
    /// `"on"` then `"off"`, and **anything else falls through to the same help line** rather than
    /// defaulting either way. The hook-visibility request is reached only from the two matches.
    pub fn do_house(&mut self, req: &mut dyn RequestSink, args: &[String]) -> ChatCommand {
        let (verb, rest) = next_arg(args);
        if verb.is_empty() {
            return ChatCommand::say(PLEASE_SEE_HELP_HOUSE, REFUSAL_CHAT_TYPE);
        }
        if eq(verb, "open") || eq(verb, "close") {
            req.send(Request::SetOpenHouseStatus(
                dereth_protocol::trade::HouseSetOpenHouseStatus {
                    open: i32::from(eq(verb, "open")),
                },
            ));
            return ChatCommand::sent();
        }
        if eq(verb, "recall") || eq(verb, "re") {
            return self.do_house_recall(req, rest);
        }
        // Three names, two `_stricmp` pairs: `mansion_recall` and `alleg_recall`,
        // then `ma` and `alleg_recall` **again**. Retail compares `alleg_recall`
        // twice; the set it accepts is the same three either way.
        if eq(verb, "mansion_recall") || eq(verb, "alleg_recall") || eq(verb, "ma") {
            return self.do_mansion_recall(req, rest);
        }
        if eq(verb, "storage") {
            return self.do_house_storage(req, rest);
        }
        if eq(verb, "remove") || eq(verb, "boot") {
            return self.do_house_boot(req, rest);
        }
        if eq(verb, "boot_all") || eq(verb, "remove_all") {
            req.send(Request::BootEveryone(
                dereth_protocol::trade::HouseBootEveryone,
            ));
            return ChatCommand::sent();
        }
        if eq(verb, "guest") {
            return self.do_house_guests(req, rest);
        }
        if eq(verb, "abandon") {
            return ChatCommand {
                house_abandon_confirmation: Some(HOUSE_ABANDON_FIRST),
                ..ChatCommand::done()
            };
        }
        if eq(verb, "available") {
            return self.do_house_available_list(req, rest);
        }
        if eq(verb, "hooks") {
            let (state, _) = next_arg(rest);
            if eq(state, "on") || eq(state, "off") {
                req.send(Request::SetHooksVisibility(
                    dereth_protocol::trade::HouseSetHooksVisibility {
                        visible: i32::from(eq(state, "on")),
                    },
                ));
                return ChatCommand::sent();
            }
        }
        ChatCommand::say(PLEASE_SEE_HELP_HOUSE, REFUSAL_CHAT_TYPE)
    }

    /// Handle `@house guest …`.
    ///
    /// `add` and `remove` share one tail: `join_args_as_name` over what is left, an empty-name
    /// refusal, then the event the flag picked. The other four arms take no argument at all and
    /// **`list` and `show` are the same arm**: the two compares join onto one request for the
    /// full guest list.
    ///
    /// An unknown sub-command prints `PLEASE_SEE_HELP_HOUSE` and returns `true`, like its parent.
    pub fn do_house_guests(&mut self, req: &mut dyn RequestSink, args: &[String]) -> ChatCommand {
        let (verb, rest) = next_arg(args);
        let add = if eq(verb, "add") {
            true
        } else if eq(verb, "remove") {
            false
        } else if eq(verb, "remove_all") {
            req.send(Request::RemoveAllPermanentGuests(
                dereth_protocol::trade::HouseRemoveAllPermanentGuests,
            ));
            return ChatCommand::sent();
        } else if eq(verb, "list") || eq(verb, "show") {
            req.send(Request::RequestFullGuestList(
                dereth_protocol::trade::HouseRequestFullGuestList,
            ));
            return ChatCommand::sent();
        } else if eq(verb, "add_allegiance") || eq(verb, "remove_allegiance") {
            req.send(Request::ModifyAllegianceGuestPermission(
                dereth_protocol::trade::HouseModifyAllegianceGuestPermission {
                    allow: i32::from(eq(verb, "add_allegiance")),
                },
            ));
            return ChatCommand::sent();
        } else {
            return ChatCommand::say(PLEASE_SEE_HELP_HOUSE, REFUSAL_CHAT_TYPE);
        };
        let joined = rest.join(" ");
        let name = crate::chat::join_args_as_name(&joined);
        if name.is_empty() {
            return ChatCommand::say(SPECIFY_THE_GUESTS_NAME, REFUSAL_CHAT_TYPE);
        }
        let name = name.to_owned();
        if add {
            req.send(Request::AddPermanentGuest(
                dereth_protocol::trade::HouseAddPermanentGuest { name },
            ));
        } else {
            req.send(Request::RemovePermanentGuest(
                dereth_protocol::trade::HouseRemovePermanentGuest { name },
            ));
        }
        ChatCommand::sent()
    }

    /// Handle `@house storage …`.
    ///
    /// [`Self::do_house_guests`] with three differences, all of them load-bearing:
    ///
    /// 1. the empty-name sentence is `SPECIFY_AN_ACTUAL_NAME`, not the guest one;
    /// 2. `add` and `remove` accept the literal name **`-all`**, which turns into
    ///    `Request::AddAllStoragePermission` (`0x025C`) / `Request::RemoveAllStoragePermission`
    ///    (`0x024C`), selected by the add/remove flag and the `-all` match;
    /// 3. the named form is one message with the flag inside it,
    ///    `Request::ChangeStoragePermission(name, add)` (`0x0249`), rather than two opcodes.
    ///
    /// The `-all` test happens **after** the empty-name refusal, so `@house storage add` alone is
    /// still the refusal and not an accidental grant to everybody.
    pub fn do_house_storage(&mut self, req: &mut dyn RequestSink, args: &[String]) -> ChatCommand {
        let (verb, rest) = next_arg(args);
        let add = if eq(verb, "add") {
            true
        } else if eq(verb, "remove") {
            false
        } else if eq(verb, "remove_all") {
            req.send(Request::RemoveAllStoragePermission(
                dereth_protocol::trade::HouseRemoveAllStoragePermission,
            ));
            return ChatCommand::sent();
        } else if eq(verb, "list") || eq(verb, "show") {
            req.send(Request::RequestFullGuestList(
                dereth_protocol::trade::HouseRequestFullGuestList,
            ));
            return ChatCommand::sent();
        } else if eq(verb, "add_allegiance") || eq(verb, "remove_allegiance") {
            req.send(Request::ModifyAllegianceStoragePermission(
                dereth_protocol::trade::HouseModifyAllegianceStoragePermission {
                    allow: i32::from(eq(verb, "add_allegiance")),
                },
            ));
            return ChatCommand::sent();
        } else {
            return ChatCommand::say(PLEASE_SEE_HELP_HOUSE, REFUSAL_CHAT_TYPE);
        };
        let joined = rest.join(" ");
        let name = crate::chat::join_args_as_name(&joined);
        if name.is_empty() {
            return ChatCommand::say(SPECIFY_AN_ACTUAL_NAME, REFUSAL_CHAT_TYPE);
        }
        if eq(name, "-all") {
            if add {
                req.send(Request::AddAllStoragePermission(
                    dereth_protocol::trade::HouseAddAllStoragePermission,
                ));
            } else {
                req.send(Request::RemoveAllStoragePermission(
                    dereth_protocol::trade::HouseRemoveAllStoragePermission,
                ));
            }
            return ChatCommand::sent();
        }
        req.send(Request::ChangeStoragePermission(
            dereth_protocol::trade::HouseChangeStoragePermission {
                name: name.to_owned(),
                has_permission: i32::from(add),
            },
        ));
        ChatCommand::sent()
    }

    /// Handle `@house boot …` and
    /// `@house remove …`.
    ///
    /// ```text
    ///   name = join_args_as_name(rest)
    ///   name empty      -> "Please see @help House …", 0x1A, TRUE
    ///   name is "-all" (any case) -> Request::BootEveryone              ; 0x025F
    ///   otherwise -> Request::BootSpecificHouseGuest(name)              ; 0x024A
    /// ```
    ///
    /// The empty case prints the *house* help line, not a name-specific one — the third distinct
    /// answer this family gives to "you left the name off".
    pub fn do_house_boot(&mut self, req: &mut dyn RequestSink, args: &[String]) -> ChatCommand {
        let joined = args.join(" ");
        let name = crate::chat::join_args_as_name(&joined);
        if name.is_empty() {
            return ChatCommand::say(PLEASE_SEE_HELP_HOUSE, REFUSAL_CHAT_TYPE);
        }
        if eq(name, "-all") {
            req.send(Request::BootEveryone(
                dereth_protocol::trade::HouseBootEveryone,
            ));
        } else {
            req.send(Request::BootSpecificHouseGuest(
                dereth_protocol::trade::HouseBootSpecificHouseGuest {
                    name: name.to_owned(),
                },
            ));
        }
        ChatCommand::sent()
    }

    /// Handle `@hslist`, and the
    /// `@house available` arm.
    ///
    /// Four case-insensitive compares in the order `apartment`, `cottage`, `villa`, `mansion`, each loading its
    /// own `HouseType` before the one shared `Request::ListAvailableHouses`. Anything else —
    /// including no argument at all — prints
    /// `PLEASE_SEE_HELP_HSLIST` on chat type `0x1A` at the command's source window and returns
    /// `true`.
    ///
    /// The answer is `0x0271 House_AvailableHouses`; ACE sends it from
    /// `Player_House.HandleActionListAvailable`, unconditionally, for every type.
    pub fn do_house_available_list(
        &mut self,
        req: &mut dyn RequestSink,
        args: &[String],
    ) -> ChatCommand {
        let (word, _) = next_arg(args);
        let Some((_, house_type)) = HOUSE_TYPES.iter().copied().find(|(n, _)| eq(word, n)) else {
            return ChatCommand::say(PLEASE_SEE_HELP_HSLIST, REFUSAL_CHAT_TYPE);
        };
        req.send(Request::ListAvailableHouses(
            dereth_protocol::trade::HouseListAvailableHouses { house_type },
        ));
        ChatCommand::sent()
    }

    /// The client's Yes arm, the **only** caller of
    /// abandon-house event in retail.
    ///
    /// Both callbacks have the die dialog's callback shape: read property `0x92` off the closing
    /// collection and act only when it is set. So a No at *either* stage is nothing at all — no
    /// message, no line — and the second question is reachable only through the first's Yes.
    pub fn confirm_house_abandon(&mut self, req: &mut dyn RequestSink) {
        req.send(Request::AbandonHouse(
            dereth_protocol::trade::HouseAbandonHouse,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RecordingRequests;

    fn a(words: &[&str]) -> Vec<String> {
        words.iter().map(|s| (*s).to_owned()).collect()
    }

    /// Every command that takes shape A prints its own sentence and sends nothing when it is
    /// given an argument — and **returns true**, so the command dispatcher adds nothing after it.
    #[test]
    fn the_shape_a_handlers_refuse_arguments_and_still_return_true() {
        let mut w = World::new();
        let mut r = RecordingRequests::default();
        let cases: Vec<(ChatCommand, &str)> = vec![
            (
                w.do_lifestone(&mut r, &a(&["x"])),
                PLEASE_SEE_HELP_LIFESTONE,
            ),
            (
                w.do_marketplace(&mut r, &a(&["x"])),
                PLEASE_SEE_HELP_MARKETPLACE,
            ),
            (w.do_pk_arena(&mut r, &a(&["x"])), PLEASE_SEE_HELP_PKARENA),
            (w.do_pkl_arena(&mut r, &a(&["x"])), PLEASE_SEE_HELP_PKLARENA),
            (w.do_pk_lite(&mut r, &a(&["x"])), PLEASE_SEE_HELP_PKLITE),
            (w.do_house_recall(&mut r, &a(&["x"])), PLEASE_SEE_HELP_HOUSE),
            (
                w.do_mansion_recall(&mut r, &a(&["x"])),
                PLEASE_SEE_HELP_HOUSE,
            ),
            (w.do_die(&a(&["x"])), PLEASE_SEE_HELP_DIE),
        ];
        for (c, text) in cases {
            assert!(
                c.handled,
                "every one of these returns TRUE even as it refuses"
            );
            assert_eq!(c.sent, 0);
            assert_eq!(c.lines, vec![(text.to_owned(), REFUSAL_CHAT_TYPE)]);
        }
        assert!(
            r.0.is_empty(),
            "not one of the eight put anything in the sink"
        );
    }

    /// The bare forms send, and each sends its own opcode.
    #[test]
    fn the_bare_forms_send_one_request_each() {
        let mut w = World::new();
        let mut r = RecordingRequests::default();
        for c in [
            w.do_lifestone(&mut r, &[]),
            w.do_marketplace(&mut r, &[]),
            w.do_pk_arena(&mut r, &[]),
            w.do_pkl_arena(&mut r, &[]),
            w.do_pk_lite(&mut r, &[]),
            w.do_house_recall(&mut r, &[]),
            w.do_mansion_recall(&mut r, &[]),
            w.do_age(&mut r, &[]),
            w.do_birth(&mut r, &[]),
            w.do_channel_index(&mut r, &[]),
        ] {
            assert!(c.handled);
            assert_eq!(c.sent, 1);
        }
        assert_eq!(r.0.len(), 10);
        // With no player weenie, all three PK commands take their "no weenie -> send anyway" arm.
        assert!(matches!(r.0[2], Request::TeleToPkArena(_)));
        assert!(matches!(r.0[3], Request::TeleToPklArena(_)));
        assert!(matches!(r.0[4], Request::EnterPkLite(_)));
    }

    /// `@chat`'s two words map to the two `add` values and `@notell`'s unknown word does not
    /// refuse — the asymmetry between two adjacent handlers.
    #[test]
    fn the_two_global_squelch_toggles_disagree_about_an_unknown_word() {
        let mut w = World::new();
        let mut r = RecordingRequests::default();
        assert_eq!(w.do_chat_toggle(&mut r, &a(&["on"])).sent, 1);
        assert_eq!(w.do_chat_toggle(&mut r, &a(&["off"])).sent, 1);
        let bad = w.do_chat_toggle(&mut r, &a(&["wibble"]));
        assert_eq!(bad.sent, 0);
        assert_eq!(
            bad.lines,
            vec![(SPECIFY_ON_OR_OFF.to_owned(), REFUSAL_CHAT_TYPE)]
        );
        assert_eq!(
            w.do_chat_toggle(&mut r, &a(&["on", "extra"])).sent,
            0,
            "argc must be one"
        );

        assert_eq!(
            w.do_no_tell(&mut r, &a(&["wibble"])).sent,
            1,
            "no unknown-word refusal"
        );
        let expected = [
            (0, GLOBAL_SQUELCH_TYPE_CHAT),
            (1, GLOBAL_SQUELCH_TYPE_CHAT),
            (1, GLOBAL_SQUELCH_TYPE_TELL),
        ];
        let got: Vec<(i32, u32)> =
            r.0.iter()
                .filter_map(|q| match q {
                    Request::ModifyGlobalSquelch(m) => Some((m.add, m.msg_type)),
                    _ => None,
                })
                .collect();
        assert_eq!(got, expected);
    }

    /// `@afk` with no argument is `@afk on`, and both mode arms are guarded by the quality.
    #[test]
    fn afk_sends_only_when_the_quality_disagrees_with_the_word() {
        let mut w = World::new();
        let mut r = RecordingRequests::default();
        assert!(!w.player_afk(), "a fresh World has no Afk quality at all");
        assert_eq!(
            w.do_afk(&mut r, &[]).sent,
            1,
            "a bare @afk takes the `on` arm"
        );
        assert_eq!(w.do_afk(&mut r, &a(&["on"])).sent, 1);
        assert_eq!(w.do_afk(&mut r, &a(&["off"])).sent, 0, "not AFK -> no send");
        assert_eq!(r.0.len(), 2);
        let bad = w.do_afk(&mut r, &a(&["wibble"]));
        assert!(
            !bad.handled,
            "the fall-through returns FALSE and prints nothing"
        );
        assert!(bad.lines.is_empty());
    }

    /// The `msg` arm's truncation, its newline guarantee and its two acknowledgements.
    #[test]
    fn the_afk_message_is_trimmed_truncated_and_newline_terminated() {
        assert_eq!(
            afk_message(&a(&["  brb  "])),
            "brb\n",
            "trim(both, \" \") then append \\n"
        );
        assert_eq!(
            afk_message(&[]),
            "\n",
            "an empty message is still newline-terminated"
        );
        let long = afk_message(&a(&[&"x".repeat(300)]));
        assert_eq!(
            long.chars().count(),
            AFK_MESSAGE_MAX + 1,
            "substring(0, 0xBF) plus the \\n"
        );
        assert_eq!(
            afk_message(&a(&["a\nb"])),
            "a\nb",
            "find_substring(\"\\n\") != -1: no append"
        );

        let mut w = World::new();
        let mut r = RecordingRequests::default();
        let empty = w.do_afk(&mut r, &a(&["msg"]));
        assert_eq!(
            empty.sent, 1,
            "an empty @afk msg still sends — that is how it resets"
        );
        assert_eq!(empty.lines[0].0, NEW_AFK_MESSAGE_DEFAULT);
        let set = w.do_afk(&mut r, &a(&["msg", "back", "soon"]));
        assert_eq!(
            set.lines[0].0,
            format!("{NEW_AFK_MESSAGE_PREFIX}back soon\n")
        );
        match &r.0[1] {
            Request::SetAfkMessage(m) => assert_eq!(m.message, "back soon\n"),
            other => panic!("{other:?}"),
        }
    }

    /// `@consent`'s five sub-commands, including the one whose word is `who` and not `list`.
    #[test]
    fn consent_takes_who_and_not_list() {
        let mut w = World::new();
        let mut r = RecordingRequests::default();
        assert_eq!(w.do_consent(&mut r, &a(&["who"])).sent, 1);
        assert_eq!(w.do_consent(&mut r, &a(&["clear"])).sent, 1);
        assert_eq!(w.do_consent(&mut r, &a(&["remove", "Bob"])).sent, 1);
        let no_name = w.do_consent(&mut r, &a(&["remove"]));
        assert_eq!(no_name.sent, 0);
        assert_eq!(no_name.lines[0].0, SPECIFY_PERSON_TO_REMOVE_FROM_CONSENT);
        let listed = w.do_consent(&mut r, &a(&["list"]));
        assert_eq!(
            listed.sent, 0,
            "`list` is ACE's word for 0x0217, not the parser's"
        );
        assert_eq!(listed.lines[0].0, SPECIFY_VALID_CONSENT_COMMAND);

        let on = w.do_consent(&mut r, &a(&["on"]));
        assert_eq!(on.option, Some((ACCEPT_LOOT_PERMITS_ORDINAL, true)));
        assert_eq!(
            on.sent, 0,
            "the option write is the caller's, not a request from here"
        );
        assert_eq!(
            on.lines[0],
            (
                CAN_NOW_ACCEPT_CORPSE_LOOTING.to_owned(),
                BROADCAST_CHAT_TYPE
            )
        );
        assert_eq!(
            w.do_consent(&mut r, &a(&["off"])).option,
            Some((ACCEPT_LOOT_PERMITS_ORDINAL, false))
        );
        assert_eq!(r.0.len(), 3);
        assert!(matches!(r.0[0], Request::DisplayPlayerConsentList(_)));
        assert!(matches!(r.0[1], Request::ClearPlayerConsentList(_)));
        match &r.0[2] {
            Request::RemoveFromPlayerConsentList(m) => assert_eq!(m.name, "Bob"),
            other => panic!("{other:?}"),
        }
    }

    /// `@permit`'s two refusals and its two events.
    #[test]
    fn permit_rejects_the_sub_command_before_it_joins_the_name() {
        let mut w = World::new();
        let mut r = RecordingRequests::default();
        assert_eq!(
            w.do_permit(&mut r, &a(&["wibble", "Bob"])).lines[0].0,
            SPECIFY_VALID_PERMIT_COMMAND
        );
        assert_eq!(
            w.do_permit(&mut r, &a(&["add"])).lines[0].0,
            SPECIFY_PERSON_FOR_PERMIT
        );
        assert_eq!(w.do_permit(&mut r, &a(&["add", "+Bob", "Smith"])).sent, 1);
        assert_eq!(w.do_permit(&mut r, &a(&["remove", "Bob"])).sent, 1);
        assert_eq!(r.0.len(), 2);
        match &r.0[0] {
            // `join_args_as_name` strips the leading `+` an allegiance-titled name carries.
            Request::AddPlayerPermission(m) => assert_eq!(m.name, "Bob Smith"),
            other => panic!("{other:?}"),
        }
        assert!(matches!(r.0[1], Request::RemovePlayerPermission(_)));
    }

    /// The three channel commands share one refusal and one failure code, and differ only in the
    /// event.
    #[test]
    fn the_three_channel_commands_share_their_two_refusals() {
        let mut w = World::new();
        let mut r = RecordingRequests::default();
        for c in [
            w.do_channel_list(&mut r, &[]),
            w.do_channel_on(&mut r, &[]),
            w.do_channel_off(&mut r, &a(&["a", "b"])),
        ] {
            assert!(c.handled);
            assert_eq!(
                c.lines,
                vec![(SPECIFY_THE_CHANNEL_NAME.to_owned(), REFUSAL_CHAT_TYPE)]
            );
        }
        let unknown = w.do_channel_on(&mut r, &a(&["nosuchchannel"]));
        assert_eq!(unknown.sent, 0);
        assert_eq!(
            unknown.failure_lines,
            vec![(THAT_CHANNEL_DOES_NOT_EXIST.to_owned(), REFUSAL_CHAT_TYPE)],
            "failure event 0x422 prints at window 0, not the command's window"
        );
        assert!(unknown.lines.is_empty());
        assert_eq!(w.do_channel_on(&mut r, &a(&["fellowship"])).sent, 1);
        assert_eq!(w.do_channel_off(&mut r, &a(&["fellowship"])).sent, 1);
        assert_eq!(w.do_channel_list(&mut r, &a(&["fellowship"])).sent, 1);
        assert!(matches!(r.0[0], Request::AddToChannel(_)));
        assert!(matches!(r.0[1], Request::RemoveFromChannel(_)));
        assert!(matches!(r.0[2], Request::ChannelList(_)));
    }

    /// The three local prints.
    #[test]
    fn the_local_handlers_print_and_send_nothing() {
        let mut w = World::new();
        assert_eq!(w.do_speaker(&[]).lines[0].0, SPEAKER_RETIRED);
        assert!(w.do_endurance(&[]).lines[0]
            .0
            .starts_with("The endurance attribute"));
        let emotes = w.do_emote_list(&[]);
        assert!(emotes.lines[0].0.starts_with("Standard Emotes:"));
        assert!(emotes.lines[0].0.contains("Wave"));
        assert_eq!(emotes.lines[0].1, crate::emotes::EMOTE_LIST_TEXT_TYPE);
    }

    /// `@die` puts a question up and sends nothing; only the Yes arm sends.
    #[test]
    fn die_asks_before_it_sends_and_a_no_sends_nothing() {
        let mut w = World::new();
        let mut r = RecordingRequests::default();
        let c = w.do_die(&[]);
        assert_eq!(c.die_confirmation, Some(DIE_CONFIRMATION));
        assert_eq!(c.sent, 0, "a confirmation dialog, and nothing else");
        assert!(r.0.is_empty());
        w.confirm_die(&mut r);
        assert_eq!(r.0.len(), 1);
        assert!(matches!(r.0[0], Request::Suicide(_)));
    }
}
