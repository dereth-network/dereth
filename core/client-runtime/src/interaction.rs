//! The interactions: selection, combat, item use, and give/drop/split.
//!
//! **This module wires; it does not implement.** Every decision below belongs to another crate or
//! module and is called by name:
//!
//! | Decision | Where |
//! |---|---|
//! | the pick itself, and its three tie-breaks | [`crate::pick_geometry`] via [`crate::pick`] |
//! | what an attack, a use, a give or a split *is* | [`dereth_client_model::World`] |
//! | the `OrderedActionHeader` stamp and its rollback | [`dereth_client_net::client_session::Session`] |
//! | which element the pointer is over | `dereth_ui`'s hit test |
//!
//! This file owns viewport mouse handling, the prioritized [`SearchReason`] state
//! machine dispatched when picking finds an object, and the sender that serializes
//! [`dereth_client_model::Request`] onto the wire.
//!
//! # Three rules this module is built around
//!
//! 1. **The client predicts nothing.** No combat animation, no stamina deduction, no damage
//!    estimate, no optimistic inventory move. `set_waiting_state(1)` ghosts the icon and
//!    the client waits (`dereth_client_model`'s own module note).
//! 2. **The game-action counter is global and must roll back** on a failed send, or the server
//!    drops everything after the first hole. That lives in [`dereth_client_net::client_session::outbound`]; this file
//!    calls [`dereth_client_net::client_session::Session::send_action`] and nothing else, so there is one counter.
//! 3. **A shipped retail bug is preserved**: ending an attack request sends the attack twice in
//!    the classic UI. It belongs to the world's end-attack-request path, and this file does not
//!    soften it. So is the inventory lock with **no timeout** — a lost reply wedges the inventory
//!    exactly as it does in retail.
//!
//! # Why the toolbar's stance click is dispatched here
//!
//! The toolbar's element-message handler maps ids `0x10000192`…`0x10000195`
//! (message 1) onto combat-mode toggling. That handler belongs in
//! `dereth/client/crates/ui-screens/src/toolbar/`, but the **request** it must send is this
//! module's. So the mapping is applied host-side, to the element the click's own hit test found,
//! and the ids come from [`dereth_client_contract::combat_mode::BUTTONS`] rather than being
//! respelled. Moving it into the toolbar would be a one-line change, and the observable behaviour
//! would be the same: a click on the stance icon toggles combat mode.
//!
//! This module joins camera and movement input, picking and selection, combat, inventory and
//! equipment, and the corresponding item messages at the application boundary.

use dereth_animation::parts::LightingMode;
use dereth_client_contract::panels::external_container::ExternalContainerNotice;
use dereth_client_contract::panels::salvage::SalvageNotice;
use dereth_client_contract::view::{AllegianceAction, DropTarget, UiRequest};
use dereth_client_contract::ElementId;
use dereth_client_model::combat::AttackHeight;
use dereth_client_model::inventory::SplitState;
use dereth_client_model::{Notice, NoticeSink, RecordingRequests, Request};
use dereth_dat::RetailDatStore;
use dereth_primitives::{ObjectId, ServerTime, Viewport};

use crate::pick::WorldPicker;

/// One pointer event the UI dispatched this frame, for the host: the UI's button number (which is
/// the action id: 7 left, 8 right, 9 middle, 10/11/12 the double-click variants), the start flag
/// (the press edge), the input manager's pointer position the dispatch used, and the element under
/// the pointer after this event's hit test. The UI shell produces them; the interaction's mouse
/// dispatch consumes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiMouseEvent {
    /// The UI's button number, which *is* the action id.
    pub action: u32,
    /// The start flag — the press edge.
    pub start: bool,
    /// The input manager's pointer position, which is what the dispatch used.
    pub x: i32,
    pub y: i32,
    /// The element under the pointer after this event's hit test.
    pub over: Option<dereth_client_contract::ElementId>,
}

/// The channel-command handler's missing-text message.
///
/// The retail client stores 48 wide characters and passes them to a wide-string constructor.
/// Printed on chat type `0x1A` when a channel command is typed with no text, after
/// which the handler returns `true` — so this string and *"That is not a valid command."* are
/// mutually exclusive and a build that printed both would be wrong twice.
pub const CHANNEL_COMMAND_NEEDS_TEXT: &str = "You must specify the text you wish to broadcast!";

/// The build-owned replacement for retail's verbose version string.
///
/// Retail constructs its value from the running executable's VERSION resource. Cargo package
/// metadata is this executable's corresponding source of truth; including the package name keeps
/// a development `0.0.0` build from presenting itself as the September 2013 retail client.
///
/// The executable is the `dereth-client` package. This code lives in the library crate, whose own
/// package name is not the executable's, so the name is spelled here; the version is the
/// workspace's, shared by every crate.
const CLIENT_BUILD_ID: &str = concat!("dereth-client", " ", env!("CARGO_PKG_VERSION"));

/// The drop-release handler accepts element `0x100001D6`, the one element of the panel
/// that is not a slot and still accepts a
/// drop; see [`Interaction::accept_paper_doll_drag_object`].
///
/// Pinned as the client's own literal rather than read through
/// `dereth_client_contract::panels::inventory::PAPER_DOLL_DRAG_MASK`, because a test on this file's side
/// must be able to detect a wrong constant without going through the symbol that wrote it; the
/// `#[test]` at the foot of this module asserts the two agree.
const DOLL_DRAG_MASK: ElementId = ElementId(0x1000_01D6);

const SQUELCH_QUERY_HEADER: &str =
    "(account) denotes a character whose account has also been squelched.\n\
Format: Name : List of squelched message types.\n\
--------\n";
const SQUELCH_REPLY_WITHOUT_TELLER: &str =
    "A player must @tell you before you can squelch them with this command.";
const SQUELCH_TARGET_MISSING: &str = "You have not specified a squelch target.";

#[derive(Debug, PartialEq, Eq)]
struct SquelchCommandArgs {
    account: bool,
    name: String,
    message_type: u32,
    warning: Option<&'static str>,
}

fn squelch_text_type(word: &str) -> Option<u32> {
    if word.eq_ignore_ascii_case("Assessment") {
        return Some(dereth_client_model::chat::text_type::APPRAISAL);
    }
    (0..u32::try_from(dereth_client_model::chat::text_type::TABLE_LEN)
        .expect("small text-type table"))
        .find(|&ty| {
            let name = dereth_client_model::chat::log_text_type_name(ty);
            name != "Unknown" && name.eq_ignore_ascii_case(word)
        })
}

/// `process_squelch_args`. Its odd success return on the two missing-target paths is
/// intentional: both callers print the warning and still send an empty-name `0x0058`/`0x0059`.
fn process_squelch_args(
    args: &[String],
    last_teller_name: &str,
    require_target: bool,
) -> Result<SquelchCommandArgs, String> {
    let mut account = false;
    let mut message_type = dereth_client_model::chat::text_type::ALL_CHANNELS;
    let mut name = String::new();
    let mut reply = false;
    let mut index = 0;
    while let Some(flag) = args.get(index).and_then(|arg| arg.strip_prefix('-')) {
        index += 1;
        if flag.eq_ignore_ascii_case("reply") {
            reply = true;
        } else if flag.eq_ignore_ascii_case("account") {
            account = true;
        } else if let Some(ty) = squelch_text_type(flag) {
            message_type = ty;
        } else {
            return Err(format!("\"{flag}\" is not a valid squelch category."));
        }
    }
    if reply {
        name = last_teller_name.to_owned();
        if name.is_empty() {
            return Ok(SquelchCommandArgs {
                account,
                name,
                message_type,
                warning: Some(SQUELCH_REPLY_WITHOUT_TELLER),
            });
        }
    } else if index < args.len() {
        let joined = dereth_client_model::cmd::interp::join_args(&args[index..]);
        name = dereth_client_model::chat::join_args_as_name(&joined).to_owned();
    }
    let warning = (require_target && name.is_empty()).then_some(SQUELCH_TARGET_MISSING);
    Ok(SquelchCommandArgs {
        account,
        name,
        message_type,
        warning,
    })
}

/// Unlike the character query this
/// formats one `SquelchInfo`, so there is no hash traversal or row-order adaptation.
fn global_squelch_query(chat: &dereth_client_model::chat::ChatState) -> String {
    let mut text =
        "The following types of messages are currently being filtered globally:\n".to_owned();
    let entry = &chat.squelch.global;
    if entry.is_empty() {
        text.push_str("none");
    } else if entry.is_squelched(dereth_client_model::chat::text_type::ALL_CHANNELS) {
        text.push_str("All message types");
    } else {
        let mut first = true;
        for ty in dereth_client_model::chat::LEGAL_CHANNELS {
            if entry.is_squelched(ty) {
                if !first {
                    text.push_str(", ");
                }
                text.push_str(dereth_client_model::chat::log_text_type_name(ty));
                first = false;
            }
        }
    }
    text.push_str("\n(For a list of filter options, type @help filter)\n");
    text
}

/// List the legal squelch channels.
///
/// The original path creates an empty-name squelch record with message type 1,
/// setting all 128 bits, then enumerates only legal channels from 0 through `0x21`.
/// This implementation shares the legality and enum-name table with the parser so
/// the list cannot drift from squelch/filter parsing.
fn message_types_text() -> String {
    let channels = dereth_client_model::chat::LEGAL_CHANNELS
        .iter()
        .map(|&ty| dereth_client_model::chat::log_text_type_name(ty))
        .collect::<Vec<_>>()
        .join(", ");
    format!("Squelch channels are as follows:\n  {channels}\n")
}

/// The decimal-prefix behavior obtains from `atoi`.
/// Extreme overflow is not pinned here; saturation keeps it outside either accepted render range.
fn render_atoi(s: &str) -> i32 {
    let s = s.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let (negative, digits) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let mut saw_digit = false;
    let mut value = 0_i64;
    for b in digits.bytes() {
        if !b.is_ascii_digit() {
            break;
        }
        saw_digit = true;
        value = value.saturating_mul(10).saturating_add(i64::from(b - b'0'));
    }
    if !saw_digit {
        return 0;
    }
    let clamped = if negative {
        value.saturating_neg()
    } else {
        value
    }
    .clamp(i64::from(i32::MIN), i64::from(i32::MAX));
    i32::try_from(clamped).expect("clamped to the i32 range")
}

/// Parse a spell-component category name.
///
/// The command's names are an old narrow enum mapper, not the display captions from the component
/// panel. In particular there are no spaces in `PowderedGem` or `AlchemicalSubstance`, while the
/// two shorter synonyms are `Powder` and `Potion`.
fn spell_component_category(word: &str) -> Option<u32> {
    use dereth_client_model::magic::component_category as c;
    [
        ("Scarab", c::SCARAB),
        ("Scarabs", c::SCARAB),
        ("Herb", c::HERB),
        ("Herbs", c::HERB),
        ("PowderedGem", c::POWDERED_GEM),
        ("PowderedGems", c::POWDERED_GEM),
        ("Powder", c::POWDERED_GEM),
        ("Powders", c::POWDERED_GEM),
        ("AlchemicalSubstance", c::ALCHEMICAL_SUBSTANCE),
        ("AlchemicalSubstances", c::ALCHEMICAL_SUBSTANCE),
        ("Potion", c::ALCHEMICAL_SUBSTANCE),
        ("Potions", c::ALCHEMICAL_SUBSTANCE),
        ("Talisman", c::TALISMAN),
        ("Talismans", c::TALISMAN),
        ("Taper", c::TAPER),
        ("Tapers", c::TAPER),
        ("Pea", c::PEA),
        ("Peas", c::PEA),
    ]
    .into_iter()
    .find_map(|(name, category)| word.eq_ignore_ascii_case(name).then_some(category))
}

/// Format a squelch query using the same retained character hash
/// that feeds the squelch panel. Account-only hash entries are deliberately absent: native walks
/// only the character table, whose per-entry flag supplies the `(account)` marker.
fn squelch_query(chat: &dereth_client_model::chat::ChatState) -> String {
    let mut text = SQUELCH_QUERY_HEADER.to_owned();
    if chat.squelch.characters.is_empty() {
        text.push_str("none\n");
        return text;
    }
    text.push('\n');
    for entry in chat.squelch.characters.values() {
        text.push_str("  ");
        if !entry.name.is_empty() {
            text.push_str("Name: ");
            text.push_str(&entry.name);
            if entry.is_zone_squelch != 0 {
                text.push_str(" (account) ");
            }
            text.push(' ');
        }
        if entry.is_squelched(dereth_client_model::chat::text_type::ALL_CHANNELS) {
            text.push_str("All message types");
        } else {
            let mut first = true;
            for ty in dereth_client_model::chat::LEGAL_CHANNELS {
                if entry.is_squelched(ty) {
                    if !first {
                        text.push_str(", ");
                    }
                    text.push_str(dereth_client_model::chat::log_text_type_name(ty));
                    first = false;
                }
            }
        }
        text.push('\n');
    }
    text
}

/// Search-reason ordering matters: every viewport mouse arm replaces the reason
/// only when its current numeric value is less than the proposed one. A stronger
/// reason already set this frame wins, so one click cannot mean two things.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
#[repr(i32)]
pub enum SearchReason {
    #[default]
    None = 0,
    MouseOver = 1,
    Select = 2,
    Examine = 3,
    Use = 4,
    Drop = 5,
    Drag = 6,
    TargetedUse = 7,
}

/// **The hovered world object's tooltip.**
///
/// The object-found notice updates the wrapper's *own* tooltip only when the hovered id changes.
/// Retail follows this order: read the player's tooltip option; clear the tooltip
/// and its enabled flag if the id is zero or the option is off; otherwise obtain the appropriate
/// object name (name mode 2, final argument 0) and convert it to a wide string. An empty name
/// neither sets nor clears the old tooltip. A nonempty name becomes a literal tooltip and enables
/// delayed hover. If a drag proxy also exists and the pointer is inside the render viewport,
/// reset the current tooltip and start this one at the mouse with duration override `0.0`.
///
/// # Three things that are easy to get wrong
///
/// 1. **The hover handler does not set the tooltip.** Mouse movement and the global loop only
///    *arm the pick*, setting `looking_for_object` and returning; the answer comes back a frame
///    later through drawing, `set_found_object`, and the object-found notice.
///    This notice is the only site in the client
///    that names the hovered object.
/// 2. **The delay is not 0.0.** `start_tooltip_at_mouse`'s float argument is a *duration* override
///    (it sets the current tooltip's duration), and `0.0` means "leave the tooltip duration
///    alone". The direct call is also **only reached while something is being dragged**
///    (a drag element is set). With no drag the notice does nothing
///    but set the tooltip flag, and the tooltip appears through the ordinary delayed hover —
///    the tooltip check's delay, **0.25 s** of no pointer motion.
/// 3. **There is no possessive and no type filter.** `NAME_APPROPRIATE` (2) picks the plural name
///    when the stack size is 2 or more and the singular otherwise; the `"'s"`/`"es"` suffix belongs
///    to `NAME_PLURAL`. Nothing here looks at `ITEM_TYPE`, so a player, a creature, a
///    portal and a lifestone are named exactly as a chest is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldTooltip {
    /// Set the tooltip to the name and raise the tooltip flag, and — while a drag proxy exists
    /// and the pointer is inside the render viewport — reset the tooltip and start it at the
    /// mouse with `0.0`.
    Set {
        /// The object's context-appropriate name.
        name: String,
        /// Mouse coordinates minus the viewport origin, compared unsigned against its
        /// width and height. [`crate::app::App`] checks the accompanying drag-element
        /// condition because it owns the UI tree.
        pointer_in_viewport: bool,
    },
    /// Lower the tooltip flag and clear the tooltip — the pointer left every object, or
    /// `ShowTooltips` is off.
    Clear,
}

/// Target mode armed by the Use / Examine toolbar buttons.
///
/// The buttons set `TargetMode::Use` / `TargetMode::Examine`, which changes the cursor
/// to indicate the pending targeted action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TargetMode {
    #[default]
    None,
    Use,
    Examine,
    /// Item use armed a target request for the item retained in `targeting_object`.
    UseTarget,
}

/// A screen-layout action raised by local UI commands and consumed by [`crate::app::App`],
/// which owns the live `crate::ui::UiShell` and preferences-file identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiLayoutCommand {
    /// Save the named layout from the manual or automatic save path.
    Save(String),
    /// Load the named layout from the manual or automatic load path.
    Load(String),
    /// Lock or unlock the UI: broadcast global `0x0D` after this value was written through
    /// player-option change handler 51.
    SetLockUi(bool),
}

/// What the interactions have done, for the log line and for the tests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InteractionStats {
    /// Object searches requested by a world click, of any [`SearchReason`].
    ///
    /// **This counts requests, not arms.** `handle_drop_release`'s arm increments it without
    /// calling the object-search routine, so a rising counter is no evidence that the pick was
    /// armed. Assert [`crate::pick::WorldPicker::looking_for_object`] instead; this stays a count
    /// of gestures that asked.
    pub picks_requested: u64,
    /// `SearchReason::Drop` picks that the object search's unsigned viewport
    /// compare rejected, so no notice will answer them. Unreachable on the live tree, because
    /// `<SBOX>`'s box is the viewport; a non-zero here means the layout moved under that claim.
    pub drops_outside_the_viewport: u64,
    /// The frame loop's `SearchReason::MouseOver` searches that
    /// the object search refused, so `looking_for_object` stayed clear and no
    /// object-found notice will answer them. Each one would otherwise leave
    /// `search_reason` latched at 1 for the rest of the session. Counted
    /// rather than silently undone, because a non-zero value here is the layout putting the
    /// pointer outside `<SBOX>` and that is worth seeing.
    pub hover_searches_not_armed: u64,
    /// Hover searches refused because the pointer was over a HUD window painted
    /// over the 3-D view — the part of the render device's viewport that
    /// the game-viewport calculation subtracts in retail and that `<SBOX>`'s raw box
    /// does not.
    pub hover_searches_under_the_hud: u64,
    /// Object-found notice deliveries with a non-zero id.
    pub objects_found: u64,
    /// Tooltip assignments the object-found notice made on `<SBOX>` —
    /// i.e. hovered objects whose name reached the viewport element. Counted apart from
    /// [`Self::object_tooltips_cleared`] because "the pointer never found a named object" and
    /// "the tooltip was never taken down again" are different defects.
    pub object_tooltips_set: u64,
    /// Tooltip clears the object-found notice made — the pointer
    /// moved off every object, or the tooltip option is off.
    pub object_tooltips_cleared: u64,
    /// Hovered objects rejected by the empty-name check — an
    /// **empty** object name, which retail answers by neither setting nor clearing, so the
    /// previous tooltip stands. Its own counter because it is the one arm that leaves the
    /// element's state untouched and would otherwise read as "the hover never happened".
    pub object_tooltips_unnamed: u64,
    /// Object-selection assignments made by the object-found notice.
    pub selections: u64,
    /// [`dereth_client_model::Request`]s handed to the session.
    pub requests_sent: u64,
    /// Requests that had nowhere to go: no session, or the session refused to encode one.
    pub requests_undeliverable: u64,
    /// Requests refused by `dereth_client_model` before any bytes existed — the inventory lock, an attack in
    /// progress, an illegal combat mode. Every one of these is a refusal the client also makes.
    pub requests_refused: u64,
    /// `UiRequest`s this module consumed.
    pub ui_requests_handled: u64,
    /// Toolbar stance-icon clicks routed to combat-mode toggling.
    pub combat_mode_toggles: u64,
    /// `interpreted_state.current_style` -> `CombatState::current_style` crossings. Without a
    /// production writer for that field every dual-wielder would charge the power bar at 1.000 s
    /// instead of 0.800 s.
    pub combat_style_bridges: u64,
    /// How many times [`Interaction::note_player_physics`] changed its answer —
    /// so a test can tell "the body has been on the ground the whole time" from "the producer never
    /// ran", which are the same reading of the field alone.
    pub physics_answers_changed: u64,
    /// The same, for [`Interaction::player_ready`] — how many times the ready
    /// answer moved. Separate from `physics_answers_changed` on purpose: the two predicates share
    /// a producer call but are different questions, and a counter shared between them could not
    /// tell "the motion answer never ran" from "the motion answer never changed".
    pub ready_answers_changed: u64,
    /// How many times [`use_time`]'s step 3 raised
    /// the object-found notice from the arm that has **no scene to sweep**. The
    /// notice block sits outside the player-presence guard,
    /// so an armed pick in a scene-less frame is answered with `click_object_id == 0` rather than
    /// dropped.
    pub scene_less_notices: u64,
    /// `set_requested_attack_height` calls a key press or a combat-window
    /// button press made. Counted rather than inferred from the field, because the client's own
    /// guard means a repeat of the *same* height while a request is in progress does nothing —
    /// so "the field says Low" and "the arm ran" are different questions.
    pub attack_height_changes: u64,
    /// `end_attack_request` calls that put at least one attack on the wire.
    /// A release with nothing charged, or below the slider cap, ends the request and sends
    /// nothing; this counts the swings, not the releases.
    pub attacks_released: u64,
    /// Writes to the UI-requested power — the power (melee) / accuracy (missile)
    /// gauge. Both producers land here: the four `CombatDecrease`/`Increase` keys through
    /// [`Interaction::on_actions`] and the window's scrollbar through
    /// `UiRequest::CombatSetDesiredPower`.
    pub desired_power_changes: u64,
    /// Magic-combat actions `handle_magic_action` turned into a
    /// magic-action notice. The spellcasting panel applies the notice, so this is the producer's
    /// count and the panel's receipt count is the consumer's — two numbers, because a
    /// key that dispatches and a spell bar that answers are different claims.
    pub magic_actions: u64,
    /// `Notice`s `dereth_client_model` raised while this module was driving it — the refusal
    /// strings among them.
    pub notices: u64,
    /// Target-mode assignments the toolbar's Use / Examine buttons made.
    pub target_modes_armed: u64,
    /// Shortcut additions requested by a drop onto the quickbar.
    pub shortcuts_added: u64,
    /// Shortcut-removal events (`0x019D`) sent this session. Counted apart from
    /// [`Self::shortcuts_added`] because a drop from one tile to another
    /// sends both, and a build that sent only the add would silently leave one object in two slots.
    pub shortcuts_removed: u64,
    /// Right-button releases that ended a **mouse-look drag** and therefore did *not*
    /// arm `SearchReason::Examine` — the mouse-up handler's early return when it turns mouse look
    /// off. Without it every camera turn would appraise whatever was under the cursor.
    pub mouse_look_releases: u64,
    /// `Item_ServerSaysContainID`, wear-item and move-item applies.
    pub move_items_applied: u64,
    /// `0x0022 Item_ServerSaysContainID` replies whose item did **not** exist yet and
    /// which therefore took the second branch of the client's net-blob handler —
    /// recording the pending contained id on the container.
    ///
    /// Separate from `move_items_applied` on purpose: the two branches of one arm do opposite
    /// things (one moves an object, one records an id for an object that does not exist), and a
    /// single counter would let a build that never reached the hard branch report a healthy total.
    pub contain_ids_preplaced: u64,
    /// `Character_ServerSaysAttemptFailed` applies.
    pub attempts_failed: u64,
    /// A `0x00A0` whose object, after previous-request-id substitution,
    /// is not a known game record. The missing-object branch skips failure handling
    /// entirely.
    ///
    /// A third state prevents silence: without
    /// it, "the arm ran and the lock was already idle" and "the arm did nothing at all" are the
    /// same reading of `attempts_failed`, and only the second one leaves a held lock behind.
    pub attempts_failed_unknown_object: u64,
    /// `0x0197 Item_UpdateStackSize` applies — a **source** stack's new count after a
    /// split or a merge, which is also the only thing that releases the inventory lock a split
    /// took. Counted apart from `move_items_applied` because they answer different halves of one
    /// split: that one is the new object's placement, this one is the old object's count.
    pub stack_sizes_applied: u64,
    /// `0x0196 Item_OnViewContents` applies — a container's contents list replaced.
    pub contents_viewed: u64,
    /// `0x0052 Item_StopViewingObjectContents` applies.
    pub contents_closed: u64,
    /// `0x00C9 Item_SetAppraiseInfo` applies.
    pub appraisals_applied: u64,
    /// `0x00B4 Writing_BookOpen` applies -- how many books the panel was told about.
    pub books_opened: u64,
    /// `0x00B8 Writing_BookPageDataResponse` replies that filled a page of the open
    /// book. `.page_data_ignored` is the ones that did not.
    pub book_pages_filled: u64,
    /// `0x01B8 Combat_HandleCommenceAttackEvent` applies.
    pub attacks_commenced: u64,
    /// `0x01A7 Combat_HandleAttackDoneEvent` applies.
    pub attacks_done: u64,
    /// `0x01C0 Combat_QueryHealthResponse` blobs that reached the arm.
    pub health_responses: u64,
    /// `0x0264 Item_QueryItemManaResponse` blobs that reached the arm.
    pub mana_responses: u64,
    /// How many of those responses actually wrote a toolbar meter.
    ///
    /// This is a **separate** counter from the two above on purpose. Both handlers begin with
    /// a selected-is-`id` test and retail drops the rest, so "reached the arm" and "changed
    /// what is drawn" are different numbers and a single counter would let one masquerade as the
    /// other.
    pub selection_meters_written: u64,
    /// `0x01C7 Item_UseDone` applies — one busy-count decrement each.
    pub uses_done: u64,
    /// `0x0020 Allegiance_AllegianceUpdate` applies.
    pub allegiance_updates: u64,
    /// The member count of the tree the last `0x0020` rebuilt.
    pub allegiance_members: u32,
    /// `0x0062 Vendor_VendorInfo` handled — vendor windows opened.
    ///
    /// This counter at zero on a session that carries eight of them means `0x0062` is not
    /// being handled.
    pub vendor_opens: u64,
    /// Stock rows the last `0x0062` carried, after unpacking item profiles.
    pub vendor_stock: usize,
    /// `0x005F Vendor_Buy` put on the wire — `buy_single_item` and "Buy All".
    pub vendor_buys: u64,
    /// `0x0060 Vendor_Sell` put on the wire — `sell_single_item` and "Sell All".
    pub vendor_sells: u64,
    /// Rows added to the buy basket by "Add to List". Local; nothing is sent.
    pub vendor_basket_rows: u64,
    /// Vendor stack-size writes applied to a description
    /// the world actually holds. Zero with a non-zero
    /// `VendorPanel::stack_size_writes` means the panel decided and nothing received it.
    pub vendor_stack_sizes_set: u64,
    /// Vendor-close requests that actually closed a vendor.
    pub vendor_closes: u64,

    // ---- fellowship -------------------------------------------------------------------
    /// Fellowship requests this client has put on the wire — all seven opcodes
    /// together, because the question the counter answers is "does the Fellowship tab reach the
    /// shard at all".
    pub fellowship_requests: u64,

    // ---- friends ----------------------------------------------------------------------
    /// Add-friend, remove-friend, clear-friends and friends-command requests
    /// put on the wire -- all four together, because the question
    /// is "does the Friends tab, or `@friends`, reach the shard at all".
    pub friends_requests: u64,
    /// Add-friend refusals when the list has more than `0x31` entries -- weenie error `0x561`,
    /// whose sentence this path does not print, so it is counted instead.
    pub friends_list_full_refusals: u64,

    // ---- contracts, friends commands and fellowship dialogs ---------------------------
    /// Abandon-contract requests (`0x0316`) put on the wire. The Contracts
    /// tab's one send.
    pub contract_requests: u64,
    /// The remove-friend chat command walking the whole list and finding no
    /// such name -- weenie error `0x563`, same treatment. `@friends remove <name>` takes a name
    /// and `0x0017` takes an id, so a name nobody answers to sends nothing at all.
    pub friends_not_a_friend_refusals: u64,
    /// The display-friends chat command runs -- `@friends` and
    /// `@friends online`. The one arm of the family that prints rather than sends.
    pub friends_listings: u64,
    /// `0x0274` bodies of type 4 that reached the fellowship panel and raised a
    /// request dialog. Counted separately because gameplay and fellowship dialogs
    /// have independent context guards and may be open simultaneously.
    pub fellowship_requests_raised: u64,

    // ---- pop-up strings ---------------------------------------------------------------
    /// `0x0004 Communication_PopUpString` bodies decoded and queued for a message dialog.
    pub pop_up_strings: u64,
    /// Popup bodies whose narrow string could not be decoded, matching the original
    /// early return. This counter is separate because a missing popup must be
    /// distinguishable from a decode failure elsewhere.
    pub pop_up_strings_undecodable: u64,
    /// Dialogs made in the current UI this session — how many of [`Self::pop_up_strings`] actually
    /// reached the screen. Two counters and not one for the reason
    /// `HudStats::fellowship_members` gives: a shell that is not yet in `GAME_PLAY` drains none,
    /// and "received but not shown yet" must not read the same as "shown".
    pub pop_ups_shown: u64,
    /// Popups the player dismissed with the box's one button (child `0x26`) —
    /// whose whole body is `CloseDialog`.
    pub pop_ups_dismissed: u64,

    // ---- confirmation requests --------------------------------------------------------
    /// `0x0274` requests queued for a gameplay confirmation or an accept-swear dialog.
    /// Fellowship invitations have their own counter. Together with the retained
    /// absent-panel and unknown-type counters, these distinguish handled questions
    /// from unsupported destinations and types.
    pub confirmations_raised: u64,
    /// Retained counter for confirmations dropped because their panel was absent.
    /// Allegiance type 1 and fellowship type 4 go to their own panels, so no current
    /// handled type reaches this counter; it stays zero. Unknown types are counted
    /// separately.
    pub confirmations_for_absent_panels: u64,
    /// `is_handled` said no: the confirmation dispatcher has no default arm, so retail drops these too.
    pub confirmations_unknown_type: u64,
    /// `0x0276` bodies that matched the open dialog and closed it —
    /// the abort-confirmation handler past both of its mismatch guards.
    pub confirmations_aborted: u64,
    /// `0x0276` bodies whose `(type, context)` pair matched nothing open; retail's two
    /// mismatch guards drop them in silence.
    pub confirmations_aborts_unmatched: u64,
    /// Answers that reached the gameplay confirmation dialog's close handler, Yes and No alike.
    pub confirmations_answered: u64,

    // ---- world-object stat updates ----------------------------------------------------
    /// `Qualities_*Update*` events for a **world object** that reached
    /// the world's stat-update path and were written.
    ///
    /// The twenty-six opcodes `0x02CD`..=`0x02EA` arrive here for any subject; the HUD-owned
    /// player-description route rejects other subjects. Without this arm a world-object stat
    /// update would miss its public mirror, sequence gate and item-attributes-changed notice,
    /// so a chest unlocked by the server would stay locked locally. The quality store, sequence
    /// gate and player-scoped handlers live in `dereth_client_model::World`;
    /// `Hud::apply_quality_update` delegates to that shared store rather than owning a second
    /// player copy.
    pub object_quality_updates: u64,
    /// Public updates the object's own `PropertySequenceGate` refused as older than what it holds.
    pub object_quality_updates_stale: u64,

    // ---- world-object stat removals -----------------------------------------------------
    /// `Qualities_*Remove*Event` for a **world object** that reached
    /// the world's stat-removal path — the eight public forms, `0x01D2`, `0x01D4`,
    /// `0x01D6`, `0x01D8`, `0x01DA`, `0x01DC`, `0x01DE` and `0x02B9`.
    ///
    /// The removal counterpart of [`Self::object_quality_updates`]: this arm is
    /// `apply_stat_remove`'s production caller. Counted separately from the updates because
    /// removing a stat is a different retail path — no value, no stat-updated mirror, and the
    /// remove handler runs instead.
    pub object_quality_removes: u64,
    /// Public removes the object's own `PropertySequenceGate` refused, or that named no live row.
    pub object_quality_removes_stale: u64,
    /// `SelectionPickUp` presses that reached backpack placement with a live selection —
    /// player-action case 1.
    pub pick_ups: u64,

    // ---- trade ------------------------------------------------------------------------
    /// `0x01FD Trade_RegisterTrade` handled — negotiations the mirror opened.
    pub trade_registers: u64,
    /// `0x01FE Trade_OpenTrade` handled. **This alone does not raise the trade
    /// window.** Trade registration raises it instead;
    /// the open-trade event reaches a no-op handler. ACE never sends `0x01FE`, so
    /// `trade_registers` is the useful counter on that server path.
    pub trade_opens: u64,
    /// `0x01FF Trade_CloseTrade` handled.
    pub trade_closes: u64,
    /// `0x0200`/`0x0201` that actually moved a row in the mirror. A message whose side is neither
    /// 1 nor 2 moves nothing and still raises its notice, which is the client's behaviour, so this
    /// is deliberately not the message count.
    pub trade_rows_changed: u64,
    /// `0x0202`/`0x0203`/`0x0205`/`0x0207`/`0x0208` handled — the five that only change flags.
    pub trade_flag_messages: u64,
    /// `0x01F8 Trade_AddToTrade` put on the wire by a drop on the table.
    pub trade_adds_sent: u64,
    /// `0x0055 StackableSplitToContainer` put on the wire by a partial-stack trade drop.
    pub trade_splits_sent: u64,
    /// `0x01FA Trade_AcceptTrade` put on the wire carrying the **mirror** — the accepting branch
    /// of the trade-accept handler.
    pub trade_accepts_sent: u64,
    /// `0x01FA` sent with an empty trade payload to report desynchronization. Counted
    /// separately from acceptance because the opcode is identical and only the body
    /// differs; a silently unused path could otherwise look like missing desync
    /// detection.
    pub trade_out_of_sync_sent: u64,
    // ---- trade offers by drag ---------------------------------------------------------
    /// The automatic trade-offer notice reached its `AddItem` arm — the
    /// **drag an item onto another player** gesture completing.
    pub trade_for_dummies_offered: u64,
    /// The same notice refused: the id was already on the table (silent), or the splitter was
    /// holding part of the selected stack and the window said so.
    pub trade_for_dummies_refused: u64,
    /// `0x01FB`, `0x0204` and `0x01F7` — decline, clear-all and close-negotiations.
    pub trade_control_sent: u64,
    /// `0x02C2 Magic_UpdateEnchantment` applies the registry accepted.
    pub enchantments_updated: u64,

    // ---- enchantment purges, squelch and spell sends -----------------------------------------
    /// `0x02C6 Magic_PurgeEnchantments` that removed at least one enchantment. The purge has two
    /// outcomes and a third state: this counts only the ones that changed something, so a session
    /// where the server sent one and the registry was already empty reads as **0 changed**, not as
    /// "the arm never ran" — `enchantments_updated` above is the denominator for that.
    pub enchantments_purged: u64,
    /// `0x0312 Magic_PurgeBadEnchantments` that removed at least one.
    pub bad_enchantments_purged: u64,
    /// `0x0058 Communication_ModifyCharacterSquelch` or sibling `0x0059` put on the wire by the
    /// chat target menu's squelch row or a typed `@squelch`/`@unsquelch`. **Zero for every recorded
    /// session**: the corpus carries neither event in either direction, so this counter can only
    /// move in a socket-free command station, on a live shard, or on a loopback.
    pub squelch_requests: u64,
    /// `0x0224 Character_SetDesiredComponentLevel` sent by the spell-component panel's edit field
    /// or the typed fill-components clear sentinel. Also **zero in the corpus**.
    pub desired_comp_sets: u64,
    /// `0x0048 Magic_CastUntargetedSpell` / `0x004A Magic_CastTargetedSpell` actually put on the
    /// wire by the spellcasting panel.
    ///
    /// It counts **sends**, not requests: a spell the spell table does not know makes
    /// spell casting return without sending and without a message, so
    /// a counter keyed on "the arm ran" would report a cast that never happened. Zero in every
    /// capture — the corpus carries no `0x0048` and no `0x004A` in either direction.
    pub spells_cast: u64,
    /// `0x01E3`/`0x01E4` pairs put on the wire by the spellbook -> spell-bar
    /// transfer: the denominator that separates "the drop was refused" from "the drop never
    /// reached a request".
    pub spell_favorites_changed: u64,
    /// `0x01A8 Magic_RemoveSpell` sent by a *confirmed* DELETE.
    pub spells_deleted: u64,
    /// Rows added to the buy basket.
    pub fill_components_rows: u64,
    /// Components it could not fill — not stocked at all, plus stocked short. Both go into the
    /// same `add_missing_comp` line in the client, which is why they are summed here and counted
    /// apart in `dereth_client_model::vendor::FillComponents`.
    pub fill_components_missing: u64,
    /// `0x02C7 Magic_DispelEnchantment` applies that found their layer.
    pub enchantments_removed: u64,

    // ---- the count-prefixed trio and the expiry line ----------------------------------------
    /// `0x02C4 Magic_UpdateMultipleEnchantments` messages consumed.
    pub multi_enchantment_updates: u64,
    /// …and the entries inside them the registry accepted. Two counters because
    /// the registry update's leading parity check can reject every entry of
    /// a well-formed list, and a single counter cannot tell that from an empty list.
    pub multi_enchantments_applied: u64,
    /// `0x02C5 Magic_RemoveMultipleEnchantments` (announcing) messages consumed.
    pub multi_enchantment_removals: u64,
    /// `0x02C8 Magic_DispelMultipleEnchantments` (silent) messages consumed. Kept apart from the
    /// counter above because the **only** difference between the two opcodes is the `bool` the
    /// dispatcher pushes, so folding them would make the silent/announcing split unobservable.
    pub multi_enchantment_dispels: u64,
    /// Ids those two took out of the registry.
    pub multi_enchantments_removed: u64,
    /// *"&lt;spell&gt; has expired."* lines written by
    /// enchantment-expiration handling, from `0x02C3` and `0x02C5`.
    /// It is **not** the number of ids removed: a cooldown (`id & 0xFFFF >= 0x8000`) and a spell
    /// the spell table does not know both take an early return with no line.
    pub enchantment_expiry_lines: u64,

    // ---- the book's authoring replies ---------------------------------------------------------
    /// `0x00B6 Writing_BookAddPageResponse` consumed.
    pub book_add_page_responses: u64,
    /// …of which got past the add-page response handler's **two id refusals** (the object is not
    /// the book, or the book id is zero) and reached the panel.
    ///
    /// It is **not** the number of pages inserted: the two later arms (a failed add, or a page
    /// other than the current one) are counted past this point, in
    /// `dereth_ui_screens::panels::book::BookPanel::book_data_refetches`, because only the panel
    /// knows the current page. `BookPanel::pages_added` is the insert count.
    pub book_add_pages_relayed: u64,
    /// `0x00B7 Writing_BookDeletePageResponse` consumed. **A counter and nothing else**: notice
    /// handling is an empty stub in
    /// **every** receiver — nothing in retail handles it. See the arm.
    ///
    /// The refetch count is
    /// `dereth_ui_screens::panels::book::BookPanel::book_data_refetches`: the decision belongs to
    /// the panel, because only the panel knows the current page.
    pub book_delete_page_responses: u64,

    // ---- three empty handlers -----------------------------------------------------------------
    /// `0x01CB Item_AppraiseDone` consumed. Its handler calls an empty
    /// stub returning zero — the same one behind
    /// `0x01C9`. A counter, for the reason [`Self::appraise_done`]'s doc gives.
    pub appraise_done: u64,
    /// `0x00C3 Item_GetInscriptionResponse` consumed. The arm decodes and discards
    /// three narrow strings, with no notice or further call.
    pub inscription_responses: u64,
    // ---- allegiance ---------------------------------------------------------------------------
    /// `0x027C Allegiance_AllegianceInfoResponseEvent` consumed.
    pub allegiance_info_responses: u64,
    /// …and how many printed a report. They differ by exactly the responses whose profile does not
    /// contain the target, which is the handler's one guard (a failed profile lookup returns).
    pub allegiance_info_reports: u64,
    /// `0x0003 Allegiance_AllegianceUpdateAborted` consumed.
    /// The panel's receiver updates only when the panel is visible
    /// and **ignores the `u32` it is handed**; the queue length is the count
    /// the panel watches and this is the message count at the router.
    pub allegiance_updates_aborted: u64,
    // ---- the two channel reports and /age -------------------------------------------------------
    /// `0x0148 Communication_ChannelList` consumed — despite the name, the list of **characters**
    /// listening on a channel (the `Communication_ChannelList` handler).
    pub channel_lists: u64,
    /// `0x0149 Communication_ChannelIndex` consumed — the list of **channels**
    /// (the `Communication_ChannelIndex` handler).
    pub channel_indices: u64,
    /// Indented rows the two of them wrote, **not** counting the header each always writes. Two
    /// counters because the header goes out for an empty list too, and a single one could not tell
    /// "no reply" from "a reply naming nobody".
    pub channel_rows: u64,
    /// `0x01C3 Character_QueryAgeResponse` consumed — `/age`'s one chat line.
    pub age_responses: u64,
    // ---- portal storms ------------------------------------------------------------------------
    /// `0x02C9`/`0x02CA`/`0x02CB`/`0x02CC` consumed, in that order.
    pub portal_storms_brewing: u64,
    pub portal_storms_imminent: u64,
    pub portal_storms_struck: u64,
    pub portal_storms_subsided: u64,
    /// Portal-storm notice deliveries — every one of the four
    /// handlers makes exactly one, so this equals the sum of the four counters above. The **level**
    /// itself is the indicator state: it is not a count.
    pub portal_storm_levels: u64,
    /// `0xF630 Character_SetPlayerVisualDesc` consumed, the only one of this group that is not a
    /// game event. The arm decodes one narrow string and passes it to an empty
    /// handler.
    pub player_visual_descs: u64,

    /// `0x01A1 Character_CharacterOptionsEvent` bodies put on the wire — the
    /// whole `PlayerModule`, re-packed from the blob the server sent.
    pub player_modules_sent: u64,
    /// Polls of live object-range registrations and the exit edges
    /// they produce. `range_exits` counts panels or selections closed by distance;
    /// `range_exits_without_a_window` distinguishes an exit with no consumer. Book
    /// and housing delivery both have consumers.
    pub range_polls: u64,
    pub range_exits: u64,
    pub range_exits_without_a_window: u64,
    /// The selection range-exit handler's
    /// `is_selected_object_in_view` arm: the selection watch re-arming instead of clearing.
    ///
    /// It stays at 0 unless the latch's producer runs; a driven selection-persistence run reads
    /// 6 of 6 range exits here. See the world's selected-object visibility state.
    pub range_selection_rearms: u64,
    /// The other arm of the same test: selection range exits that ran
    /// selection assignment with arguments `(0, 0)` and **emptied the selection**.
    ///
    /// It is the complement of `range_selection_rearms` over
    /// the selection exits whose id still matched.
    ///
    /// The latch `selected_object_in_view` is set by part drawing — here, the draw path's own
    /// `WorldScene::draw` — for any selected object whose parts a frame has submitted, and cleared
    /// only by an object-search request. Without that producer every selection range exit would
    /// take the clearing arm. What is left in this counter is the case that clears in retail too:
    /// a selection no frame has drawn.
    pub range_selection_clears: u64,
    /// Option changes that the player-option change handler sends
    /// out **immediately** as `0x0005 Character_PlayerOptionChangedEvent` — twenty-two of the
    /// fifty-three — and that this client could not send.
    ///
    /// **The message exists, so this counter does not move and stays at zero.** It is kept as
    /// the gate that makes a regression visible, and a test asserts it. Nothing increments it
    /// today.
    pub option_changes_unsendable: u64,
    /// Option changes actually put on the wire as `0x0005`, the counterpart of
    /// [`Self::option_changes_unsendable`]. The five listening options among them are chat channel
    /// subscriptions, so this is also the count of channel join/leave requests a session made.
    pub option_changes_sent: u64,
    /// Option changes that only marked the module dirty, exactly as the client
    /// does: they go out at the next `save_to_server` or the 480-second flush.
    pub option_changes_deferred: u64,
    /// `PlayerOption_*` input actions that flipped an option.
    pub option_actions_toggled: u64,
    /// The player-option change handler's engine side effects
    /// (time of day, fog, weather, target tracking) that were decided and **not applied**, because
    /// the landscape, the weather and the target tracker are not wired to this module. Counted so
    /// that "the option did nothing visible" is a number rather than a silence.
    pub option_side_effects_unapplied: u64,
    /// `0x01BF Combat_QueryHealth` and `0x0263 Item_QueryItemMana` sent by
    /// toolbar selection-change handling — the clears with id 0 and the
    /// per-selection queries together.
    pub vital_queries: u64,
    /// `0x001F Allegiance_UpdateRequest` sent by
    /// allegiance-panel visibility changes — the subscribe on show and the
    /// unsubscribe on hide. The roster has no other source, so without these the Allegiance
    /// tab does nothing.
    pub allegiance_update_requests: u64,
    /// How many Swear / Break / Kick *questions* the panel's buttons raised, and how
    /// many of each the player then confirmed. Without the three button ids bound here the tab's
    /// only working gesture would be opening it.
    pub allegiance_confirmations_raised: u64,
    /// `0x001D Allegiance_SwearAllegiance` sent out of the swear dialog's Yes arm.
    pub allegiance_swears: u64,
    /// `0x001E Allegiance_BreakAllegiance` on the **patron**, out of the break dialog's Yes arm.
    pub allegiance_breaks: u64,
    /// `0x001E Allegiance_BreakAllegiance` on a **vassal**, out of the kick dialog's Yes arm.
    /// The same opcode as `allegiance_breaks` and a different argument; see
    /// the world's allegiance-break handling.
    pub allegiance_kicks: u64,
    /// A break confirmed after the patron had already gone. Retail would put a zero id on the
    /// wire; this build declines and counts.
    pub allegiance_breaks_without_patron: u64,
    /// Allegiance events raised by `@allegiance`, `@motd` and `@alh`: the
    /// twenty-four opcodes without allegiance-panel buttons. Without the four
    /// associated command handlers every one would answer "That is not a valid command." and
    /// this counter would stay zero.
    pub allegiance_command_requests: u32,
    /// Chat lines taken off the entry box and put through
    /// the chat-command handler.
    pub chat_lines_sent: u64,
    /// Paper-doll drops that took `accept_drag_object`'s
    /// auto-wield branch and put a **single** location bit on the wire.
    pub wields_requested: u64,
    /// Paper-doll drops that took the `auto_wear` branch and sent the
    /// **whole** valid-locations mask.
    ///
    /// The two counters are separate — rather than one total — because the split
    /// between them *is* the decision this path makes, and a build that always took one
    /// branch would still show a plausible total.
    pub wears_requested: u64,
    /// Double-clicks (and Use buttons, and the `USE` action) that took
    /// `determine_use_result`'s arm 2 and put a put-item-in-container or a stackable merge
    /// on the wire. **This is the pickup**, and it is counted apart from
    /// `requests_sent` because a build that sent the use request for every double-click would
    /// also show a healthy `requests_sent`.
    pub pickups_requested: u64,
    /// Arms this build reaches and does not dispatch —
    /// open-trade, the salvage panel and the minigame start, plus a wield that `plan_auto_wield` found
    /// blocked. Reported rather than silent: the gesture was understood and nothing was sent.
    pub uses_undispatched: u64,
    /// Paper-doll drops onto an **occupied** slot that started
    /// `auto_wield`'s unblock — the blocker's `0x0019` put-item-in-container went on
    /// the wire and the dropped item's `0x001A` is owed until the server confirms.
    ///
    /// Counted apart from `wields_requested` because both are "the drop was accepted and something
    /// was sent".
    pub unblocks_started: u64,
    /// Server-says-move-item retries that put the owed `0x001A`
    /// on the wire. `unblocks_started - unblock_retries` is how many are still in flight or were
    /// abandoned; the client has **no timeout** on either.
    pub unblock_retries: u64,
    /// Unblocks the server refused (the attempt-failed notice),
    /// or whose retry found nowhere to wield after all. The dropped item's icon un-ghosts.
    pub unblocks_abandoned: u64,

    // ---- item-use completion and the arms that open a panel ----
    /// Container-open notices from a double-click on one of the player's own
    /// packs. Counted apart from `requests_sent` because it sends **nothing**: it is a panel
    /// request, and a build that never raised it looks identical on the wire.
    pub contained_containers_opened: u64,
    /// Ground-object requests that actually changed the ground-container id:
    /// a corpse or a chest became the open ground container.
    pub ground_objects_requested: u64,
    /// `attempt_set_ground_object` calls that were **understood and refused**: not `OPENABLE`, no
    /// weenie at all, or the inventory lock. Its own third state, so that "nobody double-clicked a
    /// corpse" and "every corpse refused to open" cannot report the same number.
    pub ground_objects_refused: u64,
    /// View-contents responses: `0x0196` replies that were for
    /// the **requested** ground object and therefore raised the panel.
    ///
    /// `contents_viewed - ground_panels_opened` is every other container's contents, which is the
    /// common case; this counts only the ones that opened a window.
    pub ground_panels_opened: u64,
    /// `0x0052 Item_StopViewingObjectContents` replies that closed the ground container, as
    /// opposed to merely dropping a contents list.
    pub ground_panels_closed: u64,
    /// The three object-use confirmations — a PK altar, an NPK altar or a
    /// volatile rare. **Nothing was sent**; the dialog's `Yes` sends the usage confirmation.
    pub usage_confirmations: u64,
    /// The item-use result's arm 5: `0x01F6 Trade_OpenTradeNegotiations` went out.
    pub trades_requested: u64,
    /// The item-use result's arms 6 and 7 — the salvage panel and the minigame start. Neither
    /// sends anything in retail either, so this counter is the only evidence they ran.
    ///
    /// It also counts the salvage half. The notice is carried to `panels::salvage` as well, so
    /// a non-zero here with [`Self::salvage_panel_notices`] at zero means the minigame arm and
    /// nothing else.
    pub panels_requested: u64,
    /// Salvage-panel notices queued by [`Interaction::absorb`] —
    /// the producer-side count, so a zero here and a non-zero `panels_requested` localises the
    /// break to the sink rather than to the use path.
    pub salvage_panel_notices: u64,
    /// `UiRequest::SalvageItems` that put an `0x027D` in the outbox.
    pub salvage_requests: u64,
    /// Notices delivered to the chess panel —
    /// chess boards double-clicked. A non-zero [`Self::panels_requested`] with a zero here is the
    /// salvage arm alone, and this counter is the whole of "the board window opened".
    pub minigame_boards_used: u64,
    /// Chess messages the window put in the outbox -- the `0x0269` a use sends,
    /// plus the `0x026A`/`0x026B`/`0x026D`/`0x026E` its three buttons and its board send.
    pub minigame_requests: u64,
    /// Chess gestures the panel raised that the model took -- a button click, a
    /// board press that resolved to a square, or a resign-dialog answer.
    ///
    /// Separate from [`Self::minigame_requests`]: selecting a piece or refusing an
    /// invalid move is a handled gesture that sends nothing. Diverging counts can
    /// therefore show working local rules rather than broken delivery.
    pub minigame_gestures: u64,
    /// `UiRequest::DisplayChatText` lines a panel put in the scroll.
    pub panel_notice_strings: u64,
    /// `UiRequest::HousePayment` that put an `0x021C` or an `0x0221` in the
    /// outbox through the housing panel's payment operation.
    pub house_payments_sent: u64,
    /// Partial-stack slumlord drops that reached the generic inventory split
    /// request. The authoritative create selects the result; this does not count the second,
    /// whole-stack drop that adds the payment row locally.
    pub house_splits_sent: u64,
    /// `UiRequest::HouseQueryLord` that put an `0x0258` in the outbox —
    /// the failed-house-transaction handler's retry, which is the client's **only**
    /// sender of the query-lord event.
    pub house_lord_queries: u64,
    /// Object use's general path refused with one of the client's own six reasons.
    pub uses_refused: u64,

    // ---- tell, reply and retell --------------------------------------------------------------
    /// Tells that reached the wire: one `0x005D Communication_TalkDirectByName` per `@tell` and
    /// `@retell`, one `0x0032 Communication_TalkDirect` per `@reply`.
    pub tells_sent: u64,
    /// Chat commands that were understood and **refused with one of the client's own four
    /// strings**, raised as `Notice::DisplayString { channel: 0x1A, .. }`. Its own counter, not
    /// folded into `requests_refused`, so that a missing handler shows as a zero here.
    pub chat_commands_refused: u64,
    /// The retell command's empty-line arm — the one place the client understands a verb and
    /// answers with
    /// **nothing at all**. Counted so that "retail is silent here" and "this build lost the line"
    /// are different readings; every other silent drop in this arm is now one of the three above.
    pub tells_dropped_silently: u64,
    // ---- the notice → screen chain ---------------------------------------------------------
    /// Strings handed to [`dereth_client_model::scroll::Scroll`]: one per `Notice::DisplayString`,
    /// plus three combat refusal paths that write directly to the scroll.
    ///
    /// **This counter is the seam.** `notices` above counts everything the sink saw; this counts
    /// what reached a surface that draws, so a zero here beside a non-zero `notices` means
    /// nothing reached the screen.
    pub notice_strings_scrolled: u64,
    /// Verbs the command table resolves to a handler this build does not dispatch.
    ///
    /// Every handler the command table names is dispatched (the handler census test holds the list
    /// of undispatched ones empty), so this stays at zero. It is a tripwire: a verb that lost its
    /// handler would answer weenie error `0x26`'s "That is not a valid command." where retail runs
    /// the handler, and would count here, so a non-zero value is a regression rather than a silent
    /// change of behaviour.
    pub chat_commands_unimplemented: u64,
    /// `@loc` lines the `@loc` handler printed to the chat scroll.
    /// The two refusals (any argument, cell `0`) count under
    /// [`Self::chat_commands_refused`].
    pub loc_lines_printed: u64,
    // ---- the character, comms, consent and local families -------------------------------------
    /// Requests the twenty-four `dereth_client_model::chat_cmd` handlers put on the wire. Kept apart from
    /// [`Self::allegiance_command_requests`] and [`Self::friends_requests`] so a family that
    /// regresses to the catch-all shows as a zero here rather than being masked by a neighbour.
    pub chat_command_requests: u32,
    /// Lines the same handlers printed — every refusal, acknowledgement and listing, at whatever
    /// chat type retail passes. The refusals also count under [`Self::chat_commands_refused`].
    pub chat_command_lines: u64,
    /// `@die` questions raised. The `0x0279` itself is counted by
    /// [`Self::chat_command_requests`] when the Yes arm runs, so a raised question with no send is
    /// visible as the difference.
    pub die_confirmations_raised: u32,
    // ---- `@house abandon`'s two stages -------------------------------------------------------
    /// Callback dialogs raised with the first house-abandon callback —
    /// questions raised — the **first** *"Do you really want to abandon your house?"*.
    pub house_abandon_first_raised: u32,
    /// The first house-abandon callback's Yes arm — the **second**
    /// *"Are you absolutely certain…"* question. Counted separately because the whole point of
    /// the two-stage dialog is that the first Yes sends nothing, so
    /// `house_abandon_first_raised > house_abandon_second_raised` is the shape of a refusal at
    /// stage one and `second_raised > 0` with no `0x021F` the shape of one at stage two.
    pub house_abandon_second_raised: u32,
    /// `*name*` and `<name>` runs resolved out of a
    /// spoken line. A run that misses `inq_chat_pose_command` is silent and does not count here.
    pub poses_resolved: u64,
    /// Soul-emote (`0x01E1`) messages a pose put on the wire — fewer
    /// than [`Self::poses_resolved`] when a pose's emote for others is empty.
    pub soul_emotes_sent: u64,
    /// Local `"You wave."` echoes a pose printed — the `Communication_HearSoulEmote` handler run
    /// with sender 0, `"You"` and the pose's own emote. Counted apart from the send because the
    /// two carry **different** strings.
    pub pose_echoes_printed: u64,
    /// The clear command's clear-chat-buffer notices. See
    /// `Interaction::pending_chat_clears` for what is and is not built behind it.
    pub chat_buffer_clears: u32,
    // ---- the channel-command words ------------------------------------------------
    /// Channel broadcasts (`0x0147`) sent because one of the channel-command handler's nineteen
    /// command words was typed. Kept apart from the talk-focus dropdown's own `0x0147`s, because
    /// one counter for both would hide a typed half stuck at zero behind a working dropdown half.
    pub channel_commands_sent: u64,
    /// The channel command's `argc <= 0` arm: a bare `@f` with nothing to say. Retail prints
    /// [`CHANNEL_COMMAND_NEEDS_TEXT`] and **returns `true`**, so this is not a refusal and is not
    /// counted as one.
    pub channel_commands_without_text: u64,
    // ---- parent-container changes -----------------------------------------------------
    /// New-parent-container deliveries that actually **changed**
    /// the active pack — i.e. those that passed both of
    /// the new-parent-container handler's guards and named a different pack.
    ///
    /// Its own counter rather than a fold into `ui_requests_handled`, because the two failure
    /// modes are opposite: "the panel never told anyone" and "the panel told us about a container
    /// the object tables have never seen, or one the player does not own" are different bugs, and
    /// only this counter can tell a wired seam from a refused one.
    pub open_containers_changed: u64,

    // ---- the tab-target cycle ----------------------------------------------------------------
    /// Selection-cycle calls made by one of the **sixteen**
    /// player-action selection arms — the *first* call of each arm, one per
    /// action delivered.
    ///
    /// Counted apart from [`Self::selection_cycle_wraps`] because the two answer different
    /// questions and a single total cannot: "the arm ran" and "the arm ran and had to wrap" are
    /// the first and second halves of every non-"Closest" action, and an arm whose wrap never
    /// fires looks exactly like an arm that always finds something.
    pub selection_cycles: u64,
    /// The **second** call the twelve non-"Closest" arms make when the selected id did not move.
    /// Retail compares the selected id before and after the first call. This is the
    /// wrap-around: "Next" past the farthest object comes back to the nearest.
    pub selection_cycle_wraps: u64,
    /// `0x1000003E` / `0x1000003F` deliveries whose winning selection was a corpse and therefore
    /// reached item use. The other fourteen arms never touch it, and
    /// `0x10000121` / `0x10000122` deliberately select and stop.
    pub selection_corpse_uses: u64,
    /// Selection actions that ran against an **empty** geometry snapshot — no local body, so
    /// [`crate::selection_geometry::SceneSelectionPhysics`] could answer for nothing and the cycle
    /// could not select anything. A denominator: without it "the cycle selected nothing because
    /// every candidate was filtered" and "the cycle selected nothing because it was asked on a
    /// loading screen" are the same silence.
    pub selection_cycles_without_geometry: u64,
    /// `SelectionLastAttacker` (`0x10000038`) deliveries — player-action `case 0xD`,
    /// which is the only caller of the radar-range check.
    ///
    /// Counted apart from [`Self::selection_cycles`] because it is not a `select_next` cycle at
    /// all: it selects one nominated id or nothing, and folding it into the cycle count would
    /// make a dead arm and a live one read alike.
    pub selection_last_attacker: u64,
    /// Next/previous fellowship selection reaching the object-selection assignment
    /// — **not** merely the arm running. Both functions have early returns that select
    /// nothing (no fellowship at all; an empty one, for `Next`), and a counter that could not tell
    /// those from a delivered selection would make the dead case and the live one read alike.
    pub selection_fellow_cycles: u64,
    /// Player-action `case 0x1000002E` reaching its
    /// selection assignment — i.e. the previous selected id was non-zero.
    /// The zero-history early return leaves this alone, which is the distinction between
    /// *"P did nothing because there is no history"* and *"P has no arm"*.
    pub selection_previous_restores: u64,
    /// Actions of input map `0x10` that reached the system-key handler's
    /// unconditional success return — consumed and deliberately doing nothing.
    ///
    /// A counter for a body that is empty on purpose looks odd until you ask how else a test can
    /// tell *"the arm ran and did nothing"* from *"the action fell through `_ => {}`"*. The
    /// unconsumed-event count answers the second question; this answers the first.
    pub system_keys_swallowed: u64,
    /// Auto-target calls from the tail of combat-mode changes.
    pub auto_targets: u64,
    /// `0x01B2` and `0x01B4` deliveries that reached
    /// defender-notification auto-targeting — i.e. how many times
    /// `CombatState::last_attacked_time` was stamped.
    ///
    /// A denominator, and the one that matters for this arm: the stamp is unconditional and the
    /// `auto_target` call below it is not, so "no notification arrived" and "a notification arrived and
    /// the gate refused" must be different readings.
    pub defender_notifications: u64,
    /// Of those, how many ran `auto_target`.
    ///
    /// Counted apart from [`Self::auto_targets`], which is the combat-mode change call site:
    /// the two gates differ (no selection here against `get_attack_target` +
    /// `object_is_attackable` there), so one counter for both would hide a dead arm behind a live
    /// one.
    pub defender_auto_targets: u64,
    /// The examination action (`0x1000002B`) taking its **close-first** leg —
    /// hiding `<EXAM>` `0x100005F7`, instead of
    /// examining the selected object.
    pub examine_panel_closes: u64,

    // ---- the combat selection-change handler ------------------------------------------------
    /// Combat selection-change handler calls — one per absorbed selection-change notice.
    pub selection_change_notices: u64,
    /// Of those, how many reached `auto_target` through the handler's tail call.
    ///
    /// A third counter beside [`Self::auto_targets`] and [`Self::defender_auto_targets`], for the
    /// same reason those two are separate: this is the **only** one of `auto_target`'s four call
    /// sites that can reach the `>= 15.0` fallback leg, because it is the only one that does not
    /// stamp `last_attacked_time` on the way past.
    pub selection_change_auto_targets: u64,
    /// Of those, how many times the notice was consumed by the clear-once `target_willingly_lost`
    /// instead of auto-targeting. Counted apart so that "the flag refused it" and
    /// "some other gate refused it" are different readings.
    pub selection_changes_willingly_lost: u64,

    // ---- one counter per arm, because six in aggregate would pass with five dead ---
    /// `SelectLeft`/`SelectRight` arming `leave_target_mode` — the press edge with a
    /// target mode up.
    pub leave_target_mode_armed: u64,
    /// The per-frame UI update acting on it: the target mode actually dropped.
    pub target_modes_left: u64,
    /// `EscapeKey` reaching its finish-jump leg (jump power above `0.0`).
    pub escape_finish_jumps: u64,
    /// `EscapeKey` setting the target mode to `TargetMode::None`.
    pub escape_target_mode_clears: u64,
    /// `EscapeKey` reaching the visibility toggle for `0x1000001B` — nothing
    /// selected, so Escape opens the gameplay options panel.
    pub escape_options_toggles: u64,
    /// `EscapeKey` setting `target_willingly_lost` and clearing the selection.
    pub escape_deselects: u64,
    /// `EscapeKey` reaching `stop_completely` — the "you were doing something" leg.
    pub escape_stops: u64,
    /// `EscapeKey` printing *"Action interrupted"*, which happens on that leg only
    /// when the standing-still check said **false**.
    pub escape_interrupts: u64,
    /// `EscapeKey` reaching `abort_automatic_attack`.
    pub escape_attack_aborts: u64,
    /// Screenshot actions asking the rendering device to save an image.
    pub screenshots_requested: u64,
    /// `ToggleHelp` reaching the help-page request `(0, 0x10000001)`.
    pub help_opens: u64,
    /// Plugin-manager toggle actions taking the platform's open operation.
    pub plugin_manager_opens: u64,
    /// The same arm taking the close operation — the other half of the toggle, counted
    /// separately so that a single press cannot be mistaken for a working toggle.
    pub plugin_manager_closes: u64,
    /// `ToggleRadarPanel` flipping the radar-visible flag and sending
    /// the radar-visibility update notice.
    pub radar_visibility_notices: u64,
}

/// A `bool` whose zero value is `true`.
///
/// It lives in `dereth_client_runtime::flags`, because `character.rs` names it; this path
/// resolves through the `pub use`.
pub use crate::flags::StartsTrue;

/// Viewport selection, combat input and shared UI targeting state used by this router.
#[derive(Debug, Default)]
pub struct Interaction {
    /// `WorldObjects`'s pick state and the geometry it sweeps.
    pub pick: WorldPicker,
    /// Search reason, reset at the end of every object-found notice.
    reason: SearchReason,
    /// The item a `SearchReason::Drop` is carrying.
    drop_item: ObjectId,
    /// The world
    /// selection blink's step, 0 while idle. The object-found notice sets it
    /// to 1 on a select-or-higher find; the main loop counts it to 5 and back to 0.
    flip_count: u32,
    /// Next selection-blink step time, in local-timer seconds.
    time_next_flip: f64,
    /// The id of the **last** notice raised when the smart box finds an object,
    /// stored unconditionally at its tail.
    /// Hover notices write it too, so a blink is cancelled the moment a hover finds a different
    /// object.
    iid_selected_object: ObjectId,
    /// Object-lighting changes queued for [`crate::app::App`], because this router
    /// does not own the scene object registry. App calls
    /// `WorldScene::apply_object_lighting` in the same frame;
    /// [`Self::take_pending_lighting`] drains the queue.
    pending_lighting: Vec<(ObjectId, LightingMode)>,
    /// Tooltip assignment or clearing from object-found handling,
    /// queued for [`crate::app::App`] because this router owns neither the UI tree
    /// nor the viewport element. [`Self::take_world_tooltip`] drains it. `None` means
    /// no call occurred, distinct from [`WorldTooltip::Clear`].
    pending_tooltip: Option<WorldTooltip>,
    /// Current UI target mode.
    target_mode: TargetMode,
    /// Current and maximum split sizes, which the stack slider writes.
    split: SplitState,
    /// The command table and typed-line history used to process chat input.
    chat: dereth_client_model::cmd::CommandInterp,
    /// What the mouse and the screens asked for this frame, queued by [`Self::queue`].
    mouse: Vec<UiMouseEvent>,
    ui_requests: Vec<UiRequest>,
    /// A requested combat-mode toggle awaiting the host's world borrow. The toolbar
    /// element handler cannot borrow the mutable world during UI dispatch.
    pending_combat_toggle: bool,
    /// The last `interpreted_state.current_style` copied across the
    /// motion/combat seam, so the copy is edge-driven and a test can tell "the style did not move"
    /// from "the bridge never ran".
    combat_style_bridged: Option<u32>,
    /// The second half of the same seam.
    /// The combat-ready predicate's missile arm reads
    /// the interpreted motion state's `forward_command` as well as
    /// `current_style`, and in the client both come off the one motion
    /// interpreter the combat system asks directly. Edge-driven for the same reason as the style:
    /// "the command did not move" and "the bridge never ran" must be different readings.
    combat_forward_command_bridged: Option<u32>,
    /// How many defender notifications (`0x01B2` and `0x01B4`) have arrived since
    /// the last frame ran their shared tail.
    ///
    /// A count rather than a flag because retail runs the tail **per handler call**, and the
    /// `auto_target` inside it is not idempotent: the first one can select something, and the
    /// second then finds a non-zero selected id and refuses. Collapsing two notifications into one
    /// would make the second invisible. Defender-notification auto-targeting is
    /// run once per count, in order, in the same `App::frame` — see that function for the seam.
    pending_defender_notifications: u32,
    /// Whether examination element `0x100005F7` is currently visible,
    /// as of this frame's UI update.
    ///
    /// The original action handler looks up that element and checks visibility. Here
    /// the UI tree is behind `dereth_ui_screens`, so `App` asks the live
    /// `ExaminationPanel` and pushes the answer before each `use_time`, alongside
    /// the motion-stance input.
    ///
    /// **A default of `false` is not neutral here — it is "the panel is shut", which is the
    /// common case and would hide a missing producer.** So the wiring is asserted rather than
    /// assumed: a test drives the whole `App::frame` and requires the *second*
    /// press to close a panel that a real `0x00C9` opened.
    examine_panel_open: bool,
    /// Set by the `0x1000002B` arm when it took the close-first leg; drained by
    /// `App` immediately after `use_time` returns, which is where the panel is hidden.
    ///
    /// The arm cannot call it itself for the same reason it cannot ask the question itself.
    examine_panel_close: bool,

    // ---- the combat selection-change handler ------------------------------------------------
    /// How many selection-change broadcasts have been absorbed
    /// since the last frame ran their combat handler.
    ///
    /// A count and not a flag, for exactly the reason
    /// [`Self::pending_defender_notifications`] is one — and here it matters more, because the
    /// handler's `target_willingly_lost` is **clear-once**: two notices in a frame are a refusal
    /// followed by an auto-target, and collapsing them makes the second unreachable.
    pending_selection_changes: u32,

    /// Split-stack notice carrying the selected id, raised by
    /// selection-action case 2. The toolbar/UI tree belongs to `App`, so the selected id
    /// crosses that seam and is drained immediately after this input batch.
    pending_split_stack: Option<ObjectId>,

    /// UI-layout save/load commands. The command interpreter has no UI
    /// tree, so [`crate::app::App`] drains them against gameplay in the same frame.
    pending_ui_layout_commands: Vec<UiLayoutCommand>,

    /// The process-lifetime flag the frame-rate command toggles before raising a
    /// frame-rate-display notice.
    /// The smart-box receiver is driven by `App`, which
    /// owns both the live screen and the renderer globals the meter reads.
    framerate_display: bool,
    /// The ordered edges raised by toggles this frame.
    /// Notice handlers have no replay history, so App drains this even when no gameplay screen
    /// exists rather than deriving it again from [`Self::framerate_display`].
    pending_framerate_display: Vec<bool>,

    /// Ordered preference writes returned by the render-option command. Interaction owns the
    /// command source; App owns the preference registry and live render consumer.
    pending_render_preferences: Vec<(&'static str, dereth_client_contract::view::PrefValue)>,

    /// Ordered payloads from the title command. The live gameplay
    /// screen and its retained `PlayerModule` write-back share one App-owned delivery point.
    pending_chat_window_titles: Vec<(u32, String)>,

    /// The component-list fill tail asks its owning panel to open the Buying
    /// tab. The command/model half has no UI tree, so App drains this one exact host effect after
    /// the input batch. The captured vendor identity prevents the edge from applying to a shop
    /// that replaced it before the host drain; the option is taken every frame.
    vendor_buying_tab_requested: Option<ObjectId>,

    // ---- the state owned by six UI action arms ---------------
    /// Whether the per-frame UI update should leave target mode.
    ///
    /// Set by the `SelectLeft`/`SelectRight` arm and consumed by
    /// the per-frame UI update, which is what makes the use/examine cursor a
    /// **one-shot**: the click that armed the flag still executes the target mode, and the mode is
    /// dropped at the end of the frame whether or not the click hit anything.
    leave_target_mode: bool,
    /// Radar visibility, which the `ToggleRadarPanel` arm
    /// flips. **`true` at construction**, as stored by the native constructor,
    /// and again on `on_end_character_session` — not a default chosen here, which is why it
    /// is a [`StartsTrue`] rather than a bare `bool` under this struct's `derive(Default)`.
    radar_visible: StartsTrue,
    /// Whether the platform reports the plugin manager open, held here because Keystone is not
    /// rebuilt.
    /// The `0x7C` arm's *decision* is the transcription; the state it decides on is the client's.
    plugin_manager_open: bool,
    /// Set by the `CaptureScreenshot` arm; drained by `App`, which owns the device.
    screenshot_requested: bool,
    /// Set by `EscapeKey`'s `stop_completely` leg **and by the cast arm**; drained by `App`, which
    /// owns the body. See [`Self::take_stop_completely`].
    stop_completely_requested: bool,
    /// The server-control flag that
    /// the maybe-stop check tests before it tail-calls
    /// `stop_completely`. Pushed in by `App::interaction_use_time` from
    /// `MovementCommands::lists`, which is where `lose_control_to_server` sets it.
    ///
    /// The original startup value is also false. A client that never loses control
    /// therefore stops on every cast, the common case.
    controlled_by_server: bool,
    /// Set by `EscapeKey`'s "nothing is selected" leg to the visibility-toggle action;
    /// drained by `App`, which owns the UI tree.
    visibility_toggle_requested: Option<u32>,
    /// Standing-still state pushed in by [`use_time`] from the body's
    /// own motion interpreter each frame, or `true` when no physics body exists.
    ///
    /// **`true` is the retail answer for "there is no body"**, so the
    /// default is the client's rather than a convenience; it is still asserted, because a headless
    /// frame reaches it by both routes.
    standing_still: StartsTrue,
    /// The mouse-down table entry this module needs: which element the last left press
    /// landed on. Mouse-up raises the click only when the press was on the
    /// **same** element, so a press that drags off a stance icon and releases elsewhere is not a
    /// click — and neither is a release over one that was never pressed.
    left_pressed_on: Option<ElementId>,
    /// Where the right button went down in the viewport, for
    /// the wrapper's mouse-up mouse-look gate. See [`Interaction::wrapper_mouse`].
    right_pressed_at: Option<(i32, i32)>,
    /// The latest cursor position in window coordinates.
    ///
    /// The original drop-release handler reads shared input state because element
    /// message `0x15` carries the two drag/drop elements but no coordinates.
    /// [`use_time`] step 1 writes every pointer event's position before dispatch,
    /// so this value is the position used to deliver that event.
    cursor: (i32, i32),
    /// The render target width and height
    /// — the back buffer, which is what every caller of [`use_time`] passes.
    ///
    /// Kept here for the same reason as [`Interaction::cursor`]: the drop arm is reached from
    /// [`Interaction::run_ui_requests`], which has no viewport argument and has three callers
    /// outside this file. Written by [`use_time`] before step 2, so a pick armed by a drop is
    /// measured against the same rectangle as a pick armed by a click in the same frame.
    ///
    /// **This is not the viewport:** it is the extent the viewport is clamped into, and the two
    /// are different rectangles.
    screen: (u32, u32),
    /// The viewport x, y, width, and height — the four fields
    /// object picking reads from the render-device singleton,
    /// and the rectangle `Gpu::set_viewport` actually installs.
    ///
    /// Pushed in once a frame by [`Self::note_game_viewport`], exactly as
    /// [`Self::note_examine_panel_open`] and [`Self::note_player_physics`] push in the other
    /// globals this file cannot reach. `None` is "no UI shell, no gameplay screen, or no
    /// `<SBOX>`" — the whole back buffer, which is
    /// `crate::world::WorldScene::view_params`'s own fallback and
    /// the viewport computation's answer with nothing docked. It is therefore
    /// what a headless harness gets without saying anything, which is why every existing
    /// `(800, 600)` in this file's callers still measures what it always measured.
    game_viewport: Option<Viewport>,
    /// Whether the pointer is over the 3-D view rather than a HUD window painted
    /// over it — the half of the render device's viewport that
    /// the game-viewport calculation supplies in retail and that `<SBOX>`'s raw box does not. Pushed in once a frame
    /// by `crate::app::App::pointer_over_game_view`; see [`Self::note_pointer_over_game_view`]
    /// for why the default is `true`.
    pointer_over_game_view: StartsTrue,
    /// Query the player's gameplay position, reduced to the one
    /// question [`Self::place_in_3d`] asks of it: is the player on the ground?
    ///
    /// `None` is the client's **null pointer**, not "unknown".
    /// Retail first obtains the player's id and physics object, refusing with
    /// "You cannot do that in mid air" if the object is absent. It then queries whether that
    /// object is on the ground and take the same refusal when it is not.
    /// Thus a null object and an airborne one take the **same** branch, and `None` here is read as
    /// `false` at the one site that consumes it. `\[verified\]`
    ///
    /// Stored here for the same reason as [`Interaction::cursor`] and
    /// [`Interaction::game_viewport`]: [`Self::on_world_object_found`] has no scene
    /// argument. The original viewport handler likewise queries shared physics state.
    /// [`Self::note_player_physics`] supplies this value once per frame.
    player_on_ground: Option<bool>,
    /// The combat-ready predicate every `ready:
    /// bool` in this file reads; see
    /// [`Self::note_player_physics`] for what it is and for what it is *not*.
    ///
    /// A plain `bool`, not an `Option`: the ready-position check answers `false` when there is no
    /// physics object, so "no body" and "not ready" are the same answer in the client and there
    /// is no third state to represent.
    player_ready: bool,
    /// Pending-motion state of the player's physics object, with
    /// **`None` for the client's null pointer**, as confirmed by the native null-object branch.
    ///
    /// [`Self::player_ready`] flattens this to a `bool` and is the whole answer
    /// only in `NONCOMBAT_COMBAT_MODE` and `MAGIC_COMBAT_MODE`; the melee and missile arms need
    /// the three-state form, because with the argument `true` "no body" answers `false` and "a body
    /// with a motion outstanding" answers `true`, and a `bool` cannot hold both.
    player_motions_pending: Option<bool>,
    /// The requests this frame produced, drained into the session by [`use_time`].
    outbox: Vec<Request>,
    /// Ordered notices for the current gameplay subscriber; App drains even without a UI.
    pending_external_container: Vec<ExternalContainerNotice>,
    /// The open-salvage-panel notice and the two item notices, in
    /// producer order, for `dereth_ui_screens::panels::salvage`.
    ///
    /// The open-panel notice also reaches `Notices::panels` -- a `u64` shared with the minigame
    /// start whose only reader is `stats.panels_requested`. That counter is kept (it is the only
    /// evidence the minigame arm ran); this queue is what acts when a tinkering tool is used.
    pending_salvage: Vec<SalvageNotice>,
    /// One-frame queue for housing range exits at the nine-unit threshold.
    pending_slumlord_range_exits: Vec<ObjectId>,
    /// One-frame queue for book range exits at the book's use radius.
    pending_book_range_exits: Vec<ObjectId>,
    /// The retained dialog object and prompt kind, in producer order.
    /// The callback dialog keeps the object in property `0x1000003D`; selection may change while
    /// the question is open and must not redirect the eventual Yes.
    pending_usage_confirmations: Vec<(
        ObjectId,
        dereth_client_model::inventory::use_object::UsageConfirmation,
    )>,
    pending_targeted_confirmations: Vec<(
        ObjectId,
        ObjectId,
        dereth_client_model::inventory::targeted_use::TargetedUsageConfirmation,
    )>,
    /// The ids the automatic trade-offer notice said to offer, for the trade
    /// panel's own `AddItem` hop. It goes through the same one-frame queue a drop on the table
    /// uses (`GamePlayScreen::trade_drops`), because the optimistic row and the `0x01F8` are the
    /// panel's to raise and `RemainingPanels` lives on `Hud`.
    pending_trade_for_dummies: Vec<ObjectId>,
    /// Case `0x100000D6`'s second arm:
    /// one entry per close the player asked for and the client refused to do silently, carrying
    /// the prompt the client puts in property `0xC5`.
    ///
    /// A `Vec` and not a flag for the dialog context's own reason — retail opens the dialog only
    /// when no dialog context is held, so a second press while the dialog is up adds nothing, and
    /// the queue `crate::target_confirmation::TargetedDialogs` drains is where that shows.
    pending_vendor_close_confirmations: Vec<&'static str>,
    /// The slumlord panel's two local questions. `false` is the landscape-buy
    /// slot and `true` the rent-by-proxy slot; each is independently guarded by its native
    /// dialog-context equivalent in the panel/dialog service.
    pending_house_payment_confirmations: Vec<bool>,
    /// The five gameplay confirmation types, carried as
    /// `(ConfirmationType, context id, server text)`.
    ///
    /// The server text is unchanged here. Four types append " Continue?" and `Yes_No`
    /// does not; `crate::target_confirmation::TargetedDialogs` makes that UI
    /// decision when it builds the dialog.
    pending_server_confirmations: Vec<(i32, u32, String)>,
    /// This frame's `0x0274` type-**4** bodies — `(context, inviter's name)` — for
    /// the fellowship invitation dialog. A separate queue from
    /// [`Self::pending_server_confirmations`] because retail's two dialog contexts are separate:
    /// an invitation and an "are you sure" can be on screen at once.
    pub pending_fellowship_requests: Vec<(u32, String)>,
    /// `0x0276 Character_ConfirmationDone`, as `(ConfirmationType, context id)` —
    /// relayed by the confirmation-done handler's single notice call.
    pending_confirmation_aborts: Vec<(i32, u32)>,
    /// `0x0274` with type **1** — the swear-allegiance request notice — as
    /// `(context id, the would-be vassal's name)`. The server's text field *is* the name here:
    /// the accept-swear confirmation dialog puts it straight into its confirmation string's one
    /// player-name variable.
    pending_swear_requests: Vec<(u32, String)>,
    /// This frame's `0x0004 Communication_PopUpString` bodies, in arrival order, for
    /// `crate::target_confirmation::TargetedDialogs` to turn into
    /// current-UI dialog creation calls.
    ///
    /// A `Vec` and **no** "one at a time" guard, unlike every other queue above it: retail puts
    /// these on queue **1** (`0xC3 = 1`, the all-at-once list), so five prompts in one packet burst
    /// are five boxes rather than one box and four waits. One recorded long solo session carries
    /// eighteen of them.
    pending_pop_up_strings: Vec<String>,
    /// Allegiance confirmations as `(action, target, prompt)`, for
    /// `crate::target_confirmation::TargetedDialogs`. A vector preserves requests
    /// until the dialog service applies its context guard; another press while a
    /// question is already open is dropped there.
    pending_allegiance_confirmations: Vec<(AllegianceAction, ObjectId, String)>,
    /// The `@die` command's callback-dialog
    /// prompt, for `crate::target_confirmation::TargetedDialogs` to put on screen.
    /// A `Vec` for `pending_vendor_close_confirmations`' reason, and the only chat command in
    /// retail that opens a dialog of its own.
    pending_die_confirmations: Vec<&'static str>,
    /// `@house abandon`'s prompts, first stage and second, for
    /// `crate::target_confirmation::TargetedDialogs`. Two queues rather than one because the
    /// two stages carry different callbacks: stage one's Yes raises stage two, and only stage
    /// two's Yes sends the abandon-house request.
    pending_house_abandon_first: Vec<&'static str>,
    pending_house_abandon_second: Vec<&'static str>,
    /// `@clear`'s clear-buffer notice carrying the
    /// window the player asked to clear, `0` meaning every window.
    ///
    /// **This queue has no consumer yet.** The
    /// chat log itself lives in the chat interface, inside
    /// `GamePlayScreen::chat_windows`, on the far side of the UI shell; the hop from here into it
    /// is `hud.rs` plus `gameplay.rs` plus the shell wiring. What
    /// the arm does buy today is that `@clear` stops answering *"That is not a valid command."*
    /// and that the window-selection rule (`"all"` -> `0`) is transcribed and tested.
    pending_chat_clears: Vec<u32>,
    /// `ChatPoseTable` — the enum lookup for `(7, 2, 0x11)`, the first
    /// operation the pose command performs and the one whose failure makes
    /// the pose command return `false` before anything else happens.
    ///
    /// Stamped once by `App` from the shipped dats, for the reason [`Self::player_position`] is
    /// stamped: the table is a dat object and this side of the seam has no store.
    pub chat_pose_table: Option<std::sync::Arc<dereth_assets::tables::ChatPoseTable>>,
    /// The motion commands the pose leg's keyboard command to the command
    /// interpreter issued this dispatch, for `App` to feed into
    /// the same `MovementCommands` the emote keys use. A `Vec` because `public_chat`
    /// can extract more than one run from a line.
    pending_pose_motions: Vec<u32>,
    /// Every [`Request`] the last [`use_time`] handed to the wire slot, in order.
    ///
    /// `stats.requests_sent` and `requests_undeliverable` count them and cannot tell
    /// `Communication_Talk` from `Communication_ChannelBroadcast` — and that difference is a line
    /// the player sees go nowhere versus a line every stranger on a global channel reads. A test
    /// that asserts "something went out" is not a test of where it went.
    pub last_sent: Vec<Request>,
    /// The last refusal `dereth_client_model` printed, for the log line.
    pub last_refusal: Option<String>,
    /// The local body's physics position (cell and frame), as the location command reads it.
    ///
    /// `None` while the body pointer is null, in which case
    /// the `@loc` handler prints nothing at all. Stamped by `dispatch_ui_owner_requests` from the
    /// scene's
    /// [`crate::character::Character::position`] immediately before the chat command dispatch,
    /// because the handler runs against `dereth_client_model::World`, which does not own the body.
    pub player_position: Option<dereth_primitives::Position>,
    /// The last frame time [`use_time`] was driven with.
    ///
    /// [`apply_events`] runs *before* [`use_time`] in `App::frame`, so two of its arms — the
    /// enchantment rebase and `handle_attack_done`'s power-bar build
    /// start — would otherwise have no clock at all. This is the previous frame's, which is one
    /// frame stale and **stated rather than hidden**: at any frame rate this client runs at that is
    /// far below the resolution of either value (the wire carries enchantment times in whole
    /// seconds, and the power bar's own build window is `POWER_BAR_BUILD_TIME`). The alternative —
    /// widening `apply_events` to take `now` — is a signature change across 22 call sites,
    /// which is not worth a 16 ms correction.
    pub last_use_time: dereth_primitives::LocalTime,
    pub stats: InteractionStats,
    /// The magic notices [`Self::handle_magic_action`] raised and the UI has not been handed yet.
    /// Interaction holds no `UiSystem`, so it queues them here and its owner moves them into the
    /// UI's own inbox (`UiSystem::notice_inbox`) before the spellcasting panel drains it.
    pub magic_notices: dereth_client_contract::notices::NoticeInbox,
}

/// A [`dereth_client_model::NoticeSink`] that counts and keeps the refusal text.
///
/// It also records the four panel notices. They are recorded here rather than returned
/// because a notice is what a panel will actually act on: counting the
/// enum a function returned proves the function returned it, and counting the notice proves the
/// thing that opens the window was raised.
/// `strings` is the sink's only *output*: every `DisplayString` in arrival order,
/// handed to [`dereth_client_model::scroll::Scroll`] by [`Interaction::absorb`], so no producer's
/// message is computed and dropped. `last` is kept because the log line and existing
/// assertions read it, and because "the most recent refusal" and "every refusal raised this call"
/// are different questions.
#[derive(Debug, Default)]
struct Notices {
    count: u64,
    last: Option<String>,
    /// `(channel, text)`, in arrival order.
    strings: Vec<(u32, String)>,
    /// `(id)` in arrival order; `ObjectId(0)` is a close.
    ground: Vec<ObjectId>,
    external_container: Vec<ExternalContainerNotice>,
    targeted_confirmations: Vec<(
        ObjectId,
        ObjectId,
        dereth_client_model::inventory::targeted_use::TargetedUsageConfirmation,
    )>,
    /// Open-contained-container notices.
    contained: Vec<ObjectId>,
    /// Open-salvage-panel and begin-game notices.
    panels: u64,
    /// The board ids named, in arrival order.
    begin_games: Vec<ObjectId>,
    /// Automatic trade-offer notices, in
    /// arrival order — the item ids handed to the trade panel.
    /// Collected rather than acted on here for `absorb`'s usual reason: the handler
    /// needs the world and the splitter's state.
    trade_for_dummies: Vec<ObjectId>,
    /// Synchronous notices for gameplay panel subscribers.
    /// Object-stream notices already cross this boundary in `App`; UI-queue moves need the same
    /// delivery before the next accepted message.
    moved_for_panels: Vec<Notice>,
    /// Housing range-exit notices preserving the watched owner id.
    slumlord_range_exits: Vec<ObjectId>,
    /// Book range-exit notices preserving the watched book id.
    book_range_exits: Vec<ObjectId>,
    /// The three object-carrying salvage notices in arrival order.
    salvage: Vec<SalvageNotice>,
    /// Usage callback dialogs in the current UI.
    usage_confirmations: Vec<(
        ObjectId,
        dereth_client_model::inventory::use_object::UsageConfirmation,
    )>,
    /// Selection-change notices, which
    /// selection assignment raises synchronously and which
    /// the combat selection-change handler subscribes to.
    ///
    /// Counted here rather than acted on here for the same reason every other structured notice in
    /// this sink is: `absorb` is the one place all of them arrive, and the handler needs the
    /// frame's selection geometry.
    selection_changes: u32,
    /// The two notices that resolve the trade panel's pending stack split,
    /// in the exact order raised by the batch.
    trade_split: Vec<TradeSplitNotice>,
    /// The show-pending and end-pending in-player notices, in arrival order —
    /// placing an item in the backpack raises
    /// them as a pair around one attempt and the order is the whole meaning.
    pending_in_player: Vec<PendingInPlayer>,
}

/// The two inventory pending-row notices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingInPlayer {
    /// Show the item as pending in the player's inventory.
    Show(ObjectId),
    /// End the pending inventory indication.
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TradeSplitNotice {
    ItemAttributesChanged(ObjectId, u32),
    AttemptFailed,
}

impl dereth_client_model::NoticeSink for Notices {
    fn emit(&mut self, n: Notice) {
        self.count += 1;
        match n {
            Notice::DisplayString { channel, text } => {
                self.strings.push((channel, text.clone()));
                self.last = Some(text);
            }
            Notice::SetGroundObject(id) => {
                self.ground.push(id);
                self.external_container
                    .push(ExternalContainerNotice::SetGroundObject(id));
            }
            Notice::CloseSlumlord(id) => self.slumlord_range_exits.push(id),
            Notice::CloseBook(id) => self.book_range_exits.push(id),
            Notice::ItemMoved {
                object,
                old_container,
                old_wielder,
                old_location,
                container,
                place,
                wielder,
                location,
            } => {
                self.external_container
                    .push(ExternalContainerNotice::ItemMoved { object, container });
                self.moved_for_panels.push(Notice::ItemMoved {
                    object,
                    old_container,
                    old_wielder,
                    old_location,
                    container,
                    place,
                    wielder,
                    location,
                });
            }
            Notice::OpenContainedContainer(id) => self.contained.push(id),
            // The counter stays -- the minigame start still has nothing else -- and the
            // salvage half also reaches a panel. The `AddSalvageItem` / `RemoveSalvageItem`
            // arms are here rather than merged into one because the panel's three handlers are
            // three separate notice registrations at the panel's setup.
            Notice::OpenSalvagePanel(tool) => {
                self.panels += 1;
                self.salvage.push(SalvageNotice::Open(tool));
            }
            // Keep the producer counter as evidence that object-use arm 7 ran,
            // and also deliver the board id to the chess panel, matching the original
            // begin-game notice handling.
            Notice::BeginGame(board) => {
                self.panels += 1;
                self.begin_games.push(board);
            }
            // The receiver for the automatic trade-offer notice, which has two producers
            // (trade registration's deferred arm and the attempt to trade an item).
            Notice::TradeAnItemForDummies(item) => self.trade_for_dummies.push(item),
            Notice::ItemAttributesChanged { object, kind } => {
                self.trade_split
                    .push(TradeSplitNotice::ItemAttributesChanged(object, kind));
            }
            Notice::AttemptFailed { .. } => self.trade_split.push(TradeSplitNotice::AttemptFailed),
            Notice::UsageConfirmation { object, kind } => {
                self.usage_confirmations.push((object, kind));
            }
            Notice::TargetedUsageConfirmation {
                source,
                target,
                kind,
            } => self.targeted_confirmations.push((source, target, kind)),
            // Selection assignment raises this only when the id actually
            // moved, matching the native early return — so a re-select of the same object with
            // `force = 0` raises nothing and reaches no handler, in this build and in retail.
            Notice::SelectionChanged { .. } => self.selection_changes += 1,
            // Inventory pending-row notices. Backpack placement shows the
            // pending item before attempting placement, then retracts it if the attempt
            // is refused. Without a UI receiver the pack would show nothing until the
            // authoritative `0x0022` arrived.
            // Collect the ordered pair here; `absorb` applies it with access to the world.
            Notice::ShowPendingInPlayer(item) => {
                self.pending_in_player.push(PendingInPlayer::Show(item));
            }
            Notice::EndPendingInPlayer => {
                self.pending_in_player.push(PendingInPlayer::End);
            }

            // =================================================================================
            // **One explicit arm per variant, and no wildcard.**
            //
            // A trailing `_ => {}` would make everything below read as "unmatched" to any audit
            // of which notices are handled. `dereth_physics::PhysicsNotice`'s consumer in
            // `character.rs` is the shape this follows and says why in as many words:
            // **an explicit empty arm is evidence; a wildcard is not.**
            //
            // Every arm below is deliberately empty, and each one names the poll that already
            // carries the same state change. **Filling one of these in without deleting the poll it
            // names would deliver the change twice.**
            //
            // The other half of the point is the compiler: with the wildcard gone, a new `Notice`
            // variant fails `E0004` here rather than being silently dropped, which is how
            // `core/client-model/tests/corpus_replay.rs` catches the same class.
            // =================================================================================

            // The physics create-object notice. The renderer is driven by the *queue*, not
            // by this: `ObjectStream` pushes the id onto `created` and `WorldScene::sync_objects`
            // drains it with `take_created()` (`crate::world`, the `for id in stream.take_created()`
            // loop), which is where the setup, the part array, the appearance and the physics body
            // are installed. `take_created` empties the queue, so one create is one geometry build;
            // a second builder here would cost a second shader-visible descriptor slot, which is
            // the descriptor-heap pressure `release_unlinked_appearances` exists to relieve.
            Notice::ObjectCreated(_) => {}
            // Already applied before it ever reaches this sink: `ObjectStream::
            // apply_notice_projection` (`crate::objects`) unparents the children, drops the
            // presence and the position entry, retires the physics body and pushes the id onto
            // `removed` — and only then queues the notice for the UI. Acting on it again here
            // would be a second teardown of an id the model has already forgotten.
            Notice::ObjectDeleted(_) => {}
            // Stat-update and stat-removal callbacks. `dereth_client_model` raises these
            // **only** for `QualityScope::Object(_)` (`core/client-model/src/world.rs:2336` and `:2563`);
            // the player's own qualities take the other branch and are answered directly in the
            // quality-message arm of `Hud`, which re-runs the skills/spellbook join there. An
            // object's stat is mirrored into its `PublicWeenieDesc` by
            // `dereth_client_model::weenie::mirror_stat_update`, and the item decoration `GameView` hands the
            // panels is rebuilt from that row on every frame — so the tile redraws without this.
            Notice::StatUpdated(..) | Notice::StatRemoved(..) => {}
            // Inventory changes need no subscriber on the client's stop-viewing-contents path.
            // `GamePlayScreen::update_inventory`
            // → `InventoryPanels::update`,
            // driven from the world once per frame and guarded on the panel's own snapshot.
            // `GameView::container_contents` *is* `world.inventory(id).items`,
            // which is the state the producer wrote. The one production producer — `Hud`'s `0x0013
            // Login_PlayerDescription` arm — already passes `&mut dereth_client_model::NullSink`, so this
            // notice does not even arrive from there.
            Notice::InventoryChanged(_) => {}
            // `0x00C9 Item_SetAppraiseInfo` filled the appraisal state; `HudView::appraisal` reads
            // it back and `GamePlayScreen::update_examination` runs once per frame, **after**
            // `interaction::apply_events` in `App::frame`, so the panel opens on the frame the
            // reply lands rather than the frame after.
            Notice::AppraisalReady(_) => {}
            // Same bridge, same frame:
            // `HudView::open_book` reads the open-book state and `GamePlayScreen::update_book` is driven
            // once per frame.
            Notice::OpenBook(_) => {}
            // The character player-object-description-changed notice. **Never produced.** Its only
            // constructor, the player's visual-description setter,
            // has no caller anywhere in the workspace. The appearance change it names
            // arrives as `0xF625 Item_ObjDescEvent`, whose object-stream handler
            // re-offers the id on the same `created` queue a create uses, which is where
            // `WorldScene::apply_player_objdesc` runs.
            Notice::PlayerObjDescChanged => {}
            // **The three vendor notices** are covered by polling.
            // `RemainingPanels::update` reads `HudView::shop`, via `vendor_view::shop(world)`,
            // once per frame. Those notices announce changes already represented in that
            // state; pushing them as well would open the window twice.
            Notice::OpenVendor { .. } | Notice::CloseVendor | Notice::AddItemToSell(_) => {}
            // **The trade notices** are also represented by the polled state, except
            // for the item-trade action above and the item-attributes-changed/attempt-failed
            // pair that `absorb` folds for the pending split.
            // `RemainingPanels::update` reads `HudView::trade`, via `trade_view::trade(world)`,
            // once per frame; the remaining notices need no duplicate UI delivery.
            Notice::TradeRegistered { .. }
            | Notice::OpenSecureTrade { .. }
            | Notice::CloseSecureTrade { .. }
            | Notice::TradeItemAdded { .. }
            | Notice::TradeItemRemoved { .. }
            | Notice::TradeAccepted { .. }
            | Notice::TradeDeclined { .. }
            | Notice::TradeReset { .. }
            | Notice::TradeFailure { .. }
            | Notice::TradeAcceptanceCleared => {}
            // **Unreachable in this sink, and that is the finding.** `dereth_client_model` never emits
            // either one: both are constructed in *this file*'s enchantment arms and handed
            // straight to the `panels(..)` callback, where `Hud` answers `EnchantmentsChanged` by
            // re-running `rebuild_panel_tables` (the stat panel's update traversal)
            // and deliberately does **not** answer `VitaeChanged` — the stat panel has four
            // notice registrations, and vitae changes are not among them.
            // The arms are written rather than left to a catch-all
            // so that "this sink cannot see them" is recorded rather than inferred.
            Notice::EnchantmentsChanged | Notice::VitaeChanged => {}
        }
    }
}

impl Interaction {
    /// Drop the callback-dialog requests no UI will ever show: the queues a dialog subscriber
    /// drains before it knows whether there is a screen to show them on. A question nobody can see
    /// is never answered, and a UI that comes up later must not show an old one.
    pub fn discard_unshown_dialogs(&mut self) {
        let _ = self.take_usage_confirmations();
        let _ = self.take_targeted_confirmations();
        let _ = self.take_vendor_close_confirmations();
        let _ = self.take_house_payment_confirmations();
        let _ = self.take_server_confirmations();
        let _ = self.take_confirmation_aborts();
        let _ = self.take_fellowship_requests();
    }

    pub fn take_usage_confirmations(
        &mut self,
    ) -> Vec<(
        ObjectId,
        dereth_client_model::inventory::use_object::UsageConfirmation,
    )> {
        std::mem::take(&mut self.pending_usage_confirmations)
    }

    pub fn take_targeted_confirmations(
        &mut self,
    ) -> Vec<(
        ObjectId,
        ObjectId,
        dereth_client_model::inventory::targeted_use::TargetedUsageConfirmation,
    )> {
        std::mem::take(&mut self.pending_targeted_confirmations)
    }

    /// The queue `crate::target_confirmation::TargetedDialogs` turns into
    /// current-UI dialogs with the close-vendor callback.
    pub fn take_vendor_close_confirmations(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.pending_vendor_close_confirmations)
    }

    /// Buy-house and rent-by-proxy confirmation dialogs.
    pub fn take_house_payment_confirmations(&mut self) -> Vec<bool> {
        std::mem::take(&mut self.pending_house_payment_confirmations)
    }

    /// The close-vendor dialog's **Yes** arm, and the whole of it.
    ///
    /// It returns early when property `0x92` is absent, clears the dialog context, and on a Yes
    /// raises the close-vendor notice with `false`.
    ///
    /// The close-vendor notice with `false`
    /// closes the vendor without completing the trade — the same
    /// function the empty-basket arm calls. **A No is
    /// nothing at all**: the baskets keep their contents and the window stays up.
    pub fn confirm_vendor_close(&mut self, game: &mut dereth_client_model::World) {
        let mut out = Notices::default();
        let req = RecordingRequests::default();
        if game.close_vendor(&mut out) {
            self.stats.vendor_closes += 1;
        }
        self.absorb(game, out, req);
    }

    /// Drain the five gameplay confirmation types carried by this
    /// frame's `0x0274` messages for `crate::target_confirmation::TargetedDialogs`.
    pub fn take_server_confirmations(&mut self) -> Vec<(i32, u32, String)> {
        std::mem::take(&mut self.pending_server_confirmations)
    }

    /// This frame's `0x0276`s — the server taking a question back.
    pub fn take_confirmation_aborts(&mut self) -> Vec<(i32, u32)> {
        std::mem::take(&mut self.pending_confirmation_aborts)
    }

    /// This frame's type-1 `0x0274`s — somebody swearing to the player.
    pub fn take_swear_requests(&mut self) -> Vec<(u32, String)> {
        std::mem::take(&mut self.pending_swear_requests)
    }

    /// This frame's `0x0004 Communication_PopUpString` texts, for
    /// `crate::target_confirmation::TargetedDialogs` to put on screen as message dialogs.
    pub fn take_pop_up_strings(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending_pop_up_strings)
    }

    /// How many `0x0004`s are waiting for a UI to show them.
    ///
    /// The queue is drained only once the shell is up and in `GAME_PLAY`, so a non-zero value here
    /// is "received but not yet shown" and not "lost" — which is the distinction a test needs and
    /// a counter on its own cannot make.
    #[must_use]
    pub fn pop_up_strings_pending(&self) -> usize {
        self.pending_pop_up_strings.len()
    }

    /// `@die`'s question, for
    /// `crate::target_confirmation::TargetedDialogs` to put on screen.
    pub fn take_die_confirmations(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.pending_die_confirmations)
    }

    /// The die dialog's Yes arm — suicide event `0x0279`.
    ///
    /// **A No is nothing at all**: the native refusal branch skips the call, exactly as the three
    /// allegiance dialogs do, and unlike the gameplay confirmation close, which
    /// answers the server either way.
    pub fn confirm_die(&mut self, game: &mut dereth_client_model::World) {
        let mut req = RecordingRequests::default();
        game.confirm_die(&mut req);
        self.stats.chat_command_requests += 1;
        let out = Notices::default();
        self.absorb(game, out, req);
    }

    /// `@house abandon`'s two prompts, for
    /// `crate::target_confirmation::TargetedDialogs` to put on screen.
    pub fn take_house_abandon_first(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.pending_house_abandon_first)
    }

    pub fn take_house_abandon_second(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.pending_house_abandon_second)
    }

    /// The first house-abandon callback's Yes arm.
    ///
    /// Its whole body, once the `0x92` guard passes, is a second
    /// callback dialog carrying the same two properties — `0x8E` = enum 1 and
    /// `0xC5` = the prompt — and the second house-abandon callback as the callback. **It sends
    /// nothing**: the first Yes only asks again.
    pub fn confirm_house_abandon_first(&mut self) {
        self.pending_house_abandon_second
            .push(dereth_client_model::chat_cmd::HOUSE_ABANDON_SECOND);
        self.stats.house_abandon_second_raised += 1;
    }

    /// The second house-abandon confirmation's Yes arm sends the abandon-house event,
    /// `0x021F`, the only send on this path and the only sender of that event in retail.
    pub fn confirm_house_abandon(&mut self, game: &mut dereth_client_model::World) {
        let mut req = RecordingRequests::default();
        game.confirm_house_abandon(&mut req);
        self.stats.chat_command_requests += 1;
        let out = Notices::default();
        self.absorb(game, out, req);
    }

    /// The `@clear` windows nothing consumes yet — see
    /// `Interaction::pending_chat_clears`. Exposed so a test can prove the arm reached the queue
    /// rather than inferring it from a counter alone.
    #[must_use]
    pub fn chat_clears_pending(&self) -> &[u32] {
        &self.pending_chat_clears
    }

    pub fn take_allegiance_confirmations(&mut self) -> Vec<(AllegianceAction, ObjectId, String)> {
        std::mem::take(&mut self.pending_allegiance_confirmations)
    }

    /// Swear, break and kick confirmation callbacks — the **Yes** arms, which are the only arms
    /// that send.
    ///
    /// Break re-reads the patron from the player's profile at close time and sends a break request
    /// for that id. Kick sends a break request for the retained vassal id. Swear sends a swear
    /// request for the retained proposed-patron id.
    ///
    /// **A No is nothing at all** for all three: there is no confirmation response here,
    /// because these dialogs are the *client's* own question and not one of the server's seven
    /// `0x0274` types. (The one allegiance dialog that does answer the server is
    /// the accept-swear confirmation close, confirmation type 1, which is a different
    /// path and not this one.)
    ///
    /// Break re-reads the patron here rather than using the id the question was asked about,
    /// which is retail's own asymmetry with Kick: the kick confirmation latches the would-be
    /// kicked vassal's id when it opens, while the break confirmation looks the patron up when it
    /// closes.
    pub fn confirm_allegiance(
        &mut self,
        game: &mut dereth_client_model::World,
        action: AllegianceAction,
        target: ObjectId,
    ) {
        let out = Notices::default();
        let mut req = RecordingRequests::default();
        match action {
            AllegianceAction::Swear => {
                game.swear_allegiance(&mut req, target);
                self.stats.allegiance_swears += 1;
            }
            AllegianceAction::Break => {
                if game.break_allegiance_from_patron(&mut req).is_none() {
                    // The patron went away while the question was on screen. Retail sends
                    // a break-allegiance request for id 0 here, because the patron lookup leaves
                    // its out parameter at zero and the call is unconditional; this build declines
                    // to put an object id of 0 on the wire and counts the miss instead.
                    self.stats.allegiance_breaks_without_patron += 1;
                    return;
                }
                self.stats.allegiance_breaks += 1;
            }
            AllegianceAction::Kick => {
                game.break_allegiance(&mut req, target);
                self.stats.allegiance_kicks += 1;
            }
        }
        self.absorb(game, out, req);
    }
    /// This frame's `0x0274` type-4 invitations.
    pub fn take_fellowship_requests(&mut self) -> Vec<(u32, String)> {
        std::mem::take(&mut self.pending_fellowship_requests)
    }

    /// Close a server-confirmation dialog. The native callback reads the answer byte from
    /// property `0x92`, sends it with the retained server context and confirmation type, then
    /// clears the local dialog context, confirmation type and server context.
    ///
    /// There is **no branch on the answer**: a No is a `0x0275` with `accepted = 0`, which is what
    /// lets ACE's `ConfirmationManager.HandleResponse(…, false)` act on a refusal at once instead
    /// of sitting out the thirty seconds. Retail passes property `0x92` as the answer.
    pub fn confirm_server_confirmation(
        &mut self,
        confirmation_type: i32,
        context_id: u32,
        accepted: bool,
    ) {
        self.outbox.push(Request::ConfirmationResponse(
            dereth_protocol::comms::CharacterConfirmationResponse {
                confirmation_type,
                context_id,
                // The native callback widens the answer byte: the C `bool` reaches the wire as 1 or 0.
                accepted: i32::from(accepted),
            },
        ));
        self.stats.confirmations_answered += 1;
    }

    /// Accepted targeted-use callback. Identity comes from the dialog collection, never selection.
    pub fn confirm_targeted_usage(
        &mut self,
        game: &mut dereth_client_model::World,
        source: ObjectId,
        target: ObjectId,
        now: ServerTime,
    ) {
        let mut out = Notices::default();
        let mut req = RecordingRequests::default();
        game.confirm_targeted_usage(&mut req, &mut out, source, target, self.split, now);
        self.absorb(game, out, req);
    }

    /// Accept a usage-confirmation dialog. The object is the instance id
    /// retained on the dialog, not the current selection.
    pub fn confirm_usage(
        &mut self,
        game: &mut dereth_client_model::World,
        object: ObjectId,
        now: ServerTime,
    ) {
        let mut out = Notices::default();
        let mut req = RecordingRequests::default();
        game.confirm_usage(&mut req, &mut out, object, self.split, now);
        self.absorb(game, out, req);
    }
    /// Register local chat-system commands when the `0xF658` startup flag is true.
    pub fn startup_turbine_chat_commands(&mut self) {
        self.chat.add_turbine_chat_commands();
    }
    /// Drain this frame's external-container notices, preserving producer order.
    pub fn take_external_container_notices(&mut self) -> Vec<ExternalContainerNotice> {
        std::mem::take(&mut self.pending_external_container)
    }

    /// Drain this frame's salvage notices in producer order.
    pub fn take_salvage_notices(&mut self) -> Vec<SalvageNotice> {
        std::mem::take(&mut self.pending_salvage)
    }

    /// Drain this frame's housing range exits at the nine-unit threshold.
    pub fn take_slumlord_range_exits(&mut self) -> Vec<ObjectId> {
        std::mem::take(&mut self.pending_slumlord_range_exits)
    }

    /// Drain this frame's unowned-book use-radius exits.
    pub fn take_book_range_exits(&mut self) -> Vec<ObjectId> {
        std::mem::take(&mut self.pending_book_range_exits)
    }

    /// The ids accepted this
    /// frame, for delivery into the trade panel; see [`Self::pending_trade_for_dummies`].
    pub fn take_trade_for_dummies(&mut self) -> Vec<ObjectId> {
        std::mem::take(&mut self.pending_trade_for_dummies)
    }

    #[must_use]
    pub fn new() -> Self {
        Self {
            pick: WorldPicker::new(),
            ..Self::default()
        }
    }

    /// End the character's UI session.
    ///
    /// Retail unregisters the input-action callback and cleans up the gameplay UI first. It then
    /// clears the leave-target-mode flag, target mode, radar-blank flag, current and requested
    /// ground objects, vendor id, pending vendor-open and sale ids, and busy count. It sets radar
    /// visibility to true, invalidate the current cursor id, and finally update the cursor.
    ///
    /// The true radar-visibility reset is why [`StartsTrue`] exists, and it is the one field a
    /// `*self = Self::default()` has to be trusted to get right — which `derive(Default)` does,
    /// through that type.
    ///
    /// # What is deliberately kept
    ///
    /// * **[`Interaction::chat`]** — the command table and typed-line history belong
    ///   to process startup, not the character's gameplay UI. Rebuilding them here
    ///   would lose the Turbine-chat commands added by `App::process_logon_event_queue`
    ///   on `0xF658`.
    /// * **[`Interaction::stats`]** — this run's log line, exactly as
    ///   [`crate::objects::ObjectStream::reset`] keeps its counters for the same reason.
    /// * **[`Interaction::cursor`], [`Interaction::screen`], [`Interaction::game_viewport`]** —
    ///   shared input/render-device state pushed from outside each frame. It belongs
    ///   to the device, not the character; clearing it would put the cursor at `(0, 0)`
    ///   for one frame and measure picking against an empty rectangle.
    /// * **[`Interaction::last_use_time`]** — the local timer, a process clock.
    /// * **[`Interaction::chat_pose_table`]** — a process-lifetime cached dat table.
    ///   The original pose path re-fetches enum
    ///   `(7, 2, 0x11)` on every call and fails when absent; session cleanup does not
    ///   remove the startup cache entry. Here `App::start_shell` loads it only once,
    ///   so dropping it on `SessionEvent::WorldReset` would lose it permanently, even on
    ///   the first login's entry edge. Pose lookup would then fail and chat would send the
    ///   asterisk-delimited text literally.
    ///
    /// # What is NOT this function
    ///
    /// Gameplay UI teardown is separate. The shell's mode transition removes the
    /// `dereth_ui_screens` gameplay tree at character selection; this function neither
    /// owns nor changes that tree.
    pub fn on_end_character_session(&mut self) {
        let chat = std::mem::take(&mut self.chat);
        let stats = self.stats;
        let cursor = self.cursor;
        let screen = self.screen;
        let game_viewport = self.game_viewport;
        let last_use_time = self.last_use_time;
        // Keep the process-cached `(7, 2, 0x11)` result across this
        // session reset; see the lifetime list above.
        let chat_pose_table = self.chat_pose_table.take();
        *self = Self {
            chat,
            stats,
            cursor,
            screen,
            game_viewport,
            last_use_time,
            chat_pose_table,
            ..Self::new()
        };
    }

    /// What step 7 produced: the pointer events dispatched
    /// and the requests the screens queued. Both are acted on in [`use_time`], which is where the
    /// client's per-frame interaction update sits.
    pub fn queue(&mut self, mouse: Vec<UiMouseEvent>, requests: Vec<UiRequest>) {
        self.mouse.extend(mouse);
        self.ui_requests.extend(requests);
    }

    /// The render-device viewport rectangle — the origin and extent
    /// that object picking subtracts and compares against.
    ///
    /// Pushed in by `App::ui_use_time` / `App::interaction_use_time` from `<SBOX>`'s own screen
    /// box, the same value `Renderer::set_game_viewport` installs, so the rectangle the pick
    /// measures against and the rectangle the scene is drawn into cannot disagree. `None` hands
    /// the whole back buffer back, which is what every headless harness and every pre-gameplay
    /// phase has.
    pub fn note_game_viewport(&mut self, viewport: Option<Viewport>) {
        self.game_viewport = viewport;
    }

    /// Whether the pointer is over the 3-D view rather than over a HUD window
    /// drawn on top of it — the game-viewport calculation's subtraction, which this
    /// build's `<SBOX>`-box viewport does not carry. Pushed in by
    /// `crate::app::App::pointer_over_game_view`, which is where the element tree is, once a
    /// frame, immediately before the step that consumes it — the same seam
    /// [`Self::note_examine_panel_open`] uses and for the same reason.
    ///
    /// Defaults to **true**, which is "there is no HUD over the viewport": a `--no-ui` run and
    /// every headless harness with no shell, where the render device's viewport is the whole back
    /// buffer and `compute_game_viewport`'s own answer with nothing docked is the same rectangle.
    pub fn note_pointer_over_game_view(&mut self, over: bool) {
        self.pointer_over_game_view = StartsTrue(over);
    }

    /// The live input-manager pointer position, as read by the object-found notice.
    #[must_use]
    pub fn cursor(&self) -> (i32, i32) {
        self.cursor
    }

    /// The rectangle [`Self::game_viewport`] resolves to at a given back-buffer extent.
    ///
    /// Not a plain getter: `None` is the client's "the viewport is the render target", which is
    /// the state the render device is left in before anything docks, and
    /// resolving it here is what keeps `find_object`'s two `sub`s the identity in a headless run.
    fn device_viewport(&self, screen: (u32, u32)) -> Viewport {
        self.game_viewport.unwrap_or(Viewport {
            x: 0,
            y: 0,
            width: screen.0,
            height: screen.1,
        })
    }

    /// App's synchronous mouse callback. Keep the source's pick arm before the screen's
    /// request, but leave geometric completion at draw_no_blit. Do not queue it a second time.
    ///
    /// `screen` is the back buffer; the 3D viewport's rectangle comes from
    /// [`Self::note_game_viewport`].
    pub fn dispatch_ui_mouse(&mut self, event: UiMouseEvent, screen: (u32, u32)) {
        self.cursor = (event.x, event.y);
        self.wrapper_mouse(event, screen, is_world_click(event.over));
    }

    /// Hover search from the global loop or mouse movement. A click/drop reason wins;
    /// hover never selects or uses an object. Object search has a synchronous UI-item arm.
    /// Retail returns without searching when the reason is at least 1; otherwise
    /// it stores `SearchReason::MouseOver` (1) **before** calling object search.
    ///
    /// **Why a refused search must undo the store** — otherwise the tooltip only starts after
    /// the viewport has been clicked or an item selected. `search_reason` is put back to
    /// `SearchReason::None` in exactly one
    /// place in the whole client, the unconditional tail of the object-found notice — so the store
    /// is only safe while *every* search reaches a notice.
    /// Object picking has one leg that does not: an out-of-bounds unsigned viewport comparison
    /// clears the selection cursor and returns false without arming a search.
    /// With `looking_for_object` left clear, drawing skips the notice and
    /// nothing ever reopens the hover gate. Every later frame's hover turns straight
    /// around, and the only gestures that still reach the notice are two:
    /// a world click (the mouse-down handler gates on `< SearchReason::Examine`, which 1 passes)
    /// and a hover over a UI item, which object search answers synchronously through
    /// the found-object setter.
    ///
    /// **Why this build reaches a leg retail does not.** The input manager's pointer is window-relative and
    /// stored unclamped — the native input-message handler sign-extends two **signed** shorts —
    /// while the rectangle `find_object` measures against is the render device's viewport, which
    /// in this build is `<SBOX>`'s own box. Windows delivers a `WM_MOUSEMOVE` outside the
    /// client area for the whole of any capture, i.e. every drag, and a docked smart box makes the
    /// same coordinate reachable with no capture at all. So the store-then-refuse pair is a live
    /// path here where in retail's default full-window layout it is not.
    ///
    /// The repair is the invariant and not the store: `search_reason` names a search **in
    /// flight**, `find_object`'s `false` means no search was started, and a gesture that never
    /// began must not hold the gate closed. The store stays exactly where the native path puts it —
    /// ahead of the call, so a reentrant search inside it still sees `SearchReason::MouseOver` —
    /// and is undone only on the leg that armed nothing.
    pub fn dispatch_ui_hover(
        &mut self,
        position: (i32, i32),
        item: Option<ObjectId>,
        screen: (u32, u32),
        game: &mut dereth_client_model::World,
        now: ServerTime,
    ) {
        if self.reason >= SearchReason::MouseOver {
            return;
        }
        // The native mouse-coordinate queries read the pointer **live**; writing this field only
        // in [`Self::dispatch_ui_mouse`], i.e. only when a mouse *button* moved, would make the
        // notice's viewport test measure the last click's position and not the pointer's.
        self.cursor = position;
        let before = self.reason;
        self.reason = SearchReason::MouseOver;
        if let Some(item) = item {
            let found = self.pick.set_found_object(item, -1);
            self.on_world_object_found(found, game, now);
        } else {
            self.stats.picks_requested += 1;
            let rect = self.device_viewport(screen);
            // **A pointer over a HUD window.** Object picking measures
            // against the render device's viewport, which
            // has already shrunk by every docked HUD window; a pointer over one of those is
            // outside it and the search is refused. This build's viewport is `<SBOX>`'s raw box
            // and the shipped layout arms no clamp edge, so the rectangle admits the whole window
            // — see [`Self::pointer_over_game_view`]. The refusal is spelled the same way the
            // rectangle's own refusal is, one branch below, so a HUD-covered pointer produces no
            // pick, no notice and therefore no 3-D tooltip, and leaves no gesture in flight.
            if !self.pointer_over_game_view.0 {
                // The refusal is `find_object`'s, so it has to clear what
                // `find_object` clears: it zeroes `click_object_id` and
                // `click_object_index` **before** the two unsigned viewport checks, and clears
                // the selection cursor on refusal. Returning here without them would leave the last
                // hovered UI item's id standing as the WorldObjects's found object for as long as
                // the pointer was over a HUD window. See [`WorldPicker::refuse_find_object`].
                self.pick.refuse_find_object();
                self.reason = before;
                self.stats.hover_searches_under_the_hud += 1;
                return;
            }
            if !self.pick.find_object(position.0, position.1, rect) {
                // A rejected pick never arms `looking_for_object`, so no draw-time notice arrives
                // to clear `search_reason` again.
                self.reason = before;
                self.stats.hover_searches_not_armed += 1;
            }
        }
    }

    /// Live motion facts at an input callback, before Cast/Combat request predicates read them.
    /// Same authoritative body/fields as the later `use_time` bridge; no motion is advanced here.
    pub fn prepare_ui_dispatch(
        &mut self,
        body: Option<&crate::character::Character>,
        game: &mut dereth_client_model::World,
        screen: (u32, u32),
    ) {
        self.screen = screen;
        self.note_player_physics(body);
        if let Some(body) = body {
            let driver = body.driver();
            let state = &driver.movement.interp.interpreted_state;
            let style = state.current_style.0;
            if self.combat_style_bridged != Some(style) {
                self.combat_style_bridged = Some(style);
                game.combat.current_style = style;
                self.stats.combat_style_bridges += 1;
            }
            let forward = state.forward_command.0;
            if self.combat_forward_command_bridged != Some(forward) {
                self.combat_forward_command_bridged = Some(forward);
                game.combat.forward_command = forward;
                self.stats.combat_style_bridges += 1;
            }
        }
    }

    pub fn dispatch_ui_selection_notices(
        &mut self,
        body: Option<&crate::character::Character>,
        objects: &mut crate::objects::ObjectStream,
        now: dereth_primitives::LocalTime,
    ) {
        let origin = body.map(crate::character::Character::position);
        let phys = crate::selection_geometry::SceneSelectionPhysics::new(origin.as_ref(), objects);
        let radius = dereth_client_contract::radar::radar_range(
            origin
                .as_ref()
                .is_some_and(|p| dereth_physics::landdefs::is_outdoors(p.cell)),
        );
        self.run_defender_notifications(&mut objects.world, &phys, radius, now);
        // `auto_target` can raise a second selection notice; retail delivers that reentrantly.
        while self.pending_selection_changes != 0 {
            self.run_selection_change_notices(&mut objects.world, &phys, radius, now);
        }
    }

    /// Tell this frame's `0x1000002B` arm whether `<EXAM>` is on screen, which is
    /// the question the examination action asks the UI element manager.
    ///
    /// Pushed by `App` immediately before [`use_time`], so the answer is this frame's UI state
    /// rather than the previous frame's. A caller that never calls this leaves the arm on its
    /// `false` default, which is retail's *"there is no such element"* leg — see
    /// [`Interaction::examine_panel_open`]'s own note about why that is asserted and not assumed.
    pub fn note_examine_panel_open(&mut self, open: bool) {
        self.examine_panel_open = open;
    }

    /// Whether the `0x1000002B` arm took its close-first leg this frame, and clear
    /// the flag.
    ///
    /// `App` drains this immediately after [`use_time`] returns and hides the
    /// panel.
    pub fn take_examine_panel_close(&mut self) -> bool {
        std::mem::take(&mut self.examine_panel_close)
    }

    /// **The producer for `place_in_3d`'s `player_on_ground`.**
    ///
    /// The player's physics object is queried for ground contact,
    /// which is `transient_state & 1` **and**
    /// `transient_state & 2` — `CONTACT` and `ON_WALKABLE`, both. Retail
    /// tests contact first, then walkability, returning true only when both bits are set.
    ///
    /// `body` is the local player's physics body; `None` corresponds to an absent body.
    /// [`use_time`] supplies `WorldScene::character`, and
    /// [`crate::character::Character::on_ground`] checks the same two bits.
    ///
    /// **Note this is not the command interpreter's logout gate**: that one is
    /// `transient_state & 1` **alone**, so a body in contact with a *non*-walkable
    /// surface may log out and may not drop an item. Two different predicates over one word.
    /// **This once-per-frame lookup also carries pending-motion state.**
    /// Ground contact and combat readiness are different questions, with separate
    /// answers and counters so a mutation to either remains observable. Both
    /// readers use the combat-ready predicate, which never reads
    /// `transient_state`.
    ///
    /// The original readiness decision is:
    ///
    /// * An absent physics body returns false.
    /// * Noncombat and magic return `!motions_pending`.
    /// * Melee first requires a combat maneuver table. With the attack argument
    ///   true, that is sufficient; with false, it also requires `!motions_pending`.
    /// * Missile first requires one of six styles and forward command Ready. With
    ///   the attack argument true, that is sufficient; with false, it also requires
    ///   `!motions_pending`.
    ///
    /// `!motions_pending` alone exactly answers the noncombat/magic arms, but is
    /// merely an upper bound for mode changes in melee or missile: missing weapon
    /// prerequisites can still require queueing the change. It is not a valid attack
    /// answer, because a qualifying melee/missile attack bypasses pending-motion checks;
    /// answering attack-build, execute and power-bar entry from it refuses melee attacks
    /// retail allows.
    ///
    /// [`Self::ready_for_mode_change`] and [`Self::ready_for_attack`] supply the full
    /// decisions. The former serves immediate mode toggling and
    /// pending-mode retry; they both use the false argument.
    ///
    /// The original pending-motion query checks for a movement manager, then asks
    /// its motion interpreter whether the pending list has a head. `Character`
    /// always owns a `MotionDriver`, so this build cannot represent an absent
    /// manager inside an existing body; `movement.motions_pending()` supplies the
    /// answer.
    pub fn note_player_physics(&mut self, body: Option<&crate::character::Character>) {
        let answered = body.map(crate::character::Character::on_ground);
        if answered != self.player_on_ground {
            self.stats.physics_answers_changed += 1;
        }
        self.player_on_ground = answered;

        self.player_motions_pending = body.map(|c| c.driver().movement.motions_pending());
        let ready = self.player_motions_pending == Some(false);
        if ready != self.player_ready {
            self.stats.ready_answers_changed += 1;
        }
        self.player_ready = ready;
    }

    /// Pending-motion state on the local body, `None` when there is no body.
    /// This is the argument the interaction callback takes.
    #[must_use]
    pub fn player_motions_pending(&self) -> Option<bool> {
        self.player_motions_pending
    }

    /// Combat readiness with the false argument, as passed by both mode-change call sites.
    ///
    /// `!motions_pending` alone is the whole of
    /// the non-combat and magic arms and only an **upper bound** on the other two: melee also needs
    /// a combat maneuver table and missile also needs one of the six stances and a `Ready`
    /// `forward_command`, and both of those can only subtract. This is the whole switch.
    #[must_use]
    pub fn ready_for_mode_change(&self, game: &dereth_client_model::World) -> bool {
        game.player_in_ready_position(false, self.player_motions_pending)
    }

    /// Combat readiness with the true argument, as passed by all three attack call sites.
    ///
    /// With the argument set, the melee and missile arms answer
    /// true as soon as the weapon precondition holds, **without** reaching
    /// `motions_pending` -- so this is `true` in states where [`Self::ready_for_mode_change`] is
    /// `false`, and answering the attack sites from the mode-change value refuses swings retail
    /// allows.
    ///
    /// # Declared deviation, and it is the null-physics-object arm alone
    ///
    /// The missing-body rejection is **not** composed here: `motions_pending` is passed as
    /// `Some(false)` rather than `None` when the frame has no body. Everything the readiness
    /// switch decides -- the mode, the combat maneuver table, the six stances and
    /// `forward_command` -- is answered.
    ///
    /// The reason is a seam: [`use_time`] reads the body out of an
    /// `Option<&crate::world::WorldScene>` and `WorldScene::load` needs a `Gpu`, so **every
    /// headless frame in this workspace has no body**. Composing the null arm answers `false` in
    /// all of them and refuses every attack in the headless combat tests -- which is
    /// retail's answer for a client with no physics object, and is not the state those benches
    /// are about. The mode-change flavour above **does** compose it, because its consumers queue
    /// rather than refuse and that arm is asserted end to end.
    ///
    /// This is expected to change when a frame slot can take the body independently of the
    /// render scene (`app.rs` / `world.rs`); at that point the
    /// `.or(Some(false))` below becomes `self.player_motions_pending` and the benches gain a body.
    #[must_use]
    pub fn ready_for_attack(&self, game: &dereth_client_model::World) -> bool {
        game.player_in_ready_position(true, self.player_motions_pending.or(Some(false)))
    }

    /// What [`Self::note_player_physics`] last read for
    /// the pending-motion portion of combat readiness.
    #[must_use]
    pub fn player_ready(&self) -> bool {
        self.player_ready
    }

    /// What [`Self::note_player_physics`] last read, for the tests: `Some(false)` is an airborne
    /// body and `None` is no body at all, which `place_in_3d` treats alike and the client does too.
    #[must_use]
    pub fn player_on_ground(&self) -> Option<bool> {
        self.player_on_ground
    }

    /// The [`Request`]s this frame has queued and [`use_time`]'s step 4 has not yet handed to the
    /// session.
    ///
    /// A test that drives step 2 apart from step 3 — which is the only way to
    /// observe an armed pick, since the client answers it in the same frame — cannot read the wire,
    /// because nothing has reached it yet. This is the stronger question anyway: *"was a request
    /// even built?"* rather than *"did one reach the socket?"*.
    #[must_use]
    pub fn outbox(&self) -> &[Request] {
        &self.outbox
    }

    /// The [`Request`]s produced but not yet handed to the wire — what `run`'s step 4 drains.
    ///
    /// The outbox is where "the arm ran" and "the arm sent the right thing" are
    /// distinguishable.
    #[must_use]
    pub fn pending_requests(&self) -> &[Request] {
        &self.outbox
    }

    /// The same, drained — what [`use_time`]'s step 4 does to it.
    ///
    /// A test that drives several frames needs "what did *this* frame send", and comparing
    /// lengths across frames answers a different question badly: a frame that sent one message
    /// and a frame that sent one after another sent one look identical in a suffix.
    pub fn take_pending_requests(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.outbox)
    }

    /// Current target mode, for the tests and the cursor.
    #[must_use]
    pub fn target_mode(&self) -> TargetMode {
        self.target_mode
    }

    /// Target-mode assignment changes the leave flag only when the mode changes.
    /// Use -> UseTarget cancels the pending leave, but rearming the same mode does
    /// not. The retained use-source id is separate and survives even Escape/cancel.
    fn set_target_mode(&mut self, mode: TargetMode) {
        if self.target_mode != mode {
            self.target_mode = mode;
            self.leave_target_mode = false;
        }
    }

    /// `search_reason`, for the tests.
    #[must_use]
    pub fn search_reason(&self) -> SearchReason {
        self.reason
    }

    /// Current and maximum stack-split sizes.
    #[must_use]
    pub fn split(&self) -> SplitState {
        self.split
    }

    /// Viewport mouse-down and mouse-up handling, plus
    /// the toolbar's four stance ids.
    ///
    /// The wrapper's table:
    ///
    /// | edge | button | behaviour |
    /// |---|---:|---|
    /// | down | 7 | the select-left handler with `true`; if it did not consume and `search_reason < SearchReason::Examine`: a target mode up → `SearchReason::TargetedUse`, else `SearchReason::Select`; object search at `(x, y)` |
    /// | down | 8 | `Input.UseMouseTurning` → enable mouse look |
    /// | down | 10 | mouse-look off and `search_reason < SearchReason::Use` → `SearchReason::Use`, object search |
    /// | up | 7 | the select-left handler with `false` |
    /// | up | 8 | a movement drag ends it; else mouse turning → mouse look off; else `search_reason < SearchReason::Use` → `SearchReason::Examine`, object search |
    ///
    /// So with the default `Input.UseMouseTurning = false`, **right-click is examine** and
    /// left-double-click is use. That is not rebindable; it is hard-coded.
    ///
    /// Returns whether a pick was armed.
    ///
    /// **Public only so that a test can call an input into it** — the same
    /// reason and the same precedent as [`Self::on_world_object_found`] above. Its one
    /// production caller is [`use_time`] step 1, and every reason this table parks is consumed by
    /// step 3 **in the same frame**, because drawing raises the notice
    /// unconditionally once `looking_for_object` is up. So `search_reason` is `SearchReason::None`
    /// at every frame boundary and a harness that reads it between frames is reading the tail of
    /// the notice, never the gate. Driving this apart from step 3 is the only way to observe the
    /// gate itself: the field a whole-frame test reads is not the field the gate reads.
    ///
    /// `screen` is the back buffer. The 3D viewport's rectangle — the one
    /// object picking subtracts its origin from — comes from
    /// [`Self::note_game_viewport`]; with nothing pushed in, the two are the
    /// same rectangle.
    pub fn wrapper_mouse(
        &mut self,
        e: UiMouseEvent,
        screen: (u32, u32),
        world_click: bool,
    ) -> bool {
        use crate::actions::ui as action;
        // ---- UI action cases 7 and 8 -------------
        //
        // Retail tests the action's start flag, then the current target mode. A
        // press with a nonzero mode sets the leave-target-mode flag before dispatching to the
        // UI manager's action handler; an absent manager returns false.
        //
        // The start flag belongs to the action, and this is not a vendor gate: the fields are
        // target mode and the leave-target-mode flag, which the target-mode setter independently
        // confirms.
        //
        // So this is not a vendor arm at all. It is the **one-shot** half of the use/examine
        // cursor: a click while a target mode is up marks the mode to be left, and
        // the per-frame UI update drops it at the end of the frame
        // (if the flag is set and a target mode is up, it clears the mode, unregisters the
        // targeting input map and updates the cursor; the flag is then cleared) — **whether or not the click hit
        // anything**. Without it the mode would be cleared only by
        // [`Self::execute_target_mode_for_item`], i.e. only by a click that found an object, so a
        // use-cursor armed and then clicked at the sky would stay armed for ever.
        //
        // `0x07`/`0x08` are the left and right mouse buttons — `crate::actions::ui` names the
        // same two ids `PRIMARY_CLICK`/`SECONDARY_CLICK` and the device input's Keystone
        // suppression list is `[7, 8, 10, 11]`. This arm runs **above** everything else in this
        // function because in retail it runs before the viewport wrapper ever sees the
        // click.
        if e.start
            && (e.action == crate::interaction::action::SELECT_LEFT
                || e.action == crate::interaction::action::SELECT_RIGHT)
            && self.target_mode != TargetMode::None
        {
            self.leave_target_mode = true;
            self.stats.leave_target_mode_armed += 1;
        }
        // The toolbar's stance icons, which are ordinary buttons and not the viewport.
        // Button release handling tests `action == 7 || action == 10`, so a
        // double-click's second release is a click too.
        let click = e.action == action::PRIMARY_CLICK || e.action == 0x0A;
        if e.start && click {
            self.left_pressed_on = e.over;
        } else if click {
            let pressed = self.left_pressed_on.take();
            if e.over.is_some_and(is_combat_mode_button) && pressed == e.over {
                self.stats.combat_mode_toggles += 1;
                self.toggle_combat_mode();
                return false;
            }
        }
        // Mouse-down case 8 turns mouse-look on when mouse turning is enabled, and in
        // this rebuild the answer to that test is *yes*: the host's camera input turns the camera
        // on a held right button unconditionally, at the window-system level and outside this
        // wrapper. The press point is recorded here — **above** the viewport guard, so a release
        // that lands on the HUD cannot leave a stale one behind for the next gesture.
        let right_press = if e.action == action::SECONDARY_CLICK {
            if e.start {
                self.right_pressed_at = Some((e.x, e.y));
                None
            } else {
                self.right_pressed_at.take()
            }
        } else {
            None
        };
        if !world_click {
            return false;
        }
        let rect = self.device_viewport(screen);
        let arm = |me: &mut Self, r: SearchReason| -> bool {
            me.reason = r;
            me.stats.picks_requested += 1;
            me.pick.find_object(e.x, e.y, rect)
        };
        if e.start {
            match e.action {
                // The select-left handler consumes the press only while mouse turning is on
                // *and* mouse-look is active — the classic "hold both buttons to run". Neither is
                // reachable in this build (`Input.UseMouseTurning` defaults false and this module
                // does not own the camera's mouse-look), so the press always falls through, which
                // is the retail default path.
                action::PRIMARY_CLICK if self.reason < SearchReason::Examine => {
                    let r = if self.target_mode == TargetMode::None {
                        SearchReason::Select
                    } else {
                        SearchReason::TargetedUse
                    };
                    arm(self, r)
                }
                // `case 10`: the left double-click, gated on mouse-look being off.
                0x0A if self.reason < SearchReason::Use => arm(self, SearchReason::Use),
                _ => false,
            }
        } else {
            match e.action {
                action::SECONDARY_CLICK if self.reason < SearchReason::Examine => {
                    // **Mouse-up `case 8` has three legs and only the third examines**: an active
                    // mouse movement is ended (and the cursor updated); otherwise mouse-look is
                    // turned off; otherwise, when the search reason is below 3, it becomes
                    // `SearchReason::Examine` and an object search starts.
                    //
                    // Both early returns are "the right button was **turning the camera**, so
                    // letting go ends the turn and appraises nothing". The host's camera input
                    // turns the camera on every right-button hold, so without them **every camera
                    // turn would appraise whatever happened to be under the cursor when the button
                    // came up**, one appraisal request per turn.
                    //
                    // The discriminator is the client's own drag threshold: a press and release at
                    // the same spot is a click, a press and release more than
                    // `DRAG_THRESHOLD_SQUARED` apart is a drag. `crate::actions::ui`'s constant is used
                    // rather than a new one because it is the same question
                    // as the native drag-start test.
                    let dragged = right_press.is_some_and(|(px, py)| {
                        let (dx, dy) = (e.x - px, e.y - py);
                        dx * dx + dy * dy > crate::actions::ui::DRAG_THRESHOLD_SQUARED
                    });
                    if dragged {
                        self.stats.mouse_look_releases += 1;
                        false
                    } else {
                        arm(self, SearchReason::Examine)
                    }
                }
                _ => false,
            }
        }
    }

    /// Object-found notice handling, apart from the
    /// highlighting and the tooltip.
    ///
    /// The order below is retail's, and two things in it are easy to get wrong:
    ///
    /// * the **selection happens for every reason except `SearchReason::Drop` and
    ///   `SearchReason::TargetedUse`** — not only for `SearchReason::Select`. Examining or using an
    ///   object selects it as a side effect, which is why the target box follows a right-click;
    /// * `search_reason` is cleared at the **end**, unconditionally, so a pick that found nothing
    ///   still ends the gesture.
    ///
    /// The `SearchReason::Use` arm's guard is the client's: item use on the selected object runs
    /// only when the found object's wielder id is not the player's own id — you cannot
    /// double-click your own equipped sword into a use.
    ///
    /// **Public only so that a test can call an input into it.** Its one
    /// production caller is [`use_time`] step 3, behind `WorldPicker::draw_no_blit`, which needs a
    /// loaded `WorldScene` and a rendered frame — so without this no headless test could reach
    /// any arm of this function directly. It **is** the
    /// client's notice handler, so making it callable is what the notice already is.
    pub fn on_world_object_found(
        &mut self,
        id: ObjectId,
        game: &mut dereth_client_model::World,
        now: ServerTime,
    ) {
        let mut out = Notices::default();
        let mut req = RecordingRequests::default();
        let found = id.0 != 0 && game.weenie(id).is_some_and(|w| w.pwd.bitfield & 0x80 == 0);
        if id.0 != 0 && !found {
            // The retail object-found notice rejects absent/hidden weenies by
            // set_found_object(0,-1), whose synchronous zero notice still completes a drop.
            let zero = self.pick.set_found_object(ObjectId(0), -1);
            self.on_world_object_found(zero, game, now);
            return;
        }
        // Whether the object under the cursor *changed*.
        // Latched **here**, at the top, because that is where
        // the client latches it: the value is held locally across everything the reason
        // arms do, and the tooltip block at the tail reads that copy and not the field. Reading
        // the field again at the tail would be the same answer today and would quietly stop being
        // one the moment an arm re-entered the notice.
        let object_under_cursor_changed = self.iid_selected_object != id;
        // The world selection blink. Retail compares the last found id to the
        // new id and tests whether the flip count is nonzero. When both hold, it restores the
        // previous object's lighting and zero the flip count.
        //
        // Every notice takes this — a *hover* over a different object (or over nothing, id 0)
        // while a blink is in flight restores it and stops counting.
        if self.iid_selected_object != id && self.flip_count != 0 {
            self.pending_lighting
                .push((self.iid_selected_object, LightingMode::Restore));
            self.flip_count = 0;
        }
        if found {
            self.stats.objects_found += 1;
            // `SearchReason::Select` and every reason above it, *including*
            // Drop and TargetedUse, light the found object bright and arm the flip:
            //
            // Retail applies high lighting `(0.99, 1.0)`, sets the flip count to 1
            // and the next flip time to the current local time plus 0.2 seconds, then assigns
            // selection `(id, 0)` unless the reason is Drop (5) or TargetedUse (7).
            //
            // So the world blink is bright at once, on the notice — the doll's is not.
            if self.reason >= SearchReason::Select {
                self.pending_lighting.push((id, LightingMode::High));
                self.flip_count = 1;
                self.time_next_flip = now.0 + dereth_animation::parts::SELECTION_FLIP_INTERVAL;
                if self.reason != SearchReason::Drop && self.reason != SearchReason::TargetedUse {
                    game.set_selected_object(Some(id), false, &mut out);
                    self.stats.selections += 1;
                }
            }
            match self.reason {
                SearchReason::Use => {
                    let wielded_by_player = game
                        .weenie(id)
                        .and_then(|w| w.pwd.wielder_id)
                        .is_some_and(|w| Some(w) == game.player);
                    if !wielded_by_player {
                        if let Some(sel) = game.selected {
                            self.use_object(sel, game, &mut req, &mut out, now);
                        }
                    }
                }
                // Examination raises the assess panel's request.
                // **Not** gated by the inventory lock: examination is not an inventory request.
                //
                // This is the whole of examining an object, not only its appraisal request:
                // `0x00C8` alone leaves out the examine-object notice, which
                // records the awaited appraisal id. The appraise-info reply shows the panel only
                // for the awaited id, so without the notice the cursor route would ask the
                // question with nothing listening for the answer.
                // Routing through [`Self::examine_object`] keeps one examination path in this
                // file. Its zero arm is unreachable from here: this arm is inside `if found`, which
                // requires `id.0 != 0` — exactly as it
                // is in retail, whose object-found handler puts the entire block behind a non-zero id.
                SearchReason::Examine => self.examine_object(game, &mut req, id),
                SearchReason::TargetedUse => {
                    self.execute_target_mode_for_item(id, game, &mut req, &mut out, now)
                }
                _ => {}
            }
        }
        if self.reason == SearchReason::Drop {
            // The drop-target check uses `(drop item, found id, 1)`. A found object that is a
            // container takes the item; otherwise it goes on the ground.
            let item = self.drop_item;
            if item.0 != 0 {
                self.place_in_3d(
                    item,
                    if found { Some(id) } else { None },
                    game,
                    &mut req,
                    &mut out,
                    now,
                );
            }
            self.drop_item = ObjectId(0);
        }
        // **The tooltip.** The whole block uses the change flag latched above:
        // *the object under the cursor changed*. A hover that finds the same object again does
        // nothing at all — not a re-set, not a clear — which is what stops the pointer resting
        // on a chest from
        // re-arming (and so re-delaying) the tooltip on every frame-loop pick.
        if object_under_cursor_changed {
            if let Some(call) = self.world_tooltip(id, found, game) {
                self.pending_tooltip = Some(call);
            }
        }
        // The tail assigns the last-found id unconditionally,
        // hover and zero included —
        // then clears the search reason, whatever happened.
        self.iid_selected_object = id;
        self.reason = SearchReason::None;
        self.absorb(game, out, req);
    }

    /// The global loop's first half — the
    /// flip counter — which [`Self::dispatch_ui_hover`] (its second half) left out:
    ///
    /// Retail returns when the flip count is zero or the current time is before
    /// the next flip. Otherwise it increments the count: at 5 it resets it to zero and
    /// restores lighting; below 5 it schedules the next step 0.2 seconds later and applies high
    /// lighting on odd counts or low lighting on even counts to the last found object.
    ///
    /// So after the notice's own bright (count 1): dim at +0.2 s (2), bright at +0.4 (3), dim at
    /// +0.6 (4), restored at +0.8 (5 → 0). **Two bright flashes**, 0.8 s in all: it "blinks
    /// brightly a couple of times". `cur_time` is the local timer's current time.
    pub fn global_loop_lighting(&mut self, cur_time: f64) {
        if self.flip_count == 0 || cur_time < self.time_next_flip {
            return;
        }
        let count = self.flip_count + 1;
        self.flip_count = count;
        let mode = if count < 5 {
            self.time_next_flip = cur_time + dereth_animation::parts::SELECTION_FLIP_INTERVAL;
            if count & 1 != 0 {
                LightingMode::High
            } else {
                LightingMode::Low
            }
        } else {
            self.flip_count = 0;
            LightingMode::Restore
        };
        self.pending_lighting.push((self.iid_selected_object, mode));
    }

    /// The `apply_lighting` calls queued since the last drain, oldest first — App
    /// hands each to `WorldScene::apply_object_lighting`.
    pub fn take_pending_lighting(&mut self) -> Vec<(ObjectId, LightingMode)> {
        std::mem::take(&mut self.pending_lighting)
    }

    /// The selection flip count, for tests.
    #[must_use]
    pub fn selection_flip_count(&self) -> u32 {
        self.flip_count
    }

    /// Which tooltip assignment or clear the object-found notice makes,
    /// or `None` for the one arm that makes neither. See [`WorldTooltip`] for the
    /// native behavior this transcribes.
    ///
    /// `found` is the caller's, and it matches the native object-found predicate: the block's own
    /// test is `id != 0`, but an id whose weenie is absent or `pwd._bitfield & 0x80` never reaches
    /// here — the function's head answered it with `set_found_object(0, -1)` and a re-entrant zero
    /// notice, which is the arm that produces [`WorldTooltip::Clear`].
    fn world_tooltip(
        &mut self,
        id: ObjectId,
        found: bool,
        game: &dereth_client_model::World,
    ) -> Option<WorldTooltip> {
        // Read the tooltip option through the player-option query: bit `0x100`
        // of the options word, default **on**.
        let show = game
            .player_system
            .options
            .get(dereth_client_model::player::option::SHOW_TOOLTIPS);
        if !found || !show {
            self.stats.object_tooltips_cleared += 1;
            return Some(WorldTooltip::Clear);
        }
        // After obtaining the appropriate name with final argument 0, retail tests
        // the wide buffer's length word, 1 for the empty string (the terminator alone). An object
        // with no name leaves the wrapper's tooltip exactly as it was.
        let Some(name) = game
            .weenie(id)
            .map(|w| w.object_name(dereth_client_model::weenie::NameType::Appropriate))
        else {
            self.stats.object_tooltips_unnamed += 1;
            return None;
        };
        if name.is_empty() {
            self.stats.object_tooltips_unnamed += 1;
            return None;
        }
        self.stats.object_tooltips_set += 1;
        Some(WorldTooltip::Set {
            name,
            pointer_in_viewport: self.pointer_in_viewport(),
        })
    }

    /// Input-manager mouse coordinates less the render viewport's x/y origin, each
    /// **unsigned**-compared against the viewport's width and height, so a pointer above or left
    /// of the rectangle wraps negative and fails the same bounds check. Object picking uses the
    /// identical pair of subtractions and unsigned comparisons against the same four fields, so this
    /// asks [`Self::device_viewport`] rather than respelling them.
    fn pointer_in_viewport(&self) -> bool {
        // The other half of the same rectangle: in retail the game-viewport calculation
        // has already taken the docked HUD out of it, so a pointer over a panel fails
        // these compares. Here it does not, and the hit-test order is what carries the fact — see
        // [`Self::note_pointer_over_game_view`]. Belt and braces with the hover gate above,
        // because this is the test the object-found notice actually makes and the drag arm is the one path that
        // starts the tooltip at the mouse without the ordinary hover's last-entered element.
        if !self.pointer_over_game_view.0 {
            return false;
        }
        let v = self.device_viewport(self.screen);
        #[allow(clippy::cast_sign_loss)]
        // LINT-OK: wrapping subtraction followed by an unsigned bounds comparison *is* the
        // client's test, with no signed comparison anywhere in the native block.
        let (x, y) = (
            (self.cursor.0 - v.x as i32) as u32,
            (self.cursor.1 - v.y as i32) as u32,
        );
        x < v.width && y < v.height
    }

    /// The `set_tooltip`/`clear_tooltip` call this notice made, for
    /// [`crate::app::App`] to apply to the `<SBOX>` element. Taken, so a second drain in the same
    /// frame does not re-apply it — `set_tooltip` is itself edge-guarded
    /// (`==` first), and a repeat that got through would reset a tooltip the
    /// player is reading.
    pub fn take_world_tooltip(&mut self) -> Option<WorldTooltip> {
        self.pending_tooltip.take()
    }

    /// **The inbound half of the inventory loop.**
    ///
    /// Dispatch the smart-box event `(id, 0, 0)`. All three of this file's "use" gestures — the
    /// viewport double-click's `SearchReason::Use` arm, the toolbar's Use button
    /// and the `USE` action — are that one call in the
    /// client.
    ///
    /// Object use runs
    /// use-result classification **first**, and for an object lying loose in the
    /// 3-D world the answer is 2, which goes to the item-use dispatcher's
    /// place-in-backpack arm and **returns before the use request is sent**.
    /// The pickup is the `0x0019` put-item-in-container `(item, player id, 0)` — there is no
    /// pickup-item event. See
    /// [`dereth_client_model::inventory::use_object`] for the whole chain and for what it leaves.
    ///
    /// Nothing is predicted: placing in the backpack ghosts the icon with the show-pending notice
    /// and the item does not move until `0x0022 Item_ServerSaysContainID` arrives and
    /// [`apply_events`] applies it.
    fn use_object(
        &mut self,
        id: ObjectId,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        out: &mut Notices,
        now: ServerTime,
    ) {
        use dereth_client_model::inventory::use_object::{UseOutcome, UseResult};
        // The UI use command arms target mode for id zero; the inventory use operation does not.
        if id.0 == 0 {
            self.set_target_mode(TargetMode::Use);
            return;
        }
        // The tail's effects are notices and world state, not a return value, so
        // they are counted from what `dereth_client_model` raised during *this* call and not from a summary
        // flag: `ground_object` before and after is the only thing that says whether a corpse
        // actually became the open ground container.
        let (n_ground, n_contained, n_panels, n_confirm) = (
            out.ground.len(),
            out.contained.len(),
            out.panels,
            out.usage_confirmations.len(),
        );
        let ground_before = game.ground_object;
        let outcome = game.use_object(req, out, id, self.split, now);
        let opened_ground: usize = out.ground[n_ground..].iter().filter(|g| g.0 != 0).count();
        self.stats.contained_containers_opened += (out.contained.len() - n_contained) as u64;
        self.stats.panels_requested += out.panels - n_panels;
        self.stats.usage_confirmations +=
            u64::try_from(out.usage_confirmations.len() - n_confirm).unwrap_or(0);
        // Setting the ground object is attempted only at the tail of item use, and the tail runs
        // only for a container that is useable and not the player's. The setter's own "only when
        // it differs from the current ground object" test is what makes this a change rather than a
        // call, which is why the *field* is the oracle and `opened_ground` (the closing notice for
        // whatever was open before) is only a cross-check.
        if game.ground_object != ground_before && game.ground_object.is_some() {
            self.stats.ground_objects_requested += 1;
        } else if self.tail_asked_for_a_ground_object(game, id) {
            self.stats.ground_objects_refused += 1;
        }
        debug_assert!(
            opened_ground == 0,
            "setting the ground object raises only the closing notice; the opening one is the \
             view-contents handler's"
        );
        match outcome {
            UseOutcome::Dispatched {
                result: UseResult::PlaceInBackpack,
                sent: true,
            } => {
                self.stats.pickups_requested += 1;
            }
            UseOutcome::Dispatched {
                result: UseResult::Trade,
                sent: true,
            } => {
                self.stats.trades_requested += 1;
            }
            UseOutcome::Dispatched {
                result,
                sent: false,
            } => {
                // Still reported, and still not silent: arm 7 (the minigame start) answers `false` in
                // the client itself, so a game board reaches here having raised its notice.
                self.stats.uses_undispatched += 1;
                tracing::debug!("item use arm {result:?} for {id:?} sent nothing");
            }
            UseOutcome::Confirming(kind) => {
                tracing::debug!("{id:?} asks first: {}", kind.prompt());
            }
            UseOutcome::TargetModeArmed => {
                // Record the targeting object and set `TargetMode::UseTarget`. The mode is this
                // struct's, so this arm is the one place `dereth_client_model` can ask for it.
                self.set_target_mode(TargetMode::UseTarget);
                self.stats.target_modes_armed += 1;
            }
            UseOutcome::Refused(_) => self.stats.uses_refused += 1,
            UseOutcome::Dispatched { .. }
            | UseOutcome::UseEventSent
            | UseOutcome::NoObject
            | UseOutcome::Throttled
            | UseOutcome::Busy
            | UseOutcome::Nothing => {}
        }
    }

    /// Whether the item-use dispatcher's tail would have attempted to set
    /// this object as the ground container.
    ///
    /// The same four-part test as the tail itself, asked again so that a refusal can be counted
    /// apart from a gesture that never got that far. It is a *duplicate* of a condition in
    /// `dereth_client_model`, which is normally the wrong thing to do — it is here because the alternative
    /// is a counter that cannot distinguish "no corpse was ever double-clicked" from "every
    /// corpse refused to open". That distinction is the whole point of the measurement.
    fn tail_asked_for_a_ground_object(
        &self,
        game: &dereth_client_model::World,
        id: ObjectId,
    ) -> bool {
        use dereth_client_model::inventory::use_object::ItemUses;
        let Some(w) = game.weenie(id) else {
            return false;
        };
        let uses = ItemUses(w.pwd.useability.unwrap_or(0));
        uses.is_useable()
            && !uses.is_useable_targeted()
            && w.is_container()
            && !game.is_owned_by_player(id)
    }

    /// The toolbar-drop shortcut arm, past the ancestor sweep
    /// (which `ShortcutBar::slot_under` has already run to produce `slot`).
    ///
    /// It removes whatever shortcut is in slot `n`, creates a shortcut to the item there, and —
    /// if a different object was displaced and there is an empty slot to the right of `n` — adds
    /// the displaced object in the first such slot, all with server notification.
    ///
    /// The displacement is the reason [`Self::remove_shortcut_in_slot_num`] *returns* the id it
    /// removed rather than a bool: dropping onto an occupied slot pushes its occupant rightwards
    /// instead of destroying the shortcut.
    fn shortcut_drop(
        &mut self,
        item: ObjectId,
        slot: usize,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        out: &mut Notices,
        now: ServerTime,
    ) -> bool {
        let displaced = self.remove_shortcut_in_slot_num(slot, game, req);
        // `remove_shortcut_in_slot_num` runs first and unconditionally, so a `create_shortcut_to_item`
        // that then refuses leaves the destination **empty** and the displaced shortcut gone.
        // That is the client's behaviour and it is not repaired here: the conditional
        // below is reached only after a successful create.
        if !self.create_shortcut_to_item(item, Some(slot), game, req, out, now) {
            return false;
        }
        if let Some(old) = displaced.filter(|old| *old != item) {
            if let Some(k) = game
                .player_system
                .first_empty_shortcut_to_the_right_of(slot)
            {
                self.add_shortcut(old, k, game, req, now);
            }
        }
        true
    }

    /// Create a shortcut to `item`, as a drag or the make-shortcut key asks: `(item, slot,
    /// from a drag = true, quiet = false)`. The any-slot arm is the make-shortcut key's.
    ///
    /// The gates in the client's own order; see the `DropTarget::ShortcutSlot` arm for the one
    /// piece of the shortcut-eligibility check this build declines to guess at.
    ///
    /// `slot` is `None` for "any slot", the make-shortcut key's call. That arm sweeps the bar
    /// first: an object that already has a shortcut is refused with *"There is already a
    /// shortcut to the …"*, a full bar with *"There are no free shortcut slots"*, and otherwise
    /// the shortcut goes into the first empty slot. Both callers are loud, so every refusal that
    /// has a message prints it on the feedback channel `0x1A`.
    fn create_shortcut_to_item(
        &mut self,
        item: ObjectId,
        slot: Option<usize>,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        out: &mut Notices,
        now: ServerTime,
    ) -> bool {
        // A zero id, or an id with no known object, is a no-op.
        if item.0 == 0 || game.weenie(item).is_none() {
            return false;
        }
        let name = |game: &dereth_client_model::World| {
            game.weenie(item).map_or_else(String::new, |w| {
                w.object_name(dereth_client_model::weenie::NameType::Appropriate)
            })
        };
        let refuse = |out: &mut Notices, text: String| {
            out.emit(dereth_client_model::Notice::DisplayString {
                channel: 0x1A,
                text,
            });
            false
        };
        // The shortcut-eligibility check's third test: a non-zero container id equal to the
        // vendor's id —
        // you cannot make a shortcut to something that is still the shopkeeper's.
        if let Some(v) = game.vendor_id() {
            if v.0 != 0 && game.weenie(item).and_then(|w| w.pwd.container_id) == Some(v) {
                return refuse(
                    out,
                    format!("You cannot make a shortcut to the {}", name(game)),
                );
            }
        }
        // If the item is not the player's and this call came from a drag, pick it up first.
        if !game.is_owned_by_player(item)
            && !game.place_in_backpack(req, out, item, false, self.split, now)
        {
            return false;
        }
        let slot = match slot {
            Some(n) => n,
            None => {
                if game.player_system.shortcut_slot_of(item).is_some() {
                    return refuse(
                        out,
                        format!("There is already a shortcut to the {}", name(game)),
                    );
                }
                if game.player_system.first_empty_shortcut().is_none() {
                    return refuse(out, "There are no free shortcut slots".to_owned());
                }
                // Past the last slot, which `add_shortcut` reads as "the first empty one".
                dereth_client_model::player::SHORTCUT_SLOTS
            }
        };
        // Remove the item's existing shortcut, then add it at the slot, both with server
        // notification — the removal is what stops one object occupying two slots when it is
        // dragged from one tile to another.
        self.remove_shortcut(item, game, req);
        self.add_shortcut(item, slot, game, req, now)
    }

    /// Move `(item, slot)` with server notification on.
    ///
    /// Shortcut insertion's notify-server flag guards the pair this client did not have:
    /// the immediate `0x019C` send and the retained-module update.
    /// The fill step also fills the tile.
    fn add_shortcut(
        &mut self,
        item: ObjectId,
        slot: usize,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        now: ServerTime,
    ) -> bool {
        // A slot outside the shortcut slots takes the first empty one instead and
        // gives up when there is none. A zero id is a no-op.
        let slot = if slot < dereth_client_model::player::SHORTCUT_SLOTS {
            slot
        } else {
            match game.player_system.first_empty_shortcut() {
                Some(k) => k,
                None => return false,
            }
        };
        if item.0 == 0 {
            return false;
        }
        let sc = dereth_protocol::login::ShortCutData {
            index: i32::try_from(slot).unwrap_or(-1),
            object_id: item,
            spell_id: 0,
        };
        if !game.player_system.add_shortcut(sc) {
            return false;
        }
        game.player_system.mark_dirty(now);
        dereth_client_model::RequestSink::send(
            req,
            dereth_client_model::Request::AddShortCut(
                dereth_protocol::login::CharacterAddShortCut { shortcut: sc },
            ),
        );
        self.stats.shortcuts_added += 1;
        true
    }

    /// Move `(item)` with server notification on.
    ///
    /// The sweep for the slot holding `item`, then flushing that slot's item list, resetting its
    /// shortcut number to -1 and — under the notify-server flag — the `0x019D` send beside the
    /// retained-module removal.
    /// Answers the slot it emptied.
    fn remove_shortcut(
        &mut self,
        item: ObjectId,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
    ) -> Option<usize> {
        let slot = game.player_system.shortcut_slot_of(item)?;
        game.player_system.remove_shortcut(slot);
        dereth_client_model::RequestSink::send(
            req,
            dereth_client_model::Request::RemoveShortCut(
                dereth_protocol::login::CharacterRemoveShortCut {
                    index: u32::try_from(slot).unwrap_or(0),
                },
            ),
        );
        self.stats.shortcuts_removed += 1;
        Some(slot)
    }

    /// `(slot)` with server notification on — read the
    /// slot's object id, remove *that object's* shortcut, and hand the id back.
    fn remove_shortcut_in_slot_num(
        &mut self,
        slot: usize,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
    ) -> Option<ObjectId> {
        let id = game.player_system.shortcut_at(slot)?;
        self.remove_shortcut(id, game, req);
        Some(id)
    }

    /// The complete UI examination operation.
    ///
    /// A nonzero id is passed to the world's examination operation and returns immediately.
    /// For id zero, when the target mode is not already Examine, native behavior changes it to
    /// Examine, clears the pending leave flag, registers input map `0x1000000B` with the UI's
    /// input-action callback if the input manager exists, then updates the cursor.
    ///
    /// **The zero-id arm is the examine key's behaviour with no selection.** The world's
    /// examination request correctly returns for id zero, but cannot arm the UI target cursor.
    ///
    /// With no selection, the examine key must arm the magnifying-glass cursor until
    /// a click spends it or Escape cancels it. Those consumers are
    /// `crate::cursor::update_cursor_state` and target-mode clearing; the toolbar Examine
    /// button and the key are their producers.
    ///
    /// The input-map registration is not repeated here: `App` mirrors [`Self::target_mode`] into
    /// the device input's target-mode registration after `use_time`, which is the same `0x1000000B` at the
    /// same unfocused-UI input priority this line would push, and doing it twice is how a map ends
    /// up registered under two callbacks.
    fn examine_object(
        &mut self,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        id: ObjectId,
    ) {
        if id.0 != 0 {
            game.examine_object(req, id);
            return;
        }
        if self.target_mode != TargetMode::Examine {
            self.set_target_mode(TargetMode::Examine);
            self.stats.target_modes_armed += 1;
        }
    }

    /// Execute the use/examine cursor's second click.
    fn execute_target_mode_for_item(
        &mut self,
        target: ObjectId,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        out: &mut Notices,
        now: ServerTime,
    ) {
        match self.target_mode {
            // `execute_target_mode_for_item`'s examine arm is
            // not a bare appraisal request; see the
            // `SearchReason::Examine` site.
            // It goes through the complete examination operation rather than its non-zero
            // half, because the native path calls the complete UI examination operation with the
            // found id: a second click that hits nothing re-arms the mode instead of ending it. (It
            // is already armed here — that is what `TargetMode::Examine` means — so the zero arm's
            // `if` is false and the observable is unchanged. It is routed anyway so there is one
            // examination path in this file and not two.)
            TargetMode::Examine => self.examine_object(game, req, target),
            TargetMode::Use => self.use_object(target, game, req, out, now),
            TargetMode::UseTarget => {
                // target_acquired consumes the retained source before compatibility.
                // It is not the mutable selection, and does not re-run object use's throttle.
                let _ = game.target_acquired(req, out, target, self.split, now);
            }
            TargetMode::None => {}
        }
        // execute_target_mode_for_item never clears the mode. The click's deferred
        // `use_time` tail owns that; a generic Use that rearmed UseTarget must survive it.
    }

    /// Attempt to place the dropped item in 3D on the found object.
    ///
    /// **The whole decision is the world's 3D-placement routine**, not a two-way fork where a
    /// container takes the item and anything else puts it on the ground, because the drop has
    /// **several routes to another object**:
    ///
    /// * the item's own gates — a drop onto yourself places it in the backpack, an unowned item
    ///   refuses *"You must first pick up the %s"*, an item on the trade window refuses;
    /// * `attempt_merge`, so a stack dropped on a matching stack in the world merges;
    /// * the vendor arm (`BF_VENDOR`) — `attempt_sell_to_vendor`;
    /// * the secure-trade arm, behind `DragItemOnPlayerOpensSecureTrade`;
    /// * **a `TYPE_CREATURE` target — the give request, `0x00CD`**, which is how an item is
    ///   given to an NPC by dropping it on them;
    /// * the container's own gates — *"The %s is locked"* and *"You must open the %s first"*, so
    ///   this build never sends a put-in-container at a sealed chest it has never opened.
    ///
    /// The ground leg keeps the split test, which is split size >= maximum split size —
    /// [`SplitState::is_whole_stack`], not a separate boolean — and gains the `on_ground` gate and
    /// the *"Move cancelled"* arm.
    fn place_in_3d(
        &mut self,
        item: ObjectId,
        onto: Option<ObjectId>,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        out: &mut Notices,
        now: ServerTime,
    ) {
        // Player id -> physics-object lookup -> ground-contact query,
        // which the client reaches through singletons and this build reaches through
        // [`Interaction::note_player_physics`], written once a frame by [`use_time`] from
        // `WorldScene::character`. `None` -- no body -- is the client's null pointer and takes the
        // **same** refusal branch as an airborne one in retail.
        //
        // What this gate does **not** cover is the give: the place-in-3-D path's creature arm
        // returns 1 much earlier in the path, so the client lets you hand an item
        // to an NPC in mid air. The refusal belongs to the *ground* leg alone -- split-to-3D,
        // put-in-3D and the "Move cancelled" arm.
        let player_on_ground = self.player_on_ground == Some(true);
        if !game.attempt_place_in_3d(
            req,
            out,
            item,
            onto,
            true,
            player_on_ground,
            self.split,
            now,
        ) {
            self.stats.requests_refused += 1;
            // A refused 3D placement clears the dropped item's waiting state, the same un-ghost
            // the item-list and paper-doll arms do for a refused drop.
            game.set_waiting_state(item, false);
        }
    }

    /// Toggle from peace to the default mode for the held equipment, or from any combat mode
    /// back to peace.
    fn toggle_combat_mode(&mut self) {
        // The original element handler calls shared combat state directly. Here UI
        // dispatch has no mutable world borrow, so `run_combat_mode_toggle` performs
        // the call one step later in the same frame.
        self.pending_combat_toggle = true;
    }

    /// **The tail of the two defender-notification handlers, run once per
    /// notification that arrived this frame.**
    ///
    /// The world's defender-notification auto-target operation is the whole of it and carries the
    /// two listings; this function is only the seam, and it exists for the same reason
    /// [`Self::run_combat_mode_toggle`]'s second half does — `auto_target` reaches `select_next`,
    /// which needs [`crate::selection_geometry::SceneSelectionPhysics`], and that lives on this
    /// side of the crate boundary.
    ///
    /// The loop runs the tail **per notification**, not once for the batch: the stamp is
    /// idempotent within a frame but the `auto_target` under it is not, because its own
    /// no-selection gate is falsified by the first one succeeding.
    fn run_defender_notifications(
        &mut self,
        game: &mut dereth_client_model::World,
        phys: &crate::selection_geometry::SceneSelectionPhysics,
        radar_radius: f32,
        now: dereth_primitives::LocalTime,
    ) {
        let n = std::mem::take(&mut self.pending_defender_notifications);
        if n == 0 {
            return;
        }
        let lookup = |id| phys.get(id);
        let mut notices = Notices::default();
        for _ in 0..n {
            self.stats.defender_notifications += 1;
            if game.defender_notification_auto_target(&lookup, radar_radius, now, &mut notices) {
                self.stats.defender_auto_targets += 1;
            }
        }
        self.absorb(game, notices, RecordingRequests::default());
    }

    /// **Combat selection-change handling, run once per
    /// selection-change notice absorbed since the last frame.**
    ///
    /// The world's selection-changed operation is the whole handler and carries the
    /// listing; this is only the seam, and it is the same one
    /// [`Self::run_defender_notifications`] uses and for the same reason — `auto_target` reaches
    /// `select_next`, which needs `crate::selection_geometry::SceneSelectionPhysics`.
    ///
    /// **Per notice, not once for the batch**, because `target_willingly_lost` is cleared by the
    /// notice that it refuses: two notices in one frame are *refuse, then auto-target*, and one
    /// call for the batch would lose the second.
    ///
    /// The handler can itself change the selection — `auto_target` ends in either
    /// setting the selected object or `select_next` — so it raises further notices. Those are
    /// absorbed like any other. App's synchronous owner bridge completes reentry before the next
    /// input; standalone `use_time` callers retain their existing next-frame delivery. A non-zero
    /// selected id refuses those follow-up notices, so
    /// the chain terminates rather than ringing, which is the same bound
    /// retail's synchronous re-entry has.
    fn run_selection_change_notices(
        &mut self,
        game: &mut dereth_client_model::World,
        phys: &crate::selection_geometry::SceneSelectionPhysics,
        radar_radius: f32,
        now: dereth_primitives::LocalTime,
    ) {
        let n = std::mem::take(&mut self.pending_selection_changes);
        if n == 0 {
            return;
        }
        let lookup = |id| phys.get(id);
        let mut notices = Notices::default();
        let mut req = RecordingRequests::default();
        for _ in 0..n {
            self.stats.selection_change_notices += 1;
            let lost_before = game.combat.target_willingly_lost;
            if game.on_selection_changed(&lookup, radar_radius, now, &mut req, &mut notices) {
                self.stats.selection_change_auto_targets += 1;
            } else if lost_before && !game.combat.target_willingly_lost {
                self.stats.selection_changes_willingly_lost += 1;
            }
        }
        self.absorb(game, notices, req);
    }

    /// **The per-frame UI update's target-mode tail**,
    /// the consumer of the flag armed by `SelectLeft`/`SelectRight`:
    ///
    /// When the flag is set: if a target mode is up, it is cleared to none, the targeting input
    /// map `0x1000000B` is unregistered and the cursor state is updated; the flag is cleared
    /// either way.
    ///
    /// Retail clears the flag twice on the inner path; both are kept as the one `take` here.
    ///
    /// Production runs this registered client-UI system at the NEXT frame's UIQueue phase,
    /// before timer/device input. A pick armed later in the
    /// preceding frame therefore finishes first. The standalone `use_time` adapter keeps a
    /// combined tail, but that is not App's scheduler.
    ///
    /// App mirrors this result into the device input's target-mode registration after
    /// `use_time`, removing
    /// the exact map/callback pair before the next input drain. `update_cursor_state` then reads
    /// the resulting mode. Geometric pick completion remains at draw_no_blit, not inside input.
    fn run_leave_target_mode(&mut self) {
        if !std::mem::take(&mut self.leave_target_mode) {
            return;
        }
        if self.target_mode != TargetMode::None {
            self.target_mode = TargetMode::None;
            self.stats.target_modes_left += 1;
        }
    }

    /// Radar visibility from the UI action state, for the tests.
    #[must_use]
    pub fn radar_visible(&self) -> bool {
        self.radar_visible.0
    }

    /// Whether the plugin manager is open.
    #[must_use]
    pub fn plugin_manager_open(&self) -> bool {
        self.plugin_manager_open
    }

    /// The standing-still predicate's answer, pushed in by [`use_time`] from
    /// the body's motion interpreter.
    pub fn note_standing_still(&mut self, still: bool) {
        self.standing_still = StartsTrue(still);
    }

    /// A screenshot was asked for by the `0x55` arm; `App` owns the device.
    pub fn take_screenshot_request(&mut self) -> bool {
        std::mem::take(&mut self.screenshot_requested)
    }

    /// A complete movement stop was asked for by the `EscapeKey` arm — or
    /// by a cast; `App` owns the body.
    pub fn take_stop_completely(&mut self) -> bool {
        std::mem::take(&mut self.stop_completely_requested)
    }

    /// Server-control state pushed in by `App::interaction_use_time` from
    /// `MovementCommands::lists` — the same field `CommandEnv` already reads, for the maybe-stop
    /// check's only test.
    pub fn note_controlled_by_server(&mut self, on: bool) {
        self.controlled_by_server = on;
    }

    /// A panel visibility toggle was asked for by the `EscapeKey`
    /// arm; `App` owns the UI tree.
    pub fn take_visibility_toggle(&mut self) -> Option<u32> {
        self.visibility_toggle_requested.take()
    }

    /// The pending argument, if T raised one.
    pub fn take_split_stack_notice(&mut self) -> Option<ObjectId> {
        self.pending_split_stack.take()
    }

    /// The UI-layout notices raised by valid `@saveui`/`@loadui` command handlers.
    pub fn take_ui_layout_commands(&mut self) -> Vec<UiLayoutCommand> {
        std::mem::take(&mut self.pending_ui_layout_commands)
    }

    /// The pending set-frame-rate-display notices in command order. A replacement gameplay screen
    /// does not get the process flag replayed: retail's screen setup only registers the listener.
    pub fn take_framerate_display_notices(&mut self) -> Vec<bool> {
        std::mem::take(&mut self.pending_framerate_display)
    }

    /// Valid `@render` preference writes in command order.
    pub fn take_render_preferences(
        &mut self,
    ) -> Vec<(&'static str, dereth_client_contract::view::PrefValue)> {
        std::mem::take(&mut self.pending_render_preferences)
    }

    /// Pending popup-title notices in command order.
    pub fn take_chat_window_title_notices(&mut self) -> Vec<(u32, String)> {
        std::mem::take(&mut self.pending_chat_window_titles)
    }

    /// The pending "open the vendor panel's `0x100000BA` tab" tail of a component-list fill.
    pub fn take_vendor_buying_tab_request(&mut self) -> Option<ObjectId> {
        self.vendor_buying_tab_requested.take()
    }

    /// Apply an authoritative combat-mode change after the player's Int `0x28`
    /// has passed the timestamp check. Server authority bypasses compatibility, teleport and
    /// ready-position
    /// checks, does not echo ChangeCombatMode, and preserves the pending local request.
    pub fn on_combat_mode_quality_changed(
        &mut self,
        game: &mut dereth_client_model::World,
        mode: dereth_client_model::combat::CombatMode,
        phys: &crate::selection_geometry::SceneSelectionPhysics,
        radar_radius: f32,
        now: dereth_primitives::LocalTime,
    ) {
        let before = game.combat.combat_mode;
        let mut req = RecordingRequests::default();
        let mut notices = Notices::default();
        // `set_combat_mode` only refuses/queues inside its send-to-server arm.
        // The last two arguments are deliberately hostile: authority must not consult them.
        let _ = game.set_combat_mode(&mut req, &mut notices, mode, false, false, true);
        if game.combat.combat_mode != before {
            let lookup = |id| phys.get(id);
            if game.combat_mode_auto_target(&lookup, radar_radius, now, &mut notices) {
                self.stats.auto_targets += 1;
            }
        }
        self.absorb(game, notices, req);
    }

    /// Apply a pending combat-mode toggle once the world is in hand.
    ///
    /// **And `set_combat_mode`'s `auto_target` tail, which is a second call here.**
    /// `phys`, `radar_radius`, and `now` are the seam inputs auto-targeting
    /// needs and `set_combat_mode` cannot carry; see that function for why the split is a declared
    /// deviation rather than an accident.
    fn run_combat_mode_toggle(
        &mut self,
        game: &mut dereth_client_model::World,
        ready: bool,
        phys: &crate::selection_geometry::SceneSelectionPhysics,
        radar_radius: f32,
        now: dereth_primitives::LocalTime,
    ) {
        if !std::mem::take(&mut self.pending_combat_toggle) {
            return;
        }
        let (mode, refusal) = game.toggle_combat_mode_target(false);
        if let Some(text) = refusal {
            self.refuse(game, &text);
        }
        let mut req = RecordingRequests::default();
        // `set_combat_mode`'s tail raises `SelectionChanged`, so the sink is a real one rather
        // than a discarded `Notices::default()`.
        let mut notices = Notices::default();
        // `set_combat_mode(mode, send to server)`. `teleport_in_progress` is
        // false here; `ready` is the combat-ready predicate, which the caller supplies.
        let before = game.combat.combat_mode;
        if let Err(text) = game.set_combat_mode(&mut req, &mut notices, mode, true, ready, false) {
            self.refuse(game, &text);
            self.stats.requests_refused += 1;
        }
        // Retail reaches it only by falling through the combat-mode store
        // in the native path; three of `set_combat_mode`'s `Ok` returns are above that store (the
        // no-op, the incompatible-mode refusal and the not-ready queue), so the guard is that the
        // mode actually moved rather than that the call returned `Ok`.
        if game.combat.combat_mode != before {
            let lookup = |id| phys.get(id);
            if game.combat_mode_auto_target(&lookup, radar_radius, now, &mut notices) {
                self.stats.auto_targets += 1;
            }
        }
        self.absorb(game, notices, req);
    }

    /// Run the 480-second dirty flush once per frame.
    ///
    /// `PlayerSystem::use_time` answers the timer question. It is the
    /// second of the client's two senders of the character-options event, and it is what makes
    /// a deferred option change reach the shard when the player never reopens the options page.
    ///
    /// The comparison is a strict `>` on the first-dirtied time `+ 480.0` and it fires **once**, not
    /// once per change; `use_time` clears the flag itself.
    pub fn run_player_module_use_time(
        &mut self,
        game: &mut dereth_client_model::World,
        now: ServerTime,
    ) {
        let mut req = RecordingRequests::default();
        if game.player_system.use_time_save(&mut req, now) {
            self.stats.player_modules_sent += 1;
        }
        self.absorb(game, Notices::default(), req);
    }

    /// Object-range checking.
    ///
    /// Called immediately after [`Self::run_player_module_use_time`] because that is where the
    /// client calls it: the player-system update runs the player-module update first
    /// and range-check calculation as its last statement.
    ///
    /// With no local body there is no player id to measure from, so the checks are **skipped**
    /// rather than run — running them would report every watched object out of range and close
    /// every panel. The client cannot reach that state: the player-system update is only reached
    /// in-game, where the local player body exists.
    fn run_object_range_checks(
        &mut self,
        scene: Option<&dyn crate::present::Scene>,
        objects: &mut crate::objects::ObjectStream,
        now: ServerTime,
    ) {
        // **Part drawing's half of the seam.**
        //
        // Selected-object visibility is one global in the client
        // and two halves here: the draw observes, `dereth_client_model::World` latches. This is
        // where the observation crosses, and it is before the checks deliberately -- in the client
        // viewport update is frame step 8 and drawing is step 11, so the flag
        // the range-exit handler reads at step 8 is always one an *earlier*
        // frame's draw wrote. Draining rather than copying keeps the observation single-use; the
        // latch below is what persists, exactly as retail's does.
        //
        // It runs before the `SceneRangeGeometry` bail-out because the observation must not
        // survive a frame with no body -- the pick's clear would then be undone by a stale one.
        //
        // The **clear** is drained first and the set second, which is the client's own order
        // within one frame: object search clears the flag in the UI queue
        // (step 7) and part drawing sets it in the draw (step 11), so a click and a draw in
        // the same frame leave the flag **up**. Reversing these two lines would leave it down.
        if self.pick.take_selected_object_in_view_clear() {
            objects.world.find_object();
            if let Some(sc) = scene {
                sc.clear_selected_part_drawn();
            }
        }
        if scene.is_some_and(crate::present::Scene::take_selected_part_drawn) {
            objects.world.selected_object_in_view = true;
        }
        let player_id = objects.world.player;
        let Some(geometry) = crate::object_range::SceneRangeGeometry::new(
            scene.and_then(crate::present::Scene::character),
            &objects.physics,
            player_id,
        ) else {
            return;
        };
        // Retail's outdoors predicate asks whether the player's current cell id is
        // outdoors, and its one consumer here is
        // the outdoors/indoors `75.0 : 25.0` range pair the radar
        // scales by, fetched from its one owner rather than re-derived. Taken from the body's own
        // cell, which is the `ViewerFrame::position` `Hud::sync` reads.
        let outside = scene
            .and_then(crate::present::Scene::character)
            .is_some_and(|c| dereth_physics::landdefs::is_outdoors(c.position().cell));
        let radar_radius = dereth_client_contract::radar::radar_range(outside);
        let mut out = Notices::default();
        let mut req = RecordingRequests::default();
        let stats = objects.world.calculate_object_range_checks(
            now,
            &geometry,
            radar_radius,
            &mut out,
            &mut req,
        );
        self.stats.range_polls += u64::from(stats.polled);
        self.stats.range_exits += u64::from(stats.exits);
        self.stats.range_exits_without_a_window += u64::from(stats.exits_without_a_window);
        self.stats.range_selection_rearms += u64::from(stats.selection_rearms);
        self.stats.range_selection_clears += u64::from(stats.selection_clears);
        // Selection assignment publishes the current selected id to rendering at its tail,
        // which the range-exit handler reaches on every path --
        // `calculate_object_range_checks` is where that handler runs in this build, so the write
        // is published to the render side immediately after it.
        if let Some(sc) = scene {
            sc.set_selected_object_id(objects.world.viewcone_check_object_id);
        }
        self.absorb(&mut objects.world, out, req);
    }

    /// The combat update's **head**, run once per frame and **before**
    /// [`Self::run_pending_combat_mode`], which is the same order the client has.
    ///
    /// **This is where a click becomes an attack.** The attack control is not a *held* one: a
    /// single click starts the speed/power bar charging, which kicks off an attack when it
    /// reaches the right spot. Both triggers — the combat control's `0x1C`/`1` pair and
    /// `handle_combat_action`'s press/release pair — are the *same* pair
    /// (`set_requested_attack_height` then `end_attack_request`). This call is what advances the
    /// bar; without it the only producer of an attack would be the end-attack request's release
    /// arm. See the world's combat power-bar update.
    fn run_power_bar(
        &mut self,
        game: &mut dereth_client_model::World,
        ready: bool,
        now: dereth_primitives::LocalTime,
    ) {
        let mut req = RecordingRequests::default();
        let before = req.0.len();
        let refusal = game.combat_power_bar_use_time(&mut req, ready, now);
        if req.0.len() > before {
            self.stats.attacks_released += 1;
        }
        if let Some(text) = refusal {
            self.refuse(game, text);
            self.stats.requests_refused += 1;
        }
        self.absorb(game, Notices::default(), req);
    }

    /// The combat-update tail, run once per frame.
    ///
    /// `set_combat_mode`'s not-ready branch stores the requested mode and sends
    /// nothing; this is the only thing that ever retries it. Without this call a toggle pressed
    /// while the body is busy is dropped for ever — see the busy-state gate below.
    fn run_pending_combat_mode(
        &mut self,
        game: &mut dereth_client_model::World,
        ready: bool,
        phys: &crate::selection_geometry::SceneSelectionPhysics,
        radar_radius: f32,
        now: dereth_primitives::LocalTime,
    ) {
        let mut req = RecordingRequests::default();
        let mut notices = Notices::default();
        // `use_time` clears the pending combat mode after `set_combat_mode`'s `auto_target` tail.
        // The callback keeps that order while using this frame's existing geometry snapshot.
        let refusal = game.combat_use_time_with_mode_change(
            &mut req,
            &mut notices,
            ready,
            |game, notices| {
                let lookup = |id| phys.get(id);
                if game.combat_mode_auto_target(&lookup, radar_radius, now, notices) {
                    self.stats.auto_targets += 1;
                }
            },
        );
        if let Some(text) = refusal {
            self.refuse(game, &text);
            self.stats.requests_refused += 1;
        }
        self.absorb(game, notices, req);
    }

    /// Player-description lookup for the UI request dispatcher.
    ///
    /// The dispatcher returns requests it does not own so the caller can report them;
    /// an unhandled request must not disappear silently. Its public test seam
    /// avoids constructing a dat store, scene, network and viewport just
    /// to exercise an input. [`Self::pending_requests`] exposes produced requests.
    ///
    /// The player's qualities belong to the world player record, shared with stat
    /// updates and vendor-price reads. A long-lived qualities reference cannot span
    /// the dispatcher's mutable world borrow. The caller therefore supplies
    /// `player_desc_received`, and each availability gate borrows qualities only
    /// for as long as it needs them.
    fn player_desc(
        game: &dereth_client_model::World,
        player_desc_received: bool,
    ) -> Option<&dereth_client_model::Qualities> {
        if !player_desc_received {
            return None;
        }
        // Match `Hud::player_desc`: prefer the world row, then the
        // retained login description. ACE may deliver the description before object
        // creation on different queues. While the player id or row is missing, the
        // receiver retains that description separately. Reading only the row would count
        // valid raise requests in that interval as refusals and send
        // nothing. Once present, the row remains preferred because stat updates write it.
        game.player_qualities().or_else(|| game.login_player_desc())
    }

    pub fn run_ui_requests(
        &mut self,
        game: &mut dereth_client_model::World,
        player_desc_received: bool,
        now: ServerTime,
    ) -> Vec<UiRequest> {
        self.run_ui_requests_with_chat_focus(game, player_desc_received, now, &mut |_| {})
    }

    /// App's synchronous UI subscriber boundary. A listen-option change may disable the
    /// current chat focus, and that fallback must happen before the next queued ChatLine.
    pub fn run_ui_requests_with_chat_focus(
        &mut self,
        game: &mut dereth_client_model::World,
        player_desc_received: bool,
        now: ServerTime,
        chat_focus: &mut dyn FnMut(&mut dereth_client_model::chat::ChatState),
    ) -> Vec<UiRequest> {
        let mut unowned = Vec::new();
        let mut out = Notices::default();
        let mut req = RecordingRequests::default();
        // The combat window's arms need what the keyboard's arms need.
        //
        // The two arms below are `set_requested_attack_height` and
        // `end_attack_request`, and both reach a readiness query with the true argument --
        // through attack-charge startup and attack execution respectively. So this is the
        // **attack** flavour, and it is answered by [`Self::ready_for_attack`] from the combat
        // mode, the combat table DataID and the six missile stances.
        //
        // It is read here rather than passed in because [`use_time`] calls this **after**
        // [`Self::note_player_physics`] and the style bridge, so the two inputs are already this
        // frame's; a parameter would only let a caller disagree with the frame it is in.
        //
        // `ServerTime` and `LocalTime` are the same here, which is what
        // `use_time`'s own `ServerTime(now.0)` already assumes.
        let ready = self.ready_for_attack(game);
        let local_now = dereth_primitives::LocalTime(now.0);
        for r in std::mem::take(&mut self.ui_requests) {
            match r {
                UiRequest::Select(id) => {
                    // Selection assignment with `(ulong id, int force)` takes
                    // the id as a plain `ulong`, and **0 is how retail says "nothing"** -- the
                    // Escape key's `(0, 0)` call below is one caller, and
                    // the toolbar header's `(0, 0)` tail call is another. The world's selection is
                    // an `Option`, so the `ulong` 0 is `None` here.
                    let id = (id != ObjectId(0)).then_some(id);
                    game.set_selected_object(id, false, &mut out);
                    self.stats.selections += 1;
                }
                // The paper-doll message handler's `0x1C` tail — the half of
                // paper-doll item-under-mouse lookup that needs the player.
                //
                // It finds the upper inventory object for the mask and returns if there is none.
                // On a primary click (7) with a target mode up it executes the target mode on
                // **the player**; otherwise a primary click selects the object. A secondary click
                // (8) selects and examines it.
                //
                // The target-mode arm really does pass the player's id and not the found object.
                // Reproduced, not corrected.
                UiRequest::PaperDollRegion { mask, secondary } => {
                    let Some(player) = game.player else {
                        self.stats.ui_requests_handled += 1;
                        continue;
                    };
                    // The upper-inventory fallback is the player id when nothing
                    // the player is wearing covers that colour. Clicking a bare shoulder selects
                    // you, and it is the same statement that makes an empty doll clickable at all.
                    let id = game
                        .inventory(player)
                        .and_then(|inv| inv.upper_inv_obj(mask))
                        .unwrap_or(player);
                    if !secondary && self.target_mode != TargetMode::None {
                        self.execute_target_mode_for_item(player, game, &mut req, &mut out, now);
                        self.stats.ui_requests_handled += 1;
                        continue;
                    }
                    game.set_selected_object(Some(id), false, &mut out);
                    self.stats.selections += 1;
                    if secondary {
                        game.examine_object(&mut req, id);
                    }
                }
                // The busy-count increment has one owner and is also read
                // by enable_selection's current-focus fallback. Nothing is sent here.
                UiRequest::SetTalkFocus { focus } => {
                    if let Some(focus) = dereth_client_model::chat::TalkFocus::from_raw(focus) {
                        game.chat.set_talk_focus(focus);
                    }
                    self.stats.ui_requests_handled += 1;
                    continue;
                }
                // The chat-focus enable notice carries `(n, on)` and is raised by the
                // allegiance panel's three data-update tails. The write lands on the one
                // `ChatState`; its `TalkFocusNotice` then reaches the menu row through
                // `deliver_chat_focus_notices`, the same path `0x0295 ChatRoomTracker` and the
                // fellowship messages already take. Nothing is sent.
                UiRequest::SetTalkFocusEnabled { focus, enabled } => {
                    if let Some(focus) = dereth_client_model::chat::TalkFocus::from_raw(focus) {
                        game.chat.set_talk_focus_enabled(focus, enabled);
                        // **This does not queue; it tail-jumps.**
                        //
                        // Retail updates the communication system's talk-focus mask
                        // (`mask |= 1 << focus`) and tail-call the enable-chat notice broadcaster,
                        // which invokes every registered listener synchronously.
                        // Thus the row's state change has already happened when this function
                        // returns. The two `SetPlayerOption` arms below drain the queue
                        // here for the same reason. Without it the three notices the
                        // allegiance panel's data-update tails raise would stay in
                        // `ChatState::talk_focus_notices` until the **next** frame's
                        // `App::ui_use_time`. A one-frame-late row state is invisible in a
                        // screenshot, which is why a test pins it.
                        chat_focus(&mut game.chat);
                    }
                    self.stats.ui_requests_handled += 1;
                    continue;
                }
                // The toolbar's Use button sends
                // `(id, 0, 0)` behind a null check — the same entry point the
                // double-click uses, so it is routed the same way.
                UiRequest::Use(id) => self.use_object(id, game, &mut req, &mut out, now),
                UiRequest::ExecuteTargetItem(id) => {
                    self.execute_target_mode_for_item(id, game, &mut req, &mut out, now);
                }
                UiRequest::CloseExternalContainer(id) => {
                    self.use_object(id, game, &mut req, &mut out, now);
                    game.object_range_checks.unregister(
                        dereth_client_model::range::RangeHandler::ExternalContainer,
                        id,
                    );
                }
                UiRequest::UnregisterSlumlordRange => game.unregister_slumlord_range_checks(),
                UiRequest::UnregisterBookRange => game.unregister_book_range_checks(),
                // The toolbar's Examine button: the complete examination operation.
                //
                // The *panel* half of this route has its writer on the
                // screen (`GamePlayScreen`'s `UiRequest::Examine` arm calls
                // `panels::examination::ExaminationPanel::examine_object`); this is the game half.
                // Going through [`Self::examine_object`] means the toolbar button
                // pressed with nothing selected arms the cursor the way the native toolbar's
                // examination of the selected id does. The `UiRequest::SetTargetMode` arm below is
                // *the toolbar's other* producer and stays as it is; this one is the id path.
                UiRequest::Examine(id) => self.examine_object(game, &mut req, id),
                // Spell examination's first block:
                // item appraisal with a literal zero argument.
                // The **cancel**, and the only zero-id appraise in retail — the
                // other two appraisal senders both guard the id non-zero.
                //
                // It is deliberately **not** the arm above with a zero id. Spell examination calls
                // the packer directly and never goes through ordinary object examination,
                // whose zero arm means the opposite thing — arm `TargetMode::Examine`
                // — and is what `Self::examine_object`'s `id.0 == 0` leg is. Two functions in
                // retail, two variants here; routing the cancel through `examine_object` would
                // put the pointer into examine mode on every spell right-click.
                //
                // The *whether* is the panel's (`ExaminationPanel::examine_spell`, guarding on its
                // own two appraisal ids as retail does); this arm is the unconditional half.
                UiRequest::CancelAppraisal => game.cancel_appraisal(&mut req),
                UiRequest::StackSliderChanged { split, max } => {
                    self.split = SplitState {
                        split_size: split,
                        max_split_size: max,
                    };
                    self.stats.ui_requests_handled += 1;
                    continue;
                }
                UiRequest::ClearItemWaiting(item) => game.set_waiting_state(item, false),
                // The object-state half of the item-list drag-start handler's
                // `set_waiting_state(1)` call.
                // The pick-up ghost lives on the object in retail, which is what lets
                // the item list's flush and slot clear run over the slot
                // without losing it.
                UiRequest::SetItemWaiting(item) => game.set_waiting_state(item, true),
                // The toolbar's Use / Examine
                // button pressed with **nothing selected**. This is the only writer of
                // `target_mode` in the client. The field is read at
                // three sites (`wrapper_mouse`'s `SearchReason::TargetedUse` arm, `use_shortcut`'s
                // and `execute_target_mode_for_item`), all three dead without this writer.
                UiRequest::SetTargetMode(m) => {
                    self.set_target_mode(match m {
                        dereth_client_contract::view::TargetMode::None => TargetMode::None,
                        dereth_client_contract::view::TargetMode::Use => TargetMode::Use,
                        dereth_client_contract::view::TargetMode::Examine => TargetMode::Examine,
                    });
                    self.stats.target_modes_armed += 1;
                    self.stats.ui_requests_handled += 1;
                    continue;
                }
                UiRequest::DragDrop { item, target } => {
                    self.drag_drop(item, target, game, &mut req, &mut out, now);
                }
                // The toolbar's item-list drag-start notice has one
                // effect: remove the shortcut with server notification, which updates the retained
                // player module and sends the shortcut-removal event (`0x019D`)
                // under the same notify-server flag.
                //
                // The screen raises it the moment the icon leaves the tile, not when it lands —
                // that is the client's order (the item-list drag start's closing notice)
                // and it is the whole of *"dragging items FROM the shortcut bar to remove them"*:
                // there is no removal anywhere on the drop path.
                UiRequest::RemoveShortcut(item) => {
                    self.remove_shortcut(item, game, &mut req);
                    self.stats.ui_requests_handled += 1;
                    continue;
                }
                // The make-shortcut key on the selected object: a shortcut in the first empty slot,
                // or one of the refusals, with the same pick-up of an object the player is not
                // carrying that a drag onto the bar makes.
                UiRequest::CreateShortcut(item) => {
                    if !self.create_shortcut_to_item(item, None, game, &mut req, &mut out, now) {
                        self.stats.requests_refused += 1;
                    }
                    self.stats.ui_requests_handled += 1;
                    continue;
                }
                // Inscription focus loss sends the set-inscription request.
                // The panel has already made both
                // of the client's decisions — is there anything to say, and has it changed — so
                // this arm is the world's inscription attempt and the counter.
                UiRequest::SetInscription { object, text } => {
                    game.attempt_set_inscription(&mut req, object, &text);
                    self.stats.ui_requests_handled += 1;
                    continue;
                }
                // The abuse-report panel has already refused an
                // empty field. Retail supplies literal status 1 and sends this ordered 0x0140
                // without an optimistic result; the 0x04B8..0x04BA reply owns that text.
                UiRequest::AbuseLog { target, complaint } => {
                    dereth_client_model::RequestSink::send(
                        &mut req,
                        dereth_client_model::Request::AbuseLog(
                            dereth_protocol::admin::CharacterAbuseLogRequest {
                                target,
                                status: 1,
                                complaint,
                            },
                        ),
                    );
                }
                // The book panel's five writing requests. It has already checked
                // authorship and pending-request guards. Like inscription requests, these are
                // ordered game actions; no optimistic world write belongs here.
                UiRequest::BookAddPage { book } => {
                    dereth_client_model::RequestSink::send(
                        &mut req,
                        dereth_client_model::Request::BookAddPage(
                            dereth_protocol::trade::WritingBookAddPage { book_id: book },
                        ),
                    );
                }
                UiRequest::BookModifyPage { book, page, text } => {
                    dereth_client_model::RequestSink::send(
                        &mut req,
                        dereth_client_model::Request::BookModifyPage(
                            dereth_protocol::trade::WritingBookModifyPage {
                                book_id: book,
                                page,
                                text,
                            },
                        ),
                    );
                }
                UiRequest::BookDeletePage { book, page } => {
                    dereth_client_model::RequestSink::send(
                        &mut req,
                        dereth_client_model::Request::BookDeletePage(
                            dereth_protocol::trade::WritingBookDeletePage {
                                book_id: book,
                                page,
                            },
                        ),
                    );
                }
                UiRequest::BookPageData { book, page } => {
                    dereth_client_model::RequestSink::send(
                        &mut req,
                        dereth_client_model::Request::BookPageData(
                            dereth_protocol::trade::WritingBookPageData {
                                book_id: book,
                                page,
                            },
                        ),
                    );
                }
                UiRequest::BookData { book } => {
                    dereth_client_model::RequestSink::send(
                        &mut req,
                        dereth_client_model::Request::BookData(
                            dereth_protocol::trade::WritingBookData { book_id: book },
                        ),
                    );
                }
                // The original barber panel prepares all sixteen values, closes
                // the modal, then waits for authoritative appearance data. This arm preserves
                // that field order and queues `0x0311`.
                UiRequest::BarberFinish(b) => {
                    dereth_client_model::RequestSink::send(
                        &mut req,
                        dereth_client_model::Request::FinishBarber(
                            dereth_protocol::trade::CharacterFinishBarber(
                                dereth_protocol::trade::BarberSettings {
                                    base_palette: b.base_palette,
                                    head_object: b.head_object,
                                    head_texture: b.head_texture,
                                    default_head_texture: b.default_head_texture,
                                    eyes_texture: b.eyes_texture,
                                    default_eyes_texture: b.default_eyes_texture,
                                    nose_texture: b.nose_texture,
                                    default_nose_texture: b.default_nose_texture,
                                    mouth_texture: b.mouth_texture,
                                    default_mouth_texture: b.default_mouth_texture,
                                    skin_palette: b.skin_palette,
                                    hair_palette: b.hair_palette,
                                    eyes_palette: b.eyes_palette,
                                    setup_id: b.setup_id,
                                    option1: b.option1,
                                    option2: b.option2,
                                },
                            ),
                        ),
                    );
                }
                // The character ping request -- `0x01E9`, with an
                // empty body. The panel decides *when* (on open, then every 120 s); this arm is
                // the one hop from its request to the ordered send queue.
                //
                // Without it `dereth_protocol::admin::CharacterRequestPing` has no production
                // caller, and `0x01EA` never arrives.
                UiRequest::RequestPing => {
                    dereth_client_model::RequestSink::send(
                        &mut req,
                        Request::RequestPing(dereth_protocol::admin::CharacterRequestPing),
                    );
                    self.stats.ui_requests_handled += 1;
                    continue;
                }
                // Chat command submission's only external call
                // carries `(text, window id)`, and that is
                // `dereth_client_model::cmd::CommandInterp::on_chat_command`, tested against
                // a live `@acehelp` observation. Its two server-bound outcomes both become
                // `Communication_Talk` (0x0015): `ForwardVerbatim` is the client's own
                // "unrecognised `@`-command goes to the server `@` and all, no allow-list", and a
                // plain line with the default talk focus is speech.
                //
                // `Handled` is a **locally** handled command (`@help`, `@quit`, …). One whose
                // handler is not built is counted and reported rather than turned into speech
                // — sending `@help` to the shard as a spoken line would be both wrong and rude.
                UiRequest::ChatLine { text, window } => {
                    // Convert submitted chat from wide to narrow text BEFORE command parsing and
                    // every destination. Input history remains the original wide text.
                    self.chat.push_history(&text);
                    let Some(narrow) = game
                        .chat
                        .text_conversion
                        .narrow(&text.encode_utf16().collect::<Vec<_>>())
                    else {
                        // Explicit unsupported host/count, not guessed ANSI bytes or Say fallback.
                        self.stats.chat_commands_refused += 1;
                        self.stats.ui_requests_handled += 1;
                        continue;
                    };
                    // Existing narrow-message String storage is a bijective BYTE SPELLING:
                    // cp1252::encode writes these exact bytes, even when ACP is not1252.
                    // This is NOT ACP decoding to Unicode, and must not be used for display.
                    let text = dereth_protocol::cp1252::decode(&narrow);
                    let focus =
                        dereth_client_model::cmd::TalkFocus::from_raw(game.chat.talk_focus as u32)
                            .unwrap_or(dereth_client_model::cmd::TalkFocus::Say);
                    let outcome = self.chat.on_chat_command(&text, window, focus);
                    self.dispatch_chat_outcome(
                        outcome,
                        window,
                        game,
                        &mut req,
                        &mut out,
                        now,
                        player_desc_received,
                        chat_focus,
                    );
                    self.stats.chat_lines_sent += 1;
                    self.stats.ui_requests_handled += 1;
                    continue;
                }
                // ---- the three advancement requests -----------------------------------------
                //
                // Each arm is one call into `dereth_client_model::advancement`; without these
                // arms a raise would spend nothing and change nothing.
                //
                // No local stat is incremented here. `SkillsPanel` sets its awaiting-answer
                // latch before emitting the request, then waits for a quality update. This
                // prevents spending again before the server's response.
                //
                // The sender re-reads the skill advancement class immediately before sending,
                // rather than trusting the panel's copy. It uses the same player qualities as
                // `HudView::skill_advancement`, so its gate and the footer cost agree.
                UiRequest::TrainSkill { skill, xp } => {
                    let sent = Self::player_desc(game, player_desc_received).is_some_and(|q| {
                        dereth_client_model::advancement::send_train_skill(q, &mut req, skill, xp)
                    });
                    if !sent {
                        self.stats.requests_refused += 1;
                    }
                }
                UiRequest::TrainSkillAdvancementClass { skill, credits } => {
                    let sent = Self::player_desc(game, player_desc_received).is_some_and(|q| {
                        dereth_client_model::advancement::send_train_skill_advancement_class(
                            q, &mut req, skill, credits,
                        )
                    });
                    if !sent {
                        self.stats.requests_refused += 1;
                    }
                }
                // ---- attribute raises: the other half of the same seam ----------------------
                //
                // Attribute selection, both footers and both raise buttons emit these requests;
                // this connects them to their senders.
                //
                // Unlike skill raising, attribute raising has no advancement-class gate: an
                // attribute cannot be untrained. The player-description lookup is the only
                // availability precondition; failure is counted as a refusal and sends nothing.
                //
                // `AttributeRow::wire_stat` already maps vital ids to `1/3/5`, not `2/4/6`,
                // before constructing the request. No local stat changes here; the panel owns
                // the awaiting-answer latch and the server response changes the value.
                UiRequest::TrainAttribute { attribute, xp } => {
                    match Self::player_desc(game, player_desc_received) {
                        Some(q) => {
                            dereth_client_model::advancement::send_train_attribute(
                                q, &mut req, attribute, xp,
                            );
                        }
                        None => self.stats.requests_refused += 1,
                    }
                }
                UiRequest::TrainAttribute2nd { vital, xp } => {
                    match Self::player_desc(game, player_desc_received) {
                        Some(q) => {
                            dereth_client_model::advancement::send_train_attribute_2nd(
                                q, &mut req, vital, xp,
                            );
                        }
                        None => self.stats.requests_refused += 1,
                    }
                }
                // The spellbook's filter update has already established that the mask
                // changed and has already written its own player-module copy; the server is
                // being told, not asked, so there is no gate on this one.
                UiRequest::SetSpellbookFilter { mask } => {
                    dereth_client_model::advancement::send_spellbook_filter(&mut req, mask);
                }
                // **The Titles tab reaches the shard.**
                //
                // The title panel's button arm sends the display-title request
                // with the selected title id and nothing else: no local
                // write, no gate, no reply waited for. The worn title is server state, and the
                // answer — `0x002B` with `set_as_display_title` set, or a fresh `0x0029` — is
                // what moves the header. A client that also wrote its own copy here would show
                // a title the shard had refused.
                UiRequest::SetDisplayCharacterTitle { title_id } => {
                    dereth_client_model::RequestSink::send(
                        &mut req,
                        dereth_client_model::Request::SetDisplayCharacterTitle(
                            dereth_protocol::social::SocialSetDisplayCharacterTitle { title_id },
                        ),
                    );
                }
                // **The spell bar reaches the shard.**
                //
                // Adding and removing a spell from the player module are one shape: write
                // the retained spell-favorite list and send, with no gate and no reply to wait for.
                // The panel has already decided the index (`add_favorite`); nothing is
                // recomputed here, because a second opinion about the index would be a second
                // implementation of the same arithmetic.
                //
                // The model write is what makes the row appear: `SpellcastingPanel::update`
                // rebuilds a tab out of `GameView::spell_tab`, which is `spell_tabs` below.
                UiRequest::AddSpellFavorite {
                    spell_id,
                    index,
                    tab,
                } => {
                    if game.player_system.add_spell_favorite(spell_id, index, tab) {
                        dereth_client_model::RequestSink::send(
                            &mut req,
                            dereth_client_model::Request::AddSpellFavorite(
                                dereth_protocol::combat::CharacterAddSpellFavorite {
                                    spell_id,
                                    index,
                                    spell_bank: i32::try_from(tab).unwrap_or(0),
                                },
                            ),
                        );
                        self.stats.spell_favorites_changed += 1;
                    } else {
                        self.stats.requests_refused += 1;
                    }
                }
                UiRequest::RemoveSpellFavorite { spell_id, tab } => {
                    // Spell-favorite removal is unconditional in the
                    // client — removal from a list that has no such node is a no-op —
                    // and the event goes out either way, because the caller
                    // (`remove_spell_from_menu`) has already established that the *row*
                    // exists. So the send is not gated on the model having found it.
                    game.player_system.remove_spell_favorite(spell_id, tab);
                    dereth_client_model::RequestSink::send(
                        &mut req,
                        dereth_client_model::Request::RemoveSpellFavorite(
                            dereth_protocol::combat::CharacterRemoveSpellFavorite {
                                spell_id,
                                spell_bank: i32::try_from(tab).unwrap_or(0),
                            },
                        ),
                    );
                    self.stats.spell_favorites_changed += 1;
                }
                // The spellbook's delete-confirmation callback. **Nothing local**: the
                // spell leaves the book when `0x01A8` comes back the other way, and a client that
                // dropped the row here would show a spell gone that the shard had refused to
                // remove.
                UiRequest::RemoveSpell { spell_id } => {
                    dereth_client_model::RequestSink::send(
                        &mut req,
                        dereth_client_model::Request::RemoveSpell(
                            dereth_protocol::qualities::MagicRemoveSpell {
                                layered_spell_id: spell_id,
                            },
                        ),
                    );
                    self.stats.spells_deleted += 1;
                }
                // A parent-container change stores the local pickup destination, so
                // `place_in_backpack` uses the pack the grid shows rather than always falling
                // back to the player.
                //
                // Opening a container does change that local
                // destination. This notice sends nothing; the server learns the selected
                // destination only from the subsequent `0x0019` or `0x0035`.
                UiRequest::NewParentContainer(id) => {
                    if game.on_new_parent_container(id) {
                        self.stats.open_containers_changed += 1;
                    }
                }
                // ---- vendor buttons ------------------------------------------------------
                //
                // The vendor button handler's eleven cases, arriving as the six
                // things they do. The two that **send** (`buy_single_item`, `sell_single_item`) put
                // their message in `req` and it goes out through `absorb` below, which is the
                // same wire slot every other request in this file uses.
                UiRequest::VendorBuySingle { item, split } => {
                    match game.buy_single_item(item, split, &mut req, &mut out, now) {
                        Ok(true) => self.stats.vendor_buys += 1,
                        Ok(false) | Err(_) => self.stats.requests_refused += 1,
                    }
                }
                UiRequest::VendorAddToBuyList { item, split } => {
                    if game.add_to_buy_list(item, split) {
                        self.stats.vendor_basket_rows += 1;
                    }
                }
                // Apply the vendor panel's stack-size decision to the
                // `PublicWeenieDesc` owned by the world.
                //
                // The Add to List handler (`0x100000C3`) uses the split-slider amount when
                // stack size is at least 2, otherwise 1. Without this write, a stackable stock
                // row adds exactly one item per press regardless of the slider.
                UiRequest::VendorSetObjectStackSize { item, size } => {
                    if game.set_object_stack_size(item, size) {
                        self.stats.vendor_stack_sizes_set += 1;
                    }
                }
                UiRequest::VendorBuyAll => match game.buy_all(&mut req, &mut out, now) {
                    Ok(true) => self.stats.vendor_buys += 1,
                    Ok(false) | Err(_) => self.stats.requests_refused += 1,
                },
                UiRequest::VendorSellSingle { item } => {
                    match game.sell_single_item(item, self.split, &mut req, &mut out, now) {
                        Ok(true) => self.stats.vendor_sells += 1,
                        Ok(false) | Err(_) => self.stats.requests_refused += 1,
                    }
                }
                UiRequest::VendorSellAll => {
                    if game.sell_all(&mut req, &mut out, now) {
                        self.stats.vendor_sells += 1;
                    } else {
                        self.stats.requests_refused += 1;
                    }
                }
                UiRequest::VendorClearList { sell, item } => match (sell, item) {
                    (true, Some(id)) => {
                        game.remove_from_sell_list(id);
                    }
                    (true, None) => {
                        game.flush_sell_list_sell_state();
                    }
                    (false, Some(id)) => game.shop.buy_list.retain(|(i, _)| *i != id),
                    (false, None) => game.shop.buy_list.clear(),
                },
                // Vendor-close case `0x100000D6` raises the native confirmation dialog when a
                // basket is not empty; the close arm below handles that path.
                // The vendor sell-drop handler's tail.
                //
                // The world's sell-list addition is the vendor-list add operation plus
                // the vendor item's sell-state mark, and it already carries the whole of
                // the drag-acceptability check: the owned-by-player refusal, the four
                // acceptability messages and the add-item contained-count guard.
                // Without this arm its only production caller would be the `VendorInfo` handler's
                // sell-mode arm — a shop opened *by* dropping an item on the vendor in the
                // world — and the window's own drop target would have no producer at all.
                //
                // Local only: nothing goes on the wire until "Sell Item" or "Sell All".
                UiRequest::VendorAddToSell { item } => {
                    if game.add_item_to_sell(item, &mut out) {
                        self.stats.vendor_basket_rows += 1;
                    } else {
                        self.stats.requests_refused += 1;
                    }
                }
                // **Retail asks, it does not announce.**
                //
                // Retail tests the buy list first and then the sell list. When both
                // are empty, it hides the window and returns. Otherwise, an existing dialog
                // context prevents a second prompt. With no dialog open, it sets property
                // `0x8E` to enum 1 and property `0xC5` to the unfinished-transactions prompt,
                // create a current-UI dialog with the close-vendor callback, and retain its context.
                //
                // Channel `0x1A` is **not** wrong in general: the two
                // `(0x1a, …)` calls in this very function
                // carry `L"You don't have enough money"` and `L"You must empty some slots in your
                // backpack first"` — both **buy** failures. A close with a full basket asks with
                // a dialog rather than printing a message at the top of the screen.
                UiRequest::VendorClose => match game.close_vendor_button(&mut out) {
                    Ok(true) => self.stats.vendor_closes += 1,
                    Ok(false) => {}
                    Err(text) => self.pending_vendor_close_confirmations.push(text),
                },
                // ---- secure trade --------------------------------------------------------
                //
                // The secure-trade panel's three buttons and drop target produce five request
                // forms. `dereth_client_model::trade` owns the rules: ownership, containment, anti-scam
                // comparison and the trade payload placed in `0x01FA`.
                //
                // **These requests are not exercised against a live shard.** A trade moves items
                // between characters irreversibly, so this arm is exercised from a loopback the
                // test process owns and the requests are asserted as bytes -- the same standard
                // held for buying and selling.
                UiRequest::TradeAddItem { item, position } => {
                    if game.trade_add_item(item, position, &mut out, &mut req) {
                        self.stats.trade_adds_sent += 1;
                    } else {
                        self.stats.requests_refused += 1;
                    }
                }
                UiRequest::TradeSplitItem { item, split, max } => {
                    if game.split_item_for_trade(
                        item,
                        SplitState {
                            split_size: split,
                            max_split_size: max,
                        },
                        now,
                        &mut out,
                        &mut req,
                    ) {
                        self.stats.trade_splits_sent += 1;
                    } else {
                        self.stats.requests_refused += 1;
                    }
                }
                UiRequest::TradeAccept {
                    displayed_self,
                    displayed_partner,
                } => match game.accept_trade(displayed_self, displayed_partner, &mut req) {
                    dereth_client_model::trade::AcceptDecision::Accept => {
                        self.stats.trade_accepts_sent += 1;
                    }
                    dereth_client_model::trade::AcceptDecision::OutOfSync => {
                        self.stats.trade_out_of_sync_sent += 1;
                    }
                },
                UiRequest::TradeDecline => {
                    game.decline_trade(&mut req);
                    self.stats.trade_control_sent += 1;
                }
                UiRequest::TradeReset => {
                    game.reset_trade_request(&mut req);
                    self.stats.trade_control_sent += 1;
                }
                // The close **button** (`0x1000008B`) only hides the window; the message goes out
                // because hiding raises the visibility-changed handler. One request either way.
                //
                // The two are kept apart: `close_trade_negotiations` writes `trade.open =
                // false`, so calling it directly here would take the window down on the
                // *range-exit* path and on `handle_close_trade`'s `0x01FF` too, and neither of
                // those hides anything in retail. The hide is
                // the button-message handler's hide call and belongs to
                // this gesture alone; `close_trade_window` is that call plus the
                // visibility-changed tail it raises.
                UiRequest::TradeClose => {
                    game.close_trade_window(now, &mut out, &mut req);
                    self.stats.trade_control_sent += 1;
                }
                // **The squelch wire's far end.** The chat target menu's squelch request and
                // `GamePlayScreen::chat_target_menu_item` above it produce `0x0058`; without this
                // arm every click on the chat menu's squelch row would fall into `other =>` below
                // and be logged as an unowned request.
                //
                // The field names differ across the seam and the difference is real: the panel's
                // `account` is the wire's `character_name`, which the client sends **empty** for a
                // per-character squelch (an account-wide one carries the account name and a zero
                // object id).
                UiRequest::ModifyCharacterSquelch {
                    object,
                    add,
                    account,
                    message_type,
                } => {
                    game.modify_character_squelch(&mut req, object, add, &account, message_type);
                    self.stats.squelch_requests += 1;
                }
                // `0x0059 Communication_ModifyAccountSquelch`. The original UI sends it only
                // from Squelch Account and Remove; this is that sending path.
                //
                // Counted on the same `squelch_requests` as `0x0058` because the two are one
                // gesture at two granularities and every reader of the stat wants the pair.
                UiRequest::ModifyAccountSquelch { add, name } => {
                    game.modify_account_squelch(&mut req, add, &name);
                    self.stats.squelch_requests += 1;
                }
                // The spell-component panel's
                // message `0x2F` arm — the set-desired-component-level event and the local mirror, in
                // that order. The bound is checked again here because
                // the player-module setter checks it too and this crate is
                // not the authority on it.
                UiRequest::SetDesiredComponentLevel { wcid, level } => {
                    let before = game.player_system.desired_comp_level(wcid);
                    if game.set_desired_component_level(&mut req, wcid, level) == level
                        && level != before
                    {
                        self.stats.desired_comp_sets += 1;
                    } else if !(0..dereth_client_model::magic::MAX_DESIRED_COMP_LEVEL)
                        .contains(&level)
                    {
                        self.stats.requests_refused += 1;
                    }
                }
                // Fill missing spell components — the one call site
                // of `shop_has_item` and `add_missing_comp`.
                UiRequest::VendorFillComponents {
                    category,
                    max_price,
                } => {
                    let vendor = game.shop.vendor_id;
                    let r = game.fill_component_list(category, max_price, &mut out);
                    self.stats.fill_components_rows += u64::try_from(r.added).unwrap_or(0);
                    self.stats.fill_components_missing +=
                        u64::try_from(r.not_stocked + r.short_stocked).unwrap_or(0);
                    if vendor.is_some() {
                        self.vendor_buying_tab_requested = vendor;
                    }
                }
                // **The wire that makes a player able to cast a spell.**
                //
                // The spellcasting panel is the client's **only** caller of
                // the spell-cast operation, which is the only caller
                // of `get_appropriate_spell_formula`.
                //
                // Nothing is decided here: the component check, the self-targeted branch, the
                // target compatibility test and the choice between `0x0048` and `0x004A` are all
                // decided by the spell-cast command. Its refusal string has **already** been
                // emitted as a `Notice::DisplayString` on channel `0x1A`, which `absorb` routes to
                // chat like every other refusal in this file. So the `Err` is only counted.
                UiRequest::CastSpell { spell_id } => {
                    let sent = req.0.len();
                    match game.cast_spell(&mut req, &mut out, spell_id) {
                        Ok(()) => {
                            // A spell the spell table does not know returns `Ok` and sends
                            // nothing, so "handled" is not "cast" and the two are counted apart.
                            if req.0.len() > sent {
                                self.stats.spells_cast += 1;
                                // **The free-hands-and-cast step's first statement.**
                                //
                                // Retail obtains the viewport's command interpreter
                                // and calls its free-hands operation. This tests `controlled_by_server`
                                // and, when control is local, tail-calls `stop_completely`, which
                                // stops the player completely. The spell-cast operation
                                // inlines the same call in its untargeted arm, so
                                // every cast the client actually sends runs it — and only a cast
                                // it sends, because each local refusal `return`s before
                                // the free-hands-and-cast step is reached.
                                //
                                // **It is here rather than in the game-side spell-cast operation**
                                // because the body is `Character`'s and `dereth-client-model` holds no
                                // physics object; `App::interaction_use_time` already drains this
                                // exact flag into `Character::stop_completely_from_action` for
                                // `EscapeKey`, which is the same call.
                                //
                                // The retail order is stop-then-send, and this is send-then-stop:
                                // the flag is not read until the frame's drain, which is after
                                // both, so no observer can tell. `req` is the outbound queue and
                                // is not flushed inside this arm either.
                                if !self.controlled_by_server {
                                    self.stop_completely_requested = true;
                                }
                            }
                        }
                        Err(_) => self.stats.requests_refused += 1,
                    }
                }
                // ---- the combat window's own three controls -----------------------------------
                //
                // The combat panel's message handler reaches exactly the two combat
                // functions the *keys* reach (`handle_combat_action`), which is why
                // there is one implementation here and not a parallel mouse path: the press sets
                // the height and starts the build, the click releases at `-1.0`.
                UiRequest::CombatSetAttackHeight { height } => {
                    let h = attack_height_from_raw(height);
                    if let Err(text) = game.set_requested_attack_height(h, ready, local_now) {
                        self.refuse(game, text);
                        self.stats.requests_refused += 1;
                    } else {
                        self.stats.attack_height_changes += 1;
                    }
                }
                UiRequest::CombatEndAttack { height } => {
                    let h = attack_height_from_raw(height);
                    let before = req.0.len();
                    game.end_attack_request(&mut req, h, None, ready, local_now);
                    if req.0.len() > before {
                        self.stats.attacks_released += 1;
                    }
                }
                // The UI-requested power is `position * 0.001` clamped to `[0, 1]`. Nothing goes
                // on the wire: the cap is read again when the attack request ends, which is where
                // it changes what the swing sends. The desired-attack-power-changed notice is the
                // window's own read-back and is applied by the frame's
                // `on_desired_attack_power_changed`.
                UiRequest::CombatSetDesiredPower { position } => {
                    game.combat.set_ui_requested_power_from_scrollbar(position);
                    self.stats.desired_power_changes += 1;
                }
                // The radar padlock's request reaches the same native setter as the
                // lock-UI command: bit 24 of the second options word, then broadcast option
                // `0x33`. The latter emits one immediate 0x0005 because LockUI is auto-save. Queue the
                // global-0D presentation half only after this authoritative write, matching the
                // native button's set_lock_ui-then-global-broadcast order.
                UiRequest::SetLockUi(locked) => {
                    self.apply_lock_ui_option(game, &mut req, locked, now);
                    self.pending_ui_layout_commands
                        .push(UiLayoutCommand::SetLockUi(locked));
                }
                // ---- the option wire's three ends ---------------------------------------------
                //
                // The option-change path applies `(option, current)` to the player module.
                // The page named one option
                // and its value; `PlayerSystem::set_option` is the option-change handler's whole
                // body — the two
                // fellowship mutual exclusions, the four engine side effects, and the
                // `is_auto_save_option` split between an immediate `0x0005` and the dirty flag.
                //
                // **The word is read-modify-written, never composed.** `Options::set` flips one
                // mask in the word `apply_player_module` took off `0x0013`, and
                // `mirror_options_into_module` copies both words back into the retained blob, so
                // every bit this build does not model survives — bit 25 of the second word included.
                UiRequest::SetPlayerOption(option, value) => {
                    let ordinal = crate::hud::option_ordinal(option);
                    let change = game.player_system.set_option(ordinal, value, now);
                    // Player-option change handling: exactly these five listening
                    // options recompute all six Turbine rows. Do not collapse the notices.
                    if change.moved() && matches!(ordinal, 35 | 36 | 37 | 38 | 46) {
                        let heritage = Self::player_desc(game, player_desc_received)
                            .map_or(0, |q| q.inq_int(0xBC));
                        game.enable_chat_talk_focuses(crate::chat::is_olthoi(heritage));
                        chat_focus(&mut game.chat);
                    }
                    // Player-option-changed event `0x0005`,
                    // one per option change whose option is one of the twenty-one
                    // `is_auto_save_option` ordinals.
                    //
                    // **Sent at once, not only at the next save.** Without this send the change
                    // would reach the shard only at the next options save or 480-second flush.
                    // That is not a cosmetic delay: the five listening options
                    // are **chat channel subscriptions**, and ACE's
                    // `GameActionSetSingleCharacterOption` answers `ListenToAllegianceChat`
                    // turning on with `Player.JoinTurbineChatChannel("Allegiance")` and a fresh
                    // `0x0295`. Without `0x0005` there is no way at all for a player to (re)join
                    // a Turbine chat room in a live session — which is what a monarch who has
                    // just gained a first vassal needs, because ACE's `SwearAllegiance` sends the
                    // tracker to the **vassal** only.
                    //
                    // **A loop, not one send.** A fellowship exclusion re-enters
                    // the option-change handler for the *other* option (the auto-accept setter
                    // tail-calls the change handler with option `0x12`), and that inner call
                    // sends its own `0x0005` before the outer one does. `change.sends` is that
                    // wire order; sending only the clicked option would leave the shard holding
                    // `AutomaticallyAcceptFellowshipRequests` under an unlit box.
                    for (ordinal, value) in &change.sends {
                        dereth_client_model::RequestSink::send(
                            &mut req,
                            Request::PlayerOptionChanged(
                                dereth_protocol::login::CharacterPlayerOptionChangedEvent {
                                    option: u32::try_from(*ordinal).unwrap_or(u32::MAX),
                                    value: u32::from(*value),
                                },
                            ),
                        );
                        self.stats.option_changes_sent += 1;
                    }
                    if change.deferred {
                        self.stats.option_changes_deferred += 1;
                    }
                    let effect = change.effect;
                    // The player-option handler's four engine effects — daylight,
                    // fog enablement, weather enablement and target tracking.
                    //
                    // **All four reach the scene, and none of them reaches it from here.**
                    // They are edge
                    // detectors in `App::frame` (`update_target_tracking`,
                    // `apply_player_option_effects`) rather than consumers of this edge, because
                    // this arm holds no scene and because a whole-module load (`0x0013` at login,
                    // `0x01A1` on a round trip) raises no option change at all — which is exactly
                    // why retail has player-module loading as a second
                    // producer running the same four calls.
                    //
                    // The counter therefore keeps counting and keeps its name: it is the number
                    // of **edges this arm declined**, not the number of effects nobody applied.
                    if effect.is_some() {
                        self.stats.option_side_effects_unapplied += 1;
                    }
                }
                // Saving the option page starts by sending `0x01A1`. The codec round-trips all
                // five recorded blobs byte-identically, including the stamp. `save_to_server`
                // re-packs the module the server sent — it is not
                // rebuilt from this crate's model of it.
                UiRequest::SavePlayerOptions => {
                    if game.player_system.save_to_server(&mut req, false) {
                        self.stats.player_modules_sent += 1;
                    }
                }
                // ---- the two vital queries a selection change sends ---------------------------
                //
                // Toolbar selection-change handling is the only producer of either
                // message in the client, and it reaches `dereth_client_model::World`'s
                // `query_health` / `query_item_mana` senders; without it the toolbar would draw a
                // health meter and ask nobody to fill it.
                UiRequest::QueryHealth(id) => {
                    game.query_health(&mut req, id);
                    self.stats.vital_queries += 1;
                }
                UiRequest::QueryItemMana(id) => {
                    game.query_item_mana(&mut req, id);
                    self.stats.vital_queries += 1;
                }
                // ---- the allegiance panel's update request -------------------------------------
                //
                // The display path from `0x0020` to the roster depends on this send: nothing
                // else produces `0x001F`, and the shard answers
                // this request, or pushes on change to an allegiance it has already been asked
                // about, and is otherwise silent. All 31 `0x001F` messages in the three
                // observed fellowship sessions are the retail client sending them.
                UiRequest::AllegianceUpdateRequest { on } => {
                    game.allegiance_update_request(&mut req, on);
                    self.stats.allegiance_update_requests += 1;
                }
                // ---- the allegiance panel's three buttons ---------------------------------
                //
                // The allegiance panel's message-1 handler raises a dialog
                // and stops; `0x001D` / `0x001E` leave from the dialog's close callback. So this
                // arm **queues a question**, it does not send. See `confirm_allegiance`.
                UiRequest::AllegianceConfirmation {
                    action,
                    target,
                    prompt,
                } => {
                    self.pending_allegiance_confirmations
                        .push((action, target, prompt));
                    self.stats.allegiance_confirmations_raised += 1;
                }
                // ---- the fellowship panel's seven request forms -------------------------------
                //
                // Without these the Fellowship tab could draw nothing and
                // ask for nothing. `0x00A6` below is the one that matters most: the shard sends
                // `0x02C0 Fellowship_UpdateFellow` -- 39 arrivals across three recorded
                // sessions -- only while a client has subscribed with it.
                UiRequest::FellowshipUpdateRequest { on } => {
                    game.fellowship_update_request(&mut req, on);
                    self.stats.fellowship_requests += 1;
                }
                UiRequest::FellowshipCreate { name, share_xp } => {
                    game.create_fellowship(&mut req, &name, share_xp);
                    self.stats.fellowship_requests += 1;
                }
                UiRequest::FellowshipQuit { disband } => {
                    game.fellowship_quit(&mut req, disband);
                    self.stats.fellowship_requests += 1;
                }
                UiRequest::FellowshipDismiss { target } => {
                    game.fellowship_dismiss(&mut req, target);
                    self.stats.fellowship_requests += 1;
                }
                UiRequest::FellowshipRecruit { target } => {
                    game.fellowship_recruit(&mut req, target);
                    self.stats.fellowship_requests += 1;
                }
                UiRequest::FellowshipAssignNewLeader { target } => {
                    game.fellowship_assign_new_leader(&mut req, target);
                    self.stats.fellowship_requests += 1;
                }
                // The one arm with local state behind it: `listen_to_element_message`
                // case `0x1000027D` flips the open-fellowship flag **before** it sends, so the button's
                // caption changes on the click rather than on the shard's answer.
                UiRequest::FellowshipToggleOpenness => {
                    if game.fellowship_toggle_openness(&mut req).is_some() {
                        self.stats.fellowship_requests += 1;
                    }
                }
                // ---- the friends panel's add/remove requests ----------------------
                //
                // The 100-friend cap belongs to the shared
                // add-friend operation: `@friends add` reaches it without visiting the panel.
                // Duplicating the refusal in UI and command paths would let them drift.
                UiRequest::AddFriend { name } => {
                    if game.add_friend(&mut req, &name).is_some() {
                        self.stats.friends_requests += 1;
                    } else {
                        self.stats.friends_list_full_refusals += 1;
                    }
                }
                UiRequest::RemoveFriend { target } => {
                    game.remove_friend(&mut req, target);
                    self.stats.friends_requests += 1;
                }
                // ---- the contracts panel's abandon request ------------------------
                //
                // The panel requires a selected row and nonzero contract
                // id. The shared operation repeats the nonzero-id guard because a command can
                // reach it without a panel selection.
                UiRequest::AbandonContract { contract_id } => {
                    if game.abandon_contract(&mut req, contract_id).is_some() {
                        self.stats.contract_requests += 1;
                    }
                }
                // ---- the salvage window's two requests ---------------------------
                //
                // `0x027D Inventory_CreateTinkeringTool`'s only caller in
                // retail is the salvage panel.
                UiRequest::SalvageItems { tool, items } => {
                    if game.create_tinkering_tool(&mut req, tool, &items).is_some() {
                        self.stats.salvage_requests += 1;
                    }
                }
                // `(channel, text)` from a panel. It goes
                // through the same `Scroll` entry point the notice sink's own `DisplayString` arm
                // uses, so a panel's line and a game-side refusal land in one ordered stream.
                UiRequest::DisplayChatText { channel, text } => {
                    game.scroll.on_display_string_info(channel, &text);
                    self.stats.panel_notice_strings += 1;
                }
                UiRequest::ChannelBroadcast { channel, text } => {
                    dereth_client_model::RequestSink::send(
                        &mut req,
                        Request::ChannelBroadcast(
                            dereth_protocol::comms::CommunicationChannelBroadcast {
                                channel,
                                message: text,
                            },
                        ),
                    );
                }
                // The urgent-assistance window's Send button.
                //
                // Retail returns without sending or advancing when the text box
                // is empty; otherwise it broadcasts the text on Help channel `0x400`.
                //
                // The channel broadcast `(channel, text)` is `0x0147
                // Communication_ChannelBroadcast`, client-to-server, `{ channel, message }` — no
                // sender name, which is [`dereth_protocol::comms::CommunicationChannelBroadcast`]'s
                // own correction to the community catalogue. This is the same request the `@f`
                // family builds in [`Self::chat_command`]'s channel-command arm, and it is
                // deliberately built **without** that arm's
                // `dereth_client_model::chat::channel_command_broadcasts_on` gate: the channel-command handler
                // refuses `0x400` by value, which is what
                // makes `@help` a help *command* rather than a channel, so the window is the only
                // producer of a Help-channel broadcast in the client and a gate written for the
                // chat commands would refuse the one caller that is allowed.
                //
                // Every guard is already the panel's: the empty-box test is
                // `UrgentAssistancePanel::send`'s, and the channel is retail's literal `0x400`.
                // There is nothing left for this arm to decide, so it decides nothing.
                // Housing payment's two sends and
                // the failed-house-transaction handler's retry. All three name the
                // slumlord and nothing else; the guards are the panel's, because the panel is the
                // only thing in retail that constructs any of them.
                UiRequest::HousePayment {
                    slumlord,
                    rent,
                    items,
                } => {
                    if game.house_payment(&mut req, slumlord, rent, &items) {
                        self.stats.house_payments_sent += 1;
                    }
                }
                UiRequest::HouseSplitItem { item, split, max } => {
                    if game.split_item_for_house(
                        item,
                        SplitState {
                            split_size: split,
                            max_split_size: max,
                        },
                        now,
                        &mut out,
                        &mut req,
                    ) {
                        self.stats.house_splits_sent += 1;
                    } else {
                        self.stats.requests_refused += 1;
                    }
                }
                UiRequest::HousePaymentConfirmation { rent } => {
                    self.pending_house_payment_confirmations.push(rent);
                }
                UiRequest::HouseQueryLord { slumlord } => {
                    if game.query_lord(&mut req, slumlord) {
                        self.stats.house_lord_queries += 1;
                    }
                }
                // ---- the chess window's three gestures ---------------------------
                //
                // The chess panel's two element-message paths and its resign-dialog answer
                // all use model-owned guards. Those guards read the joined game, current state
                // and board; illegal moves must be refused before reaching the wire.
                //
                // Drain text into the scroll here rather than inside the handlers so they can
                // operate with only a mutable borrow of `World.minigame`.
                UiRequest::MiniGameButton(id) => {
                    if game.minigame_button(id, &mut req) {
                        self.stats.minigame_gestures += 1;
                    }
                    self.stats.notice_strings_scrolled += game.drain_minigame_text() as u64;
                }
                UiRequest::MiniGameBoardPress(cell) => {
                    if game.minigame_board_press(cell, &mut req) {
                        self.stats.minigame_gestures += 1;
                    }
                    self.stats.notice_strings_scrolled += game.drain_minigame_text() as u64;
                }
                UiRequest::MiniGameQuitAnswer(confirmed) => {
                    game.minigame_quit_answer(confirmed, &mut req);
                    self.stats.minigame_gestures += 1;
                    self.stats.notice_strings_scrolled += game.drain_minigame_text() as u64;
                }
                other => {
                    unowned.push(other);
                    continue;
                }
            }
            self.stats.ui_requests_handled += 1;
        }
        self.absorb(game, out, req);
        unowned
    }

    /// A completed drag, from an item list or the paper doll onto somewhere.
    ///
    /// **Nothing moves here.** The panel ghosted the icon with `set_waiting_state(1)` and
    /// the item stays where it is until `Item_ServerSaysMoveItem` arrives.
    fn drag_drop(
        &mut self,
        item: ObjectId,
        target: DropTarget,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        out: &mut Notices,
        now: ServerTime,
    ) {
        let whole = self.split.is_whole_stack();
        let amount = self.split.split_size;
        let r = match target {
            // Toolbar backpack-button drop handling, exact catcher id `0x100001B1`.
            // The ownership fork and all five arguments are literal:
            //
            // * unowned -> `attempt_to_place_in_container(item, player id, 0, true, 0)`;
            // * owned   -> place in backpack `(item, false)`;
            // * false   -> `set_waiting_state(item, 0)`.
            //
            // The screen cannot make this decision because it deliberately has no object table.
            DropTarget::BackpackButton => {
                let accepted = if game.is_owned_by_player(item) {
                    game.place_in_backpack(req, out, item, false, self.split, now)
                } else if let Some(player) = game.player {
                    game.attempt_to_place_in_container(
                        req,
                        out,
                        item,
                        player,
                        ObjectId(0),
                        true,
                        0,
                        self.split,
                        now,
                    )
                } else {
                    false
                };
                if !accepted {
                    self.stats.requests_refused += 1;
                    game.set_waiting_state(item, false);
                }
                return;
            }
            // The viewport forwards drop message `0x15`. Handling reads the shared cursor
            // and arms a Drop search; the subsequent 3-D pick decides the target, so this
            // step does not send a request.
            DropTarget::World => {
                self.drop_item = item;
                self.reason = SearchReason::Drop;
                self.stats.picks_requested += 1;
                // **This arm arms the pick.** `handle_drop_release`
                // is four statements and the fourth is the one that matters:
                //
                // Read the live mouse coordinates, retain the dropped item id, set the search
                // reason to Drop, and call object search at those coordinates with the final
                // argument true. That last call is what arms the pick.
                //
                // Without it the arm would set a reason nothing would ever answer. `use_time`'s
                // step 3 is gated on `WorldPicker::looking_for_object`, so `place_in_3d` ->
                // `attempt_place_in_3d` -> `attempt_give` -> `0x00CD` would be unreachable by any
                // gesture, and — because `search_reason` is cleared **only** at the tail of
                // `on_world_object_found` — `SearchReason::Drop` (5) would then stay set and
                // outrank every `< SearchReason::Examine` / `< SearchReason::Use` gate in
                // [`Self::wrapper_mouse`], so the next left click, double-click and right release
                // in the viewport would do nothing either. One drop would disable the viewport for
                // the rest of the session.
                //
                // The increment above counts the request, not the arm, which is why the tests
                // assert `looking_for_object` and not the counter.
                //
                // The `false` leg — object search's unsigned viewport compare rejecting
                // the point — is the client's own, and the client does nothing about it either. It
                // is not reachable from here on the live tree: a `DropTarget::World` is produced
                // only by a release whose drop catcher is `<SBOX>`, and `<SBOX>`'s box **is** the
                // viewport (measured: `0,0..799,599` at 800x600). Counted rather than asserted, so
                // a layout that ever makes it reachable shows up as a number instead of a wedge.
                //
                // **That "`<SBOX>`'s box *is* the viewport" clause is literal rather
                // than incidental**: the rectangle here is `<SBOX>`'s own screen box, pushed in by
                // [`Self::note_game_viewport`], so a drop inside a *resized* smart box is measured
                // against the box it landed in. Measuring against the whole window would accept
                // the point and then aim the ray at the wrong place.
                let rect = self.device_viewport(self.screen);
                if !self.pick.find_object(self.cursor.0, self.cursor.1, rect) {
                    self.stats.drops_outside_the_viewport += 1;
                }
                return;
            }
            DropTarget::Container(c) => {
                if whole {
                    game.attempt_put_in_container(req, out, item, c, 0, now, false)
                } else {
                    game.attempt_split_to_container(req, out, item, c, 0, amount, now, false)
                }
            }
            // A paper-doll slot is a wield location, and which one is
            // the paper-doll slot-drop handler's to decide — see
            // [`Self::accept_drag_object`]. It answers `false` for a refusal, which
            // `handle_drop_release` turns into `set_waiting_state(0)`.
            DropTarget::EquipSlot(element) => {
                if !self.accept_drag_object(item, element, game, req, out, now) {
                    self.stats.requests_refused += 1;
                    // `weenie(id)->set_waiting_state(0)` — the icon un-ghosts and the
                    // item stays where the player picked it up. Nothing is on the wire.
                    game.set_waiting_state(item, false);
                }
                return;
            }
            // The toolbar-drop shortcut arm, for a drag that is a real
            // item rather than a shortcut alias: remove the shortcut in slot `n`, create one to
            // the item there, and move a different displaced object to the first empty slot to
            // the right of `n`, if there is one.
            //
            // **The wire half is not the 480-second options flush.** Shortcut addition's
            // notify-server flag guards two consecutive
            // calls — the add-shortcut event (`0x019C`) and then
            // the retained-module addition — and shortcut removal
            // pairs the remove-shortcut event (`0x019D`) with retained-module removal
            // the same way. The server is told at the moment of the drop; the
            // flush is for the *options*, which is a different section of the same blob.
            //
            // `create_shortcut_to_item`'s own gates, in its order: an unknown weenie
            // returns; the shortcut-eligibility check refuses with a message; an object that is
            // not the player's is picked up first, because this call came from a drag
            // (place in backpack `(item, false)`, and a refusal there ends it); and then
            // the shortcut removal `(item, true)` so that one object never occupies two slots.
            //
            // **Declared deviation.** The first two eligibility tests combine an unresolved
            // per-object predicate with description bit 4 and item-type bit `0x10`. Those
            // tests are not implemented here, so this path is more permissive for a
            // creature or `0x10`-typed object. The third test, whether the object belongs
            // to the open vendor, is implemented from `vendor_id`. The omitted tests can
            // allow extra drops; their unresolved meaning must not be mistaken for parity.
            DropTarget::ShortcutSlot(n) => {
                let Ok(slot) = usize::try_from(n) else {
                    self.stats.requests_refused += 1;
                    return;
                };
                // `handle_drop_release` never calls `set_waiting_state`, and there is no server reply
                // that would clear one: whatever ghosted the source icon must be undone here.
                if let Some(w) = game.tables.weenies.get_mut(item) {
                    w.waiting = false;
                }
                if !self.shortcut_drop(item, slot, game, req, out, now) {
                    self.stats.requests_refused += 1;
                }
                return;
            }
            // The toolbar-drop handler's *other* arm — reached once `0x21` on a shortcut tile
            // starts a drag:
            //
            // For a shortcut drop it removes whatever shortcut is in slot `n`, adds the dragged
            // item there, and — if a different object was displaced and the slot the drag came
            // from is available — puts the displaced object back in that slot, all with server
            // notification.
            //
            // A plain add, **not** a create-shortcut-to-item: the object was already in a slot a
            // moment ago, so its eligibility and ownership/backpack gates have already been
            // passed once and the client does not re-run them.
            //
            // Slot availability is three tests and nothing else — `n` is non-negative, below the
            // slot count, and its item list is empty —
            // i.e. "that slot number exists and its list is empty". The slot the drag left is
            // empty by construction (the pick-up removed it), *unless* the drop landed back on the
            // slot it came from, in which case the displaced id is the dragged item and the
            // displacement test does not run at all.
            DropTarget::ShortcutAlias { slot, from } => {
                let Ok(slot) = usize::try_from(slot) else {
                    self.stats.requests_refused += 1;
                    return;
                };
                let displaced = self.remove_shortcut_in_slot_num(slot, game, req);
                if !self.add_shortcut(item, slot, game, req, now) {
                    self.stats.requests_refused += 1;
                    return;
                }
                if let Some(old) = displaced.filter(|old| *old != item) {
                    // Is the slot the drag came from available?
                    let back = usize::try_from(from).ok().filter(|n| {
                        *n < dereth_client_model::player::SHORTCUT_SLOTS
                            && game.player_system.shortcut_at(*n).is_none()
                    });
                    if let Some(n) = back {
                        self.add_shortcut(old, n, game, req, now);
                    }
                }
                return;
            }
            // This unresolved item-list form names a list element, not its container
            // object. That mapping belongs to the inventory panel and is unavailable here.
            // Drop and count the request rather than guess a destination.
            DropTarget::ItemList { .. } => {
                self.stats.requests_refused += 1;
                return;
            }
            // The item-list slot-drop arm
            // decides *where in the pack* a dragged item lands. The screen has resolved the four
            // element-tree facts (the parent container id, the object under the pointer, the slot's
            // index and the list's item count); everything left needs the object table and is
            // `dereth_client_model`'s.
            //
            // `false` is `accept_drag_object`'s own refusal, and `handle_drop_release`
            // answers it with `set_waiting_state(0)` — the same un-ghost the paper-doll arm above
            // does, and the reason a refused drop does not leave the icon greyed for the rest of
            // the session.
            DropTarget::ItemListSlot {
                container,
                under,
                index,
                num_ui_items,
                dragged_is_container,
                container_list,
            } => {
                let drop = dereth_client_model::inventory::ItemListDrop {
                    parent_container: container,
                    under,
                    index,
                    num_ui_items,
                    dragged_is_container,
                    container_list,
                };
                // No `ui_requests_handled += 1` here: the `UiRequest::DragDrop` arm that called
                // this function falls into the loop's own shared increment, and counting it twice
                // would break `drag.rs`'s "interaction.rs consumed the drop" assertion.
                if !game.item_list_accept_drag(req, out, item, drop, self.split, now) {
                    self.stats.requests_refused += 1;
                    game.set_waiting_state(item, false);
                }
                return;
            }
        };
        if r.is_err() {
            self.stats.requests_refused += 1;
        }
    }

    /// Paper-doll slot-drop acceptance — nine lines, four decisions.
    ///
    /// It is not `attempt_wield` with the item's whole valid-locations mask for every one of the
    /// twenty-four slots: this is the live path for every equip a player attempts, and each of
    /// the four decisions is invisible from the screen when it is missing — the item
    /// equips, just not where or how the player aimed.
    ///
    /// Retail refuses an unknown object. It takes the object's valid locations and, when the slot
    /// is `0x200000` and the object may go at `0x100000`, also allows `0x200000` and sets the
    /// side to left. It refuses when the slot is not among the valid locations or there is no
    /// player system. A slot with none of the `0x080001FF` bits is an auto-wield on the side; any
    /// other is an auto-wear.
    ///
    /// The four, in the order the function makes them:
    ///
    /// 1. **The off-hand.** `*loc` and `*side` are the element-id location lookup's two
    ///    out-parameters, and `DropTarget::EquipSlot` carries only the element id, so the side
    ///    comes from the lookup; without it the left and right wrist, and the left and right
    ///    ring, could not be told apart. `dereth_client_model::inventory::slots::location_info_from_element_id`
    ///    answers both. The shield slot is the one place the *client* invents a side: a melee
    ///    weapon (`0x00100000`) released on the shield slot (`0x00200000`) is widened to pass the
    ///    gate and marked as the left side, which is what `auto_wield`'s own rewrite
    ///    then turns into an off-hand wield.
    /// 2. **The gate.** `(*loc & valid) == 0` refuses the drop, and the caller un-ghosts. Without
    ///    it a sword dropped on the head slot would be wielded in the hand: not refused, just
    ///    silently somewhere the player did not aim.
    /// 3. **The fork**, on the **slot's** mask and not the item's — `CLOTHING_LOC`, `0x080001FF`,
    ///    which is only the nine clothing bits and the cloak. So the shirt and pants slots wear
    ///    and everything else, armour included, wields. `auto_wear` sends the whole mask;
    ///    `auto_wield` sends the one bit its own order picks against the inventory mask.
    /// 4. **The split** stays in this module's split-size state because the wield attempt
    ///    reads that state rather than taking an argument. The auto-wear command also
    ///    honors the split size.
    ///
    /// Returns `accept_drag_object`'s own `bool`: `false` is a refusal, and `handle_drop_release`
    /// answers it with `set_waiting_state(0)`.
    fn accept_drag_object(
        &mut self,
        item: ObjectId,
        element: ElementId,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        out: &mut Notices,
        now: ServerTime,
    ) -> bool {
        use dereth_client_model::inventory::slots::{loc, location_info_from_element_id, SlotSide};

        // `handle_drop_release`'s *other* arm:
        //
        // A drop on a slot goes to the slot arm below; a drop on the figure element
        // `0x100001D6` goes to the paper-doll accept; a refusal either way clears the item's
        // waiting state.
        //
        // The element-to-location lookup has no case for `0x100001D6`, so the canvas is
        // exactly the `None` this `let-else` takes; answering `false` for it would make a drop on
        // the body do nothing.
        if element == DOLL_DRAG_MASK {
            return self.accept_paper_doll_drag_object(item, game, req, out, now);
        }
        let Some((mask, side)) = location_info_from_element_id(element.0) else {
            return false;
        };
        let Some(w) = game.weenie(item) else {
            return false;
        };
        let mut valid = w.pwd.valid_locations.unwrap_or(0);

        // 1. The off-hand, **before** the gate — this is what lets it pass.
        let side = if mask == loc::SHIELD && valid & loc::MELEE_WEAPON != 0 {
            valid |= loc::SHIELD;
            SlotSide::Left
        } else {
            side
        };

        // 2. The gate.
        if mask & valid == 0 {
            return false;
        }

        // 3. The fork.
        if mask & loc::CLOTHING == 0 {
            // Auto-wield with arguments `(id, side, 0, 1, 0, 0)` — and the fourth argument matters here.
            //
            // Every per-slot "You're already wearing …" message in `auto_wield` is guarded by
            // the fourth argument being zero, and this call site passes **1**,
            // so a doll drop onto an occupied slot shows **no** message. What it does instead is
            // the tail, which is gated on the same argument: record
            // the blocked id, blocked side and blocking id, bump the unblock attempt number, say
            // "Moving <blocker> to your backpack" and unwield the blocker, retrying the wield when
            // the server-says-move-item notice says it moved.
            //
            // That tail is built, so this is the whole function:
            // the world's auto-wield operation. Its last argument, 0, is the
            // player's aim — a ring dropped on the *left* ring slot does not wander to the right
            // one, where auto-sort and the retry (last argument 1) both may.
            let sent = game.auto_wield(req, out, item, side, false, true, false, self.split, now);
            if sent {
                // Auto-wield zeroes the unblock attempt number on entry and only its unblock tail
                // sets it, so a non-zero value here means what went on the wire was
                // the *blocker's* move. Reading it as "greater than before" miscounts a drop made
                // while an earlier unblock was still in flight, where both are 1.
                if game.unblock.unblock_attempt_num > 0 {
                    // The wire carries the *blocker's* `0x0019`, not this item's `0x001A`; the
                    // icon stays ghosted until the retry resolves it.
                    self.stats.unblocks_started += 1;
                } else {
                    self.stats.wields_requested += 1;
                }
            }
            sent
        } else {
            // `auto_wear(id, &worn, 0)` — the whole valid-locations mask, the server picks.
            let ok = game
                .auto_wear(req, out, item, self.split, now, false)
                .is_ok();
            if ok {
                self.stats.wears_requested += 1;
            }
            ok
        }
    }

    /// Paper-doll canvas acceptance — a drop on the
    /// **figure itself** rather than on one of its twenty-four slots. The hover hint promises
    /// it; this is the drop.
    ///
    /// Every branch was checked against retail. Unknown objects return false
    /// silently. A zero intersection of the valid locations and wearable mask `0x08007FFF`
    /// prints "You can't put that item there" unconditionally on channel `0x1A` and returns false.
    /// Failure of inventory readiness with argument 0, or an absent player system, also returns
    /// false. Otherwise initialize the worn output to zero and call auto-wear with the id read
    /// from the object and `quiet = 0`, returning its boolean result.
    ///
    /// Four readings, and three of them are things a "just wear it" implementation would get
    /// wrong:
    ///
    /// 1. **There is no slot and no side.** The whole point of the canvas arm is that the player
    ///    aimed at nothing in particular; `auto_wear` is just two calls —
    ///    `auto_wear_is_legal(id, &worn, quiet)` then, only if that passed and the object is
    ///    known, the wield request with the object's own valid locations; it returns the
    ///    legality answer —
    ///    and it hands the **entire** valid-locations mask over, letting the shard pick. That
    ///    is the world's attempt-wield request, which this build already had and which the *slot*
    ///    arm's clothing fork above already calls.
    /// 2. **`quiet = 0`**, the literal `push 0` at the drop call. The hover
    ///    (`InventoryPanels::on_paper_doll_canvas_message`) passes `1` and is
    ///    silent; the **drop** speaks. So `auto_wear_is_legal`'s two lines —
    ///    `"The %s is already being worn"` and `"You must remove your %s to wear
    ///    that"` — reach the player here and nowhere else on this path.
    /// 3. **The unwearable refusal is a different message from a different check.**
    ///    "You can't put that item there" is produced by the paper-doll handler
    ///    before auto-wear. Its earlier mask check makes the later auto-wear
    ///    wearable refusal unreachable on this path.
    /// 4. **False clears ghosting silently, without a second message.** The caller
    ///    clears waiting state for every refused drop, including this arm.
    ///
    /// The inventory-readiness test is not transcribed separately:
    /// auto-wear -> `auto_wear_is_legal` makes the same call first thing
    /// (`ready_for_inventory_request` is where this build keeps it), so the order
    /// and the message are already retail's. Likewise a missing player system, which the host
    /// never is.
    ///
    /// Returns `accept_paper_doll_drag_object`'s own `bool`.
    fn accept_paper_doll_drag_object(
        &mut self,
        item: ObjectId,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        out: &mut Notices,
        now: ServerTime,
    ) -> bool {
        use dereth_client_model::inventory::slots::loc;
        // An object the table does not know is refused in silence.
        let Some(w) = game.weenie(item) else {
            return false;
        };
        if w.pwd.valid_locations.unwrap_or(0) & loc::WEARABLE == 0 {
            // The literal is `panels/inventory`'s, transcribed from the native refusal.
            // `0x1A` is the feedback channel, as the literal the client
            // pushes rather than through the symbol that holds it.
            out.emit(dereth_client_model::Notice::DisplayString {
                channel: 0x1A,
                text: dereth_client_contract::panels::inventory::CANNOT_PUT_THAT_ITEM_THERE
                    .to_owned(),
            });
            self.stats.requests_refused += 1;
            return false;
        }
        // `auto_wear(id, &worn, 0)` — loud, whole mask, this module's splitter.
        let ok = game
            .auto_wear(req, out, item, self.split, now, false)
            .is_ok();
        if ok {
            self.stats.wears_requested += 1;
        }
        ok
    }

    /// One selection case of the player-action handler.
    /// The action dispatcher below returns events it did not consume; input dispatch gives the
    /// winning map's callback first refusal, leaving returned actions free for another listener.
    ///
    /// The sixteen arms below all reduce to this shape, and every one of them is written out at
    /// its own call site so that its four shipped arguments can be read against retail's
    /// `case`. What is shared is only the *mechanism*, which is identical in all twenty-six retail
    /// call sites:
    ///
    /// select-next with the arm's `(closer, ignore_current, kind, exclude_own_wielded)` and — if
    /// the selection did not move — select-next again with `(!closer, true, kind, false)`.
    ///
    /// **The retry is where the wrap-around lives**, and it is worth saying why it is not a
    /// no-op. The second call always passes `ignore_current = true`, which sends select-next
    /// down its arm: `closer` is **inverted again** and the reference is planted at the
    /// far sentinel. So `!closer` here becomes `closer` inside, with a reference of `0.0` or
    /// `73728.0` — the nearest or the farthest candidate in radar range, which is exactly the
    /// other end of the cycle. Pressing "next item" on the farthest item selects the nearest.
    ///
    /// **Ten arms wrap, not twelve.**
    /// Ten arms compare the saved and current selection, and
    /// the six arms that do not wrap are the "Closest" family: cases 4, 7, 10,
    /// 0xE, 0x13 and 0x43. They pass `wraps = false` because they already pass
    /// `ignore_current = true` on the first call and are therefore already at an end of the cycle.
    /// The arithmetic checks out against the other number in the same sentence: 6 arms x 1 call +
    /// 10 arms x 2 calls = **26** select-next calls in the player-action handler.
    ///
    /// `use_corpse` is `case 0x13`/`case 0x14`'s tail only:
    ///
    /// Look up the current selection after cycling. If that object exists and its virtual
    /// corpse predicate is true, invoke item use with `(selected id, 1, 0)`.
    ///
    /// Three details of that sequence are easy to get wrong:
    ///
    /// * It reads the selected id **after** the calls, not the id `select_next` returned — which is
    ///   nothing; `select_next` returns `void` and writes the global. So a press that selects
    ///   nothing still uses the *previous* selection if that happens to be a corpse.
    /// * The corpse predicate is virtual and has **zero direct call sites in retail**, so a
    ///   search for direct calls cannot see this line at all.
    /// * the second argument is 1, where every other gesture in this client passes 0. It is read **only**
    ///   inside the targeted-use arm; a corpse is
    ///   `is_useable` and not `is_useable_targeted`, so it takes the use-request branch above
    ///   and the two values agree here. The destination model passes 0 there and is
    ///   correct for this call — but its doc comment says that argument is 1 "only from the plugin
    ///   API and the spell-casting panel", and these two `case`s are a third producer.
    #[allow(clippy::too_many_arguments)]
    fn run_selection_cycle(
        &mut self,
        game: &mut dereth_client_model::World,
        closer: bool,
        ignore_current: bool,
        kind: dereth_client_model::selection::SelectionType,
        exclude_own_wielded: bool,
        wraps: bool,
        use_corpse: bool,
        phys: &crate::selection_geometry::SceneSelectionPhysics,
        radar_radius: f32,
        now: ServerTime,
    ) {
        let mut out = Notices::default();
        let mut req = RecordingRequests::default();
        // The retry compares against the selection captured before this arm runs.
        // Retail reads it at the selection-case dispatch, before that arm's selection calls.
        let before = game.selected;
        if phys.is_empty() {
            self.stats.selection_cycles_without_geometry += 1;
        }
        let lookup = |id| phys.get(id);
        game.select_next(
            closer,
            ignore_current,
            kind,
            exclude_own_wielded,
            &lookup,
            radar_radius,
            &mut out,
        );
        self.stats.selection_cycles += 1;
        if wraps && game.selected == before {
            game.select_next(!closer, true, kind, false, &lookup, radar_radius, &mut out);
            self.stats.selection_cycle_wraps += 1;
        }
        if use_corpse {
            if let Some(sel) = game.selected {
                if game
                    .weenie(sel)
                    .is_some_and(dereth_client_model::Weenie::is_corpse)
                {
                    self.use_object(sel, game, &mut req, &mut out, now);
                    self.stats.selection_corpse_uses += 1;
                }
            }
        }
        self.absorb(game, out, req);
    }

    #[allow(clippy::too_many_arguments)] // existing listener facts plus the actual finish-jump body owner
    fn on_actions(
        &mut self,
        events: Vec<crate::actions::Action>,
        game: &mut dereth_client_model::World,
        phys: &crate::selection_geometry::SceneSelectionPhysics,
        radar_radius: f32,
        ready: bool,
        now: dereth_primitives::LocalTime,
        character: Option<&crate::character::Character>,
    ) -> Vec<crate::actions::Action> {
        // The UI selection type, short because the sixteen arms below are a table and
        // the four arguments per row are the part that has to be readable.
        use dereth_client_model::selection::SelectionType as K;

        let mut left = Vec::new();
        let mut req = RecordingRequests::default();
        let srv = ServerTime(now.0);
        for e in events {
            // The combat action ladder, in its own order.
            // `0x1000005A` is answered whatever the mode is; everything else is offered
            // to exactly one of the two sub-handlers, chosen by the combat mode:
            //
            // `0x1000005A` toggles and answers true; otherwise melee and missile modes go to the
            // combat-action handler, magic mode to the magic-action handler, and every other mode
            // answers false.
            //
            // **In peace mode nothing below the toggle runs at all**, and that is not merely
            // belt-and-braces over the input-map registration: retail leaves the *previous*
            // mode's map registered for the frame in which the mode changes, and a plugin can
            // register any map it likes.
            //
            // **Test the positive control too.** A default `World` starts in
            // `NonCombat`, so testing one of the ten combat or twenty-one magic actions
            // without setting the mode merely exercises the false return. A test
            // drives the same
            // event in `NonCombat` and `Melee` and asserts both answers. New action tests
            // must set `combat.combat_mode`; refusal tests also need a reachable positive
            // case, or silence proves nothing.
            if e.id.0 == action::COMBAT_TOGGLE_COMBAT {
                if e.is_start() {
                    self.pending_combat_toggle = true;
                }
                continue;
            }
            let mode = game.combat.combat_mode;
            let handled = match mode {
                dereth_client_model::combat::CombatMode::Melee
                | dereth_client_model::combat::CombatMode::Missile => {
                    self.handle_combat_action(&e, game, &mut req, ready, now)
                }
                dereth_client_model::combat::CombatMode::Magic => self.handle_magic_action(&e),
                dereth_client_model::combat::CombatMode::NonCombat
                | dereth_client_model::combat::CombatMode::Undef => false,
            };
            if handled {
                continue;
            }
            match e.id.0 {
                // ---- no start-flag gate on USE and EXAMINE --------------------------------------
                //
                // The UI action handler owns both actions. Its dispatch subtracts the radar
                // action id, then 7 for USE, then 6 for EXAMINE. A literal-immediate search
                // for the USE id therefore misses the arm.
                //
                // **Neither arm reads the action's start flag.** The only arm of the handler that
                // does is the mouse-button one (7/8), which reads the press flag and then tests
                // for a target mode.
                //
                // Not gating them is therefore **fidelity**, not a deviation. It is
                // observable only on a `Hold`-family binding, which `fire_action_event`
                // releases on key-up. The shipped `ActionMap` binds neither id as a hold,
                // so the shipped keymap sees no difference and a rebound hold behaves as
                // retail's does.
                //
                // ---- examine is a TOGGLE ------------------------------------------------------
                //
                // The examination case first looks up the panel (`0x100005F7`). With no
                // manager it returns handled; with a visible panel it hides it and returns.
                // A missing or hidden panel reaches the examine request instead.
                //
                // So the examine key **shuts an open examine panel** and sends nothing; only a
                // shut panel reaches examination, and therefore only a shut panel produces
                // the `0x00C8 Item_Appraise` on the wire. Always taking the second leg would
                // leave the panel exactly as it was after pressing the key twice.
                //
                // **Three details that matter:**
                //
                // * The close calls the visibility setter, not another panel action.
                // * The visibility read selects **bit 1**, while bit 0 is the mouse-over-top flag.
                //   The right shift is necessary — reading bit 0 would
                //   have made the toggle fire on hover.
                // * The close-first runs **before** the selected id is read, so it fires with nothing
                //   selected at all. Ours does too.
                //
                // **No sibling in this switch has a close-first leg, and that holds over the
                // whole client rather than over this function.** The UI action handler has
                // eight live arms — five dispatched outcomes over the actions from 7, plus `0x7C`
                // and the three chained subtractions:
                //
                // | action | what it does |
                // |---|---|
                // | `0x07` / `0x08` `SelectLeft` / `SelectRight` | on press, mark an active target mode for exit, then forward to the UI manager |
                // | `0x27` `EscapeKey` | cancel targeting or interrupt movement/repeat attack |
                // | `0x55` `CaptureScreenshot` | capture a screenshot |
                // | `0x7B` `ToggleHelp` | send the help request `(0, 0x10000001)` |
                // | `0x7C` `TogglePluginManager` | close if open, otherwise open |
                // | `0x1000001E` `ToggleRadarPanel` | invert radar visibility and broadcast it |
                // | `0x10000025` `USE` | use the selected object, with no close-first step |
                // | `0x1000002B` `SelectionExamine` | this arm |
                //
                // Two of those are toggles by a *different* mechanism (`0x7C` through Keystone,
                // `0x1000001E` through a `bool` member and a notice) and neither goes near
                // element lookup. Element lookup happens **once** in the whole action handler,
                // and the visibility read there is the only one inside it.
                //
                // Object examination has **ten** call sites in ten distinct functions:
                // the paper doll, the examination panel's selection-change receiver,
                // the toolbar, the spellcasting panel, item lists, the viewport's object-found
                // receiver, the Decal plugin API, player actions, item target-mode execution,
                // and this arm. The other **nine** call it outright. So the toggle belongs to the
                // **key**, not to examining.
                //
                // All eight are transcribed: `0x07`/`0x08` is at the head of
                // [`Self::wrapper_mouse`] with the target-mode exit consumer in
                // [`Self::run_leave_target_mode`]; the other five are the arms below this one.
                // The `0x07`/`0x08` arm is not a *vendor* arm: the fields it reads are the target
                // mode, the leave-target-mode flag and the radar-visible flag. See
                // `wrapper_mouse`.
                action::SELECTION_EXAMINE => {
                    if self.examine_panel_open {
                        // Hide the examination panel. `App` performs it, for
                        // the same reason it answers the question: the tree is not reachable from
                        // here. Nothing else in this arm runs — no examination, no request.
                        self.examine_panel_close = true;
                        self.stats.examine_panel_closes += 1;
                    } else {
                        // `0x2B SELECTION_EXAMINE`. The neighboring action
                        // examines the selected id through the same
                        // entry point the cursor and the toolbar button use.
                        //
                        // **No gate on an empty selection**: retail loads the selected id
                        // *unconditionally* and examines whatever it
                        // holds, **including zero** — which is the arm that arms the magnifying
                        // glass. Skipping the call on an empty selection would make pressing `E`
                        // with nothing selected do nothing at all.
                        self.examine_object(game, &mut req, game.selected.unwrap_or_default());
                    }
                }
                // **The pick-up hotkey.**
                //
                // The default key map (DID `0x14000000`, offset `0x207`) binds `DIK_F` to
                // `SelectionPickUp 0x1000002C` in input map `0x10000007 ItemSelectionCommands`,
                // and that map **is** registered at the gameplay input priority (1000). Without
                // this arm the key would resolve, walk all three dispatch stages, match no arm, and
                // be swept into `InputStats::actions_expired`, with nothing on the wire.
                //
                // Retail's arm is case 1, transcribed on
                // `SELECTION_PICK_UP`: the selection gate is retail's own non-zero selected-id test,
                // and `force_main_pack` is its literal `false`, so the destination is the
                // player's preferred pack rather than forced to the main one. `place_in_backpack`
                // is the same production path the toolbar uses.
                //
                // The `break` on no selection is retail's too, and it is load-bearing: the action
                // is left **unconsumed** so a later stage may still take it, rather than being
                // swallowed into a no-op.
                action::SELECTION_PICK_UP => {
                    if let Some(sel) = game.selected.filter(|s| s.0 != 0) {
                        // `req` is this function's own sink, the one `on_actions` drains and
                        // sends. A fresh `RecordingRequests` here would take the `0x0019` and
                        // drop it on the floor: the arm would fire, the counter would climb, and
                        // nothing would reach the wire.
                        let mut out = Notices::default();
                        if game.place_in_backpack(
                            &mut req,
                            &mut out,
                            sel,
                            false,
                            // The selection-changed handler seeds this shared quantity before input.
                            // A fresh `SplitState::default()` is 0/0 and makes auto-merge encode
                            // amount 0, which ACE rejects as "Merge amount not valid!".
                            self.split,
                            ServerTime(now.0),
                        ) {
                            self.stats.pick_ups += 1;
                        }
                        self.absorb(game, out, RecordingRequests::default());
                    }
                }
                // Case 2. Unlike pick-up's
                // no-selection `break`, this arm always sends the current selected id (including
                // zero) and returns true. `App` owns the synchronous notice's toolbar receiver.
                action::SELECTION_SPLIT_STACK => {
                    self.pending_split_stack = Some(game.selected.unwrap_or_default());
                }
                action::USE => {
                    // The native UI use action calls even when the selected id is zero.
                    // Its zero arm enters generic Use rather than silently doing nothing.
                    let mut out = Notices::default();
                    self.use_object(
                        game.selected.unwrap_or_default(),
                        game,
                        &mut req,
                        &mut out,
                        ServerTime(now.0),
                    );
                    self.absorb(game, out, RecordingRequests::default());
                }

                // ---- the other six UI action arms ---
                //
                // The handler has five outcomes: mouse selection (`0x07`/`0x08`), Escape
                // (`0x27`), screenshot (`0x55`), help (`0x7B`), and `return false` for every other
                // action in its range. `0x7C` is handled separately.
                //
                // **The `SelectLeft`/`SelectRight` arm is not here**: `0x07`/`0x08` are the mouse
                // buttons, and this build routes those through `UiMouseEvent`, so it lives at the
                // head of [`Self::wrapper_mouse`] with its reading written out there.

                // The longest arm in the function, and — with `CaptureScreenshot` —
                // one of the two most likely to be reached in ordinary play. Without it
                // **nothing in the game screen would answer Escape at all**: it would fall through
                // to `_ => left.push(e)` below. Other Escape consumers are
                // pre-game `UiShell::mode_on_action`
                // map-9 handling, chat-entry deactivation, and the text element's
                // `lose_focus_on_escape` flag.
                //
                // Retail's ordering: a jump power strictly above zero finishes the jump and returns
                // true. Otherwise a focused UI element may consume Escape and return true. Then
                // test standing-still and repeat-attack state. If standing still with no repeat
                // attack, clear an active target mode first; otherwise mark a selected target as
                // willingly lost before clearing it, or toggle gameplay options `0x1000001B`
                // when nothing is selected. These paths return true. If moving or repeating an
                // attack, stop completely, print "Action interrupted" on `0x1A` only if moving,
                // recheck repeat-attack state and abort automatic attack if needed, then return false.
                //
                // Four details that decide behaviour:
                //
                // * **The first field is the target mode, not a vendor id**, so the first of the three
                //   cascade legs is *"cancel the use/examine cursor"*, which is what Escape does
                //   in play, rather than anything to do with a vendor.
                // * **The command interpreter's calls** are finish-jump, standing-still and
                //   stop-completely, on the interpreter owned by the viewport.
                // * **The saved movement flag is the *inverse* of the standing-still check**, so the
                //   *"Action interrupted"* line prints when the player was **not**
                //   standing still — and the whole stop-completely leg is taken when they were
                //   not standing still **or** a repeat attack is running.
                // * **The jump-power compare is `> 0.0`.**
                //   Retail takes the second leg on equality or less-than,
                //   so a power of exactly `0.0` is *not* a jump. The jump-power level floors at
                //   `MIN_JUMP_EXTENT` while a jump is pending, so the test is "is a jump pending".
                //
                // **The focus-element leg is structural here rather than reproduced.**
                // `interaction::use_time` is handed only the
                // actions the UI declined (`InputShell::take_events` after `UiShell::frame`), and
                // `dereth_ui`'s focused text element consumes `0x27` and relinquishes focus
                // itself. So an Escape that reaches this arm is by construction an Escape that no
                // focused element wanted — which is exactly the state the native focus gate tests
                // for, and a test asserts it.
                action::ESCAPE_KEY => {
                    self.escape_key(game, &mut req, now, character);
                }

                // The screenshot operation fills the path it is handed and
                // answers a `bool`; on `true` the arm formats `L"Screenshot saved to file '%hs'"`
                //  with it and adds it to the scroll as type `0x1A`, window 0. The arm
                // returns **TRUE either way**, so a failed screenshot still eats the key and
                // prints nothing — which is the one behaviour a player can observe about the
                // failure path.
                //
                // The device lives in `App`, so the request is recorded here and performed there,
                // the same seam as `0x1000002B`'s panel hide.
                action::CAPTURE_SCREENSHOT => {
                    self.screenshot_requested = true;
                    self.stats.screenshots_requested += 1;
                }

                // Open help page `0x10000001`, then return true. `0x10000001` is `ToggleCasPanel`'s
                // id used as a Keystone page selector, not an action raised here.
                //
                // **Keystone is third-party and outside this rebuild's scope**, so
                // what is transcribed is the arm: the key is consumed, the request is counted, and
                // nothing is drawn. That is a smaller effect than retail's and it is *declared*
                // — the alternative is leaving the id to fall through to `_ =>`.
                // `dereth_ui_screens`' `handle_key_press` still carries an empty
                // `action::OPEN_HELP => {}` arm that this one runs above.
                action::TOGGLE_HELP => {
                    self.stats.help_opens += 1;
                }

                // Ask whether the platform plugin manager is open, then close it or open it respectively;
                // return TRUE. **This is a toggle by a completely different mechanism from the
                // examine one** — it asks Keystone rather than the UI element manager, and there
                // is no element, no visibility bit and no visibility setter anywhere in it.
                //
                // Keystone is not rebuilt, so the *state* is held here; the *decision* — ask, then
                // take the opposite branch — is the transcription, and it is asserted over two
                // presses because one press cannot distinguish a toggle from a set.
                action::TOGGLE_PLUGIN_MANAGER => {
                    if self.plugin_manager_open {
                        self.plugin_manager_open = false;
                        self.stats.plugin_manager_closes += 1;
                    } else {
                        self.plugin_manager_open = true;
                        self.stats.plugin_manager_opens += 1;
                    }
                }

                // The third distinct toggle mechanism in the same function: retail
                // inverts the radar-visible flag, broadcasts its new value, and returns true.
                //
                // No element, no element lookup, no visibility bit: a `bool` member and a notice.
                //
                // **And the notice reaches nobody, in retail.**
                // The radar-visibility broadcaster invokes each subscriber's virtual handler.
                // Every implementation of that handler in retail is an empty stub.
                // The radar-visible flag has no known reader beyond startup initialization,
                // session reset and this toggle itself.
                //
                // **So the arm is transcribed as what it is — a flag flip and a notice with no
                // subscriber — and this build does not invent a radar it hides.** Hiding
                // `<RADA>` here would be a behaviour retail does not have.
                action::TOGGLE_RADAR_PANEL => {
                    self.radar_visible = StartsTrue(!self.radar_visible.0);
                    self.stats.radar_visibility_notices += 1;
                }

                // ---- the player-action handler's sixteen selection cases ----------
                //
                // **No `e.start` gate, deliberately.**
                // The player-action handler switches on the action id and never reads the start
                // flag; the combat-action handler does, which is its press/release split.
                // Every shipped binding for these sixteen is
                // `ToggleType::OneShot`, which emits only a `start: true` event, so the gate would
                // be inert on the shipped keymap; a rebound `Hold` would fire the cycle on press
                // *and* release, and that is retail's behaviour, not a defect to be papered over
                // here. (The two arms above do not gate on `e.start` either, for the reason
                // written at `SELECTION_EXAMINE`.)
                //
                // Each tuple is `(closer, ignore_current, kind, exclude_own_wielded)` and is the
                // one its `case` pushes; `wraps` is whether that `case` re-calls when the selected id
                // did not move, and `use_corpse` whether it ends in item use on a selected corpse.
                action::SELECTION_CLOSEST_COMPASS_ITEM => self.run_selection_cycle(
                    game,
                    true,
                    true,
                    K::CompassItem,
                    false,
                    false,
                    false,
                    phys,
                    radar_radius,
                    srv,
                ),
                action::SELECTION_PREVIOUS_COMPASS_ITEM => self.run_selection_cycle(
                    game,
                    true,
                    false,
                    K::CompassItem,
                    false,
                    true,
                    false,
                    phys,
                    radar_radius,
                    srv,
                ),
                action::SELECTION_NEXT_COMPASS_ITEM => self.run_selection_cycle(
                    game,
                    false,
                    false,
                    K::CompassItem,
                    false,
                    true,
                    false,
                    phys,
                    radar_radius,
                    srv,
                ),
                // The one call site of twenty-six that sets the exclude-own-wielded flag -- and
                // the one place it cannot matter: see the constant's doc.
                action::SELECTION_CLOSEST_ITEM => self.run_selection_cycle(
                    game,
                    true,
                    true,
                    K::Item,
                    true,
                    false,
                    false,
                    phys,
                    radar_radius,
                    srv,
                ),
                action::SELECTION_PREVIOUS_ITEM => self.run_selection_cycle(
                    game,
                    true,
                    false,
                    K::Item,
                    false,
                    true,
                    false,
                    phys,
                    radar_radius,
                    srv,
                ),
                action::SELECTION_NEXT_ITEM => self.run_selection_cycle(
                    game,
                    false,
                    false,
                    K::Item,
                    false,
                    true,
                    false,
                    phys,
                    radar_radius,
                    srv,
                ),
                action::SELECTION_CLOSEST_MONSTER => self.run_selection_cycle(
                    game,
                    true,
                    true,
                    K::Monster,
                    false,
                    false,
                    false,
                    phys,
                    radar_radius,
                    srv,
                ),
                action::SELECTION_PREVIOUS_MONSTER => self.run_selection_cycle(
                    game,
                    true,
                    false,
                    K::Monster,
                    false,
                    true,
                    false,
                    phys,
                    radar_radius,
                    srv,
                ),
                action::SELECTION_NEXT_MONSTER => self.run_selection_cycle(
                    game,
                    false,
                    false,
                    K::Monster,
                    false,
                    true,
                    false,
                    phys,
                    radar_radius,
                    srv,
                ),
                action::SELECTION_CLOSEST_PLAYER => self.run_selection_cycle(
                    game,
                    true,
                    true,
                    K::Player,
                    false,
                    false,
                    false,
                    phys,
                    radar_radius,
                    srv,
                ),
                action::SELECTION_PREVIOUS_PLAYER => self.run_selection_cycle(
                    game,
                    true,
                    false,
                    K::Player,
                    false,
                    true,
                    false,
                    phys,
                    radar_radius,
                    srv,
                ),
                action::SELECTION_NEXT_PLAYER => self.run_selection_cycle(
                    game,
                    false,
                    false,
                    K::Player,
                    false,
                    true,
                    false,
                    phys,
                    radar_radius,
                    srv,
                ),
                // `case 0x13` and `case 0x14` — select, then use the winner if it is a corpse.
                action::SELECTION_USE_CLOSEST_UNOPENED_CORPSE => self.run_selection_cycle(
                    game,
                    true,
                    true,
                    K::UnopenedCorpse,
                    false,
                    false,
                    true,
                    phys,
                    radar_radius,
                    srv,
                ),
                action::SELECTION_USE_NEXT_UNOPENED_CORPSE => self.run_selection_cycle(
                    game,
                    false,
                    false,
                    K::UnopenedCorpse,
                    false,
                    true,
                    true,
                    phys,
                    radar_radius,
                    srv,
                ),
                // `case 0x43` and `case 0x44` — the same two selections, and no use.
                action::SELECTION_CLOSEST_UNOPENED_CORPSE => self.run_selection_cycle(
                    game,
                    true,
                    true,
                    K::UnopenedCorpse,
                    false,
                    false,
                    false,
                    phys,
                    radar_radius,
                    srv,
                ),
                action::SELECTION_NEXT_UNOPENED_CORPSE => self.run_selection_cycle(
                    game,
                    false,
                    false,
                    K::UnopenedCorpse,
                    false,
                    true,
                    false,
                    phys,
                    radar_radius,
                    srv,
                ),
                // `case 0xD` — the seventeenth player-action
                // selection arm and the only one that is not a `select_next` cycle.
                //
                // The id is read **here as well as inside** `select_last_attacker`, through the
                // shared last-attacker state, because the geometry seam runs the other way from
                // the sixteen above: `select_next` is handed a lookup and asks it per candidate,
                // while the radar-range check is called with an id already in
                // hand and resolves it itself (object lookup, then
                // player-space conversion). `None` here is that function's own
                // null-object / null-cell escape, which returns **false**
                // — so an attacker with no physics object is never re-selected, the opposite of
                // the hearing check's escape. Only `x` and `y`: `z` is not in the sum.
                action::SELECTION_LAST_ATTACKER => {
                    let mut out = Notices::default();
                    let attacker = game.last_attacker();
                    let where_is_he = phys
                        .get(attacker)
                        .map(|p| (p.player_space.0, p.player_space.1));
                    game.select_last_attacker(where_is_he, radar_radius, &mut out);
                    self.stats.selection_last_attacker += 1;
                    self.absorb(game, out, RecordingRequests::default());
                }
                // ---- the player-action handler's two fellowship cases ------------
                //
                // The only two `Selection*` actions in `ItemSelectionCommands` that are not a
                // `select_next` cycle over the world: they walk the membership table and
                // never look at geometry, a radar radius or a selection kind.
                action::SELECTION_NEXT_FELLOW => self.select_fellow(game, true),
                action::SELECTION_PREVIOUS_FELLOW => self.select_fellow(game, false),
                // ---- `case 0x1000002E`, the shipped `P` -------------------------
                //
                // The native arm loads the previous selection, returns false if it is zero,
                // and otherwise assigns that id with `force = 0`.
                //
                // Selection history is modelled: selection assignment writes
                // `prev_selected = old` inside its own `old != id` guard, as retail does
                // — so "previous" means *the value
                // the selected id held immediately before the last change that actually changed it*,
                // a cleared selection included (retail stores the zero here and keeps the last
                // **non**-zero one in separate retained state, which
                // this arm does not read).
                //
                // Because selection assignment then records the outgoing id as the new previous,
                // pressing `P` twice returns to where it started: retail's `P` is a **toggle**
                // between the last two selections, not a stack. That falls out of the two
                // functions rather than being arranged here, and a test asserts it.
                // ---- input map 0x10, the system-key swallow ---------------------
                //
                // The native system-key callback returns true unconditionally — consume, do nothing, and
                // deny the action to the input-handler chain behind it. The decision about what
                // the *keys* do belongs to the platform input-message handler and is already transcribed in
                // the device input's system-key rule; see [`action::SYSTEM_ALT_TAB`] for the
                // four-way answer. This arm exists so that the four rows are handled and the
                // emptiness is a **verified decision** rather than a
                // `_ => {}` nobody has looked at.
                action::SYSTEM_ALT_TAB
                | action::SYSTEM_ALT_ENTER
                | action::SYSTEM_ALT_F4
                | action::SYSTEM_CTRL_SHIFT_ESC => {
                    self.stats.system_keys_swallowed += 1;
                }
                action::SELECTION_PREVIOUS_SELECTION => {
                    if let Some(prev) = game.prev_selected.filter(|id| id.0 != 0) {
                        let mut out = Notices::default();
                        game.set_selected_object(Some(prev), false, &mut out);
                        self.stats.selection_previous_restores += 1;
                        self.absorb(game, out, RecordingRequests::default());
                    }
                }
                // ---- the `PlayerOption_*` actions: each one flips its option -----------------
                //
                // A key bound to one of these reads the option and writes its opposite through
                // the same setter the Character Options page uses, so the change hook, the
                // fellowship exclusions and the save-at-once split all apply. Show-cloak and
                // lock-UI have action names but no arm, and fall through unhandled.
                id if player_option_action(id).is_some() => {
                    if let Some(ordinal) = player_option_action(id) {
                        self.toggle_player_option(game, &mut req, ordinal, srv);
                    }
                }
                _ => {
                    left.push(e);
                    continue;
                }
            }
        }
        self.absorb(game, Notices::default(), req);
        left
    }

    /// Select the next or previous fellowship member — the shipped `M` and `N`.
    /// Both walk the membership table and apply the answer with
    /// `force = false`. With no fellowship, neither changes the selection. An empty
    /// table leaves next-selection unchanged but clears previous-selection.
    ///
    /// So, in one sentence each: **next** is the member after the selected one, wrapping to the
    /// first, and the first when the selection is not a fellow; **previous** is the member before
    /// the selected one, wrapping to the last, and the last when the selection is not a fellow.
    /// **Self is included** — the fellow table holds every member, the player among them — and both
    /// directions can therefore land on the player's own object.
    ///
    /// # The one deviation, stated rather than hidden
    ///
    /// The original `PackableHashTable` of fellows iterates bucket `id % bucket_count`,
    /// then that bucket's chain, then the next nonempty bucket.
    /// This build keeps the fellowship in a
    /// `BTreeMap<ObjectId, Fellow>`, so the order is ascending object id. Reproducing the bucket
    /// order would mean reproducing the unpacked bucket count of a table this build never
    /// receives as a table, and it would put `M` in a *different* order from the fellowship panel,
    /// which iterates the same `BTreeMap` (the HUD's fellowship view). The cycle, the
    /// wrap and the two "not a fellow" fallbacks below are retail's; only which member is "next"
    /// within a fixed set differs, and it differs consistently with what the panel shows.
    fn select_fellow(&mut self, game: &mut dereth_client_model::World, next: bool) {
        let Some(f) = game.fellowship.as_ref() else {
            // No fellowship: both directions return without
            // touching the selection.
            return;
        };
        let ids: Vec<dereth_primitives::ObjectId> = f.members.keys().copied().collect();
        let selected = game.selected.filter(|id| id.0 != 0);
        // Test fellowship membership of the *current* selection, which both
        // functions make before they look for it in the walk.
        let at = selected.and_then(|id| ids.iter().position(|m| *m == id));

        let pick = if next {
            match at {
                // The successful next-member search and its fall-through share a fallback: the wrap and the
                // not-a-fellow case are the same statement in retail, and they are here too.
                Some(i) => ids.get(i + 1).or_else(|| ids.first()).copied(),
                None => ids.first().copied(),
            }
        } else {
            match at {
                // Previous-of-first has no preceding key; the walk continues to the end
                // and selects the LAST key, as it does when the selection is not a member.
                Some(0) | None => ids.last().copied(),
                Some(i) => ids.get(i - 1).copied(),
            }
        };

        match pick {
            Some(id) => {
                let mut out = Notices::default();
                game.set_selected_object(Some(id), false, &mut out);
                self.stats.selection_fellow_cycles += 1;
                self.absorb(game, out, RecordingRequests::default());
            }
            None if !next => {
                // The previous-member empty-table leg *clears* the selection,
                // where the next-member empty-table leg returns instead.
                // Faithful, and unreachable on a shard: a fellowship always holds its founder.
                let mut out = Notices::default();
                game.set_selected_object(None, false, &mut out);
                self.stats.selection_fellow_cycles += 1;
                self.absorb(game, out, RecordingRequests::default());
            }
            None => {}
        }
    }

    /// The UI action handler's `EscapeKey` arm.
    ///
    /// The first Escape leg is synchronous with the
    /// input event, before a following jump press/release. App can invoke only this complete
    /// early-return leg; all remaining Escape owners stay in `escape_key`. No start-bit guard
    /// is present in the primary. A focused UI consumer that already ate Escape is not here.
    pub fn try_finish_jump_from_escape(
        &mut self,
        game: &mut dereth_client_model::World,
        now: dereth_primitives::LocalTime,
        character: Option<&crate::character::Character>,
    ) -> bool {
        // Strictly greater than zero.
        if game.combat.jump_power_level(now) > 0.0 {
            crate::jump::finish(&mut game.combat, character);
            self.stats.escape_finish_jumps += 1;
            true
        } else {
            false
        }
    }

    /// The listing and the four details are at the `action::ESCAPE_KEY` call site; this is the
    /// body, in retail's own order. Every leg is `return TRUE` except the last two, which are
    /// `return FALSE` — and the difference is not cosmetic in retail, because a `FALSE` lets the
    /// action reach the next input handler. Here `on_actions` consumes the event either way,
    /// which is the same deviation every other arm in this `match` already carries; the counters
    /// below are what distinguishes the legs for a test.
    fn escape_key(
        &mut self,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        now: dereth_primitives::LocalTime,
        character: Option<&crate::character::Character>,
    ) {
        if self.try_finish_jump_from_escape(game, now, character) {
            return;
        }
        // The focused-element leg is structural here; see the call site.
        //
        // Standing still is true without a physics body; otherwise the motion interpreter
        // answers it.
        let standing_still = self.standing_still.0;
        // Repeat-attack state is asked **twice** in retail, once here and again
        // after `stop_completely`. Nothing between the two can change it in this build, so it is
        // read once; the second read is where `abort_automatic_attack` hangs.
        let repeating = game.repeat_attack_in_progress();
        if standing_still && !repeating {
            // Target mode takes precedence over selection and options.
            if self.target_mode != TargetMode::None {
                // Set the target mode to none. Straight to `None`
                // rather than through `leave_target_mode`: setting the target mode also
                // clears that flag directly.
                self.set_target_mode(TargetMode::None);
                self.stats.escape_target_mode_clears += 1;
            } else if game.selected.is_none_or(|id| id.0 == 0) {
                // Escape with nothing selected toggles the gameplay options panel. `App`
                // performs the UI operation; a missing manager consumes the action without
                // opening anything, corresponding here to a request with no host drain.
                self.visibility_toggle_requested = Some(action::TOGGLE_GAMEPLAY_OPTIONS_PANEL);
                self.stats.escape_options_toggles += 1;
            } else {
                // Mark the target as willingly lost. **This is the only writer of `true` in
                // the client**, and its reader is the selection-changed notice, which
                // clears it — so this store and the selection-change handler's clear-once are one
                // mechanism: Escape
                // says *"I let go of that target on purpose"*, and the very next selection change
                // is not answered by auto-targeting.
                //
                // Set **before** the selection is cleared, which is what makes the flag visible to the
                // notice that call raises.
                game.combat.target_willingly_lost = true;
                let mut out = Notices::default();
                // Clear the selection with `force = false`.
                game.set_selected_object(None, false, &mut out);
                self.stats.escape_deselects += 1;
                self.absorb(game, out, RecordingRequests::default());
            }
            return;
        }
        // Stop the interpreter completely, including its physics body. `App` owns the body.
        self.stop_completely_requested = true;
        self.stats.escape_stops += 1;
        // Print only when the player was **moving**, on refusal channel `0x1A`.
        if !standing_still {
            self.refuse(game, "Action interrupted");
            self.stats.escape_interrupts += 1;
        }
        // Recheck the cached repeat-attack state and abort any automatic attack.
        if repeating {
            game.abort_automatic_attack(req);
            self.stats.escape_attack_aborts += 1;
        }
    }

    /// Handle combat actions for **melee and missile at once**, which
    /// is the shape of the function and the shape of the feature.
    ///
    /// Retail shares the exact
    /// same keybinds to lower/increase the gauge that scales between SPEED and POWER (melee) or
    /// ACCURACY (missile), as well as the keybinds to initiate an attack with high, medium or low
    /// height: `0x1000005B` (`CombatDecreaseAttackPower`) and
    /// `0x100000EF` (`CombatDecreaseMissileAccuracy`) share a `case`, as do the increase pair and
    /// each of the three heights with its aim twin:
    ///
    /// Retail's press handling dispatches on one range of action suffixes, `0x5B..0xF3`.
    /// Release handling uses a second table, reads the currently requested attack height,
    /// and ends the attack request with that height and power argument `-1.0`.
    ///
    /// Two details, both invisible from the screen:
    ///
    /// 1. **The release uses the requested-attack-height field, not the released key's
    ///    height** — so releasing a *different*
    ///    height key than the one held swings at the height that is set, and does not silently
    ///    change it.
    /// 2. **The press goes through `set_requested_attack_height`**, whose guard is
    ///    `old != new || !attack_request_in_progress` — so a **held** key, which the repeat sweep
    ///    re-delivers every frame, runs the whole thing once and then nothing. What that saves is
    ///    `start_attack_request`'s target check and its `0x1A` refusal, once per frame; it is *not*
    ///    the power bar, which `attempt_start_building_attack` protects on its own. See
    ///    the world's attack-height setter.
    ///
    /// Returns `handle_combat_action`'s own `bool`, which is what the action callback returns and
    /// therefore
    /// what decides whether the event is offered to the next listener.
    fn handle_combat_action(
        &mut self,
        e: &crate::actions::Action,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        ready: bool,
        now: dereth_primitives::LocalTime,
    ) -> bool {
        // The release table: the same six actions, and only those six.
        if !e.is_start() {
            if !matches!(
                e.id.0,
                action::COMBAT_LOW_ATTACK
                    | action::COMBAT_MEDIUM_ATTACK
                    | action::COMBAT_HIGH_ATTACK
                    | action::COMBAT_AIM_LOW
                    | action::COMBAT_AIM_MEDIUM
                    | action::COMBAT_AIM_HIGH
            ) {
                return false;
            }
            // End the attack request at the requested attack height with power `-1.0`. `-1.0` is
            // `None` here: attack-request completion consults the bar and the cap only when the
            // power is `-1.0`.
            let height = game.combat.requested_attack_height;
            let before = req.0.len();
            game.end_attack_request(req, height, None, ready, now);
            if req.0.len() > before {
                self.stats.attacks_released += 1;
            }
            return true;
        }
        match e.id.0 {
            // The gauge. `CombatIncreaseAttackPower` and `CombatIncreaseMissileAccuracy` are the
            // increase arm (action ids `0x1000005C` / `0x100000F0`); the
            // other two step down. The desired-attack-power-changed notice is the
            // window's read-back and is applied by the frame.
            a @ (action::COMBAT_DECREASE_ATTACK_POWER
            | action::COMBAT_INCREASE_ATTACK_POWER
            | action::COMBAT_DECREASE_MISSILE_ACCURACY
            | action::COMBAT_INCREASE_MISSILE_ACCURACY) => {
                let increase = a == action::COMBAT_INCREASE_ATTACK_POWER
                    || a == action::COMBAT_INCREASE_MISSILE_ACCURACY;
                game.combat.adjust_ui_requested_power(increase);
                self.stats.desired_power_changes += 1;
                true
            }
            a @ (action::COMBAT_LOW_ATTACK
            | action::COMBAT_MEDIUM_ATTACK
            | action::COMBAT_HIGH_ATTACK
            | action::COMBAT_AIM_LOW
            | action::COMBAT_AIM_MEDIUM
            | action::COMBAT_AIM_HIGH) => {
                let height = match a {
                    action::COMBAT_LOW_ATTACK | action::COMBAT_AIM_LOW => AttackHeight::Low,
                    action::COMBAT_HIGH_ATTACK | action::COMBAT_AIM_HIGH => AttackHeight::High,
                    _ => AttackHeight::Medium,
                };
                // `set_requested_attack_height`, whose tail is `start_attack_request`.
                // The requested height lives in `dereth_client_model`'s own
                // `CombatState`, because the attack-done handler re-fires the
                // auto-repeat swing with it.
                match game.set_requested_attack_height(height, ready, now) {
                    Ok(()) => self.stats.attack_height_changes += 1,
                    Err(text) => {
                        self.refuse(game, text);
                        self.stats.requests_refused += 1;
                    }
                }
                true
            }
            _ => false,
        }
    }

    /// Handle the magic-combat keys, including the eighteen bound by default.
    ///
    /// A recognized press emits one spellcasting notice and returns true. Every
    /// release and unknown action returns false:
    ///
    /// | action | notice role |
    /// |---|---|
    /// | `0x10000060` `CombatCastCurrentSpell` | cast the current spell |
    /// | `0x10000061` / `0x10000062` | previous / next spell selection |
    /// | `0x10000063` / `0x10000064` | previous / next spell tab |
    /// | `0x10000065`…`0x10000070` | cast quickslot `action + 0xEFFFFF9B` |
    /// | `0x10000102` / `0x10000103` | first / last spell selection |
    /// | `0x10000104` / `0x10000105` | first / last spell tab |
    ///
    /// **The quickslot range is twelve wide and the shipped keymap binds nine of it.**
    /// The slot is `action - 0x10000065`, so `UseSpellSlot_1` is slot **0**; the cases run to
    /// `0x10000070`, i.e. slot 11, while `MagicCombat`'s defaults stop at `UseSpellSlot_9`. The
    /// three unbound ones are transcribed anyway because a player can bind them.
    ///
    /// The notice is emitted here through `dereth_client_contract::notices::NoticeInbox` and
    /// consumed by `SpellcastingPanel` during `RemainingPanels::update`. This queue
    /// crosses the input/UI boundary; producer and consumer counts remain separate
    /// so emitting a notice cannot masquerade as applying it.
    fn handle_magic_action(&mut self, e: &crate::actions::Action) -> bool {
        use dereth_client_contract::view::MagicNotice as N;
        if !e.is_start() {
            return false;
        }
        let n = match e.id.0 {
            action::COMBAT_CAST_CURRENT_SPELL => N::CastCurrentSpell,
            action::COMBAT_PREV_SPELL => N::PrevSpellSelection,
            action::COMBAT_NEXT_SPELL => N::NextSpellSelection,
            action::COMBAT_PREV_SPELL_TAB => N::PrevSpellTab,
            action::COMBAT_NEXT_SPELL_TAB => N::NextSpellTab,
            action::COMBAT_FIRST_SPELL => N::FirstSpellSelection,
            action::COMBAT_LAST_SPELL => N::LastSpellSelection,
            action::COMBAT_FIRST_SPELL_TAB => N::FirstSpellTab,
            action::COMBAT_LAST_SPELL_TAB => N::LastSpellTab,
            a if (action::USE_SPELL_SLOT_FIRST..=action::USE_SPELL_SLOT_LAST).contains(&a) => {
                // The slot is `action - 0x10000065`.
                N::CastQuickslotSpell {
                    slot: (a - action::USE_SPELL_SLOT_FIRST) as usize,
                }
            }
            _ => return false,
        };
        self.magic_notices.emit(n);
        self.stats.magic_actions += 1;
        true
    }

    /// **Local chat-command dispatch, used by the methods below.**
    ///
    /// `on_chat_command` resolves `@tell`/`@t`/`@send`/`@whisper`/`@w`,
    /// `@reply`/`@r`/`@rp` and `@retell`/`@rt` to their handler names — including the
    /// trailing-comma trim that turns `@tell bob,` into the verb `tell`, which is exactly the
    /// shape players type. This is the dispatch for that answer.
    ///
    /// **Why the three handlers are in `dereth_client_model::chat` and only this table is here.**
    /// They read and write `ChatState::last_teller` and `last_tellee_name`; the decision "which
    /// player does
    /// this line go to" is the model's, and this file's job is to turn the answer into a
    /// [`Request`] and a notice. This module wires.
    ///
    /// The refusals are the client's own four strings on chat type `0x1A`, raised as
    /// [`Notice::DisplayString`] because that is what the handler's scroll write of type `0x1A`
    /// to the current command source's window is.
    ///
    /// [`Interaction::absorb`] passes each `DisplayString` to
    /// [`dereth_client_model::scroll::Scroll`] for fan-out. Type `0x1A` appears
    /// in the strip across the viewport top because the main chat window's default
    /// filter clears bit 26.
    ///
    /// The window id is still lost on the way: `Notice::DisplayString` carries a channel and no
    /// command source, so the fan-out sends window `0` — a broadcast — where the
    /// client would have sent the id of the chat window the command was typed into. That is
    /// unobservable for `0x1A` (no window's default filter accepts it, so the id decides nothing)
    /// and would matter for a command that answered on a filtered-in type.
    /// The motion commands this frame's poses issued, for
    /// [`crate::app::App::apply_input_actions`] to hand to `MovementCommands`.
    pub fn take_pose_motions(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.pending_pose_motions)
    }

    /// Process the whole of what an ordinary spoken
    /// line does, which is **not** just the talk event.
    ///
    /// Walk the text, attempting pose extraction first between `*` delimiters and then
    /// between `<` and `>`. After processing all runs, trim whitespace on both sides.
    /// Send the remainder as speech (`0x0015`) only when it is nonempty.
    ///
    /// # Why this path exists
    ///
    /// `dereth_client_model::emotes::public_chat` and `emotes::pose` transcribe this
    /// — including the asymmetric cursor restore, the `%p` pronoun and the fact that
    /// the wire and the echo carry two *different* strings. Sending the line straight
    /// to `Request::Talk` instead would speak `*wave*` out loud, asterisks and all; no `0x01E1`
    /// could leave this client, no pose animation would play, and `Request::SoulEmote` would have
    /// no producer.
    ///
    /// # The three things one resolved run does, in the pose command's own order
    ///
    /// 1. the motion, **locally and immediately** — the one animation the client
    ///    predicts without waiting for the server. Queued for `App` rather than played here,
    ///    because the body lives on the scene;
    /// 2. soul-emote event `0x01E1`, carrying the
    ///    third-person form with `%p` replaced by the player's `Gender` pronoun;
    /// 3. the `Communication_HearSoulEmote` handler with sender 0, `"You"` and the pose's own
    ///    emote — the local echo,
    ///    with the **first**-person form. Sender id `0` short-circuits the hearing check, so it
    ///    can be neither squelched nor out of earshot, and the `^` the hear-soul-emote handler
    ///    appends makes it always understood; what reaches the scroll is therefore
    ///    `hear_emote_line("You", own_emote)` on type `0xC`, window 0.
    ///
    /// A run whose name misses `inq_chat_pose_command` is **silent and is left in the line**; a run
    /// that resolves but whose motion name misses `string2command` is broadcast and *also* left in
    /// the line, which is the pose command's return value being the `string2command` answer and nothing
    /// else. Both cases are `dereth_client_model::emotes`' and are not re-decided here.
    fn public_chat(
        &mut self,
        line: &str,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
    ) {
        // Read player integer quality `0x71` (Gender) once per line, as the original
        // path reads it once per nonempty alternate emote. Absence yields 0 rather
        // than 1, selecting the "her" branch.
        let gender = game
            .player_qualities()
            .or_else(|| game.login_player_desc())
            .map_or(0, |q| q.inq_int(crate::hud::GENDER));
        let table = self.chat_pose_table.clone();
        let out = dereth_client_model::emotes::public_chat(line, |name| {
            // The native early return on a null table makes every run a miss, not a
            // crash and not a pose that half-fires.
            let t = table.as_deref()?;
            dereth_client_model::emotes::pose(t, name, gender, |n| {
                dereth_animation::command::MotionCommand::from_name(n).map(|c| c.0)
            })
        });
        for p in &out.poses {
            self.stats.poses_resolved += 1;
            if let Some(cmd) = p.motion_command {
                self.pending_pose_motions.push(cmd);
            }
            if let Some(text) = &p.soul_emote {
                dereth_client_model::RequestSink::send(
                    req,
                    Request::SoulEmote(dereth_protocol::comms::CommunicationSoulEmote {
                        message: text.clone(),
                    }),
                );
                self.stats.soul_emotes_sent += 1;
            }
            if let Some(text) = &p.my_emote {
                let name = crate::chat::soul_emote_sender_name(
                    dereth_client_model::emotes::LOCAL_ECHO_NAME,
                );
                let (_, trimmed) = crate::chat::language_marker(&name);
                let line = crate::chat::hear_emote_line(trimmed, text);
                game.scroll.add_text_to_scroll(
                    &line,
                    dereth_client_model::chat::text_type::EMOTE,
                    true,
                    0,
                );
                self.stats.pose_echoes_printed += 1;
            }
        }
        if let Some(text) = out.speech {
            dereth_client_model::RequestSink::send(
                req,
                Request::Talk(dereth_protocol::comms::CommunicationTalk { message: text }),
            );
        }
    }

    /// What a [`dereth_client_model::chat_cmd::ChatCommand`] means on this side of the seam.
    ///
    /// Four kinds of effect, and the window each line goes to is the point of keeping them apart:
    ///
    /// * [`lines`](dereth_client_model::chat_cmd::ChatCommand::lines) are the handler's own
    ///   scroll write to the current command source's window — the chat window the command
    ///   was typed into;
    /// * [`failure_lines`](dereth_client_model::chat_cmd::ChatCommand::failure_lines) are
    ///   the failure-event handler's, and every one of its arms passes window **0**. Three of the four
    ///   reachable here are chat type `0` and `0x422`'s is `0x1A`, so the type travels with the
    ///   line rather than being a constant;
    /// * [`option`](dereth_client_model::chat_cmd::ChatCommand::option) changes a player option by
    ///   `ordinal`, which for an `is_auto_save_option` ordinal is a
    ///   `0x0005 Character_PlayerOptionChangedEvent` — the same path `@allegiance chat on` takes;
    /// * [`die_confirmation`](dereth_client_model::chat_cmd::ChatCommand::die_confirmation) is the one
    ///   chat command in retail that opens a dialog.
    ///
    /// `handled == false` is `do_command`'s failure event `0x26`. Only
    /// `@afk <unknown word>` can reach it in this family — every other refusal returns `true` and
    /// prints its own sentence.
    fn apply_chat_command(
        &mut self,
        cmd: dereth_client_model::chat_cmd::ChatCommand,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        out: &mut Notices,
        now: dereth_primitives::ServerTime,
    ) {
        let _ = out;
        for (text, ty) in &cmd.lines {
            game.scroll
                .add_text_to_scroll(text, *ty, true, self.chat.current_command_source);
            self.stats.chat_command_lines += 1;
            if *ty == dereth_client_model::chat_cmd::REFUSAL_CHAT_TYPE {
                self.stats.chat_commands_refused += 1;
            }
        }
        for (text, ty) in &cmd.failure_lines {
            game.scroll.add_text_to_scroll(text, *ty, true, 0);
            self.stats.chat_command_lines += 1;
            self.stats.chat_commands_refused += 1;
        }
        if let Some((ordinal, value)) = cmd.option {
            let change = game.player_system.set_option(ordinal, value, now);
            for (ordinal, value) in &change.sends {
                dereth_client_model::RequestSink::send(
                    req,
                    Request::PlayerOptionChanged(
                        dereth_protocol::login::CharacterPlayerOptionChangedEvent {
                            option: u32::try_from(*ordinal).unwrap_or(u32::MAX),
                            value: u32::from(*value),
                        },
                    ),
                );
                self.stats.option_changes_sent += 1;
            }
            if change.deferred {
                self.stats.option_changes_deferred += 1;
            }
        }
        if let Some(window) = cmd.clear_chat {
            self.pending_chat_clears.push(window);
            self.stats.chat_buffer_clears += 1;
        }
        if let Some(prompt) = cmd.die_confirmation {
            self.pending_die_confirmations.push(prompt);
            self.stats.die_confirmations_raised += 1;
        }
        if let Some(prompt) = cmd.house_abandon_confirmation {
            self.pending_house_abandon_first.push(prompt);
            self.stats.house_abandon_first_raised += 1;
        }
        self.stats.chat_command_requests += cmd.sent;
        if !cmd.handled {
            game.scroll.add_text_to_scroll(
                dereth_client_model::cmd::NOT_A_VALID_COMMAND,
                0x1A,
                true,
                self.chat.current_command_source,
            );
            self.stats.chat_commands_refused += 1;
        }
    }

    /// Flip one player option from a `PlayerOption_*` input action.
    ///
    /// The new value is the opposite of the current one, written through
    /// [`dereth_client_model::player::PlayerSystem::set_option`]; every `0x0005` that write puts
    /// on the wire is sent, and a moved chat-listening option re-enables the Turbine talk focuses
    /// exactly as a tick on the page does.
    fn toggle_player_option(
        &mut self,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        ordinal: usize,
        now: ServerTime,
    ) {
        let value = !game.player_system.options.get(ordinal);
        let change = game.player_system.set_option(ordinal, value, now);
        self.stats.option_actions_toggled += 1;
        if change.moved() && matches!(ordinal, 35 | 36 | 37 | 38 | 46) {
            let heritage = game
                .player_qualities()
                .or_else(|| game.login_player_desc())
                .map_or(0, |q| q.inq_int(0xBC));
            game.enable_chat_talk_focuses(crate::chat::is_olthoi(heritage));
        }
        for (ordinal, value) in &change.sends {
            dereth_client_model::RequestSink::send(
                req,
                Request::PlayerOptionChanged(
                    dereth_protocol::login::CharacterPlayerOptionChangedEvent {
                        option: u32::try_from(*ordinal).unwrap_or(u32::MAX),
                        value: u32::from(*value),
                    },
                ),
            );
            self.stats.option_changes_sent += 1;
        }
        if change.deferred {
            self.stats.option_changes_deferred += 1;
        }
    }

    /// Set the player's lock-UI option, including the virtual change-notification `(51)` tail.
    /// Both the radar and the lock-UI command enter here so they cannot disagree on the authoritative
    /// retained module, equality guard, or immediate `0x0005` wire shape.
    fn apply_lock_ui_option(
        &mut self,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        locked: bool,
        now: ServerTime,
    ) {
        let change = game.player_system.set_option(
            dereth_client_model::player::options::option::LOCK_UI,
            locked,
            now,
        );
        for (ordinal, value) in &change.sends {
            dereth_client_model::RequestSink::send(
                req,
                Request::PlayerOptionChanged(
                    dereth_protocol::login::CharacterPlayerOptionChangedEvent {
                        option: u32::try_from(*ordinal).unwrap_or(u32::MAX),
                        value: u32::from(*value),
                    },
                ),
            );
            self.stats.option_changes_sent += 1;
        }
        if change.deferred {
            self.stats.option_changes_deferred += 1;
        }
    }

    /// Finish one `on_chat_command` result. Physical chat input and `LoadFile` both enter
    /// this exact dispatcher; their only differences happen before it (history and file echo).
    /// It keeps the `UiRequest::ChatLine` routing intact: unknown-command Talk, `public_chat`,
    /// tell/channel/Turbine routing, and the local handlers retain the same request types,
    /// focus/window arguments, and failure path.
    #[allow(clippy::too_many_arguments)] // one parameter per input the call takes
    fn dispatch_chat_outcome(
        &mut self,
        outcome: dereth_client_model::cmd::CommandOutcome,
        window: u32,
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        out: &mut Notices,
        now: dereth_primitives::ServerTime,
        player_desc_received: bool,
        chat_focus: &mut dyn FnMut(&mut dereth_client_model::chat::ChatState),
    ) {
        use dereth_client_model::cmd::CommandOutcome as O;
        match outcome {
            O::ForwardVerbatim(line) => {
                dereth_client_model::RequestSink::send(
                    req,
                    Request::Talk(dereth_protocol::comms::CommunicationTalk { message: line }),
                );
            }
            // on_chat_command's Say arm is public_chat, including its `*pose*` extraction.
            O::Chat {
                destination: dereth_client_model::cmd::TalkFocus::Say,
                text,
            } => {
                self.public_chat(&text, game, req);
            }
            // Tell focus with no current speakable target is silently dropped, not public speech.
            O::Chat {
                destination: dereth_client_model::cmd::TalkFocus::Tell,
                text,
            } => {
                if let Some(target) = game.selected {
                    dereth_client_model::RequestSink::send(
                        req,
                        Request::TalkDirect(dereth_protocol::comms::CommunicationTalkDirect {
                            message: text,
                            target,
                        }),
                    );
                }
            }
            O::Chat { destination, text } if destination.channel_bit().is_some() => {
                let channel = destination.channel_bit().unwrap_or(0);
                dereth_client_model::RequestSink::send(
                    req,
                    Request::ChannelBroadcast(
                        dereth_protocol::comms::CommunicationChannelBroadcast {
                            channel,
                            message: text,
                        },
                    ),
                );
            }
            O::Handled {
                handler,
                verb,
                args,
            } => {
                self.chat_command(
                    handler,
                    &verb,
                    &args,
                    game,
                    req,
                    out,
                    now,
                    player_desc_received,
                    chat_focus,
                );
            }
            O::Failed(text) => {
                dereth_client_model::NoticeSink::emit(
                    out,
                    Notice::DisplayString {
                        channel: dereth_client_model::chat::REFUSAL_CHANNEL,
                        text: text.to_owned(),
                    },
                );
                self.stats.chat_commands_refused += 1;
            }
            O::Chat { destination, text } => {
                if let Some(focus) =
                    dereth_client_model::chat::TalkFocus::from_raw(destination as u32)
                {
                    game.send_turbine_chat(req, focus, false, &text, window, chat_real_time());
                }
            }
            O::Empty => {}
        }
    }

    #[allow(clippy::too_many_arguments)] // one parameter per input the call takes
    fn chat_command(
        &mut self,
        handler: &'static str,
        verb: &str,
        args: &[String],
        game: &mut dereth_client_model::World,
        req: &mut RecordingRequests,
        out: &mut Notices,
        now: dereth_primitives::ServerTime,
        player_desc_received: bool,
        chat_focus: &mut dyn FnMut(&mut dereth_client_model::chat::ChatState),
    ) {
        use dereth_client_model::chat::TellOutcome as T;
        // `do_command` hands the handler `argc`/`argv`; all three tell handlers begin by re-joining
        // them with `join_args`, which is why the original spacing inside a command is
        // lost and a forwarded line's is not.
        let joined = dereth_client_model::cmd::interp::join_args(args);
        // **The `title` handler.** Sources 1 and 8 are the two main-chat aliases;
        // only the four popup sources may set the chat window title. Retail's stored string
        // length includes its NUL and must be below `0x64`, so at most 98 user characters are
        // admitted.
        if handler == "title" {
            let source = self.chat.current_command_source;
            let refusal = if source == 1 || source == 8 {
                Some("This command must be issued from a popup chat window.")
            } else if joined.is_empty() {
                Some("You must provide a new title for the window.")
            } else if joined.chars().count() >= 99 {
                Some("Window title length cannot exceed 100 characters.")
            } else {
                None
            };
            if let Some(text) = refusal {
                game.scroll.add_text_to_scroll(
                    text,
                    dereth_client_model::chat::text_type::DEFAULT,
                    true,
                    source,
                );
                self.stats.chat_command_lines += 1;
                self.stats.chat_commands_refused += 1;
            } else {
                self.pending_chat_window_titles.push((source, joined));
            }
            return;
        }
        // **Version reporting.** The original prints its executable resource
        // version, not protocol or dat versions. This build uses its Cargo identity in
        // the same literal shape. The privileged tail checks the same player Boolean
        // qualities as the privilege predicate and sends bare-Control F7CC after both
        // local lines, never an ordered game action.
        if handler == "version" {
            let source = self.chat.current_command_source;
            if !args.is_empty() {
                game.scroll.add_text_to_scroll(
                    "Unexpected arguments to @version",
                    dereth_client_model::chat::text_type::LOCAL_ERROR,
                    true,
                    source,
                );
                self.stats.chat_command_lines += 1;
                self.stats.chat_commands_refused += 1;
                return;
            }
            if game.chat.using_turbine_chat {
                game.scroll.add_text_to_scroll(
                    "Using Turbine Chat.\n",
                    dereth_client_model::chat::text_type::DEFAULT,
                    true,
                    source,
                );
                self.stats.chat_command_lines += 1;
            }
            game.scroll.add_text_to_scroll(
                &format!("Client version {CLIENT_BUILD_ID}\n"),
                dereth_client_model::chat::text_type::DEFAULT,
                true,
                source,
            );
            self.stats.chat_command_lines += 1;
            let psr = game
                .player_qualities()
                .or_else(|| game.login_player_desc())
                .is_some_and(|q| [0x2C, 0x2D, 0x61].into_iter().any(|p| q.inq_bool(p)));
            if psr {
                dereth_client_model::RequestSink::send(
                    req,
                    Request::AdminGetServerVersion(
                        dereth_protocol::admin::AdminSendAdminGetServerVersion,
                    ),
                );
                self.stats.chat_command_requests += 1;
            }
            return;
        }
        // **`@messagetypes`.** The handler never reads argc/argv. It
        // obtains this process-local list, writes timestamped type zero to the current command
        // source, and returns true without a request; even extra arguments therefore print it.
        if handler == "messagetypes" {
            let line = message_types_text();
            game.scroll.add_text_to_scroll(
                &line,
                dereth_client_model::chat::text_type::DEFAULT,
                true,
                self.chat.current_command_source,
            );
            self.stats.chat_command_lines += 1;
            return;
        }
        // **The `day` handler.** Retail does not inspect argc/argv: it toggles daylight,
        // prints the resulting sentence, then writes that same value
        // through the player-option setter. The retained option model is the source App's
        // ordinary `apply_player_option_effects` latch reads, so one write reaches both persistence
        // and the real landscape without a parallel lighting flag here. Ordinal 5 is deliberately
        // absent from `is_auto_save_option`; this dirties the module for its normal bulk flush and
        // sends no immediate `0x0005`.
        if handler == "day" {
            let ordinal = dereth_client_model::player::options::option::PERSISTENT_AT_DAY;
            let day = !game.player_system.options.get(ordinal);
            let change = game.player_system.set_option(ordinal, day, now);
            for (ordinal, value) in &change.sends {
                dereth_client_model::RequestSink::send(
                    req,
                    Request::PlayerOptionChanged(
                        dereth_protocol::login::CharacterPlayerOptionChangedEvent {
                            option: u32::try_from(*ordinal).unwrap_or(u32::MAX),
                            value: u32::from(*value),
                        },
                    ),
                );
                self.stats.option_changes_sent += 1;
            }
            if change.deferred {
                self.stats.option_changes_deferred += 1;
            }
            if change.effect.is_some() {
                self.stats.option_side_effects_unapplied += 1;
            }
            let line = if day {
                "Let there be light!"
            } else {
                "Normality has been restored."
            };
            game.scroll
                .add_text_to_scroll(line, 0x1A, true, self.chat.current_command_source);
            self.stats.chat_command_lines += 1;
            return;
        }
        // **`@render`.** Retail forwards argc/argv to
        // render-option handling, which reads only the option and value,
        // uses decimal `atoi`, and returns up to two strings. First output is type `0x1A`; usage is
        // type zero. A false usage return then reaches `do_command`'s generic failure `0x26`, while
        // an unknown option is a silent true.
        if handler == "render" {
            const USAGE: &str = "Usage:\n@render <option> <value>\n  radius #        : set landscape radius (between 5 and 25)\n  fov #           : set field of view (between 10 and 160)\n";
            let source = self.chat.current_command_source;
            let Some(option) = args.first() else {
                game.scroll.add_text_to_scroll(USAGE, 0, true, source);
                game.scroll.add_text_to_scroll(
                    dereth_client_model::cmd::NOT_A_VALID_COMMAND,
                    0x1A,
                    true,
                    0,
                );
                self.stats.chat_command_lines += 2;
                self.stats.chat_commands_refused += 1;
                return;
            };
            if option.eq_ignore_ascii_case("usage") {
                game.scroll.add_text_to_scroll(USAGE, 0, true, source);
                game.scroll.add_text_to_scroll(
                    dereth_client_model::cmd::NOT_A_VALID_COMMAND,
                    0x1A,
                    true,
                    0,
                );
                self.stats.chat_command_lines += 2;
                self.stats.chat_commands_refused += 1;
                return;
            }
            let output = if option.eq_ignore_ascii_case("radius") {
                match args.get(1) {
                    None => Some("Must specify a radius"),
                    Some(value) => {
                        let radius = render_atoi(value) as u32;
                        if (5..=25).contains(&radius) {
                            self.pending_render_preferences.push((
                                "Render.LandscapeDrawDistance",
                                dereth_client_contract::view::PrefValue::Int(radius as i32),
                            ));
                            Some("Landscape radius set")
                        } else {
                            Some("Radius must be between 5 and 25")
                        }
                    }
                }
            } else if option.eq_ignore_ascii_case("fov") {
                match args.get(1) {
                    None => Some("Must specify a field of view"),
                    Some(value) => {
                        let fov = render_atoi(value) as u32;
                        if (10..=160).contains(&fov) {
                            self.pending_render_preferences.push((
                                "Render.FieldOfView",
                                dereth_client_contract::view::PrefValue::Float(fov as f32),
                            ));
                            Some("Field of view set")
                        } else {
                            Some("Field of view must be between 10 and 160")
                        }
                    }
                }
            } else {
                None
            };
            if let Some(output) = output {
                game.scroll.add_text_to_scroll(output, 0x1A, true, source);
                self.stats.chat_command_lines += 1;
            }
            return;
        }
        // **`@framerate`.** This is a process-local flag and a UI notice,
        // never a game action. The receiver needs the current render globals and live WorldObjects,
        // so App observes this flag after the ordinary command dispatch and performs the exact
        // show/update/hide edge there. Arguments are handled (TRUE) but do not change the flag.
        if handler == "framerate" {
            if args.is_empty() {
                self.framerate_display = !self.framerate_display;
                self.pending_framerate_display.push(self.framerate_display);
            } else {
                game.scroll.add_text_to_scroll(
                    "Unexpected arguments to @framerate",
                    0x1A,
                    true,
                    self.chat.current_command_source,
                );
                self.stats.chat_commands_refused += 1;
            }
            return;
        }
        // **`@loadfile`.** The joined name is
        // passed to `fopen("rt")` exactly as typed: unlike `@log`, there is no default extension.
        // Every `fgets(0x400)` chunk is substituted, displayed, then synchronously re-enters the
        // same `on_chat_command` outcome dispatcher as a physical chat line. File commands do not
        // enter typed-command history.
        if handler == "loadfile" {
            let source = self.chat.current_command_source;
            if joined.is_empty() {
                game.scroll.add_text_to_scroll(
                    "You must provide a file name.",
                    dereth_client_model::chat::text_type::LOCAL_ERROR,
                    true,
                    source,
                );
                self.stats.chat_command_lines += 1;
                self.stats.chat_commands_refused += 1;
                return;
            }

            let path = std::path::Path::new(&joined);
            let mut file = match std::fs::File::open(path) {
                Ok(file) => file,
                Err(_) => {
                    game.scroll.add_text_to_scroll(
                        &format!("Cannot open file {joined}"),
                        dereth_client_model::chat::text_type::LOCAL_ERROR,
                        true,
                        source,
                    );
                    self.stats.chat_command_lines += 1;
                    self.stats.chat_commands_refused += 1;
                    return;
                }
            };
            let mut bytes = Vec::new();
            // A read error makes native `fgets` stop and close the stream; opening, not a later
            // read failure, is the only path that prints "Cannot open file".
            let _ = std::io::Read::read_to_end(&mut file, &mut bytes);
            for chunk in dereth_client_model::cmd::loadfile::text_mode_chunks(&bytes) {
                let real_time = i64::from(chat_real_time());
                let date = dereth_client_model::cmd::loadfile::format_date(
                    real_time,
                    crate::platform::clock::local_utc_offset_secs(real_time),
                );
                // The original file loader treats each `fgets` result as a C string. Bytes after
                // the first NUL were consumed in that fetched chunk but do not reach
                // substitution, display or command parsing. The next fetched chunk still
                // executes normally.
                let native_len = chunk
                    .iter()
                    .position(|byte| *byte == 0)
                    .unwrap_or(chunk.len());
                let narrow_line = dereth_protocol::cp1252::decode(&chunk[..native_len]);
                let line = dereth_client_model::cmd::loadfile::make_variable_substitutions(
                    &narrow_line,
                    &date,
                );
                game.scroll.add_text_to_scroll(&line, 0, true, source);
                self.stats.chat_command_lines += 1;
                let focus =
                    dereth_client_model::cmd::TalkFocus::from_raw(game.chat.talk_focus as u32)
                        .unwrap_or(dereth_client_model::cmd::TalkFocus::Say);
                let outcome = self.chat.on_chat_command(&line, source, focus);
                self.dispatch_chat_outcome(
                    outcome,
                    source,
                    game,
                    req,
                    out,
                    now,
                    player_desc_received,
                    chat_focus,
                );
                self.stats.chat_lines_sent += 1;
            }
            return;
        }
        // **`@log`.** No arguments closes the native process-wide
        // append handle. A name gets `.txt` only when it has no extension, and replacement closes
        // the old handle before attempting the new open (`start_copy_output_to_file`).
        // This command does not apply the load-file variable substitutions: `%DATE%` is literal.
        if handler == "log" {
            let source = self.chat.current_command_source;
            if args.is_empty() {
                let line = if game.scroll.close_log_file(source) {
                    "Chat output now directed only to the screen."
                } else {
                    "Please specify a file to append chat messages to."
                };
                game.scroll.add_text_to_scroll(line, 0, true, source);
                self.stats.chat_command_lines += 1;
                return;
            }
            let mut path = std::path::PathBuf::from(&joined);
            if path
                .extension()
                .is_none_or(|extension| extension.is_empty())
            {
                path = std::path::PathBuf::from(format!("{joined}.txt"));
            }
            let name = path.to_string_lossy();
            // The `fopen` is the host's: the model takes a callback and runs
            // it where `start_copy_output_to_file` runs it, after the close.
            let line = if game.scroll.start_copy_output_to_file(&name, source, || {
                crate::platform::text::open_chat_log(&path)
            }) {
                format!(
                    "Copying chat to {name}.  Run command again with no arguments to turn off logging."
                )
            } else {
                self.stats.chat_commands_refused += 1;
                format!("Failed to redirect to file {name}!")
            };
            game.scroll.add_text_to_scroll(&line, 0, true, source);
            self.stats.chat_command_lines += 1;
            return;
        }
        if handler == "emote" {
            // The emote command silently accepts empty joined arguments. Nonempty text sends
            // the emote event, without public-speech/pose processing or a local echo.
            if let dereth_client_model::chat::EmoteOutcome::Send(message) =
                dereth_client_model::chat::do_emote(&joined)
            {
                dereth_client_model::RequestSink::send(
                    req,
                    Request::Emote(dereth_protocol::comms::CommunicationEmote { message }),
                );
            }
            return;
        }
        // The channel-command wrapper delegates nonempty arguments to channel dispatch.
        // Nineteen command names share this one
        // handler — `@a @c @covassal @covassals @co-vassals @f @fellow @fellows @fellowship @g
        // @group @party @m @monarch @p @patron @v @vassal @vassals` — and without this arm every one
        // of them would reach the catch-all below and print *"That is not a valid command."*, while
        // the very same `0x0147` composed from the talk-focus dropdown goes out fine.
        //
        // Retail dispatches to the channel command only when `argc > 0`. Otherwise
        // it prints the missing-text message on chat type `0x1A` in the current command-source
        // window, with logging enabled, and return true.
        //
        // **The `true` is the load-bearing byte.** `do_command` only raises
        // failure event `0x26` — *"That is not a valid command."* — when a handler returns
        // `false`, so an empty `@f` says *"You must specify the text you wish to broadcast!"* and
        // nothing else. The retail literal is 48 wide characters.
        //
        // The channel command then resolves the channel from the current command — hence `verb` —
        // refuses `0` and `0x400` (`@help`), joins the rest and calls
        // channel broadcast `(id, text)`, returning **that** function's
        // answer. The `args.is_empty()` branch is `argc <= 0`, not "the text is blank": retail tests
        // the word count, so `@f " "` has argc 1 and broadcasts a space.
        if handler == "channel_shortcut" {
            if args.is_empty() {
                // The scroll write of type `0x1A` to the current command source's window — the
                // chat window the command was typed into, which this path *does* carry, unlike the
                // `Notice::DisplayString` route, which loses it.
                game.scroll.add_text_to_scroll(
                    CHANNEL_COMMAND_NEEDS_TEXT,
                    0x1A,
                    true,
                    self.chat.current_command_source,
                );
                self.stats.channel_commands_without_text += 1;
                return;
            }
            let channel = dereth_client_model::chat::get_channel_id(verb);
            if dereth_client_model::chat::channel_command_broadcasts_on(channel) {
                dereth_client_model::RequestSink::send(
                    req,
                    Request::ChannelBroadcast(
                        dereth_protocol::comms::CommunicationChannelBroadcast {
                            channel,
                            message: joined,
                        },
                    ),
                );
                self.stats.channel_commands_sent += 1;
            } else {
                // The channel-command handler returned `false`, so `do_command`'s
                // failure event `0x26`
                // *does* fire here. No word registered against this handler resolves to 0 or
                // 0x400, so this arm is unreachable from the shipped table and exists because the
                // two halves refuse for different reasons and only one of them is silent.
                game.scroll.add_text_to_scroll(
                    dereth_client_model::cmd::NOT_A_VALID_COMMAND,
                    0x1A,
                    true,
                    self.chat.current_command_source,
                );
                self.stats.chat_commands_refused += 1;
            }
            return;
        }
        // ---- `@friends`, `@friends_add`, `@friends_remove` --------------------------------------
        //
        // Three command names whose handlers live here; without this arm every one of them would
        // fall to the catch-all below and answer *"That is not a valid command."*. This is the
        // whole chat entry point for the friends list.
        //
        // The original three friends-command handlers send no requests directly: each
        // notifies the friends panel, which performs the action. This path instead
        // makes the decision in `dereth_client_model::friends`, the same model the panel draws,
        // rather than calling through a UI panel from the command handler.
        //
        // The friends handler re-splits its own argv (`next_arg` for the sub-command, then
        // `join_args_as_name` for the rest), so the model is handed `args` and not `joined`: joining
        // and re-splitting here would lose a name containing a run of spaces that `join_args_as_name`
        // preserves.
        if let Some(outcome) = match handler {
            "friends" => Some(game.do_friends(req, args)),
            "friends_add" => Some(game.do_friends_add(req, args)),
            "friends_remove" => Some(game.do_friends_remove(req, args)),
            _ => None,
        } {
            use dereth_client_model::friends::FriendsOutcome as F;
            match outcome {
                // The three sending arms. The request is already in `req` -- these only count.
                F::Added(_) | F::RemovedById(_) | F::Cleared | F::OldList => {
                    self.stats.friends_requests += 1;
                    // The remove-all-friends chat command prints its
                    // acknowledgement *before* any answer from the shard, on chat type 0.
                    if matches!(outcome, F::Cleared) {
                        game.scroll.add_text_to_scroll(
                            dereth_client_model::friends::FRIENDS_LIST_CLEARED,
                            dereth_client_model::friends::LISTING_CHAT_TYPE,
                            true,
                            0,
                        );
                    }
                }
                // A scroll write of type 0 to window `0`, not the command source. That
                // asymmetry with the refusals below is retail's: the listing is the *panel's*
                // print and the refusals are the command handler's.
                F::Listed { lines, .. } => {
                    for line in lines {
                        game.scroll.add_text_to_scroll(
                            &line,
                            dereth_client_model::friends::LISTING_CHAT_TYPE,
                            true,
                            0,
                        );
                    }
                    self.stats.friends_listings += 1;
                }
                F::Refused(text) => {
                    game.scroll.add_text_to_scroll(
                        text,
                        dereth_client_model::friends::REFUSAL_CHAT_TYPE,
                        true,
                        self.chat.current_command_source,
                    );
                    self.stats.chat_commands_refused += 1;
                }
                // Display the object error with an empty detail string -- the failure-event
                // handler with an empty detail, the same function a `0x028A` reaches.
                //
                // `0x561`'s arm prints on channel `0x1A`, while `0x563` formats its message and
                // prints on channel 0, as confirmed against retail. The table covers the whole
                // function, so the refusal prints the sentence retail prints.
                //
                // Window **0**, not the command source: every one of the failure-event handler's
                // 232 scroll writes passes `0` as its last
                // argument, which is the asymmetry the doc on `apply_chat_command` describes.
                F::WeenieError(code) => {
                    if code == dereth_client_model::friends::ERROR_FRIENDS_LIST_FULL {
                        self.stats.friends_list_full_refusals += 1;
                    } else {
                        self.stats.friends_not_a_friend_refusals += 1;
                    }
                    if let Some(m) =
                        dereth_client_contract::chat::failure::handle_failure_event(code, "")
                    {
                        game.scroll.add_text_to_scroll(
                            crate::chat::add_text_to_scroll_trim(&m.body),
                            u32::from(m.ty),
                            true,
                            0,
                        );
                    }
                }
            }
            return;
        }
        // ---- `@allegiance`, `@motd`, `@alh`/`@ah`, `@ab` ---------------------------
        //
        // Four command names whose handlers live here; without this arm every one would fall to
        // the catch-all below, count itself in `chat_commands_unimplemented` and answer *"That is
        // not a valid command."*.
        //
        // These commands are the production path for all twenty-four events. Each
        // original event has one command-handler caller. The allegiance panel sends
        // only `0x001D`, `0x001E` and `0x001F`; none of the twenty-four has a button,
        // context menu or confirmation dialog, so command dispatch is essential.
        //
        // The parse lives in `dereth_client_model::allegiance_cmd` for the same reason `@friends`' does:
        // "which allegiance member does this line name, and is the level legal" is a model
        // decision, and this file's job is to turn the answer into a `Request`, a scroll line and
        // a counter. `args` and not `joined`, because these handlers re-split their own argv
        // (`next_arg`, then `join_args` or `join_args_as_name` for the tail) and a name carrying a run
        // of spaces survives that and would not survive a join-then-resplit here.
        if let Some(cmd) = match handler {
            "allegiance" => Some(game.do_allegiance(req, args)),
            "allegiance_hometown" => Some(game.do_allegiance_hometown(req, args)),
            "allegiance_broadcast" => Some(game.do_allegiance_broadcast(req, args)),
            "motd" => Some(game.do_motd(req, args)),
            _ => None,
        } {
            // Write the allegiance-chat listening bit, then tail-call the option-change handler
            // with ordinal `0x1B`, which is the same path
            // `UiRequest::SetPlayerOption` takes and therefore the same `0x0005`
            // player-option change when the ordinal is an `is_auto_save_option` one.
            // The `@allegiance chat on` path is the only one in this family that sends
            // something other than an allegiance event.
            if let Some(on) = cmd.hear_allegiance_chat {
                let change = game.player_system.set_option(
                    dereth_client_model::allegiance_cmd::HEAR_ALLEGIANCE_CHAT_ORDINAL,
                    on,
                    now,
                );
                for (ordinal, value) in &change.sends {
                    dereth_client_model::RequestSink::send(
                        req,
                        Request::PlayerOptionChanged(
                            dereth_protocol::login::CharacterPlayerOptionChangedEvent {
                                option: u32::try_from(*ordinal).unwrap_or(u32::MAX),
                                value: u32::from(*value),
                            },
                        ),
                    );
                    self.stats.option_changes_sent += 1;
                }
                if change.deferred {
                    self.stats.option_changes_deferred += 1;
                }
            }
            // The scroll write to the current command source's window — every print site in
            // this family passes the command's own window, including `do_allegiance_boot`'s
            // acknowledgement, which is the one that is **not** a refusal (chat type 0).
            for (text, ty) in &cmd.lines {
                game.scroll
                    .add_text_to_scroll(text, *ty, true, self.chat.current_command_source);
                if *ty == dereth_client_model::allegiance_cmd::REFUSAL_CHAT_TYPE {
                    self.stats.chat_commands_refused += 1;
                }
            }
            self.stats.allegiance_command_requests += cmd.sent;
            // `do_command`'s failure event `0x26`. The `@allegiance` handler never gets here
            // (it always returns true and prints its own hint); `@motd wibble`, `@ab` with no text
            // and `@ah` are the entries that can.
            if !cmd.handled {
                game.scroll.add_text_to_scroll(
                    dereth_client_model::cmd::NOT_A_VALID_COMMAND,
                    0x1A,
                    true,
                    self.chat.current_command_source,
                );
                self.stats.chat_commands_refused += 1;
            }
            return;
        }
        // ---- `@loc` -----------------------------------------------------------------
        //
        // Verified against retail:
        //
        // Extra arguments print the wide "Unexpected arguments to @loc" message on `0x1A`,
        // with logging enabled and the command-source window, then return true. With no arguments,
        // an absent player body returns true silently. Otherwise read the body's cell and frame;
        // cell zero prints the wide "Not in valid cell!" message on `0x1A`. A valid cell is
        // formatted into the narrow "Your location is: %s\n" template (21 characters plus NUL)
        // and printed on type 0, again logged in the command-source window.
        //
        // Position formatting uses `_snprintf(buf, 100, "0x%08X [%f %f %f] %f %f %f %f",
        // cell, x, y, z, qw, qx, qy, qz)` -- `%f` at C's default six decimals, confirmed by
        // the observed retail output. The trailing `\n` of the outer template is
        // the scroll write's to trim (`chat::add_text_to_scroll_trim`), as it is for every line.
        if handler == "loc" {
            if !args.is_empty() {
                game.scroll.add_text_to_scroll(
                    "Unexpected arguments to @loc",
                    0x1A,
                    true,
                    self.chat.current_command_source,
                );
                self.stats.chat_commands_refused += 1;
                return;
            }
            let Some(p) = self.player_position else {
                return;
            };
            if p.cell.0 == 0 {
                game.scroll.add_text_to_scroll(
                    "Not in valid cell!",
                    0x1A,
                    true,
                    self.chat.current_command_source,
                );
                self.stats.chat_commands_refused += 1;
                return;
            }
            let line = format!("Your location is: {}\n", position_to_string(&p));
            game.scroll
                .add_text_to_scroll(&line, 0, true, self.chat.current_command_source);
            self.stats.loc_lines_printed += 1;
            return;
        }
        // `@lockui`: any argument prints the specific usage line and returns true.
        // The no-argument path toggles the lock-UI option, whose setter tail-jumps
        // the option-change handler with ordinal `0x33`. `LockUI` is in `is_auto_save_option`, so one changed bit
        // is mirrored into the retained module and leaves immediately as `0x0005`; equality is a
        // complete no-op. App performs the following global-0D UI cascade with the chosen value.
        if handler == "lockui" {
            if args.is_empty() {
                let ordinal = dereth_client_model::player::options::option::LOCK_UI;
                let locked = !game.player_system.options.get(ordinal);
                self.apply_lock_ui_option(game, req, locked, now);
                self.pending_ui_layout_commands
                    .push(UiLayoutCommand::SetLockUi(locked));
            } else {
                game.scroll.add_text_to_scroll(
                    "Please use @help lockui for proper usage.",
                    0x1A,
                    true,
                    self.chat.current_command_source,
                );
                self.stats.chat_command_lines += 1;
                self.stats.chat_commands_refused += 1;
            }
            return;
        }
        // ---- `@saveui` / `@loadui` and their `#auto` siblings -------------------
        //
        // The handlers validate argument count and the optional name, then raise
        // save/load-layout notices. `App` applies them to the live gameplay screen,
        // matching the original two notice handlers. Refusals still return handled,
        // so they do not append the generic failure-`0x26` line.
        if matches!(handler, "saveui" | "loadui" | "saveautoui" | "loadautoui") {
            let auto = matches!(handler, "saveautoui" | "loadautoui");
            let operation = if matches!(handler, "saveui" | "saveautoui") {
                "save"
            } else {
                "load"
            };
            if (auto && !args.is_empty()) || (!auto && args.len() > 1) {
                let auto_word = if auto { "auto" } else { "" };
                let line = format!("Please use @help {operation}{auto_word}ui for proper usage.");
                game.scroll
                    .add_text_to_scroll(&line, 0x1A, true, self.chat.current_command_source);
                self.stats.chat_command_lines += 1;
                self.stats.chat_commands_refused += 1;
                return;
            }
            let name = if auto {
                dereth_client_contract::persist::ScreenLayout::AUTO_NAME.to_owned()
            } else {
                args.first().cloned().unwrap_or_default()
            };
            // The original narrow-string length includes its trailing NUL and is compared
            // with `0x10`: fifteen visible characters pass and sixteen fail. Parsing maps
            // each CP-1252 source byte to one scalar, so counting UTF-8 bytes here would
            // incorrectly reject valid non-ASCII names.
            if !auto && name.chars().count() >= 16 {
                game.scroll.add_text_to_scroll(
                    "The file name must be 16 characters or less.",
                    0x1A,
                    true,
                    self.chat.current_command_source,
                );
                self.stats.chat_command_lines += 1;
                self.stats.chat_commands_refused += 1;
                return;
            }
            let command = if operation == "save" {
                UiLayoutCommand::Save(name)
            } else {
                UiLayoutCommand::Load(name)
            };
            self.pending_ui_layout_commands.push(command);
            return;
        }
        // ---- `@fillcomps` ---------------------------------------------
        //
        // The command raises the same local vendor notice as the Components
        // UI. The existing vendor fill operation owns the shortfall, stock, price-cap and
        // missing-row rules; this arm only preserves the command's argc/category/price grammar.
        if handler == "fillcomps" {
            if args.len() > 2 {
                game.scroll.add_text_to_scroll(
                    "Please use @help fillcomps for proper usage.",
                    dereth_client_model::chat::text_type::LOCAL_ERROR,
                    true,
                    self.chat.current_command_source,
                );
                self.stats.chat_command_lines += 1;
                self.stats.chat_commands_refused += 1;
                return;
            }
            if args
                .first()
                .is_some_and(|arg| arg.eq_ignore_ascii_case("clear"))
            {
                game.clear_desired_components(req);
                game.scroll.add_text_to_scroll(
                    "Component list cleared.",
                    dereth_client_model::chat::text_type::LOCAL_ERROR,
                    true,
                    self.chat.current_command_source,
                );
                self.stats.desired_comp_sets += 1;
                self.stats.chat_command_requests += 1;
                self.stats.chat_command_lines += 1;
                return;
            }

            let category = args.first().and_then(|arg| spell_component_category(arg));
            let price_word = if category.is_some() {
                args.get(1).map(String::as_str)
            } else {
                // Retail keeps parsing the first argument when the category mapper misses. A
                // permitted second argument is ignored in this branch, whether the first is a
                // price or an invalid word.
                args.first().map(String::as_str)
            };
            let max_price = match price_word.and_then(|word| word.parse::<i32>().ok()) {
                Some(price) if price < 1 => {
                    game.scroll.add_text_to_scroll(
                        "Please specify a value greater than 0.",
                        dereth_client_model::chat::text_type::LOCAL_ERROR,
                        true,
                        self.chat.current_command_source,
                    );
                    self.stats.chat_command_lines += 1;
                    self.stats.chat_commands_refused += 1;
                    return;
                }
                Some(price) => price,
                None if category.is_some() || args.is_empty() => 0,
                None => {
                    game.scroll.add_text_to_scroll(
                        "Invalid component type specified.",
                        dereth_client_model::chat::text_type::LOCAL_ERROR,
                        true,
                        self.chat.current_command_source,
                    );
                    self.stats.chat_command_lines += 1;
                    self.stats.chat_commands_refused += 1;
                    return;
                }
            };
            let vendor = game.shop.vendor_id;
            let result = game.fill_component_list(category, max_price, out);
            self.stats.fill_components_rows += u64::try_from(result.added).unwrap_or(0);
            self.stats.fill_components_missing +=
                u64::try_from(result.not_stocked + result.short_stocked).unwrap_or(0);
            if vendor.is_some() {
                self.vendor_buying_tab_requested = vendor;
            }
            return;
        }
        // ---- `@squelch` / `@unsquelch` -----------------------------------------
        //
        // The two handlers are identical after choosing the
        // add flag. With no argv they run the local squelch query; otherwise
        // `process_squelch_args` chooses either the character event (`0x0058`, object 0,
        // plus `LogTextType`) or account event (`0x0059`, no object/type fields).
        if matches!(handler, "squelch" | "unsquelch") {
            if args.is_empty() {
                let line = squelch_query(&game.chat);
                game.scroll.add_text_to_scroll(
                    &line,
                    dereth_client_model::chat::text_type::DEFAULT,
                    true,
                    0,
                );
                self.stats.chat_command_lines += 1;
                return;
            }
            let parsed = match process_squelch_args(args, &game.chat.last_teller_name, true) {
                Ok(parsed) => parsed,
                Err(line) => {
                    game.scroll.add_text_to_scroll(
                        &line,
                        dereth_client_model::chat::text_type::LOCAL_ERROR,
                        true,
                        self.chat.current_command_source,
                    );
                    self.stats.chat_command_lines += 1;
                    self.stats.chat_commands_refused += 1;
                    return;
                }
            };
            if let Some(line) = parsed.warning {
                game.scroll.add_text_to_scroll(
                    line,
                    dereth_client_model::chat::text_type::LOCAL_ERROR,
                    true,
                    self.chat.current_command_source,
                );
                self.stats.chat_command_lines += 1;
                self.stats.chat_commands_refused += 1;
            }
            let add = handler == "squelch";
            if parsed.account {
                game.modify_account_squelch(req, add, &parsed.name);
            } else {
                game.modify_character_squelch(
                    req,
                    ObjectId(0),
                    add,
                    &parsed.name,
                    parsed.message_type,
                );
            }
            self.stats.squelch_requests += 1;
            self.stats.chat_command_requests += 1;
            return;
        }
        // ---- `@filter` / `@unfilter` -------------------------------------------
        //
        // The global squelch modifier reuses `process_squelch_args`, but with
        // `require_target = false`, then accepts only a dash-prefixed message type with neither
        // account nor target. The client sends `0x005B` and does not predict the global table;
        // the authoritative `0x01F4` replacement remains the only local-state writer.
        if matches!(handler, "filter" | "unfilter") {
            if args.is_empty() {
                let line = global_squelch_query(&game.chat);
                game.scroll.add_text_to_scroll(
                    &line,
                    dereth_client_model::chat::text_type::DEFAULT,
                    true,
                    0,
                );
                self.stats.chat_command_lines += 1;
                return;
            }
            if !args.first().is_some_and(|arg| arg.starts_with('-')) {
                game.scroll.add_text_to_scroll(
                    "You must specify a valid message type prefixed by a dash.",
                    dereth_client_model::chat::text_type::LOCAL_ERROR,
                    true,
                    self.chat.current_command_source,
                );
                self.stats.chat_command_lines += 1;
                self.stats.chat_commands_refused += 1;
                return;
            }
            let parsed = match process_squelch_args(args, &game.chat.last_teller_name, false) {
                Ok(parsed) => parsed,
                Err(line) => {
                    game.scroll.add_text_to_scroll(
                        &line,
                        dereth_client_model::chat::text_type::LOCAL_ERROR,
                        true,
                        self.chat.current_command_source,
                    );
                    self.stats.chat_command_lines += 1;
                    self.stats.chat_commands_refused += 1;
                    return;
                }
            };
            if let Some(line) = parsed.warning {
                game.scroll.add_text_to_scroll(
                    line,
                    dereth_client_model::chat::text_type::LOCAL_ERROR,
                    true,
                    self.chat.current_command_source,
                );
                self.stats.chat_command_lines += 1;
                self.stats.chat_commands_refused += 1;
            }
            if parsed.account || !parsed.name.is_empty() {
                game.scroll.add_text_to_scroll(
                    "Incorrect usage, use @help for proper arguements.",
                    dereth_client_model::chat::text_type::LOCAL_ERROR,
                    true,
                    self.chat.current_command_source,
                );
                self.stats.chat_command_lines += 1;
                self.stats.chat_commands_refused += 1;
                return;
            }
            dereth_client_model::RequestSink::send(
                req,
                Request::ModifyGlobalSquelch(
                    dereth_protocol::comms::CommunicationModifyGlobalSquelch {
                        add: i32::from(handler == "filter"),
                        msg_type: parsed.message_type,
                    },
                ),
            );
            self.stats.chat_command_requests += 1;
            return;
        }
        // ---- `@corpse` / `@cor` -----------------------------------------------
        //
        // The corpse-location command reads the local player's position quality `0x0E`
        // and prints one type-zero line without sending a request. Unlike the location
        // command, it ignores all supplied arguments.
        //
        // The original missing-description return is unhandled; this router directly
        // emits the same ordinary failure line. An available description lacking
        // `0x0E` instead produces a handled apology.
        if handler == "corpse" {
            let Some(qualities) = game.player_qualities() else {
                game.scroll.add_text_to_scroll(
                    dereth_client_model::cmd::NOT_A_VALID_COMMAND,
                    0x1A,
                    true,
                    self.chat.current_command_source,
                );
                self.stats.chat_commands_refused += 1;
                return;
            };
            let coordinate = qualities
                .positions
                .as_ref()
                .and_then(|positions| positions.get(&0x0E))
                .and_then(|position| corpse_coordinate_string(position.objcell_id));
            let line = coordinate.map_or_else(
                || {
                    "We're sorry, but we have no record of your last outside corpse location.\n"
                        .to_owned()
                },
                |coordinate| {
                    format!(
                        "The last time you died outside, your corpse was located at ({coordinate}).\n"
                    )
                },
            );
            game.scroll
                .add_text_to_scroll(&line, 0, true, self.chat.current_command_source);
            self.stats.chat_command_lines += 1;
            return;
        }
        // **The `help` handler: `@help` and `@?`.**
        //
        // The command table's very first two entries are `?` and `help`, both the `help`
        // handler; every string they need is in `dereth_client_model::cmd::table::HELP_TEXTS`.
        //
        // The handler is a pure function of the command table, so it lives in the command
        // interpreter beside that table rather than in `dereth_client_model::chat_cmd`: nothing
        // about it is a model question and it sends nothing at all. What this file owns is the
        // window — every line goes to
        // the current command source, which is `self.chat.current_command_source` — and the
        // counters.
        //
        // The handler returns **true** on every path, including the `"Unknown command"` one, so
        // `do_command`'s failure event `0x26` never fires for it.
        if handler == "help" {
            for (text, ty) in self.chat.do_help(args) {
                game.scroll
                    .add_text_to_scroll(&text, ty, true, self.chat.current_command_source);
                self.stats.chat_command_lines += 1;
                if ty == dereth_client_model::chat_cmd::REFUSAL_CHAT_TYPE {
                    self.stats.chat_commands_refused += 1;
                }
            }
            return;
        }
        // ---- `@join` / `@leave` ------------------------------------------------
        //
        // Both handlers call `next_arg` once, compare that
        // one token case-insensitively against six native channel names, and ignore the tail.
        // A recognized token calls the matching chat-option handler; all six options are
        // auto-saved, so a moved bit leaves as `0x0005 Character_PlayerOptionChangedEvent`.
        // Missing or unknown tokens return FALSE and let `do_command` print its ordinary failure.
        if matches!(handler, "join" | "leave") {
            let Some(ordinal) = join_leave_channel_option(args.first().map(String::as_str)) else {
                game.scroll.add_text_to_scroll(
                    dereth_client_model::cmd::NOT_A_VALID_COMMAND,
                    0x1A,
                    true,
                    self.chat.current_command_source,
                );
                self.stats.chat_commands_refused += 1;
                return;
            };
            let change = game
                .player_system
                .set_option(ordinal, handler == "join", now);
            // The retail option-changed notice immediately recomputes the five
            // Turbine talk-focus rows. Allegiance (27) is instead driven by its room tracker.
            if change.moved() && matches!(ordinal, 35 | 36 | 37 | 38 | 46) {
                let heritage =
                    Self::player_desc(game, player_desc_received).map_or(0, |q| q.inq_int(0xBC));
                game.enable_chat_talk_focuses(crate::chat::is_olthoi(heritage));
                chat_focus(&mut game.chat);
            }
            for (ordinal, value) in &change.sends {
                dereth_client_model::RequestSink::send(
                    req,
                    Request::PlayerOptionChanged(
                        dereth_protocol::login::CharacterPlayerOptionChangedEvent {
                            option: u32::try_from(*ordinal).unwrap_or(u32::MAX),
                            value: u32::from(*value),
                        },
                    ),
                );
                self.stats.option_changes_sent += 1;
            }
            if change.deferred {
                self.stats.option_changes_deferred += 1;
            }
            return;
        }
        // ---- the character, comms, consent and local families ----------------------
        //
        // Twenty-four more command names whose handlers live here; without this arm every one
        // would fall to the catch-all below, count itself in `chat_commands_unimplemented`
        // and answer *"That is not a valid command."*.
        //
        // **The chat entry is the whole production path for all of them.** Retail gives
        // each teleport/query/consent-list/player-permission event,
        // each AFK/channel/global-squelch event and both house-teleport events
        // exactly one caller in retail, a chat-command handler.
        // There is no button for any of them.
        //
        // The parse and the literals live in `dereth_client_model::chat_cmd` for the reason `@friends`' and
        // `@allegiance`'s do: "is this player a PK", "is the Afk quality set" and "does this word
        // name a channel" are model questions, and this file's job is to turn the answer into a
        // scroll line, a counter, an option write or a dialog. `args` and not `joined`, because
        // the three shape-C handlers re-split their own argv (`next_arg`, then `join_args` or
        // `join_args_as_name`) and a name carrying a run of spaces survives that.
        if let Some(cmd) = match handler {
            "lifestone" => Some(game.do_lifestone(req, args)),
            "marketplace" => Some(game.do_marketplace(req, args)),
            "house_recall" => Some(game.do_house_recall(req, args)),
            "mansion_recall" => Some(game.do_mansion_recall(req, args)),
            "pkarena" => Some(game.do_pk_arena(req, args)),
            "pklarena" => Some(game.do_pkl_arena(req, args)),
            "pklite" => Some(game.do_pk_lite(req, args)),
            "age" => Some(game.do_age(req, args)),
            "birth" => Some(game.do_birth(req, args)),
            "die" => Some(game.do_die(args)),
            "chat" => Some(game.do_chat_toggle(req, args)),
            "notell" => Some(game.do_no_tell(req, args)),
            "index" => Some(game.do_channel_index(req, args)),
            "clist" => Some(game.do_channel_list(req, args)),
            "on" => Some(game.do_channel_on(req, args)),
            "off" => Some(game.do_channel_off(req, args)),
            "afk" => Some(game.do_afk(req, args)),
            "consent" => Some(game.do_consent(req, args)),
            "permit" => Some(game.do_permit(req, args)),
            "speaker" => Some(game.do_speaker(args)),
            "endurance" => Some(game.do_endurance(args)),
            "emotes" => Some(game.do_emote_list(args)),
            "clear" => Some(game.do_clear(args)),
            // ---- `@house`/`@hou` and `@hslist` -----------------------------------
            //
            // The `house` handler is handed the whole of `argv`; its own `next_arg` takes the
            // sub-command and each sub-handler `next_arg`s again, so `args` and not `joined` for
            // the reason the three shape-C handlers above want it. `@house guest add Baron Lark`
            // has to reach `join_args_as_name` with its interior spacing intact.
            "house" => Some(game.do_house(req, args)),
            "hslist" => Some(game.do_house_available_list(req, args)),
            _ => None,
        } {
            self.apply_chat_command(cmd, game, req, out, now);
            return;
        }
        // The `say` handler: `@say` and `@s`.
        //
        // Join arguments and trim whitespace on both sides (the emote command does not trim).
        // An empty result prints "You must specify the text you wish to say!" on `0x1A` in
        // the command-source window; a nonempty result goes through public-chat processing.
        // Both paths return true.
        //
        // Two adjacent handlers, two different answers to an empty line: the `say` handler
        // refuses and the `emote` handler silently returns true. Both are transcribed rather than harmonised.
        if handler == "say" {
            let text = joined.trim_matches(dereth_client_model::chat::WHITESPACE);
            if text.is_empty() {
                game.scroll.add_text_to_scroll(
                    dereth_client_model::chat_cmd::YOU_MUST_SPECIFY_TEXT_TO_SAY,
                    0x1A,
                    true,
                    self.chat.current_command_source,
                );
                self.stats.chat_commands_refused += 1;
                return;
            }
            let text = text.to_owned();
            self.public_chat(&text, game, req);
            return;
        }
        let turbine_focus = match handler {
            "guild" => Some(dereth_client_model::chat::TalkFocus::Allegiance),
            "general" => Some(dereth_client_model::chat::TalkFocus::General),
            "trade" => Some(dereth_client_model::chat::TalkFocus::Trade),
            "lfg" => Some(dereth_client_model::chat::TalkFocus::Lfg),
            "roleplay" => Some(dereth_client_model::chat::TalkFocus::Roleplay),
            "society" => Some(dereth_client_model::chat::TalkFocus::Society),
            "olthoi" => Some(dereth_client_model::chat::TalkFocus::Olthoi),
            _ => None,
        };
        if let Some(focus) = turbine_focus {
            // Each explicit handler refuses zero arguments itself; command dispatch also reports
            // error `0x26` when the handler returns false. No fallback public-speech packet.
            if args.is_empty() {
                game.scroll.add_text_to_scroll(
                    dereth_client_model::cmd::NOT_A_VALID_COMMAND,
                    0x1A,
                    true,
                    0,
                );
            }
            // The allegiance-channel handler (`@a`) is the **only** one of the seven
            // that tests its room before speaking. After the `argc` guard above it reads the
            // tracker and leaves:
            //
            // Retail reads the allegiance room from the tracker and continues to
            // chat sending only if it is nonzero. Otherwise it reports error `0x414` with an
            // empty detail string ("You are not in an allegiance!") and return false, causing
            // command dispatch to add error `0x26` below.
            //
            // `send_turbine_chat`'s *"Turbine chat is not available."* — which has exactly one
            // native string referrer — is
            // therefore **unreachable from `/a`**; printing it would point the diagnosis at the
            // chat provider when the missing thing is the room id. The six sibling commands
            // have no such test and keep `send_turbine_chat`'s sentence.
            if focus == dereth_client_model::chat::TalkFocus::Allegiance
                && !args.is_empty()
                && game.chat.chat_rooms.get(&1).copied().unwrap_or(0) == 0
            {
                if let Some(m) = dereth_client_contract::chat::failure::handle_failure_event(
                    dereth_client_contract::chat::failure::YOU_ARE_NOT_IN_ALLEGIANCE,
                    "",
                ) {
                    // A scroll write of type `0x1a` to window 0, not the command
                    // source, exactly as the failure-event handler's own arm passes it.
                    game.scroll
                        .add_text_to_scroll(&m.body, u32::from(m.ty), true, 0);
                }
                game.scroll.add_text_to_scroll(
                    dereth_client_model::cmd::NOT_A_VALID_COMMAND,
                    0x1A,
                    true,
                    0,
                );
                self.stats.chat_commands_refused += 1;
                return;
            }
            if args.is_empty()
                || !game.send_turbine_chat(
                    req,
                    focus,
                    true,
                    &joined,
                    self.chat.current_command_source,
                    chat_real_time(),
                )
            {
                game.scroll.add_text_to_scroll(
                    dereth_client_model::cmd::NOT_A_VALID_COMMAND,
                    0x1A,
                    true,
                    0,
                );
                self.stats.chat_commands_refused += 1;
            }
            return;
        }
        let outcome = match handler {
            "tell" => game.chat.do_tell(&joined),
            "reply" => game.chat.do_reply(&joined),
            "retell" => game.chat.do_retell(&joined),
            // No handler the table names reaches this arm; see `chat_commands_unimplemented`.
            _ => {
                self.stats.chat_commands_unimplemented += 1;
                T::Refused(dereth_client_model::cmd::NOT_A_VALID_COMMAND)
            }
        };
        match outcome {
            T::ByName {
                message,
                target_name,
            } => {
                dereth_client_model::RequestSink::send(
                    req,
                    Request::TalkDirectByName(
                        dereth_protocol::comms::CommunicationTalkDirectByName {
                            message,
                            target_name,
                        },
                    ),
                );
                self.stats.tells_sent += 1;
            }
            T::ById { message, target } => {
                dereth_client_model::RequestSink::send(
                    req,
                    Request::TalkDirect(dereth_protocol::comms::CommunicationTalkDirect {
                        message,
                        target,
                    }),
                );
                self.stats.tells_sent += 1;
            }
            T::Refused(text) => {
                dereth_client_model::NoticeSink::emit(
                    out,
                    Notice::DisplayString {
                        channel: dereth_client_model::chat::REFUSAL_CHANNEL,
                        text: text.to_owned(),
                    },
                );
                self.stats.chat_commands_refused += 1;
            }
            T::Nothing => self.stats.tells_dropped_silently += 1,
        }
    }

    /// **The last mile.** Every notice this call raised, plus the requests it queued.
    ///
    /// Display-string notice handling adds `(text, channel, true, 0)` to the scroll;
    /// here the scroll is the shared chat scroll, which is the one object this file and
    /// `hud.rs` — the two halves of the seam — both hold a `&mut` to.
    /// Three combat refusals bypass notices: two from combat-mode
    /// toggling and one from starting an attack. All three write
    /// `(text, 0x1A, true, 0)` to the scroll, reaching the same channel as
    /// `DisplayString` one step earlier in the chain.
    fn refuse(&mut self, game: &mut dereth_client_model::World, text: &str) {
        game.scroll.add_text_to_scroll(
            text,
            dereth_client_model::scroll::LOCAL_ERROR_TYPE,
            true,
            0,
        );
        self.stats.notice_strings_scrolled += 1;
        self.last_refusal = Some(text.to_owned());
    }

    /// Object-maintenance/create/delete broadcasts join the same subscribers as UI replies.
    /// ObjectStream has already retired the exact old instance before offering these UI notices.
    pub fn apply_object_notices(
        &mut self,
        game: &mut dereth_client_model::World,
        notices: Vec<Notice>,
    ) {
        let mut out = Notices::default();
        let mut req = RecordingRequests::default();
        for notice in notices {
            let moved = match &notice {
                Notice::ItemMoved {
                    object, container, ..
                } => Some((*object, *container)),
                _ => None,
            };
            out.emit(notice);
            if let Some((object, container)) = moved {
                self.on_item_moved(game, object, container, &mut out, &mut req);
            }
        }
        self.absorb(game, out, req);
    }

    /// Complete only this lifetime notice's selection subscribers while the old Weenie is
    /// queryable. A defender event is a different producer and is not drained opportunistically.
    pub fn dispatch_object_notice(
        &mut self,
        game: &mut dereth_client_model::World,
        notice: Notice,
        geometry: &crate::selection_geometry::SceneSelectionPhysics,
        radius: f32,
        now: dereth_primitives::LocalTime,
    ) {
        let preceding = std::mem::take(&mut self.pending_selection_changes);
        self.apply_object_notices(game, vec![notice]);
        if self.pending_selection_changes != 0 {
            let geometry = geometry.for_current_objects(game);
            while self.pending_selection_changes != 0 {
                self.run_selection_change_notices(game, &geometry, radius, now);
            }
        }
        self.pending_selection_changes = preceding;
    }

    /// The toolbar's direct health/mana queries, shared with ordinary `UiRequest`
    /// delivery. This bounded tail drains no unrelated input and replays no UI tick.
    pub fn dispatch_toolbar_query(
        &mut self,
        game: &mut dereth_client_model::World,
        request: &UiRequest,
    ) -> bool {
        let mut req = RecordingRequests::default();
        match request {
            UiRequest::QueryHealth(id) => game.query_health(&mut req, *id),
            UiRequest::QueryItemMana(id) => game.query_item_mana(&mut req, *id),
            _ => return false,
        }
        self.stats.vital_queries += 1;
        self.outbox.extend(req.0);
        true
    }

    /// Complete the three client subscribers to item-move notices: the
    /// toolbar's ownership sweep first, then secure trade's partner-row lifetime, then the
    /// player's blocked inventory retry.
    fn on_item_moved(
        &mut self,
        game: &mut dereth_client_model::World,
        object: ObjectId,
        container: ObjectId,
        out: &mut Notices,
        req: &mut RecordingRequests,
    ) {
        // **The toolbar's item-move handler.**
        //
        // The handler's second half is a sweep over the shortcut slots for the slot holding the
        // moved object, gated on the object having left the player:
        //
        // For each slot whose item id matches, remove its shortcut with server notification
        // when object lookup fails or the object is no longer owned by the player.
        //
        // The world's ownership query already answers `false` for an id with no weenie (the walk
        // terminates on the missing lookup, exactly as the original's null object lookup
        // does), so the two disjuncts are one call here.
        //
        // The notice's producers are every inventory movement — `0x0022`, `0x0024`, `0x0025`,
        // `0x019A`, and behind `0xF747` — so this one arm
        // covers both halves of losing items on death: the shortcut row goes, and with it the
        // per-object numeral `Hud::slot_decoration` reads (removing a shortcut sets its number
        // to -1), which is what stops the corpse window's copy being decorated.
        // An item moving between the player's own containers is still `is_owned_by_player`, so this
        // cannot disturb an ordinary move.
        if game.player_system.shortcut_slot_of(object).is_some() && !game.is_owned_by_player(object)
        {
            self.remove_shortcut(object, game, req);
        }
        if game.trade_item_moved_to_partner(object, container) {
            self.stats.trade_rows_changed += 1;
        }
        if game.unblock.unblock_attempt_num == 0 {
            return;
        }
        let retried = game.unblock_on_item_moved(
            req,
            out,
            object,
            container,
            self.split,
            ServerTime(self.last_use_time.0),
        );
        if retried && game.unblock.unblock_attempt_num == 0 {
            self.stats.unblock_retries += 1;
        } else if retried {
            self.stats.unblocks_started += 1;
        } else if game.unblock.unblock_attempt_num == 0 {
            self.stats.unblocks_abandoned += 1;
        }
    }

    fn absorb(
        &mut self,
        game: &mut dereth_client_model::World,
        mut out: Notices,
        mut req: RecordingRequests,
    ) {
        self.stats.notices += out.count;
        self.pending_external_container
            .extend(out.external_container);
        self.stats.salvage_panel_notices += out.salvage.len() as u64;
        self.pending_salvage.extend(out.salvage);
        self.pending_slumlord_range_exits
            .extend(out.slumlord_range_exits);
        self.pending_book_range_exits.extend(out.book_range_exits);
        self.pending_usage_confirmations
            .extend(out.usage_confirmations);
        self.pending_targeted_confirmations
            .extend(out.targeted_confirmations);
        // Selection-change notices converge here for the combat
        // subscriber. [`Self::run_selection_change_notices`] handles them in the same
        // frame; a handler that changes selection again feeds back into this queue.
        self.pending_selection_changes = self
            .pending_selection_changes
            .saturating_add(out.selection_changes);
        for (channel, text) in out.strings {
            game.scroll.on_display_string_info(channel, &text);
            self.stats.notice_strings_scrolled += 1;
        }
        // The two notices resolving a pending trade split. An authoritative
        // result joins the already-validated automatic-offer queue and is added at
        // position 0 without repeating the selected-partial-stack refusal.
        for notice in out.trade_split {
            match notice {
                TradeSplitNotice::ItemAttributesChanged(item, kind) => {
                    if game.trade_split_item_attributes_changed(item, kind) {
                        self.pending_trade_for_dummies.push(item);
                    }
                }
                TradeSplitNotice::AttemptFailed => game.clear_pending_trade_split(),
            }
        }
        // Fold the inventory pending-row pair to its final state.
        //
        // The original synchronous notice handler shows the pending row inside
        // backpack placement, before the attempt. Refusal then clears waiting state,
        // followed by the end-pending notice. Here both queued notices are applied
        // after the clear; blindly replaying Show would restore a ghost that the
        // refusal just removed and strand it.
        // A refused full-backpack placement exposes that case.
        //
        // Show alone arms a row. Show plus End around a refused attempt does nothing,
        // leaving the refusal's final object state intact. End alone still removes
        // whatever pending row is armed.
        if !out.pending_in_player.is_empty() {
            let armed = out.pending_in_player.iter().fold(None, |_, n| match n {
                PendingInPlayer::Show(item) => Some(*item),
                PendingInPlayer::End => None,
            });
            match armed {
                Some(item) => game.show_pending_in_player(item),
                None => {
                    game.end_pending_in_player();
                }
            }
        }
        // **Automatic trade-offer handling.**
        //
        // Both producers meet here, and the decision needs the current and maximum
        // split sizes stored on this interaction state. Its refusal message goes
        // straight to the scroll on channel `0x1A`, like the strings above.
        for item in out.trade_for_dummies {
            match game.trade_an_item_for_dummies(item, self.split) {
                dereth_client_model::trade::ForDummies::Offer => {
                    self.pending_trade_for_dummies.push(item);
                    self.stats.trade_for_dummies_offered += 1;
                }
                dereth_client_model::trade::ForDummies::MustSplit => {
                    game.scroll.on_display_string_info(
                        dereth_client_model::trade::TRADE_MESSAGE_CHANNEL,
                        dereth_client_model::trade::messages::MUST_SPLIT,
                    );
                    self.stats.notice_strings_scrolled += 1;
                    self.stats.trade_for_dummies_refused += 1;
                }
                // The item list's membership test answered yes: refuse, and retail says nothing.
                dereth_client_model::trade::ForDummies::AlreadyOffered => {
                    self.stats.trade_for_dummies_refused += 1;
                }
            }
        }
        // **The begin-game notice's receiver.**
        //
        // The minigame handler needs a mutable world borrow, so it runs in `absorb`,
        // where the world and notice batch meet. The original begin-game delivery is
        // synchronous; this path also completes it within the initiating call.
        //
        // The receiver shows the window before trying to join. The resulting `0x0269`
        // therefore enters this frame's outbox and the scroll receives the first
        // window message.
        let boards = std::mem::take(&mut out.begin_games);
        for board in boards {
            let mut join = RecordingRequests::default();
            game.begin_game(board, &mut join);
            self.stats.minigame_boards_used += 1;
            self.stats.minigame_requests += join.0.len() as u64;
            req.0.extend(join.0);
            self.stats.notice_strings_scrolled += game.drain_minigame_text() as u64;
        }
        if let Some(t) = out.last {
            self.last_refusal = Some(t);
        }
        self.outbox.extend(req.0);
    }
}

/// **The reply half of the inventory loop.**
///
/// The network dispatcher inventory arms, which are the *only*
/// things that ever move an item or release the request lock. The client sends a
/// move byte-exactly, ghosts the icon and takes an inventory lock with **no timeout**; these
/// arms are the answer. **A passing corpus test is not a routing client** —
/// `core/client-model/tests/corpus_replay.rs` has a message router the application does not.
///
/// | opcode | client arm |
/// |---|---|
/// | `0x0022 Item_ServerSaysContainID` | the item's server-says-move-item `(container, slot, 0, 0, notify UI = 1)`; when the item is not created yet the id is recorded in the container's list at `slot` so the later `0xF745` lands in the right place |
/// | `0x0023 Item_WearItem` | server-says-move-item `(0, 0, player id, slot, 1)` |
/// | `0x019A Item_ServerSaysMoveItem` | server-says-move-item `(0, 0, 0, 0, 1)` — the item leaves the pack entirely |
/// | `0x00A0 Character_ServerSaysAttemptFailed` | server-says-attempt-failed `(reason, 1)` on the object the **lock** names, not the one the message names — see the arm below |
///
/// `0x0023` is here as well because it is the same apply function and
/// the same lock, and equipping is the half of "my inventory is not interactable" that a
/// double-click produces.
///
/// The `0x00A0` row does **not** always clear the lock: the attempt-failed handler carries the
/// same object-id equality guard as the other three clearers, and this function hands it the
/// previous request's object id itself. Both halves are transcribed and tested.
///
/// **Where this lives, and why it is not `hud.rs`.** The UI queue is drained by
/// [`crate::hud::Hud::ui_event`], which owns chat and the vitals; the
/// three arms here are inventory. Both read the same `SessionEvent` slice and
/// neither opcode set overlaps the other's.
///
/// **The "item not created yet" branch of `0x0022`** — the container's
/// server-says-contain-id record, which is
/// the world's pending-containment record. Twenty-nine of the corpus's 100 `0x0022` take it.
/// Without it the item would land at the **head** of the pack whatever slot the
/// server asked for — the head, not the tail, because setting the weenie description
/// substitutes `0` for the place-in-list lookup's `-1`.
pub fn apply_events(
    inter: &mut Interaction,
    events: &[dereth_client_net::client_session::SessionEvent],
    game: &mut dereth_client_model::World,
) {
    apply_events_at_boundary(inter, events, game, None, &mut |_, _, _| {});
}

/// The `Communication_ChannelList` / `ChannelIndex` handlers — the part the two share, with
/// the header as the one parameter.
///
/// The header is written **before** the list is looked at and unconditionally, so an empty reply
/// still prints one line; the rows are `"   " + name + "\n"`,
/// each its own scroll write of type 0 to window 0.
///
/// Returns how many rows went out, which is `names.len()` — returned rather than assumed so the
/// caller's counter has a producer it did not compute itself.
fn channel_report(game: &mut dereth_client_model::World, header: &str, names: &[String]) -> u64 {
    game.scroll.add_text_to_scroll(
        header,
        dereth_client_model::chat::text_type::DEFAULT,
        true,
        0,
    );
    for n in names {
        game.scroll.add_text_to_scroll(
            &format!("   {n}\n"),
            dereth_client_model::chat::text_type::DEFAULT,
            true,
            0,
        );
    }
    u64::try_from(names.len()).unwrap_or(u64::MAX)
}

/// App's one-message source boundary. UI0024 delivers its Remove callbacks before clearing
/// being_removed; standalone callers can retain a collected NoticeSink with `None` geometry.
pub fn apply_events_at_boundary(
    inter: &mut Interaction,
    events: &[dereth_client_net::client_session::SessionEvent],
    game: &mut dereth_client_model::World,
    geometry: Option<(&crate::selection_geometry::SceneSelectionPhysics, f32)>,
    panels: &mut dyn FnMut(&mut Interaction, &mut dereth_client_model::World, &Notice),
) {
    use dereth_protocol::{Message, Opcode};

    let mut out = Notices::default();
    // `handle_attack_done`'s auto-repeat re-fire *sends*, so the sink is not discarded at the
    // call to `absorb`.
    let mut req = RecordingRequests::default();
    // **The one arm in this function that is not a `UiEvent`.**
    //
    // Player-description handling calls `initialize_player` only while the player has not yet
    // been initialized. That operation finishes by sending `0x021E`, the request that makes the
    // House tab reachable at all.
    // It lands here rather than in `Hud::apply_events` because this is the function that has a
    // `RequestSink`; the model's `player_initialized` flag is the once-per-session guard, and
    // `ObjectStream::reset` clears with the rest of the world on `WorldReset`.
    for e in events {
        if matches!(
            e,
            dereth_client_net::client_session::SessionEvent::PlayerDescription(_)
        ) {
            game.initialize_player(&mut req);
        }
    }
    for e in events {
        let dereth_client_net::client_session::SessionEvent::UiEvent { opcode, blob } = e else {
            continue;
        };
        // The blob begins with its own type dword (`SessionEvent::UiEvent`'s contract), which is
        // what `Message::read` expects to have been stripped.
        let body = blob.get(4..).unwrap_or_default();
        let mut r = dereth_protocol::archive::Reader::new(body);
        // Server-directed item movement raises a notice that drives
        // the player's unblock retry. All three message arms below use the same apply
        // operation and therefore the same notice listener. Record it here and act
        // after the match, rather than attaching another consumer only to `0x019A`.
        let mut moved: Option<(ObjectId, ObjectId)> = None;
        match *opcode {
            Opcode::ITEM_SERVER_SAYS_CONTAIN_ID => {
                let Ok(m) = dereth_protocol::objects::ItemServerSaysContainId::read(&mut r) else {
                    continue;
                };
                // **The arm's second branch.** The client's `0x22` handling looks
                // the item up first and only moves an item it
                // already has; when it has none it pre-places the id in the container instead, so
                // that the `0xF745` still to come lands in the server's slot rather than at the
                // head of the pack. 29 of the corpus's 100 `0x0022` take this branch.
                if game.weenie(m.item).is_some() {
                    game.server_says_move_item(
                        m.item,
                        m.container,
                        m.slot,
                        ObjectId(0),
                        0,
                        true,
                        &mut out,
                    );
                } else {
                    if game.server_says_contain_id(
                        m.container,
                        m.item,
                        m.slot,
                        m.container_properties,
                    ) {
                        inter.stats.contain_ids_preplaced += 1;
                    }
                    // The server-says-move-item callback runs on the way out of this branch —
                    // unconditionally, and with every
                    // "old" field zero, because there is no object to read an old container off.
                    // The known-item branch `return`s before it and fires the same notice from
                    // inside the server-says-move-item handler.
                    out.emit(Notice::ItemMoved {
                        object: m.item,
                        old_container: ObjectId(0),
                        old_wielder: ObjectId(0),
                        old_location: 0,
                        container: m.container,
                        place: m.slot,
                        wielder: ObjectId(0),
                        location: 0,
                    });
                }
                // Counted for both branches: this is "the arm ran", and the
                // branch it took is `contain_ids_preplaced`.
                inter.stats.move_items_applied += 1;
                moved = Some((m.item, m.container));
            }
            Opcode::ITEM_SERVER_SAYS_REMOVE => {
                let Ok(m) = dereth_protocol::objects::ItemServerSaysRemove::read(&mut r) else {
                    continue;
                };
                let now = inter.last_use_time;
                if let Some((geometry, radius)) = geometry {
                    game.server_says_remove_with_dispatch(
                        m.object,
                        ServerTime(now.0),
                        &mut |game, notice| {
                            inter.dispatch_object_notice(
                                game,
                                notice.clone(),
                                geometry,
                                radius,
                                now,
                            );
                            panels(inter, game, &notice);
                        },
                    );
                } else {
                    game.server_says_remove(m.object, ServerTime(now.0), &mut out);
                    moved = Some((m.object, ObjectId(0)));
                }
            }
            Opcode::ITEM_WEAR_ITEM => {
                let Ok(m) = dereth_protocol::objects::ItemWearItem::read(&mut r) else {
                    continue;
                };
                let player = game.player.unwrap_or_default();
                game.server_says_move_item(m.item, ObjectId(0), 0, player, m.slot, true, &mut out);
                inter.stats.move_items_applied += 1;
                moved = Some((m.item, ObjectId(0)));
            }
            Opcode::ITEM_SERVER_SAYS_MOVE_ITEM => {
                let Ok(m) = dereth_protocol::objects::ItemServerSaysMoveItem::read(&mut r) else {
                    continue;
                };
                game.server_says_move_item(m.item, ObjectId(0), 0, ObjectId(0), 0, true, &mut out);
                inter.stats.move_items_applied += 1;
                moved = Some((m.item, ObjectId(0)));
            }
            // **The source stack's half of a split.**
            //
            // The network-blob dispatcher's `0x197` case is the fourth inventory arm. Its body is
            // the sequence byte, the
            // (unaligned) object id, the amount and the value, handed to object maintenance.
            //
            // A drag-split's answer is three messages — `0xF745` create, `0x0022` for the **new**
            // object and `0x0197` for the **source** — and this is the only one that carries the
            // source stack's new count. Without it the source would keep its old count, keep
            // `set_waiting_state(1)`'s ghost, and — because the lock names the source while the
            // `0x0022` names the new object — hold the global inventory lock for the rest of the
            // session, so every later move would be refused with *"You can only move or use one
            // item at a time"*.
            //
            // The handler also releases the lock. The *optimistic* half, the local
            // insert the item list makes at the destination before any of this
            // arrives, is not done here.
            Opcode::ITEM_UPDATE_STACK_SIZE => {
                let Ok(m) = dereth_protocol::items::ItemUpdateStackSize::read(&mut r) else {
                    continue;
                };
                if game.server_says_set_stack_size(
                    m.item,
                    m.sequence,
                    m.amount,
                    m.new_value,
                    &mut out,
                ) {
                    inter.stats.stack_sizes_applied += 1;
                }
            }
            // **The id substitution.**
            //
            // The client's `0x00A0` arm does **not** run the handler on the object the
            // message names. It reads the message's id, then overwrites it with the previous
            // request's object id whenever the lock is held, and only then looks the object up:
            //
            // Retail retains the message's reason but replaces its object id with
            // the previous-request object id whenever that lock is nonzero. It then looks up the
            // chosen object and return without invoking the failure handler when the lookup fails.
            //
            // Three consequences:
            //
            // 1. **The handler's `this` is the substituted weenie**, so its `waiting` clear
            //    (a store of 0), the object **name** in the refusal line
            //    (the wide object-name lookup on that object) and the guard on the lock
            //    (comparing the object's id with the lock) all name the *locked* object.
            // 2. **The notice carries the substituted id too** — the handler's own object id
            //    is what the only producer of the attempt-failed notice passes.
            //    So `unblock_on_attempt_failed` must be asked about the substituted id as well;
            //    asking it about `m.object` compares the blocker against the wrong thing.
            // 3. **An unknown object runs nothing**: the null-object branch skips the call, so no
            //    text, no notice, and *the lock is not released*. This is not a tidy-up — it is
            //    the arm that decides whether a `0x00A0` for a stale id can wedge the inventory,
            //    and a recorded one names `object = 0` (a long solo session, `t = 812.106`).
            //
            // Retail's guard inside the attempt-failed handler can therefore never fail — the
            // function has exactly one caller and is never called indirectly — which is why
            // `RequestLock::clear` is unconditional here. That equivalence
            // holds only for the **lock**, not for the object the
            // handler runs on; tests pin the bytes and drive this arm through them.
            Opcode::CHARACTER_SERVER_SAYS_ATTEMPT_FAILED => {
                let Ok(m) =
                    dereth_protocol::objects::CharacterServerSaysAttemptFailed::read(&mut r)
                else {
                    continue;
                };
                // A nonzero request-lock id replaces the message's object id.
                let object = game.request_lock.substitute(m.object);
                // The native null-object branch leaves the failure otherwise untouched.
                if game.weenie(object).is_none() {
                    inter.stats.attempts_failed_unknown_object += 1;
                } else {
                    game.server_says_attempt_failed(object, m.reason, &mut out);
                    inter.stats.attempts_failed += 1;
                    // The attempt-failed notice: the blocker's move was
                    // refused, so the item the player dropped un-ghosts and the state is dropped.
                    // Its notice is raised from inside the handler above, on the handler's own
                    // object -- the substituted one.
                    if game.unblock_on_attempt_failed(object) {
                        inter.stats.unblocks_abandoned += 1;
                    }
                }
                // The arm's second half, whether or not the object was known: every reason
                // but the seven whose object line already says it all is also handled as a
                // failure event, so a wield the shard refuses for the player's heritage
                // (`0x585`) says why as well as what -- "You are restricted to clothes and
                // armor created for your race." beside "The Academy Coat can't be wielded".
                // Window 0, as every failure-event line is.
                if !dereth_protocol::objects::CharacterServerSaysAttemptFailed::suppresses_generic_text(
                    m.reason,
                ) {
                    if let Some(c) =
                        dereth_client_contract::chat::failure::handle_failure_event(m.reason, "")
                    {
                        game.scroll.add_text_to_scroll(
                            crate::chat::add_text_to_scroll_trim(&c.body),
                            u32::from(c.ty),
                            true,
                            0,
                        );
                    }
                }
            }

            // -------------------------------------------------------------------------------
            // **Inbound arms that call the model's handlers.**
            //
            // Everything below is decoded by `dereth_protocol`, ordered by
            // `dereth_client_net::client_session` and delivered to this function; without an arm
            // here each would be dropped. Each one calls the model's implementation; where an
            // implementation had to be written the doc comment says so.
            // -------------------------------------------------------------------------------

            // The reply to `0x0195` or a container double-click fills that container's contents.
            // The model's contents operation is the only writer of that list for anything but
            // the **player's own** pack (which `0x0013`'s `content_profiles` fills), so without it
            // no chest, corpse or side pack could fill: `InventoryPanel`'s
            // `open_container` reads `GameView::container_contents`, which reads exactly the list
            // this writes.
            //
            // **This arm is the complete contents response, not only the contents list.** The
            // response handler's tail is what
            // *opens the window*, and it is the only place the opening
            // form of the set-ground-object notice is ever raised:
            //
            // when the container is the requested ground object, and is not the object of a
            // pending pick-up request, the ground-object notice is raised for it.
            //
            // Without it the client would ask the server for a corpse's contents, receive them,
            // and never show anything.
            Opcode::ITEM_ON_VIEW_CONTENTS => {
                let Ok(m) = dereth_protocol::objects::ItemOnViewContents::read(&mut r) else {
                    continue;
                };
                if game.on_view_contents(
                    m.container,
                    &m.contents,
                    &mut out,
                    ServerTime(inter.last_use_time.0),
                ) {
                    inter.stats.ground_panels_opened += 1;
                }
                inter.stats.contents_viewed += 1;
            }
            // The stop-viewing arm has two ordered effects. It first closes the ground container
            // when the server is the one ending the view,
            // with the notify-server argument `false` so that no `0x0195` is sent back at it.
            // It then clears the model's viewed-container contents.
            Opcode::ITEM_STOP_VIEWING_OBJECT_CONTENTS => {
                let Ok(m) = dereth_protocol::objects::ItemStopViewingObjectContents::read(&mut r)
                else {
                    continue;
                };
                if game.handle_stop_viewing_object_contents(
                    &mut req,
                    &mut out,
                    m.object,
                    ServerTime(inter.last_use_time.0),
                ) {
                    inter.stats.ground_panels_closed += 1;
                }
                inter.stats.contents_closed += 1;
            }
            // ---------------------------------------------------------------------------------
            // **The network dispatcher's `0x00B4` and `0x00B8` arms** — reading scrolls, letters
            // and signs. `dereth_protocol` decodes `Writing_BookOpen`, the corpus carries one (a
            // long solo session, `t = 611.154`), and `dereth_client_net::client_session` puts it on
            // the UI queue with `recv_queue: UiQueue`; without these arms nothing would be
            // displayed. See `dereth_client_model::book`.
            //
            // The client's `0xb4` arm is worth transcribing because the **second dword is not part
            // of the page list**, which a plainer reading of "book id, pages, inscription" loses:
            //
            // The `0x00B4` arm reads the book id and a separate maximum-page count before
            // unpacking the page list, inscription, scribe id and scribe name. It performs an
            // object lookup whose result is discarded, then raises the open-book notice with all
            // six decoded values in that order.
            //
            // **The object lookup is made and its answer thrown away** -- unlike `0x00A0`
            // just above, where the null test decides whether anything runs at all. So a `0x00B4`
            // for an object this client has never seen still opens the panel, and this arm has no
            // weenie guard for the same reason.
            Opcode::WRITING_BOOK_OPEN => {
                let Ok(m) = dereth_protocol::trade::WritingBookOpen::read(&mut r) else {
                    continue;
                };
                game.open_book(
                    m.book_id,
                    m.max_num_pages,
                    m.pages,
                    m.inscription,
                    m.scribe_id,
                    m.scribe_name,
                    &mut out,
                    ServerTime(inter.last_use_time.0),
                );
                inter.stats.books_opened += 1;
            }
            // The page-data response answers the
            // page-data request a page with no text included provokes. Wired with `0xb4`
            // because a book whose pages arrive empty is otherwise a window of blank pages, and
            // the two are one feature.
            Opcode::WRITING_BOOK_PAGE_DATA_RESPONSE => {
                let Ok(m) = dereth_protocol::trade::BookPageDataResponse::read(&mut r) else {
                    continue;
                };
                if game.book_page_data_response(m.object_id, m.page, m.data) {
                    inter.stats.book_pages_filled += 1;
                }
            }
            // **`0x00B6 Writing_BookAddPageResponse`, the third of the book's four.**
            //
            // The network dispatcher's `0x00B6` arm reads three consecutive dwords:
            // book id, page and success. Its notice reaches the book panel's add-page
            // response handler, the sole implementation among the 82 notice recipients.
            //
            // The handler is split across the seam — the two id refusals here,
            // the current-page check and the page-list insert in `dereth_ui_screens::panels::book`.
            Opcode::WRITING_BOOK_ADD_PAGE_RESPONSE => {
                let Ok(m) = dereth_protocol::trade::WritingBookAddPageResponse::read(&mut r) else {
                    continue;
                };
                inter.stats.book_add_page_responses += 1;
                if game.book_add_page_response(m.book_id, m.page_number, m.success != 0) {
                    inter.stats.book_add_pages_relayed += 1;
                }
            }
            // **`0x00B7 Writing_BookDeletePageResponse`: a counter, and the counter is
            // the transcription.**
            //
            // The `0x00B7` response carries the same three dwords as `0x00B6`, but its
            // notice has an empty implementation in **all 82** recipients. Retail receives
            // it without changing the page list; deleting a page here would invent behavior.
            //
            // The neighboring add-page notice is the positive control: it is empty in 81
            // recipients and reaches the book panel's real handler in the 82nd.
            Opcode::WRITING_BOOK_DELETE_PAGE_RESPONSE => {
                game.book_delete_page_response();
                inter.stats.book_delete_page_responses += 1;
            }

            // The answer to every `0x00C8 Item_Appraise`
            // this client sends, through the appraise handler and its highlight table.
            Opcode::ITEM_SET_APPRAISE_INFO => {
                let Ok(m) = dereth_protocol::objects::ItemSetAppraiseInfo::read(&mut r) else {
                    continue;
                };
                game.set_appraise_info(m.object, m.profile, &mut out);
                inter.stats.appraisals_applied += 1;
            }
            // **`0x01CB Item_AppraiseDone`, and the answer to *"what retail does after
            // the appraisal"* is: nothing.**
            //
            // The dispatcher validates the opcode, passes the body to a three-byte function that
            // returns zero, and performs no other work. The same folded function backs the
            // fellowship-update-done no-op; neither path has a substantive handler.
            //
            // So the arm is a counter, and that **is** the transcription: the appraisal itself is
            // `0x00C9 Item_SetAppraiseInfo`'s (the arm just above). `0x01CB` is the server saying
            // "that was the last one", and the client's
            // examination pane is already correct when it lands — exactly `0x01C9`'s shape.
            Opcode::ITEM_APPRAISE_DONE => {
                inter.stats.appraise_done += 1;
            }
            // **`0x00C3 Item_GetInscriptionResponse`: a *dead handler*, which is not
            // the same thing as a folded one.**
            //
            // The arm contains real code rather than a folded empty function: it constructs
            // three strings, skips four bytes before each of the first two strings, unpacks all
            // three, destroys them and returns zero.
            //
            // No notice, no handler call, nothing stored. It differs from the community catalogue
            // for this reason. The inscription box is fed by `0x00C8`/`0x00C9`, and this
            // message — which
            // the community catalogue calls `Communication_HearRangedSpeech`-family — feeds it
            // nothing in retail. An arm that wrote an inscription from it would be this client
            // inventing behaviour.
            //
            // The body **is** decoded, because retail unpacks it: a blob that does not decode is
            // a different event from one that does, and `dereth_protocol::items` reproduces the two
            // four-byte skips deliberately.
            Opcode::ITEM_GET_INSCRIPTION_RESPONSE => {
                let Ok(_m) = dereth_protocol::items::ItemGetInscriptionResponse::read(&mut r)
                else {
                    continue;
                };
                inter.stats.inscription_responses += 1;
            }
            // **`0xF630 Character_SetPlayerVisualDesc`, the third stub and the only
            // one of these sixteen that is not a game event.**
            //
            // It arrives on the UI queue as a bare message rather than a game event. The
            // dispatcher unpacks one narrow string and passes its buffer to a three-byte
            // return-only receiver. The call is direct, and every relevant notice slot except the
            // persistent-data object's unrelated implementation resolves to that same no-op.
            //
            // The player's appearance arrives through the already-wired `0xF625`
            // visual-description update carrying `ObjDesc`. `0xF630` instead carries a
            // string that the original client discards; ACE never sends it.
            Opcode::CHARACTER_SET_PLAYER_VISUAL_DESC => {
                let Ok(_m) = dereth_protocol::login::PlayerAppearanceMessage::read(&mut r) else {
                    continue;
                };
                inter.stats.player_visual_descs += 1;
            }

            // Commence-attack handling is the server's acknowledgement that the swing began.
            Opcode::COMBAT_HANDLE_COMMENCE_ATTACK_EVENT => {
                game.handle_commence_attack();
                inter.stats.attacks_commenced += 1;
            }
            // The attack-done handler, given `result`. The requests this can raise — the
            // auto-repeat re-fire — go out through `absorb` below, which is the same wire slot
            // every other request in this file uses.
            Opcode::COMBAT_HANDLE_ATTACK_DONE_EVENT => {
                let Ok(m) = dereth_protocol::combat::CombatHandleAttackDoneEvent::read(&mut r)
                else {
                    continue;
                };
                // Every arm of the attack-done handler reaches a
                // call that passes 1, so this is the attack flavour. Computed here rather than
                // taken as a parameter because `game` is in hand and the two inputs
                // (`player_motions_pending`, the bridged stance) are the previous frame's, which
                // is what `last_use_time` on the same line already is.
                let ready_for_attack = inter.ready_for_attack(game);
                game.handle_attack_done(&mut req, m.error, ready_for_attack, inter.last_use_time);
                inter.stats.attacks_done += 1;
            }
            // **The production writer of `CombatState::last_attacked_time`.** Defender
            // notification (`0x01B2`)
            // and the evasion-defender notification handler (`0x01B4`) share a
            // tail: stamp the clock, then auto-target if the option is on and nothing
            // is selected. See the spell-selection branch for both and for why this is recorded
            // here and run in [`use_time`].
            //
            // The chat half belongs to `hud.rs`. Both routes deliberately decode the
            // body: original UI dispatch decodes before calling a handler, so malformed
            // input must not reach this timestamp/auto-target tail. Counting a failed
            // decode here would stamp a clock that the original leaves unchanged.
            Opcode::COMBAT_HANDLE_DEFENDER_NOTIFICATION_EVENT => {
                if dereth_protocol::combat::DefenderNotification::read(&mut r).is_ok() {
                    inter.pending_defender_notifications += 1;
                }
            }
            Opcode::COMBAT_HANDLE_EVASION_DEFENDER_NOTIFICATION_EVENT => {
                if dereth_protocol::combat::EvasionDefenderNotification::read(&mut r).is_ok() {
                    inter.pending_defender_notifications += 1;
                }
            }
            // `Combat_QueryHealthResponse` — the selected creature's health bar.
            Opcode::COMBAT_QUERY_HEALTH_RESPONSE => {
                let Ok(m) = dereth_protocol::combat::CombatQueryHealthResponse::read(&mut r) else {
                    continue;
                };
                if game.update_object_health(m.object, m.health) {
                    inter.stats.selection_meters_written += 1;
                }
                inter.stats.health_responses += 1;
            }
            // `Item_QueryItemManaResponse` — the selected item's mana bar.
            Opcode::ITEM_QUERY_ITEM_MANA_RESPONSE => {
                let Ok(m) = dereth_protocol::items::ItemQueryItemManaResponse::read(&mut r) else {
                    continue;
                };
                if game.update_item_mana(m.object, m.mana, m.success != 0) {
                    inter.stats.selection_meters_written += 1;
                }
                inter.stats.mana_responses += 1;
            }
            // `Item_UseDone` — the universal "action finished" acknowledgement,
            // which is what takes the busy cursor back off. The failure half of the same handler
            // is a chat line and lives in `hud.rs`, which owns the chat scroll.
            Opcode::ITEM_USE_DONE => {
                let Ok(m) = dereth_protocol::objects::ItemUseDone::read(&mut r) else {
                    continue;
                };
                game.use_done(m.failure_type);
                inter.stats.uses_done += 1;
            }

            // `Allegiance_AllegianceUpdate` — a full replace of the tree.
            Opcode::ALLEGIANCE_ALLEGIANCE_UPDATE => {
                let Ok(m) = dereth_protocol::social::AllegianceUpdate::read(&mut r) else {
                    continue;
                };
                let n = game.handle_allegiance_update(&m.profile);
                inter.stats.allegiance_members = n;
                inter.stats.allegiance_updates += 1;
            }
            // **`0x0148` and `0x0149`, and the names are the other way round from
            // what they read like.**
            //
            // Both dispatchers decode a count-prefixed list of narrow strings. Their
            // handlers differ only in the heading:
            //
            // * `0x0148 ChannelList`: "The following characters are currently listening on the channel:"
            // * `0x0149 ChannelIndex`: "The following channels are available to you:"
            //
            // Thus ChannelList lists characters, while ChannelIndex lists channels. Each
            // handler widens and prints the heading, then prints every entry with three
            // leading spaces and a trailing newline on chat type 0.
            //
            // These replies feed chat output, not a channel-selector widget. An empty
            // list still prints its heading, proving the command ran.
            Opcode::COMMUNICATION_CHANNEL_LIST => {
                let Ok(m) = dereth_protocol::comms::CommunicationChannelListRecv::read(&mut r)
                else {
                    continue;
                };
                inter.stats.channel_lists += 1;
                inter.stats.channel_rows += channel_report(
                    game,
                    "The following characters are currently listening on the channel:\n",
                    &m.names,
                );
            }
            Opcode::COMMUNICATION_CHANNEL_INDEX => {
                let Ok(m) = dereth_protocol::comms::CommunicationChannelIndexRecv::read(&mut r)
                else {
                    continue;
                };
                inter.stats.channel_indices += 1;
                inter.stats.channel_rows += channel_report(
                    game,
                    "The following channels are available to you:\n",
                    &m.names,
                );
            }
            // **`0x01C3 Character_QueryAgeResponse`, `/age`'s answer.**
            //
            // The query-age dispatcher unpacks **two** narrow strings and hands them to a
            // two-branch formatter. It tests the target-name buffer length against 1. An empty
            // target formats the self-age sentence; a nonempty target formats the named-player
            // sentence. Both are written to chat type 0.
            //
            // **Retail's stored-length-is-1 test is an emptiness test, not a length-one test**: the
            // stored length counts the terminator, so `1` is the empty string. That is
            // what `dereth_protocol::admin`'s *"an empty target name means self"* already says, and it
            // is the whole of the two-arm split — the server sends the name back only for somebody
            // else's `/age`.
            Opcode::CHARACTER_QUERY_AGE_RESPONSE => {
                let Ok(m) = dereth_protocol::admin::CharacterQueryAgeResponse::read(&mut r) else {
                    continue;
                };
                let line = if m.target_name.is_empty() {
                    format!("You have played for {}.\n", m.age)
                } else {
                    format!("{} has played for {}.\n", m.target_name, m.age)
                };
                game.scroll.add_text_to_scroll(
                    &line,
                    dereth_client_model::chat::text_type::DEFAULT,
                    true,
                    0,
                );
                inter.stats.age_responses += 1;
            }
            // **The four portal-storm notices, `0x02C9`…`0x02CC`.**
            //
            // The portal-storm brewing and imminent dispatchers pass the dword after the opcode as
            // a **float**
            // to handlers taking one argument; storm and subsided tail-call handlers taking
            // none. `dereth_client_model::portal_storm` carries the four bodies and the three
            // differences between them; this arm is the routing and the counters.
            //
            // Note the guard the four dispatchers share and this router already satisfies:
            // they compare the UI-system pointer and then the opcode.
            // A blob whose first dword is not the expected opcode returns 0 without
            // touching anything, which is `dereth_client_net::client_session`'s dispatch here.
            Opcode::MISC_PORTAL_STORM_BREWING => {
                let Ok(m) = dereth_protocol::trade::MiscPortalStormBrewing::read(&mut r) else {
                    continue;
                };
                game.portal_storm_brewing(m.extent);
                inter.stats.portal_storms_brewing += 1;
                inter.stats.portal_storm_levels += 1;
            }
            Opcode::MISC_PORTAL_STORM_IMMINENT => {
                let Ok(m) = dereth_protocol::trade::MiscPortalStormImminent::read(&mut r) else {
                    continue;
                };
                game.portal_storm_imminent(m.extent);
                inter.stats.portal_storms_imminent += 1;
                inter.stats.portal_storm_levels += 1;
            }
            Opcode::MISC_PORTAL_STORM => {
                game.portal_storm_struck();
                inter.stats.portal_storms_struck += 1;
                inter.stats.portal_storm_levels += 1;
            }
            Opcode::MISC_PORTAL_STORM_SUBSIDED => {
                game.portal_storm_subsided();
                inter.stats.portal_storms_subsided += 1;
                inter.stats.portal_storm_levels += 1;
            }
            // **`0x027C Allegiance_AllegianceInfoResponseEvent` is chat output, not the
            // allegiance panel.**
            //
            // The dispatcher reads a target id and complete allegiance profile, then
            // prints the five response lines as `(text, 0, true, 0)`. This is the chat
            // answer to `/allegiance info`; it changes neither the panel nor the stored
            // allegiance hierarchy.
            Opcode::ALLEGIANCE_ALLEGIANCE_INFO_RESPONSE_EVENT => {
                let Ok(m) = dereth_protocol::social::AllegianceInfoResponse::read(&mut r) else {
                    continue;
                };
                inter.stats.allegiance_info_responses += 1;
                if game.allegiance_info_response(m.target, &m.profile) {
                    inter.stats.allegiance_info_reports += 1;
                }
            }
            // **`0x0003 Allegiance_AllegianceUpdateAborted`.**
            //
            // The update-aborted dispatcher calls the handler, which raises a notice whose only
            // receiver in retail is the allegiance panel: it updates only when visible, the
            // `u32` ignored.
            //
            // **Not to be confused with `0x01C8 AllegianceAllegianceUpdateDone`**, which this
            // client has no switch arm for at all — ACE sends it, and the recorded sessions carry
            // 77 of them. `0x0003`
            // is a different message that retail *does* handle.
            Opcode::ALLEGIANCE_ALLEGIANCE_UPDATE_ABORTED => {
                let Ok(m) = dereth_protocol::social::AllegianceUpdateAborted::read(&mut r) else {
                    continue;
                };
                game.allegiance_update_aborted(m.failure_type);
                inter.stats.allegiance_updates_aborted += 1;
            }

            // ---- the confirmation seam, `0x0274` / `0x0276` --------------------
            //
            // The `0x0274` dispatcher decodes the type, context and text, then hands them to a
            // seven-arm `switch` on the **type** with no `default`:
            //
            // ```text
            // case 1: raise an allegiance-swear request
            // case 2: raise an alter-skill confirmation
            // case 3: raise an alter-attribute confirmation
            // case 4: raise a fellowship request
            // case 5: raise a craft-interaction confirmation
            // case 6: raise an augmentation confirmation
            // case 7: raise a general yes/no confirmation
            // ```
            //
            // Without this arm the question never appears, nothing is sent, and ACE's
            // thirty-second `ConfirmationManager` timeout aborts it.
            // The answer half of the link-status panel's round trip.
            //
            // Return-ping dispatch raises the ping notice. Its only handler computes the
            // round-trip time from the current time and the last request time. The body is empty,
            // so the *arrival* is the whole message and a counter is a faithful carrier;
            // `crate::net::ping_holder`'s own doc says why it is a count and not a timestamp.
            //
            // The three `fellowship-*` captures carry this opcode 0/0/0 times, and the reason
            // is that it is "an answer to a request the retail client
            // never made in these three sessions". Retail only pings while the link-status panel
            // is open, and it was not.
            Opcode::CHARACTER_RETURN_PING => {
                crate::net::ping_holder::returned();
                continue;
            }
            Opcode::CHARACTER_CONFIRMATION_REQUEST => {
                let Ok(m) = dereth_protocol::comms::CharacterConfirmationRequest::read(&mut r)
                else {
                    continue;
                };
                // `is_handled` is `1..=7` — the `switch`'s arms, and nothing else.
                if !dereth_protocol::comms::CharacterConfirmationRequest::is_handled(
                    m.confirmation_type,
                ) {
                    inter.stats.confirmations_unknown_type += 1;
                    continue;
                }
                // **Type 1.** The allegiance panel turns it into an
                // accept-swear confirmation — the question a **monarch or
                // patron** is asked when somebody swears to them. Dropped, nobody could
                // ever gain a vassal in this client: the question would never appear and ACE's
                // thirty-second `ConfirmationManager` timeout would abort it.
                //
                // It goes to its own queue rather than to `pending_server_confirmations`, because
                // the dialog is the *panel's* and has its own context slot
                // (the accept-swear context): a gameplay confirmation already on screen must not
                // suppress it, which sharing the gameplay confirmation context would do.
                //
                // Type 4 is the fellowship invitation handled separately below.
                if m.confirmation_type == 1 {
                    inter.pending_swear_requests.push((m.context_id, m.text));
                    inter.stats.confirmations_raised += 1;
                    continue;
                }
                // **Type 4.** A fellowship invitation goes to its own
                // queue, separate from gameplay and allegiance confirmations. Their dialog
                // contexts are independent, so one can remain open while another is shown.
                if m.confirmation_type == 4 {
                    inter
                        .pending_fellowship_requests
                        .push((m.context_id, m.text));
                    inter.stats.fellowship_requests_raised += 1;
                    continue;
                }
                // Both panels exist, so no type is dropped as having an absent panel; control
                // falls through to the ordinary server-confirmation path. The
                // `confirmations_for_absent_panels` field is kept and simply stays 0, which a
                // test asserts.
                inter.stats.confirmations_raised += 1;
                inter.pending_server_confirmations.push((
                    m.confirmation_type,
                    m.context_id,
                    m.text,
                ));
            }

            // Confirmation-done handling raises one abort notice carrying `(type, context)`. It
            // represents the server taking
            // the question back, which on this shard is `ConfirmationManager.EnqueueAbort` after
            // thirty seconds. **The client has no timer of its own on this path**; this message is
            // the only thing that takes an unanswered confirmation down.
            Opcode::CHARACTER_CONFIRMATION_DONE => {
                let Ok(m) = dereth_protocol::comms::CharacterConfirmationDone::read(&mut r) else {
                    continue;
                };
                inter
                    .pending_confirmation_aborts
                    .push((m.confirmation_type, m.context_id));
            }

            // Vendor-info handling opens the shop. This is the active vendor id's writer;
            // without it `toolbar::splitter`'s vendor arm and `use_object`'s two vendor guards
            // would be inert.
            Opcode::VENDOR_VENDOR_INFO => {
                let Ok(m) = dereth_protocol::trade::VendorInfo::read(&mut r) else {
                    continue;
                };
                let n = game.handle_vendor_info(
                    &m,
                    &mut out,
                    &mut req,
                    ServerTime(inter.last_use_time.0),
                );
                inter.stats.vendor_stock = n;
                inter.stats.vendor_opens += 1;
            }

            // ---- the ten server-to-client trade messages -------------------
            //
            // Without these handlers the trade state would be a field nothing writes, and this
            // client could open a negotiation and then see nothing at all.
            //
            // **The recorded corpus carries none of these.** A calibrated scan of all seven
            // captures over 994 server game events and 2,564 client actions finds 0 of every
            // trade opcode, against the vendor family's 8 + 5 + 1 in the same scan. Everything
            // below is exercised from synthesised messages until a capture contains the trade flow.
            Opcode::TRADE_REGISTER_TRADE => {
                let Ok(m) = dereth_protocol::trade::TradeRegisterTrade::read(&mut r) else {
                    continue;
                };
                if game.handle_register_trade(&m, &mut out, ServerTime(inter.last_use_time.0)) {
                    inter.stats.trade_registers += 1;
                }
            }
            Opcode::TRADE_OPEN_TRADE => {
                let Ok(m) = dereth_protocol::trade::TradeOpenTrade::read(&mut r) else {
                    continue;
                };
                game.handle_open_trade(m.source, &mut out);
                inter.stats.trade_opens += 1;
            }
            Opcode::TRADE_CLOSE_TRADE => {
                let Ok(m) = dereth_protocol::trade::TradeCloseTrade::read(&mut r) else {
                    continue;
                };
                game.handle_close_trade(m.reason, ServerTime(inter.last_use_time.0), &mut out);
                inter.stats.trade_closes += 1;
            }
            Opcode::TRADE_ADD_TO_TRADE_RECV => {
                let Ok(m) = dereth_protocol::trade::TradeAddToTradeRecv::read(&mut r) else {
                    continue;
                };
                if game.handle_add_to_trade(&m, &mut out) {
                    inter.stats.trade_rows_changed += 1;
                }
            }
            Opcode::TRADE_REMOVE_FROM_TRADE => {
                let Ok(m) = dereth_protocol::trade::TradeRemoveFromTrade::read(&mut r) else {
                    continue;
                };
                if game.handle_remove_from_trade(&m, ServerTime(inter.last_use_time.0), &mut out) {
                    inter.stats.trade_rows_changed += 1;
                }
            }
            Opcode::TRADE_ACCEPT_TRADE_RECV => {
                let Ok(m) = dereth_protocol::trade::TradeAcceptTradeRecv::read(&mut r) else {
                    continue;
                };
                game.handle_accept_trade(m.source, &mut out);
                inter.stats.trade_flag_messages += 1;
            }
            Opcode::TRADE_DECLINE_TRADE_RECV => {
                let Ok(m) = dereth_protocol::trade::TradeDeclineTradeRecv::read(&mut r) else {
                    continue;
                };
                game.handle_decline_trade(m.source, &mut out);
                inter.stats.trade_flag_messages += 1;
            }
            Opcode::TRADE_RESET_TRADE_RECV => {
                let Ok(m) = dereth_protocol::trade::TradeResetTradeRecv::read(&mut r) else {
                    continue;
                };
                game.handle_reset_trade(m.source, ServerTime(inter.last_use_time.0), &mut out);
                inter.stats.trade_flag_messages += 1;
            }
            Opcode::TRADE_TRADE_FAILURE => {
                let Ok(m) = dereth_protocol::trade::TradeTradeFailure::read(&mut r) else {
                    continue;
                };
                game.handle_trade_failure(&m, &mut out);
                inter.stats.trade_flag_messages += 1;
            }
            // The anti-scam message. Its handler raises the notice and **clears nothing**; the
            // `0x0202`s the server sends alongside it do that.
            Opcode::TRADE_CLEAR_TRADE_ACCEPTANCE => {
                game.handle_clear_trade_acceptance(ServerTime(inter.last_use_time.0), &mut out);
                inter.stats.trade_flag_messages += 1;
            }

            // Update and removal both operate on the **player's** enchantment registry; the
            // client keeps exactly one such registry.
            // Both use the counted forms:
            // adding an enchantment to the list ends by updating the spell totals by +1 and
            // removing one by -1, so the helpful and harmful enchantment counts have a
            // production writer and the buff/debuff indicator is not stuck at 0/0.
            Opcode::MAGIC_UPDATE_ENCHANTMENT => {
                let Ok(m) = dereth_protocol::qualities::MagicUpdateEnchantment::read(&mut r) else {
                    continue;
                };
                let now = inter.last_use_time;
                if game.update_player_enchantment(&m.0, now) {
                    inter.stats.enchantments_updated += 1;
                }
                // **The receiver for live enchantment changes.** After updating the registry,
                // native handling tests the enchantment's
                // `VITAE` bit. Vitae raises `VitaeChanged`; every other enchantment raises
                // `EnchantmentsChanged`. The stat-management panel listens to the latter and
                // refreshes every skill row. Without the notice, a buff that landed **live** would
                // update the registry while the Skills page went on drawing the join it had built
                // at `0x0013`. The attribute rows need no notice because `HudView::attribute` asks
                // the attribute lookup on every read; the skills page is a cached join and needs
                // the notice.
                //
                // Keep the original branch distinct: a vitae enchantment raises the other
                // notice, to which the stat-management panel does not subscribe.
                let vitae =
                    m.0.smod.kind & dereth_protocol::types::qualities::enchantment_type::VITAE != 0;
                let notice = if vitae {
                    Notice::VitaeChanged
                } else {
                    Notice::EnchantmentsChanged
                };
                panels(inter, game, &notice);
            }
            // Removal (`0x02C3`) and dispel (`0x02C7`)
            // have identical wire bodies, as the protocol round-trip test confirms, and share one
            // storage in the world, which avoids duplicate updates and spell-total recomputation.
            //
            // **The expiry line.** The shared removal operation
            // notifies only when its Boolean argument is true: removal passes true, while
            // dispel passes false. Expiry therefore prints a line such as
            // "Strength Self VI has expired." and dispel remains silent.
            Opcode::MAGIC_REMOVE_ENCHANTMENT | Opcode::MAGIC_DISPEL_ENCHANTMENT => {
                let announce = *opcode == Opcode::MAGIC_REMOVE_ENCHANTMENT;
                let id = if announce {
                    let Ok(m) = dereth_protocol::qualities::MagicRemoveEnchantment::read(&mut r)
                    else {
                        continue;
                    };
                    m.layered_spell_id
                } else {
                    let Ok(m) = dereth_protocol::qualities::MagicDispelEnchantment::read(&mut r)
                    else {
                        continue;
                    };
                    m.layered_spell_id
                };
                // The `Magic_RemoveEnchantment` handler's notice half,
                // which is an **either/or** and not a pair. Retail reads the current
                // vitae enchantment before removing the requested id. A matching vitae id raises
                // `VitaeChanged`; every other id raises `EnchantmentsChanged`.
                //
                // The test is read **before** the removal, because afterwards the vitae is gone.
                let was_vitae = game
                    .player_qualities()
                    .and_then(|q| q.enchantments.vitae)
                    .is_some_and(|v| v.id == id);
                if game.remove_player_enchantment(id) {
                    inter.stats.enchantments_removed += 1;
                }
                let notice = if was_vitae {
                    Notice::VitaeChanged
                } else {
                    Notice::EnchantmentsChanged
                };
                panels(inter, game, &notice);
                if announce && game.notify_of_enchantment_removal(id) {
                    inter.stats.enchantment_expiry_lines += 1;
                }
            }
            // **`0x02C4 Magic_UpdateMultipleEnchantments`, the count-prefixed form of
            // the arm above.**
            //
            // The dispatcher validates `0x02C4`, unpacks a count-prefixed enchantment list and
            // passes that list to the update handler.
            //
            // **The layout is `PackableList`'s and nothing more** — a `u32` count followed by that
            // many `Enchantment`s. There
            // is no per-entry header and no trailing word; `0x02C4` is `0x02C2`'s body with a count
            // in front of it, and `dereth_protocol::qualities::MagicUpdateMultipleEnchantments` reads
            // it that way.
            Opcode::MAGIC_UPDATE_MULTIPLE_ENCHANTMENTS => {
                let Ok(m) =
                    dereth_protocol::qualities::MagicUpdateMultipleEnchantments::read(&mut r)
                else {
                    continue;
                };
                let now = inter.last_use_time;
                let applied = game.update_player_enchantments(&m.0, now, &mut out);
                inter.stats.multi_enchantment_updates += 1;
                inter.stats.multi_enchantments_applied += applied as u64;
                // The handler raises `EnchantmentsChanged` once, after
                // the whole list, unconditional, and never the vitae one however many vitae
                // entries the list held. That asymmetry against the single arm above belongs to
                // the retail handler.
                panels(inter, game, &Notice::EnchantmentsChanged);
            }
            // **`0x02C5` and `0x02C8`, one handler and one `bool`.**
            //
            // Both dispatchers read a count-prefixed list of `u32` enchantment ids and
            // call the same removal operation. `0x02C5` supplies notify=true; `0x02C8`
            // supplies notify=false through the dispel path.
            //
            // The `0x02C3`/`0x02C7` pair above has exactly this shape one layer down, which is why
            // the two arms are written the same way: the *silence* of a dispel is the transcription
            // and folding the two opcodes into one arm would lose it.
            Opcode::MAGIC_REMOVE_MULTIPLE_ENCHANTMENTS
            | Opcode::MAGIC_DISPEL_MULTIPLE_ENCHANTMENTS => {
                let announce = *opcode == Opcode::MAGIC_REMOVE_MULTIPLE_ENCHANTMENTS;
                let ids = if announce {
                    let Ok(m) =
                        dereth_protocol::qualities::MagicRemoveMultipleEnchantments::read(&mut r)
                    else {
                        continue;
                    };
                    m.0
                } else {
                    let Ok(m) =
                        dereth_protocol::qualities::MagicDispelMultipleEnchantments::read(&mut r)
                    else {
                        continue;
                    };
                    m.0
                };
                let (removed, announced) =
                    game.remove_player_enchantments(&ids, announce, &mut out);
                if announce {
                    inter.stats.multi_enchantment_removals += 1;
                } else {
                    inter.stats.multi_enchantment_dispels += 1;
                }
                inter.stats.multi_enchantments_removed += removed as u64;
                inter.stats.enchantment_expiry_lines += announced as u64;
                // `EnchantmentsChanged` is unconditional and occurs *before*
                // the per-id notification loop; `VitaeChanged` is a second, separate raise on
                // this handler rather than an
                // alternative to it.
                panels(inter, game, &Notice::EnchantmentsChanged);
            }
            // **The two purges' only trigger.**
            // The `0x02C6` and `0x0312` dispatcher arms reach the normal and harmful-only purge
            // handlers. Their bodies perform the purge and then raise
            // the enchantments-changed and vitae-changed notices. **Both messages have
            // empty bodies**, so there is nothing to decode and no `dereth_protocol` type is needed.
            //
            // There is no client-side death hook: the client purges when the **server** says so,
            // and these two opcodes are the whole mechanism.
            Opcode::MAGIC_PURGE_ENCHANTMENTS => {
                if game.purge_enchantments(&mut out) {
                    inter.stats.enchantments_purged += 1;
                }
                // Both handlers raise **both** notices, unconditionally — this is
                // the one place in the family where they are a pair rather than a choice.
                panels(inter, game, &Notice::EnchantmentsChanged);
                panels(inter, game, &Notice::VitaeChanged);
            }
            Opcode::MAGIC_PURGE_BAD_ENCHANTMENTS => {
                if game.purge_bad_enchantments(&mut out) {
                    inter.stats.bad_enchantments_purged += 1;
                }
                panels(inter, game, &Notice::EnchantmentsChanged);
                panels(inter, game, &Notice::VitaeChanged);
            }
            // **Public world-object quality updates.** The twenty-six update
            // opcodes in `0x02CD`..=`0x02EA` include public and private forms. This route
            // accepts only decoded public forms, identified by a present subject id.
            //
            // Without this route the player-only HUD route would leave other objects without an
            // update consumer, and a chest could remain locked locally after the server unlocked
            // it: mirroring Boolean quality 3 updates the public description's openable flag
            // to the inverse value, which the local use guard reads.
            //
            // Private forms have no subject id and are handled by the player route. Public
            // forms naming the player can be offered to both routes, but the
            // world owns one shared player-quality store and sequence gate, so there is no
            // separate HUD quality copy to maintain.
            op if dereth_client_model::qualities::update::is_update_opcode(op) => {
                let Some(u) = dereth_client_model::qualities::update::decode(op, body) else {
                    continue;
                };
                let Some(subject) = u.subject else { continue };
                if game.apply_stat_update(subject, u.key, u.value, u.sequence, &mut out) {
                    inter.stats.object_quality_updates += 1;
                } else {
                    inter.stats.object_quality_updates_stale += 1;
                }
            }
            // **The `Remove` half of the arm above, and the same partition.**
            //
            // Stat removal looks up the wire's object id, not just the player. The eight
            // public forms therefore enter this route; the eight private forms, with no
            // subject id, use `Hud::apply_quality_remove`. This is the world-removal
            // caller.
            //
            // Removal deliberately does not run the `PublicWeenieDesc` mirror. Unlike
            // updates, none of the eight observed removal paths invokes that mirror, so
            // removing ItemType or Burden leaves its mirrored value unchanged.
            // `apply_stat_remove` preserves that omission.
            op if dereth_client_model::qualities::remove::is_remove_opcode(op) => {
                let Some(r) = dereth_client_model::qualities::remove::decode(op, body) else {
                    continue;
                };
                let Some(subject) = r.subject else { continue };
                if game.apply_stat_remove(subject, r.key, r.sequence, &mut out) {
                    inter.stats.object_quality_removes += 1;
                } else {
                    inter.stats.object_quality_removes_stale += 1;
                }
            }
            // **Range registration after a slumlord profile.** Hud retains the
            // profile for display. This router also decodes the message, then uses its
            // covenant-crystal id and the world range list's clock to register the watch.
            Opcode::HOUSE_HOUSE_PROFILE => {
                let Ok(m) = dereth_protocol::trade::HouseProfileMessage::read(&mut r) else {
                    continue;
                };
                game.register_slumlord_range_check(
                    m.covenant_crystal,
                    ServerTime(inter.last_use_time.0),
                );
            }
            // **`0x0004 Communication_PopUpString` tutorial message boxes.**
            //
            // The recorded logins include three across the fellowship recordings and 26
            // across seven session captures.
            //
            // The dispatcher decodes one narrow string, widens it as literal text and
            // builds a property collection. The properties are:
            //
            // * `0x8E = 3`: `DialogKind::Message`, the one-button message dialog,
            //   element type `0x17`, rather than a yes/no question.
            // * `0xC3 = 1`: `dialog::factory::NON_QUEUED`, the all-at-once list, whose production
            //   caller this message is. A recorded long solo session contains 18 such prompts; five
            //   arriving together must show five boxes rather than queue four.
            // * `0xC5`: the prompt text displayed by child `0x3E`
            //   (`dialog::base::child::TEXT`), shared with other dialog builders.
            //
            // No callback or modal property `0xAC` is installed, so these boxes do not
            // block world clicks. Vendor, fellowship and allegiance questions are also
            // nonmodal; gameplay confirmations differ.
            //
            // This function has a mutable world borrow but no UI system. It queues the
            // text for [`crate::target_confirmation::TargetedDialogs`], which owns the
            // dialog controller. The router provides that factory path; the neighboring
            // communication handlers in `Hud::ui_event` do not.
            //
            // Event `0x0318` carries the same one string and reaches the same pop-up.
            Opcode::COMMUNICATION_POP_UP_STRING | Opcode::COMMUNICATION_POP_UP_STRING_0318 => {
                let Ok(m) = dereth_protocol::comms::CommunicationPopUpString::read(&mut r) else {
                    inter.stats.pop_up_strings_undecodable += 1;
                    continue;
                };
                inter.pending_pop_up_strings.push(m.message);
                inter.stats.pop_up_strings += 1;
            }
            // The third wildcard the UI queue can land in. `Hud::ui_event` and this function both
            // see every `SessionEvent::UiEvent`, so an opcode is only *dropped* when it falls
            // through **both**; the ledger records per site and a test takes the intersection.
            _ => {
                crate::dropped::record(crate::dropped::Site::Interaction, *opcode);
            }
        }
        // Once the blocking move completes, send the deferred wield owed by the paper
        // doll. Any required split uses this interaction's retained splitter state.
        if let Some((object, container)) = moved {
            inter.on_item_moved(game, object, container, &mut out, &mut req);
        }
        for notice in std::mem::take(&mut out.moved_for_panels) {
            panels(inter, game, &notice);
        }
    }
    inter.absorb(game, out, req);
}

/// The action ids this module handles, from the shipped `ActionMap`'s own enum names
/// (`crate::actions::names::ACTION_ENUM_NAMES`).
pub mod action {
    /// `CombatToggleCombat`.
    pub const COMBAT_TOGGLE_COMBAT: u32 = 0x1000_005A;
    /// `CombatLowAttack`.
    pub const COMBAT_LOW_ATTACK: u32 = 0x1000_005D;
    /// `CombatMediumAttack`.
    pub const COMBAT_MEDIUM_ATTACK: u32 = 0x1000_005E;
    /// `CombatHighAttack`.
    pub const COMBAT_HIGH_ATTACK: u32 = 0x1000_005F;

    // ---- the rest of the combat-action handler's press and release arms ---------
    //
    // The ids are the action name table's own, pinned here as literals because
    // reading a constant back through the same symbol that wrote it is unfalsifiable.
    // A test checks them
    // against the shipped `ActionMap` rather than against this list.
    //
    // **The gauge keys and the height keys are one set of physical keys per mode**: the shipped
    // defaults bind `DIK_INSERT`/`DIK_PRIOR`/`DIK_DELETE`/`DIK_END`/`DIK_NEXT` in `MeleeCombat`,
    // in `MissileCombat` and in `MagicCombat`, and the input-map registration keeps exactly
    // one of the three registered. That is why there is one handler and not three.

    /// `CombatDecreaseAttackPower` — `DIK_INSERT` in `MeleeCombat`.
    pub const COMBAT_DECREASE_ATTACK_POWER: u32 = 0x1000_005B;
    /// `CombatIncreaseAttackPower` — `DIK_PRIOR` in `MeleeCombat`.
    pub const COMBAT_INCREASE_ATTACK_POWER: u32 = 0x1000_005C;
    /// `CombatDecreaseMissileAccuracy` — `DIK_INSERT` in `MissileCombat`, and it shares
    /// the action-handler case with [`COMBAT_DECREASE_ATTACK_POWER`].
    pub const COMBAT_DECREASE_MISSILE_ACCURACY: u32 = 0x1000_00EF;
    /// `CombatIncreaseMissileAccuracy` — `DIK_PRIOR` in `MissileCombat`.
    pub const COMBAT_INCREASE_MISSILE_ACCURACY: u32 = 0x1000_00F0;
    /// `CombatAimLow` — `DIK_DELETE` in `MissileCombat`, the low attack height.
    pub const COMBAT_AIM_LOW: u32 = 0x1000_00F1;
    /// `CombatAimMedium` — `DIK_END` in `MissileCombat`, the medium attack height.
    pub const COMBAT_AIM_MEDIUM: u32 = 0x1000_00F2;
    /// `CombatAimHigh` — `DIK_NEXT` in `MissileCombat`, the high attack height.
    pub const COMBAT_AIM_HIGH: u32 = 0x1000_00F3;

    // ---- `handle_magic_action`'s eighteen ---------------------------------

    /// `CombatCastCurrentSpell` — `DIK_END` in `MagicCombat`.
    pub const COMBAT_CAST_CURRENT_SPELL: u32 = 0x1000_0060;
    /// `CombatPrevSpell` — `DIK_DELETE`.
    pub const COMBAT_PREV_SPELL: u32 = 0x1000_0061;
    /// `CombatNextSpell` — `DIK_NEXT`.
    pub const COMBAT_NEXT_SPELL: u32 = 0x1000_0062;
    /// `CombatPrevSpellTab` — `DIK_INSERT`.
    pub const COMBAT_PREV_SPELL_TAB: u32 = 0x1000_0063;
    /// `CombatNextSpellTab` — `DIK_PRIOR`.
    pub const COMBAT_NEXT_SPELL_TAB: u32 = 0x1000_0064;
    /// `UseSpellSlot_1` — `DIK_1`, and **slot 0**: the client's slot is `action - 0x10000065`.
    pub const USE_SPELL_SLOT_FIRST: u32 = 0x1000_0065;
    /// The last case of the twelve-entry spell quickslot run. The shipped `MagicCombat` map binds
    /// only through `UseSpellSlot_9` (`0x1000006D`); the remaining three are still bindable.
    pub const USE_SPELL_SLOT_LAST: u32 = 0x1000_0070;
    /// `CombatFirstSpell` — `DIK_LCONTROL+DIK_RCONTROL+DIK_DELETE`.
    pub const COMBAT_FIRST_SPELL: u32 = 0x1000_0102;
    /// `CombatLastSpell`.
    pub const COMBAT_LAST_SPELL: u32 = 0x1000_0103;
    /// `CombatFirstSpellTab`.
    pub const COMBAT_FIRST_SPELL_TAB: u32 = 0x1000_0104;
    /// `CombatLastSpellTab`.
    pub const COMBAT_LAST_SPELL_TAB: u32 = 0x1000_0105;
    /// `SelectionExamine`.
    pub const SELECTION_EXAMINE: u32 = 0x1000_002B;
    /// `USE`.
    pub const USE: u32 = 0x1000_0025;
    /// `SelectionPickUp` — the shipped default key map's `DIK_F`.
    ///
    /// The player-action handler's case 1: with a selected object it places that object in the
    /// backpack and consumes the action; with no selection the action is **not** consumed.
    ///
    /// Its next neighbour is `SelectionSplitStack` (case 2), transcribed below;
    /// `SelectionPreviousSelection` (case 3) remains unconsumed here.
    pub const SELECTION_PICK_UP: u32 = 0x1000_002C;
    /// `SelectionSplitStack` — `DIK_T` in `ItemSelectionCommands`. Case 2 sends the selected id
    /// to the split-stack receiver and consumes even the zero-id no-op.
    pub const SELECTION_SPLIT_STACK: u32 = 0x1000_002D;

    // ---- six UI-system action arms -----------------------------
    //
    // Five actions are selected through a 117-entry byte table indexed by `action - 7`.
    // Table entries 0 through 3 select the click, Escape, screenshot and help handlers; entry 4
    // is the `return false` fallback used by the other 112 indices. `0x7C` has its own branch
    // before the table, while the three high action ids use a subtraction chain after it.

    /// `SelectLeft` — **the left mouse button**, not a keyboard select: `crate::actions::ui`
    /// names the same id `PRIMARY_CLICK` and `fire::KEYSTONE_SUPPRESSED_ACTIONS` lists 7, 8, 10 and
    /// 11 as the click family. Both click actions share the same arm.
    pub const SELECT_LEFT: u32 = 0x0000_0007;
    /// `SelectRight` — the right mouse button; it shares an arm with [`SELECT_LEFT`].
    pub const SELECT_RIGHT: u32 = 0x0000_0008;
    /// `EscapeKey` — the longest of the eight arms.
    pub const ESCAPE_KEY: u32 = 0x0000_0027;
    /// `CaptureScreenshot` — the screenshot arm.
    pub const CAPTURE_SCREENSHOT: u32 = 0x0000_0055;

    /// The four actions of input map `0x10`, the **system-key swallow**.
    ///
    /// UI initialization is the only site that registers map `0x10`. It installs the client's
    /// input-action callback at priority -1, one below the normal lowest priority. That callback
    /// returns true without performing any other work.
    ///
    /// So retail's arm for all four *is* the registration: consume the action, do nothing, and —
    /// because the callback answered `TRUE` — deny it to the input-handler chain behind
    /// the action broadcast, which is where the element manager's
    /// global-message-1 broadcast of the action and the visibility toggle live. At priority −1 the
    /// map is last in the walk, so any map that binds the same control still wins; this is the
    /// floor, not a barrier.
    ///
    /// **What actually happens to each of the four keys is decided in the window procedure,
    /// not here**, and this build already reproduces that half in
    /// the device input's system-key rule — the system-keys-enabled flag is always false, so a
    /// `WM_SYSKEY*` is swallowed after being forwarded to the input manager *except* for two:
    ///
    /// * **Alt+Tab** — swallowed; the client's window never passes it to `DefWindowProc`.
    /// * **Alt+Enter** — **not** swallowed: it sets the toggle-full-screen flag, and
    ///   the event loop's epilogue performs the flip. So yes, Alt+Enter *is*
    ///   retail's full-screen toggle — through the message pump, never through this action.
    /// * **Alt+F4** — **not** swallowed: it falls through to `DefWindowProc`, which closes the
    ///   window.
    /// * **Ctrl+Shift+Esc** — not a `WM_SYSKEY*` at all (no Alt), so the rule above never sees it;
    ///   the shell takes it before the window does.
    ///
    /// The arm below is therefore the *action* half only, and it is deliberately empty.
    pub const SYSTEM_ALT_TAB: u32 = 0x0000_0053;
    /// See [`SYSTEM_ALT_TAB`]. `WM_SYSKEYDOWN VK_RETURN` is retail's full-screen toggle, in the
    /// pump; the action itself does nothing.
    pub const SYSTEM_ALT_ENTER: u32 = 0x0000_007D;
    /// See [`SYSTEM_ALT_TAB`]. `WM_SYSKEYDOWN VK_F4` reaches `DefWindowProc` and closes the window;
    /// the action itself does nothing.
    pub const SYSTEM_ALT_F4: u32 = 0x0000_007E;
    /// See [`SYSTEM_ALT_TAB`].
    pub const SYSTEM_CTRL_SHIFT_ESC: u32 = 0x0000_007F;
    /// `ToggleHelp` — opens help with arguments `(0, 0x10000001)`.
    pub const TOGGLE_HELP: u32 = 0x0000_007B;
    /// `TogglePluginManager` — the one arm reached by its own branch rather than by the
    /// shared dispatch or the subtraction chain.
    pub const TOGGLE_PLUGIN_MANAGER: u32 = 0x0000_007C;
    /// `ToggleRadarPanel` — the radar-panel visibility action.
    pub const TOGGLE_RADAR_PANEL: u32 = 0x1000_001E;
    /// `ToggleGameplayOptionsPanel`, which is not an arm of the UI action handler but the argument
    /// `EscapeKey`'s "nothing is selected" leg hands on. This is why Escape opens the options
    /// panel.
    pub const TOGGLE_GAMEPLAY_OPTIONS_PANEL: u32 = 0x1000_001B;

    // ---- the sixteen tab-target actions -------------------------------------------
    //
    // Every one is already in `crate::actions::names`; the names below are that table's, verbatim.
    // The player-action handler numbers its cases from `0x1000002A`, and the case number each
    // id lands on is given so the arm can be checked against retail's case order.

    /// `SelectionLastAttacker` — `case 0xD`, and the **only** action that reaches
    /// the last-attacker range check rather than `select_next`.
    pub const SELECTION_LAST_ATTACKER: u32 = 0x1000_0038;

    /// `SelectionClosestCompassItem` — `case 4`, `select_next(true, true, COMPASS_ITEM, false)`.
    pub const SELECTION_CLOSEST_COMPASS_ITEM: u32 = 0x1000_002F;
    /// `SelectionPreviousCompassItem` — `case 5`.
    pub const SELECTION_PREVIOUS_COMPASS_ITEM: u32 = 0x1000_0030;
    /// `SelectionNextCompassItem` — `case 6`.
    pub const SELECTION_NEXT_COMPASS_ITEM: u32 = 0x1000_0031;
    /// `SelectionClosestItem` — `case 7`, and the **only** one of the twenty-six call sites that
    /// passes `exclude_own_wielded = true`.
    ///
    /// **And it is inert there.**
    /// The select-next item-selection arm rejects any object with a non-zero wielder
    /// outright — *any* wielder — while the exclude-own-wielded gate rejects only an object the
    /// player wields, a strict subset of it for every non-zero player id. So
    /// "closest item" and "next item" are the **same** about a wielded object: neither can pick
    /// one. The widely-repeated consequence — *"'closest item' will not pick your own drawn weapon
    /// while 'next item' will"* — is false, and a test asserts it both ways.
    pub const SELECTION_CLOSEST_ITEM: u32 = 0x1000_0032;
    /// `SelectionPreviousItem` — `case 8`.
    pub const SELECTION_PREVIOUS_ITEM: u32 = 0x1000_0033;
    /// `SelectionNextItem` — `case 9`.
    pub const SELECTION_NEXT_ITEM: u32 = 0x1000_0034;
    /// `SelectionClosestMonster` — `case 10`.
    pub const SELECTION_CLOSEST_MONSTER: u32 = 0x1000_0035;
    /// `SelectionPreviousMonster` — `case 0xB`.
    pub const SELECTION_PREVIOUS_MONSTER: u32 = 0x1000_0036;
    /// `SelectionNextMonster` — `case 0xC`.
    pub const SELECTION_NEXT_MONSTER: u32 = 0x1000_0037;
    /// `SelectionClosestPlayer` — `case 0xE`.
    pub const SELECTION_CLOSEST_PLAYER: u32 = 0x1000_0039;
    /// `SelectionPreviousPlayer` — `case 0xF`.
    pub const SELECTION_PREVIOUS_PLAYER: u32 = 0x1000_003A;
    /// `SelectionNextPlayer` — `case 0x10`.
    pub const SELECTION_NEXT_PLAYER: u32 = 0x1000_003B;
    /// `SelectionPreviousSelection`, the shipped `DIK_P` —
    /// case `0x1000002E`.
    pub const SELECTION_PREVIOUS_SELECTION: u32 = 0x1000_002E;
    /// `SelectionPreviousFellow`, the shipped `DIK_N` —
    /// fellowship selection's previous-member operation.
    pub const SELECTION_PREVIOUS_FELLOW: u32 = 0x1000_003C;
    /// `SelectionNextFellow`, the shipped `DIK_M` —
    /// fellowship selection's next-member operation.
    pub const SELECTION_NEXT_FELLOW: u32 = 0x1000_003D;
    /// `SelectionUseClosestUnopenedCorpse` — `case 0x13`, which selects **and then uses**.
    pub const SELECTION_USE_CLOSEST_UNOPENED_CORPSE: u32 = 0x1000_003E;
    /// `SelectionUseNextUnopenedCorpse` — `case 0x14`, which selects, wraps, **and then uses**.
    pub const SELECTION_USE_NEXT_UNOPENED_CORPSE: u32 = 0x1000_003F;
    /// `SelectionClosestUnopenedCorpse` — `case 0x43`: the same selection as `case 0x13` with
    /// **no** object-use call.
    pub const SELECTION_CLOSEST_UNOPENED_CORPSE: u32 = 0x1000_0121;
    /// `SelectionNextUnopenedCorpse` — `case 0x44`: the same as `case 0x14` with no object use.
    pub const SELECTION_NEXT_UNOPENED_CORPSE: u32 = 0x1000_0122;
}

/// The `PlayerOption_*` input actions the player system handles, each with the `PlayerOption`
/// ordinal it flips.
///
/// Fifty of the fifty-three options: `AppearOffline` (39), `ShowCloak` (50) and `LockUI` (51)
/// have no such action handled (show-cloak's `0x1000012F` is named but falls through). The last
/// row, hear-PK-deaths, is the newest.
pub const PLAYER_OPTION_ACTIONS: [(u32, usize); 50] = [
    (0x1000_0071, 0),  // AutoRepeatAttack
    (0x1000_0072, 1),  // IgnoreAllegianceRequests
    (0x1000_0073, 2),  // IgnoreFellowshipRequests
    (0x1000_0074, 3),  // IgnoreTradeRequests
    (0x1000_0075, 4),  // DisableMostWeatherEffects
    (0x1000_0076, 5),  // PersistentAtDay
    (0x1000_0077, 6),  // AllowGive
    (0x1000_0078, 7),  // ViewCombatTarget
    (0x1000_0079, 8),  // ShowTooltips
    (0x1000_007A, 9),  // UseDeception
    (0x1000_007B, 10), // ToggleRun
    (0x1000_007C, 11), // StayInChatMode
    (0x1000_007D, 12), // AdvancedCombatUI
    (0x1000_007E, 13), // AutoTarget
    (0x1000_007F, 14), // VividTargetingIndicator
    (0x1000_0080, 15), // FellowshipShareXP
    (0x1000_0081, 16), // AcceptLootPermits
    (0x1000_0082, 17), // FellowshipShareLoot
    (0x1000_0083, 18), // FellowshipAutoAcceptRequests
    (0x1000_0085, 20), // CoordinatesOnRadar
    (0x1000_0086, 21), // SpellDuration
    (0x1000_0087, 22), // DisableHouseRestrictionEffects
    (0x1000_0088, 23), // DragItemOnPlayerOpensSecureTrade
    (0x1000_0089, 24), // DisplayAllegianceLogonNotifications
    (0x1000_008A, 25), // UseChargeAttack
    (0x1000_008B, 26), // UseCraftSuccessDialog
    (0x1000_008C, 27), // HearAllegianceChat
    (0x1000_008D, 28), // DisplayDateOfBirth
    (0x1000_008E, 29), // DisplayAge
    (0x1000_008F, 30), // DisplayChessRank
    (0x1000_0090, 31), // DisplayFishingSkill
    (0x1000_0091, 32), // DisplayNumberDeaths
    (0x1000_0092, 33), // DisplayTimeStamps
    (0x1000_0093, 34), // SalvageMultiple
    (0x1000_010E, 35), // HearGeneralChat
    (0x1000_010F, 36), // HearTradeChat
    (0x1000_0110, 37), // HearLFGChat
    (0x1000_0112, 38), // HearRoleplayChat
    (0x1000_011B, 40), // DisplayNumberCharacterTitles
    (0x1000_011D, 41), // MainPackPreferred
    (0x1000_011E, 42), // LeadMissileTargets
    (0x1000_011F, 43), // UseFastMissiles
    (0x1000_0120, 44), // FilterLanguage
    (0x1000_0123, 45), // ConfirmVolatileRareUse
    (0x1000_0125, 46), // HearSocietyChat
    (0x1000_012A, 47), // ShowHelm
    (0x1000_012C, 48), // DisableDistanceFog
    (0x1000_012D, 49), // UseMouseTurning
    (0x1000_013E, 19), // SideBySideVitals
    (0x1000_013F, 52), // HearPKDeaths
];

/// The option a `PlayerOption_*` input action flips, or `None` for any other action.
#[must_use]
pub fn player_option_action(id: u32) -> Option<usize> {
    PLAYER_OPTION_ACTIONS
        .iter()
        .find(|(a, _)| *a == id)
        .map(|(_, o)| *o)
}

/// Retail's attack height (`HIGH` 1, `MEDIUM` 2, `LOW` 3) as `dereth_client_model`'s enum —
/// the one place the combat window's raw number crosses into the model.
///
/// The window carries the raw value because `dereth-ui-screens` does not depend on `dereth-client-model`;
/// anything the enum does not name is `MEDIUM`, which is `set_requested_attack_height`'s own
/// behaviour for an out-of-range argument (it stores whatever it is given, and every producer in
/// the client passes one of the three literals).
#[must_use]
fn attack_height_from_raw(v: u32) -> AttackHeight {
    match v {
        1 => AttackHeight::High,
        3 => AttackHeight::Low,
        _ => AttackHeight::Medium,
    }
}

/// One of the toolbar's four stance icons, whose click toggles combat mode.
#[must_use]
fn is_combat_mode_button(id: ElementId) -> bool {
    dereth_client_contract::combat_mode::BUTTONS
        .iter()
        .any(|(_, e)| *e == id)
}

/// Whether a pointer event landed in the 3D viewport rather than on a piece of the HUD.
///
/// Object search first honors the UI element under the mouse, so an item icon over the viewport
/// wins, and otherwise runs the geometry picker at `(x, y)`. The smart-box wrapper becomes the
/// live hit-test result because its initialization makes the wrapper mouse-visible.
///
/// Drop targeting needs a hit element as well as clicks:
/// mouse-over switching asks that element for its drag-and-drop catcher, and the shipped wrapper
/// names itself as the catcher through attribute `0x36`. Making the wrapper hit-testable
/// supplies `Some(SMART_BOX)` to both click and drop consumers.
///
/// The `None` arm remains for synthesized `crate::ui::UiMouseEvent` values that carry no hit
/// test. It preserves their established world-click behavior, but the live tree is expected to
/// provide `Some(SMART_BOX)`.
#[must_use]
pub fn is_world_click(over: Option<ElementId>) -> bool {
    const SMART_BOX: ElementId = dereth_client_contract::gameplay::SMART_BOX;
    match over {
        None => true,
        Some(id) => id == SMART_BOX,
    }
}

/// Step 8's interaction slot: the drawing pass's pick read-out and the requests produced by it
/// and by the screens.
///
/// Called from [`crate::app::App::frame`] between the camera update and `DrawWorld`, which is
/// where the client runs it: the viewpoint update builds the ray from **this** frame's camera, and
/// the notice is raised before the UI overlay draws.
///
/// Returns the [`UiRequest`]s nothing owns yet, for the caller to report.
#[allow(clippy::too_many_arguments)]
pub fn use_time(
    inter: &mut Interaction,
    store: &RetailDatStore,
    world: Option<&dyn crate::present::Scene>,
    objects: &mut crate::objects::ObjectStream,
    net: Option<&mut crate::net::ClientNetwork>,
    actions: Vec<crate::actions::Action>,
    player_desc_received: bool,
    viewport: (u32, u32),
    now: dereth_primitives::LocalTime,
) -> (Vec<UiRequest>, Vec<crate::actions::Action>) {
    use_time_with_chat_focus(
        inter,
        store,
        world,
        objects,
        net,
        actions,
        player_desc_received,
        viewport,
        now,
        &mut |_| {},
    )
}

/// The App supplies the live chat subscriber for synchronous option-notice delivery.
#[allow(clippy::too_many_arguments)]
pub fn use_time_with_chat_focus(
    inter: &mut Interaction,
    store: &RetailDatStore,
    world: Option<&dyn crate::present::Scene>,
    objects: &mut crate::objects::ObjectStream,
    net: Option<&mut crate::net::ClientNetwork>,
    actions: Vec<crate::actions::Action>,
    player_desc_received: bool,
    viewport: (u32, u32),
    now: dereth_primitives::LocalTime,
    chat_focus: &mut dyn FnMut(&mut dereth_client_model::chat::ChatState),
) -> (Vec<UiRequest>, Vec<crate::actions::Action>) {
    interaction_frame_tail(
        inter,
        store,
        world,
        objects,
        net,
        actions,
        player_desc_received,
        viewport,
        now,
        chat_focus,
        true,
    )
}

/// App's draw_no_blit/input tail. Registered UI-system timers have already run at frame entry.
#[allow(clippy::too_many_arguments)]
pub fn draw_use_time_with_chat_focus(
    inter: &mut Interaction,
    store: &RetailDatStore,
    world: Option<&dyn crate::present::Scene>,
    objects: &mut crate::objects::ObjectStream,
    net: Option<&mut crate::net::ClientNetwork>,
    actions: Vec<crate::actions::Action>,
    player_desc_received: bool,
    viewport: (u32, u32),
    now: dereth_primitives::LocalTime,
    chat_focus: &mut dyn FnMut(&mut dereth_client_model::chat::ChatState),
) -> (Vec<UiRequest>, Vec<crate::actions::Action>) {
    interaction_frame_tail(
        inter,
        store,
        world,
        objects,
        net,
        actions,
        player_desc_received,
        viewport,
        now,
        chat_focus,
        false,
    )
}

/// A use-time tick invokes registered systems in startup registration order. Client UI, player,
/// and combat run before the timer; this helper performs the work owned by those first three.
pub fn registered_systems_use_time(
    inter: &mut Interaction,
    world: Option<&dyn crate::present::Scene>,
    objects: &mut crate::objects::ObjectStream,
    net: Option<&mut crate::net::ClientNetwork>,
    now: dereth_primitives::LocalTime,
) {
    inter.last_use_time = now;
    inter.last_sent.clear();
    inter.note_player_physics(world.and_then(crate::present::Scene::character));
    if let Some(c) = world.and_then(crate::present::Scene::character) {
        let driver = c.driver();
        objects.world.combat.current_style =
            driver.movement.interp.interpreted_state.current_style.0;
        objects.world.combat.forward_command =
            driver.movement.interp.interpreted_state.forward_command.0;
    }
    inter.run_leave_target_mode();
    inter.run_player_module_use_time(&mut objects.world, ServerTime(now.0));
    inter.run_object_range_checks(world, objects, ServerTime(now.0));
    let origin = world
        .and_then(crate::present::Scene::character)
        .map(crate::character::Character::position);
    let phys = crate::selection_geometry::SceneSelectionPhysics::new(origin.as_ref(), objects);
    let radius = dereth_client_contract::radar::radar_range(
        origin
            .as_ref()
            .is_some_and(|p| dereth_physics::landdefs::is_outdoors(p.cell)),
    );
    let ready = inter.ready_for_attack(&objects.world);
    inter.run_power_bar(&mut objects.world, ready, now);
    let ready = inter.ready_for_mode_change(&objects.world);
    inter.run_pending_combat_mode(&mut objects.world, ready, &phys, radius, now);
    inter.dispatch_ui_selection_notices(
        world.and_then(crate::present::Scene::character),
        objects,
        now,
    );
    flush_requests(inter, net, false);
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn interaction_frame_tail(
    inter: &mut Interaction,
    store: &RetailDatStore,
    world: Option<&dyn crate::present::Scene>,
    objects: &mut crate::objects::ObjectStream,
    net: Option<&mut crate::net::ClientNetwork>,
    actions: Vec<crate::actions::Action>,
    player_desc_received: bool,
    viewport: (u32, u32),
    now: dereth_primitives::LocalTime,
    chat_focus: &mut dyn FnMut(&mut dereth_client_model::chat::ChatState),
    compatibility_timers: bool,
) -> (Vec<UiRequest>, Vec<crate::actions::Action>) {
    let server_now = ServerTime(now.0);
    inter.last_use_time = now;
    // Store the render-target width and height before anything can ask for a pick: the drop
    // arm reads it out of the struct because it is reached through
    // `run_ui_requests`, which the client reaches through a singleton too. The viewport
    // width/height are a different pair of numbers -- see
    // `Interaction::game_viewport`, which the App pushes in beside this.
    inter.screen = viewport;
    // The render device's viewport rectangle, resolved once for the whole frame, so
    // the pick and its read-out cannot be measured against two different rectangles.
    let rect = inter.device_viewport(viewport);
    // 1. Pointer handling. The element manager already dispatched these events
    //    through the tree; this is the viewport-specific handling of those events.
    for e in std::mem::take(&mut inter.mouse) {
        // Each pointer event carries its dispatch position. Retain it before handling
        // the event so step 2's drop handling sees the latest position, matching the
        // original input path that completes drag/drop inside mouse release.
        inter.cursor = (e.x, e.y);
        let world_click = is_world_click(e.over);
        inter.wrapper_mouse(e, viewport, world_click);
    }
    // 1b. Combat reads the motion interpreter's current
    //     style to select the charge duration: dual wield (`0x80000046`) uses
    //     **0.800 s**, otherwise **1.000 s**. The original combat system queries
    //     motion state directly. Here animation belongs to `dereth_animation` and combat
    //     to `dereth_client_model`, so the value crosses this seam once per frame.
    //
    //     The server's movement style reaches the local motion state;
    //     this bridge supplies `World.combat`. It runs where both the body and
    //     mutable world are available. `Character::driver()` gives read-only access
    //     to motion, and the combat copy is written only when the style changes.
    // 1c. Query the player's gameplay position once a frame, so
    //     that the place-in-3-D path's `on_ground` test has the answer the client's has. Before
    //     step 3
    //     for the same reason the client's is before `draw_no_blit`: the notice is what consumes it.
    inter.note_player_physics(world.and_then(crate::present::Scene::character));
    if let Some(scene) = world {
        if let Some(c) = scene.character() {
            let style = c.driver().movement.interp.interpreted_state.current_style.0;
            if inter.combat_style_bridged != Some(style) {
                inter.combat_style_bridged = Some(style);
                objects.world.combat.current_style = style;
                inter.stats.combat_style_bridges += 1;
            }
            // The same struct's `forward_command`, which is the other half of
            // the ready-position check's missile arm: movement interpretation supplies the command,
            // and readiness compares it with `0x41000003`. Counted on the same counter because
            // style and forward command cross the same seam.
            let fwd = c
                .driver()
                .movement
                .interp
                .interpreted_state
                .forward_command
                .0;
            if inter.combat_forward_command_bridged != Some(fwd) {
                inter.combat_forward_command_bridged = Some(fwd);
                objects.world.combat.forward_command = fwd;
                inter.stats.combat_style_bridges += 1;
            }
        }
    }
    // 2. The screens' requests, and the keyboard actions the UI declined.
    let unowned = inter.run_ui_requests_with_chat_focus(
        &mut objects.world,
        player_desc_received,
        server_now,
        chat_focus,
    );
    // Each combat-mode change or attack asks for readiness at its own call site.
    //
    // **Five original consumers, two argument values:**
    // attack-build, attack-execute and the power-bar entry use true; immediate
    // mode change and pending-mode retry use false. In melee and missile, true
    // returns as soon as the weapon prerequisite holds; false additionally tests
    // `!motions_pending`.
    //
    // The full mode/weapon switch is in the two readiness functions. The remaining
    // deviation for a missing physics body at attack sites is documented in
    // [`Interaction::ready_for_attack`].
    //
    // **This frame path keeps four separate readiness reads.** They
    // read the combat mode at the time of the call. `run_combat_mode_toggle` can
    // change that mode, so hoisting one answer would make later sites use the
    // mode already left behind. The original power-bar update's order relative
    // to input actions was not established here; asking separately remains valid
    // under any such ordering, while hoisting does not.
    //
    // Tests without a scene cannot distinguish these reads: no body gives false.
    // A test supplies a headless scene and advances the real motion
    // queue to prove pending-ready retry and auto-targeting. A toolbar change in
    // the same frame as an already-queued mode is not covered by a test.
    let ready_for_attack = inter.ready_for_attack(&objects.world);
    // **Take the selection geometry's inputs once.**
    //
    // Selection distance is measured from the smart box's player, which in this build is the
    // local body — the same `Position` `run_object_range_checks` measures from
    // and the same one `Hud::sync` hands the radar. With no body the snapshot is empty, which is
    // the player-space conversion's own null-player return propagated to every candidate.
    //
    // Built here rather than inside `on_actions` because the snapshot borrows `objects.presences`
    // and the cycle needs `&mut objects.world`; the borrow ends at construction because
    // `SceneSelectionPhysics` owns its table.
    let selection_origin = world
        .and_then(crate::present::Scene::character)
        .map(crate::character::Character::position);
    let selection_phys =
        crate::selection_geometry::SceneSelectionPhysics::new(selection_origin.as_ref(), objects);
    // The radar radius comes from its single owner, exactly as `run_object_range_checks` does:
    // the outside test is
    // `is_outdoors` on the player's cell id. Passed in once because `select_next` calls it
    // per candidate for a value that cannot move during the scan.
    let selection_radius = dereth_client_contract::radar::radar_range(
        selection_origin
            .as_ref()
            .is_some_and(|p| dereth_physics::landdefs::is_outdoors(p.cell)),
    );
    // The two defender-notification handlers' shared tail, which `apply_events`
    // recorded earlier in this same `App::frame`. Here rather than there because it is the first
    // point at which the selection geometry exists, and **before** `on_actions` because retail
    // runs it from the net-blob drain, ahead of
    // every input action in the frame. It is the only writer of `CombatState::last_attacked_time`,
    // which is what makes auto-targeting's 15-second reselect arm reachable at all.
    inter.run_defender_notifications(&mut objects.world, &selection_phys, selection_radius, now);
    // Run the combat system's selection-change subscriber for every
    // selection change absorbed since the last frame — including the ones this frame's own
    // `apply_events` and `run_defender_notifications` raised, which is why it sits below them.
    // Retail dispatches the notice synchronously from inside selection assignment; the seam is
    // declared on the handler itself.
    inter.run_selection_change_notices(&mut objects.world, &selection_phys, selection_radius, now);
    // `EscapeKey` asks whether the player is standing still
    // through the command interpreter. It queries the motion interpreter when a body exists;
    // otherwise it returns true. So a frame with
    // no body answers **true**, and that is the value this leaves in
    // place rather than a convenience. Read here, one line above `on_actions`, for the same reason
    // the combat-style bridge is read at step 1c: this is the only per-frame slot that holds the
    // body and the interaction at once.
    if let Some(c) = world.and_then(crate::present::Scene::character) {
        let d = c.driver();
        let still = d.movement.interp.is_standing_still(&d.env);
        drop(d);
        inter.note_standing_still(still);
    }
    let left = inter.on_actions(
        actions,
        &mut objects.world,
        &selection_phys,
        selection_radius,
        ready_for_attack,
        now,
        world.and_then(crate::present::Scene::character),
    );
    // Combat-mode setup step 2 pushes zero, and the switch it runs reads the
    // mode being LEFT, because the new mode is assigned after the call.
    let ready_for_mode_change = inter.ready_for_mode_change(&objects.world);
    inter.run_combat_mode_toggle(
        &mut objects.world,
        ready_for_mode_change,
        &selection_phys,
        selection_radius,
        now,
    );
    // Start the combat update: charge the power bar, and swing when it arrives at the
    // requested level. This is what makes a click an attack rather than a hold.
    // This uses the attack readiness check.
    if compatibility_timers {
        let ready_for_attack = inter.ready_for_attack(&objects.world);
        inter.run_power_bar(&mut objects.world, ready_for_attack, now);
        // Finish the combat update: retry a mode change the ready check refused earlier,
        // using the mode-change readiness check answered from the body.
        let ready_for_mode_change = inter.ready_for_mode_change(&objects.world);
        inter.run_pending_combat_mode(
            &mut objects.world,
            ready_for_mode_change,
            &selection_phys,
            selection_radius,
            now,
        );
        // Run the 480-second whole-module flush.
        inter.run_player_module_use_time(&mut objects.world, server_now);
        // Object range checks finish the frame update: this closes a vendor, a corpse
        // or a secure trade when the player walks away from it.
        inter.run_object_range_checks(world, objects, server_now);
    }

    // 3. The drawing pass's tail — the pick and the notice it raises.
    if inter.pick.looking_for_object() {
        match world {
            Some(w) => {
                if let Some(id) =
                    inter
                        .pick
                        .draw_no_blit(store, w.as_pick_scene(), objects, viewport, rect)
                {
                    inter.on_world_object_found(id, &mut objects.world, server_now);
                }
            }
            // **This arm raises the notice even without a scene.**
            //
            // The native drawing tail first tests whether a player exists. A missing player skips
            // viewer and normal-mode rendering but lands at the same armed-pick test. When a pick
            // is armed, it reads the selected part and object, raises the object-found notice,
            // clears the armed flag, and then clears the selection cursor.
            //
            // The player-present guard leads **to** the notice block, not past it, so a frame with
            // no player still answers an armed pick. What it answers *with* is 0:
            // `clear_selection_cursor` left the mouse-select found-polygon and
            // found-sphere flags false at the end of the previous frame, nothing swept this one,
            // and the mouse-selection object id is the polygon's only when found-polygon
            // and otherwise the sphere's only when found-sphere — zero. `[verified]`
            //
            // Dropping it silently would latch the drop search reason: the reason is cleared
            // **only** at the object-found notice's unconditional tail, so a scene-less
            // frame between the release and the sweep would leave reason 5 set, and every later
            // `< SearchReason::Examine` / `< SearchReason::Use` gate in
            // [`Interaction::wrapper_mouse`] would refuse — a wedged viewport. The drop's item is
            // not lost either: the zero id takes `place_in_3d`'s `onto = None` leg, which is the
            // ground, and the ground leg is refused in a scene-less frame because there is no
            // physics object to be on the ground.
            // Use `draw_no_blit_scene_less` rather than merely clearing the
            // selection cursor. The normal read-out writes **both** answer fields before raising
            // the notice, while cursor clearing writes neither. The id was already 0 on this arm;
            // `click_object_index` was
            // `find_object`'s `-1` and the client's is `0`.
            None => {
                if let Some(id) = inter.pick.draw_no_blit_scene_less() {
                    inter.stats.scene_less_notices += 1;
                    inter.on_world_object_found(id, &mut objects.world, server_now);
                }
            }
        }
    }

    // 3b. The UI-system target-mode tail. It runs after the pick,
    //     deliberately: see [`Interaction::run_leave_target_mode`] for why that ordering is a
    //     choice made here rather than one read from retail.
    if compatibility_timers {
        inter.run_leave_target_mode();
    }

    // 4. The wire. One `send_action` per request, and the rollback is the session's.
    flush_requests(inter, net, compatibility_timers);
    (unowned, left)
}

fn flush_requests(
    inter: &mut Interaction,
    net: Option<&mut crate::net::ClientNetwork>,
    clear: bool,
) {
    let outbox = std::mem::take(&mut inter.outbox);
    if clear {
        inter.last_sent.clear();
    }
    inter.last_sent.extend(outbox.iter().cloned());
    match net {
        Some(net) => {
            for r in outbox {
                if send_request(&mut net.session, &r) {
                    inter.stats.requests_sent += 1;
                } else {
                    inter.stats.requests_undeliverable += 1;
                }
            }
        }
        None => {
            for r in outbox {
                inter.stats.requests_undeliverable += 1;
                tracing::warn!("{r:?} with no server to send it to");
            }
        }
    }
}

/// The one place a [`dereth_client_model::Request`] becomes bytes.
///
/// It lives in `dereth_client_runtime::requests` -- it is plain
/// `dereth_client_model`/`dereth_client_net::client_session` plumbing that `objects.rs` also
/// reads. This path resolves through the `pub use`.
pub use crate::requests::send_request;

/// Position formatting follows the original
/// `_snprintf(buf, 100, "0x%08X [%f %f %f] %f %f %f %f",
/// objcell_id, x, y, z, qw, qx, qy, qz)`.
///
/// The eight operands are the cell id followed by seven floats: origin, then
/// quaternion with w first. Each float is widened to double and printed at six
/// decimals. Formatting the same `f32 as f64` preserves the binary value and
/// negative zero as `-0.000000`. This Rust formatter does not impose the original
/// 100-byte cap; sufficiently large synthetic coordinates can exceed it.
#[must_use]
pub fn position_to_string(p: &dereth_primitives::Position) -> String {
    let o = p.frame.origin;
    let q = p.frame.rotation;
    format!(
        "{:#010X} [{:.6} {:.6} {:.6}] {:.6} {:.6} {:.6} {:.6}",
        p.cell.0,
        f64::from(o.x),
        f64::from(o.y),
        f64::from(o.z),
        f64::from(q.w),
        f64::from(q.x),
        f64::from(q.y),
        f64::from(q.z),
    )
}

/// Convert a land-cell id to the local coordinate text used by `@corpse`.
///
/// Two details matter: both coordinate components subtract `0x400`, and formatting supplies
/// N/S first and E/W second for `"%.1f%s, %.1f%s"`. The origin inside the position is
/// deliberately ignored; retail uses only the position's cell id.
fn corpse_coordinate_string(cell: u32) -> Option<String> {
    let (east_west, north_south) =
        dereth_physics::landdefs::gid_to_lcoord(dereth_primitives::CellId(cell))?;
    let east_west = f64::from(east_west - 0x400) * 0.1 + 0.5;
    let north_south = f64::from(north_south - 0x400) * 0.1 + 0.5;
    let suffix = |value: f64, positive: &'static str, negative: &'static str| {
        if value < 0.0 {
            negative
        } else if value > 0.0 {
            positive
        } else {
            ""
        }
    };
    Some(format!(
        "{:.1}{}, {:.1}{}",
        north_south.abs(),
        suffix(north_south, "N", "S"),
        east_west.abs(),
        suffix(east_west, "E", "W"),
    ))
}

/// The first-token table shared by `@join` and
/// `@leave`. The values are the retail `PlayerOption` ordinals passed by each
/// Set-hear-chat tail call.
fn join_leave_channel_option(token: Option<&str>) -> Option<usize> {
    let token = token?;
    if token.eq_ignore_ascii_case("Allegiance") {
        Some(27)
    } else if token.eq_ignore_ascii_case("General") {
        Some(35)
    } else if token.eq_ignore_ascii_case("Trade") {
        Some(36)
    } else if token.eq_ignore_ascii_case("LFG") {
        Some(37)
    } else if token.eq_ignore_ascii_case("Roleplay") {
        Some(38)
    } else if token.eq_ignore_ascii_case("Society") || token.eq_ignore_ascii_case("Soc") {
        Some(46)
    } else {
        None
    }
}

/// Chat timestamps use CRT `time(NULL)`, not the frame's simulated clock.
fn chat_real_time() -> i32 {
    #[allow(clippy::cast_possible_truncation)]
    // Retail carries signed 32-bit time_t. Preserve those low bits explicitly.
    {
        crate::platform::clock::system_unix_time().map_or(0, |d| d.as_secs()) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_cannot_replace_pending_click_and_hidden_or_absent_answers_complete_with_zero() {
        let mut game = dereth_client_model::World::new();
        let target = ObjectId(42);
        let mut weenie = dereth_client_model::weenie::Weenie::new(target);
        weenie.pwd.bitfield = 0x80; // Hidden UI flag; visibility checks use the low byte.
        game.tables.weenies.insert(target, weenie);
        game.selected = Some(ObjectId(99));
        let mut inter = Interaction::new();
        for reason in [
            SearchReason::Select,
            SearchReason::Drop,
            SearchReason::TargetedUse,
        ] {
            inter.reason = reason;
            inter.pick.set_found_object(ObjectId(17), 2);
            inter.dispatch_ui_hover(
                (100, 100),
                Some(target),
                (800, 600),
                &mut game,
                ServerTime(1.0),
            );
            assert_eq!(
                inter.reason, reason,
                "a mouse-move or frame-loop hover must not overwrite pending work"
            );
            assert_eq!(inter.pick.click_object(), (ObjectId(17), 2));
        }
        for answer in [target, ObjectId(123)] {
            inter.reason = SearchReason::Drop;
            inter.drop_item = ObjectId(456); // Absent source: refuse, but still finish the drop tail.
            inter.pick.set_found_object(answer, 3);
            inter.on_world_object_found(answer, &mut game, ServerTime(2.0));
            assert_eq!(inter.pick.click_object(), (ObjectId(0), -1));
            assert_eq!(inter.reason, SearchReason::None);
            assert_eq!(inter.drop_item, ObjectId(0));
            assert_eq!(game.selected, Some(ObjectId(99)));
            assert!(inter.pending_requests().is_empty());
        }
        game.weenie_mut(target).unwrap().pwd.bitfield = 0;
        inter.dispatch_ui_hover(
            (100, 100),
            Some(target),
            (800, 600),
            &mut game,
            ServerTime(3.0),
        );
        assert_eq!(inter.pick.click_object(), (target, -1));
        assert_eq!(
            game.selected,
            Some(ObjectId(99)),
            "positive hover remains non-selecting"
        );
    }

    /// Oracle: the `SearchReason` ordering and every threshold comparison in the smart-box
    /// wrapper's mouse table.
    #[test]
    fn the_search_reasons_are_ordered_as_the_client_compares_them() {
        assert!(SearchReason::None < SearchReason::MouseOver);
        assert!(SearchReason::MouseOver < SearchReason::Select);
        assert!(SearchReason::Select < SearchReason::Examine);
        assert!(SearchReason::Examine < SearchReason::Use);
        assert!(SearchReason::Use < SearchReason::Drop);
        assert!(SearchReason::Drop < SearchReason::Drag);
        assert!(SearchReason::Drag < SearchReason::TargetedUse);
        assert_eq!(SearchReason::TargetedUse as i32, 7);
    }

    /// Oracle: the toolbar's message-1 handler maps element ids `0x10000192` through
    /// `0x10000195` to combat-mode toggling.
    #[test]
    fn the_four_stance_icons_are_the_ids_the_toolbar_names() {
        for id in 0x1000_0192..=0x1000_0195u32 {
            assert!(is_combat_mode_button(ElementId(id)), "{id:#X}");
        }
        assert!(!is_combat_mode_button(ElementId(0x1000_0191)));
        assert!(!is_combat_mode_button(ElementId(0x1000_0196)));
    }

    /// Oracle: `crate::actions::names::ACTION_ENUM_NAMES`, the shipped `ActionMap`'s own enum table.
    #[test]
    fn the_action_ids_are_the_ones_the_shipped_action_map_names() {
        let name =
            |a: u32| crate::actions::names::enum_name_for_action(crate::actions::ActionId(a));
        assert_eq!(name(action::COMBAT_TOGGLE_COMBAT), "CombatToggleCombat");
        assert_eq!(name(action::COMBAT_LOW_ATTACK), "CombatLowAttack");
        assert_eq!(name(action::COMBAT_MEDIUM_ATTACK), "CombatMediumAttack");
        assert_eq!(name(action::COMBAT_HIGH_ATTACK), "CombatHighAttack");
        assert_eq!(name(action::SELECTION_EXAMINE), "SelectionExamine");
        assert_eq!(name(action::USE), "USE");
    }

    /// Oracle: `SplitState`'s contract: `split_size == max_split_size` means *move everything*;
    /// there is no separate boolean.
    #[test]
    fn the_default_split_state_is_a_whole_stack() {
        let i = Interaction::new();
        assert!(
            i.split().is_whole_stack(),
            "0 >= 0, so an untouched slider moves the stack"
        );
        assert_eq!(i.search_reason(), SearchReason::None);
        assert_eq!(i.target_mode(), TargetMode::None);
    }

    #[test]
    fn trade_split_notices_replay_in_the_order_the_batch_raised_them() {
        const SOURCE: ObjectId = ObjectId(0x7100_0001);
        const RESULT: ObjectId = ObjectId(0x7100_0002);
        fn game() -> dereth_client_model::World {
            let mut game = dereth_client_model::World::new();
            let mut result = dereth_client_model::Weenie::new(RESULT);
            result.pwd.wcid = 0xCAFE;
            result.pwd.stack_size = Some(3);
            game.tables.weenies.insert(RESULT, result);
            game.trade.pending_split = Some(dereth_client_model::trade::PendingTradeSplit {
                source: SOURCE,
                wcid: 0xCAFE,
                stack_size: 3,
            });
            game
        }

        let mut matched_first = game();
        let mut out = Notices::default();
        dereth_client_model::NoticeSink::emit(
            &mut out,
            Notice::ItemAttributesChanged {
                object: RESULT,
                kind: 1,
            },
        );
        dereth_client_model::NoticeSink::emit(
            &mut out,
            Notice::AttemptFailed {
                object: SOURCE,
                reason: 0,
            },
        );
        let mut inter = Interaction::new();
        inter.absorb(&mut matched_first, out, RecordingRequests::default());
        assert_eq!(inter.take_trade_for_dummies(), vec![RESULT]);

        let mut failed_first = game();
        let mut out = Notices::default();
        dereth_client_model::NoticeSink::emit(
            &mut out,
            Notice::AttemptFailed {
                object: SOURCE,
                reason: 0,
            },
        );
        dereth_client_model::NoticeSink::emit(
            &mut out,
            Notice::ItemAttributesChanged {
                object: RESULT,
                kind: 1,
            },
        );
        let mut inter = Interaction::new();
        inter.absorb(&mut failed_first, out, RecordingRequests::default());
        assert!(inter.take_trade_for_dummies().is_empty());
        assert!(failed_first.trade.pending_split.is_none());
    }

    /// Changing target mode clears a pending leave request, while setting the current mode again
    /// preserves it. A generic use can therefore enter targeted-use mode without a stale request
    /// canceling that transition during the next use-time update.
    #[test]
    fn target_mode_change_cancels_pending_leave_but_same_mode_does_not() {
        let mut i = Interaction::new();
        i.set_target_mode(TargetMode::Use);
        i.leave_target_mode = true;
        i.set_target_mode(TargetMode::UseTarget);
        i.run_leave_target_mode();
        assert_eq!(i.target_mode(), TargetMode::UseTarget);
        i.leave_target_mode = true;
        i.set_target_mode(TargetMode::UseTarget);
        i.run_leave_target_mode();
        assert_eq!(i.target_mode(), TargetMode::None);
        assert!(!i.leave_target_mode);
    }

    /// The paper doll canvas is 0x100001d6 and is no slot.
    #[test]
    fn the_paper_doll_canvas_is_0x100001d6_and_is_no_slot() {
        assert_eq!(DOLL_DRAG_MASK, ElementId(0x1000_01D6));
        assert_eq!(
            DOLL_DRAG_MASK,
            dereth_client_contract::panels::inventory::PAPER_DOLL_DRAG_MASK,
            "the panel that names the drop target and the file that resolves it must agree"
        );
        assert_eq!(
            dereth_client_model::inventory::slots::location_info_from_element_id(DOLL_DRAG_MASK.0),
            None,
            "the element-id decoder has no case for the canvas -- that is why it needs its own arm"
        );
    }

    /// The make-shortcut key on the selected object: the first empty slot takes it, with the
    /// add sent to the server; asked again, the object already has one and the player is told;
    /// with every slot full the player is told that instead.
    #[test]
    fn the_make_shortcut_key_fills_the_first_empty_slot_or_says_why_not() {
        const PLAYER: ObjectId = ObjectId(0x5000_0001);
        const COAT: ObjectId = ObjectId(0x8000_0010);
        let mut game = dereth_client_model::World::new();
        game.player = Some(PLAYER);
        game.tables
            .weenies
            .insert(PLAYER, dereth_client_model::weenie::Weenie::new(PLAYER));
        let mut coat = dereth_client_model::weenie::Weenie::new(COAT);
        coat.pwd.name = "Academy Coat".to_owned();
        coat.pwd.container_id = Some(PLAYER);
        game.tables.weenies.insert(COAT, coat);
        // Slot 0 is taken by something else, so the first empty slot is 1.
        game.player_system
            .add_shortcut(dereth_protocol::login::ShortCutData {
                index: 0,
                object_id: PLAYER,
                spell_id: 0,
            });
        let lines = |game: &dereth_client_model::World| -> Vec<(u32, String)> {
            game.scroll
                .pending()
                .iter()
                .map(|f| (f.chat_type, f.body.clone()))
                .collect()
        };

        let mut inter = Interaction::new();
        inter.queue(Vec::new(), vec![UiRequest::CreateShortcut(COAT)]);
        assert!(inter
            .run_ui_requests(&mut game, false, ServerTime(1.0))
            .is_empty());
        assert_eq!(game.player_system.shortcut_at(1), Some(COAT));
        assert!(
            matches!(
                inter.pending_requests(),
                [Request::AddShortCut(m)] if m.shortcut.index == 1 && m.shortcut.object_id == COAT
            ),
            "{:?}",
            inter.pending_requests()
        );

        let mut inter = Interaction::new();
        inter.queue(Vec::new(), vec![UiRequest::CreateShortcut(COAT)]);
        inter.run_ui_requests(&mut game, false, ServerTime(2.0));
        assert!(
            inter.pending_requests().is_empty(),
            "{:?}",
            inter.pending_requests()
        );
        assert!(
            lines(&game).contains(&(
                0x1A,
                "There is already a shortcut to the Academy Coat".to_owned()
            )),
            "{:?}",
            lines(&game)
        );

        game.player_system.remove_shortcut(1);
        for n in 1..dereth_client_model::player::SHORTCUT_SLOTS {
            game.player_system
                .add_shortcut(dereth_protocol::login::ShortCutData {
                    index: i32::try_from(n).unwrap(),
                    object_id: PLAYER,
                    spell_id: 0,
                });
        }
        let mut inter = Interaction::new();
        inter.queue(Vec::new(), vec![UiRequest::CreateShortcut(COAT)]);
        inter.run_ui_requests(&mut game, false, ServerTime(3.0));
        assert!(
            inter.pending_requests().is_empty(),
            "{:?}",
            inter.pending_requests()
        );
        assert!(
            lines(&game).contains(&(0x1A, "There are no free shortcut slots".to_owned())),
            "{:?}",
            lines(&game)
        );
    }

    /// Every `PlayerOption_*` row names the option its action spells, no action or option twice.
    #[test]
    fn the_player_option_actions_name_their_own_options() {
        use dereth_client_contract::actions::{names::enum_name_for_action, ActionId};
        use dereth_client_model::player::options::PLAYER_OPTIONS;
        let mut actions = std::collections::BTreeSet::new();
        let mut options = std::collections::BTreeSet::new();
        for (a, o) in PLAYER_OPTION_ACTIONS {
            let want = format!("PlayerOption_{}", PLAYER_OPTIONS[o].0);
            assert_eq!(enum_name_for_action(ActionId(a)), want, "{a:#x}");
            assert!(actions.insert(a) && options.insert(o));
        }
        assert_eq!(player_option_action(0x1000_013F), Some(52));
        assert_eq!(player_option_action(0x1000_0125), Some(46));
        assert_eq!(
            player_option_action(0x1000_012F),
            None,
            "show-cloak has no arm"
        );
    }

    /// A bound hear-PK-deaths key flips the option both ways, and each flip is saved at once as a
    /// `0x0005`; a non-auto-saved option only marks the module dirty.
    #[test]
    fn a_player_option_action_flips_its_option_and_saves_an_auto_saved_one_at_once() {
        use dereth_client_contract::actions::{Action, ActionId};
        use dereth_client_model::player::options::option;
        let mut inter = Interaction::new();
        let mut game = dereth_client_model::World::new();
        let phys = crate::selection_geometry::SceneSelectionPhysics::default();
        let press = |inter: &mut Interaction, game: &mut dereth_client_model::World, id| {
            inter.on_actions(
                vec![Action::begin(ActionId(id))],
                game,
                &phys,
                0.0,
                false,
                dereth_primitives::LocalTime(1.0),
                None,
            )
        };
        let sent = |inter: &mut Interaction| -> Vec<(u32, u32)> {
            inter
                .take_pending_requests()
                .into_iter()
                .filter_map(|r| match r {
                    Request::PlayerOptionChanged(e) => Some((e.option, e.value)),
                    _ => None,
                })
                .collect()
        };
        assert!(game.player_system.options.hear_pk_deaths());
        assert!(press(&mut inter, &mut game, 0x1000_013F).is_empty());
        assert!(!game.player_system.options.hear_pk_deaths());
        assert_eq!(sent(&mut inter), vec![(52, 0)]);
        assert!(press(&mut inter, &mut game, 0x1000_013F).is_empty());
        assert!(game.player_system.options.hear_pk_deaths());
        assert_eq!(sent(&mut inter), vec![(52, 1)]);

        // Advanced combat UI is not saved at once: it only dirties the module.
        assert!(!game.player_system.options.get(option::ADVANCED_COMBAT_UI));
        assert!(press(&mut inter, &mut game, 0x1000_007D).is_empty());
        assert!(game.player_system.options.get(option::ADVANCED_COMBAT_UI));
        assert!(sent(&mut inter).is_empty());
        assert!(game.player_system.is_dirty());
        assert_eq!(inter.stats.option_actions_toggled, 3);

        // An action with no arm is handed back.
        let left = press(&mut inter, &mut game, 0x1000_012F);
        assert_eq!(left.len(), 1);
    }

    /// Event `0x0318` queues its one string for a message box, exactly as `0x0004` does.
    #[test]
    fn event_0318_raises_the_same_pop_up_as_0004() {
        use dereth_client_net::client_session::SessionEvent;
        fn ui_event<M: dereth_protocol::Message>(m: &M) -> SessionEvent {
            let mut blob = M::OPCODE.0.to_le_bytes().to_vec();
            blob.extend(dereth_protocol::write_body(m).expect("a synthetic message encodes"));
            SessionEvent::UiEvent {
                opcode: M::OPCODE,
                blob,
            }
        }
        let mut inter = Interaction::new();
        let mut game = dereth_client_model::World::new();
        apply_events(
            &mut inter,
            &[
                ui_event(&dereth_protocol::comms::CommunicationPopUpString0318 {
                    message: "Welcome to the Olthoi Horde.".into(),
                }),
                ui_event(&dereth_protocol::comms::CommunicationPopUpString {
                    message: "Welcome back.".into(),
                }),
            ],
            &mut game,
        );
        assert_eq!(
            inter.take_pop_up_strings(),
            vec![
                "Welcome to the Olthoi Horde.".to_owned(),
                "Welcome back.".to_owned()
            ]
        );
        assert_eq!(inter.stats.pop_up_strings, 2);
        assert_eq!(inter.stats.pop_up_strings_undecodable, 0);
    }
}
