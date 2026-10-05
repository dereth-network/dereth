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

mod actions;
mod chat;
mod combat;
mod dialogs;
mod events;
mod frame;
mod items;
mod notices;
mod pointer;
mod ui_requests;

#[cfg(any(test, feature = "test-support"))]
mod testing;
pub use events::apply_events_at_boundary;
pub use frame::{draw_use_time_with_chat_focus, registered_systems_use_time};
#[cfg(any(test, feature = "test-support"))]
pub use testing::{apply_events, use_time, use_time_with_chat_focus};

use crate::requests::send_request;
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
/// This is only the fallback: the version that matters is the running program's, which this library
/// crate cannot see (its own package version is the workspace's, not the client's). The program
/// names itself at start-up through [`Interaction::client_build_id`].
const CLIENT_BUILD_ID: &str = concat!("dereth-client", " ", env!("CARGO_PKG_VERSION"));

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

pub use crate::stats::InteractionStats;

/// A `bool` whose zero value is `true`.
///
/// It lives in `dereth_client_runtime::flags`, because `character.rs` names it; this path
/// resolves through the `pub use`.
pub use crate::flags::StartsTrue;

/// Viewport selection, combat input and shared UI targeting state used by this router.
#[derive(Debug, Default)]
pub struct Interaction {
    pub chat_interface: dereth_client_contract::options::interface::Interface,
    pending_chat_entries: Vec<dereth_client_contract::chat::entry::EntryUpdate>,
    chat_target_next: f64,

    /// The running program's name and version, as `@version` prints them after "Client version".
    /// The program sets it at start-up; until then it is this library's own fallback.
    pub client_build_id: &'static str,
    /// The world's systems, stamped by the application each frame: a request for a system the
    /// world lacks is refused here.
    pub era_features: dereth_primitives::era::EraFeatures,
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
    /// The command table used to process chat input.
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
    /// Standing-still state pushed in by [`draw_use_time_with_chat_focus`] from the body's
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
    /// [`draw_use_time_with_chat_focus`] step 1 writes every pointer event's position before dispatch,
    /// so this value is the position used to deliver that event.
    cursor: (i32, i32),
    /// The render target width and height
    /// — the back buffer, which is what every caller of [`draw_use_time_with_chat_focus`] passes.
    ///
    /// Kept here for the same reason as [`Interaction::cursor`]: the drop arm is reached from
    /// [`Interaction::run_ui_requests`], which has no viewport argument and has three callers
    /// outside this file. Written by [`draw_use_time_with_chat_focus`] before step 2, so a pick armed by a drop is
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
    /// `false` at the one site that consumes it.
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
    /// The requests this frame produced, drained into the session by [`draw_use_time_with_chat_focus`].
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
    /// Host-loaded trade-note denominations used by shared payment gestures.
    pub trade_note_values: Vec<(u32, u32)>,
    pub journal_coords: Option<(f32, f32)>,
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
    /// Every [`Request`] the last [`draw_use_time_with_chat_focus`] handed to the wire slot, in order.
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
    /// The last frame time [`draw_use_time_with_chat_focus`] was driven with.
    ///
    /// [`apply_events_at_boundary`] runs *before* [`draw_use_time_with_chat_focus`] in `App::frame`, so two of its arms — the
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
    strings: Vec<(u32, String, dereth_client_contract::feedback::Feedback)>,
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
    /// The two notices that resolve the trade panel's pending stack split (the first also
    /// resolves the vendor sell list's), in the exact order raised by the batch.
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
            Notice::DisplayString {
                channel,
                text,
                feedback,
            } => {
                self.strings.push((channel, text.clone(), feedback));
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
    #[must_use]
    pub fn new() -> Self {
        Self {
            client_build_id: CLIENT_BUILD_ID,
            era_features: dereth_primitives::era::EraId::default().features(),
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
    /// * **[`Interaction::chat`]** — the command table belongs
    ///   to process startup, not the character's gameplay UI. Rebuilding it here
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
    /// and the requests the screens queued. Both are acted on in [`draw_use_time_with_chat_focus`], which is where the
    /// client's per-frame interaction update sits.
    pub fn queue(&mut self, mouse: Vec<UiMouseEvent>, requests: Vec<UiRequest>) {
        self.mouse.extend(mouse);
        self.ui_requests.extend(requests);
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

    /// The standing-still predicate's answer, pushed in by [`draw_use_time_with_chat_focus`] from
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
}

/// The action ids this module handles, from the shipped `ActionMap`'s own enum names
/// (`crate::actions::names::ACTION_ENUM_NAMES`).
pub mod action;

/// The `PlayerOption_*` input actions the player system handles, each with the `PlayerOption`
/// ordinal it flips.
///
/// Fifty-one of the fifty-three options: `AppearOffline` (39) and `LockUI` (51) have no such
/// action handled. Hear-PK-deaths is the newest retail row; the show-cloak row is this client's
/// (the end-of-retail client names the action and does nothing with it).
pub const PLAYER_OPTION_ACTIONS: [(u32, usize); 51] = [
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
    // Not the end-of-retail client's: its show-cloak action does nothing. Here it flips the
    // option as the others do.
    (0x1000_012F, 50), // ShowCloak
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
    let (north_south, east_west) =
        dereth_primitives::position::landscape_coordinates(east_west, north_south);
    Some(dereth_client_model::quests::location_string(
        north_south,
        east_west,
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
mod tests;

#[cfg(test)]
mod shared_social_tests;

#[cfg(test)]
mod feedback_tests;
