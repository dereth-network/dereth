//! `GamePlayScreen` — mode `0x10000008`, the in-game HUD's single root and the sixteen windows.
//!
//! Everything the player interacts with in the game is a **child element of this screen's one
//! root**, not a framework of its own. The screen itself does five things: it creates
//! the root, places the sixteen windows from the screen-layout file, cascades the UI lock, routes
//! four hot keys, and queues the two exits.
//!
//! # The HUD itself
//!
//! Because the sixteen windows are *elements* of this one screen rather than screens of their own,
//! the post-init set-up of every one of them is this file's too — this crate's element factory
//! produces a `PlainElement` for all 84 game types on purpose, so whoever owns the subtree does
//! the binding. [`GamePlayScreen::post_init`] is that, and the part of it a player can see is the
//! three panel-stack child set-ups: `PanelStack`, `EnvironmentPanelStack` and `CombatPanelStack`
//! each end their child set-up by hiding **every page they just registered**, and not one of those
//! twenty-three pages carries `UICore_Element_hide` (`0x3B`) in the shipped layouts, so all
//! twenty-three come up visible. Without the loops the inventory, the options pages and the admin
//! panel all draw at once over the world.
//!
//! What is live from server state: the vitals ([`GamePlayScreen::update_vitals`]), the toolbar's
//! selected-object read-out, the radar's coordinates and compass ([`GamePlayScreen::update_radar`])
//! and the chat ([`GamePlayScreen::recv_display_final_string_info`]). Every window's start state is
//! [`GamePlayScreen::HUD_START_VISIBILITY`], and the server's own placement blob then overrides
//! part of it in [`GamePlayScreen::update_from_player_module`].

use dereth_primitives::{LocalTime, ObjectId};
use dereth_ui::framework::ScreenCx;
use dereth_ui::framework::{LayoutEnum, Screen};
use dereth_ui::persist::{SavedWindow, ScreenLayout};
use dereth_ui::{
    ElemHandle, ElementId, ElementMessage, MessageId, NoticeId, UiError, UiMode, UiSystem,
};

use crate::bind::{attr, attr_enum, attr_int, bind_children, set_attr_float, Bound};
use crate::chat::interface::{ChatInterface, ChatMessage, Routed};
use crate::hud::floaty::{WindowPlacements, GAMEPLAY_WINDOWS, READS_PLACEMENT_VISIBILITY};
use crate::panels::panel_stack::PanelStack;
use crate::toolbar::Toolbar;
use crate::view::{GameView, UiRequest, Vital};

/// The screen's child set-up: one root element, the gameplay UI.
///
/// Retail creates the root from element `0x10000495` without naming the layout enum. That enum
/// is **`0x10000006`** (`classic_gameplay` → layout `0x21000005`), the only shipped layout whose
/// root element is `0x10000495`, as verified against the shipped layout index and enum map. Note
/// that this enum is *not* one of the three unregistered **mode**
/// ids that share its numbering; layout enums and mode ids are different spaces.
const LAYOUT: LayoutEnum = LayoutEnum(0x1000_0006);
const ROOT: ElementId = ElementId(0x1000_0495);

/// The screen's own listener id, the same shape every other screen in this crate uses.
const ME: dereth_ui::ListenerId = dereth_ui::ListenerId::External(LAYOUT.0);

/// The four hot keys handles directly, on global message 1.
pub mod action {
    /// Log out to character select.
    pub const LOGOUT_TO_SELECT: u32 = 0x1000_0026;
    /// Log out and quit.
    pub const LOGOUT_AND_QUIT: u32 = 0x1000_0027;
    /// Toggle whole-UI visibility.
    pub const TOGGLE_UI: u32 = 0x54;
    /// Open the help window (arguments `0`, `0x10000001`).
    pub const OPEN_HELP: u32 = 0x7B;
}

/// Leaving the game from inside it: the two *Gameplay Options* buttons and the
/// confirmation one of them raises.
///
/// The client takes these actions for element message 1:
///
/// | element | what the client does |
/// |---|---|
/// | `0x10000203` *Exit to Character Selection* | the end-character-session notice, argument 1 |
/// | `0x10000617` *Exit Game* | a broadcast of global message 1 with `0x10000027` — the synthesised **quit** action |
///
/// Both ids are in the shipped `classic_gameplay` layout, at (26, 20) and (26, 60) of the options
/// page — exactly where the paired retail sweep photographed them. **The buttons were never the
/// missing piece; the handler was.** `GameplayOptionsPanel` is a `PlainElement` in this build, so
/// its subtree's messages bubble to this screen, which is where the client's own
/// end-character-session handler lives anyway.
///
/// Note the asymmetry, which is the client's and is reproduced rather than tidied: *Exit to
/// Character Selection* **asks**, and *Exit Game* does not — the client's
/// `0x10000027` arm sets all three flags and ends the session on the spot.
pub mod logout {
    use dereth_ui::framework::LayoutEnum;
    use dereth_ui::ElementId;

    /// *Exit to Character Selection*.
    pub const EXIT_TO_CHARACTER_SELECTION: ElementId = ElementId(0x1000_0203);
    /// *Exit Game*.
    pub const EXIT_GAME: ElementId = ElementId(0x1000_0617);

    /// The `Dialog` layout — enum **2**, `0x2100003C` in this dat build.
    pub const DIALOG_LAYOUT: LayoutEnum = LayoutEnum(2);
    /// The confirmation dialog's root inside it: element `0x15`, type `0x13`
    /// (`dereth_ui::dialog::DialogKind::Confirmation::root_element_id`). Dumped from the shipped
    /// layout, the subtree is the panel `0x3D` holding the body text `0x3E` and the button strip
    /// `0x1000032F`, whose two `Button`s are `0x17` at x 80..159 and `0x19` at
    /// x 240..319 — *Yes* on the left and *No* on the right, as in the retail frame.
    pub const CONFIRMATION_ROOT: ElementId = ElementId(0x15);
    /// The dialog's `child::BUTTON1` — *Yes*.
    pub const BUTTON_YES: ElementId = dereth_ui::dialog::base::child::BUTTON1;
    /// `child::BUTTON2` — *No*.
    pub const BUTTON_NO: ElementId = dereth_ui::dialog::base::child::BUTTON2;
    /// The `TextElement` the prompt is written into. See
    /// [`dereth_ui::dialog::base::child::TEXT`] for why this is `0x3E` and not `0x3D`.
    pub const PROMPT_TEXT: ElementId = dereth_ui::dialog::base::child::TEXT;

    /// The table enum the end-character-session and log-off handlers build their
    /// `StringInfo` in — **`0x10000001`**, not the `0x10000002` every pre-game dialog uses.
    pub const STRING_TABLE_ENUM: u32 = 0x1000_0001;
    /// The client's string, verified against the shipped
    /// table: *"This will exit your character from the game world."* then *"Are you sure?"*,
    /// which is the sentence in the paired retail frame.
    pub const END_SESSION_CONFIRM: &str = "ID_Client_EndCharacterSessionConfirm";
    /// The client's: *"… and close the application."*
    pub const LOGOFF_CONFIRM: &str = "ID_Client_LogoffConfirm";

    /// The string the log-off handler shows when it refuses a log-off because
    /// the player is off the ground (not in contact): it builds a string info, sets this as its
    /// literal value, and displays it on chat type `0x1A`.
    ///
    /// It is a plain wide literal, 32 code units including the terminator.
    pub const AIRBORNE_REFUSAL: &str = "Cannot log off while in mid-air.";
    /// The chat type pushed — `dereth_client_model::scroll::LOCAL_ERROR_TYPE`, the one
    /// accepted by the chat window and displayed without a timestamp.
    pub const AIRBORNE_REFUSAL_CHAT_TYPE: u32 = 0x1A;
}

/// The element ids whose visibility change reports to the
/// server: `0x10000505`, or anything in `0x1000050E..=0x10000510` — i.e. the four floaty chat
/// windows.
#[must_use]
pub fn reports_panel_visibility(id: ElementId) -> bool {
    id.0 == 0x1000_0505 || (0x1000_050E..=0x1000_0510).contains(&id.0)
}

/// The notices the screen registers for: the logout pair. Nothing in the client sends the log-off
/// notice; the registration is the seam anything that does send it lands on.
pub const NOTICES: [NoticeId; 2] = [NoticeId::EndCharacterSession, NoticeId::Logoff];

/// One row of [`GamePlayScreen::HUD_START_VISIBILITY`]: an element, the state the HUD comes up in,
/// and the sentence that justifies it.
///
/// The `why` string is not decoration. Some rows are a rule read from the client and some are a
/// live observation whose mechanism is not known, and a reader has to be able to tell which without
/// leaving the table — so the acceptance test asserts that every `UNVERIFIED` row names its open
/// question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartVisibility {
    pub element: ElementId,
    pub visible: bool,
    pub why: &'static str,
}

const fn sv(element: ElementId, visible: bool, why: &'static str) -> StartVisibility {
    StartVisibility {
        element,
        visible,
        why,
    }
}

// The three chat-window children live in `crate::chat::window`, which is where they are
// documented: the log is `0x10000011`, the **editable** entry is `0x10000016` and the "new text
// below" arrow is the 16 x 16 button `0x1000048C`. Measured off the shipped layout.

/// One element's own parent-relative rectangle, or an empty one when the handle is stale.
///
/// The box is inclusive (`x1 >= x0`), which is why the
/// compass-token centring below divides `width()`/`height()` rather than `x1 - x0`.
fn element_box(ui: &UiSystem, h: ElemHandle) -> dereth_ui::Box2D {
    ui.node(h)
        .map_or_else(dereth_ui::Box2D::default, |n| n.region.box_)
}

/// Every element of one **type** in a subtree, in tree order.
///
/// The recursive child lookup searches by *id*, and an indicator lamp has no id worth naming: which
/// element in which window is a lamp is layout data, and the shipped layout has the same lamp type
/// twice. This is the search the client's element-class registration does implicitly by handing every instance
/// of the class its own object.
/// The element's glyphs as a string.
///
/// `dereth_ui::text::TextElement` keeps the text as `GlyphList` and exposes no whole-string reader
/// (only `selected_text`), so the read is done here rather than by widening the UI crate's API.
fn read_text(ui: &mut UiSystem, h: ElemHandle) -> String {
    match ui.text_element_mut(h) {
        Some(t) => {
            let u: Vec<u16> = t.glyphs.glyphs.iter().map(|g| g.data).collect();
            String::from_utf16_lossy(&u)
        }
        None => String::new(),
    }
}

/// The element base's "is ancestor of me" from `<SBOX>` — whether a drop landed in the 3D
/// viewport.
///
/// `WorldView` is registered for element message `0x15` on its own subtree and forwards it into
/// its drop handling, so what makes a drop "into the world" is that the target element
/// is the smart box or something inside it — the same ancestor walk `Toolbar` does for its
/// eighteen shortcut lists.
fn is_in_world_view(ui: &UiSystem, mut h: ElemHandle) -> bool {
    loop {
        if ui.node(h).map(dereth_ui::ElementNode::element_id) == Some(window::SMART_BOX) {
            return true;
        }
        match ui.parent(h) {
            Some(p) => h = p,
            None => return false,
        }
    }
}

/// The same ancestor walk, by element id rather than by handle.
fn is_under_element(ui: &UiSystem, mut h: ElemHandle, id: dereth_ui::ElementId) -> bool {
    loop {
        if ui.node(h).map(dereth_ui::ElementNode::element_id) == Some(id) {
            return true;
        }
        match ui.parent(h) {
            Some(p) => h = p,
            None => return false,
        }
    }
}

fn elements_of_type(
    ui: &UiSystem,
    root: ElemHandle,
    ty: dereth_ui::ElementType,
) -> Vec<ElemHandle> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(h) = stack.pop() {
        if ui.node(h).is_some_and(|n| n.ty() == ty) {
            out.push(h);
        }
        stack.extend(ui.children(h));
    }
    out
}

/// Element attribute **`0x12`** — the `InputAction` a `Button` fires when it is clicked.
///
/// **A re-export, not a second definition.** `dereth_ui::props::attr::BUTTON_INPUT_ACTION` is the
/// one, and `dereth-ui`'s tests pin it as the literal `0x12`. Two definitions of one transcribed
/// constant can drift apart while every test that reads either through its own symbol stays
/// green.
pub use dereth_ui::props::attr::BUTTON_INPUT_ACTION;

/// One indicator lamp of the strip, bound by element **type** the way the client's element-class
/// registration binds them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IndicatorLamp {
    handle: ElemHandle,
    /// Which of the six classes this element is.
    ty: dereth_ui::ElementType,
    /// The effect-type indicator kind, attribute `0x1000000C`. Meaningless
    /// for the other five classes and left at [`crate::hud::indicators::effects_kind::BOTH`].
    kind: u32,
    /// The last state written, so a lamp is only restated when its answer changes — the client's
    /// lamps are driven by notices and restate only when one arrives.
    state: Option<u32>,
}

/// One lamp's update, dispatched on its element type — the six lamp rows.
///
/// `None` means "this build's [`GameView`] does not carry that lamp's input". Every non-timed lamp
/// in the shipped tree now has one; the link lamp retains its separate time-based machine.
///
/// **Why this is not [`crate::hud::indicators::all_states`]**, which is the same six update
/// functions and is the obvious thing to call. `all_states` takes **one** `effects_kind` and
/// returns the six states in `INDICATORS` order; the shipped strip carries the effects lamp
/// **twice**, `0x1000000C` = 1 at `0x100000F5` and = 2 at `0x100000F6`, so one kind for both is
/// wrong by construction — it lights the debuff lamp for a buff. It is also keyed by position in a
/// fixed six-element array where the elements are found by *type*, and the shipped layout has no
/// portal-storm element at all. So `all_states` remains a correct statement of the six updates
/// and a **dead transcription for this layout**; it is left in place with its own tests rather
/// than deleted, and this note is what stops anyone wiring the wrong one.
fn indicator_state(view: &dyn GameView, ty: dereth_ui::ElementType, kind: u32) -> Option<u32> {
    use crate::element_types::ty as t;
    use crate::hud::indicators as ind;
    Some(match ty {
        t::BURDEN_INDICATOR => ind::burden_state(view.load()),
        t::EFFECTS_INDICATOR => {
            let (helpful, harmful) = view.enchantment_counts();
            ind::effects_state(kind, helpful, harmful)
        }
        t::MINI_GAME_INDICATOR => {
            ind::minigame_state(view.minigame().is_some_and(|game| game.visible))
        }
        t::PORTAL_STORM_INDICATOR => ind::portal_storm_state(view.portal_storm_level()).0,
        t::VITAE_INDICATOR => ind::vitae_state(view.vitae()),
        _ => return None,
    })
}

/// The camera scale forces **every frame** while a camera set exists: the scale is not a one-off
/// setting.
pub const CAMERA_SCALE: f32 = 1.1;

/// The windows whose element ids are named individually, so a call site never spells one.
pub mod window {
    /// `<SBOX>` — the 3D viewport.
    ///
    /// It lives in [`dereth_client_contract::gameplay`], because
    /// `dereth_client::interaction` names it to tell a world click from a panel click.
    pub use dereth_client_contract::gameplay::SMART_BOX;
    use dereth_ui::ElementId;
    /// `<CHAT>` — the main chat window.
    pub const MAIN_CHAT: ElementId = ElementId(0x1000_0601);
    /// `<EXAM>` — the examination panel.
    pub const EXAMINATION: ElementId = ElementId(0x1000_05F7);
    /// `<VITS>` — the stacked vitals bar.
    pub const STACKED_VITALS: ElementId = ElementId(0x1000_05FA);
    /// `<SVIT>` — the side-by-side vitals bar.
    pub const SIDE_VITALS: ElementId = ElementId(0x1000_06D5);
    /// `<ENVP>` — the environment panel.
    pub const ENV_PANEL: ElementId = ElementId(0x1000_05FD);
    /// `<PANS>` — the toolbar's panel stack.
    pub const PANEL_STACK: ElementId = ElementId(0x1000_05FF);
    /// `<TBAR>` — the toolbar.
    pub const TOOLBAR: ElementId = ElementId(0x1000_0603);
    /// `<PBAR>` — the power / accuracy bar.
    pub const POWER_BAR: ElementId = ElementId(0x1000_0613);
    /// `<COMB>` — the combat panel.
    pub const COMBAT_PANEL: ElementId = ElementId(0x1000_06B5);
    /// `<RADA>` — the radar.
    pub const RADAR: ElementId = ElementId(0x1000_06D2);
    /// The seventeenth child of the gameplay root, which the sixteen-window table does not name:
    /// element `0x1000001F` of layout `0x21000009` (`classic_keyboard`) — `KeyboardPanel`'s
    /// key-mapping window. It has no `0x1000007E` window id and no `0x10000029` panel id.
    pub const KEYBOARD: ElementId = ElementId(0x1000_04A8);
    /// The eighteenth: element `0x100004AC` of layout `0x21000058` (`classic_admin`) —
    /// `AdminPropertiesPanel`. Same shape: no window id, no panel id.
    pub const ADMIN: ElementId = ElementId(0x1000_04D1);
    /// The **second** `PowerBar` (type `0x1000000F`), inside `<SBOX>` rather than in the
    /// `<PBAR>` floaty wrapper: the strip across the bottom of the viewport.
    pub const SMART_BOX_POWER_BAR: ElementId = ElementId(0x1000_0044);
    /// `BarberPanel` (type `0x1000004A`), also inside `<SBOX>`.
    pub const BARBER: ElementId = ElementId(0x1000_0598);
    /// `VividTargetIndicator`'s on-screen bracket group — its four corner children.
    pub const TARGET_ON_SCREEN: ElementId = ElementId(0x1000_0038);
    /// The off-screen indicator, child `0x10000045`.
    pub const TARGET_OFF_SCREEN: ElementId = crate::hud::target::OFF_SCREEN_ELEMENT;
    /// `InventoryPanelStack`'s page inside `<PANS>` — the eleventh id of
    /// [`crate::panels::catalogue::PANEL_PAGES`], and the only page whose type
    /// (`0x10000023`) is the inventory. It is named because
    /// `InventoryPanelStack`'s three sub-panels are looked up from it, not from the screen root: every
    /// `ItemListWidget` in the tree shares its slots' element ids, so a search that starts
    /// higher finds the wrong subtree.
    pub const INVENTORY_PAGE: ElementId = ElementId(0x1000_018B);
}

/// What `GamePlayScreen` and the ten floating-window player-state update bodies read out of
/// the player system's player module.
///
/// Three separate reads, and they are not interchangeable:
///
/// * the per-window chat option read — [`WindowPlacements`], the per-window
///   position, size, visibility and title;
/// * the side-by-side vitals character option, which decides which of the two vitals windows
///   is up;
/// * the lock-UI flag — the master "don't move the HUD" flag.
// `Eq` is not derivable here: the two chat opacities are `f32`, and this type
// is only ever compared for equality in tests.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayerSettingsView {
    pub placements: WindowPlacements,
    pub side_by_side_vitals: bool,
    pub lock_ui: bool,
    /// Gameplay-option property `0x10000080` `Option_DefaultOpacity` — the Chat Options page's
    /// *idle* chat opacity slider, and `None` when the module carries no value.
    ///
    /// The floaty chat panel re-reads the per-window properties
    /// "filter, position, opacity" out of the blob; this is the opacity half, decoded by the host
    /// because a `Screen` has no player system.
    pub chat_default_opacity: Option<f32>,
    /// Gameplay-option property `0x10000081` `Option_ActiveOpacity` — the *focused or
    /// moused-over* slider. See [`Self::chat_default_opacity`].
    pub chat_active_opacity: Option<f32>,
    /// The per-window chat option `0x1000007F` for every window the blob carries a row for — the
    /// client's one read of it.
    ///
    /// `(window id, mask)` pairs, not a map: five windows are the whole domain and the order is
    /// the array's. A window absent from this list keeps the default mask the client's `switch`
    /// gave it, which is what the client's
    /// per-window chat option read answering `false` leaves behind.
    pub chat_filters: Vec<(u32, u64)>,
}

/// `Radar`'s eight layout-named children, resolved through the attributes that name them.
///
/// Reads attributes `0x10000031`…`0x10000038`,
/// each of which holds a **child element id**, and caches the child it names. Nothing here is
/// hard-coded, which is the point: the compass tokens and the coordinate read-outs are wherever the
/// layout puts them.
/// `MapPanel`'s five cached children and the marker area it reads off the map image.
///
/// `crate::mapradar::map` holds the arithmetic and `panels::catalogue::MAP` the five element ids;
/// this resolves them. Without it the map panel draws its image and its 53 town markers from
/// layout data and no code ever writes its date, its coordinates, or moves its player icon.
#[derive(Debug, Clone, Copy, Default)]
pub struct MapChildren {
    /// The date/time text — the only one of the five that is never gated on being outdoors.
    pub date_time_text: Option<ElemHandle>,
    /// The map image, the element the marker area is read off.
    pub map_image: Option<ElemHandle>,
    /// The player location icon — the green circle.
    pub player_icon: Option<ElemHandle>,
    /// The house location icon.
    pub house_icon: Option<ElemHandle>,
    /// The coordinate text.
    pub coordinate_text: Option<ElemHandle>,
    /// The map marker area, attributes `0x1000004E`…`0x10000051`.
    pub marker_area: crate::mapradar::map::MarkerArea,
    /// The `MapPanel` element itself, [`crate::mapradar::map::MAP_PAGE`].
    ///
    /// The client's `0x18` arm only acts when the source is the page itself, so
    /// the page needs a handle of its own to recognise its own visibility edge; the five children
    /// above are not it. `None` is a layout with no map page, which is the same silent skip every
    /// other binding here takes.
    pub page: Option<ElemHandle>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct RadarChildren {
    pub north: Option<ElemHandle>,
    pub south: Option<ElemHandle>,
    pub east: Option<ElemHandle>,
    pub west: Option<ElemHandle>,
    pub coordinate_container: Option<ElemHandle>,
    pub combined_coords: Option<ElemHandle>,
    pub x_coord: Option<ElemHandle>,
    pub y_coord: Option<ElemHandle>,
    /// The centre point: the **struct** attribute `0x1000002E`, whose two `Integer` members are
    /// named `0x1000002F` (x) and `0x10000030` (y). See [`Self::radius`]'s neighbour in
    /// `radar_post_init` for why that is not three attributes.
    pub center: (f32, f32),
    /// The radar radius, attribute `0x1000002D`.
    pub radius: i32,
    /// Each compass token's distance from the centre, in [`crate::mapradar::radar::Compass::ALL`]
    /// order.
    pub magnitudes: [f32; 4],
    /// The window id, attribute `0x1000007E` — the key every per-window chat option row is stored
    /// under. The shipped `classic_gameplay` layout gives the radar **14**.
    pub window_id: u32,
    /// The object under the mouse — recomputed by every radar update, which is
    /// [`GamePlayScreen::update_radar`], and read by the select arm of
    /// the element-message handler.
    pub object_under_mouse: Option<dereth_primitives::ObjectId>,
    /// Where the radar window was the last time [`GamePlayScreen::update_radar`] looked.
    ///
    /// The stand-in for an overridden move. The client persists a dragged window from inside its
    /// own move handling; this crate's [`dereth_ui::UiSystem::move_to`] is a plain function, so the
    /// screen that owns the window watches its origin instead and emits the same two per-window
    /// option writes when it changes.
    pub last_origin: Option<(i32, i32)>,
}

/// The mouse X minus the element's screen X0, and the same for Y — the pointer in one
/// element's own coordinates, which is the space hit-tests its blips in.
fn cursor_in(ui: &UiSystem, h: ElemHandle) -> (i32, i32) {
    let o = ui.screen_box(h);
    let (x, y) = ui.mouse_pos();
    (x - o.x0, y - o.y0)
}

/// The two `Option_Placement` members written by the client.
///
/// The full row has more members; these are the X and Y, and they are the same two ids every
/// floating-window move writes.
pub mod placement {
    /// X.
    pub const X: u32 = 0x1000_0086;
    /// Y.
    pub const Y: u32 = 0x1000_0087;
}

/// What one call to [`GamePlayScreen::update_toolbar_selection`] did.
///
/// The toolbar's selection-changed handler is one function with two guarded halves, and
/// the host counts them separately: `HudStats::toolbar_written` is the read-out and
/// `HudStats::split_gate_runs` is the splitter's **settling** counter. Two fields rather than one
/// `bool` because they genuinely disagree — a `wants_reseed` tick runs the splitter with the
/// read-out guard closed, and the first idle frame runs the read-out with the client's edge test
/// false.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ToolbarSelection {
    /// The name and the two meters were rewritten.
    pub wrote: bool,
    /// The splitter block ran — the edge fired, or `wants_reseed` did.
    pub split_gate_ran: bool,
    /// The client writes across every live item list on this edge —
    /// the rings that went up and the rings that came down.
    pub rings: usize,
    /// Which vital query the not-a-stack arm decided to send.
    ///
    /// `None` means the arm was not reached at all — a stack, a deselect, nothing selected, or a
    /// call in which the splitter half did not run. [`SelectionQuery::NotAsked`] is the third
    /// state and the one that matters: the arm *was* reached and the host answered no
    /// [`GameView::selection_query_facts`], so nothing could be decided. Without it a host with no
    /// producer is indistinguishable from a selection that legitimately wanted neither query.
    pub query: Option<SelectionQuery>,
    /// `QueryHealth(0)` / `QueryItemMana(0)` — the edge block's **clear**, raised only
    /// for a meter that was visible.
    pub cleared: bool,
}

/// What the client's not-a-stack arm asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionQuery {
    /// `QueryHealth` for the selected object.
    Health,
    /// `QueryItemMana` for the selected object.
    ItemMana,
    /// The mana leg with `is_owned_by_player` false: the client sends **nothing**, which is a real
    /// outcome and not a failure.
    Neither,
    /// The host does not implement [`GameView::selection_query_facts`], so the branch could not be
    /// evaluated. Distinct from [`Self::Neither`] on purpose.
    NotAsked,
}

/// `GamePlayScreen` — mode `0x10000008`.
#[derive(Debug, Default)]
pub struct GamePlayScreen {
    roots: Vec<ElemHandle>,
    /// The FPS display. It is bound only by the enabled
    /// arm of the framerate-display notice and cleared again by the disabled arm; `None` is not
    /// merely hidden, it means the per-frame update must not overwrite the authored child
    /// caption.
    fps_display: Option<ElemHandle>,
    /// Whether the layout came from file — set from `load_screen_layout`'s return, and what
    /// suppresses the server-side per-window position restore.
    pub layout_from_file: bool,
    /// The lock-UI flag's current value, as last cascaded.
    pub locked: bool,
    /// Whether ending the session also quits the application.
    pub should_quit_on_logout: bool,
    /// Whether the player has confirmed the log-out.
    pub logout_confirmed: bool,
    /// A pending end-session request.
    pub do_end_session: bool,
    /// The ending-session latch, taken before [`Self::logout_confirmed`] is
    /// looked at and cleared when the player
    /// answers. Without it the confirmation could never come back: the frame that raised the
    /// dialog would already have decided.
    pub ending_session: bool,
    /// The player's transient-state bit 1 (`CONTACT_TS`), host-pushed.
    ///
    /// The client's non-quit branch refuses a log-off while the player is **not** in
    /// contact with the ground, says so on channel `0x1A`, clears [`Self::ending_session`] so a
    /// *later* press is heard and clears [`Self::do_end_session`] so *this* one is thrown away
    /// (the refusal is a cancellation, not a deferral). `false` — the `Default` —
    /// is retail's "on the ground", so a screen nobody pushes to
    /// behaves as a standing player does. The host writes it every frame from
    /// `dereth_client::character::Character::in_contact` — `CONTACT_TS` alone, which is the bit the
    /// client tests, and deliberately not `on_ground`'s `Contact && OnWalkable` pair.
    pub player_airborne: bool,
    /// How many times the per-frame update has refused a log-off for being airborne.
    ///
    /// The client's refusal is a display-string notice on chat type `0x1A`, carrying
    /// [`logout::AIRBORNE_REFUSAL`]. The count is kept because it is the one observable that
    /// separates "refused" from "never looked".
    pub logoff_refusals: u64,
    /// The log-out dialog context, reduced to the element it names.
    ///
    /// In the client this is a `DialogController` context and the element is the factory's;
    /// the client's guard is "no context yet", which is `is_none()` here. See the note on
    /// [`Self::make_logout_confirmation_dialog`] for what is not the factory's yet.
    logout_dialog: Option<ElemHandle>,
    /// The `StringInfo`'s **symbolic id**, from table enum
    /// [`logout::STRING_TABLE_ENUM`]. This crate has no string service, so the host resolves it and
    /// hands the text back, exactly as it does for the disconnected screen's reason and the
    /// character screen's delete phrase.
    pub logout_prompt: Option<String>,
    /// The resolved sentence last written into the dialog.
    pub logout_prompt_text: Option<String>,
    /// Whether [`Self::show_logout_prompt`] has written the current [`Self::logout_prompt`].
    logout_prompt_applied: bool,
    /// The UI-visibility toggle / the framework's shown state.
    pub shown: bool,

    // ---- the drop hint ---------------------------------------------------------------------
    /// The drag proxy's payload element — [`crate::items::widget::DragStart::drag_icon`], which is
    /// the element the drag-icon preparation wrote the five drop-icon-info properties onto
    /// and which the drag-and-drop start copies to make the drag element.
    ///
    /// The client reads the payload straight off the drag element; `dereth_ui` does not expose that
    /// handle, so the screen keeps the one it started the drag from. `None` means no item drag of
    /// this screen's is in flight.
    pub drag_payload: Option<ElemHandle>,

    // ---- the HUD's own children ------------------------------------------------------------
    /// `PanelStack` inside `<PANS>` — the sixteen toolbar pages.
    pub panels: PanelStack,
    /// `EnvironmentPanelStack` inside `<ENVP>` — its five pages.
    pub env_panel: PanelStack,
    /// `CombatPanelStack` inside `<COMB>` — its two pages.
    pub combat_panel: PanelStack,
    /// The world's era's features as [`Self::apply_era`] last applied them; `None` until the
    /// first frame, and again whenever the toolbar is rebuilt.
    pub era_features: Option<dereth_primitives::EraFeatures>,
    /// The toolbar's panel-button array.
    pub toolbar: Toolbar,
    /// The toolbar panel's post-init's nine named children.
    pub toolbar_children: Bound,
    /// The two `VitalsPanel` subtrees, bound separately because both carry the same six child ids
    /// and a recursive lookup from the screen root would always find `<VITS>`'s.
    pub stacked_vitals: Bound,
    /// See [`Self::stacked_vitals`].
    pub side_vitals: Bound,
    /// `ChatInterface`, one per chat window, in [`crate::chat::interface::window`] id order:
    /// main (8) then floaty 1…4 (2…5).
    pub chat: Vec<ChatInterface>,
    /// The three bound children of each chat window, in the same order as [`Self::chat`] —
    /// the log (`0x10000011`), the entry (`0x10000016`) and the "new text below" arrow
    /// (`0x1000048C`).
    pub chat_windows: Vec<crate::chat::window::ChatWindow>,
    /// `MainChat`'s own state — the talk-focus menu and the chat-target caption. Without it the
    /// menu table has no reader and every line goes out as speech.
    pub main_chat: crate::chat::mainchat::MainChatPanel,
    /// Whether the last speakable target is squelched, pushed in by the
    /// host the way [`Self::reply_targets`] is — `dereth_client_model::chat::ChatState::is_squelched` is on
    /// the far side of this crate's seam. It is what the squelch row negates.
    pub chat_target_squelched: bool,
    /// The auto-target throttle, the client's file-scope "next time".
    pub chat_auto_target_next: f64,
    /// What the object system last answered about the selection and the speakable target, pushed
    /// in by the host once a frame the way [`Self::reply_targets`] and
    /// [`Self::chat_target_squelched`] are. [`Screen::update`] runs the sweep off it.
    ///
    /// An all-default value is "nothing selected, nothing in range", which is what the sweep sees
    /// before a session starts and is exactly the
    /// [`crate::chat::mainchat::AutoTarget::Unchanged`] it answers there.
    pub chat_auto_target_world: crate::chat::mainchat::AutoTargetWorld,
    /// `FloatingChat`, one per floaty chat window, in [`Self::chat`]'s order minus the main
    /// window: the title, the close button and the four placement write-backs.
    pub floaty_chat: Vec<crate::chat::floaty::FloatyChatUI>,
    /// What the client's three reply keys need, pushed in by the host
    /// the way [`Self::set_lock_ui`] pushes the lock-UI flag.
    pub reply_targets: crate::chat::window::ReplyTargets,
    /// The stay-in-chat-mode option — read before the chat entry decides whether Enter also drops
    /// focus.
    pub stay_in_chat_mode: bool,
    /// The resolved `ID_AssistedTell` row, when the host has a string service; empty falls back to
    /// [`crate::chat::window::ASSISTED_TELL_FALLBACK`].
    pub reply_template: String,
    /// `ClientOptionsPanel` — the Client Options page's option box and its option array.
    ///
    /// The page's post-init binds `0x10000200` and calls its option build; without either the page
    /// draws an empty box and there is no slider on the screen to move. See
    /// [`crate::options::page`].
    pub config_page: crate::options::page::PlayerOptionPage,
    /// `CharacterSettingsPanel` — the Character Options page's 62 rows.
    ///
    /// The page's post-init binds `0x100001FA` and calls its option build; without either the page
    /// draws an empty box.
    pub character_options: crate::options::character::CharacterSettingsPage,
    /// `ChatOptionsPanel` — the Chat Options page's two opacity sliders and five 64-bit filter
    /// controls.
    ///
    /// The page's post-init binds `0x1000050D` and calls its option build; without either the
    /// page's six window descriptors have no consumer at all, and the tab draws an empty box. See
    /// [`crate::options::chat`].
    pub chat_options: crate::options::chat::ChatOptionsPage,
    /// Visibility edges and `0x100001FC`/`FD`/`FE` presses on the Chat Options page, queued for
    /// [`Self::drive_chat_options`] for exactly the reason
    /// [`Self::character_option_visibility`] is: the show arm re-reads every control from the
    /// `PlayerModule` and a `Screen` handler has no [`GameView`].
    chat_option_visibility: Vec<bool>,
    /// See [`Self::chat_option_visibility`].
    chat_option_buttons: Vec<ElementId>,
    /// The edge this screen has already
    /// answered — see [`Self::drive_chat_font`].
    font_preference_epoch: u64,
    /// `KeyboardPanel` — the six key-binding list boxes and one `ActionKeyMapOption` row per
    /// user-bindable action.
    ///
    /// [`Self::post_init`] binds it; the rows themselves cannot be built here
    /// because every one of them reads the merged master input map, which lives in the host's
    /// `InputManager` and not in the UI tree — see [`Self::key_bindings_init_options`].
    pub key_bindings: crate::options::keybinding::KeyBindingPage,
    /// Element messages addressed to a [`Self::key_bindings`] row, held until the host can hand
    /// the page its `InputManager`.
    ///
    /// **The reason is `dereth_ui::Delivery`'s.** A `Screen` handler is given
    /// `&mut UiSystem` and nothing else, so a listener that needs a system this crate does not own
    /// cannot be dispatched inside the broadcast. `crate::requests` solves the same problem the
    /// same way for `UiRequest`: append in dispatch order, drain after the broadcast returns. Here
    /// the missing system is `dereth_input::InputManager`, and the drain is
    /// [`Self::drive_key_bindings`].
    ///
    /// It also removes the re-entrancy hazard outright: every write a row makes — the string-info
    /// attribute write on a key button, a state change, a dialog — happens **after** the broadcast
    /// has unwound, so nothing is read back out of the arena from inside its own handler.
    key_binding_inbox: Vec<ElementMessage>,
    /// A key press captured while a [`Self::key_bindings`] row is in capture, held for the same
    /// drain. The control chord is what the key press hands the row.
    key_binding_key_hits: Vec<dereth_input::ControlChord>,
    /// What [`Self::drive_key_bindings`] made of the messages addressed to the Key Bindings
    /// page's own four buttons, in dispatch order. Drained by
    /// [`Self::take_key_binding_page_events`].
    key_binding_page_events: Vec<crate::options::keybinding::PageEvent>,
    /// Visibility edges on the Character Options page, held for the same reason
    /// [`Self::key_binding_inbox`] is: the client's show
    /// arm is `save_current_values`, which re-reads every check box from the `PlayerModule`, and a
    /// `Screen` message handler is not given the [`GameView`] that holds one.
    ///
    /// Drained by [`Self::drive_character_options`]. Nothing here substitutes an
    /// empty view for the missing one: that would be a test fixture standing in for the missing
    /// producer, written into production source.
    character_option_visibility: Vec<bool>,
    /// `0x100001FC`/`FD`/`FE` presses attributed to the Character Options page, queued for
    /// [`Self::drive_character_options`] for the reason
    /// [`Self::character_option_visibility`] is.
    character_option_buttons: Vec<ElementId>,
    /// `GameplayOptionsPanel` — the Options *Game / Support* page.
    ///
    /// Bound in [`Self::post_init`]. The page carries no state because retail's carries none: the
    /// class has no post-init set-up, so this is a handle and nothing else, and it exists so a
    /// click on one of the page's seven buttons can be attributed to *this* page before it is acted
    /// on.
    pub gameplay_options: crate::options::gameplay::GameplayOptionsPage,
    /// `Radar`'s layout-named children and geometry.
    pub radar: RadarChildren,
    /// `MapPanel`'s children —
    pub map: MapChildren,
    /// The 53 location notes the map panel creates under the map image, in
    /// [`crate::mapradar::map::MAP_NOTES`] order.
    ///
    /// Held rather than discarded for the reason retail's set-up holds nothing: retail never
    /// touches a note again, but this build has to be able to *show* that it made them, and a
    /// station that re-derives them by walking the map image's children cannot tell a note from the
    /// player icon.
    pub map_notes: Vec<ElemHandle>,
    /// The client's own change guards: the text is set only when it differs. Kept
    /// per element so a redundant write is not counted as a write, the way the radar keeps
    /// `last_coords`.
    last_map_date_time: Option<String>,
    last_map_coords: Option<String>,
    /// The map's next-update time. Read by the client's global-message-3 check (update when
    /// the time has come) and written by `Update`'s own first line — see [`Self::map_update_due`].
    next_map_update: f64,
    /// The window id per window element, in [`GAMEPLAY_WINDOWS`] order.
    pub window_ids: Vec<(ElementId, u32)>,
    /// The last `(current, max)` written into each of the three meters, so an unchanged vital does
    /// not re-run `set_attribute_float`. The client's update is notice-driven, not per-frame.
    last_vitals: [Option<(u32, u32)>; 3],
    /// The toolbar's selected-object id — **the only copy in the workspace.**
    ///
    /// Both halves of [`Self::update_toolbar_selection`] read it: the read-out compares against
    /// `Some(sel)` (the outer `None` is this screen's own "never asked", which the client does not
    /// need because its member starts at 0) and the splitter compares `flatten() != sel`, which is
    /// the client's own "stored id differs from the current selection" edge. There is no second
    /// copy on `dereth_client::hud::Hud`, so the client's one selection edge is one edge and the
    /// two halves cannot disagree.
    last_selection: Option<Option<ObjectId>>,
    /// The current selection **as the input-dispatch boundary last supplied it** — a
    /// different client value from [`Self::last_selection`], which is the toolbar's stored
    /// selected-object id.
    ///
    /// The distinction is written here rather than left to a reader: retail reads the live
    /// selected-object global *inside each callback*, and it reads
    /// the toolbar's own member only in the selection-changed handler's edge test. Those are two
    /// members with two lifetimes, and this build needs two fields for the same reason
    /// `ExaminationPanel::last_selected` is a third — `dereth-ui-screens` cannot see `World`, so the
    /// live value has to be handed in at the boundary where the client would simply read the
    /// global. Two transcriptions of **two** members is what the client does; two of **one** would
    /// be a defect.
    ///
    /// The outer `None` is this screen's "never supplied", which is why
    /// [`Self::on_target_mode_button`] falls back to the toolbar readback for standalone callers
    /// rather than treating an unsupplied context as "nothing selected".
    interaction_selection: Option<Option<ObjectId>>,
    interaction_target_mode: bool,
    /// The last coordinate pair the radar read-out was built for.
    last_coords: Option<(f32, f32)>,
    /// The last heading the compass tokens were placed for.
    last_heading: Option<f32>,

    // ---- the indicator strip ---------------------------------------------------------------
    /// Every `LinkStatusIndicator` in the tree, with its own state machine.
    ///
    /// A `Vec` because the strip exists twice in the shipped layout — inside `<INDI>` and inside
    /// the docked `IndicatorStrip` — and each instance is its own object in the client, registered
    /// for global message 3 in its own right.
    link_lamps: Vec<(ElemHandle, crate::hud::indicators::LinkStatusIndicator)>,
    /// The strip's other five lamp classes, bound the same way and for the same
    /// reason. See [`IndicatorLamp`] and [`indicator_state`].
    indicator_lamps: Vec<IndicatorLamp>,
    /// How many clicks [`Self::handle_button_click`] has turned into an input action — a
    /// denominator, so a test can say *"the arm ran"* rather than *"the panel is up"*.
    pub button_actions_fired: u32,

    // ---- the identify panel -----------------------------------------------------------------
    /// `ExaminationPanel` / `FloatingExamination` — `<EXAM>` `0x100005F7`.
    ///
    /// It is a *window* of this screen's root rather than a page of the panel stack, so it binds
    /// here beside the radar and the toolbar rather than in `RemainingPanels`. It is the consumer
    /// of the `0x00C9` reply to the identify button's `0x00C8`.
    pub examination: crate::panels::examination::ExaminationPanel,
    /// `BookPanel` — `<BOOK>` `0x10000182`, the reader window.
    ///
    /// Unlike `<EXAM>` this *is* a page of the panel stack, and it binds here rather than in
    /// `RemainingPanels` for the same reason the examination panel does: it has a body, an inbound
    /// message and a click, and `RemainingPanels` constructs pages it leaves inert.
    pub book: crate::panels::book::BookPanel,
    /// `BarberPanel` -- `<BRBR>` `0x10000598`.
    pub barber: crate::panels::barber::BarberPanel,
    /// How many presses [`Self::on_vitals_press`] has turned into a state change.
    ///
    /// A denominator: "the press never reached the screen" and "it reached it and the state did
    /// not move" are otherwise the same observation.
    pub vitals_display_toggles: u32,
    /// The four combat-mode buttons, one of which the mode-change notice shows.
    combat_mode_buttons: Vec<(u32, ElemHandle)>,
    /// The combat mode the toolbar icon was last set for.
    last_combat_mode: Option<u32>,
    /// `CombatWindow`'s own element, `0x1000005C` — the first page inside `<COMB>`.
    combat_ui_page: Option<ElemHandle>,
    /// The recklessness field, `0x100005EF`.
    combat_recklessness_field: Option<ElemHandle>,
    /// `SpellcastingPanel`'s own element, `0x10000061` — the second page inside `<COMB>`.
    spellcasting_page: Option<ElemHandle>,
    /// The `(advanced_combat_ui, recklessness advancement class)` the combat cluster was last set
    /// for, so the edge fires when either input moves and not only when the mode does.
    last_combat_ui_inputs: Option<(bool, u32)>,

    // ---- the panels -------------------------------------------------------------------------
    /// `InventoryPanelStack` and its three sub-panels — the twenty-eight `ItemListWidget`s of the
    /// inventory page, and the container the item grid is showing.
    pub inventory: crate::panels::inventory::InventoryPanels,

    // ---- the quickbar -----------------------------------------------------------------------
    /// The shortcut slots — the eighteen shortcut lists and their one slot each.
    pub shortcuts: crate::toolbar::shortcuts::ShortcutBar,

    // ---- the stack splitter -----------------------------------------------------------------
    /// The split size and the maximum split size — the pair the entry box and the slider are two
    /// views of. Both globals in the client; one field here, because `Toolbar` is the only
    /// writer and this screen is `Toolbar`.
    ///
    /// The selection-changed handler seeds it from the stack size in the selection's public
    /// description and resets it to `1 / 1` for anything that is not a stack, which is what
    /// [`Self::update_toolbar_selection`] does.
    pub splitter: crate::toolbar::splitter::Splitter,

    // ---- the stat-management panels' input --------------------------------------------------
    /// Element messages this frame that a stat-management panel or `SpellbookPanel` might own.
    ///
    /// **This queue exists because `RemainingPanels` lives on `dereth_client::hud::Hud`.** Tests
    /// read `app.hud().panels`, so the panels stay there. So the screen — which *is* the registered
    /// listener — records the two message ids those panels switch on and the HUD drains them in the
    /// same frame, which is the same one-step deferral `crate::requests` uses and for the same
    /// borrow reason. Moving the panels here would let this queue go.
    panel_messages: Vec<dereth_ui::ElementMessage>,

    /// Items dropped on `TradePanel`'s own list, `0x10000088`.
    ///
    /// The tuple also carries this gesture's split size and maximum split size. The same one-frame
    /// deferral as [`Self::panel_messages`] exists for the same reason: the trade panel lives on
    /// `dereth_client::hud::Hud` with the rest of `RemainingPanels`, and this screen is the element
    /// the drop message is delivered to. The client has no such hop -- its drop handling *is* the
    /// window's own handler and tests whether the drop target is inside its own items list itself.
    trade_drops: Vec<(ObjectId, u32, u32)>,

    /// Items dropped on either `HousingPanel` payment list, with the two
    /// item-holder words belonging to that gesture. The panel lives across the same HUD seam as
    /// secure trade, so this queue has the same one-frame lifetime.
    slumlord_drops: Vec<(ObjectId, u32, u32)>,

    /// Items whose non-pointer retail producer adds them to the trade at position 0: currently the
    /// trade-an-item shortcut and the authoritative split result.
    trade_offers: Vec<(ObjectId, u32)>,

    /// The items released over the vendor's sell list `0x100000CE` this frame, and
    /// whether the splitter had the whole stack in hand when each was.
    ///
    /// The same one-frame deferral as [`Self::trade_drops`], for the same reason and against the
    /// same client function shape: the drop target inside the sell list, the dropped icon's
    /// info read, and not an alias (flags `0x0E`) -> accept, which is the secure-trade
    /// drop handling with one element id changed. `VendorPanel`
    /// lives on `dereth_client::hud::Hud` with the rest of `RemainingPanels`, so the drop
    /// is recorded here and delivered there.
    ///
    /// The two numbers are the splitter's split and maximum, whose equality is the whole-stack
    /// test `accept_drag_object` forks on. They travel with the drop rather than being read
    /// again on the far side because they are a property of *this* gesture.
    vendor_sell_drops: Vec<(ObjectId, u32, u32)>,

    /// The items released over `SalvagePanel`'s list `0x10000074` this frame.
    ///
    /// The same one-frame deferral as [`Self::trade_drops`] and [`Self::vendor_sell_drops`], for
    /// the same reason and against the same client function shape: the salvage panel's drop
    /// handling is the drop target inside the salvage list, the dropped icon's info read, and
    /// not an alias (flags `0x0E`) -> its drag-accept, which is the secure-trade panel's
    /// drop handling with one element id changed. The panel
    /// lives on `dereth_client::hud::Hud` with the rest of `RemainingPanels`, so the drop
    /// is recorded here and delivered there.
    ///
    /// No ghost, for the reason the trade table and the sell basket have none: `accept_drag_object`
    /// leads to the salvage-list insertion, whose trade-state write of 1 is the **opposite**
    /// polarity to a pack move's waiting-state set (1) -- the row enters the window at once and
    /// nothing has been asked of the shard yet.
    salvage_drops: Vec<ObjectId>,
    /// The components dropped on the Create Spell page's formula this frame. The page lives with
    /// the rest of `RemainingPanels`, so the drop is recorded here and delivered there.
    research_drops: Vec<ObjectId>,
}

impl GamePlayScreen {
    /// The factory registered for this screen class.
    #[must_use]
    pub fn create_screen() -> Box<dyn Screen> {
        Box::new(Self {
            shown: true,
            ..Self::default()
        })
    }

    /// The screen's one root, once created.
    #[must_use]
    pub fn root(&self) -> Option<ElemHandle> {
        self.roots.first().copied()
    }

    /// The smart box panel's set framerate display notice.
    ///
    /// The enable edge resolves the shipped child, updates its localized text when renderer
    /// values are available, then shows it. The disable edge hides the stored child and clears
    /// the pointer. A screen that never receives an enable notice therefore leaves the authored
    /// hidden child and caption untouched.
    pub fn on_set_framerate_display(
        &mut self,
        ui: &mut UiSystem,
        enabled: bool,
        values: Option<(f32, f32)>,
    ) {
        if !enabled {
            if let Some(display) = self.fps_display.take() {
                ui.set_visible(display, false);
            }
            return;
        }

        let Some(display) = self.fps_display.or_else(|| {
            self.root()
                .and_then(|root| ui.get_child_recursive(root, crate::hud::world_view::FPS_DISPLAY))
        }) else {
            return;
        };
        self.fps_display = Some(display);
        if let Some((framerate, degrade)) = values {
            let _ = crate::hud::world_view::update_fps_meter(ui, display, framerate, degrade);
        }
        ui.set_visible(display, true);
    }

    /// The tail of the smart box panel's per-frame step: refresh the FPS meter when the display
    /// is bound.
    pub fn framerate_use_time(&mut self, ui: &mut UiSystem, framerate: f32, degrade: f32) {
        let Some(display) = self.fps_display else {
            return;
        };
        let _ = crate::hud::world_view::update_fps_meter(ui, display, framerate, degrade);
    }

    /// The element messages the stat-management and spellbook panels may own, taken
    /// once per frame by whoever holds those panels. See [`Self::panel_messages`].
    #[must_use]
    pub fn take_panel_messages(&mut self) -> Vec<dereth_ui::ElementMessage> {
        std::mem::take(&mut self.panel_messages)
    }

    /// The items dropped on the secure-trade list this frame; see
    /// [`Self::trade_drops`]. Drained by whoever holds `RemainingPanels`.
    pub fn take_trade_drops(&mut self) -> Vec<(ObjectId, u32, u32)> {
        std::mem::take(&mut self.trade_drops)
    }

    /// Drain this frame's drops on the house purchase / maintenance lists.
    pub fn take_slumlord_drops(&mut self) -> Vec<(ObjectId, u32, u32)> {
        std::mem::take(&mut self.slumlord_drops)
    }

    /// Drain trade items whose native caller supplies an explicit insertion position.
    pub fn take_trade_offers(&mut self) -> Vec<(ObjectId, u32)> {
        std::mem::take(&mut self.trade_offers)
    }

    /// The secure trade panel's trade-an-item shortcut's add-item arm, arriving from the client's
    /// notice rather than from a pointer drag.
    ///
    /// It joins [`Self::trade_drops`] at the panel consumer because the two reach the same place:
    /// the secure-trade panel's drag-accept and its trade-an-item shortcut both end in the same
    /// add-item call at position 0, so there is one insert path and one `0x01F8`, not two. It is
    /// kept in the explicit-position queue because both this caller and the authoritative split
    /// callback pass position 0. The host has already run the ownership and
    /// split guards the notice's own handler runs; what is left is the window's half.
    pub fn offer_trade_item(&mut self, item: ObjectId) {
        self.trade_offers.push((item, 0));
    }

    /// The items dropped on the vendor's sell list this frame; see
    /// [`Self::vendor_sell_drops`]. Drained by whoever holds `RemainingPanels`.
    pub fn take_vendor_sell_drops(&mut self) -> Vec<(ObjectId, u32, u32)> {
        std::mem::take(&mut self.vendor_sell_drops)
    }

    /// The items dropped on the salvage window's list this frame; see
    /// [`Self::salvage_drops`]. Drained by whoever holds `RemainingPanels`.
    pub fn take_salvage_drops(&mut self) -> Vec<ObjectId> {
        std::mem::take(&mut self.salvage_drops)
    }

    /// The objects dropped on the Create Spell page's formula this frame; see
    /// [`Self::research_drops`]. Drained by whoever holds `RemainingPanels`.
    pub fn take_research_drops(&mut self) -> Vec<ObjectId> {
        std::mem::take(&mut self.research_drops)
    }

    /// Apply a parsed layout file to the live tree.
    ///
    /// "For each recognised tag the element is **resized first and moved second**
    /// (`resize_to(w, h)` then `move_to(x, y)`), because `move_to` clamps against the parent."
    /// An unrecognised tag is skipped; the function returns 1 if the file opened at all.
    ///
    /// Returns how many of the sixteen windows were actually placed.
    pub fn load_screen_layout(&mut self, ui: &mut UiSystem, layout: &ScreenLayout) -> usize {
        let Some(root) = self.root() else { return 0 };
        let mut placed = 0;
        for (tag, w) in &layout.windows {
            let Some(slot) = GAMEPLAY_WINDOWS
                .iter()
                .find(|s| std::str::from_utf8(s.tag).is_ok_and(|t| t == *tag))
            else {
                continue;
            };
            let Some(h) = ui.get_child_recursive(root, slot.element) else {
                continue;
            };
            ui.resize_to(h, w.w, w.h);
            ui.move_to(h, w.x, w.y);
            placed += 1;
        }
        self.layout_from_file = true;
        placed
    }

    /// Read the sixteen windows' live rectangles
    /// back out, in file order.
    ///
    /// "the coordinates saved are the element's screen x0, screen y0, width and
    /// height, absolute screen pixels".
    #[must_use]
    pub fn save_screen_layout(&self, ui: &UiSystem) -> ScreenLayout {
        let mut out = ScreenLayout::default();
        let Some(root) = self.root() else { return out };
        for slot in GAMEPLAY_WINDOWS {
            let Some(h) = ui.get_child_recursive(root, slot.element) else {
                continue;
            };
            let Some(tag) = std::str::from_utf8(slot.tag).ok() else {
                continue;
            };
            let (x, y) = ui.screen_origin(h);
            let b = ui.screen_box(h);
            // The UI crate's table is the one the file format is keyed on; borrow its `&'static
            // str` tags so the two can never disagree about spelling.
            let Some(j) = dereth_ui::persist::WINDOWS.iter().find(|w| w.tag == tag) else {
                continue;
            };
            out.windows.push((
                j.tag,
                SavedWindow {
                    x,
                    y,
                    w: b.width(),
                    h: b.height(),
                },
            ));
        }
        out
    }

    /// The lock-UI flag flipped: the radar broadcasts global `0x0D` and every window that
    /// subscribes runs its locked-status update.
    pub fn cascade_lock(&mut self, ui: &mut UiSystem, locked: bool) {
        self.locked = locked;
        let Some(root) = self.root() else { return };
        // The radar's own two children first: the drag button hides when locked.
        if let Some(h) = ui.get_child_recursive(root, crate::mapradar::radar::child::DRAG_BUTTON) {
            ui.set_visible(h, !locked);
        }
        if let Some(radar) = ui.get_child_recursive(root, window::RADAR) {
            self.sync_lock_button(ui, radar);
        }
        // **It hides its eight, it does not swap them.** The smart box has one set of eight rather
        // than two, and retail sets all eight visible exactly when the UI is not locked: it hides
        // them when a player system exists and its UI-lock option is set, and shows them otherwise.
        // So "no player system yet" reads as *unlocked* — the borders are shown, which is what a
        // screen coming up before login draws.
        for id in crate::hud::floaty::SMART_BOX_CHROME {
            if let Some(h) = ui.get_child_recursive(root, id) {
                ui.set_visible(h, !locked);
            }
        }
        // …and each floating window swaps its eight normal pieces for the eight `_Locked` ones.
        for block in crate::hud::floaty::FLOATY_CHROME {
            crate::hud::floaty::swap_chrome(ui, root, &block, locked);
        }
    }

    // -------------------------------------------------------------------------------------
    // The HUD.
    // -------------------------------------------------------------------------------------

    /// Every post-init set-up the sixteen windows run when the screen's root tree comes up.
    ///
    /// The client reaches these through element construction: each window is a registered element
    /// type and the factory runs its set-up as the tree initialises. This crate's element factory
    /// produces a `PlainElement` for all 84 game types on purpose (see `crate::game_element`), so
    /// the screen that owns the subtree does the binding — and that is this function.
    ///
    /// **The part that is load-bearing for what a player sees is the three child set-ups.**
    /// `PanelStack`, `EnvironmentPanelStack` and `CombatPanelStack` each end their child set-up by
    /// hiding every page they just registered, and the set-up then hides the stack itself because
    /// nothing is current yet. **Not one of those twenty-three pages carries `0x3B`
    /// (`UICore_Element_hide`) at all**, so they come up visible whichever way that attribute is
    /// read and a rebuild that skips the loops draws the inventory panel, the options panel, the
    /// admin panel and twenty more on top of each other over the world. [verified: these loops are
    /// the client's own code, not a compensation for the polarity]
    pub fn post_init(&mut self, ui: &mut UiSystem) {
        let Some(root) = self.root() else { return };

        // The window id — attribute `0x1000007E` — off each of the sixteen windows.
        self.window_ids.clear();
        for w in GAMEPLAY_WINDOWS {
            let Some(h) = ui.get_child_recursive(root, w.element) else {
                continue;
            };
            let id = attr_enum(ui, h, attr::WINDOW_ID)
                .or_else(|| attr_int(ui, h, attr::WINDOW_ID).map(|v| u32::try_from(v).unwrap_or(0)))
                .unwrap_or(0);
            self.window_ids.push((w.element, id));
        }

        // Examination-panel initialization and the three sub-UI
        // constructors' child lookups, off the screen root because `<EXAM>` is one of
        // the sixteen movable windows rather than a page.
        self.examination.post_init(ui, root);
        // `<BOOK>` is a page of `<PANS>`, so the
        // lookup is still off the screen root -- the child lookup is recursive and the client's
        // own set-up runs on the page element itself.
        self.book.post_init(ui, root);
        // The barber panel's child lookups; the notice makes this modal visible later.
        self.barber.post_init(ui, root);
        // The panel stack's child set-up, then the stack's own visibility.
        if let Some(h) = ui.get_child_recursive(root, window::PANEL_STACK) {
            self.panels
                .post_init(ui, h, &crate::panels::catalogue::PANEL_PAGES);
        }
        // The environment panel's child set-up only; the panel's own visibility is not touched
        // there.
        if let Some(h) = ui.get_child_recursive(root, window::ENV_PANEL) {
            self.env_panel
                .setup_children_with(ui, h, &crate::panels::catalogue::ENV_PANEL_PAGES);
            self.env_panel.root = Some(h);
        }
        // Its two pages, `0x1000005C` and `0x10000061`.
        //
        // **Why this starts at `<COMB>`.** The class is not "found as child `0x10000055` of
        // `CombatWindow`": `<COMB>` *is* a `CombatPanelStack`, because `FloatingCombatStack`
        // derives from it, and `CombatWindow` (`0x1000005C`) is one of its two **pages**. The
        // `0x10000055` is an element **id** that collides with this element **type**, and it is the
        // *ViewCombatTarget* option checkbox.
        if let Some(h) = ui.get_child_recursive(root, window::COMBAT_PANEL) {
            self.combat_panel.setup_children_with(
                ui,
                h,
                &crate::panels::catalogue::COMBAT_PANEL_PAGES,
            );
            // **How `<COMB>` becomes visible.** `CombatPanelStack`'s set-panel-visibility notice
            // shows and hides the stack window itself, exactly as `PanelStack` does for `<PANS>`;
            // `PanelStack` already reproduces that and it is gated on [`PanelStack::root`], which
            // must be set for this stack. Without it the combat cluster could be shown and would
            // still be inside a hidden window — the same shape as the indicator lamps and `<PANS>`.
            //
            // Assigned rather than taken from `post_init`, because
            // the combat-panel window's post-init is the base set-up plus the child set-up
            // and does **not** carry the generic panel post-init's trailing "visible only when a
            // page is current". `<COMB>` starts hidden because
            // the layout hides it, and `HUD_START_VISIBILITY` restates that.
            self.combat_panel.root = Some(h);
            // The combat panel's post-init: look up the recklessness field `0x100005EF` and hide
            // it immediately after. Without the hide, the combat-mode notice handler's **missile**
            // arm — which does not touch the field — would leave a recklessness meter drawn in a
            // bow stance, because the shipped layout carries `0x100005EF` visible.
            self.combat_ui_page =
                ui.get_child_recursive(h, crate::hud::combat_notice::COMBAT_UI_PAGE);
            self.spellcasting_page =
                ui.get_child_recursive(h, crate::hud::combat_notice::SPELLCASTING_PAGE);
            self.combat_recklessness_field = self.combat_ui_page.and_then(|p| {
                ui.get_child_recursive(p, crate::hud::combat_notice::RECKLESSNESS_FIELD)
            });
            if let Some(r) = self.combat_recklessness_field {
                ui.set_visible(r, false);
            }
            self.last_combat_ui_inputs = None;
        }

        // The seven panel buttons and the nine named children.
        if let Some(h) = ui.get_child_recursive(root, window::TOOLBAR) {
            self.toolbar.setup_buttons(ui, h);
            self.era_features = None;
            if let Some(spec) = crate::panels::catalogue::spec("Toolbar") {
                self.toolbar_children = bind_children(ui, h, spec.children);
            }
            // The toolbar's shortcut-array initialisation, which its set-up calls immediately
            // after the panel-button loop. **This is what builds the quickbar's tiles**: each of
            // the eighteen `ItemListWidget`s runs the item-list initialisation, and the numbered
            // empty plate is that slot's own `ItemSlot` root in state `0x1000001C`.
            //
            // **The stack-size entry box's number-input filter, the one the set-up installs the
            // moment it has bound the box.**
            //
            // Find child `0x100001A3` beneath the selected-object field, cast it to text
            // type `0x0C`, and cache the result as the stack-size entry. Install the numeric
            // input filter only if that cast succeeds.
            //
            // Without it the splitter's quantity box takes letters, and
            // `parse_stack_quantity` — which is `wcstoul(text, NULL, 0)` and documents itself as
            // running "over this number-input filter's ASCII input domain" — would be handed a
            // domain nothing had narrowed. `0x0` typed into it is base-8 and `0x10` is sixteen.
            if let Some(b) = ui.get_child_recursive(h, crate::toolbar::splitter::ENTRY_BOX) {
                dereth_ui::text::set_input_filter(ui, b, dereth_ui::text::number_input_filter);
            }
            self.shortcuts.init_shortcut_array(ui, h);
            // **The set-up's tail, four hides in a row right after the shortcut array is built.**
            //
            // These four are the *initial* state of the selection strip, and they are not the
            // same act as the client's hides: that handler only runs on
            // an edge (the stored selection differs from the current one), and a client that starts
            // with nothing selected never takes that branch, because its member also starts at 0.
            // So without these four lines the strip's authored visibility stands: the shipped
            // layout brings the stack-size box `0x100001A3` and the slider `0x100001A4` up
            // **visible**, and an empty selection would draw a split slider and edit box over
            // nothing. (The two meters ship hidden, so those two calls are no-ops on
            // this data; they are transcribed because the client makes them and because a layout
            // change would otherwise silently produce the same defect for the meters.)
            for field in [
                "sel_object_health_meter",
                "sel_object_mana_meter",
                "stack_size_entry_box",
                "stack_size_slider",
            ] {
                if let Some(c) = self.toolbar_children.get(field) {
                    ui.set_visible(c, false);
                }
            }
        }

        // The config panel's post-init — bind the option list box `0x10000200` and run
        // its option build, which is the 27 rows. The page is a child of
        // the panel stack, so it is reached from the screen root by id like every other window.
        if let Some(h) = ui.get_child_recursive(root, crate::options::config::CONFIG_PAGE_ELEMENT) {
            if let Some(p) = crate::options::page::config_post_init(ui, h) {
                self.config_page = p;
            }
        }

        // The character settings panel's post-init — bind `0x100001FA` and run its option
        // build, which is six headers, six separators and 50 player-option toggles.
        // The check-box *values* come from the player-option read through
        // [`crate::view::GameView::player_option`], which may have no producer at set-up: the
        // rows are built with whatever the view answers, `values_seen` records the denominator,
        // and [`Self::update_character_options`] re-reads them once one exists.
        //
        // The lookup is [`crate::options::character::find_page`] and **not**
        // `get_child_recursive`: the shipped tree carries the id `0x10000211` twice and a
        // depth-first walk reaches `KeyboardPanel`'s tab page first.
        if let Some(h) = crate::options::character::find_page(ui, root) {
            let view = crate::view::EmptyGameView;
            if let Some(p) = crate::options::character::character_settings_post_init(ui, h, &view) {
                self.character_options = p;
            }
        }

        // The chat options panel's post-init — bind `0x1000050D` and run its option build,
        // which is six headers, six separators, two opacity sliders and five
        // wide bit-field check-box options holding 64 check boxes between them.
        //
        // Looked up by id and **type**, like the Character Options page: `0x1000050C` is unique in
        // the shipped tree today, but the three option pages are siblings under one panel and a
        // page bound to the wrong element finds no option box and draws nothing — which is
        // indistinguishable from "the layout changed". The values come from the `PlayerModule`
        // through `GameView::gameplay_option_float` / `chat_window_filter`, which nothing answers
        // at set-up time; `values_seen` records the denominator and
        // [`Self::update_chat_options`] re-reads them once the host has a module.
        if let Some(h) = crate::options::chat::find_page(ui, root) {
            let view = crate::view::EmptyGameView;
            if let Some(p) = crate::options::chat::chat_options_post_init(ui, h, &view) {
                self.chat_options = p;
            }
        }

        // Bind the six tab pages' list boxes and the two keymap buttons. Option-row
        // initialization is *not* run here: every row it adds
        // reads `find_keys_for_action` on the merged master map, which this crate does not
        // own. [`Self::key_bindings_init_options`] is that half, and the host calls it once it has
        // built its `InputManager` (input-map startup runs before the UI, so
        // in a live client it is always available by the time this screen exists).
        if let Some(h) = ui.get_child_recursive(root, crate::options::keybinding::KEYBOARD_UI) {
            self.key_bindings = crate::options::keybinding::KeyBindingPage::bind(ui, h);
        }

        // `GameplayOptionsPanel`, the Game / Support page. It has no post-init
        // set-up (the class has six functions and that is all of them), so there is nothing to bind
        // but the page element itself, which is what attributes a click to this page rather than to
        // an id that happens to repeat. See [`crate::options::gameplay`].
        self.gameplay_options = crate::options::gameplay::GameplayOptionsPage::bind(ui, root);

        // The vitals panel's post-init, once per vitals window.
        if let Some(spec) = crate::panels::catalogue::spec("VitalsPanel") {
            if let Some(h) = ui.get_child_recursive(root, window::STACKED_VITALS) {
                self.stacked_vitals = bind_children(ui, h, spec.children);
            }
            if let Some(h) = ui.get_child_recursive(root, window::SIDE_VITALS) {
                self.side_vitals = bind_children(ui, h, spec.children);
            }
        }
        // **Why the vitals press arm is reachable by a player at all.**
        //
        // Registering for element messages is not only a route: registering any of the four
        // ids in [`dereth_ui::msg::element::id::AUTO_MOUSE_VISIBLE`] against an element id is one of
        // the two things in this client that ever makes an element mouse-visible, and
        // the mouse-over hit test returns nothing else. Neither
        // vitals window carries a tooltip or a context menu, and `Meter` does not make
        // its should-be-mouse-visible query answer true the way the five grab-with-the-mouse types
        // do — so without this line **every pixel of the bar's face is transparent to the
        // pointer**. A press at the centre of `<VITS>` hit-tests to the spew box `0x10000046`
        // behind it: of the window's 9,280 face pixels only its own drag and resize bars (2,080)
        // answer anything under `<VITS>`, and the press arm is unreachable.
        //
        // Registering by **element id** is also the client's own route to this handler:
        // `VitalsPanel` installs its element-message handler on the vitals element itself, and the
        // by-id table is consulted before the bubble (`broadcast_element_message_at` step (a)),
        // which is the same route `logout::BUTTON_YES` above takes.
        for w in [window::STACKED_VITALS, window::SIDE_VITALS] {
            ui.register_for_element_message(w, dereth_ui::msg::element::id::MOUSE_PRESS, ME);
        }

        // The chat interface's post-init, once per chat window. The scrollback is bound from
        // the window's own subtree because all five windows carry the same child ids.
        use crate::chat::interface::window as chatwin;
        self.chat.clear();
        self.chat_windows.clear();
        let f = crate::hud::floaty::FLOATY_CHAT_WINDOWS;
        let chat_windows = [
            (window::MAIN_CHAT, chatwin::MAIN),
            (f[0], chatwin::FLOATY_1),
            (f[1], chatwin::FLOATY_2),
            (f[2], chatwin::FLOATY_3),
            (f[3], chatwin::FLOATY_4),
        ];
        for (element, window_id) in chat_windows {
            self.chat.push(ChatInterface::new(window_id));
            // The three children are bound from the
            // window's own subtree because all five windows carry the same child ids, and the log
            // is handed its 34-entry colour table here.
            let w = ui
                .get_child_recursive(root, element)
                .map_or_else(crate::chat::window::ChatWindow::default, |h| {
                    crate::chat::window::ChatWindow::post_init(ui, h)
                });
            // The client's two arms, run once the window's properties are merged: `0x10000080` ->
            // the idle opacity, `0x10000081` -> the active opacity, then the idle value is pushed
            // through. Without them every chat window draws at a flat `alpha_blend_mod`.
            let last = self.chat.len() - 1;
            w.read_opacity_attributes(ui, &mut self.chat[last]);
            self.chat_windows.push(w);
        }

        // The client's own tail, which is the startup
        // half of the chat font:
        //
        // Read the global font size and face, pass face then size to the font-settings
        // notice handler, then run the player-module update.
        //
        // Run here rather than inside the loop above because the two statics are the *same* for
        // every window — they belong to the client rather than the interface — and this crate reads them
        // out of [`crate::options::store`].
        self.chat_recv_notice_font_settings_changed(ui);

        // The client's own tail — the chat-target caption
        // element, the talk-focus menu and selecting talk focus 1 — and one `FloatingChat`
        // per floaty window, which is what gives the close button and the four placement
        // write-backs somewhere to live.
        if let Some(h) = ui.get_child_recursive(root, window::MAIN_CHAT) {
            self.main_chat = crate::chat::mainchat::MainChatPanel::post_init(ui, h);
        }
        self.floaty_chat.clear();
        for (element, window_id) in [
            (f[0], chatwin::FLOATY_1),
            (f[1], chatwin::FLOATY_2),
            (f[2], chatwin::FLOATY_3),
            (f[3], chatwin::FLOATY_4),
        ] {
            let w = ui
                .get_child_recursive(root, element)
                .map_or_else(crate::chat::floaty::FloatyChatUI::default, |h| {
                    crate::chat::floaty::FloatyChatUI::post_init(ui, h, window_id)
                });
            self.floaty_chat.push(w);
        }

        self.indicators_post_init(ui, root);
        self.radar_post_init(ui, root);
        // Beside its sibling. The map panel is one of
        // `PanelStack`'s sixteen pages and comes up hidden, which does not matter: the set-up runs
        // on every panel at construction in retail too, and `Update` writes to hidden elements.
        self.map_post_init(ui, root);

        // The inventory panel's post-init and the three sub-panels under it. **This is what
        // creates the item slots**: every `ItemListWidget` runs the item-list initialisation,
        // whose internal item creation builds each slot's `UiItemWidget` from the
        // `ItemSlot` layout. Without it the backpack is an empty frame however full the pack is
        // (and the quickbar likewise).
        //
        // It runs **after** the two indicator passes and **before** `apply_start_visibility`,
        // because the item-list initialisation returns immediately on an invisible list and the
        // page is still up at this point.
        if let Some(h) = ui.get_child_recursive(root, window::INVENTORY_PAGE) {
            self.inventory.post_init(ui, h);
        }

        self.apply_start_visibility(ui);
        self.let_the_world_through(ui);
        self.world_view_post_init(ui);
    }

    /// The inventory panel's set-display-inventory notice and
    /// the paper doll's inventory remake, driven from the world once per frame.
    ///
    /// Returns true on a frame that actually changed a slot; the guard is the panel's own
    /// snapshot, because the client's version is notice-driven.
    pub fn update_inventory(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        self.inventory.update(ui, view)
    }

    /// The toolbar's update from the player description plus its set-combat-mode notice's
    /// `set_shortcut_num` pass — the quickbar, driven from `PlayerModule`.
    ///
    /// Guards on its own snapshot, like the inventory: the client's version is notice-driven
    /// (player description received and set combat mode), not per-frame.
    pub fn update_shortcuts(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        self.shortcuts.update(ui, view)
    }

    /// **The cooldown heartbeat**, which a rebuild has to drive because it has no
    /// global-message broadcast.
    ///
    /// It is here rather than inside [`Self::update_inventory`] / [`Self::update_shortcuts`]
    /// because both of those guard on a snapshot of *what the pack holds*, and a wedge ticking
    /// down changes nothing about the pack. In the client the item element's global-message
    /// handler runs the heartbeat step once a second off global message 3, and that is the only
    /// thing that moves it.
    ///
    /// The other half of the item slots' per-frame work, the selection ring, is **not** here: it is
    /// an edge, and this screen already has the client's one remembered selection. See
    /// [`Self::update_toolbar_selection`].
    ///
    /// Returns how many slots re-ran their cooldown display.
    pub fn do_item_heartbeat(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> usize {
        let now = ui.now.0;
        let mut beats = {
            let remaining = |id: u32| view.cooldown_remaining(id, now);
            self.inventory
                .lists_mut()
                .map(|w| w.do_heartbeat(ui, now, &remaining))
                .sum::<usize>()
        };
        beats += self.shortcuts.do_heartbeat(ui, view);
        beats
    }

    ///  on global message 1 — one input action, and
    /// what the toolbar does with it.
    ///
    /// The focus gate is the client's: while the stack-size entry box has focus every action is
    /// swallowed, and action `0x27` resets the box instead of firing slot 0.
    ///
    /// The make-shortcut key reads the selection the host supplied through
    /// [`Self::set_interaction_context`] (the toolbar's own read-out otherwise) and asks for a
    /// shortcut to it in the first empty slot; with nothing selected it does nothing.
    #[must_use]
    pub fn on_input_action(&self, ui: &UiSystem, action: u32) -> Option<UiRequest> {
        use crate::toolbar::shortcuts::ShortcutAction;
        // The focus element is the client's own test, and the stack-size entry box is
        // `0x100001A3`, bound at set-up.
        let focused = match (
            self.toolbar_children.get("stack_size_entry_box"),
            ui.focus_element(),
        ) {
            (Some(b), Some(f)) => f == b,
            _ => false,
        };
        match crate::toolbar::shortcuts::dispatch(action, focused)? {
            ShortcutAction::Use { slot, primary } => {
                self.shortcuts
                    .use_shortcut(slot, primary, self.interaction_target_mode)
            }
            // With nothing selected the key does nothing.
            ShortcutAction::CreateToSelected => self
                .interaction_selection
                .unwrap_or(self.last_selection.flatten())
                .filter(|id| id.0 != 0)
                .map(UiRequest::CreateShortcut),
        }
    }

    /// Element message `0x15` reached the
    /// drop **target**, with the drag's owner in `p2`.
    ///
    /// # The drag names itself.
    ///
    /// The item is not resolved by asking [`crate::panels::inventory::InventoryPanels::locate`]
    /// which of the lists owned the element the drag started from; that fails for drags begun in a
    /// chest, the shortcut bar or a vendor/trade list. **Retail never asks.**
    ///
    /// The element manager's drag-and-drop stop builds a drag-drop record — the element (the
    /// proxy), the owner, the catcher and a success flag — and hands it to "catch dropped item" on
    /// the catcher and "drag and drop complete" on the owner, each of which is one element-message
    /// broadcast. The drop-release handler then returns unless the catcher is this element or
    /// inside it, reads the proxy's (item, spell, flags), and returns if the item id is 0 or
    /// `flags & 0x0E` is set (the alias mask is 14).
    ///
    /// The record's owner is **never read** by this function. The identity is on the proxy, which
    /// is why a drag begun in a chest, a shortcut tile or a vendor list needs no registry entry
    /// anywhere: whatever picked the icon up wrote `0x1000000F`…`0x10000014` onto it
    /// ([`crate::items::widget::ItemSlot::prepare_drag_icon`]) and every drop handler in the
    /// client reads them back with [`crate::items::widget::inq_drop_icon_info`].
    ///
    /// `owner` here is the element `dereth_ui`'s `stop_drag_and_drop` puts in `p2` — the source
    /// slot's drag icon, which is the element the properties were written onto and the one
    /// the client copies them to make the proxy, so it answers
    /// [`crate::items::widget::inq_drop_icon_info`] identically. The `locate` walk survives only as
    /// the fallback for a
    /// **direct call**, where a test hands in a bare slot handle that carries no drag properties
    /// at all.
    ///
    /// Clearing the item's waiting state (0) — the client's tail, and the tile's own `0x15` arm.
    ///
    /// Both halves, because the client's clear is both halves: the object's flag crosses the seam
    /// as [`UiRequest::ClearItemWaiting`] and reaches every panel through
    /// `GameView::slot_decoration`, and the sweep over this screen's own widgets is the same clear
    /// a frame early, so the icon comes back on the frame the player let go rather than on the next
    /// one. The eighteen shortcut lists are in the sweep for the same reason they are in the
    /// drag-hint one: `UiItemWidget` is the class, and a quickbar tile is one.
    fn release_item_ghost(&mut self, ui: &mut UiSystem, item: ObjectId) {
        for w in self
            .inventory
            .lists_mut()
            .chain(self.shortcuts.slots.iter_mut())
        {
            w.clear_waiting(ui, item);
        }
        ui.requests.emit(UiRequest::ClearItemWaiting(item));
    }

    /// Returns the request it emitted, for a test to assert on.
    pub fn handle_drop_release(
        &mut self,
        ui: &mut UiSystem,
        target: ElemHandle,
        owner: ElemHandle,
    ) -> Option<UiRequest> {
        // Read the dragged element's (item, spell, flags), then return when there is no item.
        let info = crate::items::widget::inq_drop_icon_info(ui, owner);
        let item = match info.item {
            Some(id) => id,
            // The direct-call fallback described above. It is deliberately **not** reached by a
            // real gesture: a real drag always has a prepared icon, because
            // the drag start is never reached when
            // preparing the drag icon fails.
            None => self.locate_drag_owner(ui, owner).and_then(|(_, _, i)| i)?,
        };
        // The toolbar's drop handling is asked **first**, because the toolbar's
        // eighteen shortcut lists are `ItemListWidget`s too and `InventoryPanels::locate` would
        // not find them. The client's own order is the same: `Toolbar`'s handler runs the
        // sweep over its shortcut slots before anything else looks at the drop.
        //
        // It is handed the flags as well. The toolbar does not *gate* on `flags & 0x0E` the way the
        // other handlers do — it **forks** on it, and the second arm is the whole of the shortcut
        // bar's "relocate": see [`crate::toolbar::shortcuts::ShortcutBar::handle_drop_release`].
        if let Some(r) = self
            .shortcuts
            .handle_drop_release(ui, target, item, info.flags)
        {
            // **Correction: no ghost here.** The waiting-state set (1) is what a request that the
            // *server* must answer sets, and the toolbar's drop handling never calls it: a shortcut
            // is the player module's add-shortcut, applied locally and flushed with the options
            // blob 480 s later. Ghosting it would leave the dragged icon greyed for ever, because
            // the reply that would clear it does not exist.
            //
            // **And the ghost the *pick-up* put on has to come off here, which is the item list's
            // own drop handling's second clear route.** The shortcut list under the pointer runs
            // the class's own `0x15` arm like every other list, and its second test is the "is
            // alias list" test — the vendor-item, salvage or shortcut list flag, exactly the three
            // lists the item list's begin-drag refuses to ghost a source in — which jumps straight
            // to the client's waiting-state clear (0). Without it dragging a **backpack** out of
            // the side-pack strip onto the quickbar would leave it greyed for the session: the
            // shortcut is local, nothing is asked of the shard, and no reply is ever coming.
            self.release_item_ghost(ui, item);
            ui.requests.emit(r.clone());
            return Some(r);
        }
        // Return when `flags & 0x0E` is set — the client, and **the alias mask is 14, not one
        // bit**. Every handler below this line repeats the same test in the client:
        // The smart-box wrapper element's drop handling,
        // the secure-trade panel's, the vendor panel's,
        // the paper doll panel's and
        // the item list's own — so one gate here is **five**
        // transcriptions, not a shortcut past them. Each of the five pairs it with the no-item
        // return, in the same order.
        //
        // **This is what stops a shortcut dragged off the bar and released over the pack from being
        // sent to the shard as a container move for an item that never left the pack.**
        // The removal has already happened at pick-up;
        // the release is supposed to do nothing at all.
        if !info.is_inventory_move() {
            return None;
        }
        // The client tests the catcher's element id for the inventory button **before** walking the
        // shortcut lists. The shortcut walk is already above because it needs the drag flags; this
        // exact-id arm is otherwise independent and must precede `InventoryPanels::drop_target`,
        // where toolbar chrome correctly resolves to no inventory list. A successful move keeps the
        // pick-up ghost until the shard answers; the host applies the client's refusal clear after
        // the object-table gates have run.
        if ui.node(target).map(dereth_ui::ElementNode::element_id)
            == Some(crate::toolbar::INVENTORY_BUTTON)
        {
            let r = self.accept_drag_object(item, crate::view::DropTarget::BackpackButton)?;
            ui.requests.emit(r.clone());
            return Some(r);
        }
        // The element-message handler's `0x15` arm →
        // its drop handling, which is "drop it in the world": the drop's *target* is
        // whatever the 3D pick then finds, so `DropTarget::World` arms a pick rather than naming a
        // destination. `locate(target)` returns `None` for the viewport, so without this arm the
        // whole request would be dropped, `DropTarget::World` would be a variant **nothing emits**,
        // and `interaction::place_in_3d` — the client's full container/ground/split fork — would be
        // unreachable.
        //
        // It is tested **before** the inventory's own lists because `<SBOX>` is not one of them and
        // `locate` would answer `None` for it either way; the order that matters is the toolbar's,
        // above.
        if is_in_world_view(ui, target) {
            let r = self.accept_drag_object(item, crate::view::DropTarget::World)?;
            // The waiting-state set (1) — the same ghost as any other move. The item does not
            // leave the pack until `Item_ServerSaysMoveItem` says so.
            self.inventory.ghost_item(ui, item);
            ui.requests.emit(r.clone());
            return Some(r);
        }
        // The secure trade panel's drop handling: when the drop target is inside
        // its own items list, read the dropped icon's info and accept the item if there is one and
        // it is not an alias (flags `0x0E`).
        //
        // Tested here, before the inventory's own lists, for the same reason the smart box is:
        // `0x10000088` is not one of them and `InventoryPanels::drop_target` would answer `None`,
        // so without this test a drop on the trade table would be discarded whole. The
        // **partner's** list (`0x10000081`) is deliberately not a target -- initialization
        // registers the drag handler on the self list alone.
        //
        // No ghost: the client's whole-stack arm calls
        // the waiting-state clear (**0**) before adding the item, which is the opposite of the pack
        // move's
        // waiting-state set (1) -- the row goes into the window at once and the server's `0x0200`
        // confirms it rather than authorising it.
        if is_under_element(ui, target, crate::panels::trade::SELF_LIST) {
            // The secure trade panel's drag-accept test calls the waiting-state clear (0) before
            // adding the item. The row goes into the window at once and the ghost the pick-up put
            // on comes off with it.
            self.release_item_ghost(ui, item);
            self.trade_drops
                .push((item, self.splitter.split_size, self.splitter.max_split_size));
            return None;
        }
        // The housing panel registers the same drag handler on
        // both payment lists, and its drag-accept reads this gesture's splitter words:
        // a whole stack enters the panel immediately, while a partial stack asks the inventory
        // holder to split beside its source. In neither arm does the released source stay ghosted.
        if is_under_element(ui, target, crate::panels::slumlord::BUY_LIST)
            || is_under_element(ui, target, crate::panels::slumlord::RENT_LIST)
        {
            self.release_item_ghost(ui, item);
            self.slumlord_drops.push((
                item,
                self.splitter.split_size,
                self.splitter.max_split_size,
            ));
            return None;
        }
        // The vendor's drop handling, which is the arm above with one element id
        // changed: the drop target is tested against the sell list instead.
        //
        // Tested here, before the pack's own lists, for the reason the trade table and the smart
        // box are: `0x100000CE` is not one of `InventoryPanels`' twenty-seven, so
        // `InventoryPanels::drop_target` answers `None` for it and without this test the whole drop
        // would be discarded. **That ownership split is the same one that stops a drop reaching an
        // external container** -- the vendor's three lists and `ExternalContainerPanel`'s three
        // both hang off `RemainingPanels` -- and it is answered here rather than by widening
        // `InventoryPanels`, so the container half stays a separate change.
        //
        // The stock list `0x100000BD` and the buy basket `0x100000C5` are deliberately **not**
        // targets: the set-up registers the drag handler on the sell list alone, exactly as secure
        // trade registers on the self list and not the partner's.
        //
        // No ghost, and no clear: the client's whole-stack arm calls the waiting-state clear
        // (**0**) before adding the item to the sell basket, the same polarity secure trade uses --
        // the row enters the basket at once and nothing has been asked of the shard yet.
        if is_under_element(ui, target, crate::panels::vendor::SELL_LIST) {
            // The vendor sell page's drag-accept test's waiting-state clear (0). The sell list is
            // also an alias list (the vendor-item list), so the client's alias-list route reaches
            // the same clear. The splitter this gesture used goes with the drop: the whole-stack
            // fork `accept_drag_object` makes is read from it.
            self.release_item_ghost(ui, item);
            self.vendor_sell_drops.push((
                item,
                self.splitter.split_size,
                self.splitter.max_split_size,
            ));
            return None;
        }
        // The same three lines again
        // with the salvage list (`0x10000074`) as the target test. Tested here, before the pack's
        // own lists, for the reason the two above are: `0x10000074` is not one of
        // `InventoryPanels`' twenty-seven, so `InventoryPanels::drop_target` answers `None` for it
        // and without this test the whole drop would be discarded: the window could not be filled
        // by a drag even when open.
        //
        // The client registers the drag handler on the salvage list and there is no second list, so
        // this panel has exactly one drop target.
        if is_under_element(ui, target, crate::panels::salvage::LIST) {
            // The salvage list is flagged as the salvage alias list, so the
            // alias-list test answers true and clears.
            self.release_item_ghost(ui, item);
            self.salvage_drops.push(item);
            return None;
        }
        // The Create Spell page's formula takes a carried component dragged onto it. Nothing
        // leaves the pack: the component is only named in the formula, so the pick-up's ghost
        // comes straight off.
        if is_under_element(ui, target, crate::panels::research::FORMULA) {
            self.release_item_ghost(ui, item);
            self.research_drops.push(item);
            return None;
        }
        // `InventoryPanels::drop_target` is the paper doll panel's drop handling's
        // location-from-element-id fork: a drop on one of the doll's twenty-four slots is a
        // `DropTarget::EquipSlot`, not a container move.
        // The client's is-container drop flag, read off the drag proxy exactly as the client reads
        // it — `owner` here is the source
        // slot's drag icon, which is the element
        // [`crate::items::widget::ItemSlot::prepare_drag_icon`] wrote `0x10000011` onto and which
        // the client copies the instance properties to make the proxy. It is
        // **not** a property of the list that was dropped on: it selects the container capacity and
        // container list over the item capacity and item list throughout the drag-accept test.
        let dragged_is_container = crate::items::widget::inq_drop_icon_info(ui, owner).is_container()
            // A direct call (a test, or an owner that is the slot rather than its drag icon)
            // carries no instance properties; ask the source slot the same question
            // `prepare_drag_icon` asked it.
            || self.inventory.slot_is_container(ui, owner);
        // **The client's first and third clear routes.** When the catcher is the
        // list itself rather than one of its tiles, or the list is an alias list, the ghost is
        // cleared; otherwise the drag is offered to the drag-accept test, and only a refusal
        // clears it (an accepted move keeps the ghost, because the shard owes an answer).
        //
        // Both of the `?`s below are "the drop resolved to nothing": an element this panel does
        // not map (the strip's own body, a window frame, a chrome element that happens to catch)
        // or a target `accept_drag_object` has no request for. Retail's answer to both is the
        // clear, and without it a **backpack** let go anywhere inside the inventory
        // window but not on a slot kept the ghost put on it —
        // for ever, because the wait has no timeout and no reply is coming.
        let Some(t) = self.inventory.drop_target(target) else {
            self.release_item_ghost(ui, item);
            return None;
        };
        let target = self
            .inventory
            .resolve_item_list_drop(t, dragged_is_container);
        let Some(r) = self.accept_drag_object(item, target) else {
            self.release_item_ghost(ui, item);
            return None;
        };
        // **The ghost is not applied here.**
        //
        // The item list element's drag-accept test reaches its one
        // waiting-state set (1) only *past* every decline, and
        // three of those declines are no-ops that never reach the shard at all: the item is found
        // in neither list; it is dropped back onto its own slot; or, with the whole stack in hand,
        // it is dropped on the slot just after its own.
        //
        // Each of those simply returns false — no request, no notice, and **no ghost**.
        // Those two tests already exist on the far side of this seam, in
        // `item_list_accept_drag` (`if idx == old` / `if idx == old + 1 &&
        // whole stack`), because they need the "place in items list" read and this crate may not
        // read the object table. Ghosting here would leave the refusal no way to undo the overlay:
        // nothing moves, so no `Item_ServerSaysMoveItem` ever arrives, and the wait has no timeout.
        // The item would stay ghosted for the rest of the session.
        //
        // **And the drop *clears* the ghost rather than setting one, which is the item list's
        // own drop-handling tail:** it offers the drag with `flags & 0xFFFFFF01`; accepted, it
        // leaves the ghost alone; refused, it takes the ghost off the item's object (if any).
        //
        // The ghost that is being taken off there is the one put
        // on when the icon was *picked up* — not one the drop invented. Ours is the
        // same: [`crate::items::widget::ItemListWidget::begin_drag`] ghosts the source slot.
        //
        // The seam turns retail's synchronous `if (!accepted)` into "clear it, and let the model
        // put it back": `dereth_client_model` writes `waiting = true` on each of the eight paths that
        // actually emit a `Request`, so an accepted move is re-ghosted by
        // [`crate::panels::inventory::InventoryPanels::update`]'s ghost pass on the next pass over
        // a snapshot that has genuinely changed, and a declined one is not — its snapshot is
        // identical to the one before the drag, which is precisely why nothing could ever clear a
        // hand-applied ghost. A drop the *shard* later refuses is unaffected:
        // `Item_ServerSaysAttemptFailed` clears the same flag and the same pass answers it.
        //
        // **No local `clear_waiting` here: the item list's drop handling does not clear anything on
        // this leg.**
        // An **accepted** drag leaves the ghost exactly where
        // the item list's begin-drag put it, and only the refusal branch reaches
        // the waiting-state clear (0). Both halves of that exist on
        // the far side of this seam: the pick-up writes the object's flag
        // ([`UiRequest::SetItemWaiting`], the client), and `Interaction`'s `ItemListSlot` and
        // `EquipSlot` arms already answer a model refusal with `set_waiting_state(item, false)`.
        //
        // Clearing it here would be a **desync**, not a deviation that merely lost a frame: the
        // widget would say "not waiting" while the object said "waiting", and
        // [`crate::panels::inventory::InventoryPanels::update`] only re-applies ghosts on a pass
        // whose snapshot changed -- and the snapshot cannot change, because the object's flag was
        // already `true` before the drop and is still `true` after it.
        ui.requests.emit(r.clone());
        Some(r)
    }

    /// The drop-release ancestor check, applied to
    /// the drag's **owner** instead of to the target.
    ///
    /// A drag started by [`crate::items::widget::ItemListWidget::begin_drag`] owns its drag icon —
    /// a child of the slot, not the slot — because that is the element the drag start is handed.
    /// The client never asks the owner what it holds (it reads the item id back off the proxy; see
    /// [`crate::items::widget::inq_drop_icon_info`]); this screen's map is keyed by slot, so the
    /// owner is walked up to the first element the inventory recognises. One step, for a drag icon;
    /// zero, for the direct call a test makes.
    fn locate_drag_owner(
        &self,
        ui: &UiSystem,
        owner: ElemHandle,
    ) -> Option<(ElementId, u32, Option<ObjectId>)> {
        let mut cur = Some(owner);
        while let Some(h) = cur {
            if let Some(r) = self.inventory.locate(h) {
                return Some(r);
            }
            cur = ui.parent(h);
        }
        None
    }

    /// The client's `0x21` arm, over this screen's item
    /// lists.
    ///
    /// See [`crate::items::widget::begin_drag_from_rejected`] for why a rejected drag is how an
    /// item slot is picked up at all.
    ///
    /// # The eighteen shortcut lists are offered too.
    ///
    /// The client's parent check does not consult a registry: it asks the pressed slot's own
    /// parent whether it is an `ItemListWidget` (type `0x10000031`), and a shortcut tile's
    /// parent **is** one — the toolbar's shortcut slot `n`, built during shortcut-array
    /// initialization. Offering only the inventory's four would make `0x21` on a tile start no
    /// drag at all, which in turn would make the client's is-shortcut drop arm unreachable.
    pub fn begin_item_drag(
        &mut self,
        ui: &mut UiSystem,
        source: ElemHandle,
        x: i32,
        y: i32,
    ) -> Option<crate::items::widget::DragStart> {
        let mut lists: Vec<&mut crate::items::widget::ItemListWidget> = self
            .inventory
            .item_list
            .iter_mut()
            .chain(self.inventory.container_list.iter_mut())
            .chain(self.inventory.top_container.iter_mut())
            .chain(self.inventory.doll.iter_mut().map(|(_, w)| w))
            .chain(self.shortcuts.slots.iter_mut())
            .collect();
        let started = crate::items::widget::begin_drag_from_rejected(ui, &mut lists, source, x, y);
        // Retail parity, observed in play: T, type a quantity, then click-drag the already-selected
        // stack commits that quantity without Enter. The exact native focus edge is unresolved: the
        // item element itself takes a plain mouse-down, while its bubbled `0x1C` reaches the item
        // list's listener. Preserve the observable at the narrow point where a real item drag has
        // actually started, reusing the focus-loss parser/clamp above rather than teaching every
        // item element to take focus. A press that never crosses the threshold does not reach this
        // path.
        if started.is_some() {
            if let Some(entry) = self.toolbar_children.get("stack_size_entry_box") {
                if ui.focus_element() == Some(entry) {
                    self.commit_stack_box(ui, entry);
                    ui.relinquish_focus(entry);
                }
            }
        }
        // **Beginning an item-list drag writes the ghost onto the OBJECT, and that is the only
        // reason a refill cannot lose it.** See [`UiRequest::SetItemWaiting`] for the three
        // functions that settle it. The widget's own `set_waiting` inside `begin_drag` is the
        // client's element half (the overlay); this is its object-model half, which the crate
        // boundary will not let this crate perform directly.
        if let Some(item) = started.as_ref().filter(|d| d.ghosted).and_then(|d| d.item) {
            ui.requests.emit(UiRequest::SetItemWaiting(item));
        }
        // Remember which element carries the drag's drop-icon-info payload; the
        // client reads it off the drag element, which `dereth_ui` keeps private.
        self.drag_payload = started.as_ref().map(|d| d.drag_icon);
        // The item list's begin-drag tail — the item-list-begin-drag notice with the list and the
        // item's slot number, sent for **every** list, not only the toolbar's.
        //
        // The toolbar's item-list-begin-drag notice is the only listener that does
        // anything with it, and what it does is *"dragging items FROM the shortcut bar to remove
        // them"*: the shortcut comes off the bar at **pick-up**, and the drop has no
        // removal half at all. See
        // [`crate::toolbar::shortcuts::ShortcutBar::on_item_list_begin_drag`].
        if let Some(d) = started.as_ref() {
            if let Some(item) = self.shortcuts.on_item_list_begin_drag(d.list) {
                ui.requests.emit(UiRequest::RemoveShortcut(item));
            }
        }
        started
    }

    /// The client's **`0x3E` arm** — the one thing the
    /// retail client changes on screen while an inventory item is being dragged. This is the
    /// consumer arm; it is driven by the message, not by a per-frame poll.
    ///
    /// # The producer
    ///
    /// The element manager's mouse-over switch, after the ordinary mouse-over bookkeeping: while
    /// a drag is under way, the element under the pointer is asked for its drag-and-drop catcher.
    /// When that catcher differs from the last one, the old one (if any) gets `0x3E` with
    /// `p1 = 0` (leaving), the new one (if any) gets `0x3E` with `p1 = 1` (entering), and the new
    /// one is remembered.
    ///
    /// It lives in `dereth_ui::focus`'s `mouse_move`, so the remembered catcher
    /// is `DragState::last_drag_cursor_over` and **this screen keeps no copy of it**: the message
    /// names the element, which is the whole point of not polling.
    ///
    /// # The arm
    ///
    /// On `0x3E`: leaving (`p1 == 0`), or entering with no drag element, puts the slot's
    /// drag-accept icon (if it has one and is not already there) in state `0x1000003f`.
    /// Otherwise, if the slot's parent is an item list (type `0x10000031`), the list's drag-over
    /// handling is asked with the drag element and this slot.
    ///
    /// Three things it is worth being exact about, because each is a place a poll would differ:
    ///
    /// * **The leave arm acts on the one slot the message names**, not on every slot of the list.
    ///   A poll has no message and therefore has to sweep.
    /// * **The enter arm with no drag element clears rather than falls through** — it is the
    ///   same drag-accept state `0x1000003F` as the leave arm.
    /// * The item list's drag-over test is asked on the slot's **own list**, so the decision belongs to the
    ///   list and the drawing to the slot. Here the list is whichever of this panel's lists holds
    ///   the slot; `slot_of` finds it by handle, and deliberately not
    ///   [`crate::panels::inventory::InventoryPanels::locate`], which answers a bare *list* handle
    ///   with slot 0 and so cannot tell a list from its own first slot.
    ///
    /// # Where the empty-slot count comes from
    ///
    /// The item list's drag-over test's container branch looks up the weenie object for the item under the
    /// pointer and then asks its empty-slot count. An element-message handler on this
    /// screen has no [`GameView`] — the seam is `&dyn GameView` and the `Screen` trait's
    /// `on_element_message` does not carry one — so the two values that answer it, the item
    /// capacity and the number of items held, are kept on the slot by
    /// [`crate::items::widget::ItemSlot::num_empty_item_slots`], written by the item element's
    /// update's own decoration pass. That is the same pair the capacity bar reads off the same
    /// weenie in the same function, so the cache goes stale exactly when the capacity bar
    /// does and never independently of it.
    ///
    /// Returns the drag-accept state now showing, if any. `None` is *"one of the drag-over test's
    /// early-outs fired"*, which is not the same as choosing `drag_accept_state::NONE`.
    pub fn on_drag_cursor_over(
        &mut self,
        ui: &mut UiSystem,
        over: ElemHandle,
        entering: bool,
    ) -> Option<dereth_ui::StateId> {
        use crate::items::widget::{drag_accept_state, inq_drop_icon_info};

        // "a drag has started and there is a drag element" — `is_dragging` is "there is a drag
        // element", which is the stricter of the two and is the one the client tests.
        //
        // **Read the drag element itself.** The client reads the element manager's drag element and
        // nothing else. Reading a copy of the *owner* that `Self::begin_item_drag` recorded goes
        // wrong the moment a drag begins somewhere this screen does not own — a chest slot, a
        // vendor list — because `begin_item_drag` then answers `None` and the hover hint silently
        // stops appearing over the pack. `drag_payload` is still written, as a fallback and for the
        // tests that pin the producer.
        let payload = ui
            .drag_state()
            .element
            .or(self.drag_payload)
            .filter(|_| ui.is_dragging());
        let info = payload.map(|p| inq_drop_icon_info(ui, p));
        // **The client's first statement, the list's own drag handler.** Two of this screen's list
        // families register one, and neither ever falls through to the default below:
        //
        // * the eighteen shortcut lists, answered
        //   here by [`crate::toolbar::shortcuts::ShortcutBar::on_drag_cursor_over`];
        // * the twenty-four doll lists, whose handler asks about the item's valid locations and so
        //   needs a `GameView` this arm does not have. The message is already in
        //   [`Self::panel_messages`], and the host hands it to [`Self::on_paper_doll_drag_over`] in
        //   the same frame. Stepping aside here is what stops the default's
        //   "not a container list -> ACCEPT" from painting green on a slot the doll is about to
        //   refuse.
        if let Some(s) = self
            .shortcuts
            .on_drag_cursor_over(ui, over, info.filter(|_| entering))
        {
            return s;
        }
        if self.inventory.doll_slot_of(over).is_some() {
            return None;
        }
        for w in self.inventory.lists_mut() {
            let Some(slot) = w.slot_of(over) else {
                continue;
            };
            let Some(info) = info.filter(|_| entering) else {
                // Both of the arm's clearing routes: `p1 == 0`, and `p1 != 0` with no drag
                // element.
                w.slots[slot].set_drag_accept_state(ui, drag_accept_state::NONE);
                return Some(drag_accept_state::NONE);
            };
            // The empty-slot count of the item's weenie object, read off the slot the
            // pointer is over — the only object the drag-over test asks about.
            let under = w.slots.get(slot).and_then(|s| s.item);
            let free = w
                .slots
                .get(slot)
                .map(crate::items::widget::ItemSlot::num_empty_item_slots);
            return w.drag_over(ui, slot, info, &|id| {
                if Some(id) == under {
                    free
                } else {
                    None
                }
            });
        }
        None
    }

    /// The paper doll's item-list drag-over, delivered by the host together with the
    /// view the arm needs. See
    /// [`crate::panels::inventory::InventoryPanels::on_paper_doll_drag_over`]. This is the same
    /// one-frame hop [`Self::panel_messages`] gives `RemainingPanels`, taken by the one handler
    /// whose lists live on this screen rather than there. Returns true when the message was a
    /// doll tile's `0x3E`.
    pub fn on_paper_doll_drag_over(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::ElementMessage,
        view: &dyn GameView,
    ) -> bool {
        self.inventory.on_paper_doll_drag_over(ui, m, view)
    }

    /// A drag released over one of this
    /// screen's item lists.
    ///
    /// A `DropTarget::ItemList` carries a *list element id and a slot index*, and only the panel
    /// that filled the list knows which container object that list is showing or which object
    /// sits in the slot, so the interaction layer cannot resolve it. Resolving it here, where the
    /// map is, turns the drop into the `DropTarget::Container` the interaction layer already sends
    /// `PutItemInContainer` for.
    ///
    /// Returns the request to emit, or `None` when the drop lands on nothing (an empty slot of a
    /// list with no container, or a shortcut slot, which is `Toolbar`'s).
    #[must_use]
    pub fn accept_drag_object(
        &self,
        item: ObjectId,
        target: crate::view::DropTarget,
    ) -> Option<UiRequest> {
        use crate::view::DropTarget;
        match target {
            // A paper-doll slot is a wield location. This arm passes the element id through because
            // the id is all `DropTarget::EquipSlot` carries and `InventoryPanels::location_of_slot`
            // inverts it on the far side.
            //
            // What the far side still has to do, transcribed from the paper doll's drag-accept,
            // whose whole body is: take the item's valid locations; if the slot mask is
            // `0x00200000` and the item may go in `0x00100000`, also allow `0x00200000` and pick
            // the left side; refuse (and un-ghost) when the mask and the valid locations share no
            // bit; otherwise auto-wield (one bit) when `mask & 0x080001FF` is 0, else auto-wear
            // (the whole mask).
            //
            // A **gate** on the slot's mask against the item's valid locations, and a fork between
            // auto-wield (which picks a *single* free bit, `plan_auto_wield`) and auto-wear (which
            // sends the whole mask, `auto_wear`). TODO: `dereth_client::interaction`'s arm does
            // neither; it sends the valid locations unconditionally.
            DropTarget::BackpackButton
            | DropTarget::EquipSlot(_)
            | DropTarget::World
            | DropTarget::Container(_) => Some(UiRequest::DragDrop { item, target }),
            // [`crate::panels::inventory::InventoryPanels::resolve_item_list_drop`] runs first and
            // produces a `DropTarget::ItemListSlot` for this panel's lists; resolving to a bare
            // `DropTarget::Container` would throw the slot index away and, on an occupied slot, aim
            // the move at *the object in that slot* whether or not it is a container. Anything
            // still arriving here as `ItemList` is a list this panel does not own and keeps this
            // deliberately conservative answer.
            DropTarget::ItemList { .. } => {
                let c = self.inventory.resolve_drop(target)?;
                Some(UiRequest::DragDrop {
                    item,
                    target: DropTarget::Container(c),
                })
            }
            DropTarget::ItemListSlot { .. } => Some(UiRequest::DragDrop { item, target }),
            // The toolbar's drop handling → its "create shortcut to item" / "add shortcut". The
            // slot is already resolved — it is a slot **number**, not an element id — so unlike
            // `ItemList` there is nothing left to look up: the request goes out unchanged and the
            // bar fills when `PlayerModule` says it has. `ShortcutAlias` is the client's
            // is-shortcut drop arm. Same shape and for the same reason: both slot numbers are
            // already resolved, so there is nothing left for this map to look up.
            DropTarget::ShortcutSlot(_) | DropTarget::ShortcutAlias { .. } => {
                Some(UiRequest::DragDrop { item, target })
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // The gestures whose handlers existed and whose callers did not
    // -----------------------------------------------------------------------------------------

    /// The item list's element-message handler, message **`0x1C`** — the arm that
    /// makes an inventory icon do anything at all.
    ///
    /// What the client does, by input action (`p1`, not a button index), for the item under the
    /// mouse (none: nothing happens):
    ///
    /// * **7** — in a target mode, the targeted-use left click; otherwise, for an item, the
    ///   single-selection handling (on a single-selection list), select it, and open it when this
    ///   is a container list with a child list.
    /// * **8** — for an item, select it and examine it; for a spell, examine the spell.
    /// * **10** — use the item, unless this is the vendor's or the salvage list.
    ///
    /// **This arm is what makes inventory interactable.** Listening on `0x19 MOUSE_CLICK` instead
    /// would mean a double-click never eats, reads or equips anything and a right-click never
    /// appraises. All three requests are honoured by `dereth_client::interaction`; this is the arm
    /// that emits them.
    ///
    /// Two details that are easy to lose and are load-bearing:
    ///
    /// * **`0x1C` is the press, and `p1` is the input action** — 7 left, 8 right, 10 left
    ///   double-click (`dereth_ui::focus::action`). `0x19 MOUSE_CLICK` is a *completed* click and
    ///   folds the double-click into action 7, so it cannot tell "use" from "select";
    /// * **Selection runs on the right button too**, before the examine, which is why
    ///   the target box follows an appraisal. Both requests are emitted, in that order.
    ///
    /// The quickbar's eighteen lists are `ItemListWidget`s as well and reach the same handler,
    /// so a tile is resolved through [`crate::toolbar::shortcuts::ShortcutBar::slot_under`] when
    /// the inventory's own map does not know the element.
    ///
    /// The client checks the target mode before ordinary left-click selection/navigation. The app
    /// refreshes that global at each delivery, not once per frame.
    /// The client's `0x1C` arm — the click on the doll's **body**.
    ///
    /// Only on the drag mask `0x100001d6`: the doll item under the mouse position is looked up,
    /// and if there is one, action 7 runs the target mode on the **player** when a target mode is
    /// active and otherwise selects the item; action 8 selects it and examines it.
    ///
    /// Two things in that are worth saying out loud because they are surprising and they are
    /// what retail does.
    ///
    /// 1. **The target-mode arm passes the player's id, not the item under the mouse.** So using a
    ///    targeted item on your own doll targets **you**, whichever body region you clicked. It
    ///    reads like a bug in the client and it is the client's behaviour; it is transcribed, not
    ///    corrected, and the mask is carried anyway so the host can make the same choice.
    /// 2. **There is no empty-region special case.** The lookup falls back to
    ///    the player's own id when the placement list holds nothing over that colour,
    ///    so clicking a bare shoulder selects *yourself*. That fallback is the host's, on the far
    ///    side of [`UiRequest::PaperDollRegion`].
    ///
    /// Returns whether the press was the drag mask's, so the caller knows not to run the item-list
    /// handler over it — which would find no list and do nothing anyway, but says so here rather
    /// than relying on that.
    pub fn on_paper_doll_press(
        &mut self,
        ui: &mut UiSystem,
        source: ElemHandle,
        action: u32,
    ) -> bool {
        use dereth_ui::focus::action as act;
        if !self.inventory.is_doll_drag_mask(source) {
            return false;
        }
        let secondary = match action {
            act::PRIMARY_CLICK => false,
            act::SECONDARY_CLICK => true,
            // The `0x1C` arm has no other `p1` case; the press is still the mask's.
            _ => return true,
        };
        let (mx, my) = ui.mouse_pos();
        let mask = self.inventory.paper_doll_region_under_mouse(ui, mx, my);
        // No doll item — a colour that matched none of the nine, or a point off the surface.
        if mask == 0 {
            return true;
        }
        ui.requests
            .emit(UiRequest::PaperDollRegion { mask, secondary });
        true
    }

    pub fn on_item_list_press(&mut self, ui: &mut UiSystem, source: ElemHandle, action: u32) {
        use dereth_ui::focus::action as act;
        let item = self
            .inventory
            .locate(source)
            .and_then(|(_, _, item)| item)
            .or_else(|| {
                let slot = self.shortcuts.slot_under(ui, source)?;
                self.shortcuts.item_at(slot)
            });
        // No item under the mouse, or an item id of 0 — an empty slot does nothing at all,
        // not even a deselect.
        let Some(item) = item else { return };
        match action {
            act::PRIMARY_CLICK if self.interaction_target_mode => {
                ui.requests.emit(UiRequest::ExecuteTargetItem(item));
            }
            act::PRIMARY_CLICK => {
                // On a single-selection list, the single-selection handling runs
                // first, before the selection, and on the list that was clicked.
                self.single_select(ui, source);
                ui.requests.emit(UiRequest::Select(item));
                // The item list's open-container, and only for a container list; the panel's
                // own `on_slot_clicked` applies that guard.
                let _ = self.inventory.on_slot_clicked(&mut ui.requests, source);
            }
            act::SECONDARY_CLICK => {
                // `case 8` runs the same two lines as `case 7` before the examine; the walk is
                // not a left-click-only thing.
                self.single_select(ui, source);
                ui.requests.emit(UiRequest::Select(item));
                ui.requests.emit(UiRequest::Examine(item));
            }
            // `case 10` — `0x0A` is the left **double**-click.
            0x0A => ui.requests.emit(UiRequest::Use(item)),
            _ => {}
        }
    }

    /// The item list's single-selection handling, dispatched to whichever of this
    /// screen's lists the pressed element belongs to.
    ///
    /// The client has no dispatch to do — the element-message handler runs *on* the list — so this
    /// is the shape of the seam and not of the client: the inventory panel owns twenty-seven lists
    /// and the quickbar eighteen, and the handle is all that says which.
    ///
    /// Returns how many *other* slots were deselected. **It is 0 on every list this screen owns**,
    /// and that is retail's answer too: `UI_ItemList_SingleSelection` resolves `true` on exactly
    /// **1 of the 65** `ItemListWidget`s in the live gameplay tree — `0x100000C5`, the vendor's
    /// So the pack, the doll, the spellbook and the quickbar all take the `false` arm. See
    /// [`crate::items::widget::attr::SINGLE_SELECTION`].
    fn single_select(&mut self, ui: &mut UiSystem, source: ElemHandle) -> usize {
        self.inventory
            .handle_single_selection(ui, source)
            .or_else(|| self.shortcuts.handle_single_selection(ui, source))
            .unwrap_or(0)
    }

    /// The toolbar panel's element-message handler's `case 0x1000019d` and `case 0x100001a5`,
    /// on element message 1.
    ///
    /// Each button has two arms and the discriminator is whether anything is selected:
    ///
    /// | button | selection | what it does |
    /// |---|---|---|
    /// | `0x1000019D` Use | yes | use the selected object |
    /// | `0x1000019D` Use | no | enter the use target mode — the "use on…" cursor |
    /// | `0x100001A5` Examine | yes | examine the selected object |
    /// | `0x100001A5` Examine | no | enter the examine target mode |
    ///
    /// The app supplies the live selection through the input-dispatch context. Standalone screen
    /// callers keep the last toolbar readback as their explicit fallback.
    ///
    /// This is the only writer of the target mode from the toolbar; without it both targeted-use
    /// paths and `execute_target_mode_for_item` are unreachable.
    #[must_use]
    pub fn on_target_mode_button(&self, element: ElementId) -> Option<UiRequest> {
        use crate::toolbar::target_mode::{EXAMINE_BUTTON, USE_BUTTON};
        use crate::view::TargetMode;
        let selected = self
            .interaction_selection
            .unwrap_or(self.last_selection.flatten());
        match (element, selected) {
            (USE_BUTTON, Some(id)) => Some(UiRequest::Use(id)),
            (USE_BUTTON, None) => Some(UiRequest::SetTargetMode(TargetMode::Use)),
            (EXAMINE_BUTTON, Some(id)) => Some(UiRequest::Examine(id)),
            (EXAMINE_BUTTON, None) => Some(UiRequest::SetTargetMode(TargetMode::Examine)),
            _ => None,
        }
    }

    /// The client's target-mode and selected-object globals, read at a delivery boundary. Do not update
    /// last_selection here: that cache owns the toolbar's visible selection-change work.
    pub fn set_interaction_context(&mut self, target_mode: bool, selected: Option<ObjectId>) {
        self.interaction_target_mode = target_mode;
        self.interaction_selection = Some(selected);
    }

    /// The same input-dispatch target-mode snapshot for HUD-owned item-list subscribers.
    #[must_use]
    pub fn interaction_target_mode_active(&self) -> bool {
        self.interaction_target_mode
    }

    /// The selected-stack hotkey receiver.
    ///
    /// The native gates are in this order: the notice id still equals the current selection, the
    /// object resolves, its raw stack size is nonzero and greater than one, and the registered
    /// entry box exists. The receiver then gives that box focus and selects all of its text, so
    /// the next digit replaces the whole quantity.
    pub fn recv_split_stack(&mut self, ui: &mut UiSystem, selected: ObjectId, view: &dyn GameView) {
        if view.selection() != Some(selected) {
            return;
        }
        let Some(stack) = view.slot_decoration(selected).map(|d| d.stack_size) else {
            return;
        };
        if stack <= 1 {
            return;
        }
        let Some(box_h) = self.toolbar_children.get("stack_size_entry_box") else {
            return;
        };
        ui.take_focus(box_h);
        if let Some(text) = ui.text_element_mut(box_h) {
            text.select_all();
        }
    }

    /// The element-message handler's `0x100001A3` / message `0x2F` arm — the stack-size
    /// entry box gaining and losing focus.
    ///
    /// Gaining focus (`p1 != 0`) selects all the text and stops. Losing it parses the text
    /// (`wcstoul`), clamps it to `1..=max` (0 becomes 1), writes the clamped value back if it
    /// differs from what was typed, stores it as the split size, and — when it changed — sets the
    /// slider's `0x86` to `v / max` and sends the stack-slider-changed notice.
    ///
    /// [`crate::toolbar::splitter::Splitter::on_text`] is the parse-clamp-store part and had no
    /// caller outside its own tests, which is why `UiRequest::StackSliderChanged` was emitted by
    /// nothing and **every** split ran on the default whole-stack `SplitState` in
    /// `interaction.rs`.
    pub fn on_stack_box_focus(&mut self, ui: &mut UiSystem, gained: bool) {
        let Some(box_h) = self.toolbar_children.get("stack_size_entry_box") else {
            return;
        };
        if gained {
            // The text element's select all.
            if let Some(t) = ui.text_element_mut(box_h) {
                t.select_all();
            }
            return;
        }
        self.commit_stack_box(ui, box_h);
    }

    /// Commit the text currently visible in the stack entry through the same clamp/slider/notice
    /// path as its ordinary focus-loss handler.
    fn commit_stack_box(&mut self, ui: &mut UiSystem, box_h: ElemHandle) {
        let before = self.splitter.split_size;
        let typed = read_text(ui, box_h);
        let (notice, position) = self.splitter.on_text(&typed);
        let clamped = self.splitter.split_size;
        // "write it back if it was clamped".
        if crate::toolbar::splitter::parse_stack_quantity(&typed) != clamped {
            if let Some(t) = ui.text_element_mut(box_h) {
                t.set_text(&clamped.to_string());
            }
        }
        if clamped != before {
            if let Some(s) = self.toolbar_children.get("stack_size_slider") {
                set_attr_float(ui, s, attr::SLIDER_POSITION, position);
            }
            ui.requests.emit(notice);
        }
    }

    /// The element-message handler's `0x100001A4` / message `0x0A` arm — the slider
    /// moved.
    ///
    /// `split = unsigned_clamp(1 - trunc(p1 * max * widened(-0.001f)), 1, max)`, then
    /// the text box is written and the notice sent. The position0 endpoint is one item.
    ///
    /// Retail reads the message's `p1` as unsigned and multiplies by the widened -0.001f constant;
    /// it does not read the slider attribute back or reverse the formula. See
    /// `Splitter::on_slider`.
    pub fn on_stack_slider(&mut self, ui: &mut UiSystem, position_thousandths: u32) {
        let notice = self.splitter.on_slider(position_thousandths);
        if let Some(b) = self.toolbar_children.get("stack_size_entry_box") {
            let v = self.splitter.split_size.to_string();
            if let Some(t) = ui.text_element_mut(b) {
                t.set_text(&v);
            }
        }
        ui.requests.emit(notice);
    }

    /// The vendor panel's item list begin drag notice's partial sell-row tail:
    /// split size = maximum split size, then the update-toolbar-selection-display notice.
    ///
    /// Retail has one global pair. This rebuild has this visible toolbar copy and Interaction's
    /// inventory-facing copy, so the existing [`UiRequest::StackSliderChanged`] carries the same
    /// assignment across after the text and slider are refreshed here.
    pub fn reset_stack_split_to_max(&mut self, ui: &mut UiSystem) {
        self.splitter.split_size = self.splitter.max_split_size;
        if let Some(b) = self.toolbar_children.get("stack_size_entry_box") {
            if let Some(text) = ui.text_element_mut(b) {
                text.set_text(&self.splitter.split_size.to_string());
            }
        }
        if let Some(slider) = self.toolbar_children.get("stack_size_slider") {
            set_attr_float(ui, slider, attr::SLIDER_POSITION, 1.0);
        }
        ui.requests.emit(UiRequest::StackSliderChanged {
            split: self.splitter.split_size,
            max: self.splitter.max_split_size,
        });
    }

    /// The client's focus arm: global message 1, action
    /// `0x27`, **while the stack-size box has focus** — write the split size into the box and
    /// relinquish focus.
    ///
    /// Retail loads the split size, not the maximum split size: cancel restores the committed
    /// quantity. This global arm does not bypass the text element's own action handler consuming
    /// an ordinary focused `EditControls` Escape key.
    ///
    /// Relinquishing focus raises `0x2F` with `p1 = 0` on the box, so the read-back and the notice
    /// come out of [`Self::on_stack_box_focus`] rather than being duplicated here — which is the
    /// client's own path too.
    pub fn reset_stack_size_box(&mut self, ui: &mut UiSystem) {
        let Some(h) = self.toolbar_children.get("stack_size_entry_box") else {
            return;
        };
        if ui.focus_element() != Some(h) {
            return;
        }
        let committed = self.splitter.split_size.to_string();
        if let Some(t) = ui.text_element_mut(h) {
            t.set_text(&committed);
        }
        ui.relinquish_focus(h);
    }

    /// The link-status indicator's initialization and
    /// the client's four stance buttons.
    ///
    /// The lamps are found **by element type**, not by id: the client's element-class registration
    /// covers six widget classes and which element in which window is one of them is layout data. The
    /// shipped `classic_gameplay` puts a link lamp in `<INDI>` and another in the docked strip.
    fn indicators_post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        self.link_lamps.clear();
        for h in elements_of_type(ui, root, crate::element_types::ty::LINK_STATUS_INDICATOR) {
            let mut lamp = crate::hud::indicators::LinkStatusIndicator::default();
            let state = lamp.post_init();
            ui.set_state(h, dereth_ui::StateId(state));
            self.link_lamps.push((h, lamp));
        }
        // The other five lamp classes, found the same way and for the same reason.
        // `hud::indicators::all_states` drives them; without this the burden, buff, debuff, vitae,
        // mini-game and portal-storm lamps sit for ever in whatever state the shipped layout gave
        // them (`0x0E` for burden, `0x0D` for the rest) whatever the character is carrying or
        // suffering.
        //
        // The effects lamp is the one that needs a per-element field: `EffectsIndicator` reads
        // attribute `0x1000000C` to decide **which** enchantments it counts, and the shipped strip
        // carries two of them — `0x100000F5` with kind 1 (helpful) and `0x100000F6` with kind 2
        // (harmful). Reading it once and applying it to both would light the debuff lamp for a
        // buff.
        self.indicator_lamps.clear();
        for ty in [
            crate::element_types::ty::BURDEN_INDICATOR,
            crate::element_types::ty::EFFECTS_INDICATOR,
            crate::element_types::ty::MINI_GAME_INDICATOR,
            crate::element_types::ty::PORTAL_STORM_INDICATOR,
            crate::element_types::ty::VITAE_INDICATOR,
        ] {
            for h in elements_of_type(ui, root, ty) {
                let kind = attr_enum(ui, h, crate::hud::indicators::effects_kind::ATTRIBUTE)
                    .unwrap_or(crate::hud::indicators::effects_kind::BOTH);
                self.indicator_lamps.push(IndicatorLamp {
                    handle: h,
                    ty,
                    kind,
                    state: None,
                });
            }
        }
        // The combat-mode notice handler's four child lookups and visibility writes, paired with
        // the `COMBAT_MODE` value each stands for — see [`crate::toolbar::combat_mode`].
        self.combat_mode_buttons.clear();
        for (mode, id) in crate::toolbar::combat_mode::BUTTONS {
            if let Some(h) = ui.get_child_recursive(root, id) {
                self.combat_mode_buttons.push((mode, h));
            }
        }
        self.last_combat_mode = None;
    }

    /// The lamps' half of global message 3, plus the toolbar's combat-mode icon.
    ///
    /// The client's per-frame update reads the current time off the global timer;
    /// [`UiSystem::now`] is the same
    /// value supplied by the frame driver that broadcasts message 3 in the first place.
    ///
    /// Returns how many elements were written, so a host can count that it ran at all.
    pub fn update_indicators(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> u32 {
        let now = ui.now.0;
        let status = view.link_status();
        let mut wrote = 0;
        for i in 0..self.link_lamps.len() {
            let (h, mut lamp) = self.link_lamps[i];
            let before = lamp.media_state;
            let state = lamp.use_time(now, status);
            self.link_lamps[i].1 = lamp;
            if state != before {
                ui.set_state(h, dereth_ui::StateId(state));
                wrote += 1;
            }
        }
        // The other five lamps' update. Each is the client's own function, transcribed in
        // [`crate::hud::indicators`]; this is the call and the media-state write.
        //
        // In the client each lamp runs its own update off its own notices (player description
        // received, load changed, enchantments changed, vitae changed, begin and end game, portal
        // storm level). This
        // build has no notice bus reaching an element, so they are re-evaluated on the same tick
        // the link lamp uses and written only when the answer changes — which is what the notices
        // would have produced, minus the latency of at most one frame: a lamp here can light one
        // frame after the message that lit it, where retail lights it in the notice.
        for i in 0..self.indicator_lamps.len() {
            let lamp = self.indicator_lamps[i];
            let Some(state) = indicator_state(view, lamp.ty, lamp.kind) else {
                continue;
            };
            if lamp.state == Some(state) {
                continue;
            }
            self.indicator_lamps[i].state = Some(state);
            ui.set_state(lamp.handle, dereth_ui::StateId(state));
            wrote += 1;
        }
        // **The combat-mode fan-out lives here.**
        // See [`crate::hud::combat_notice`] for why the edge stands in for the notice and for the
        // four handlers it reaches. `Toolbar`'s stance icons are one arm; the other three are
        // below.
        let mode = view.combat_mode();
        let inputs = (
            view.advanced_combat_ui(),
            view.recklessness_advancement_class(),
        );
        if self.last_combat_mode != Some(mode) {
            self.last_combat_mode = Some(mode);
            for (m, h) in &self.combat_mode_buttons {
                ui.set_visible(*h, *m == mode);
                wrote += 1;
            }
        } else if self.last_combat_ui_inputs == Some(inputs) {
            return wrote;
        }
        // The two extra inputs get their own edge: AdvancedCombatUI and the
        // `Recklessness` advancement class are read *inside* `CombatWindow`'s handler, so in retail
        // they take effect at the next mode change. Here they would otherwise never take effect
        // at all in a session where the mode does not move, which is strictly worse than the
        // one-frame lateness the rest of this edge carries.
        self.last_combat_ui_inputs = Some(inputs);
        wrote += self.on_set_combat_mode(ui, mode, inputs.0, inputs.1);
        wrote
    }

    /// The three combat-mode notice handlers that were absent —
    ///
    /// Updates the combat page and spellcasting page for one `COMBAT_MODE`, and clears the default
    /// page. The caller and [`crate::toolbar::shortcuts`] own the other two effects.
    ///
    /// **The order is `CombatWindow` then `SpellcastingPanel`, and it matters.** Both raise element
    /// message `0x18` into the `<COMB>` stack, and the stack shows one page at a time: whichever
    /// runs last wins the "current" slot. Retail's order is the notice-registration order and this
    /// build has no registration list, so the two are ordered the way the id block orders them
    /// (`CombatWindow` is element type `0x1000000C`, `SpellcastingPanel` `0x10000015`) — and the
    /// choice is *unobservable* here, because the two arms are mutually exclusive: `CombatWindow`
    /// shows only in melee/missile and `SpellcastingPanel` only in magic, so at most one of the two
    /// `set_visible(true)` calls ever happens for a given mode. `// UNVERIFIED:` the registration
    /// order itself; the mutual exclusion is `\[verified\]` against both retail handlers.
    ///
    /// Returns how many elements were written, so the caller can count that it ran at all.
    pub fn on_set_combat_mode(
        &mut self,
        ui: &mut UiSystem,
        mode: u32,
        advanced_combat_ui: bool,
        recklessness_sac: u32,
    ) -> u32 {
        use crate::hud::combat_notice as cn;
        let mut wrote = 0;
        if let Some(page) = self.combat_ui_page {
            let arm = cn::combat_ui_arm(mode, advanced_combat_ui, recklessness_sac);
            cn::apply_combat_ui_arm(ui, page, self.combat_recklessness_field, arm);
            wrote += 1;
        }
        if let Some(page) = self.spellcasting_page {
            ui.set_visible(page, cn::spellcasting_arm(mode));
            wrote += 1;
        }
        // `CombatPanelStack`'s set-combat-mode notice — the default page is cleared on
        // `NONCOMBAT`. `PanelStack::previous` is the same value (`PanelStack` calls it the
        // previously shown page, `CombatPanelStack` and `EnvironmentPanelStack` the default
        // page); see [`cn::combat_panel_clears_default_page`] for why this arm cannot
        // be observed on the shipped layout.
        if cn::combat_panel_clears_default_page(mode) {
            self.combat_panel.previous = None;
        }
        wrote
    }

    /// `ExaminationPanel`'s frame. One line, so the panel is driven from the same
    /// place every other window on this screen is; see
    /// [`crate::panels::examination::ExaminationPanel::update`] for why it is a pull.
    ///
    /// Returns whether the panel changed this frame.
    pub fn update_examination(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        self.examination.update(ui, view)
    }

    /// The vitals panel's element-message handler's `0x1C` arm.
    ///
    /// On action 7 or `0xA` it toggles the window between states `0x10000006` and `0x10000007`
    /// (`0x10000006` becomes `0x10000007`; anything else becomes `0x10000006`).
    ///
    /// The state is read from and written to the **vitals window**, not the child that was
    /// pressed. `set_state`'s pass-to-children walk is what takes
    /// the new state down to the three meter groups and their labels, all of which author both
    /// `0x10000006` and `0x10000007` in the shipped `classic_gameplay` layout.
    ///
    /// Returns the window it toggled, for the frame's own denominator.
    pub fn on_vitals_press(
        &mut self,
        ui: &mut UiSystem,
        m: &ElementMessage,
    ) -> Option<dereth_ui::ElementId> {
        use crate::hud::vitals::vitals_display as v;
        if m.id != dereth_ui::msg::element::id::MOUSE_PRESS || !v::is_toggle_press(m.p1) {
            return None;
        }
        for id in [window::STACKED_VITALS, window::SIDE_VITALS] {
            if !is_under_element(ui, m.source, id) {
                continue;
            }
            let Some(h) = ui.get_element(id) else {
                continue;
            };
            let cur = ui.node(h).map_or(dereth_ui::StateId(0), |n| n.state);
            ui.set_state(h, v::next_state(cur));
            self.vitals_display_toggles += 1;
            return Some(id);
        }
        None
    }

    /// `BookPanel`'s frame, and the same one line for the same reason.
    ///
    /// The client is *pushed*; this build pulls
    /// [`GameView::open_book`] and runs on the frame that first sees a new
    /// `opening` counter. See [`crate::panels::book`].
    pub fn update_book(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        self.book.update(ui, view)
    }

    /// Pull a new `0x0075 Character_StartBarber` into the shipped barber modal.
    pub fn update_barber(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        self.barber.update(ui, view)
    }

    /// The button-click handling for elements that are not `Button` widgets — without it no lamp
    /// in the indicator strip can open anything.
    ///
    /// The button-click handling reads the input action from attribute `0x12` (none: refuse),
    /// refuses action 1 and a missing input manager or action map, and otherwise hands the
    /// action handler a press event (toggle type 3) for that action.
    ///
    /// It is reached — element message
    /// **1 on the button itself**, refused while attribute `0x0D` (disabled) is set — so **every**
    /// button in the shipped tree that carries attribute `0x12` fires an *input action* on click,
    /// exactly as if the key bound to it had been pressed. The client's
    /// press arm is the key-press event with the action and its extent, which is
    /// [`dereth_ui::UiSystem::key_press`] — global message 1 plus
    /// the visibility-toggle action, the leg that raises `0x31` on
    /// every element registered for the action under attribute `0x57` and lets its `0x58` open the
    /// panel.
    ///
    /// **This is the middle of that chain.** Measured over the shipped `classic_gameplay` tree:
    /// **38** elements carry attribute `0x12` and **40** carry `0x57`; without a reader of `0x12`
    /// all 38 are dead buttons, the six indicator lamps among them.
    ///
    /// | lamp | element | attribute `0x12` | the action's name | the `0x57` listener it opens |
    /// |---|---|---|---|---|
    /// | link status | `0x100000F8` | `0x10000009` | Show/Hide Link Status Panel | `0x10000187` `LinkStatusPanel` |
    /// | buffs | `0x100000F5` | `0x10000006` | Show/Hide Positive Magic Panel | `0x10000184` `EffectsPanel` |
    /// | debuffs | `0x100000F6` | `0x10000007` | Show/Hide Negative Magic Panel | `0x10000185` `EffectsPanel` |
    /// | vitae | `0x100000F4` | `0x1000000C` | Show/Hide Vitae Panel | `0x1000018A` `VitaePanel` |
    /// | burden | `0x100000F7` | `0x10000005` | Show/Hide Character Info Panel | `0x10000183` `CharacterInfoPanel` |
    /// | mini-game | `0x100000F3` | `0x1000000A` | *(unnamed in the keymap)* | `0x10000188` `MiniGamePanel` |
    ///
    /// # Why this copy is live
    ///
    /// `dereth_ui`'s `Button` widget carries the arm, so a real `Button` with a live `0x12`
    /// returns stop-processing and this copy is unreachable for one. It is **not** unreachable
    /// in general: the six indicator lamps are not `ty::BUTTON`, they are
    /// `BurdenIndicator` and its five siblings, registered in
    /// [`crate::register_all`] through `game_element` and therefore carrying no `Button`
    /// behaviour. Their message 1 bubbles, and **this is the only thing that fires their action**;
    /// five real clicks on the lamps take `button_actions_fired` to five.
    ///
    /// **The faithful home is still not here.** In retail each lamp class carries its own copy:
    /// the burden indicator's element-message handler repeats the button handler — on message 1
    /// to itself, unless attribute `0xD` (disabled) is set, it runs the button-click handling and
    /// stops processing when that fired — because these classes **derive from `Button`**. TODO:
    /// give the six lamp element classes the button arm (in this crate's `register_all`, not in
    /// `dereth-ui`), after which this copy really is dead and goes.
    ///
    /// Returns the action fired, so a caller can count that it ran.
    fn handle_button_click(&mut self, ui: &mut UiSystem, source: ElemHandle) -> Option<u32> {
        let props = ui.node(source)?.merged_properties();
        // "a click is swallowed when attribute `0x0D` (disabled) is set" — the guard is in
        // the element-message handler, above the call, and is the client's own.
        if props.get_bool(attr::DISABLED).unwrap_or(false) {
            return None;
        }
        let action = props.get_enum(BUTTON_INPUT_ACTION)?;
        // Action 1 is the client's "no action", and it is refused
        // before the map is consulted.
        if action == 1 {
            return None;
        }
        let (x, y) = ui.mouse_pos();
        ui.key_press(&dereth_ui::focus::InputEvent {
            action,
            start: true,
            x,
            y,
        });
        Some(action)
    }

    /// The client's third statement — the one that makes
    /// the 3-D viewport hit-testable at all.
    ///
    /// After base post-init, construct the drag icon's layout descriptor with values
    /// `0x23`, `5`, and `0x10000038`, create child `0x10000345`, cache it, and hide it.
    /// Then make the containing element mouse-visible.
    ///
    /// That is the **same line** the item slot carries, without which nothing in an item list is
    /// hit-testable. Without it `<SBOX>` has the identical hole, and it costs two different things:
    ///
    /// * the mouse-over hit test only considers an element that is mouse-visible or blocks
    ///   clicks, so `hit_test_screen` over the viewport returns **`None`**
    ///   (`dereth_client::interaction::is_world_click` reads `None` as the world, so *clicks* would
    ///   still work);
    /// * the mouse-over switch asks the **hit element** for its drag-and-drop catcher, and with no
    ///   hit element the last drag-cursor-over element stays null, the drag-and-drop stop
    ///   broadcasts its `0x15` to nobody, and [`Self::handle_drop_release`] is never called for a
    ///   drop into the world, so the `DropTarget::World` arm has **no producer on the live tree**.
    ///
    /// So the fix is the hit test rather than either consumer. The catcher half is right as it is:
    /// `<SBOX>` carries attribute `0x36` from the shipped layout, and
    /// `drag_and_drop_catcher(<SBOX>)` answers `<SBOX>`.
    ///
    /// The drag icon child (`0x10000345`) is deliberately not created here: this screen has no
    /// drag proxy of its own, `dereth_ui::UiSystem` makes one for every drag, and
    /// the set-up's own next act is to hide it.
    fn world_view_post_init(&mut self, ui: &mut UiSystem) {
        let Some(root) = self.root() else { return };
        let Some(sbox) = ui.get_child_recursive(root, window::SMART_BOX) else {
            return;
        };
        ui.set_mouse_visible(sbox, true);
    }

    /// The one line that reconciles the client's compositing with this rebuild's.
    ///
    /// **The world is not drawn "behind" the UI: it is an element.** `WorldViewWrapper` (type
    /// `0x10000030`) hosts the world view inside the element tree. So in the client the gameplay
    /// root's own background — `classic_gameplay` gives element `0x10000495` the full-screen image
    /// `0x060022BA` — is blitted first and then covered, pixel for pixel, by the smart box's 3D
    /// output, which is a *later* child in the same tree.
    ///
    /// This rebuild draws the 3D scene in its own pass **before** the UI overlay, because that is
    /// what `PresentFrame(true) ` is ("the 2D UI overlay, `EndScene`, `Present`").
    /// Painting the backdrop from the overlay would therefore cover the world rather than be
    /// covered by it — a full-screen brown rectangle over Holtburg, the same shape of bug a wrong
    /// z-level sort order produces one layer up.
    ///
    /// The transparent flag suppresses the **background blit and nothing else** (children and
    /// composed text still draw), so setting it on the root while the smart box covers the root is
    /// the identical picture. When the player shrinks `<SBOX>` the two arrangements diverge — the
    /// client shows the backdrop around the viewport and this shows the world — and that is the
    /// compositing difference, reported rather than papered over.
    fn let_the_world_through(&mut self, ui: &mut UiSystem) {
        let Some(root) = self.root() else { return };
        let Some(sbox) = ui.get_child_recursive(root, window::SMART_BOX) else {
            return;
        };
        let (r, b) = (ui.screen_box(root), ui.screen_box(sbox));
        let covered = b.x0 <= r.x0 && b.y0 <= r.y0 && b.x1 >= r.x1 && b.y1 >= r.y1;
        if let Some(n) = ui.node_mut(root) {
            n.region.flags.transparent = covered;
        }
    }

    /// The HUD's start state: which of the eighteen children of `0x10000495` are up before any
    /// notice has fired.
    ///
    /// Six windows are visible when the game phase begins, and that is a **verified live
    /// observation** made before any of this was read out of the layout. The retail client
    /// at 800×600 against this same ACE server shows the viewport, the indicator strip top-left,
    /// the stacked vitals top-centre, the radar top-right, the main chat window bottom-left and the
    /// toolbar bottom-right, and nothing else.
    ///
    /// **The layout, read correctly, is the mechanism.** `0x3B` is `UICore_Element_hide`, not
    /// "visible"; read that way the shipped `classic_gameplay` layout brings up **eight** of the
    /// eighteen children, and the only two of those eight retail does not draw are taken down by
    /// the client's own code:
    ///
    /// | brought up by the layout | and then | by code? |
    /// |---|---|---|
    /// | `<SBOX>`, `<INDI>`, `<VITS>`, `<RADA>`, `<CHAT>`, `<TBAR>` | nothing — these are the six retail shows | the layout |
    /// | `<PANS>` | sets its own visibility from its current page, which is null | yes |
    /// | `<SVIT>` | sets its visibility from the side-by-side vitals option | yes |
    ///
    /// and the other ten — `<PBAR>`, `<COMB>`, `<EXAM>`, `<ENVP>`, all four `<FCHn>`,
    /// `classic_keyboard` and `classic_admin` — carry `0x3B = true` and need no code at all. That
    /// is checked directly by a test, on a tree built from the retail dat with **no** set-up run.
    ///
    /// In particular `0x100004A8` (`classic_keyboard`) and `0x100004D1` (`classic_admin`), which
    /// carry no window id, no panel id and are referenced by no client code, and which cover the
    /// whole viewport when shown, are hidden by their own base elements' `0x3B`. There is no code
    /// to find.
    ///
    /// The table is **kept** rather than deleted, because it is not redundant: `<SVIT>`/`<VITS>`
    /// and the three notice-driven bars are code-driven in the client too, and it is what runs
    /// before the server's placement blob arrives. It *agrees* with the layout everywhere.
    ///
    /// The server blob does not explain any of it either: a live `0x0013` from ACE for this account
    /// carries 17 `Option_Placement` rows of which exactly one (window 13, `<PBAR>`) sets
    /// `Option_Placement_Visibility`, and it sets it false.
    pub const HUD_START_VISIBILITY: [StartVisibility; 17] = [
        // ---- the layout's own `0x3B`, restated so the blob-override order below has a base ----
        sv(window::MAIN_CHAT, true, "layout: no hide flag"),
        sv(ElementId(0x1000_0505), false, "layout: 0x3B = true"),
        sv(ElementId(0x1000_050E), false, "layout: 0x3B = true"),
        sv(ElementId(0x1000_050F), false, "layout: 0x3B = true"),
        sv(ElementId(0x1000_0510), false, "layout: 0x3B = true"),
        sv(
            window::KEYBOARD,
            false,
            "layout: 0x3B = true on its base element",
        ),
        sv(
            window::ADMIN,
            false,
            "layout: 0x3B = true on its base element",
        ),
        // The client hides the panel on the arm that leaves no page current, exactly as
        // `PanelStack` does, and its set-up is not known to run it at start-up — but the
        // layout settles it anyway:
        // `<ENVP>` carries `0x3B = true`.
        sv(
            window::ENV_PANEL,
            false,
            "layout: 0x3B = true; no page current either way",
        ),
        // ---- documented ----
        // The power bar, combat panel, and examination panel are shown and hidden by notices.
        // None of the
        // three notices has fired when the phase begins.
        sv(
            window::POWER_BAR,
            false,
            "notice-driven: shown when a power-bar charge begins",
        ),
        sv(
            window::SMART_BOX_POWER_BAR,
            false,
            "notice-driven: the in-viewport power bar",
        ),
        sv(
            window::COMBAT_PANEL,
            false,
            "notice-driven: shown when combat mode is entered",
        ),
        sv(
            window::EXAMINATION,
            false,
            "notice-driven: shown when an object is examined",
        ),
        // The client registers the player-appearance-changed and start-barber notices, and nothing
        // else opens the window: it is the barber's modal, and it is a child of the smart box.
        sv(window::BARBER, false, "notice-driven: the barber's modal"),
        // "If disabled, turned off or **no target**, both elements are hidden."
        sv(
            window::TARGET_ON_SCREEN,
            false,
            "no target: both target indicators are hidden",
        ),
        sv(
            window::TARGET_OFF_SCREEN,
            false,
            "no target: both target indicators are hidden",
        ),
        // Exactly one of the two vitals windows is up, and the side-by-side vitals option is false
        // until the server says otherwise.
        // `update_from_player_module` re-decides both once a `PlayerModule` has arrived.
        sv(
            window::STACKED_VITALS,
            true,
            "the side-by-side vitals option starts off",
        ),
        sv(
            window::SIDE_VITALS,
            false,
            "the side-by-side vitals option starts off",
        ),
    ];

    /// Apply [`Self::HUD_START_VISIBILITY`]. Called from `post_init`, before the server's placement
    /// blob has arrived; [`Self::update_from_player_module`] then overrides whatever the blob does
    /// carry, which is the order the client runs them in.
    pub fn apply_start_visibility(&mut self, ui: &mut UiSystem) -> usize {
        let Some(root) = self.root() else { return 0 };
        let mut n = 0;
        for row in Self::HUD_START_VISIBILITY {
            if let Some(h) = ui.get_child_recursive(root, row.element) {
                // TBAR already has its authored visibility. Retail has no startup visibility change
                // here; a redundant call invokes its real persistence tail and would
                // overwrite the character's stored hidden state during construction.
                if row.element != window::TOOLBAR {
                    ui.set_visible(h, row.visible);
                }
                n += 1;
            }
        }
        n
    }

    /// The geometry and the eight attribute-named children.
    /// The five child lookups and the four
    /// marker-area attributes.
    ///
    /// The five ids are looked up from the gameplay root rather than from a `MapPanel` handle
    /// because the map's panel page is layout data (attribute `0x10000029`) and nothing in this
    /// crate pairs a page id with a class. The five ids are unique in the shipped layout, so the
    /// recursive walk finds the same elements retail's set-up would.
    ///
    /// **The note loop.** The 53 location notes are created from the layout named by attribute
    /// `0x48` and the element enum at `0x47`; both attributes are in the shipped layout
    /// (`0x47 = 0x100001F0`, `0x48 = 0x21000026`), the loop is
    /// [`crate::mapradar::map::create_map_notes`], and the notes are not cosmetic: their tooltips
    /// are the only thing on the page a pointer can do anything with.
    fn map_post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        use crate::mapradar::map::{attr, child};
        let find = |ui: &UiSystem, id| ui.get_child_recursive(root, id);
        self.map.page = find(ui, crate::mapradar::map::MAP_PAGE);
        self.map.date_time_text = find(ui, child::DATE_TIME_TEXT);
        self.map.coordinate_text = find(ui, child::COORDINATE_TEXT);
        self.map.player_icon = find(ui, child::PLAYER_LOCATION_ICON);
        self.map.house_icon = find(ui, child::HOUSE_LOCATION_ICON);
        self.map.map_image = find(ui, child::MAP_IMAGE);
        self.map_notes.clear();
        if let Some(m) = self.map.map_image {
            let a = |name| attr_int(ui, m, name).unwrap_or(0);
            self.map.marker_area = crate::mapradar::map::MarkerArea {
                x0: a(attr::MARKER_AREA_X0),
                x1: a(attr::MARKER_AREA_X1),
                y0: a(attr::MARKER_AREA_Y0),
                y1: a(attr::MARKER_AREA_Y1),
            };
            // The set-up's own next statement: read enum attribute `0x47` and data-id attribute
            // `0x48`, load that layout, then add 53 map notes. It reads both
            // attributes off the map image, which is why it is inside this block and not beside it.
            self.map_notes = crate::mapradar::map::create_map_notes(ui, m);
        }
        // Hiding the house icon is not in retail's set-up; the icon comes up however the layout
        // left it and the first update decides. A zeroed house position is not valid, so the first
        // pass hides it.
        self.last_map_date_time = None;
        self.last_map_coords = None;
    }

    /// The map panel's update, throttled by the caller.
    ///
    /// Returns whether anything was written, the same contract [`Self::update_radar`] has.
    ///
    /// The three blocks, and their three *different* gates — not all three "outdoors only":
    ///
    /// 1. **Date/time**, gated on the date/time text existing and nothing else. Being outside is
    ///    not consulted; the date keeps ticking in a dungeon.
    /// 2. **Coordinates and the player icon**, gated on both the coordinate text and the player
    ///    icon existing, and then on being outside. Indoors the text is set to the empty string —
    ///    **set, not hidden**, which is where this differs from the radar's strip — and the icon
    ///    is hidden. Note the **and**: a layout missing either element suppresses
    ///    both.
    /// 3. **The house icon**, gated on the house position being valid, which is
    ///    not being outside either. Left inert here: `0x0225 HouseData` is never received by this
    ///    client, so there is no valid position to place and the hide arm is the whole of it.
    ///
    /// `place_marker_on_map` ends by making the element it moved visible.
    /// The map panel's global-message handler's whole body bar the call: on global message 3,
    /// update when the next-update time is at or before now.
    ///
    /// Separate from [`Self::update_map`] because that is where retail's split is: the guard reads
    /// the next-update time in the listener, and `Update`'s *first* line is what advances it.
    /// It starts at zero, so the first tick after construction always runs.
    pub fn map_update_due(&self, now: f64) -> bool {
        self.next_map_update <= now
    }

    /// See [`Self::map_update_due`] for the caller's guard.
    pub fn update_map(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        use crate::mapradar::map::{date_time_text, place_marker_on_map, UPDATE_INTERVAL_SECONDS};
        // Next update = now + 5.0 — the client, the first thing
        // `Update` does, before it has looked at a single element.
        self.next_map_update = ui.now.0 + f64::from(UPDATE_INTERVAL_SECONDS);
        let mut wrote = false;

        // 1. The date. Ungated.
        if let Some(h) = self.map.date_time_text {
            let strings = view.game_date_time();
            let text = date_time_text(strings.as_ref().map(|(d, t)| (d.as_str(), t.as_str())));
            if self.last_map_date_time.as_deref() != Some(text.as_str()) {
                if let Some(t) = ui.text_element_mut(h) {
                    t.set_text(&text);
                    self.last_map_date_time = Some(text);
                    wrote = true;
                }
            }
        }

        // 2. The coordinates and the green circle. Both elements required, then being outside.
        if let (Some(text_h), Some(icon_h)) = (self.map.coordinate_text, self.map.player_icon) {
            // `player_outside` and `player_coords` are one question here: the cell the
            // second rejects is exactly the cell the first calls inside (`gid_to_lcoord` bails on
            // `(id & 0xFFFF) >= 0x100`, which is the outside test itself).
            let coords = view
                .player_outside()
                .then(|| view.player_coords())
                .flatten();
            let text = match coords {
                // `"%.1f%s, %.1f%s"` with `|ns|`, the NS suffix, `|ew|` and the EW suffix -- the
                // same string the radar's strip shows, and
                // the suffix is empty, not "N"/"E", when the component is exactly zero.
                Some((ns, ew)) => {
                    let c = crate::mapradar::radar::update_coordinates((ns, ew));
                    c.combined
                }
                // The empty string, then the text write.
                None => String::new(),
            };
            if self.last_map_coords.as_deref() != Some(text.as_str()) {
                if let Some(t) = ui.text_element_mut(text_h) {
                    t.set_text(&text);
                    self.last_map_coords = Some(text);
                    wrote = true;
                }
            }
            match coords {
                Some((ns, ew)) => {
                    let b = element_box(ui, icon_h);
                    // The width read and the height read on the Y pair are both **`x1 - x0 + 1`**,
                    // an inclusive box. Passing `(x1 - x0, y1 - y0)` would be one short in both
                    // axes, and `place_marker_on_map` halves them. On the shipped 17x16 icon
                    // `0x100001ED` the X error cancels (17/2 == 16/2 == 8) and the Y one does not
                    // (16/2 = 8, 15/2 = 7), so **the green circle would sit one pixel below where
                    // retail puts it** in every position on the map.
                    let size = (b.width(), b.height());
                    let (x, y) = place_marker_on_map(self.map.marker_area, ew, ns, size);
                    if (b.x0, b.y0) != (x, y) {
                        ui.move_to(icon_h, x, y);
                        wrote = true;
                    }
                    // The make-visible half of `place_marker_on_map`'s tail.
                    if !ui.node(icon_h).is_some_and(|n| n.region.flags.visible) {
                        ui.set_visible(icon_h, true);
                        wrote = true;
                    }
                }
                None => {
                    if ui.node(icon_h).is_some_and(|n| n.region.flags.visible) {
                        ui.set_visible(icon_h, false);
                        wrote = true;
                    }
                }
            }
        }

        // 3. The house icon. No `HouseData` reaches this client, so the house position is never
        // valid and
        // this is the hide arm every time.
        if let Some(h) = self.map.house_icon {
            if ui.node(h).is_some_and(|n| n.region.flags.visible) {
                ui.set_visible(h, false);
                wrote = true;
            }
        }
        wrote
    }

    fn radar_post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        use crate::mapradar::radar::{token_magnitude, Compass};
        let Some(radar) = ui.get_child_recursive(root, window::RADAR) else {
            return;
        };

        // The radar radius, and the centre point.
        //
        // **The earlier description reads as three separate attributes, but it is one.**
        // `0x1000002E` is a **`Struct`** whose two integer members are named `0x1000002F` and
        // `0x10000030` — which is what "a `Vector2` assembled from three attribute reads" is
        // describing: an attribute read on the outer name and then one read per member. In the
        // shipped `classic_radar` the struct is `{0x1000002F: 60, 0x10000030: 60}` and the radius
        // is 50, so reading `0x1000002E` as a float yields nothing and the centre lands on `(0, 0)`
        // — which puts every compass token off the top-left corner of the screen. [read off the
        // live merged property set] The window id, enum attribute `0x1000007e` — the set-up's very
        // first read after the base set-up, and the key every per-window chat option row is filed
        // under. Without it the radar has no window id and cannot persist a drag.
        self.radar.window_id = attr_enum(ui, radar, 0x1000_007E)
            .or_else(|| attr_int(ui, radar, 0x1000_007E).map(|v| u32::try_from(v).unwrap_or(0)))
            .unwrap_or(0);
        self.radar.radius = attr_int(ui, radar, 0x1000_002D).unwrap_or(0);
        #[allow(clippy::cast_precision_loss)] // pixel coordinates inside an 800x600 display
        let m = |inner: u32| {
            crate::bind::attr_struct_int(ui, radar, 0x1000_002E, inner).unwrap_or(0) as f32
        };
        self.radar.center = (m(0x1000_002F), m(0x1000_0030));

        // Each attribute holds a **child element id**; the child is what gets cached.
        let by_attr = |a: u32| -> Option<ElemHandle> {
            let id = attr_enum(ui, radar, a)
                .or_else(|| attr_int(ui, radar, a).map(|v| u32::try_from(v).unwrap_or(0)))?;
            ui.get_child_recursive(radar, ElementId(id))
        };
        self.radar.north = by_attr(Compass::North.attribute());
        self.radar.south = by_attr(Compass::South.attribute());
        self.radar.east = by_attr(Compass::East.attribute());
        self.radar.west = by_attr(Compass::West.attribute());
        self.radar.coordinate_container = by_attr(0x1000_0035);
        self.radar.combined_coords = by_attr(0x1000_0036);
        self.radar.x_coord = by_attr(0x1000_0037);
        self.radar.y_coord = by_attr(0x1000_0038);

        // Each token's magnitude = |centre point - token centre|, recorded once.
        let center = self.radar.center;
        for (i, which) in Compass::ALL.into_iter().enumerate() {
            let b = self.radar_token(which).map(|h| element_box(ui, h));
            self.radar.magnitudes[i] = b.map_or(0.0, |b| token_magnitude(center, b));
        }

        // "`0x100006A3` -> the drag button (**hidden at init**)".
        if let Some(h) = ui.get_child_recursive(radar, crate::mapradar::radar::child::DRAG_BUTTON) {
            ui.set_visible(h, false);
        }

        // **The padlock.** The radar update ends by giving the lock-UI button its media for the
        // *current* lock-UI flag — `0x10000063` when locked, `0x10000064` when not. Doing that swap
        // in [`Self::cascade_lock`] only would mean that until the player toggled the lock the
        // button had never been told which of its two states to draw and the upper-left of the ring
        // came up empty. Retail shows it there from the first frame.
        //
        // This button looks like "a green G (range) button". It is neither green-G nor a range
        // control: it is the **lock-UI padlock**, element `0x10000619`, and its click
        // handler (the element-message handler, message `0x19`) flips
        // the lock-UI flag and broadcasts global `0x0D`. The radar's range is not a button
        // at all — the radar's per-frame step reads it from the player system's radar radius
        // every 25 ms, which answers 75 outdoors and 25 indoors with nothing for the player to
        // press. See [`crate::mapradar::radar::radar_range`].
        self.sync_lock_button(ui, radar);
    }

    /// Finish radar initialization or a lock-status update: give
    /// the lock-UI button the media of the lock state the player module is actually in.
    ///
    /// It is a **state change**, not an attribute write: the two ids are keys in the button's own
    /// state table, each holding the `MediaDesc` for one padlock image, and the element's
    /// default state is 0 with no base media — so a button that is never given a state has no
    /// picture. See [`crate::mapradar::radar::lock_state`].
    fn sync_lock_button(&self, ui: &mut UiSystem, radar: ElemHandle) {
        let Some(h) = ui.get_child_recursive(radar, crate::mapradar::radar::child::LOCK_BUTTON)
        else {
            return;
        };
        let state = if self.locked {
            crate::mapradar::radar::lock_state::LOCKED
        } else {
            crate::mapradar::radar::lock_state::UNLOCKED
        };
        ui.set_state(h, dereth_ui::StateId(state));
    }

    fn radar_token(&self, which: crate::mapradar::radar::Compass) -> Option<ElemHandle> {
        use crate::mapradar::radar::Compass;
        match which {
            Compass::North => self.radar.north,
            Compass::South => self.radar.south,
            Compass::East => self.radar.east,
            Compass::West => self.radar.west,
        }
    }

    /// The shared player-state update for all ten floating windows, run once per window.
    ///
    /// Three separate rules, and mixing them up is how a HUD ends up with both vitals bars or with
    /// four empty chat windows down the left edge:
    ///
    /// 1. **Visibility** comes from the per-window chat option `0x1000008A` and is read
    ///    *unconditionally* — a local screen-layout file does not suppress it — but only by the
    ///    five classes in [`READS_PLACEMENT_VISIBILITY`].
    /// 2. **Position and size** come from `0x10000086`…`0x10000089` and are read **only when
    ///    the layout did not come from file**: a local layout file wins.
    /// 3. **The two vitals windows** ignore all of that and follow
    ///    the side-by-side option: the stacked window is visible when it is off and the
    ///    side-by-side window when it is on. The gameplay screen's option-change notice is
    ///    the same statement on the option-changed edge.
    ///
    /// Returns how many windows the placement blob actually showed or hid.
    pub fn update_from_player_module(
        &mut self,
        ui: &mut UiSystem,
        pm: &PlayerSettingsView,
    ) -> usize {
        let Some(root) = self.root() else { return 0 };
        let mut applied = 0;
        for w in GAMEPLAY_WINDOWS {
            let Some(h) = ui.get_child_recursive(root, w.element) else {
                continue;
            };
            let window_id = self.window_id_of(w.element);
            if window_id == 0 {
                continue;
            }
            let Some(row) = pm.placements.get(window_id).cloned() else {
                continue;
            };
            let is_toolbar = w.element == window::TOOLBAR;
            if !self.layout_from_file && is_toolbar {
                // The floaty toolbar's update from the player module: unlike the local file
                // loader, call move_to FIRST, resize_to SECOND,
                // and visibility change last. Its move_to clamp therefore uses the OLD size.
                if let (Some(x), Some(y)) = (row.x, row.y) {
                    ui.move_to(h, x, y);
                }
                if let (Some(cw), Some(ch)) = (row.w, row.h) {
                    ui.resize_to(h, cw, ch);
                }
            }
            if READS_PLACEMENT_VISIBILITY.contains(&w.class) {
                if let Some(v) = row.visible {
                    ui.set_visible(h, v);
                    applied += 1;
                }
            }
            if !self.layout_from_file && !is_toolbar {
                // Existing restoration for other classes. Their per-class ordering is not
                // implemented by the toolbar's bounded correction; of their virtual geometry
                // overrides, exactly one is implemented, and it is the one below.
                if let (Some(cw), Some(ch)) = (row.w, row.h) {
                    // This is the call site that reaches the restore, which is
                    // *not* a plain `resize_to`:
                    // the floaty chat panel's update from the player module reads the width
                    // (`Option_Placement_Width`) and height (`Option_Placement_Height`) and calls
                    // the element's own resize, so a class that overrides the resize gets its
                    // override.
                    //
                    // `MainChat`'s override is the main chat panel's resize, whose whole body is
                    // the at-the-end guard — `is_at_vertical_end` on the log before, scroll to the
                    // last glyph after — and the floaty main chat window's
                    // resize opens by calling it. The generic floaty chat window's resize does
                    // **not** guard (it persists the new size instead), so the four floaty chat
                    // windows keep the bare resize: that is parity, not an omission.
                    //
                    // Without this, a player whose module carries a chat-window size — anyone who
                    // has ever dragged or maximised the window — comes back into the world with
                    // the log's view height changed under a scroll offset that was the end at the
                    // *old* height. `is_at_vertical_end` is false from the first line afterwards and
                    // the main chat window shows the welcome and nothing else for the rest of the
                    // session, while `MessageLogPanel`'s `0x1A` strip, which has no scroll, keeps
                    // working. `p1_55_chat_log.rs` is the measurement.
                    if w.element == window::MAIN_CHAT {
                        let chat_win = self.chat_windows.first().copied().unwrap_or_default();
                        chat_win.resize_to(ui, h, cw, ch);
                    } else {
                        ui.resize_to(h, cw, ch);
                    }
                }
                if let (Some(x), Some(y)) = (row.x, row.y) {
                    ui.move_to(h, x, y);
                }
            }
            // The client reads the typed title after
            // visibility and dispatches `set_window_title`, including its local write-back.
            if let Some(title) = row.title.as_ref() {
                if let Some(i) = self
                    .floaty_chat
                    .iter()
                    .position(|chat| chat.window_id == window_id)
                {
                    let mut chat = std::mem::take(&mut self.floaty_chat[i]);
                    chat.set_window_title_info(ui, title.clone());
                    self.floaty_chat[i] = chat;
                }
            }
        }
        // The floaty chat panel's update from the player module's opacity third,
        // which reaches the default- and active-opacity writes and from
        // there the fade. Both sliders are in the Chat Options page's **general** section, so
        // they are not per-window and every window takes them.
        applied += usize::from(
            self.chat_set_stored_opacity(ui, pm.chat_default_opacity, pm.chat_active_opacity) > 0,
        );
        // The chat interface's update from the player module, the **filter**
        // third of the same function. Per window, unlike the opacity pair: the chat option read is
        // indexed by the window id.
        applied += usize::from(self.chat_set_stored_filters(ui, &pm.chat_filters) > 0);
        // Rule 3.
        let stacked = ui.get_child_recursive(root, window::STACKED_VITALS);
        let side = ui.get_child_recursive(root, window::SIDE_VITALS);
        crate::hud::vitals::apply_side_by_side_option(ui, stacked, side, pm.side_by_side_vitals);
        self.cascade_lock(ui, pm.lock_ui);
        applied
    }

    /// The window id for one of the sixteen windows, or 0 when the layout carries no
    /// `0x1000007E` — which is the chat-option lookup's own "no window id, no blob" case.
    #[must_use]
    pub fn window_id_of(&self, element: ElementId) -> u32 {
        self.window_ids
            .iter()
            .find(|(e, _)| *e == element)
            .map_or(0, |(_, id)| *id)
    }

    /// The vitals panel's update, for both vitals windows.
    ///
    /// "`Update` is driven by the quality-changed notice … and by
    /// the player-description-received notice, **not by a timer**." This is called every frame
    /// by the host and guards itself on the value having changed, which is the same observable
    /// behaviour from a host with no notice bus. Returns true when anything was written.
    pub fn update_vitals(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        let Some(player) = view.player() else {
            return false;
        };
        let mut wrote = false;
        for (i, v) in Vital::ALL.into_iter().enumerate() {
            let Some((cur, max)) = view.vital(player, v) else {
                continue;
            };
            if self.last_vitals[i] == Some((cur, max)) {
                continue;
            }
            self.last_vitals[i] = Some((cur, max));
            wrote = true;
            let (meter, label) = crate::hud::vitals::fields(v);
            let level = crate::hud::vitals::meter_level(cur, max);
            let text = crate::hud::vitals::label_text(cur, max);
            let pairs = [
                (
                    self.stacked_vitals.get(meter),
                    self.stacked_vitals.get(label),
                ),
                (self.side_vitals.get(meter), self.side_vitals.get(label)),
            ];
            for (m, l) in pairs {
                if let Some(m) = m {
                    set_attr_float(ui, m, attr::METER_LEVEL, level);
                }
                if let Some(l) = l {
                    if let Some(t) = ui.text_element_mut(l) {
                        t.set_text(&text);
                    }
                }
            }
        }
        wrote
    }

    /// The toolbar panel's selection changed handling — **the whole function**.
    ///
    /// # One client function, one method, one selection id
    ///
    /// The handler does two jobs: the selected object's name plus its two meters (the
    /// selected-object block), and the stack splitter. Both live here, so [`Self::last_selection`]
    /// is the only stored selection id in the workspace and the two halves cannot see different
    /// edges.
    ///
    /// # Where the health and mana queries go
    ///
    /// The client's `QueryHealth` and `QueryItemMana` for the selected id are
    /// **inside the not-a-stack arm** of the "stack size below 2" branch — the `None` early return
    /// of [`Self::apply_stack_split_gate`]'s `for_selection`, marked there in place. They are not
    /// sent from the read-out half and not from the edge block, and an implementation that puts
    /// them anywhere else does not match the client.
    ///
    /// # The two guards are different, and both are load-bearing
    ///
    /// The read-out half re-runs when [`Self::last_selection`] — an `Option<Option<..>>`, whose
    /// outer `None` is "this screen has never been asked" — differs from `Some(sel)`. The
    /// splitter half re-runs on the client's own "stored id differs from the current selection",
    /// which is
    /// `last_selection.flatten() != sel`: **on the first frame with nothing selected the read-out
    /// runs and the splitter does not**, because `Toolbar`'s constructor leaves its stored id at 0
    /// and the current selection is 0, so the client's edge test is false. That is not a nicety: a
    /// phantom first-frame edge hides the two elements the shipped layout brings up visible, which
    /// is a state retail never reaches.
    ///
    /// The splitter half also re-runs with **no** edge, on
    /// [`crate::toolbar::splitter::wants_reseed`] — the other caller,
    /// the item-attributes-changed notice. So the early return is on
    /// `!readout && !reseed`, not on the read-out guard alone.
    pub fn update_toolbar_selection(
        &mut self,
        ui: &mut UiSystem,
        view: &dyn GameView,
    ) -> ToolbarSelection {
        let sel = view.selection();

        // Look up the selected object's weenie and return if there is none —
        // `None` is *either* early return (nothing selected, or no weenie row) and `Some(n)` is a
        // live weenie whose stack size is `n`. The handler treats those two apart, so the seam
        // must too.
        //
        // `slot_decoration` is the accessor because it is the one that already carries the stack
        // size across this seam, and its documented `0 -> 1` mapping is **invariant for both
        // readers here**: `shows_split_widget` tests `>= 2` and `wants_reseed` tests `max(1, n) !=
        // max split size`, so 0 and 1 are already the same value to each. What matters is the
        // `Option`, and that is `weenie(id).is_some()` on both sides.
        let stack = sel
            .and_then(|id| view.slot_decoration(id))
            .map(|d| d.stack_size);

        // Stored id differs from the current selection — the client's edge, on the one copy.
        let edge = self.last_selection.flatten() != sel;
        let reseed = stack.is_some_and(|n| {
            crate::toolbar::splitter::wants_reseed(n, self.splitter.max_split_size)
        });
        // This screen's own "has never been asked" state, which the client does not have because
        // its member starts at 0 rather than at "unset".
        let readout = self.last_selection != Some(sel);
        if !readout && !reseed {
            return ToolbarSelection {
                wrote: false,
                split_gate_ran: false,
                rings: 0,
                query: None,
                cleared: false,
            };
        }
        // The selected-item notice's `(old, new)`, taken **before** the write.
        //
        // The client's selection notice carries both ids and every `ItemListWidget` is
        // registered for it, so one edge moves the ring in all twenty-two
        // live lists at once — the backpack grid, the two container strips, the paper doll and the
        // eighteen quickbar tiles. It rides *this* edge rather than one of its own because the
        // client keeps exactly one stored selection id.
        let previous = self.last_selection.flatten();
        self.last_selection = Some(sel);
        let mut rings = 0;
        if readout {
            for w in self.inventory.lists_mut() {
                rings += w.set_selected_item(ui, previous, sel);
            }
            rings += self.shortcuts.set_selected_item(ui, previous, sel);
        }

        if readout {
            if let Some(h) = self.toolbar_children.get("sel_object_field") {
                // The selection-changed handler calls `set_state(0)` here, not
                // `set_visible(false)`. State zero supplies the empty field's authored background.
                ui.set_state(h, dereth_ui::StateId(0));
            }
            if let Some(h) = self.toolbar_children.get("sel_object_name") {
                if let Some(t) = ui.text_element_mut(h) {
                    t.set_text("");
                }
            }
            if let Some(id) = sel {
                if let Some(h) = self.toolbar_children.get("sel_object_name") {
                    let name = view.name(id).unwrap_or_default().to_string();
                    if let Some(t) = ui.text_element_mut(h) {
                        t.set_text(&name);
                    }
                }
                // **No meter write here.** The selection-changed handler never writes either meter:
                // on the edge it *hides* them (below), and the only writers in the whole client are
                // the update-object-health and update-item-mana notices, which are
                // [`Self::update_selected_meters`]. Filling both meters here from
                // `GameView::vital(selected)` — the object table's qualities — would write **0.0**
                // on every selection of a remote creature, because the shard never sends those.
            }
        }

        // **The edge block's clear, which is the selection-changed handler's first act.** If the
        // health meter is visible, send `QueryHealth(0)` and hide it; if the mana meter is visible,
        // send `QueryItemMana(0)` and hide it.
        //
        // The id is **0** and that is the whole point: it tells the shard to stop sending updates
        // about the object just deselected. Both halves of each `if` are reproduced — the send
        // *and* the hide — and the visibility test that guards them is the client's own.
        //
        // **The hide is only safe because [`Self::update_selected_meters`] exists.** It is the
        // update-object-health / update-item-mana notice, the client's only re-show: hiding the
        // meters on the first edge without it would hide them for the rest of the session.
        //
        // The guard does real work here rather than being a formality: the shipped layout brings
        // both meters up **hidden**, so a first selection clears nothing and only a selection that
        // follows a reply sends anything at all.
        let mut cleared = false;
        if edge {
            for (field, request) in [
                (
                    "sel_object_health_meter",
                    UiRequest::QueryHealth(ObjectId(0)),
                ),
                (
                    "sel_object_mana_meter",
                    UiRequest::QueryItemMana(ObjectId(0)),
                ),
            ] {
                let Some(h) = self.toolbar_children.get(field) else {
                    continue;
                };
                if ui.node(h).is_some_and(|n| n.region.flags.visible) {
                    ui.requests.emit(request);
                    cleared = true;
                    ui.set_visible(h, false);
                }
            }
        }

        let split_gate_ran = edge || reseed;
        let query = if split_gate_ran {
            self.apply_stack_split_gate(ui, edge, stack, sel, view)
        } else {
            None
        };
        ToolbarSelection {
            wrote: readout,
            split_gate_ran,
            rings,
            query,
            cleared,
        }
    }

    /// The toolbar panel's update object health notice and
    /// The update item mana notice.
    ///
    /// Both handlers are the same three lines: compare the notice's object id with the toolbar's
    /// own selected id, show the meter if it is hidden, and
    /// `set_attribute_float(meter, 0x69, fraction)`. The id comparison is
    /// `update_object_health`'s, upstream of this seam — this crate is handed a
    /// fraction only when it is about the selected object.
    ///
    /// These are the **only** writers of the two meters in the client, which is why
    /// [`Self::update_toolbar_selection`]'s read-out half does not write them, and why hiding them
    /// on the selection edge is safe: this is the re-show.
    ///
    /// It is a poll rather than a notice for the reason every other update on this screen is:
    /// there is no notice registration here. Returns how many meters were written, so *"the reply
    /// arrived and nothing drew"* and *"no reply arrived"* are different numbers.
    pub fn update_selected_meters(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> usize {
        let (health, mana) = view.selected_meters();
        let mut wrote = 0;
        for (field, value) in [
            ("sel_object_health_meter", health),
            ("sel_object_mana_meter", mana),
        ] {
            let Some(level) = value else { continue };
            let Some(h) = self.toolbar_children.get(field) else {
                continue;
            };
            // Show the meter if it is hidden — the reply brings the meter up and
            // the selection edge is what took it down.
            ui.set_visible(h, true);
            set_attr_float(ui, h, attr::METER_LEVEL, level);
            wrote += 1;
        }
        wrote
    }

    /// The client's splitter block.
    ///
    /// # The gate is the stack size, read and not inferred
    ///
    /// The client hides the stack-size box and the slider, resets the split size and the maximum
    /// split size to `1 / 1`, and then shows the pair again **only** on a stack size of 2 or more —
    /// see [`crate::toolbar::splitter::shows_split_widget`] for the transcription and for why the
    /// weenie type, the maximum stack size and the bit field are all not it. A creature carries no
    /// stack size at all, so the value is 0 and a creature and a stack of one take the same arm.
    ///
    /// # The handler's shape, which is not one reset but two
    ///
    /// 1. **The edge** (the stored id differs from the current selection): store the new id, clear
    ///    the name, hide both meters, and hide the entry box and the slider — hidden, but the split
    ///    size is **not** reset here.
    /// 2. Nothing selected: stop. No weenie row for the selection: stop.
    /// 3. The name; then **the second reset**, split size = maximum split size = 1, and the entry
    ///    box and slider hidden again.
    /// 4. Stack size below 2: the not-a-stack arm. Otherwise seed the pair, write the box and the
    ///    slider, and show both.
    ///
    /// **The two early returns are load-bearing.** With nothing selected the client leaves the
    /// split size and the maximum split size exactly as they were; only the visibility comes down.
    /// Resetting the pair on every idle frame would destroy splitter state the player set up.
    ///
    /// `edge` is the caller's "stored id differs from the current selection"; `stack` carries the
    /// two early returns as `None` and a live weenie's stack size as `Some(n)`.
    ///
    /// # Why this is a poll and why the poll settles
    ///
    /// The selection-changed handler has two callers: the selection-changed notice and the
    /// item-attributes-changed notice, which re-runs it when and only when the changed object
    /// **is** the selection, has a weenie, and `max(1, stack size)` differs from the maximum split
    /// size. Both are reproduced by the caller: the selection edge is [`Self::last_selection`] and
    /// the second is [`crate::toolbar::splitter::wants_reseed`], applied only when there is a
    /// weenie to read — with a current selection of 0 no incoming object id can equal it, so that
    /// caller cannot fire at all. Together they make a per-frame call idempotent, **the important
    /// half being that a slider drag moves the split size and not the maximum split size**, so a
    /// player mid-drag is never re-seeded.
    fn apply_stack_split_gate(
        &mut self,
        ui: &mut UiSystem,
        edge: bool,
        stack: Option<u32>,
        sel: Option<ObjectId>,
        view: &dyn GameView,
    ) -> Option<SelectionQuery> {
        use crate::toolbar::splitter;

        let entry = self.toolbar_children.get("stack_size_entry_box");
        let slider = self.toolbar_children.get("stack_size_slider");
        let hide_or_show = |ui: &mut UiSystem, v: bool| {
            for h in [entry, slider].into_iter().flatten() {
                ui.set_visible(h, v);
            }
        };

        // The edge block: both hidden, and **the pair deliberately left alone**.
        if edge {
            hide_or_show(ui, false);
        }
        // The two early returns: nothing selected, and no weenie row.
        let stack_size = stack?;

        // The second reset, which the client reaches unconditionally once it has a weenie.
        //
        // **It is carried across the seam.** In the client there is **one** split-size /
        // maximum-split-size pair: this handler writes it and the drop handling reads *the same
        // words* (a whole stack is put in the container, anything less is split into it with the
        // split size). In this rebuild the pair
        // is two copies either side of a seam — this field, and `dereth_client::Interaction::split`,
        // which is what the drop path reads — and the stack-slider-changed notice is the only
        // carrier between them. The client does not raise a notice here **because it does not need
        // one**; omitting it here would leave the far end holding the *previous* selection's
        // quantity, so selecting a second stack and dragging it untouched would send
        // `StackableSplitToContainer(5)` where retail sends `PutItemInContainer`.
        //
        // A declared deviation in mechanism and an identity in observable: both of the client's
        // writes to the pair are mirrored, in the order it makes them (`1 / 1` here, the seed
        // below), so the far end ends the frame holding exactly what the globals hold. Selecting a
        // second stack is the case that separates it: a `SplitState::default()` of `{0, 0}` already
        // answers `is_whole_stack()`, so the **first** selection of a session looks right either
        // way.
        self.splitter = splitter::Splitter::default();
        ui.requests.emit(UiRequest::StackSliderChanged {
            split: self.splitter.split_size,
            max: self.splitter.max_split_size,
        });
        hide_or_show(ui, false);

        let Some(s) = splitter::Splitter::for_selection(stack_size) else {
            if let Some(h) = self.toolbar_children.get("sel_object_field") {
                // Both non-stack branches: state `0x1000000B`.
                ui.set_state(h, dereth_ui::StateId(0x1000_000B));
            }
            // **The not-a-stack arm.** The health and mana queries belong here and nowhere else;
            // the client's own behaviour, immediately after the "stack size below 2" test, is: for
            // an object that is not a player, has no pet owner and is not attackable, put the
            // selected-object field in state `0x1000000B`, send `QueryItemMana` if the player owns
            // it, and stop — **not** the health query. Everything else gets the same state and
            // `QueryHealth`.
            //
            // It is not "`QueryHealth` for anything `object_is_attackable` accepts": being a player
            // or having a pet owner reaches the health leg **without consulting
            // `object_is_attackable` at all**, and the mana leg is further gated on
            // `is_owned_by_player`, so an object that is none of those gets **neither** query. Read,
            // not inferred.
            let id = sel?;
            let Some(f) = view.selection_query_facts(id) else {
                return Some(SelectionQuery::NotAsked);
            };
            if !f.is_player && !f.has_pet_owner && !f.attackable {
                if f.owned_by_player {
                    ui.requests.emit(UiRequest::QueryItemMana(id));
                    return Some(SelectionQuery::ItemMana);
                }
                return Some(SelectionQuery::Neither);
            }
            ui.requests.emit(UiRequest::QueryHealth(id));
            return Some(SelectionQuery::Health);
        };
        if let Some(h) = self.toolbar_children.get("sel_object_field") {
            // Stack arm: state `0x1000000C`.
            ui.set_state(h, dereth_ui::StateId(0x1000_000C));
        }
        self.splitter = s;
        // The seed's half of the mirror above — split size = maximum split size = stack size,
        // carried across the seam so the drop path reads the same pair.
        ui.requests.emit(UiRequest::StackSliderChanged {
            split: s.split_size,
            max: s.max_split_size,
        });
        // Write the split size into the stack-size entry box.
        if let Some(h) = entry {
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(&s.split_size.to_string());
            }
        }
        // Set the slider's float attribute `0x86` to split size / maximum split size.
        //
        // A fresh seed writes 1.0, the whole-stack end. Retail's slider handler confirms the same
        // direction; retail does not invert this value.
        if let Some(h) = slider {
            set_attr_float(ui, h, attr::SLIDER_POSITION, s.slider_position());
        }
        hide_or_show(ui, true);
        // The stack arm asks for neither query: the client's `else` seeds the pair and falls
        // straight through to the common return. A stack is asked for **nothing**.
        None
    }

    /// The radar panel's coordinates update, the compass tokens update and the object draw.
    ///
    /// **The blips.** `draw_objects` writes them pixel by pixel into the radar's own surface, and a
    /// draw command that could only name a dat image could not carry them. `dereth_ui::UiDrawCmd`
    /// carries [`dereth_ui::UiFill`] — the fill-area primitive both the blips and the background
    /// erase are made of — so the shapes go straight onto the radar element's region.
    pub fn update_radar(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        use crate::mapradar::radar::{compass_token_position, update_coordinates, Compass};
        let mut wrote = false;

        // The radar's object draw, once per frame. The list is rebuilt every time rather than
        // diffed: the radar's per-frame step re-renders the whole blip field on its own 25 ms
        // gate and never keeps a per-object rectangle.
        //
        // **The whole regeneration is gated on there being a player** — the per-frame update does
        // its work only when the world view has a player, and it is that branch which ends by
        // marking the root dirty. With no player the radar is never marked dirty, so the child draw
        // never runs and neither the blips nor the centre cross are ever generated. Without this
        // gate the cross would appear in a scene the retail client would have left blank, which is
        // a state retail cannot reach but ours can (an offline `App` sits in `GamePlayScreen` with
        // an empty object stream).
        let live_radar = self
            .roots
            .first()
            .filter(|_| view.player().is_some())
            .and_then(|r| ui.get_child_recursive(*r, window::RADAR));
        if let Some(radar) = live_radar {
            let objects = view.radar_objects();
            // The object draw fetches the player's own weenie for the two PK-threat comparisons, so
            // this is **our** character and not merely the first
            // player in the list.
            let player = objects.iter().find(|o| o.is_self);
            let geom = crate::mapradar::radar::RadarGeometry {
                radius: self.radar.radius,
                center: self.radar.center,
            };
            let blips = crate::mapradar::radar::draw_objects(
                objects,
                player,
                geom,
                crate::mapradar::radar::radar_range(view.player_outside()),
                view.selection(),
                view.radar_blank(),
            );
            let mut fills: Vec<dereth_ui::UiFill> = blips
                .iter()
                .flat_map(crate::mapradar::radar::blip_fills)
                .collect();
            // **The player's own marker.** The object draw runs `draw_objects` and then lays the
            // green cross over the centre point, unconditionally and outside the blip loop — the
            // player is not in the radar's blip list, because the blip loop skips the player's own
            // id. So it is appended here, after the blips, in the same order the client issues its
            // fills.
            fills.extend(crate::mapradar::radar::center_marker_fills(geom));
            if let Some(n) = ui.node_mut(radar) {
                if n.region.surface_fills != fills {
                    n.region.surface_fills = fills;
                    wrote = true;
                }
            }
            let hit = crate::mapradar::radar::object_under_mouse(&blips, cursor_in(ui, radar))
                .and_then(|i| objects.get(i))
                .map(|o| o.id);
            let name = hit.and_then(|id| view.name(id)).map(ToString::to_string);
            wrote |= self.set_object_under_mouse(ui, radar, hit, name);
        }

        // The client's tail, which is the *other* half of the drag handle.
        // See [`RadarChildren::last_origin`] for why it is observed here rather than overridden.
        let any_radar = self
            .roots
            .first()
            .and_then(|r| ui.get_child_recursive(*r, window::RADAR));
        if let Some(b) = any_radar.and_then(|h| ui.node(h)).map(|n| n.region.box_) {
            let at = (b.x0, b.y0);
            let was = self.radar.last_origin.replace(at);
            if was.is_some() && was != Some(at) {
                self.persist_window_position(&mut ui.requests, at);
            }
        }

        // The coordinate read-out has **two** gates: the `CoordinatesOnRadar` option (option-word
        // bit 22) and
        // `player_coords` (a body in a cell). If either fails the text writes are skipped. The
        // tail runs on every update either way: it sets the four coordinate elements' visibility
        // to whether both gates passed, then shows one more element (the radar's lock button)
        // unconditionally.
        //
        // So the four coordinate elements are **down**, not blank, when either gate fails — which
        // is what keeps an empty strip off the screen between entering the world and the body
        // existing, and what a player who unticks *Display Coordinates on Radar* expects to see.
        //
        // `HudView::player_option` reads the live option word. The option is **default-on** (the
        // default option value answers true for ordinal 20), so the shipped profile shows the
        // strip.
        //
        // Before a `0x0013` lands, `HudView::player_option` must answer the *constructed* default
        // `true`, as retail does (its player module is a member, not a pointer): this gate is a
        // consumer of one of the sixteen default-on options, and answering `false` there would take
        // the strip down for a body in the world with no `0x0013`. `HudView::player_option` reads
        // the option word directly, which is what the check-box option control's value read does;
        // see its note.
        let coords = view
            .player_coords()
            .filter(|_| view.player_option(crate::view::PlayerOption::CoordinatesOnRadar));
        let shown = coords.is_some();
        for h in [
            self.radar.coordinate_container,
            self.radar.combined_coords,
            self.radar.x_coord,
            self.radar.y_coord,
        ]
        .into_iter()
        .flatten()
        {
            if ui.node(h).is_some_and(|n| n.region.flags.visible) != shown {
                ui.set_visible(h, shown);
                wrote = true;
            }
        }
        if let Some(coords) = coords {
            if self.last_coords != Some(coords) {
                self.last_coords = Some(coords);
                wrote = true;
                let c = update_coordinates(coords);
                let fields = [
                    (self.radar.combined_coords, c.combined),
                    (self.radar.y_coord, c.y_field),
                    (self.radar.x_coord, c.x_field),
                ];
                for (h, text) in fields {
                    let Some(h) = h else { continue };
                    if let Some(t) = ui.text_element_mut(h) {
                        t.set_text(&text);
                    }
                }
            }
        }

        let heading = view.player_heading();
        if self.last_heading != Some(heading) {
            self.last_heading = Some(heading);
            wrote = true;
            let center = self.radar.center;
            for (i, which) in Compass::ALL.into_iter().enumerate() {
                let Some(h) = self.radar_token(which) else {
                    continue;
                };
                let (x, y) =
                    compass_token_position(center, heading, self.radar.magnitudes[i], which);
                // `MoveTo` centres the token on the orbit point: the client subtracts half the
                // token's own width and height before moving it.
                let b = element_box(ui, h);
                ui.move_to(
                    h,
                    dereth_primitives::num::to_i32(x) - b.width() / 2,
                    dereth_primitives::num::to_i32(y) - b.height() / 2,
                );
            }
        }
        wrote
    }

    /// The rest of the client's loop: **which blip the pointer is on**, the tooltip
    /// that names it, and the mouse visibility the tooltip carries with it.
    ///
    /// At the top of the blip drawing the object under the mouse is cleared. For each drawn blip
    /// whose squared distance to the pointer is under `0x25` and the best so far, that object
    /// becomes the best, and its plain name (form 2) is set as the tooltip with tooltips on. A
    /// best id, if any, becomes the object under the mouse; if there is none, tooltips go off and
    /// the tooltip is cleared.
    ///
    /// The hit test is [`crate::mapradar::radar::object_under_mouse`]; this is its caller, and the
    /// two lines around it are what make it reachable. The element base's tooltip write re-runs
    /// the "should be mouse visible" test, so the tooltip is also what makes the radar body
    /// hit-testable at all — the element carries no mouse-visible attribute in the shipped layout,
    /// so with no blip under the cursor a click passes straight through it to the game view. That
    /// is the load-bearing part: without the tooltip the select arm can never fire, however
    /// correct the hit test is.
    ///
    /// The tooltip flag is `set_tooltip_on`, property `0x4B` — the flag the hover code
    /// reads before it will start a hover — so it is set here rather than assumed.
    fn set_object_under_mouse(
        &mut self,
        ui: &mut UiSystem,
        radar: ElemHandle,
        hit: Option<dereth_primitives::ObjectId>,
        name: Option<String>,
    ) -> bool {
        let was = std::mem::replace(&mut self.radar.object_under_mouse, hit);
        if was == hit {
            return false;
        }
        let on = hit.is_some();
        if let Some(n) = ui.node_mut(radar) {
            n.region.flags.tooltip = on;
        }
        if on {
            // The object's name in form 2 — the plain name, which is what
            // `GameView::name` answers.
            ui.set_tooltip(radar, Some(name.unwrap_or_default()));
        } else {
            ui.clear_tooltip(radar);
        }
        true
    }

    /// The radar move operation's tail, preserving the operation order:
    ///
    /// after the parent-box clamp, move the window; then, on the player system's player module,
    /// write property `0x10000086` (X) and then `0x10000087` (Y) as chat-window options under the
    /// radar's window id.
    ///
    /// `0x10000086`/`0x10000087` are the X and Y of one `Option_Placement` row — the same two
    /// `dereth_client::hud::decode_placements` reads back and [`Self::update_from_player_module`]
    /// re-applies. So a dragged radar survives a relog because the drag wrote the row the login
    /// blob carries.
    ///
    /// **Nothing is sent from here.** The per-window option write changes the *local*
    /// `PlayerModule`; the
    /// blob reaches the server with the rest of the options, not once per drag.
    fn persist_window_position(
        &self,
        requests_out: &mut crate::requests::Outbox,
        (x, y): (i32, i32),
    ) {
        for (property, value) in [(placement::X, x), (placement::Y, y)] {
            requests_out.emit(UiRequest::SetChatWindowOption {
                window: self.radar.window_id,
                property,
                value,
            });
        }
    }

    /// The lock-UI setter's effect on this screen, pushed in by the host.
    ///
    /// The client re-reads the lock-UI flag from the player system singleton every time global
    /// `0x0D` arrives. This crate has no singleton and
    /// a screen has no `GameView` at message-delivery time, so the host writes the flag here and
    /// *then* broadcasts `0x0D`, and [`Self::on_global_message`]'s existing arm cascades it —
    /// which keeps the broadcast, not the write, as the thing that repaints the HUD.
    pub fn set_lock_ui(&mut self, locked: bool) {
        self.locked = locked;
    }

    /// The final display-string notice, fanned out to the five
    /// chat windows, and the surviving text written into each window's chat log.
    ///
    /// The client sends a line to "every `ChatInterface` whose filter accepts the type, or whose
    /// window id matches". The routing already exists; what this adds
    /// is writing the resulting log into the element, because a `TextElement` is where the
    /// player actually reads it.
    ///
    /// Returns the window ids that took the line.
    pub fn recv_display_final_string_info(
        &mut self,
        ui: &mut UiSystem,
        m: &ChatMessage,
    ) -> Vec<u32> {
        let mut took = Vec::new();
        for i in 0..self.chat.len() {
            // The append, the colours, `is_at_vertical_end` and the scroll-to-end all
            // live in [`crate::chat::window::ChatWindow`], because they are things done to the
            // *element*: rewriting the whole log with `set_text` would draw every glyph in the
            // element's own `font_color` (white) and leave the view pinned to the top.
            let Some(w) = self.chat_windows.get(i).copied() else {
                continue;
            };
            if w.recv_display_final_string_info(ui, &mut self.chat[i], m) != Routed::Accepted {
                continue;
            }
            took.push(self.chat[i].window_id);
        }
        took
    }

    /// The set-panel-visibility notice, fanned out to the three page groups that listen for it, in
    /// set-up registration order.
    ///
    /// All three ignore a panel id they do not own, so the notice is simply offered to each.
    ///
    /// `Toolbar` is a fourth listener on the same notice, and the *only* thing
    /// that ever puts a toolbar panel button out — see
    /// [`crate::toolbar::Toolbar::on_set_panel_visibility`]. It is offered the notice here
    /// and, separately, each notice `PanelStack` re-sends for the page it just covered, because in
    /// the client those are real panel-visibility notices sent from inside
    /// the panel-visibility notice handler and they reach every registered handler, not only the
    /// server-side record. Without the second call the button of the *covered* page stays lit.
    pub fn recv_set_panel_visibility(&mut self, ui: &mut UiSystem, panel: u32, show: bool) {
        // A page of a system the world lacks (the journal's quest page, the trade window, ...)
        // never shows, whichever key, button or server notice asks.
        if show && self.era_refuses(panel) {
            return;
        }
        let out = [
            self.panels.recv_set_panel_visibility(ui, panel, show),
            self.env_panel.recv_env_panel_visibility(ui, panel, show),
            self.combat_panel.recv_set_panel_visibility(ui, panel, show),
        ];
        self.toolbar.on_set_panel_visibility(ui, panel, show);
        for group in out {
            for r in group {
                if let UiRequest::SetPanelVisibility { panel, visible } = r {
                    self.toolbar.on_set_panel_visibility(ui, panel, visible);
                }
                ui.requests.emit(r);
            }
        }
    }

    /// Whether a page of the world's systems is refused: its panel id names a page, of the toolbar
    /// stack or the environment stack, whose system the world lacks ([`era_lacks_page`]).
    fn era_refuses(&self, panel: u32) -> bool {
        let Some(features) = self.era_features else {
            return false;
        };
        panel != 0
            && self
                .panels
                .pages
                .iter()
                .chain(self.env_panel.pages.iter())
                .any(|p| p.panel_id == panel && era_lacks_page(features, p.element))
    }

    /// Take off the screen the pages of the systems the world lacks ([`era_lacks_page`]): each
    /// open one is closed, its toolbar button hidden, and [`Self::recv_set_panel_visibility`]
    /// refuses to show it. The rest of what an era lacks is the panels' ([`crate::panels::era`]).
    /// Applied once each time the features change; returns whether anything was applied.
    pub fn apply_era(
        &mut self,
        ui: &mut UiSystem,
        features: dereth_primitives::EraFeatures,
    ) -> bool {
        if self.era_features == Some(features) {
            return false;
        }
        let pages: Vec<(u32, bool)> = self
            .panels
            .pages
            .iter()
            .chain(self.env_panel.pages.iter())
            .filter(|p| p.panel_id != 0)
            .map(|p| (p.panel_id, !era_lacks_page(features, p.element)))
            .collect();
        for (panel, has) in pages {
            if !has {
                self.recv_set_panel_visibility(ui, panel, false);
            }
            for b in self.toolbar.buttons.iter().filter(|b| b.panel_id == panel) {
                ui.set_visible(b.handle, has);
                ui.set_mouse_visible(b.handle, has);
            }
        }
        // The buttons left close up over the hidden ones.
        self.toolbar.arrange_buttons(ui);
        self.era_features = Some(features);
        true
    }

    /// The client's three calls, on the page the
    /// click actually belongs to; see [`is_under_config_page`].
    ///
    /// Returns the option page's own answer — how many controls were written — so a test can say
    /// *"Cancel reverted three"* rather than *"Cancel did not crash"*.
    pub fn on_config_page_button(
        &mut self,
        ui: &mut UiSystem,
        source: dereth_ui::ElemHandle,
        source_id: ElementId,
    ) -> usize {
        use crate::options::config::button;
        if !matches!(source_id, button::APPLY | button::CANCEL | button::DEFAULTS) {
            return 0;
        }
        // The three ids are shared by all three option pages, so the click is attributed by
        // walking up to `0x10000213`; that is what makes this arm `ClientOptionsPanel`'s and not
        // `ChatOptionsPanel`'s.
        if !is_under_config_page(ui, source) {
            return 0;
        }
        match source_id {
            // The option page base's save of the current values — a snapshot, not a write.
            button::APPLY => {
                self.config_page.save_current_values();
                0
            }
            button::CANCEL => self.config_page.restore_saved_values(ui),
            _ => self.config_page.restore_default_values(ui),
        }
    }

    /// One press on the Options *Game / Support* page, performed.
    ///
    /// The client does all five of these itself; in
    /// this build the effects live on the screen and on the request queue, so the page returns the
    /// action ([`crate::options::gameplay::GameplayOptionsPage::on_element_message`]) and this
    /// performs it. Returns how many controls the press wrote, which is zero for everything but
    /// *Restore Defaults* — a number a station can assert on rather than "it did not panic".
    ///
    /// | action | here |
    /// |---|---|
    /// | `EndCharacterSession { ask }` | [`Self::on_end_character_session`], the asking form |
    /// | `OpenUrl` | [`crate::requests`] — this crate may not call `ShellExecuteA` |
    /// | `BroadcastGlobal { id: 1, param }` | [`Self::handle_key_press`], because the only listener in this build that answers global 1 with `0x10000027` is that arm, so it is called rather than the bus re-entered from inside a dispatch |
    /// | `BroadcastGlobal { id: 0x0C, .. }` | *Restore Defaults* on all three `PlayerOptionPage`s |
    ///
    /// **Global `0x0C` is `RESTORE_DEFAULTS`, not a refresh**, though `0x100005CC` reads like
    /// *"refresh the options panels"*; `dereth_ui::msg::global::RESTORE_DEFAULTS` is `0x0C` and
    /// every option page in the client is registered for it, so the button restores defaults
    /// everywhere. The reach here is the three option pages — exactly what each page's own
    /// *Defaults* button reaches. The fourth retail listener, the action-key-map control's
    /// global-message handler, needs the host's `InputManager` and is a known gap rather than faked
    /// here.
    pub fn on_gameplay_options_action(
        &mut self,
        ui: &mut UiSystem,
        a: &crate::options::pages::GameplayOptionAction,
    ) -> usize {
        use crate::options::pages::GameplayOptionAction as A;
        match a {
            A::Request(UiRequest::EndCharacterSession { ask }) => {
                // The end-character-session notice, argument 1 — sender and receiver are both this
                // screen's, so the notice is delivered rather than queued.
                self.on_end_character_session(ui, i32::from(*ask));
                0
            }
            A::Request(r) => {
                ui.requests.emit(r.clone());
                0
            }
            A::BroadcastGlobal { id: 1, param } => {
                self.handle_key_press(ui, *param);
                0
            }
            A::BroadcastGlobal { id, .. }
                if MessageId(*id) == dereth_ui::msg::global::RESTORE_DEFAULTS =>
            {
                let mut n = self.config_page.restore_default_values(ui);
                n += self.character_options.restore_default_values(ui);
                let (c, effects) = self.chat_options.restore_default_values(ui);
                n += c;
                for e in effects {
                    self.chat_recv_notice_gameplay_option_changed(ui, e);
                }
                n
            }
            A::BroadcastGlobal { .. } => 0,
        }
    }

    /// The same three buttons, on the **Character Options** page.
    ///
    /// The character settings panel's element-message handler is one function and both pages
    /// use it — the config panel's construction and the character settings panel's construction
    /// both build a `PlayerOptionPage`, and the handler is the one they share — so the
    /// three element ids `0x100001FC`/`FD`/`FE` mean the same three calls here as they do there.
    /// What tells the two apart is only **which page the click sits under**, which is exactly what
    /// [`is_under_config_page`] settles for the other one.
    ///
    /// Without this arm Apply, Cancel and *Restore Defaults* on the Character page do nothing at
    /// all: the buttons are in the shipped tree and raise message 1, but nothing else is keyed on
    /// this page.
    ///
    /// The page is matched by **handle** rather than by id, because `0x10000211` appears twice in
    /// the shipped tree — see [`crate::options::character::find_page`], which is what bound this
    /// one.
    ///
    /// Returns the page's own answer — how many rows were written — so a test can say *"Cancel
    /// reverted three"* rather than *"Cancel did not crash"*. Defaults answers **0** on a host
    /// with no `GameView::player_option_default`; see
    /// [`crate::options::character::CharacterSettingsPage::restore_default_values`].
    pub fn on_character_page_button(
        &mut self,
        ui: &mut UiSystem,
        view: &dyn GameView,
        source_id: ElementId,
    ) -> usize {
        use crate::options::config::button;
        match source_id {
            // Re-read every row from the module and
            // re-snapshot the saved values, which is what gives Cancel something to revert to. It
            // writes no option, exactly as on the other page.
            button::APPLY => self.character_options.save_current_values(ui, view),
            button::CANCEL => self.character_options.restore_saved_values(ui),
            button::DEFAULTS => self.character_options.restore_default_values(ui),
            _ => 0,
        }
    }

    ///  on the Client Options page.
    ///
    /// **This is the part a rebuild is most likely to skip**, and it is the one the row named:
    /// closing the page without pressing Apply calls `restore_saved_values` and rolls every
    /// uncommitted change back. Returns how many controls were reverted.
    pub fn config_page_visibility_changed(&mut self, ui: &mut UiSystem, visible: bool) -> usize {
        self.config_page.on_visibility_changed(ui, visible)
    }

    // ---- the Character Options page ---------------------------------------------------------

    /// The option page's visibility handling on the Character Options page —
    /// `save_current_values` on show, `restore_saved_values` on hide.
    ///
    /// Returns how many rows moved, which is the observable half: a show that re-read 50 identical
    /// values and one that read nothing at all are otherwise the same.
    ///
    /// **Known defect, pinned by a test.** The *first* show of a `Panel` page never delivers its
    /// `0x18`, so this show arm does not run on a first visit. The fix belongs in
    /// `dereth_ui::widgets`.
    pub fn character_options_visibility_changed(
        &mut self,
        ui: &mut UiSystem,
        view: &dyn GameView,
        visible: bool,
    ) -> usize {
        self.character_options
            .on_visibility_changed(ui, view, visible)
    }

    /// Re-read every Character Options check box from the `PlayerModule`.
    ///
    /// This is `save_current_values` without the visibility edge, for the host to call when the
    /// module it is a view of changes — `0x0013`'s `PlayerModule` arriving, or a `0x01A1` going
    /// out. Returns how many rows moved.
    pub fn update_character_options(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> usize {
        self.character_options.save_current_values(ui, view)
    }

    /// Take the queued visibility edges without applying them — for a test that wants to observe
    /// **which** edges reached the screen rather than what they did.
    pub fn take_character_option_visibility(&mut self) -> Vec<bool> {
        std::mem::take(&mut self.character_option_visibility)
    }

    /// Drain the Character Options page's queued visibility edges against the host's view.
    ///
    /// Called once per frame, beside [`Self::drive_key_bindings`] and `crate::requests::take`.
    /// Returns how many rows each edge moved, in the order the edges arrived.
    pub fn drive_character_options(
        &mut self,
        ui: &mut UiSystem,
        view: &dyn GameView,
    ) -> Vec<usize> {
        // Apply / Cancel / Restore Defaults, drained ahead of the visibility edges
        // because a click always precedes the close that would follow it.
        let buttons: Vec<usize> = std::mem::take(&mut self.character_option_buttons)
            .into_iter()
            .map(|id| self.on_character_page_button(ui, view, id))
            .collect();
        let mut out = buttons;
        out.extend(
            std::mem::take(&mut self.character_option_visibility)
                .into_iter()
                .map(|visible| {
                    // The character-option page's save-current-values method is an
                    // *override*: it calls `save_to_server(module, false)` and only then the base
                    // save-current-values method. The page's visibility handler reaches it on
                    // the show edge, so this is where a page that comes up with un-flushed option
                    // changes behind it sends its `0x01A1`. The host owns the module and the wire;
                    // the page raises the request, in the client's order — save first, re-read second.
                    if visible {
                        ui.requests.emit(UiRequest::SavePlayerOptions);
                    }
                    self.character_options
                        .on_visibility_changed(ui, view, visible)
                })
                .collect::<Vec<usize>>(),
        );
        out
    }

    // ---- the Chat Options page --------------------------------------------------------------

    /// The gameplay-option-changed notice `(prop, windowId)`, delivered to the chat windows.
    ///
    /// The player module's property-changed hook raises it as its **first line** on every
    /// per-window chat option write and every option write, so the sender is the module and the
    /// receivers are the five windows. Sender and receiver are both inside this process and the
    /// module is the host's, so the page hands its effect back and this delivers it — the same
    /// shape [`Self::on_end_character_session`] uses for a notice whose sender and
    /// receiver are both this screen.
    ///
    /// The two arms are **not** symmetrical, and that is measured:
    ///
    /// * The chat interface's gameplay option changed notice tests that the property is
    ///   `0x1000007F` **and** that the window id is its own, so a filter reaches exactly one
    ///   window;
    /// * The floaty main chat panel's gameplay option changed notice intercepts
    ///   `0x10000080` / `0x10000081` **before** chaining and applies them with **no** window-id
    ///   test, so one pair of sliders moves all five.
    ///
    /// Returns how many windows took it.
    pub fn chat_recv_notice_gameplay_option_changed(
        &mut self,
        ui: &mut UiSystem,
        effect: crate::options::chat::ChatOptionEffect,
    ) -> usize {
        use crate::chat::interface::opacity_attr;
        use crate::options::chat::ChatOptionEffect as E;
        match effect {
            E::Opacity { property, value } => {
                let (d, a) = match property {
                    opacity_attr::DEFAULT => (Some(value), None),
                    opacity_attr::ACTIVE => (None, Some(value)),
                    _ => return 0,
                };
                self.chat_set_stored_opacity(ui, d, a)
            }
            E::Filter { window_id, mask } => self.chat_set_stored_filter(ui, window_id, mask),
        }
    }

    /// The chat interface's font-settings-changed notice is offered to every chat
    /// window.
    ///
    /// The producer is the font-preference change callback,
    /// registered during preference initialization on **both** `UI.ChatFontFace` and
    /// `UI.ChatFontSize`; it reads the two statics it was registered against and broadcasts them.
    /// Here the statics are [`crate::options::store`]'s registry, which is where the Client
    /// Options page's Apply writes, and the callback is
    /// [`crate::options::store::font_preference_epoch`] — see [`Self::drive_chat_font`].
    ///
    /// The face and size are `UInt` preferences, so the value is the **row index** into
    /// the chat font face and chat font size choice lists; `Chat_<Face>_<Size>` is then a name
    /// in the font enum-id map and that name's enum maps to the font's `DataID`. The shipped
    /// `client_local_English.dat` carries all 25 of them, e.g. `Chat_PalatinoLinotype_Small` ->
    /// `0x40000000` (which is what the layout already gives the log, so the *defaults* agree and
    /// only a change is observable) and `Chat_Tahoma_XL` -> `0x4000000C`.
    ///
    /// Returns how many windows re-resolved their font. `0` is a miss at any hop and is the
    /// client's behaviour too: every failure arm of the client leaves the log's font alone.
    pub fn chat_recv_notice_font_settings_changed(&mut self, ui: &mut UiSystem) -> usize {
        use crate::view::PrefValue;
        let index = |name: &str| match crate::options::store::inq_value(name) {
            Some(PrefValue::Int(v)) => usize::try_from(v).ok(),
            _ => None,
        };
        let face = index(dereth_ui::persist::preferences::keys::CHAT_FONT_FACE);
        let size = index(dereth_ui::persist::preferences::keys::CHAT_FONT_SIZE);
        let (Some(face), Some(size)) = (face, size) else {
            return 0;
        };
        let Some(did) = crate::chat::interface::resolve_chat_font(ui, face, size) else {
            return 0;
        };
        // The font fetch. A font this build cannot measure is a miss, not a silent
        // swap to nothing.
        let Some(metrics) = ui.font_metrics(did) else {
            return 0;
        };
        let mut n = 0;
        for w in self.chat_windows.clone() {
            n += usize::from(w.set_chat_font(ui, did, std::sync::Arc::clone(&metrics)));
        }
        n
    }

    /// The client's callback edge, drained once a frame.
    ///
    /// Retail's callback is synchronous, inside the store's own write. This crate's store cannot
    /// call back into a screen, so [`crate::options::store::font_preference_epoch`] counts the
    /// writes and this compares it with the one already answered — the same edge, and it catches
    /// the live menu choice, *Cancel* and *Restore Defaults* alike because all three go through
    /// `set_value`. Returns how many windows moved.
    pub fn drive_chat_font(&mut self, ui: &mut UiSystem) -> usize {
        let now = crate::options::store::font_preference_epoch();
        if now == self.font_preference_epoch {
            return 0;
        }
        self.font_preference_epoch = now;
        self.chat_recv_notice_font_settings_changed(ui)
    }

    /// The chat interface's gameplay option changed notice's `0x1000007F` arm, offered to
    /// all five windows so the window-id compare is the thing that selects one.
    ///
    /// Returns how many windows took it — **one** for a real window id, and `0` for an id no
    /// window carries, which is the loud form of "the blob names a window this tree does not
    /// have".
    pub fn chat_set_stored_filter(
        &mut self,
        ui: &mut UiSystem,
        window_id: u32,
        mask: u64,
    ) -> usize {
        let _ = ui;
        let mut n = 0;
        for w in &mut self.chat {
            n += usize::from(w.on_gameplay_option_changed(
                crate::options::chat::CHAT_FILTER_PROPERTY,
                window_id,
                mask,
            ));
        }
        n
    }

    /// The chat interface's per-window filter read over all five windows — the **filter** third of
    /// the client's "filter, position, opacity".
    ///
    /// Returns how many windows moved.
    pub fn chat_set_stored_filters(&mut self, ui: &mut UiSystem, filters: &[(u32, u64)]) -> usize {
        let _ = ui;
        let mut n = 0;
        for w in &mut self.chat {
            let mask = filters
                .iter()
                .find(|(id, _)| *id == w.window_id)
                .map(|(_, m)| *m);
            n += usize::from(w.update_filter_from_player_module(mask));
        }
        n
    }

    /// The Chat Options page's Apply / Cancel / Restore Defaults, once the host's [`GameView`] is
    /// available.
    ///
    /// The character settings panel's element-message handler is the one handler all three
    /// `PlayerOptionPage` subclasses share, so `0x100001FC`/`FD`/`FE` mean the same three calls
    /// here as on the other two pages; what tells them apart is only which page the press sits
    /// under. Returns each press's answer — how many controls were written.
    pub fn on_chat_page_button(
        &mut self,
        ui: &mut UiSystem,
        view: &dyn GameView,
        source_id: ElementId,
    ) -> usize {
        use crate::options::config::button;
        let (n, effects) = match source_id {
            // The option page base's save of the current values — a re-read, not a write.
            button::APPLY => (self.chat_options.save_current_values(ui, view), Vec::new()),
            button::CANCEL => self.chat_options.restore_saved_values(ui),
            button::DEFAULTS => self.chat_options.restore_default_values(ui),
            _ => return 0,
        };
        for e in effects {
            self.chat_recv_notice_gameplay_option_changed(ui, e);
        }
        n
    }

    /// Re-read every Chat Options control from the `PlayerModule` — `save_current_values` without
    /// the visibility edge, for the host to call when the module it is a view of changes.
    pub fn update_chat_options(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> usize {
        self.chat_options.save_current_values(ui, view)
    }

    /// Drain the Chat Options page's queued button presses and visibility edges against the
    /// host's view, delivering every notice the page's writes raise.
    ///
    /// Called once per frame beside [`Self::drive_character_options`]. Returns what each drained
    /// event answered, buttons first — a click always precedes the close that would follow it.
    pub fn drive_chat_options(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> Vec<usize> {
        let mut out: Vec<usize> = std::mem::take(&mut self.chat_option_buttons)
            .into_iter()
            .map(|id| self.on_chat_page_button(ui, view, id))
            .collect();
        for visible in std::mem::take(&mut self.chat_option_visibility) {
            // The player-option page's save-current-values is an override: it calls
            // `save_to_server(module, false)` and only then
            // `save_current_values`, and the visibility-changed handler reaches it on
            // the **show** edge. Same order as the Character Options page — save first, re-read
            // second.
            if visible {
                ui.requests.emit(UiRequest::SavePlayerOptions);
            }
            let (n, effects) = self.chat_options.on_visibility_changed(ui, view, visible);
            for e in effects {
                self.chat_recv_notice_gameplay_option_changed(ui, e);
            }
            out.push(n);
        }
        out
    }

    /// Take the queued visibility edges without applying them — for a test that wants to observe
    /// **which** edges reached the screen rather than what they did.
    pub fn take_chat_option_visibility(&mut self) -> Vec<bool> {
        std::mem::take(&mut self.chat_option_visibility)
    }

    // ---- the key-binding page ---------------------------------------------------------------

    /// Build one row per user-bindable action.
    ///
    /// Separate from [`Self::post_init`] because every row reads the merged master input map,
    /// which lives in the host's `InputManager`. Returns how many rows were built; `0` with a
    /// bound page means the action map was empty, which is a loud answer rather than a silent one.
    pub fn key_bindings_init_options(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_input::InputManager,
    ) -> usize {
        self.key_bindings.init_options(ui, m)
    }

    /// A key press captured while a key-binding row is in capture —
    /// the input manager's key-hit handler call, whose one registered handler is the row that
    /// raised the map-warn dialog.
    ///
    /// Queued rather than dispatched, for the reason [`Self::key_binding_inbox`] gives. Returns
    /// whether any row is actually capturing, which is the client's own *"is a key-hit handler
    /// registered"* and is what stops every keystroke in the game being queued here.
    pub fn key_bindings_key_hit(&mut self, control: dereth_input::ControlChord) -> bool {
        if !self
            .key_bindings
            .rows
            .iter()
            .any(super::super::options::keybinding::ActionKeyMapRow::capturing)
        {
            return false;
        }
        self.key_binding_key_hits.push(control);
        true
    }

    /// Drain the queued key-binding work against the host's `InputManager`.
    ///
    /// Called once per frame, after `UiFlow::frame`, exactly where `crate::requests::take` is
    /// called and for the same reason. Returns `(row, event)` per element message taken and the
    /// verdict of each captured key, in dispatch order.
    pub fn drive_key_bindings(
        &mut self,
        ui: &mut UiSystem,
        m: &mut dereth_input::InputManager,
    ) -> (
        Vec<(usize, crate::options::keybinding::RowEvent)>,
        Vec<dereth_input::binding::Capture>,
    ) {
        let mut events = Vec::new();
        for msg in std::mem::take(&mut self.key_binding_inbox) {
            // The key-binding page handler is the page's own handler, and each
            // `ActionKeyMapOption` has the row handler;
            // they are two listeners on one broadcast, so both are offered every message and the
            // page is offered it first, which is the initialization registration order. A message
            // can only ever match one of them — the page's buttons are not
            // row children.
            if let Some(e) = self.key_bindings.on_page_element_message(ui, m, &msg) {
                self.key_binding_page_events.push(e);
                continue;
            }
            if let Some(e) = self.key_bindings.on_element_message(ui, m, &msg) {
                events.push(e);
            }
        }
        // The client's element half. The row above owns the
        // context; this screen owns the live root alongside its other gameplay elements.
        self.roots
            .extend(self.key_bindings.service_dialog_elements(ui));
        let mut verdicts = Vec::new();
        for control in std::mem::take(&mut self.key_binding_key_hits) {
            // Through the page rather than into the row, so that
            //  runs on the far side of
            // Which is the only thing that lights *Revert to Saved*.
            if let Some(v) = self.key_bindings.key_hit(ui, m, control) {
                verdicts.push(v);
            }
        }
        // A captured key closes and deletes its wait dialog before returning here, then may open a
        // conflict dialog. Give that new context its live element in the same frame, and deliver
        // physical Yes/No/Notice answers from dialogs that were already on screen.
        self.roots
            .extend(self.key_bindings.service_dialog_elements(ui));
        let _ = self.key_bindings.service_dialog_answers(ui, m);
        if let Some(event) = self.key_bindings.service_file_dialog_answer(ui) {
            self.key_binding_page_events.push(event);
        }
        self.roots.retain(|h| ui.node(*h).is_some());
        (events, verdicts)
    }

    /// The refresh-action-key-mapping notice: every row re-reads the map.
    pub fn key_bindings_refresh(&mut self, ui: &mut UiSystem, m: &dereth_input::InputManager) {
        self.key_bindings.on_refresh_action_key_mapping(ui, m);
    }

    /// Whether an element message is addressed to a key-binding row — a key button, a clear
    /// button, or the row element itself — **or** to one of the page's own six buttons.
    ///
    /// **The page half matters.** Queueing only row elements would mean a click on Apply, Cancel,
    /// *Restore Defaults* or *Revert to Saved* never reaches [`Self::drive_key_bindings`] at all:
    /// the four buttons are in the shipped tree and raise their messages.
    #[must_use]
    fn is_key_binding_element(&self, source: ElemHandle) -> bool {
        let p = &self.key_bindings;
        if [
            p.load_button,
            p.save_button,
            p.reset_defaults_button,
            p.revert_to_saved_button,
            p.ok_button,
            p.cancel_button,
        ]
        .contains(&Some(source))
        {
            return true;
        }
        p.rows.iter().any(|r| {
            r.element == Some(source)
                || r.clear_button == Some(source)
                || r.key_buttons.contains(&source)
        })
    }

    /// Take what the Key Bindings page's own buttons did this frame.
    pub fn take_key_binding_page_events(&mut self) -> Vec<crate::options::keybinding::PageEvent> {
        std::mem::take(&mut self.key_binding_page_events)
    }

    /// Whether the page carrying `panel_id` is the one currently shown, in any of the three groups
    /// Which is what a toolbar button's click needs in order to send the *opposite*.
    #[must_use]
    pub fn panel_is_visible(&self, panel_id: u32) -> bool {
        [&self.panels, &self.env_panel, &self.combat_panel]
            .into_iter()
            .any(|s| {
                s.current.is_some_and(|e| {
                    s.pages
                        .iter()
                        .any(|p| p.element == e && p.panel_id == panel_id)
                })
            })
    }

    /// The game play screen's end character session notice.
    ///
    /// The screen keeps four flags — [`Self::ending_session`], [`Self::do_end_session`],
    /// [`Self::should_quit_on_logout`] and [`Self::logout_confirmed`] — and the notice sets them
    /// as follows:
    ///
    /// * `param != 0`: end the session, do not quit, and raise the confirmation dialog with
    ///   `ID_Client_EndCharacterSessionConfirm`;
    /// * `param == 0`: confirmed, end the session, and quit.
    ///
    /// The two branches are *ask, then go to character select* and *quit now, without asking*; the
    /// button that sends `1` is named for the former. The asking branch reads like "quit on
    /// logout"; retail does otherwise, and this follows retail.
    pub fn on_end_character_session(&mut self, ui: &mut UiSystem, param: i32) {
        if param != 0 {
            self.do_end_session = true;
            self.should_quit_on_logout = false;
            self.make_logout_confirmation_dialog(ui, logout::END_SESSION_CONFIRM);
        } else {
            self.logout_confirmed = true;
            self.do_end_session = true;
            self.should_quit_on_logout = true;
        }
    }

    /// The log-off notice: the same dialog with `ID_Client_LogoffConfirm`, and quit-on-logout set,
    /// so *Yes* quits.
    pub fn on_logoff(&mut self, ui: &mut UiSystem) {
        self.do_end_session = true;
        self.should_quit_on_logout = true;
        self.make_logout_confirmation_dialog(ui, logout::LOGOFF_CONFIRM);
    }

    /// The game play screen's logout confirmation dialog build.
    ///
    /// Only when no dialog context exists: clear the confirmation, show the framework, and make
    /// the dialog (properties `0x8E`, `0xC3`, `0xC5`) — it refuses to raise a second one while the
    /// first is up, un-confirms, and forces the framework visible, so the player can never hide a
    /// modal dialog.
    ///
    /// **What is faithful and what is not.** The element, its layout, its two buttons, the
    /// sentence, the modality (blocking clicks and moving the dialog to the front), and every
    /// flag transition are the
    /// client's. This path bypasses the dialog factory:
    /// the factory lives on [`dereth_ui::framework::UiFlow`] and a [`Screen`] has no handle to it,
    /// so the queues, the contexts, the open-dialog notice and the "N waiting" banner are not
    /// exercised here. This screen raises exactly one dialog and never two, so nothing it does
    /// depends on a queue — but the five server-driven confirmations and the wait dialog do.
    pub fn make_logout_confirmation_dialog(&mut self, ui: &mut UiSystem, string_id: &str) -> bool {
        if self.logout_dialog.is_some() {
            return false;
        }
        self.logout_confirmed = false;
        // `Show(1)`.
        self.shown = true;
        if let Some(root) = self.root() {
            ui.set_visible(root, true);
        }
        let Ok(h) = ui.require_env().and_then(|e| {
            e.create_and_add_root_element(ui, logout::DIALOG_LAYOUT, logout::CONFIRMATION_ROOT)
        }) else {
            return false;
        };
        // Property 0xAC. `DialogElement::on_set_attribute` turns it into `block_clicks` plus
        // bring-to-front, which is what puts the dialog over the HUD and stops a click reaching
        // the world behind it.
        ui.set_attribute_bool(h, dereth_ui::props::attr::DIALOG_MODAL, true);
        // By **element id**, not by bubbling: the confirmation dialog itself listens for message 1
        // from these two children and returns stop-processing
        // (`dialog::types::DialogElement::listen_to_element_message`), so a listener registered on
        // the dialog's root would never be reached. The by-id table is consulted first, which is
        // the same route `DisconnectedScreen` takes to its OK button.
        ui.register_for_element_message(logout::BUTTON_YES, MessageId(1), ME);
        ui.register_for_element_message(logout::BUTTON_NO, MessageId(1), ME);
        // Recorded as one of this screen's roots, so use_new_mode deletes it with the
        // screen — which is `~` calling `Reset()`,
        // "every UI mode switch clears all dialogs".
        self.roots.push(h);
        self.logout_dialog = Some(h);
        self.logout_prompt = Some(string_id.to_string());
        self.logout_prompt_applied = false;
        true
    }

    /// Whether the prompt still needs its text from the host.
    #[must_use]
    pub fn needs_logout_prompt_text(&self) -> bool {
        self.logout_prompt.is_some() && !self.logout_prompt_applied
    }

    /// The resolved sentence, written into the dialog's one `TextElement`.
    pub fn show_logout_prompt(&mut self, ui: &mut UiSystem, text: &str) {
        self.logout_prompt_applied = true;
        self.logout_prompt_text = Some(text.to_string());
        let Some(d) = self.logout_dialog else { return };
        if let Some(h) = ui.get_child_recursive(d, logout::PROMPT_TEXT) {
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(text);
            }
        }
        // The dialog text write ends by resizing and repositioning the popup, and
        // the client calls it too: the panel is grown to fit the sentence and then
        // centred. Without it the shipped 400 x 95 panel shows the first line of a two-line prompt
        // and sits in the top-left corner instead of over the middle of the screen.
        dereth_ui::dialog::base::update_popup_size_and_position(ui, d);
    }

    /// The confirmation dialog, while it is up.
    #[must_use]
    pub fn logout_dialog(&self) -> Option<ElemHandle> {
        self.logout_dialog
    }

    /// The client's logout arm.
    ///
    /// On the context that matches the log-out dialog's: **[`Self::ending_session`] is cleared**,
    /// so the per-frame update will look again, and, when the answer under property 0x92 is
    /// true,
    /// [`Self::logout_confirmed`] and [`Self::do_end_session`] are set. *No* therefore leaves the
    /// player in the world with every flag back where it started.
    pub fn close_logout_dialog(&mut self, ui: &mut UiSystem, accepted: bool) {
        let Some(h) = self.logout_dialog.take() else {
            return;
        };
        self.roots.retain(|r| *r != h);
        ui.unregister_for_element_message(logout::BUTTON_YES, MessageId(1), ME);
        ui.unregister_for_element_message(logout::BUTTON_NO, MessageId(1), ME);
        ui.remove_and_delete_root(h);
        self.logout_prompt = None;
        self.logout_prompt_text = None;
        self.logout_prompt_applied = false;
        self.ending_session = false;
        if accepted {
            self.logout_confirmed = true;
            self.do_end_session = true;
        }
    }

    /// Global message 1, four actions.
    pub fn handle_key_press(&mut self, ui: &mut UiSystem, action: u32) {
        match action {
            action::LOGOUT_TO_SELECT => {
                self.should_quit_on_logout = false;
                self.logout_confirmed = true;
                self.do_end_session = true;
            }
            action::LOGOUT_AND_QUIT => {
                self.should_quit_on_logout = true;
                self.logout_confirmed = true;
                self.do_end_session = true;
            }
            action::TOGGLE_UI => {
                self.shown = !self.shown;
                if let Some(root) = self.root() {
                    ui.set_visible(root, self.shown);
                }
            }
            action::OPEN_HELP => {}
            _ => {}
        }
    }
    // -----------------------------------------------------------------------------------------
    // The chat surface's keyboard, its talk-focus menu and the floaty write-backs
    // -----------------------------------------------------------------------------------------

    /// The `ID_AssistedTell` row, or [`crate::chat::window::ASSISTED_TELL_FALLBACK`] when the host
    /// has no string service.
    fn reply_template(&self) -> &str {
        if self.reply_template.is_empty() {
            crate::chat::window::ASSISTED_TELL_FALLBACK
        } else {
            &self.reply_template
        }
    }

    /// The chat interface's child-action handler, fanned out to the five windows.
    ///
    /// Each window's arms are guarded on "the child that raised the action is *my*
    /// entry", so at most one of the five answers; the fan-out is what a container chain does in
    /// the client, where all five `ChatInterface`s sit on the same parent chain and each one's
    /// `on_child_action` runs the same test.
    ///
    /// Returns whether one of them consumed it. `true` is what stops the focused `TextElement`
    /// running its own `0x25`/`0x27`/arrow arms — see
    /// [`dereth_ui::UiSystem::dispatch_action`] for the ordering this stands in the middle of.
    pub fn chat_on_child_action(
        &mut self,
        ui: &mut UiSystem,
        child: ElemHandle,
        e: &dereth_ui::focus::InputEvent,
    ) -> bool {
        let stay = self.stay_in_chat_mode;
        for i in 0..self.chat_windows.len() {
            let w = self.chat_windows[i];
            let Some(iface) = self.chat.get_mut(i) else {
                continue;
            };
            let r = w.on_child_action(ui, iface, child, e, stay);
            if let Some(req) = r.request {
                ui.requests.emit(req);
            }
            if r.consumed {
                return true;
            }
        }
        false
    }

    /// The chat interface's action handler, delivered to the **main** chat window.
    ///
    /// The main chat panel's post-init is the one chat class that calls
    /// registers input maps `0x1000000D` and `0x1000000A` for itself, so the
    /// six reply/activate/toggle actions reach `MainChat`'s action handler and not the four floaty
    /// windows'. That is also what a player expects: the reply key fills the main box.
    ///
    /// `0x10000119`'s "start a tell to the selection" reads the existing host snapshot of the
    /// current selection and its name in form 2, then raises the same start-tell notice as the
    /// Friends button. The notice receiver owns focus and composition.
    pub fn chat_on_action(&mut self, ui: &mut UiSystem, action: u32) -> bool {
        let e = dereth_ui::focus::InputEvent {
            action,
            start: true,
            x: 0,
            y: 0,
        };
        let Some(w) = self.chat_windows.first().copied() else {
            return false;
        };
        let template = self.reply_template().to_owned();
        let targets = self.reply_targets.clone();
        let selected = Some((
            self.chat_auto_target_world.selected_id,
            self.chat_auto_target_world.selected_name.as_str(),
        ));
        let Some(iface) = self.chat.first_mut() else {
            return false;
        };
        let r = w.on_action(ui, iface, &e, &targets, &template, selected);
        if let Some(req) = r.request {
            ui.requests.emit(req);
        }
        r.consumed
    }

    /// The client's menu arm, reached from an element
    /// message on the talk-focus menu container `0x10000014`.
    ///
    /// On action 7 the menu's selection is cleared; the picked item is `p2` (none: stop). The
    /// squelch toggle item toggles squelch on the current speakable target; any other item whose
    /// enum attribute `0x1000000B` is set selects that talk focus.
    ///
    /// The client reads the focus off the **item's** attribute `0x1000000B`. The items are created
    /// at runtime by the menu's text-item insert, which this build's `Menu`
    /// does not implement, so the row is addressed by its **index in the menu** instead —
    /// the talk-focus menu build adds the squelch toggle at index 0 and then
    /// [`crate::chat::mainchat::TALK_FOCUS_MENU_ORDER`], which carries exactly the ids those
    /// enum-attribute writes set. Index 0 is the squelch toggle and is not a focus.
    pub fn chat_target_menu_selection(
        &mut self,
        ui: &mut UiSystem,
        index: usize,
    ) -> Option<crate::chat::mainchat::TalkFocusChange> {
        let focus = *crate::chat::mainchat::TALK_FOCUS_MENU_ORDER.get(index.checked_sub(1)?)?;
        let change = self.main_chat.on_menu_chosen(ui, focus)?;
        ui.requests.emit(UiRequest::SetTalkFocus {
            focus: change.focus,
        });
        Some(change)
    }

    /// The same arm addressed the way the client addresses it — by the **element** in `p2`.
    ///
    /// This is the whole of the client's message-7 body,
    /// including the branch [`Self::chat_target_menu_selection`] could not have: the squelch row
    /// is matched by pointer, not by attribute and not by index, because it is the one row
    /// `init_talk_focus_menu` deliberately leaves untagged.
    ///
    /// It is also the first caller
    /// The main chat panel's toggle squelch on current speakable target has ever had.
    pub fn chat_target_menu_item(
        &mut self,
        ui: &mut UiSystem,
        item: dereth_ui::ElemHandle,
    ) -> crate::chat::mainchat::MenuRowChoice {
        use crate::chat::mainchat::MenuRowChoice;
        let choice = self.main_chat.on_menu_chosen_item(ui, item);
        match &choice {
            MenuRowChoice::Focus(change) => {
                ui.requests.emit(UiRequest::SetTalkFocus {
                    focus: change.focus,
                });
            }
            MenuRowChoice::Squelch => {
                // The squelch test lives in `dereth_client_model::chat::ChatState`, on the far side
                // of this crate's seam, and the host answers it into `chat_target_squelched`.
                if let Some(e) = self
                    .main_chat
                    .toggle_squelch_on_current_speakable_target(self.chat_target_squelched)
                {
                    ui.requests.emit(UiRequest::ModifyCharacterSquelch {
                        object: dereth_primitives::ObjectId(e.object),
                        add: e.add,
                        account: e.account,
                        message_type: e.message_type,
                    });
                }
            }
            MenuRowChoice::None => {}
        }
        choice
    }

    /// The once-a-second auto-target sweep, with its own throttle; the caller of
    /// `use_time_auto_target`.
    ///
    /// The throttle is the client's file-scope "next time": return early while now is before
    /// it, and set it to now + 1.0 on **every** path out, taken *before* the
    /// work rather than after it. Keeping it on the screen rather than in
    /// `dereth_ui_screens::chat` is the same choice the client makes — the throttle is not a member
    /// of `MainChat`.
    ///
    /// `world` is what the object system answered; `resolve` turns the id the sweep picked into
    /// the name/talkable/squelched triple needs, which is the same seam
    /// [`Self::chat_target_menu_item`] crosses. Returns what the sweep decided, or `None` when the
    /// second has not elapsed.
    pub fn chat_use_time(
        &mut self,
        ui: &mut UiSystem,
        now: f64,
        world: &crate::chat::mainchat::AutoTargetWorld,
        resolve: impl FnOnce(u32) -> Option<crate::chat::mainchat::SpeakableTarget>,
    ) -> Option<crate::chat::mainchat::AutoTarget> {
        use crate::chat::mainchat::AutoTarget;
        if now < self.chat_auto_target_next {
            return None;
        }
        self.chat_auto_target_next = now + crate::chat::mainchat::AUTO_TARGET_INTERVAL_SECONDS;
        let decision = self.main_chat.use_time_auto_target(world);
        match decision {
            AutoTarget::Unchanged => {}
            AutoTarget::Clear => self.main_chat.set_selected(ui, None),
            AutoTarget::Adopt(id) => {
                let t = resolve(id);
                self.main_chat.set_selected(ui, t.as_ref());
            }
        }
        Some(decision)
    }

    /// Advances the opacity fade for each of the five chat windows by one step.
    ///
    /// Returns the windows that moved this frame, as `(window id, opacity)`, so a test can say
    /// *"the main window faded to 0.98"* rather than *"nothing crashed"*. A window whose fade has
    /// settled is absent — that is the client unregistering from global message 3, which a
    /// rebuild easily misses.
    pub fn chat_fade_tick(&mut self, ui: &mut UiSystem) -> Vec<(u32, f32)> {
        let mut out = Vec::new();
        for i in 0..self.chat.len() {
            let Some(w) = self.chat_windows.get(i).copied() else {
                continue;
            };
            let engaged = w.fade_engaged(ui);
            let Some(v) = self.chat[i].fade_tick(engaged) else {
                continue;
            };
            w.set_opacity(ui, &mut self.chat[i], v);
            out.push((self.chat[i].window_id, v));
        }
        out
    }

    /// The two **stored** chat options — gameplay-option properties `0x10000080` and
    /// `0x10000081`, which `ChatOptionsPanel`'s two general-section sliders write and the floaty
    /// chat panel's update from the player module re-reads.
    ///
    /// This is the property *read* half: the host decodes the `PlayerModule` (it owns
    /// the player system, a `Screen` does not) and hands the pair down. `None` leaves that half
    /// alone, which is a module that carries no value rather than one that carries zero — the
    /// distinction the client's fallback of `1.0f` turns on.
    ///
    /// Applied to every window, because the two sliders are in the **general** section and are
    /// not per-window. Re-arms the fade, since the target moved.
    /// Returns how many windows were written.
    pub fn chat_set_stored_opacity(
        &mut self,
        ui: &mut UiSystem,
        default_opacity: Option<f32>,
        active_opacity: Option<f32>,
    ) -> usize {
        if default_opacity.is_none() && active_opacity.is_none() {
            return 0;
        }
        for i in 0..self.chat.len() {
            let Some(w) = self.chat_windows.get(i).copied() else {
                continue;
            };
            // `set_default_opacity` first, exactly as `on_set_attribute` applies them, so its
            // "raise active to match" clause runs before the active value arrives.
            //
            // They go through [`crate::chat::window::ChatWindow`]'s own setters, which preserve
            // both active and idle tails: an idle window takes the new idle value **now**, an
            // engaged one takes the new active value now, and it is that push — not the fade — that
            // the shipped `ChatOptionsPanel` slider relies on. Moving only the fade's target would
            // change the window by 5 % of its travel per frame on a drag and not at all on a login
            // until something happened to tick the fade.
            if let Some(d) = default_opacity {
                w.set_default_opacity(ui, &mut self.chat[i], d);
            }
            if let Some(a) = active_opacity {
                w.set_active_opacity(ui, &mut self.chat[i], a);
            }
            // The window may still be the wrong side of its own target — an engaged window whose
            // *idle* value moved has nothing to apply now and everything to fade to when the
            // pointer leaves — so it re-subscribes either way, which is
            // where the client re-subscribes to global message 3.
            self.chat[i].fading = true;
        }
        self.chat.len()
    }

    /// The chat interface's clear-chat-buffer notice, offered to all
    /// five windows — `windowId == 0` is every window and any other value is the one that owns it.
    ///
    /// Beside the model half this clears the **element**: the chat log's glyph list, which is what
    /// a player sees. Returns
    /// the window ids that cleared.
    ///
    /// **Its producer is not wired here.** The client has exactly one caller: the communication
    /// system's clear step, which the command initialisation installs as the `/clear` chat command
    /// (`"all"` as the first argument passes window 0, i.e. every window). The chat-command table
    /// is the other side of that seam.
    pub fn on_clear_chat_buffer(&mut self, ui: &mut UiSystem, window_id: u32) -> Vec<u32> {
        let mut out = Vec::new();
        for i in 0..self.chat.len() {
            if window_id != 0 && window_id != self.chat[i].window_id {
                continue;
            }
            self.chat[i].recv_clear_chat_buffer(window_id);
            if let Some(log) = self.chat_windows.get(i).and_then(|w| w.log) {
                if let Some(t) = ui.text_element_mut(log) {
                    t.set_text("");
                }
            }
            if let Some(arrow) = self.chat_windows.get(i).and_then(|w| w.new_text_below) {
                ui.set_state(arrow, crate::chat::window::NEW_TEXT_BELOW_OFF);
            }
            out.push(self.chat[i].window_id);
        }
        out
    }

    /// The client's `case 0x12`, delivered to whichever of
    /// the five windows owns the entry the character was typed into.
    ///
    /// Returns what was expanded, which is `None` for every character that is not the space after
    /// one of the five aliases — i.e. almost all of them.
    pub fn chat_on_entry_character(
        &mut self,
        ui: &mut UiSystem,
        source: ElemHandle,
        ch: char,
    ) -> Option<crate::chat::interface::TextReplacement> {
        let targets = self.reply_targets.clone();
        for i in 0..self.chat_windows.len() {
            let w = self.chat_windows[i];
            if w.entry != Some(source) {
                continue;
            }
            let iface = self.chat.get_mut(i)?;
            return w.on_entry_character(ui, iface, source, ch, &targets);
        }
        None
    }

    /// The main chat panel's element-message handler's **message-1** arm:
    /// element `0x1000046F` runs the maximize-button handling.
    pub fn chat_on_maximize_button(&mut self, ui: &mut UiSystem) -> Option<(i32, i32)> {
        let root = self.root()?;
        let main = ui.get_child_recursive(root, window::MAIN_CHAT)?;
        // The main window's own children, so `handle_maximize_button`'s resize lands in
        // resize_to the way the client's virtual call does.
        let w = self.chat_windows.first().copied().unwrap_or_default();
        self.main_chat.handle_maximize_button(ui, main, &w)
    }

    /// Light the lamp button that stands
    /// for one of the four floaty chat windows.
    ///
    /// This runs **in addition to** [`Self::recv_set_panel_visibility`]'s page swap, because the
    /// notice is broadcast and both `PanelStack` and `MainChat` receive it.
    pub fn chat_recv_set_panel_visibility(&mut self, ui: &mut UiSystem, panel: u32, visible: bool) {
        let Some(root) = self.root() else { return };
        crate::chat::mainchat::MainChatPanel::on_set_panel_visibility(
            ui,
            root,
            ElementId(panel),
            visible,
        );
    }

    /// The start-tell notice's `(name)` one
    /// receiver, which is on the **main** window.
    ///
    /// The notice is broadcast to every notice handler, but only the main chat window (and its
    /// floating main-chat variant) handles the start-tell notice, so the four `FloatingChat`
    /// windows ignore it and only `chat_windows[0]` fills. Reached from `UiShell::handle_request`'s
    /// [`UiRequest::StartTell`] arm.
    ///
    /// Returns whether a main window was there to take it.
    pub fn chat_recv_notice_start_tell(&mut self, ui: &mut UiSystem, name: &str) -> bool {
        let Some(w) = self.chat_windows.first().copied() else {
            return false;
        };
        let Some(iface) = self.chat.first_mut() else {
            return false;
        };
        w.on_start_tell(ui, iface, name);
        true
    }

    /// The main chat panel's text tag iid string click notice.
    ///
    /// The receiver accepts only `TextTagType::Tell` (`0x10000001`), ignores the IID, and starts
    /// the main tell editor only while its entry is not focused. The string payload is the
    /// appropriate-form name composed into the clickable run.
    pub fn chat_recv_notice_iid_string_click(
        &mut self,
        ui: &mut UiSystem,
        payload: &dereth_ui::NoticePayload,
    ) {
        if payload.a != 0x1000_0001 {
            return;
        }
        if self
            .chat_windows
            .first()
            .is_some_and(|w| w.is_text_entry_focused(ui))
        {
            return;
        }
        self.chat_recv_notice_start_tell(ui, payload.text.as_deref().unwrap_or(""));
    }

    ///  for the four floaty windows: message 1
    /// on `0x1000052A` hides the window, which writes `Option_Placement_Visibility` back.
    ///
    /// Returns the window id that closed.
    pub fn chat_on_close_button(&mut self, ui: &mut UiSystem, source: ElemHandle) -> Option<u32> {
        for w in &self.floaty_chat {
            if w.listen_to_element_message(ui, source, dereth_ui::msg::element::id::BUTTON_CLICKED)
            {
                return Some(w.window_id);
            }
        }
        None
    }

    /// The set-chat-window-title notice `(windowId, si)`, broadcast to all five windows and
    /// taken by the one whose window id matches — the floaty chat panel's set chat window title
    /// notice.
    pub fn on_set_chat_window_title(
        &mut self,
        ui: &mut UiSystem,
        window_id: u32,
        title: crate::view::ChatWindowTitle,
    ) -> bool {
        for i in 0..self.floaty_chat.len() {
            let mut w = std::mem::take(&mut self.floaty_chat[i]);
            let took = w.on_set_chat_window_title(ui, window_id, title.clone());
            self.floaty_chat[i] = w;
            if took {
                return true;
            }
        }
        false
    }
}

/// The Client Options page's Apply / Cancel / Defaults buttons.
///
/// The three ids are shared by all three option pages, so the click is attributed by walking
/// up to [`crate::options::config::CONFIG_PAGE_ELEMENT`]; that is what makes this arm
/// `ClientOptionsPanel`'s and not `ChatOptionsPanel`'s.
///
/// All three are live because the option array holds the 27 rows' 30 controls, and Apply and
/// Cancel are defined entirely in terms of it; all three buttons walk it:
///
/// | element | function | effect |
/// |---|---|---|
/// | `0x100001FC` | | snapshots the saved values; writes nothing |
/// | `0x100001FD` | | writes the old value back, **only for the controls that changed** |
/// | `0x100001FE` | | writes every control's default value |
///
/// `restore_default_values` over the live page and the static
/// [`crate::options::config::restore_default_values`] table produce the same 30 writes; a test
/// asserts they agree, which is what stops the two drifting apart.
fn is_descendant_of(
    ui: &UiSystem,
    mut h: dereth_ui::ElemHandle,
    root: dereth_ui::ElemHandle,
) -> bool {
    loop {
        if h == root {
            return true;
        }
        match ui.parent(h) {
            Some(p) => h = p,
            None => return false,
        }
    }
}

fn is_under_config_page(ui: &UiSystem, source: dereth_ui::ElemHandle) -> bool {
    use crate::options::config;
    // Walk up the ancestors looking for the config page by id.
    let mut h = source;
    loop {
        match ui.node(h) {
            Some(n) if n.element_id() == config::CONFIG_PAGE_ELEMENT => return true,
            _ => match ui.parent(h) {
                Some(p) => h = p,
                None => return false,
            },
        }
    }
}

impl Screen for GamePlayScreen {
    fn create(&mut self, cx: &mut ScreenCx<'_>) -> Result<(), UiError> {
        let ui = &mut *cx.ui;
        let root = ui
            .require_env()
            .and_then(|e| e.create_and_add_root_element(ui, LAYOUT, ROOT))?;
        self.roots.push(root);
        // The client registers the framework as
        // a listener on the root, and every element message in the subtree then bubbles to it.
        // Without this the screen's element-message handler is never called and **nothing on the
        // HUD does anything**: no toolbar button opens a panel, no chat window reports its
        // visibility. It is the same line as on the character screen.
        ui.register_for_element_messages(root, ME);
        self.shown = true;
        // Every window's post-init set-up. In the client these run from the element
        // factory as the tree initialises, which is *inside* root-element creation; here the
        // screen owns the subtree, so they run immediately after it.
        self.post_init(ui);
        ui.register_for_global_message(dereth_ui::msg::global::KEY_DOWN_UNCONSUMED, ME);
        ui.register_for_global_message(dereth_ui::msg::global::TICK, ME);
        ui.register_for_global_message(dereth_ui::msg::global::WINDOW_MOVED, ME);
        // `0x0D` is what the radar's padlock broadcasts, and every window whose
        // locked-status update this screen stands in for listens for it —
        // the radar's global-message handler, `WorldView`, and each floating window.
        // [`Self::on_global_message`] had the arm and nothing had ever registered for the message,
        // so the whole cascade was unreachable from a click.
        ui.register_for_global_message(dereth_ui::msg::global::UI_LOCK_TOGGLED, ME);
        // The logout pair, [`NOTICES`].
        //
        // **Measured: nothing in the retail client ever sends the log-off notice:** retail
        // registers this handler and never fires it. The registration is kept because it is the
        // client's, and because it is the seam anything that does fire the notice must land on.
        for id in NOTICES {
            ui.notices.register(id, ME);
        }
        // This registration belongs to `MainChat`, not the logout pair above. The Rust screen owns
        // the main `ChatWindow`, so it is the external adapter for that receiver.
        ui.notices.register(NoticeId::IidStringTagClicked, ME);
        Ok(())
    }

    /// The chat interface's child-action handler and the floaty main chat window's,
    /// which are the same eight arms — see [`Self::chat_on_child_action`].
    fn on_child_action(
        &mut self,
        cx: &mut ScreenCx<'_>,
        child: ElemHandle,
        e: &dereth_ui::focus::InputEvent,
    ) -> bool {
        let ui = &mut *cx.ui;
        self.chat_on_child_action(ui, child, e)
    }

    /// The notice half of `GamePlayScreen`'s registrations: the logout pair and the main chat
    /// panel's text-tag click.
    fn on_notice(
        &mut self,
        cx: &mut ScreenCx<'_>,
        id: NoticeId,
        payload: &dereth_ui::NoticePayload,
    ) {
        let ui = &mut *cx.ui;
        match id {
            // The main chat panel's object-and-string text-tag click notice, separate from
            // `GamePlayScreen`'s own logout pair.
            NoticeId::IidStringTagClicked => self.chat_recv_notice_iid_string_click(ui, payload),
            // No argument; the handler sets
            // `do_end_session` and `should_quit_on_logout` and raises the confirmation.
            NoticeId::Logoff => self.on_logoff(ui),
            // The game play screen's end character session notice, whose `int` argument
            // the character-management panel's end character session notice forwards verbatim.
            NoticeId::EndCharacterSession => {
                #[allow(clippy::cast_possible_wrap)]
                // LINT-OK: the client's parameter is a plain `int`; the bus carries it as `u32`.
                self.on_end_character_session(ui, payload.a as i32);
            }
            _ => {}
        }
    }

    fn on_global_message(&mut self, cx: &mut ScreenCx<'_>, id: MessageId, param: u32) {
        let ui = &mut *cx.ui;
        if id == dereth_ui::msg::global::KEY_DOWN_UNCONSUMED {
            // Two listeners in the client are registered for global message 1 from
            // this screen's subtree: the gameplay key handler below
            // **and the toolbar's global-message handler**, which handles the eighteen
            // quickbar hotkeys and the make-shortcut key. [`Self::on_input_action`] is the second
            // of those; without this call pressing `1` does nothing even with the broadcast. The
            // request goes out through the ordinary queue,
            // which is what `use_shortcut`'s object use and
            // selected-object set become here.
            if let Some(r) = self.on_input_action(ui, param) {
                // The client's target-mode arm is *two* calls, and the
                // order is load-bearing: `execute_target_mode_for_item` reads the live
                // target mode to decide whether the slot's object is a use target, an examine
                // target or a plain use, and only then does the client clear it. Emitting the
                // clear first would turn every second key press into a no-op. This is the one
                // path that clears the mode itself — target-mode execution never
                // clears it, which is why the pointer's version of the same gesture is a single arm.
                let clears_target_mode = matches!(r, UiRequest::ExecuteTargetItem(_));
                ui.requests.emit(r);
                if clears_target_mode {
                    ui.requests
                        .emit(UiRequest::SetTargetMode(crate::view::TargetMode::None));
                }
            }
            // `ACTION_CANCEL` while the stack-size box has focus restores its committed quantity
            // and un-focuses it, instead of firing slot 0 — the toolbar's focus gate, the other
            // half of the `None` `on_input_action` returns in that state.
            if param == crate::toolbar::shortcuts::ACTION_CANCEL {
                self.reset_stack_size_box(ui);
            }
            // The client's six arms — the three reply
            // keys, activate-and-select-all, the toggle and the two command-or-alias keys. In the
            // client they arrive through the two input maps the chat interface registers
            // (`0x1000000D` and `0x1000000A`); here they arrive as the same global
            // message 1 every other hotkey in this screen uses, which is what the key-press event
            // broadcasts for an action nothing consumed.
            self.chat_on_action(ui, param);
            self.handle_key_press(ui, param);
        } else if id == dereth_ui::msg::global::TICK {
            // The menu option control's global-message handler delays the
            // resolution confirmation for two global-message-3 edges. The page owns the option
            // and factory context; this screen owns the live dialog root beside every other
            // gameplay root.
            self.roots.extend(self.config_page.confirmation_tick(ui));
            self.roots.retain(|h| ui.node(*h).is_some());
            // The chat interface's global-message handler — the opacity
            // fade, one step per frame, on the same global message 3 the client runs it on. The
            // screen is already a registered listener (see `post_init`); this arm is `fade_step`'s
            // caller.
            self.chat_fade_tick(ui);
            // The client's font-preference callback's
            // broadcast, delivered on the next tick rather than inside the store's own write —
            // see [`Self::drive_chat_font`]. Costs one thread-local read on a frame where nobody
            // touched the two font preferences.
            self.drive_chat_font(ui);
        } else if id == dereth_ui::msg::global::UI_LOCK_TOGGLED {
            let locked = self.locked;
            self.cascade_lock(ui, locked);
        }
    }

    fn on_element_message(&mut self, cx: &mut ScreenCx<'_>, m: &ElementMessage) {
        let ui = &mut *cx.ui;
        // The option controls' arms — a drag on a slider (`0x0A`), a click on a check box
        // (`1`). In the client these are each control's own element-message handler;
        // here the page owns the value array, so the dispatch is one call. It is **first**,
        // because a check box on the options page raises message 1 and every other message-1 arm
        // below is keyed on an element id this one never carries.
        self.config_page.on_element_message(ui, m);
        // The examine
        // window's close control. It is another message-1 arm keyed on one element id, and it is
        // the *only* thing in this build that hides `<EXAM>` from a gesture. Deliberately does not
        // clear `awaiting`/`current`: see `ExaminationPanel::on_element_message` for why that is
        // what stops the 0.75 s combat re-poll re-opening a panel the player closed.
        self.examination.on_element_message(ui, m);
        // The client's two paging arms. Another
        // message-1 pair keyed on two element ids, and the reader's only gesture.
        self.book.on_element_message(ui, m);
        // Hair/face preview edits and native no-send Cancel.
        self.barber.on_element_message(ui, m);
        // The vitals press handler: clicking the vitals bar toggles between numerical display and
        // bars with a glyph in the centre of each.
        //
        // The client's handler is installed on the vitals element itself and its `this` is the
        // element it toggles; here the screen root is the one listener, so the receiver has to be
        // recovered from the source's ancestry. Both windows carry it: the stacked window
        // (`<VITS>`) inherits the base vitals handler, while the side-by-side window forwards
        // directly to that same handler. Reading the toggle as the side-by-side window's alone
        // would leave the stacked bar, the default, unable to toggle at all.
        self.on_vitals_press(ui, m);
        // The client's message-1 arm on
        // the Character Options page: read attribute `0x0E`, then apply it with
        // the row's own option and current value — **one option, never a word**. See the module
        // docs on `options::character` for why composing the word from the page's own check boxes
        // would silently clear bit 25 of the second option word.
        self.character_options.on_element_message(ui, m);
        // The Chat Options page's two slider option arms (`0x0A`) and its 64 wide bit-field
        // check-box arms (message 1). Unlike the
        // Character Options page this one needs **no** `GameView` to take a gesture — `Apply` is
        // a property write, not a re-read — so it is dispatched here and only Apply / Cancel /
        // Defaults and the visibility edge are queued. Each accepted gesture's effect is turned
        // straight into the gameplay-option-changed notice, which is what the player module's
        // change handler raises as its first line.
        for e in self.chat_options.on_element_message(ui, m) {
            self.chat_recv_notice_gameplay_option_changed(ui, e);
        }
        // A click or a right-click on a key button is queued rather than
        // dispatched: the row needs the host's `InputManager`, which a `Screen` handler is not
        // given. [`Self::drive_key_bindings`] is the drain.
        if self.is_key_binding_element(m.source) {
            self.key_binding_inbox.push(m.clone());
        }
        // On the Character
        // Options page. Queued rather than applied here because the show arm re-reads every row
        // from the `PlayerModule` and this handler has no view — see
        // [`Self::character_option_visibility`].
        if m.id == dereth_ui::msg::element::id::VISIBILITY_CHANGED
            && m.source_id == crate::options::character::CHARACTER_PAGE_ELEMENT
            && self.character_options.page == Some(m.source)
        {
            self.character_option_visibility.push(m.p1 != 0);
        }
        // The player-option page's visibility-changed handler — `save_current_values` on show,
        // `restore_saved_values` on hide. In the client that is a virtual call on the page element;
        // here it is the element message the visibility setter raises at the same
        // moment, which is the only signal this build's panel stack gives.
        if m.id == dereth_ui::msg::element::id::VISIBILITY_CHANGED
            && m.source_id == crate::options::config::CONFIG_PAGE_ELEMENT
        {
            self.config_page_visibility_changed(ui, m.p1 != 0);
        }
        // The client's only other arm: on its own `0x18`, a hide unregisters
        // the page from global message 3, and a show registers it and runs the update at once.
        //
        // The registration half is what makes the five-second tick reach the page **only while it
        // is open**; the immediate update is what makes the date, the coordinates and the marker
        // correct in the frame the player opens the map rather than up to five seconds later,
        // because the update's first line sets the next update to now + 5.
        //
        // Only the update half is transcribed here: this build's tick does not go through a
        // listener registration, it goes through `dereth_client::hud`'s `if
        // screen.map_update_due(...)`. Zeroing `next_map_update` makes the next poll due at once,
        // which is exactly what running the update on the show edge achieves; the
        // unregister-on-hide is **not** transcribed, so this build keeps polling a closed page.
        // That costs one guarded-and-usually-idle pass every five seconds and writes only to hidden
        // elements of the page itself, leaving continued polling while the page is closed as the
        // one remaining difference.
        if m.id == dereth_ui::msg::element::id::VISIBILITY_CHANGED
            && self.map.page.is_some()
            && self.map.page == Some(m.source)
            && m.p1 != 0
        {
            self.next_map_update = 0.0;
        }
        // The same edge on the Chat Options page, queued for the same reason the
        // Character Options one is: the show arm re-reads every control from the `PlayerModule`.
        if m.id == dereth_ui::msg::element::id::VISIBILITY_CHANGED
            && m.source_id == crate::options::chat::CHAT_PAGE_ELEMENT
            && self.chat_options.page == Some(m.source)
        {
            self.chat_option_visibility.push(m.p1 != 0);
        }
        // The panel-stack visibility handler: on `0x18`, find the page among the
        // stack's children and send the set-panel-visibility notice with that page's panel id
        // and the child's current visibility, stopping at the first match.
        //
        // [`crate::panels::panel_stack::PanelStack::on_page_visibility_changed`] is that loop and
        // **had no caller**, which is the second half of why an indicator lamp opened nothing: the
        // action's `0x31` reaches the page and its `0x58 = 1` shows it, but without this the stack
        // never learns which page is current, never hides the one it covered and never brings
        // `<PANS>` itself up — so the panel is "visible" inside a window that is not.
        //
        // The notice is delivered here as well as queued for the same reason the end-session one
        // is: sender and
        // receiver are both this screen. The re-entry is bounded because
        // the visibility setter raises `0x18` only on a *change*, so the hide of the
        // covered page raises one more `0x18` whose `recv_set_panel_visibility` finds itself not
        // current and returns.
        if m.id == dereth_ui::msg::element::id::VISIBILITY_CHANGED {
            // **The `<COMB>` stack is in this loop too.** The combat-panel window's element-message
            // handler is the same `case 0x18` walk over its children, and `FloatingCombatStack` —
            // which *is* `<COMB>`, deriving from `CombatPanelStack` — inherits it. Without this the
            // combat cluster's own show reaches nothing and `<COMB>` stays down: no combat UI pops.
            //
            // `FloatingEnvironmentStack`'s element-message handler is the same implementation as
            // `FloatingCombatStack`'s. It is the same bridge and reads the child's CURRENT visible
            // flag, not a stale queued value.
            //
            // The three stacks' panel ids are disjoint (`0x1000005C` is 17 and `0x10000061` is 22;
            // the sixteen toolbar pages are 1–16 and 25, the five env pages 18–21 and 23 — read
            // off the shipped `classic_gameplay`), so offering both cannot cross-talk.
            // The child's visible flag, read off the child at notice time — **not** `m.p1`, the
            // value the broadcast was stamped with. The two differ whenever a page's visibility was
            // written more than once before the outbox was drained, and the stale reading is a
            // live-lock: a queued `p1=0` next to a queued `p1=1` makes this handler hide a page
            // that is up and show one that is down, each write queueing its own replacement
            // notice, so `UiShell::deliver_pending`'s drain loop never empties. Measured on the
            // shipped `<BOOK>` page `0x10000182`, whose child set-up hide and whose
            // OpenBook show are both queued in the frame a late `UiShell` comes up.
            let child_visible = ui.node(m.source).is_some_and(|n| n.region.flags.visible);
            let hit = self
                .panels
                .on_page_visibility_changed(m.source_id, child_visible)
                .or_else(|| {
                    self.combat_panel
                        .on_page_visibility_changed(m.source_id, child_visible)
                })
                .or_else(|| {
                    self.env_panel
                        .on_page_visibility_changed(m.source_id, child_visible)
                });
            if let Some(r) = hit {
                if let UiRequest::SetPanelVisibility { panel, visible } = r {
                    ui.requests.emit(r.clone());
                    self.recv_set_panel_visibility(ui, panel, visible);
                }
            }
        }
        // "0x18 (visibility changed) | element id is 0x10000505 or in 0x1000050E..0x10000510 |
        // the set-panel-visibility notice (elementId, visible)"
        if m.id == dereth_ui::msg::element::id::VISIBILITY_CHANGED
            && reports_panel_visibility(m.source_id)
        {
            ui.requests.emit(UiRequest::SetPanelVisibility {
                panel: m.source_id.0,
                visible: m.p1 != 0,
            });
            // The same notice reaches the main chat panel's set-panel-visibility
            // receiver, which lights one of the four lamp buttons `0x10000522`..`0x10000525` on
            // the main chat window's frame. The shipped layout carries all four and nothing lit
            // them.
            self.chat_recv_set_panel_visibility(ui, m.source_id.0, m.p1 != 0);
        }
        // The client's `case 0x12` — the
        // one element message the chat surface takes that is not a click. `TextElement` broadcasts
        // `CHARACTER` (`0x12`) with the character in `p1`; without this listener the
        // `r `/`rp `/`reply `/`mr `/`pr ` expansions cannot fire. The client
        // tests that the source is the chat entry and the low 16 bits of `p1` are `0x20`.
        if m.id == dereth_ui::msg::element::id::CHARACTER {
            self.chat_on_entry_character(ui, m.source, char::from_u32(m.p1).unwrap_or('\0'));
        }
        // The whole sequence: a **click** (element message 1) on a toolbar panel button broadcasts
        // a panel-visibility notice `(panelId, !visible)`,
        // and the panel-visibility notice handler shows that page and hides the
        // one it covered. Both ends live on this screen, so the notice is delivered here as well as
        // queued — the queued copy is what reaches the server-side "which panels are open" record.
        //
        // The button and the page are paired only by attribute `0x10000029`, read off the
        // live tree by `Toolbar::setup_buttons` and `PanelStack::setup_children`. Nothing here
        // knows which panel is which.
        // The client's message-1 arm, the
        // whole of what a click on a chat window's own two buttons does: "Send" (`0x10000019`)
        // runs `process_command`, and the "new text below" arrow (`0x1000048C`) jumps the log to
        // the end and takes itself down. All five windows carry the same child ids, so the window
        // is identified by the **handle**, which post_init resolved per subtree.
        // The client's only arm — the
        // button's own message 1 fires the input action attribute `0x12` names. It runs before
        // every arm below because in the client it *is* before them: it lives on the button, which
        // is the first listener the broadcast reaches, and returns stop-processing. See
        // [`Self::handle_button_click`] for what is and is not reproduced.
        if m.id == dereth_ui::msg::element::id::BUTTON_CLICKED
            && self.handle_button_click(ui, m.source).is_some()
        {
            self.button_actions_fired += 1;
        }
        if m.id == dereth_ui::msg::element::id::BUTTON_CLICKED {
            for i in 0..self.chat_windows.len() {
                let w = self.chat_windows[i];
                if w.send != Some(m.source) && w.new_text_below != Some(m.source) {
                    continue;
                }
                if let Some(r) =
                    w.listen_to_element_message(ui, &mut self.chat[i], m.source_id, m.id)
                {
                    ui.requests.emit(r);
                }
                break;
            }
            // The client's only arm:
            // message 1 from element `0x1000052A` hides the window — and the window's visibility
            // override then writes `Option_Placement_Visibility` back, which is what
            // makes a closed floaty window stay closed across a relog.
            self.chat_on_close_button(ui, m.source);
            // The client's message-1 arm:
            // the maximize button `0x1000046F`, which is the only button `MainChat` adds to
            // the ones `ChatInterface` already answers.
            if m.source_id == crate::chat::mainchat::main_chat::MAXIMIZE_BUTTON {
                self.chat_on_maximize_button(ui);
            }
        }
        // The client's menu arm: message `7`, with the chosen **item** in `p2`.
        //
        // The rows are real elements, `MENU_CHOSEN` is 7 (not `MENU_OPENED`'s value — see the
        // constant's own note), and `p2` carries the row, so there is no row index to read out of
        // `p1`.
        if m.id == dereth_ui::msg::element::id::MENU_CHOSEN
            && m.source_id == crate::chat::mainchat::main_chat::TALK_FOCUS_MENU_CONTAINER
        {
            self.chat_target_menu_item(ui, dereth_ui::ElemHandle::from_raw(m.p2));
        }
        if m.id == dereth_ui::msg::element::id::BUTTON_CLICKED {
            // **No panel-button arm here.** The toolbar class never sends the set-panel-visibility
            // notice and the toolbar's element-message handler's message-1 `switch` has no
            // panel-button case; and with the button-click handling on `dereth_ui`'s `Button`, a
            // panel button carrying a live attribute `0x12` returns stop-processing and **this
            // handler is never reached for one at all**. A test asserts this over the shipped tree:
            // if any of the seven ever loses its `0x12` the arm becomes reachable again and that
            // test says so. The same switch on the element id for message 1 that carries the four
            // stance icons carries the Use and Examine buttons, and each has *two* arms rather than
            // one — see [`Self::on_target_mode_button`].
            if let Some(r) = self.on_target_mode_button(m.source_id) {
                // The examine-object notice `(id)` — the notice the identify button raises, whose
                // one listener is the examination panel's examine-object receiver. Sender and
                // receiver are both this screen, so it is delivered here as well as queued, exactly
                // as `EndCharacterSession` is. Without it the panel's awaited appraisal id is never
                // set and the appraisal-info guard refuses every reply — which is the second link
                // of the three-link chain in [`crate::panels::examination`].
                if let UiRequest::Examine(id) = r {
                    self.examination.examine_object(id);
                }
                ui.requests.emit(r);
            }
            // The client's message-1 arm on
            // the slot checkbox (`0x100005BE`): the *Slots* checkbox swaps the paper doll for the
            // nine armour-coverage grids and back. The panel owns both halves.
            self.inventory.on_slot_checkbox(ui, m.source);
            // The character settings panel's element-message
            // handler, which the config panel's construction installs on the Client Options
            // page: the three buttons under the option box. See [`is_under_config_page`] and
            // [`crate::options::config::button`] for which call each takes.
            self.on_config_page_button(ui, m.source, m.source_id);
            // The same three ids under the Character Options page. Queued rather
            // than applied, for the reason the visibility edge below is: Apply re-reads all 50
            // rows from the `PlayerModule` and this handler has no `GameView`.
            // [`Self::drive_character_options`] is the drain.
            if matches!(
                m.source_id,
                crate::options::config::button::APPLY
                    | crate::options::config::button::CANCEL
                    | crate::options::config::button::DEFAULTS
            ) && self
                .character_options
                .page
                .is_some_and(|p| is_descendant_of(ui, m.source, p))
            {
                self.character_option_buttons.push(m.source_id);
            }
            // The same three ids under the Chat Options page — `0x1000050C`
            // carries its own `0x100001FC`/`FD`/`FE`, measured in the shipped tree. Queued for
            // [`Self::drive_chat_options`], because Apply re-reads every control from the
            // `PlayerModule` and this handler has no `GameView`.
            if matches!(
                m.source_id,
                crate::options::config::button::APPLY
                    | crate::options::config::button::CANCEL
                    | crate::options::config::button::DEFAULTS
            ) && self
                .chat_options
                .page
                .is_some_and(|p| is_descendant_of(ui, m.source, p))
            {
                self.chat_option_buttons.push(m.source_id);
            }
            // In full: message 1 from element `0x100000FA` sends the
            // end-character-session notice with argument 1.
            //
            // The lamp row's rightmost button is the **log-out** button, and the argument `1` is
            // the asking form: the handler raises the
            // `ID_Client_EndCharacterSessionConfirm` dialog for a non-zero argument. This is the
            // indicator strip's caller of `on_end_character_session`; the other two are on the
            // *Gameplay Options* page.
            //
            // `FloatingIndicators` repeats the same handler, and both instances of the strip are
            // in the shipped tree, so this is keyed on the element id exactly as the client is.
            if m.source_id == crate::hud::indicators::LOGOUT_BUTTON {
                self.on_end_character_session(ui, 1);
            }
            // The Options *Game / Support* page, all seven of its buttons rather than only the two
            // ways out of the game: the two support-ticket buttons and *Restore Defaults* go
            // through the transcription of the handler (`options::pages::gameplay_option_action`)
            // as well. Both ends are joined here: the page module resolves the click, this screen
            // performs it. See [`crate::options::gameplay`] for the handler and for the two buttons
            // that are driven by the layout instead of by the class.
            if let Some(a) = self.gameplay_options.on_element_message(ui, m) {
                self.on_gameplay_options_action(ui, &a);
            } else if self.logout_dialog.is_some() {
                if m.source_id == logout::BUTTON_YES {
                    self.close_logout_dialog(ui, true);
                } else if m.source_id == logout::BUTTON_NO {
                    self.close_logout_dialog(ui, false);
                }
            }
        }
        // The stack splitter — the toolbar's element-message handler's first two arms, which are
        // keyed on the element id *before* the message id. ---- the radar panel's element-message
        // handler, message `0x19` -------------------
        //
        // A click on the padlock `0x10000619` (when a player system exists) flips the lock-UI flag
        // and
        // broadcasts global `0xD`; otherwise a click on the radar itself with an object under the
        // mouse selects that object.
        //
        // Two arms, one message id, and the `else` is real: a click on the padlock is *never* also
        // a select, because the padlock is a child and the "radar itself" test fails for it.
        //
        // The lock arm reaches `PlayerModule` and the `0x0D` broadcast through the host, because
        // the flag is the player's and this crate never touches player state — see
        // [`Self::set_lock_ui`] and `dereth_client::ui`'s `SetLockUi` handler. The select arm is
        // `UiRequest::Select`, which selects the object and is routed by the host.
        if m.id == dereth_ui::msg::element::id::MOUSE_CLICK {
            if m.source_id == crate::mapradar::radar::child::LOCK_BUTTON {
                ui.requests.emit(UiRequest::SetLockUi(!self.locked));
            } else if m.source_id == window::RADAR {
                if let Some(id) = self.radar.object_under_mouse {
                    ui.requests.emit(UiRequest::Select(id));
                }
            }
        }
        if m.source_id == crate::toolbar::splitter::ENTRY_BOX
            && m.id == dereth_ui::msg::element::id::FOCUS_CHANGED
        {
            self.on_stack_box_focus(ui, m.p1 != 0);
        }
        if m.source_id == crate::toolbar::splitter::SLIDER
            && m.id == dereth_ui::msg::element::id::SCROLL_POSITION
        {
            self.on_stack_slider(ui, m.p1);
        }
        // The ui item element's element-message handler: a failed owner's 0x15
        // clears its weenie's waiting state and hides its ghost. `stop_drag_and_drop` notifies the
        // persistent drag icon, whose item-element parent owns this callback, not the temporary
        // proxy.
        // In this message seam only the owner's copy has p2 == 0; the catcher's separate 0x15
        // must still submit its request below and must not cancel the resulting wait.
        if m.id == dereth_ui::msg::element::id::DROP_FAILED && m.p2 == 0 {
            // **Off the proxy, not out of a registry.** The client's own version of this arm reads
            // the source slot's own weenie object, clears that object's waiting state (0) and hides
            // its ghost icon; it does not ask any list which item the drag held. Asking
            // `InventoryPanels::locate` would leave a **refused** drag out of a chest or a vendor
            // list greyed for the session. `ClearItemWaiting` reaches `dereth_client_model`, and
            // every panel's ghost pass reads `view.item_waiting`, so the right tile un-ghosts
            // wherever it lives; the local `clear_waiting` sweep below is this screen's own lists
            // doing it a frame early.
            let item = crate::items::widget::inq_drop_icon_info(ui, m.source)
                .item
                .or_else(|| self.locate_drag_owner(ui, m.source).and_then(|(_, _, i)| i));
            if let Some(item) = item {
                // The sweep and the request are one helper, which covers the eighteen shortcut
                // lists too — the client's arm is `UiItemWidget`'s, the class's, so a quickbar tile
                // runs it like any other. See [`Self::release_item_ghost`].
                self.release_item_ghost(ui, item);
            }
        }
        // Element message `0x15` on the drop
        // **target**, whose `p2` carries the drag's owner (see `dereth_ui::focus`'s correction). The
        // owner's own copy of the message has `p2 == 0`, which is how the two are told apart.
        if m.id == dereth_ui::msg::element::id::DROP_FAILED && m.p2 != 0 {
            // The target item element receives this same 0x15 before its parent item list handles
            // the drop. Its handler clears *that tile's* object and ghost unconditionally; it is
            // not the drag source, whose accepted 0x16 deliberately remains waiting.
            let catcher_item = self
                .inventory
                .lists_mut()
                .chain(self.shortcuts.slots.iter_mut())
                .find_map(|w| w.slot_of(m.source).and_then(|slot| w.slots[slot].item));
            if let Some(item) = catcher_item {
                self.release_item_ghost(ui, item);
            }
            // The item list element's element-message handler's `0x15` arm is two
            // statements, in this order: if the source element is a UI item (type `0x10000032`),
            // put its drag-accept icon in state `0x1000003f`; then run the drop release.
            //
            // The message id is the whole guard, so
            // the clear runs on the drop **target**, which is the copy this arm already keys on
            // (`p2 != 0`).
            //
            // **The first** is what takes the hint down when the drop lands; without it the hint
            // stands on the slot that was dropped on. The client raises no `0x3E` of its own, so
            // this arm is the only producer of the clear on the drop path.
            //
            // **The eighteen shortcut lists are `ItemListWidget`s too.** The arm is the class's, so
            // every list on this screen runs it, and `ShortcutBar::slots` has to be in this sweep
            // beside `InventoryPanels::lists_mut` (the pack, the strips and the doll). Without them
            // the cross `on_drag_cursor_over` put up on a quickbar tile stays up after the drop.
            // The leave route is not involved: `0x3E` with `p1 == 0` acts on the tile in
            // `ShortcutBar::on_drag_cursor_over`, and retail has no `0x3F` message.
            for w in self
                .inventory
                .lists_mut()
                .chain(self.shortcuts.slots.iter_mut())
            {
                if let Some(slot) = w.slot_of(m.source) {
                    w.slots[slot]
                        .set_drag_accept_state(ui, crate::items::widget::drag_accept_state::NONE);
                    break;
                }
            }
            self.handle_drop_release(ui, m.source, dereth_ui::ElemHandle::from_raw(m.p2));
        }
        // The ui item element's element-message handler's `0x3E` arm.
        //
        // This is the consumer: the producer is the drag-leave and drag-enter paths in
        // `dereth_ui::focus`, and without this line the broadcast would reach the screen's external
        // listener and fall through. There is no per-frame poll; see [`Self::on_drag_cursor_over`].
        if m.id == dereth_ui::msg::element::id::DRAG_CURSOR_OVER {
            self.on_drag_cursor_over(ui, m.source, m.p1 != 0);
        }
        // The client's two arms on the
        // inventory button `0x100001B1`, which is not an item list and reaches none of the
        // above: `0x3E` puts its drag overlay in state `0x10000046`
        // or `0x1000003F`, and `0x15` puts it in `0x1000003F` before
        // the drop release.
        self.toolbar.on_drag_cursor_over(ui, m);
        self.toolbar.on_drop_release(ui, m);
        // The client's `0x21` arm — the drag the
        // manager refused is the drag an item list starts for itself.
        if m.id == dereth_ui::msg::element::id::DRAG_REJECTED {
            let (x, y) = m.point.window;
            // **The doll's *figure* is not an item list and never reaches the line below.** The
            // paper doll panel has its own `0x21` arm on the drag mask `0x100001D6`, which resolves
            // the item by reading a pixel out of the click map and starts the drag from
            // `EquipmentPanel`'s **own** drag icon. It is the exact counterpart of the `0x1C` arm
            // one branch down, and without it a press-and-drag on the figure does nothing at all:
            // the mask is not a slot of any list, so
            // [`crate::items::widget::begin_drag_from_rejected`] declines it and no drag starts.
            // See [`crate::panels::inventory::InventoryPanels::begin_paper_doll_drag`].
            if self
                .inventory
                .begin_paper_doll_drag(ui, m.source, x, y)
                .is_none()
            {
                self.begin_item_drag(ui, m.source, x, y);
            }
        }
        // The client's whole first
        // branch is on message `0x1c`, and it switches on `p1` — the **input action**
        // that pressed. See [`Self::on_item_list_press`].
        if m.id == dereth_ui::msg::element::id::MOUSE_PRESS {
            // **The doll's *body* is not an item list and never reaches the line below.** The paper
            // doll panel's element-message handler's `0x1C` arm is a separate handler on the drag
            // mask `0x100001D6`, and it resolves the item by reading a pixel out of the click map
            // rather than by asking a slot which item it holds. Without it a click on the figure
            // does nothing until the *Slots* checkbox swaps it for the nine armour grids, which are
            // item lists and do reach [`Self::on_item_list_press`]. See
            // [`Self::on_paper_doll_press`].
            if !self.on_paper_doll_press(ui, m.source, m.p1) {
                self.on_item_list_press(ui, m.source, m.p1);
            }
        }
        // The stat-management panels switch on `0x1C` (a press on a list row → `set_selection`) and
        // `1` (the two raise buttons), and the spellbook switches on `1` for its thirteen filter
        // buttons. Those panels hang off `Hud` rather than off this screen (see
        // [`Self::panel_messages`]), so the message is recorded here and delivered there.
        // `0x0A` goes on the same queue. The combat panel's element-message
        // handler's third arm is the combat window's power **scrollbar**
        // (`0x1000004F`, element type `0x0B` in the shipped tree), and its handler needs a
        // `GameView` for the advancement class of skill `0x32` — which this slot does not have and
        // `RemainingPanels::on_element_message` does. The window filters on its own subtree, so a
        // scroll anywhere else on the screen reaches it and is declined.
        // So do `7` and `0x2C`: the vendor panel switches on four ids, and these two are the ones
        // that make the vendor's filter strip do anything: `MENU_CHOSEN` from `0x100000BF` and
        // `TAB_PAGE_CHANGED` from `0x100000B8`. A gate written against only some consumers does not
        // fail when another arrives — it goes quiet, and the symptom is a filter arm that is never
        // reached and looks unwired.
        if m.id == dereth_ui::msg::element::id::MOUSE_PRESS
            || m.id == dereth_ui::msg::element::id::BUTTON_CLICKED
            || m.id == dereth_ui::msg::element::id::SCROLL_POSITION
            || m.id == dereth_ui::msg::element::id::MENU_CHOSEN
            || m.id == dereth_ui::msg::element::id::TAB_PAGE_CHANGED
            // `0x15`: the spellcasting panel's element-message handler's
            // third arm invokes the spell bar's own drop handler, and
            // its `add_favorite` needs a `GameView` for the favourite-spells list -- which this slot
            // does not have and `RemainingPanels::on_element_message` does. It is the same
            // message [`Self::handle_drop_release`] above keys on and the two do not collide: the
            // bar's arm refuses any drop whose proxy carries no `0x10000010` spell id, and the
            // inventory's refuses any whose owner is not one of its own slots.
            || m.id == dereth_ui::msg::element::id::DROP_FAILED
            // `0x21`, for the same reason. `begin_item_drag` below offers
            // `begin_drag_from_rejected` the four inventory lists; the spellbook's and the
            // bar's eight hang off `RemainingPanels`, so without this a press-and-drag on a spell
            // finds no owner and no drag starts. Both handlers run and exactly one list can
            // own the slot, so the duplicate delivery is a no-op on whichever misses.
            || m.id == dereth_ui::msg::element::id::DRAG_REJECTED
            // `0x3E`: the item element's element-message handler's
            // drag-over arm forwards to the item list, whose **first** act is to ask
            // the list's drag handler; `VendorPanel` registers one (the client, on
            // the sell list `0x100000CE`) and lives on `RemainingPanels`. [`Self::
            // on_drag_cursor_over`] below answers the same message for the four lists this screen
            // owns, and the two cannot collide: each keys on a list of its own and `slot_of`
            // answers `None` for the other's.
            || m.id == dereth_ui::msg::element::id::DRAG_CURSOR_OVER
            // `0x10000003` from `0x1000004A`, the first entry here keyed on the *source element* as
            // well as on the message. It has to be: `0x10000003` is a layout-defined id, not one of
            // the element base's, and the only thing that raises it is the over-head bubble's own
            // message media step. It reaches the spew box panel's element-message handler through
            // `RemainingPanels::on_element_message`; if this filter dropped it, every
            // channel-`0x1A` notice this client put on screen would stay there for the rest of the
            // session.
            || (m.id == crate::hud::speech_bubbles::MSG_BUBBLE_EXPIRED
                && m.source_id == crate::hud::speech_bubbles::BUBBLE_ELEMENT)
            // Two more, both keyed on their source for the same reason.
            //
            // `0x18` reaches the journal panel's element-message handler's first arm (its own
            // element becoming visible -> update), which is the **only** thing that ever fills the
            // page list, so without this line that panel is a permanently empty list box. It is
            // keyed on the two panel ids because `0x18` is raised by every visibility change in the
            // tree and forwarding all of them would put several hundred messages a frame through a
            // fan-out that wants two.
            //
            // `0x43` reaches the page list panel's element-message handler's
            // `check_for_double_click` arm — open this row's page in the journal — and is keyed on
            // the page list's own list box, which is the only list in the tree that raises one
            // anybody listens for.
            || (m.id == dereth_ui::msg::element::id::VISIBILITY_CHANGED
                && (m.source_id == crate::panels::journal::PANEL
                    || m.source_id == crate::panels::pagelist::PANEL))
            // The slumlord panel's element-message handler's two mode
            // writers and its own hidden edge. Its native object inherits `Panel`, so
            // a physical tab click changes one of these pages and the same object hears that
            // visibility edge; hiding the root retires its nine-unit range handler. The Rust
            // panel model lives behind `RemainingPanels`, so forward only its root and two pages.
            || (m.id == dereth_ui::msg::element::id::VISIBILITY_CHANGED
                && (m.source_id == crate::panels::slumlord::PANEL
                    || m.source_id == crate::panels::slumlord::BUY_PAGE
                    || m.source_id == crate::panels::slumlord::RENT_PAGE))
            || (m.id == dereth_ui::msg::element::id::LIST_ITEM_ACTIVATED
                && m.source_id == crate::panels::pagelist::LIST)
            // `0x04` from the Titles tab's own list box, keyed on its source for the reason the
            // page list's `0x43` is: setting the selected item raises a
            // `4` on every list in the tree and a blanket forward would put a great many of
            // them through a fan-out that wants one. It reaches
            // the character title panel's element-message handler's
            // "id 4 or `0x43` -> update the buttons" arm, which is how that panel
            // learns a row was picked at all -- the client has no `0x1C` arm of its own, so the
            // Titles tab does not transcribe the list box's press arm locally.
            //
            // `0x43` from the same list is here too, because the client's arm is
            // on id 4 or `0x43`, and `0x43` is what a press on the already-selected
            // title raises. It is idempotent -- `update_buttons` re-reads the same selection --
            // but a transcribed arm with no producer is exactly the defect class this filter
            // exists to avoid, and it costs one id.
            || ((m.id == dereth_ui::msg::element::id::LIST_SELECTION_CHANGED
                || m.id == dereth_ui::msg::element::id::LIST_ITEM_ACTIVATED)
                && m.source_id == crate::panels::titles::TITLE_LIST)
            // The same clause for `AllegiancePanel`'s vassal list and for exactly the
            // reason above: the client's message-`4` arm is what
            // reads `0x10000001` off the picked row as the selected vassal and enables the
            // Kick button, and without this line that arm has **no producer** — the list selects,
            // the highlight moves, and the panel never hears about it. Keyed on the one list id
            // rather than forwarded blanket, as the Titles note requires.
            || (m.id == dereth_ui::msg::element::id::LIST_SELECTION_CHANGED
                && m.source_id == crate::panels::allegiance::VASSAL_LIST)
            // The same pair from the Fellowship tab's own list box, keyed on its source for the
            // Titles tab's reason, and the entry box's `0x12`/`0x44`.
            // The fellowship panel's handler has `case 4: case 0x43:` (pick
            // a fellow -- which also sets the **world** selection) and `case 0x12: case 0x44:`
            // (a keystroke in the name box enables or disables the Create button). Both are
            // keyed on one element: `0x12` in particular is raised by every text element in the
            // tree, including the chat entry, and a blanket forward would put every character
            // the player types through a fan-out that wants one box.
            || ((m.id == dereth_ui::msg::element::id::LIST_SELECTION_CHANGED
                || m.id == dereth_ui::msg::element::id::LIST_ITEM_ACTIVATED)
                && m.source_id == crate::panels::fellowship::FELLOWS_LIST)
            || ((m.id == dereth_ui::msg::element::id::CHARACTER
                || m.id == dereth_ui::msg::element::id::TEXT_CHANGED)
                && m.source_id == crate::panels::fellowship::NAME_ENTRY_BOX)
            // The same two clauses for the Friends tab, keyed on its own list box and its own name
            // box for the Titles and Fellowship reason. The friends panel has `case 4: case 0x43:`
            // (a pick, which is what arms the Remove and Tell buttons) and `case 0x12: case 0x44:`
            // (a keystroke in the name box arms Add). Without these two lines both arms have **no
            // producer**: the row highlights and the panel never hears it.
            || ((m.id == dereth_ui::msg::element::id::LIST_SELECTION_CHANGED
                || m.id == dereth_ui::msg::element::id::LIST_ITEM_ACTIVATED)
                && m.source_id == crate::panels::friends::FRIENDS_LIST)
            || ((m.id == dereth_ui::msg::element::id::CHARACTER
                || m.id == dereth_ui::msg::element::id::TEXT_CHANGED)
                && m.source_id == crate::panels::friends::NAME_ENTRY_BOX)
            // The same two clauses for the Squelch tab, keyed on its own list box and its own name
            // box for the same reason. The squelch panel has `case 4: case 0x43:` (a pick, which is
            // the only thing that arms the Remove button) and `case 0x12: case 0x44:` (a keystroke
            // in the name box arms the two squelch buttons). Without these two lines both arms have
            // **no producer**: the row highlights and the panel never hears it, and Remove is
            // greyed for ever.
            || ((m.id == dereth_ui::msg::element::id::LIST_SELECTION_CHANGED
                || m.id == dereth_ui::msg::element::id::LIST_ITEM_ACTIVATED)
                && m.source_id == crate::panels::squelch::SQUELCH_LIST)
            || ((m.id == dereth_ui::msg::element::id::CHARACTER
                || m.id == dereth_ui::msg::element::id::TEXT_CHANGED)
                && m.source_id == crate::panels::squelch::NAME_ENTRY_BOX)
            // The same source-keyed text forwarding for `AbusePanel`'s two
            // entry boxes. The text-entry handling is reached by `case 0x12: case 0x44`
            // in the panel's element-message handler; it is the native producer that enables
            // Continue only after both the target name and complaint are nonempty.
            || ((m.id == dereth_ui::msg::element::id::CHARACTER
                || m.id == dereth_ui::msg::element::id::TEXT_CHANGED)
                && (m.source_id == crate::panels::abuse::NAME_ENTRY
                    || m.source_id == crate::panels::abuse::COMPLAINT_ENTRY))
            // The same clause for the Contracts tab, keyed on its own list box for the same reason.
            // The contracts panel's element-message handler runs `update_buttons` on id 4
            // and `check_for_double_click` then `update_buttons` on id `0x43`, and both are what tell the
            // panel a row was picked at all -- the client has no `0x1C` arm of its own. Without
            // this line the row highlights, the selected item moves, and the six detail fields
            // never change.
            || ((m.id == dereth_ui::msg::element::id::LIST_SELECTION_CHANGED
                || m.id == dereth_ui::msg::element::id::LIST_ITEM_ACTIVATED)
                && m.source_id == crate::panels::contracts::CONTRACTS_BOX)
            // The same clause for the Components panel's own list box, keyed on its source for the
            // same reason. The spell-component panel's element-message handler handles only ids `4`
            // and `0x2F` -- there is **no** `0x43` arm, so only the `4` is forwarded. Its `4` arm
            // selects the region's object for a row and selects object 0 for a header, and without
            // this line that arm has **no producer**: the row highlights and the world selection
            // never moves.
            || (m.id == dereth_ui::msg::element::id::LIST_SELECTION_CHANGED
                && m.source_id == crate::panels::spellcomponent::COMPONENT_LIST)
        {
            self.panel_messages.push(m.clone());
        }
    }

    /// The per-frame update queues `0x1000000A`; the end session paths
    /// queue `0x10000009` when [`Self::should_quit_on_logout`] is set.
    ///
    /// The client's two guards are what make a *confirmation* possible at all: only with an
    /// end-session request pending, and only if the ending-session latch was not already set (it is
    /// set now), and only when the log-out is confirmed, does it quit or go to character select.
    ///
    /// [`Self::ending_session`] latches on the first look and is cleared only by
    /// [`Self::close_logout_dialog`], and an **unconfirmed** logout falls straight through. The
    /// keyboard's route sets [`Self::logout_confirmed`] itself and never asks.
    fn is_game(&self) -> bool {
        true
    }

    fn on_game(&mut self, cx: &mut ScreenCx<'_>, g: &mut dereth_ui::framework::GameCx<'_>) -> bool {
        crate::screens::gameplay_host::on_game(self, cx, g)
    }

    /// The logout-confirmation dialog builder was handed a string in table enum
    /// `0x10000001`; the screen holds its symbolic id and the host hands over the table, for the
    /// same reason the disconnected screen's reason is resolved there.
    fn on_pregame(
        &mut self,
        cx: &mut ScreenCx<'_>,
        p: &dereth_ui::framework::PregameCx<'_>,
    ) -> Option<UiMode> {
        // The barber panel owns the same character-generation state and
        // preview viewport as the creation wizard, so it consumes the same already-loaded tables.
        if let Some(t) = p
            .tables
            .clone()
            .and_then(|t| std::rc::Rc::downcast::<crate::screens::chargen::CharGenTables>(t).ok())
        {
            self.barber.set_tables(t);
        }
        if self.needs_logout_prompt_text() {
            let id = self.logout_prompt.clone().unwrap_or_default();
            let resolved = p.client_strings.and_then(|t| {
                cx.ui
                    .resolve_string(t, dereth_primitives::num::hash::str_hash(id.as_bytes()))
            });
            let text = resolved.unwrap_or(id);
            self.show_logout_prompt(cx.ui, &text);
        }
        None
    }

    fn update(&mut self, cx: &mut ScreenCx<'_>, now: LocalTime) -> Option<UiMode> {
        let ui = &mut *cx.ui;
        // A virtual method the framework calls every frame, and
        // the *only* thing that keeps "Tell to <selected>" pointing at something a player can talk
        // to.
        let world = self.chat_auto_target_world.clone();
        self.chat_use_time(ui, now.0, &world, |id| Some(world.adopted(id)));
        if !self.do_end_session || self.ending_session {
            return None;
        }
        self.ending_session = true;
        if !self.logout_confirmed {
            return None;
        }
        // The **quit** branch, and the only one that queues a mode.
        if self.should_quit_on_logout {
            self.do_end_session = false;
            return Some(dereth_ui::framework::mode::EPILOGUE);
        }
        // **The non-quit branch, which queues nothing at all.**
        //
        // Returning `CHARACTER_MANAGEMENT` here, in the frame the player answered *Yes*, would snap
        // straight to character select, where the character should clap above their head,
        // dematerialize, and go back to portal space for a bit. Retail's branch has no
        // `queue_ui_mode(0x1000000A)` in it; it asks for a character log-off (not a quit) and
        // **leaves the player standing in the world** for the six seconds the shard takes to
        // answer. Everything the player sees in that window -- the emote the server broadcasts
        // (index `0x11E`), the world fade-out three seconds in and the portal tunnel behind it,
        // and a hide play script the moment a shard sends one -- plays in that time.
        //
        // What carries the flow to character select is the server's own `0xF658`, through the
        // framework's persistent-data character-set notice -> the UI flow's update ->
        // [`Screen::update`]'s caller.
        //
        // Contact bit (`transient_state & 1`) clear -- a player off the ground
        // is refused, **told why**, and the request is thrown away. Retail's no-world-view arm
        // (the end-session request left set under a latched ending-session flag, so nothing ever
        // happens again) is **not** modelled: it is unreachable from a screen a player is standing
        // on.
        //
        // Two things about the refusal:
        //
        // 1. It is not silent. It is a display-string notice on chat type `0x1A` with a wide
        //    literal -- see [`logout::AIRBORNE_REFUSAL`].
        //
        // 2. **The refusal cancels the log-off; it does not defer it.** The question is not
        //    asked again next frame, and the log-off does not go through the frame contact
        //    returns. The refusal block falls through into
        //    the same "clear the end-session request" the two `return` arms take. So retail
        //    clears the ending-session flag, which only
        //    un-latches the one-shot guard so the *next* press is heard, and clears
        //    the end-session request, which throws this press away. A player who presses log off
        //    mid-jump gets one line and stays in the world when he lands; he has to press again.
        if self.player_airborne {
            self.ending_session = false;
            self.logoff_refusals += 1;
            ui.requests.emit(UiRequest::DisplayChatText {
                channel: logout::AIRBORNE_REFUSAL_CHAT_TYPE,
                text: logout::AIRBORNE_REFUSAL.to_owned(),
            });
            // The client, reached by fall-through from the notice.
            self.do_end_session = false;
            return None;
        }
        self.do_end_session = false;
        ui.requests
            .emit(UiRequest::EndCharacterSession { ask: false });
        None
    }

    fn roots(&self) -> &[ElemHandle] {
        &self.roots
    }
}

/// Whether the page `element` (of the toolbar stack or the environment stack) belongs to a system
/// `features` lacks: the quest page without the journal, the secure-trade window without trade,
/// the salvage window without tinkering, the house purchase window without housing, and the chess
/// window without chess.
#[must_use]
pub fn era_lacks_page(features: dereth_primitives::EraFeatures, element: ElementId) -> bool {
    use crate::panels::{journal, minigame, salvage, slumlord, trade};
    match element {
        e if e == journal::PAGE => !features.journal,
        e if e == trade::WINDOW => !features.trade,
        e if e == salvage::PANEL => !features.tinkering,
        e if e == slumlord::PANEL => !features.housing,
        e if e == minigame::PANEL => !features.chess,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the recovered screen catalogue plus the shipped layout index — layout
    /// `0x21000005` (`classic_gameplay`, enum `0x10000006`) is the only shipped layout whose root
    /// element is `0x10000495`, which pins the otherwise omitted enum.
    #[test]
    fn the_screen_names_the_recovered_layout_enum_and_root() {
        assert_eq!(LAYOUT, LayoutEnum(0x1000_0006));
        assert_eq!(ROOT, ElementId(0x1000_0495));
        assert_eq!(CAMERA_SCALE, 1.1);
    }

    /// Oracle: §9's element-message table — the id test that decides which visibility changes are
    /// reported to the server.
    #[test]
    fn only_the_four_floaty_chat_windows_report_their_visibility() {
        assert!(reports_panel_visibility(ElementId(0x1000_0505)));
        for id in 0x1000_050E..=0x1000_0510 {
            assert!(reports_panel_visibility(ElementId(id)), "{id:#X}");
        }
        assert!(!reports_panel_visibility(ElementId(0x1000_050D)));
        assert!(!reports_panel_visibility(ElementId(0x1000_0511)));
        assert!(
            !reports_panel_visibility(ElementId(0x1000_0601)),
            "the main chat window does not"
        );
    }

    /// Oracle: the key press handling's four-row table.
    #[test]
    fn the_four_hot_keys_set_the_documented_flags() {
        let mut ui = UiSystem::new((800, 600));
        let mut s = GamePlayScreen {
            shown: true,
            ..Default::default()
        };

        s.handle_key_press(&mut ui, action::LOGOUT_TO_SELECT);
        assert!(!s.should_quit_on_logout && s.logout_confirmed && s.do_end_session);
        let _ = ui.requests.take();
        assert_eq!(
            s.update(
                &mut dereth_ui::framework::ScreenCx::new(&mut ui),
                LocalTime(0.0)
            ),
            None,
            "the non-quit branch queues no mode"
        );
        assert!(
            ui.requests
                .take()
                .contains(&UiRequest::EndCharacterSession { ask: false }),
            "it asks the host to log the character off instead"
        );
        assert!(!s.do_end_session, "and takes the request once");

        let mut s = GamePlayScreen {
            shown: true,
            ..Default::default()
        };
        s.handle_key_press(&mut ui, action::LOGOUT_AND_QUIT);
        assert!(s.should_quit_on_logout);
        assert_eq!(
            s.update(
                &mut dereth_ui::framework::ScreenCx::new(&mut ui),
                LocalTime(0.0)
            ),
            Some(dereth_ui::framework::mode::EPILOGUE)
        );
        assert_eq!(
            s.update(
                &mut dereth_ui::framework::ScreenCx::new(&mut ui),
                LocalTime(0.0)
            ),
            None,
            "the exit is queued once"
        );

        // Toggling UI visibility flips a flag, and an unknown action does nothing.
        let mut s = GamePlayScreen {
            shown: true,
            ..Default::default()
        };
        s.handle_key_press(&mut ui, action::TOGGLE_UI);
        assert!(!s.shown);
        s.handle_key_press(&mut ui, action::TOGGLE_UI);
        assert!(s.shown);
        s.handle_key_press(&mut ui, 0xDEAD);
        assert!(s.shown && !s.do_end_session);
    }

    /// A press off the ground is refused out loud once and thrown away.
    #[test]
    fn a_press_off_the_ground_is_refused_out_loud_once_and_thrown_away() {
        let mut ui = UiSystem::new((800, 600));
        let mut s = GamePlayScreen {
            shown: true,
            player_airborne: true,
            ..Default::default()
        };
        s.handle_key_press(&mut ui, action::LOGOUT_TO_SELECT);
        assert!(s.do_end_session);

        let _ = ui.requests.take();
        assert_eq!(
            s.update(
                &mut dereth_ui::framework::ScreenCx::new(&mut ui),
                LocalTime(0.0)
            ),
            None,
            "no mode is ever queued here"
        );
        assert_eq!(s.logoff_refusals, 1);
        assert!(
            !s.do_end_session,
            "the refusal path consumes the pending press"
        );
        assert!(
            !s.ending_session,
            "the refusal path unlatches the guard for a later press"
        );
        let said = ui.requests.take();
        assert!(
            said.contains(&UiRequest::DisplayChatText {
                channel: 0x1A,
                text: "Cannot log off while in mid-air.".to_owned(),
            }),
            "the player is told why: {said:?}"
        );
        assert!(
            !said
                .iter()
                .any(|r| matches!(r, UiRequest::EndCharacterSession { .. })),
            "and nothing was asked of the host: {said:?}"
        );

        // Another look, still airborne, with no fresh press: `do_end_session` is clear, so
        // `UseTime` returns and there is no second line.
        assert_eq!(
            s.update(
                &mut dereth_ui::framework::ScreenCx::new(&mut ui),
                LocalTime(0.0)
            ),
            None
        );
        assert_eq!(s.logoff_refusals, 1, "one press, one refusal");
        assert!(ui.requests.take().is_empty(), "and no second line");

        // He lands. Nothing was deferred, so nothing happens until he presses again.
        s.player_airborne = false;
        assert_eq!(
            s.update(
                &mut dereth_ui::framework::ScreenCx::new(&mut ui),
                LocalTime(0.0)
            ),
            None
        );
        assert!(
            ui.requests.take().is_empty(),
            "landing alone logs nobody off"
        );

        // The second press, on the ground, goes through.
        s.handle_key_press(&mut ui, action::LOGOUT_TO_SELECT);
        assert_eq!(
            s.update(
                &mut dereth_ui::framework::ScreenCx::new(&mut ui),
                LocalTime(0.0)
            ),
            None
        );
        assert!(
            ui.requests
                .take()
                .contains(&UiRequest::EndCharacterSession { ask: false }),
            "the guard was un-latched, so the next press is heard"
        );
        assert_eq!(s.logoff_refusals, 1);
    }

    /// Exit game is not gated by the ground and says nothing.
    #[test]
    fn exit_game_is_not_gated_by_the_ground_and_says_nothing() {
        let mut ui = UiSystem::new((800, 600));
        let mut s = GamePlayScreen {
            shown: true,
            player_airborne: true,
            ..Default::default()
        };
        s.handle_key_press(&mut ui, action::LOGOUT_AND_QUIT);
        let _ = ui.requests.take();
        assert_eq!(
            s.update(
                &mut dereth_ui::framework::ScreenCx::new(&mut ui),
                LocalTime(0.0)
            ),
            Some(dereth_ui::framework::mode::EPILOGUE),
            "exit mode is reached even while the body is in mid-air"
        );
        assert_eq!(s.logoff_refusals, 0);
        assert!(ui.requests.take().is_empty(), "and no refusal line");
    }

    /// Oracle: §9's element-message table — a floaty chat window's visibility change becomes a
    /// set-panel-visibility notice carrying the *element id* as the panel id.
    #[test]
    fn a_floaty_chat_visibility_change_is_reported_with_its_element_id() {
        let mut ui = UiSystem::new((800, 600));
        let mut s = GamePlayScreen::default();
        let msg = |id: u32, visible: u32| ElementMessage {
            source_id: ElementId(id),
            source: ElemHandle::for_test(1),
            id: dereth_ui::msg::element::id::VISIBILITY_CHANGED,
            p1: visible,
            p2: 0,
            point: dereth_ui::msg::MessagePoint::default(),
            serial: 1,
        };
        s.on_element_message(
            &mut dereth_ui::framework::ScreenCx::new(&mut ui),
            &msg(0x1000_050F, 1),
        );
        s.on_element_message(
            &mut dereth_ui::framework::ScreenCx::new(&mut ui),
            &msg(0x1000_0601, 1),
        );
        assert_eq!(
            ui.requests.take(),
            vec![UiRequest::SetPanelVisibility {
                panel: 0x1000_050F,
                visible: true
            }]
        );
    }

    /// Every start visibility row names the mechanism that puts it there.
    #[test]
    fn every_start_visibility_row_names_the_mechanism_that_puts_it_there() {
        let rows = GamePlayScreen::HUD_START_VISIBILITY;
        assert_eq!(rows.len(), 17);

        // No element appears twice: two rows disagreeing would make the order load-bearing.
        let mut ids: Vec<u32> = rows.iter().map(|r| r.element.0).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before, "an element is listed twice");

        let from_the_layout = [
            window::MAIN_CHAT,
            ElementId(0x1000_0505),
            ElementId(0x1000_050E),
            ElementId(0x1000_050F),
            ElementId(0x1000_0510),
            window::KEYBOARD,
            window::ADMIN,
            window::ENV_PANEL,
        ];
        for r in rows {
            assert!(!r.why.is_empty());
            assert!(
                !r.why.contains("#223"),
                "{:#010X}: {:?}",
                r.element.0,
                r.why
            );
            assert!(
                !r.why.contains("UNVERIFIED"),
                "{:#010X}: {:?}",
                r.element.0,
                r.why
            );
            assert_eq!(
                r.why.starts_with("layout:"),
                from_the_layout.contains(&r.element),
                "{:#010X}: {:?}",
                r.element.0,
                r.why
            );
        }

        // The six windows the retail session shows are the six not switched off here: the table
        // turns eleven things off and two on, and the four it never mentions — `<SBOX>`,
        // `<TBAR>`, `<INDI>`, `<RADA>` — are the ones the layout already has up.
        let off: Vec<u32> = rows
            .iter()
            .filter(|r| !r.visible)
            .map(|r| r.element.0)
            .collect();
        assert_eq!(off.len(), 15);
        for id in [
            window::SMART_BOX,
            window::TOOLBAR,
            window::RADAR,
            ElementId(0x1000_0611),
        ] {
            assert!(!off.contains(&id.0), "{:#010X} must stay visible", id.0);
        }
        assert!(rows
            .iter()
            .any(|r| r.element == window::MAIN_CHAT && r.visible));
        assert!(rows
            .iter()
            .any(|r| r.element == window::STACKED_VITALS && r.visible));
    }

    /// Setting up a page group hides every page and the stack with them.
    #[test]
    fn setting_up_a_page_group_hides_every_page_and_the_stack_with_them() {
        let mut ui = UiSystem::new((800, 600));
        let stack = ui.create_hollow(None);
        let pages: Vec<ElemHandle> = crate::panels::catalogue::PANEL_PAGES
            .iter()
            .map(|_| ui.create_hollow(Some(stack)))
            .collect();
        for h in &pages {
            assert!(
                ui.node(*h).unwrap().region.flags.visible,
                "a fresh element starts visible"
            );
        }

        // The synthetic pages carry none of the documented ids, so bind them by hand and then run
        // only the part under test: the hide loop and the stack's own visibility.
        let mut s = PanelStack {
            pages: pages
                .iter()
                .enumerate()
                .map(|(i, h)| crate::panels::panel_stack::PageInfo {
                    element: ElementId(crate::panels::catalogue::PANEL_PAGES[i]),
                    handle: *h,
                    panel_id: u32::try_from(i).unwrap() + 1,
                    transient: false,
                })
                .collect(),
            ..PanelStack::default()
        };
        for p in &s.pages {
            ui.set_visible(p.handle, false);
        }
        s.root = Some(stack);
        ui.set_visible(stack, s.current.is_some());

        assert!(
            !ui.node(stack).unwrap().region.flags.visible,
            "no page current, no stack"
        );
        for h in &pages {
            assert!(!ui.node(*h).unwrap().region.flags.visible);
        }

        // Opening one page shows that page and the stack, and nothing else.
        s.recv_set_panel_visibility(&mut ui, 3, true);
        assert!(ui.node(stack).unwrap().region.flags.visible);
        for (i, h) in pages.iter().enumerate() {
            assert_eq!(
                ui.node(*h).unwrap().region.flags.visible,
                i == 2,
                "page {i}"
            );
        }
        // Closing it puts both back.
        s.recv_set_panel_visibility(&mut ui, 3, false);
        assert!(!ui.node(stack).unwrap().region.flags.visible);
        assert!(pages
            .iter()
            .all(|h| !ui.node(*h).unwrap().region.flags.visible));
    }

    /// Oracle: the floaty vitals panel's update from the player module (`SetVisible(!sideBySide)`)
    /// and the floaty side vitals panel's update from the player module (`SetVisible(sideBySide)`),
    /// and the recovered HUD behavior for the placement blob: **visibility is read unconditionally,
    /// position and size only when the layout did not come from file**.
    #[test]
    fn the_player_module_decides_visibility_always_and_position_only_without_a_layout_file() {
        use crate::hud::floaty::WindowPlacement;

        // Two windows: one that reads `0x1000008A` (the toolbar) and one that does not (the
        // examination panel), so the class test is exercised in both directions.
        let mut placements = WindowPlacements::default();
        placements.set(
            10,
            WindowPlacement {
                x: Some(11),
                y: Some(22),
                w: Some(120),
                h: Some(60),
                visible: Some(false),
                title: None,
            },
        );
        placements.set(
            7,
            WindowPlacement {
                visible: Some(false),
                ..WindowPlacement::default()
            },
        );

        let mut ui = UiSystem::new((800, 600));
        let mut s = GamePlayScreen::default();
        let root = ui.create_hollow(None);
        s.roots.push(root);
        let tbar = ui.create_hollow(Some(root));
        let exam = ui.create_hollow(Some(root));
        // The recursive child lookup matches on the element id, so the synthetic children have to
        // carry the real ones.
        ui.node_mut(tbar).unwrap().desc.element_id = window::TOOLBAR;
        ui.node_mut(exam).unwrap().desc.element_id = window::EXAMINATION;
        s.window_ids = vec![(window::TOOLBAR, 10), (window::EXAMINATION, 7)];

        let pm = PlayerSettingsView {
            placements,
            side_by_side_vitals: false,
            lock_ui: false,
            ..PlayerSettingsView::default()
        };
        assert_eq!(
            s.update_from_player_module(&mut ui, &pm),
            1,
            "only the toolbar reads 0x1000008A"
        );
        assert!(!ui.node(tbar).unwrap().region.flags.visible);
        assert!(
            ui.node(exam).unwrap().region.flags.visible,
            "the examination window never reads placement visibility"
        );
        // With no layout file the rectangle is applied, resize before move.
        let b = ui.node(tbar).unwrap().region.box_;
        assert_eq!((b.x0, b.y0, b.width(), b.height()), (11, 22, 120, 60));

        // With one, the server's rectangle is ignored and the visibility is not.
        let mut s2 = GamePlayScreen {
            layout_from_file: true,
            ..GamePlayScreen::default()
        };
        s2.roots.push(root);
        s2.window_ids = s.window_ids.clone();
        ui.move_to(tbar, 500, 500);
        ui.resize_to(tbar, 40, 40);
        ui.set_visible(tbar, true);
        assert_eq!(s2.update_from_player_module(&mut ui, &pm), 1);
        assert!(
            !ui.node(tbar).unwrap().region.flags.visible,
            "visibility still applies"
        );
        let b = ui.node(tbar).unwrap().region.box_;
        assert_eq!(
            (b.x0, b.y0),
            (500, 500),
            "a local layout file wins over the server rectangle"
        );
    }

    /// Oracle: the recovered HUD behavior's table and `hud::floaty::READS_PLACEMENT_VISIBILITY`,
    /// which was read from all ten floating-window player-state update bodies: exactly five
    /// mention `0x1000008A`.
    #[test]
    fn exactly_five_window_classes_take_their_visibility_from_the_server() {
        let five = crate::hud::floaty::READS_PLACEMENT_VISIBILITY;
        assert_eq!(five.len(), 5);
        let named: Vec<&str> = GAMEPLAY_WINDOWS
            .iter()
            .filter(|w| five.contains(&w.class))
            .map(|w| w.class)
            .collect();
        // `<CHAT>` plus the four `<FCHn>` plus `<INDI>`, `<PBAR>` and `<TBAR>` — eight windows
        // across five classes.
        assert_eq!(named.len(), 8);
        for c in five {
            assert!(
                GAMEPLAY_WINDOWS.iter().any(|w| w.class == c),
                "{c} names no window"
            );
        }
        // And the three that call `SetVisible` from somewhere else are not in it.
        for c in [
            "FloatingExamination",
            "FloatingEnvironmentStack",
            "FloatingCombatStack",
        ] {
            assert!(
                !five.contains(&c),
                "{c} does not read Option_Placement_Visibility"
            );
        }
    }

    /// Oracle: the recovered chat behavior — "`windowId != 0` targets one window;
    /// `windowId == 0` broadcasts subject to the per-window 64-bit filter", and §4's default
    /// filters, which are what make the main window reject the over-head-bubble type `0x1A`.
    #[test]
    fn a_broadcast_line_reaches_the_windows_whose_filter_accepts_its_type() {
        use crate::chat::interface::window as w;
        let mut ui = UiSystem::new((800, 600));
        let mut s = GamePlayScreen {
            chat: vec![
                ChatInterface::new(w::MAIN),
                ChatInterface::new(w::FLOATY_1),
                ChatInterface::new(w::FLOATY_2),
                ChatInterface::new(w::FLOATY_3),
                ChatInterface::new(w::FLOATY_4),
            ],
            chat_windows: vec![crate::chat::window::ChatWindow::default(); 5],
            ..GamePlayScreen::default()
        };

        // A system line broadcasts and the main window takes it.
        let sys = ChatMessage {
            // `LogTextType` 5 = System (the recovered chat and social behavior).
            ty: 5,
            body: "Welcome to Asheron's Call".into(),
            prefix: None,
            window: 0,
        };
        let took = s.recv_display_final_string_info(&mut ui, &sys);
        assert!(took.contains(&w::MAIN), "the main window takes System");
        assert_eq!(s.chat[0].log_text(), "Welcome to Asheron's Call");

        // Type 0x1A is the over-head bubble channel and the main window's default filter masks it
        // out (`0xFBFFFFFF`).
        let bubble = ChatMessage {
            ty: 0x1A,
            body: "hi".into(),
            prefix: None,
            window: 0,
        };
        assert!(!s
            .recv_display_final_string_info(&mut ui, &bubble)
            .contains(&w::MAIN));

        // A line addressed to floaty 2 goes only there, filter or no filter.
        let direct = ChatMessage {
            ty: 0x1A,
            body: "tell".into(),
            prefix: None,
            window: w::FLOATY_2,
        };
        assert_eq!(
            s.recv_display_final_string_info(&mut ui, &direct),
            vec![w::FLOATY_2]
        );
    }
}
