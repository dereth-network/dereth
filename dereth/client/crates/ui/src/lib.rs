//! The game-agnostic engine UI: the element model, the description system, the event channels, text
//! editing, dialogs, persistence and the mode machine.
//!
//! **Depends on** `dereth-primitives`, the decoded layouts and fonts of `dereth-assets`, the
//! shared text (`dereth-text`), the contract (`dereth-client-contract`) and the device input's
//! event types (`dereth-input`). **Used
//! by** the retail screens (`dereth-ui-screens`), the client and its test kit.
//!
//! **Must never** hold a concrete screen or panel (those are `dereth-ui-screens`'), put pixels on
//! the screen (the renderer's) or reach the client runtime (`cargo xtask seams`, `seam: client
//! crates`).
//!
//! **There is no per-frame walk of the element tree.** [`UiSystem::use_time`] runs a fixed
//! seven-step sequence, one step of which broadcasts [global message 3](msg::global::TICK) to
//! whoever has registered for it, so a static screen does zero work per frame. The same message
//! drives [`UiFlow`]'s screen switch, and the transition must run **last**,
//! because [`use_new_mode`](framework::UiFlow::use_new_mode) destroys the current screen and every
//! open dialog with it.
//!
//! Reading order: [`region`] → [`element`] → [`desc`] → [`factory`] → [`props`] → [`msg`] →
//! [`focus`] → [`media`] → [`widgets`] → [`text`] → [`dialog`] → [`persist`] → [`framework`].

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use dereth_primitives::num::rng::Ran2;
use dereth_primitives::{DataId, LocalTime};

pub mod desc;
pub mod dialog;
pub mod element;
pub mod env;
pub mod factory;
pub mod focus;
pub mod framework;
pub mod layout;
pub mod media;
pub mod msg;
pub mod persist;
pub mod props;
pub mod region;
pub mod scrollable;
pub mod text;
pub mod widgets;

pub use desc::{DescLibrary, ElementDesc, LayoutDesc, PropertyTypes, StateDesc};
pub use element::{
    CallbackLoseFocusResult, ElemCtx, Element, ElementFlags, ElementMessageListenResult,
    ElementNode, PlainElement,
};
pub use factory::ElementCtor;
pub use focus::InputEvent;
pub use framework::{LayoutEnum, LayoutEnumResolver, Screen, TableResolver, UiFlow, UiMode};
pub use layout::{BorderLocation, EdgeMode, Edges, SizeClamps};
pub use media::MediaEffect;
pub use msg::{Delivery, ElementMessage, ListenerId, NoticeId, NoticePayload};
pub use props::{PropertyCollection, PropertyValue};
pub use region::{
    BlitMode, Box2D, DrawEvent, DrawStep, GraphicRef, ImageSource, Region, RegionFlags, UiFill,
};

// ---------------------------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------------------------

macro_rules! ui_id {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
        pub struct $name(pub u32);
        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, concat!(stringify!($name), "(0x{:X})"), self.0)
            }
        }
    };
}

/// Unique **inside a layout**, and the key of the whole message system.
///
/// The definition is [`dereth_client_contract::ids`]'s, because `UiRequest::DropTarget`
/// carries one and the contract crate may not depend on this one. It is the same `ui_id!`
/// expansion, hex `Debug` included, re-exported at this path. The other four
/// `ui_id!` types below live here: the contract names none of them.
pub use dereth_client_contract::ids::ElementId;
ui_id!(
    /// The element type. **0 means "partial"** — inherit from the base element — so 0 is not a legal
    /// widget type id.
    ElementType
);
ui_id!(
    /// The UI state id. 0 is the element's own base state.
    StateId
);
ui_id!(
    /// An element-message or global-message id. The two spaces are separate.
    MessageId
);

/// A generational index into the element arena.
///
/// Twelve bits of generation and twenty of index, which caps the arena at 1 048 575 live elements —
/// four hundred times the 2 162 descriptions the retail dat ships.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ElemHandle(u32);

impl ElemHandle {
    const INDEX_BITS: u32 = 20;
    const INDEX_MASK: u32 = (1 << Self::INDEX_BITS) - 1;

    #[must_use]
    pub const fn index(self) -> usize {
        (self.0 & Self::INDEX_MASK) as usize
    }

    #[must_use]
    pub const fn generation(self) -> u32 {
        self.0 >> Self::INDEX_BITS
    }

    const fn new(index: u32, generation: u32) -> Self {
        Self((generation << Self::INDEX_BITS) | (index & Self::INDEX_MASK))
    }

    /// A handle with generation 0, for tests that never touch an arena.
    #[must_use]
    pub const fn for_test(index: u32) -> Self {
        Self::new(index, 0)
    }

    /// The handle a message parameter carries back.
    ///
    /// Element-message parameters are `u32`s and several messages put a handle in one — the client
    /// puts a raw pointer there (`p1` is a pointer to the drag-and-drop record on message
    /// `0x15`). `UiSystem::node` validates the generation, so a stale value is a `None` and not a
    /// wrong element.
    #[must_use]
    pub const fn from_raw(v: u32) -> Self {
        Self(v)
    }

    /// The inverse: the `u32` a message parameter carries. Menu selection puts the selected-item
    /// handle in `p2` this way, and it is the only thing that
    /// distinguishes one row of a menu from another — every row of one menu is built from the
    /// same description and so shares one element id.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl std::fmt::Debug for ElemHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Elem#{}v{}", self.index(), self.generation())
    }
}

// ---------------------------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------------------------

/// Everything this crate can refuse to do.
#[derive(Debug, thiserror::Error)]
pub enum UiError {
    #[error("layout {did}: {reason}")]
    Layout { did: DataId, reason: String },
    #[error("description inheritance cycle at layout {layout} element {element:?}")]
    DescCycle { layout: DataId, element: ElementId },
    #[error("layout {layout} has no element {element:?} to inherit from")]
    MissingBase { layout: DataId, element: ElementId },
    #[error("layout {did} has no root element {element:?}")]
    MissingRoot { did: DataId, element: ElementId },
    /// The whole subtree is dropped, exactly as the client's recursive full-description create does. This is
    /// only an error when it happens to the *root*; for a child it is logged and skipped.
    #[error("no factory registered for element type {ty:?} (nor engine type {engine_ty:?})")]
    UnknownElementType {
        ty: ElementType,
        engine_ty: ElementType,
    },
    #[error("layout enum {0:?} has no DataID in the resolver")]
    UnresolvedLayoutEnum(LayoutEnum),
    #[error("persistence: {0}")]
    Persist(String),
}

/// `dereth-client-contract`'s persistence parsers refuse in their own type, because a crate whose one
/// dependency is `dereth-primitives` cannot name [`UiError`] — `Layout`, `DescCycle` and their siblings
/// carry this crate's own `ElementType` and `LayoutEnum`.
///
/// The conversion is the identity on the message, so a `?` produces
/// `UiError::Persist(msg)` and `Display` prints the same line.
impl From<dereth_client_contract::persist::PersistError> for UiError {
    fn from(e: dereth_client_contract::persist::PersistError) -> Self {
        Self::Persist(e.0)
    }
}

// ---------------------------------------------------------------------------------------------
// The arena
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Default)]
struct Slot {
    generation: u32,
    node: Option<ElementNode>,
}

/// One element the factory dropped because no class is registered for its type.
///
/// Every drop is logged, because otherwise a missing widget looks like a layout bug.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DroppedSubtree {
    pub element_id: ElementId,
    pub ty: ElementType,
    pub engine_ty: ElementType,
    /// How many descriptions went with it, including the node itself.
    pub descendants: usize,
}

/// What a region asks the renderer to blit. The `RenderBackend` in `dereth-primitives` is the world
/// renderer's mesh seam; the 2D UI path needs a different shape, and this is it. The renderer's
/// `dereth_render::ui` turns one of these into a quad.
#[derive(Debug, Clone, PartialEq)]
pub struct UiDrawCmd {
    pub who: ElemHandle,
    /// Absolute screen coordinates, inclusive.
    pub screen: Box2D,
    /// The screen clip box; never invalid when a command is emitted.
    pub clip: Box2D,
    pub image: Option<DataId>,
    /// The pixel operation the image is shown *through*, when it is a runtime derivation rather
    /// than the dat picture. Appearance color spots and composed icons are the only two producers
    /// in the shipped screens. See [`region::SurfaceOp`].
    pub image_op: Option<region::SurfaceOp>,
    /// Which files [`Self::image`] is read from.
    pub image_source: ImageSource,
    pub blit_mode: BlitMode,
    pub alpha_blend_mod: f32,
    pub tiling_offset: (i32, i32),
    /// The element's **z** rotation in integer degrees, and
    /// `0` for everything the shipped client draws. The UI tiling unit.
    ///
    /// This carries the client's third argument to the renderer,
    /// which is the third conjunct of the client's
    /// `vw == pw && vh == ph && rotation is identity` and therefore selects
    /// the blit's sampler (`dereth_render::ui::pixel_rules::ui_surface_sampler`).
    ///
    /// **Nothing in this crate ever sets it, and nothing in the original client does.** Exhaustive
    /// call-site review found no rotation producer, and there is no region or element rotation
    /// setter or rotation attribute. The field exists
    /// so the conditional the renderer evaluates is the client's **whole** conditional rather
    /// than one with a term wired to a constant, and so that arm can be driven by a test instead
    /// of only reasoned about. See `ui_tiling.rs`.
    pub rotation_z_degrees: i32,
    /// `D3DRS_TEXTUREFACTOR`, which is what the UI material modulates the surface by. The renderer
    /// packs it from the material layer's diffuse colour × the colour modifier; the region supplies
    /// RGB and the alpha blend modifier supplies alpha. The packed value uses ARGB order.
    pub color: u32,
    /// The glyphs this element composes over its own blit, in draw order — see [`text::compose`].
    ///
    /// Empty for all but `TextElement` and its subclasses. A `Vec` that is empty does not
    /// allocate, so carrying it on every command costs nothing.
    pub glyphs: Vec<text::PlacedGlyph>,
    /// The current outline colour when this element's attribute `0x21` (`UICore_Text_outline`) is set,
    /// and `None` otherwise — see [`element::Element::text_outline_color`].
    ///
    /// When it is `Some`, the renderer draws [`Self::glyphs`] **twice**: an outline pass in this
    /// colour first, then the ordinary foreground pass, which is `TextElement`'s own two-pass
    /// glyph loop. `None` for every element that is not a text element, and for the 338 shipped
    /// text elements that do not ask for it.
    pub text_outline: Option<u32>,
    /// The rectangles this element's foreground glyph pass **inverts** —
    /// the colour-bit inversion, one per selected glyph, already intersected with
    /// the element's visible box exactly as the text element's draw intersects each
    /// cell with its surface window. Absolute screen coordinates, inclusive far edge.
    ///
    /// This is the client's second surface-window pixel primitive; without it a selected text
    /// box draws identically to an unselected one.
    /// Empty for every element that is not a text element and for every text element whose
    /// selecting bit (0x80) is clear.
    ///
    /// A `Vec` that is empty does not allocate, so carrying it on every command costs nothing —
    /// the same argument [`Self::glyphs`] makes.
    pub invert: Vec<Box2D>,
    /// The flat-colour rectangles this element put in its own UI surface, in the order the client
    /// writes them: step 2's self-erase first, then whatever the widget generated.
    ///
    /// Without these the base erase operation would have no counterpart in the draw path and
    /// the radar's blip draw, which writes blips pixel by pixel into a UI surface,
    /// could not be expressed at all. Both use the same flat-fill primitive in the client, and this
    /// is that one primitive. Coordinates are **absolute screen** by the time they reach here.
    pub fills: Vec<UiFill>,
}

/// The client's draw-mode switch. This is deliberately separate from
/// [`BlitMode::from_u32`]: layout attribute values use that enum's zero-based numbering, while
/// media descriptors use retail's 2/3 spellings for the two alpha paths.
const fn media_image_blit_mode(draw_mode: u32) -> BlitMode {
    match draw_mode {
        2 => BlitMode::Alpha3,
        3 => BlitMode::Alpha4,
        _ => BlitMode::Normal,
    }
}

/// The 2D blit seam, carrying the blit mode, alpha blend modifier, and tiling offset.
pub trait UiDrawBackend {
    fn draw_region(&mut self, cmd: &UiDrawCmd);
}

/// A backend that records instead of drawing, so the draw order can be asserted with no GPU.
#[derive(Debug, Default)]
pub struct RecordingDrawBackend {
    pub calls: Vec<UiDrawCmd>,
}

impl UiDrawBackend for RecordingDrawBackend {
    fn draw_region(&mut self, cmd: &UiDrawCmd) {
        self.calls.push(cmd.clone());
    }
}

/// The material layer's diffuse colour for the generated UI material: white rgb, the alpha blend
/// modifier in the alpha.
///
/// Material creation sets the diffuse colour to white `(1,1,1,1)`, and the alpha-blend setter writes the
/// opacity into its alpha; nothing in the shipped
/// screens calls the material-colour setter, so the rgb stays white.
#[must_use]
fn diffuse_color(alpha_blend_mod: f32) -> u32 {
    let a = dereth_primitives::num::to_i32(alpha_blend_mod.clamp(0.0, 1.0) * 255.0).clamp(0, 255);
    #[allow(clippy::cast_sign_loss)]
    // LINT-OK: the byte field store that follows the clamped conversion above.
    let a = a as u32;
    0x00FF_FFFF | (a << 24)
}

/// The dat, as a host service — [`UiSystem::assets`].
///
/// `dereth_primitives::AssetSource` carries no `Debug` bound and [`UiSystem`] derives `Debug`, so the
/// service seam is this one-method trait rather than `AssetSource` itself. Every `AssetSource`
/// that can print itself is one, automatically.
pub trait LayoutAssets: std::fmt::Debug {
    fn as_asset_source(&self) -> &dyn dereth_primitives::AssetSource;
}

impl<T: std::fmt::Debug + dereth_primitives::AssetSource> LayoutAssets for T {
    fn as_asset_source(&self) -> &dyn dereth_primitives::AssetSource {
        self
    }
}

/// The input seam is [`dereth_client_contract::input`]'s, so that
/// `dereth_client::input::InputShell` can implement it: with the type in one crate and the
/// trait in another, the impl would have nowhere legal to live. Both resolve here too.
pub use dereth_client_contract::input::{InputPump, NullInputPump};

// ---------------------------------------------------------------------------------------------
// UiSystem
// ---------------------------------------------------------------------------------------------

/// The UI manager: the singleton that owns the tree, factories, listener tables, mouse, tooltip
/// machine, drag-and-drop, focus, and activation.
#[derive(Debug)]
pub struct UiSystem {
    slots: Vec<Slot>,
    free: Vec<u32>,
    /// The root element — a hollow element covering the whole screen.
    root: ElemHandle,
    /// Every live element, in creation order.
    element_list: Vec<ElemHandle>,
    /// Elements queued for deletion.
    delete_queue: Vec<ElemHandle>,
    /// The per-element-type constructor table.
    factories: BTreeMap<ElementType, ElementCtor>,
    /// The layout store and the description cache.
    pub lib: DescLibrary,
    /// The `MasterProperty` id→type table; without it no layout can be decoded.
    pub property_types: PropertyTypes,
    /// The `MasterProperty` row defaults, keyed by property id -- each property
    /// description's default value.
    ///
    /// **The element's enum attribute getter reads them.** When the element's property
    /// query finds nothing on the element, the second arm builds a property
    /// by assigning the property name, looking up its description, and initializing from that
    /// description's default; only a row with no default falls through to zero.
    /// `0x5E UICore_ListBox_item_selected_state` defaults to **6**, and that default is the whole
    /// reason a pressed Friends row highlights.
    ///
    /// Filled by [`UiSystem::install_master`]. A build that assigns `property_types` alone reads
    /// every default as absent.
    pub property_defaults: BTreeMap<u32, PropertyValue>,
    /// The live display width and height.
    display: (i32, i32),

    /// The element listener table: element_id → message_id → listeners.
    element_listeners: BTreeMap<(ElementId, MessageId), Vec<msg::element::ListenerReg>>,
    /// The global-message listener table.
    global_listeners: BTreeMap<MessageId, Vec<msg::element::ListenerReg>>,
    /// The client's global event handler's notice table. Owned here so a rebuild has one bus.
    pub notices: msg::notice::NoticeBus,
    /// The monotonically increasing message serial-number source.
    serial: u32,
    /// The last-listened serial number, per listener.
    serial_seen: BTreeMap<ListenerId, u32>,
    /// Whether a message broadcast is in flight.
    broadcasting: bool,
    /// How many behaviour objects are currently lifted out of their arena slots.
    lifted: u32,
    /// State changes requested from inside a widget callback; see [`Self::queue_set_state`].
    deferred_states: Vec<(ElemHandle, StateId)>,
    /// [`Self::update_mouse_visibility`] calls raised from inside a widget's **own** callback; see
    /// [`Self::update_mouse_visibility`].
    deferred_mouse_visibility: Vec<ElemHandle>,
    /// Focus relinquishes raised from inside a widget's **own** callback; see
    /// [`Self::queue_relinquish_focus`].
    deferred_relinquish: Vec<ElemHandle>,
    /// Deferred unregistrations.
    removals: Vec<msg::element::Removal>,
    /// Deliveries to listeners this crate does not own; see [`msg::Delivery`].
    outbox: Vec<Delivery>,

    // ---- mouse / hover / drag / focus -------------------------------------------------------
    pub(crate) mouse: focus::MouseState,
    /// Element messages a handler asked to raise once the broadcast in flight has unwound —
    /// [`Self::queue_element_message`]. Oldest first.
    pub(crate) queued_messages: Vec<(ElemHandle, MessageId, u32, u32)>,
    /// Visibility sets a handler asked for once the broadcast in flight has unwound —
    /// [`Self::queue_set_visible`]. Oldest first, and the same hazard as `queued_messages`.
    pub(crate) queued_visibility: Vec<(ElemHandle, bool)>,
    pub(crate) tooltip: dialog::tooltip::TooltipState,
    /// `DialogController`'s statics — the per-queue open dialog, the waiting queues, the all-at-once
    /// list and the global context counter.
    ///
    /// **It lives on `UiSystem`, not on `UiFlow`.** A [`crate::Screen`] is handed a
    /// `&mut UiSystem` and never the `UiFlow`, so a factory owned by the flow would be out of
    /// reach of the screens that raise dialogs.
    ///
    /// The client settles the ownership question outright: `DialogController` is not a member of any
    /// framework. Its state is **static** — the global context counter, the per-queue open map
    /// and the queue table — like the element-manager singleton. Dialog creation reaches the
    /// element manager through that singleton, and the UI framework's destructor resets the
    /// factory rather than owning its state. `UiSystem` has that manager role in this build, so this
    /// is where the statics belong.
    ///
    /// `UiFlow::use_new_mode` still resets the dialog factory during a mode switch, which is the
    /// behaviour that must not change: every open dialog dies with the framework.
    pub dialogs: dialog::DialogController,

    pub(crate) drag: focus::DragState,
    /// The focused element.
    pub(crate) focus_element: Option<ElemHandle>,
    /// The active element — the *root* element that owns input.
    pub(crate) active_element: Option<ElemHandle>,
    /// The activatable elements, for [`Self::activate_next`].
    pub(crate) activatable: Vec<ElemHandle>,
    /// The element input-action listener table — property 0x57.
    pub(crate) input_action_listeners: BTreeMap<u32, Vec<ElemHandle>>,
    /// The default cursor id and hotspot -- the cursor the manager falls back to when no element
    /// supplies one. Only a `SetCursor` whose make-default argument is true writes it, which is why an element's override cannot
    /// overwrite it: the cursor check passes `false` for an element and `true` only for the default.
    pub default_cursor: Option<(DataId, i32, i32)>,
    /// The last cursor id and hotspot -- what was last handed to
    /// the device cursor setter. The client compares all three and returns early
    /// when they match, so a redundant push never reaches the device.
    pub last_cursor: Option<(DataId, i32, i32)>,
    /// The push `SetCursor` let through and the host has not applied yet, taken with
    /// [`UiSystem::take_pending_cursor`].
    ///
    /// This crate has no device. Setting a cursor loads the qualified image and passes it with the
    /// hotspot to the device, which needs
    /// both a dat and a window; the manager therefore records the push and the client performs it.
    pub(crate) pending_cursor: Option<(DataId, i32, i32)>,

    /// The input manager's shift-key-down answer as of the last [`Self::use_time`]; see
    /// [`InputPump::shift_key_down`].
    pub(crate) shift_key_down: bool,

    /// **The system clipboard, which is one buffer and not one per text box.**
    ///
    /// The text element sends selected text to, and retrieves paste text from, **free functions of
    /// the device layer**, so what one element copies is what the next element pastes. A
    /// `String` on each `TextElement` would make copy and paste work inside one
    /// box and do nothing at all between two: selecting a line of the chat log and pasting it into
    /// the entry box would be impossible.
    ///
    /// The device's send-to-clipboard is the host hop, and it is not performed here
    /// because this crate has no window. Its format selection is worth recording, because it is
    /// computed branchlessly and reads as a magic number: the format is
    /// `(-(os_version != 2) & 0xfffffff4) + 0xd`, and the `GlobalAlloc(0x42, …)` buffer is
    /// `len + 1` bytes on the ANSI leg or `len * 2 + 2` on the wide one.
    ///
    /// OS version 2 is NT, where the expression is `0 + 0xd` = **`CF_UNICODETEXT` (13)** and
    /// the buffer is the wide string copied verbatim as UTF-16. Every other value
    /// gives `0xfffffff4 + 0xd` = **`CF_TEXT` (1)** with each code unit truncated to one byte. A
    /// rebuild targets NT, so the format is `CF_UNICODETEXT`.
    pub clipboard: String,

    /// What the host still owes.
    ///
    /// This crate has no window and cannot call user32, so the send is deferred exactly the way
    /// the cursor push is (see [`Self::pending_cursor`]): `Copy` and `Cut` record the text here,
    /// and the client drains it with [`Self::take_pending_clipboard`] and hands it to
    /// `dereth_clipboard::set_text`, the crate that confines this hop's unsafe.
    ///
    /// `None` is the common case and the empty-selection gate's answer: a `Copy` with nothing
    /// selected records nothing, so a stray Ctrl+C does not empty the user's real clipboard.
    pub(crate) pending_clipboard: Option<String>,

    /// The Numerical Recipes random-number generator the media
    /// machine draws from. It must never be merged with the CRT generator: the two are separate
    /// streams in retail.
    pub rng: Ran2,
    /// The shared animation-level easing table.
    pub easing: [i16; 100],

    /// `client_local_<Language>.dat`'s string tables, as a service.
    ///
    /// The client reaches them through a singleton data cache; this crate has no
    /// asset source, so the host installs one. `None` means every non-literal `StringInfo` resolves
    /// to nothing, which is what a layout-only test wants and is why installing it is optional.
    pub strings: Option<std::rc::Rc<dyn text::StringResolver>>,
    /// The font mapper, as a service. As [`UiSystem::strings`].
    pub fonts: Option<std::rc::Rc<dyn text::FontProvider>>,
    /// The dat cache, as a service. As [`UiSystem::strings`].
    ///
    /// The tooltip is the one element this manager builds **on its own**, out of a layout, with no
    /// caller to hand it an asset source: the element's start-tooltip-at-mouse runs from
    /// the element's mouse-hover handler inside the tooltip check, which is step 4 of
    /// [`Self::use_time`]. The client has no problem
    /// there because the original layout lookup reaches its asset cache through a process-global
    /// service. This field is the corresponding service, installed like the string tables and font
    /// mapper.
    ///
    /// `None` means the hover machine latches and no tooltip window is built, which is what a
    /// layout-only test wants and is why installing it is optional.
    pub assets: Option<std::rc::Rc<dyn LayoutAssets>>,

    /// Every subtree the factory dropped, so a missing widget is not mistaken for a layout bug.
    pub dropped: Vec<DroppedSubtree>,
    /// Effects the media machines raised this frame, for the host to act on.
    pub media_effects: Vec<(ElemHandle, MediaEffect)>,
    /// The last `now` [`UiSystem::use_time`] was given, matching the client's stored UI time.
    ///
    /// The client reads the clock itself and runs its update immediately, so the pause and
    /// fade steps a state set
    /// starts are stamped with the *current* time. Passing a zero here instead makes every one of
    /// them expire on its first tick, because the
    /// next tick's `now` is the real elapsed time and `now - 0.0` is already past any duration: the
    /// intro's two 2.75-second holds would be one frame long.
    pub now: LocalTime,
    /// The caret's half-period in seconds. Retail reads `USER32!GetCaretBlinkTime()` on every
    /// tick of the text element's per-frame loop (an import call, then
    /// `* 0.001`); this is that value, defaulting to Windows' own default of 530 ms
    /// ([`text::CARET_BLINK_PERIOD`]). The host may overwrite it with the player's setting.
    pub caret_blink_time: f64,

    /// The requests this UI has raised for the game, in emission order, until the host drains
    /// them. Every emitter appends here: a screen's handlers, the panels they call, and element
    /// behaviours whose hooks run inside this manager's own move, resize and visibility calls.
    /// One queue per UI, so a host drives two UIs, or a test matrix runs in parallel, without any
    /// shared state.
    pub requests: dereth_client_contract::requests::Outbox,
    /// The inbound notices the panels of this UI drain. See [`dereth_client_contract::notices`].
    pub notice_inbox: dereth_client_contract::notices::NoticeInbox,
    /// The asset source and layout resolver screens and panels build elements from. See
    /// [`env`](mod@env). `None` until the host calls [`Self::set_env`].
    env: Option<env::Env>,
}

impl UiSystem {
    /// Give this UI the asset source and resolver its screens build from, or take it away.
    pub fn set_env(&mut self, env: Option<env::Env>) {
        self.env = env;
    }

    /// The environment [`Self::set_env`] installed, if any.
    #[must_use]
    pub fn env(&self) -> Option<&env::Env> {
        self.env.as_ref()
    }

    /// A clone of the installed environment, or the loud [`UiError::Persist`] a screen
    /// constructed before the dat is open has always answered with. A clone, so the caller can go
    /// on to borrow this `UiSystem` mutably (every `Env` builder takes `&mut UiSystem`); the clone
    /// shares the original's asset source, resolver and memo.
    pub fn require_env(&self) -> Result<env::Env, UiError> {
        self.env.clone().ok_or_else(env::no_env)
    }

    /// The element manager's init, minus the parts that belong to other tracks (the
    /// preference registration, the render callback, the input-map push and the console command).
    ///
    /// Step 5 of `Init` creates the root as a hollow element with no parent, marks it activatable
    /// and pushes it onto the activatable-element list.
    #[must_use]
    pub fn new(display: (i32, i32)) -> Self {
        let mut s = Self {
            slots: Vec::new(),
            free: Vec::new(),
            root: ElemHandle::for_test(0),
            element_list: Vec::new(),
            delete_queue: Vec::new(),
            factories: BTreeMap::new(),
            lib: DescLibrary::new(),
            property_types: PropertyTypes::new(),
            property_defaults: BTreeMap::new(),
            display,
            element_listeners: BTreeMap::new(),
            global_listeners: BTreeMap::new(),
            notices: msg::notice::NoticeBus::new(),
            serial: 0,
            serial_seen: BTreeMap::new(),
            broadcasting: false,
            lifted: 0,
            deferred_states: Vec::new(),
            deferred_mouse_visibility: Vec::new(),
            deferred_relinquish: Vec::new(),
            removals: Vec::new(),
            outbox: Vec::new(),
            mouse: focus::MouseState::default(),
            queued_messages: Vec::new(),
            queued_visibility: Vec::new(),
            tooltip: dialog::tooltip::TooltipState::default(),
            dialogs: dialog::DialogController::new(),

            drag: focus::DragState::default(),
            focus_element: None,
            active_element: None,
            activatable: Vec::new(),
            input_action_listeners: BTreeMap::new(),
            default_cursor: None,
            last_cursor: None,
            pending_cursor: None,
            shift_key_down: false,
            clipboard: String::new(),
            pending_clipboard: None,
            rng: Ran2::new(1),
            easing: media::level_array(),
            strings: None,
            fonts: None,
            assets: None,
            dropped: Vec::new(),
            media_effects: Vec::new(),
            now: LocalTime(0.0),
            caret_blink_time: text::CARET_BLINK_PERIOD,
            requests: dereth_client_contract::requests::Outbox::owned(),
            notice_inbox: dereth_client_contract::notices::NoticeInbox::owned(),
            env: None,
        };
        factory::register_engine_classes(&mut s);
        let root = s.create_hollow(None);
        s.root = root;
        if let Some(n) = s.node_mut(root) {
            n.flags.set_activatable(true);
            n.flags.set_is_root_element(true);
        }
        s.activatable.push(root);
        s
    }

    /// The root element.
    #[must_use]
    pub fn root(&self) -> ElemHandle {
        self.root
    }

    /// The `TextElement` base of an element, for a caller that wants to set its text.
    ///
    /// This is the rebuild's typed receiver for a text child resolved during screen binding,
    /// corresponding to the original client's stored text-child pointer.
    pub fn text_element_mut(&mut self, h: ElemHandle) -> Option<&mut text::TextElement> {
        self.node_mut(h)?.behaviour.as_mut()?.as_text_mut()
    }

    /// `StringInfo` resolution, through whatever the host installed in [`UiSystem::strings`].
    ///
    /// This resolves `(out, id, vars, use-meta-language)`
    /// with **no** variables, and the arm it takes is the one
    /// the string info's internal query takes: `true`. So a row with no substitutions
    /// still goes through the meta-language render, and that render's `flags & 1` tail
    /// collapses every run of spaces in the whole output
    /// (the excess-space trim).
    ///
    /// That tail is the only thing that separates the two arms for a markup-free row, and it is
    /// **visible**: 62 of the 6 899 shipped rows ship a double space that retail never draws, and
    /// one more (`{{keepspaces}}Lore Master  Quiz Night`) ships the option that keeps it
    /// [measured over every shipped string table]. Skipping the renderer would draw every one of
    /// those rows with the extra space.
    ///
    /// [`Self::resolve_string_rendered`] is the same call with values. This one renders **variant
    /// 0 alone**, which is the row a caller with no values is asking for; handing `render` all the
    /// fragments of a multi-variant row would concatenate the pieces around a variable that the
    /// caller has nothing to put in.
    #[must_use]
    pub fn resolve_string(&self, table: DataId, string_id: u32) -> Option<String> {
        let raw = self.strings.as_ref()?.resolve_raw(table, string_id)?;
        Some(text::unescape(text::metalanguage::render(&[raw], &[])))
    }

    /// How many fragments a row holds — one more than the number of variables the client passes
    /// it. A panel that guards on the count (a fragment count that disagrees with the values is a
    /// string-table mismatch, not a formatting choice) needs this and nothing else.
    #[must_use]
    pub fn resolve_string_variant_count(&self, table: DataId, string_id: u32) -> Option<usize> {
        Some(
            self.strings
                .as_ref()?
                .resolve_variants_raw(table, string_id)?
                .len(),
        )
    }

    /// The `bUseMetaLanguage == false` arm of the same row, **unrendered** — variant 0 exactly as
    /// the dat stores it, without meta-language rendering or unescaping.
    ///
    /// Kept for the one caller that genuinely wants the stored text rather than the drawn text:
    /// a census or a probe asking what the dat holds. Production text goes through
    /// [`Self::resolve_string`].
    #[must_use]
    pub fn resolve_string_unrendered(&self, table: DataId, string_id: u32) -> Option<String> {
        self.strings.as_ref()?.resolve(table, string_id)
    }

    /// The same row as [`UiSystem::resolve_string`], but **all** its variants.
    ///
    /// A `StringTableEntry` holds a list: variant 0 is the singular/default form, and a row with
    /// substitutions stores the literal pieces around its variables — which is the only way to
    /// rebuild `ID_DataPatch_PatchProgress`.
    #[must_use]
    pub fn resolve_string_variants(&self, table: DataId, string_id: u32) -> Option<Vec<String>> {
        self.strings.as_ref()?.resolve_variants(table, string_id)
    }

    /// The row's **own** variable list — [`text::StringResolver::resolve_variables`], i.e. the
    /// hashed ids used to look up the caller's values. `None` means the installed resolver cannot
    /// say.
    #[must_use]
    pub fn resolve_string_variables(&self, table: DataId, string_id: u32) -> Option<Vec<u32>> {
        self.strings.as_ref()?.resolve_variables(table, string_id)
    }

    /// `(out, id, vars)`, meta-language on, **as retail
    /// calls it**: the caller names each value and the *row* decides where it goes.
    ///
    /// # The matching rule
    ///
    /// Each named value is filed under the string hash of `"KEY"` (`dereth_primitives::num::hash::str_hash`), not a slot number. Resolution
    /// then walks the **row's** variable ids and looks up each matching value. Order at the call
    /// site is therefore invisible; only the row's order is real. Two shipped rows prove it
    /// matters: `ID_ActionKeyMap_OverwriteExistingBinding` lists `KEY, ACTION` and
    /// `ID_ActionKeyMap_Binding` lists `ACTION, KEY` — the same two values, the other way round —
    /// and `ID_CharacterInfo_Resists` lists `RESIST, RESIST, REGEN`, the same id twice, which a
    /// name lookup satisfies with one value and a positional list only satisfies by accident.
    ///
    /// # A variable the caller did not name
    ///
    /// A failed lookup sets the *missing* flag, and the metalanguage arm's early return of the
    /// missing-variable string runs **before** the render — so
    /// nothing is rendered and the string info's outer getter hands back the empty string it
    /// started with. This answers `None` for that, which every caller here already turns into its
    /// own fall-back. Retail never relies on it: the duration formatter adds all
    /// seven of `ID_DurationFormat`'s variables on every call and passes `""` for the components
    /// that are zero, rather than leaving them out. A value the row does **not** name is simply
    /// never looked up, exactly as an extra entry in the client's hash table would be.
    ///
    /// When the installed resolver cannot name the row's variables — every fixture that holds
    /// fragments and nothing else — this falls back to [`Self::resolve_string_rendered`]'s
    /// positional order.
    #[must_use]
    pub fn resolve_string_named(
        &self,
        table: DataId,
        string_id: u32,
        values: &[(&str, &str)],
    ) -> Option<String> {
        text::string_table::render_named(self.strings.as_deref()?, table, string_id, values)
    }

    /// The meta-language resolve -- the row's
    /// pieces and the caller's values put through
    /// [`text::metalanguage::render`] and then
    /// [`text::unescape`], which is the order
    /// the string info's internal query uses.
    ///
    /// [`UiSystem::resolve_string_variants`] is the non-meta-language path: it hands
    /// back the pieces unescaped and leaves the interleave to the caller, which is right for a
    /// row with no markup and wrong for one with a `{a|b}` block in it. Prefer this whenever the
    /// values are known at the call site.
    ///
    /// # This is the positional shim, not the client's call
    ///
    /// Retail matches a value to a variable by the string hash of its **name**, never by
    /// position (see [`Self::resolve_string_named`], which is the real thing). This entry point
    /// assumes the caller knows the row's variable order, so a localised dat that reordered
    /// a row would silently swap its values. It survives for two kinds of caller: a fixture
    /// resolver that cannot name a row's variables at all, and a row with exactly **one**
    /// variable, where the two rules cannot disagree.
    #[must_use]
    pub fn resolve_string_rendered(
        &self,
        table: DataId,
        string_id: u32,
        values: &[String],
    ) -> Option<String> {
        text::string_table::render_positional(self.strings.as_deref()?, table, string_id, values)
    }

    /// The font lookup, through [`UiSystem::fonts`].
    #[must_use]
    pub fn font_metrics(&self, did: DataId) -> Option<std::sync::Arc<dyn text::FontMetrics>> {
        self.fonts.as_ref()?.metrics(did)
    }

    #[must_use]
    pub fn display(&self) -> (i32, i32) {
        self.display
    }

    /// The live display reference box, `(0, 0, w-1, h-1)`.
    #[must_use]
    pub fn display_box(&self) -> Box2D {
        Box2D::new(0, 0, self.display.0 - 1, self.display.1 - 1)
    }

    // ---- arena ----------------------------------------------------------------------------

    #[must_use]
    pub fn node(&self, h: ElemHandle) -> Option<&ElementNode> {
        let s = self.slots.get(h.index())?;
        if s.generation == h.generation() {
            s.node.as_ref()
        } else {
            None
        }
    }

    pub fn node_mut(&mut self, h: ElemHandle) -> Option<&mut ElementNode> {
        let s = self.slots.get_mut(h.index())?;
        if s.generation == h.generation() {
            s.node.as_mut()
        } else {
            None
        }
    }

    #[must_use]
    pub fn is_alive(&self, h: ElemHandle) -> bool {
        self.node(h).is_some()
    }

    pub(crate) fn alloc(&mut self, node: ElementNode) -> ElemHandle {
        if let Some(i) = self.free.pop() {
            let idx = i as usize;
            self.slots[idx].node = Some(node);
            let h = ElemHandle::new(i, self.slots[idx].generation);
            self.element_list.push(h);
            h
        } else {
            // The only `expect` in this crate's library code. It guards an internal capacity
            // invariant, not input: the handle's index field is 20 bits (1 048 575 live elements)
            // and the retail dat ships 2 162 element *descriptions* in total. Saturating instead
            // would hand out a colliding handle and corrupt the tree silently, which is strictly
            // worse than stopping.
            let i = u32::try_from(self.slots.len())
                .expect("the element arena exceeded 2^32 slots, which cannot happen");
            self.slots.push(Slot {
                generation: 0,
                node: Some(node),
            });
            let h = ElemHandle::new(i, 0);
            self.element_list.push(h);
            h
        }
    }

    /// Behavior: a **linear** search returning the first match.
    /// Kept linear on purpose: the id-uniqueness contract in the doc comment on [`ElementId`]
    /// exists precisely because this is what the client does.
    #[must_use]
    pub fn get_element(&self, id: ElementId) -> Option<ElemHandle> {
        self.element_list
            .iter()
            .copied()
            .find(|h| self.node(*h).is_some_and(|n| n.element_id() == id))
    }

    /// Every live element, in creation order.
    #[must_use]
    pub fn element_list(&self) -> &[ElemHandle] {
        &self.element_list
    }

    // ---- tree -----------------------------------------------------------------------------

    #[must_use]
    pub fn parent(&self, h: ElemHandle) -> Option<ElemHandle> {
        self.node(h).and_then(|n| n.region.parent)
    }

    #[must_use]
    pub fn children(&self, h: ElemHandle) -> Vec<ElemHandle> {
        self.node(h)
            .map(|n| n.region.children.clone())
            .unwrap_or_default()
    }

    /// The element's direct-child lookup.
    #[must_use]
    pub fn get_child(&self, h: ElemHandle, id: ElementId) -> Option<ElemHandle> {
        self.children(h)
            .into_iter()
            .find(|c| self.node(*c).is_some_and(|n| n.element_id() == id))
    }

    /// Behavior: depth-first search of the subtree.
    #[must_use]
    pub fn get_child_recursive(&self, h: ElemHandle, id: ElementId) -> Option<ElemHandle> {
        for c in self.children(h) {
            if self.node(c).is_some_and(|n| n.element_id() == id) {
                return Some(c);
            }
            if let Some(found) = self.get_child_recursive(c, id) {
                return Some(found);
            }
        }
        None
    }

    /// Behavior: walk up until an element flagged as a root element.
    #[must_use]
    pub fn root_of(&self, h: ElemHandle) -> Option<ElemHandle> {
        let mut cur = h;
        loop {
            let n = self.node(cur)?;
            if n.flags.is_root_element() {
                return Some(cur);
            }
            cur = n.region.parent?;
        }
    }

    /// The is-ancestor-of-me test.
    #[must_use]
    pub fn is_ancestor_of(&self, ancestor: ElemHandle, of: ElemHandle) -> bool {
        let mut cur = self.parent(of);
        while let Some(c) = cur {
            if c == ancestor {
                return true;
            }
            cur = self.parent(c);
        }
        false
    }

    /// Set the parent: detach from the old parent, add the child to the
    /// new one (which pushes at the tail and re-sorts by `z_level`), then cascade the size,
    /// position and visibility updates down the subtree.
    pub fn set_parent(&mut self, child: ElemHandle, parent: Option<ElemHandle>) {
        if let Some(old) = self.parent(child) {
            if let Some(n) = self.node_mut(old) {
                n.region.remove_child(child);
            }
        }
        if let Some(n) = self.node_mut(child) {
            n.region.parent = parent;
        }
        if let Some(p) = parent {
            // The z-level comparison: `z_level` first, then `read_order` — see
            // [`region::Region::add_child`], which is where the tie-break is justified.
            let key: BTreeMap<ElemHandle, (i32, u32)> = self
                .children(p)
                .into_iter()
                .chain(std::iter::once(child))
                .filter_map(|c| {
                    self.node(c)
                        .map(|n| (c, (n.region.z_level, n.desc.read_order)))
                })
                .collect();
            if let Some(n) = self.node_mut(p) {
                n.region
                    .add_child(child, |h| key.get(&h).copied().unwrap_or((0, 0)));
            }
        }
        self.update_for_parent_size_change(child);
    }

    /// Behavior: **a child-list operation and nothing else.**
    ///
    /// It removes the child from the parent's list; if the child was absent it does nothing, and
    /// otherwise it appends the child at the tail.
    ///
    /// There is **no [`Self::set_parent`], no child-add and no [`Self::update_for_parent_size_change`]**: nothing in
    /// it writes the parent and nothing re-anchors the child. That is the
    /// whole point of the function and it is why drag startup uses it to bring the proxy to the
    /// top
    /// on the original manager's root element — to put the drag proxy on the screen.
    /// See [`crate::UiSystem::start_drag_and_drop_at`] for what re-anchoring it
    /// instead would do to the ghost.
    ///
    /// The one bookkeeping divergence: retail's proxy keeps a null parent (it was created as a child
    /// element with no parent, and the client's *same-parent* early-out made the
    /// one parent-set call a no-op), and this sets `region.parent` so that deletion, hit-testing
    /// and `screen_origin` find the same tree every other element lives in. The root's own box
    /// starts at `(0, 0)`, so the two readings of every screen coordinate are the same number.
    pub fn bring_child_to_top(&mut self, parent: ElemHandle, child: ElemHandle) {
        if let Some(old) = self.parent(child) {
            if let Some(n) = self.node_mut(old) {
                n.region.remove_child(child);
            }
        }
        if let Some(n) = self.node_mut(child) {
            n.region.parent = Some(parent);
        }
        if let Some(n) = self.node_mut(parent) {
            n.region.remove_child(child);
            n.region.children.push(child);
        }
    }

    /// Move a region to the tail of its parent's child list.
    pub fn bring_to_front(&mut self, h: ElemHandle) {
        if let Some(p) = self.parent(h) {
            if let Some(n) = self.node_mut(p) {
                n.region.bring_child_to_front(h);
            }
        }
    }

    // ---- geometry -------------------------------------------------------------------------

    /// The region's absolute screen origin.
    #[must_use]
    pub fn screen_origin(&self, h: ElemHandle) -> (i32, i32) {
        let mut x = 0;
        let mut y = 0;
        let mut cur = Some(h);
        while let Some(c) = cur {
            let Some(n) = self.node(c) else { break };
            x += n.region.box_.x0;
            y += n.region.box_.y0;
            cur = n.region.parent;
        }
        (x, y)
    }

    /// The element's box in absolute screen coordinates.
    #[must_use]
    pub fn screen_box(&self, h: ElemHandle) -> Box2D {
        let Some(n) = self.node(h) else {
            return Box2D::empty();
        };
        let (px, py) = self.parent(h).map_or((0, 0), |p| self.screen_origin(p));
        n.region.box_.offset(px, py)
    }

    /// Behavior: this box intersected with every ancestor's.
    /// Returns the empty box `(0,0,-1,-1)` when fully clipped.
    #[must_use]
    pub fn screen_clip_box(&self, h: ElemHandle) -> Box2D {
        let mut clip = self.screen_box(h);
        let mut cur = self.parent(h);
        while let Some(c) = cur {
            if !clip.is_valid() {
                return Box2D::empty();
            }
            clip = clip.intersect(&self.screen_box(c));
            cur = self.parent(c);
        }
        if clip.is_valid() {
            clip
        } else {
            Box2D::empty()
        }
    }

    /// The element's `MoveTo`.
    pub fn move_to(&mut self, h: ElemHandle, x: i32, y: i32) {
        let (x, y) = self
            .node(h)
            .and_then(|n| n.behaviour.as_ref())
            .map_or((x, y), |b| b.constrain_move(self, h, x, y));
        let Some(n) = self.node_mut(h) else { return };
        let b = n.region.box_;
        if b.x0 == x && b.y0 == y {
            let mut out = std::mem::take(&mut self.requests);
            if let Some(b) = self.node(h).and_then(|n| n.behaviour.as_ref()) {
                b.after_move(self, &mut out, h, x, y);
            }
            self.requests = out;
            return;
        }
        n.region.box_ = Box2D::from_xywh(x, y, b.width(), b.height());
        let notify = n.flags.notify_on_move();
        for c in self.children(h) {
            self.update_for_parent_size_change(c);
        }
        if notify {
            self.broadcast_global(msg::global::ELEMENT_GEOMETRY, 0);
        }
        let mut out = std::mem::take(&mut self.requests);
        if let Some(b) = self.node(h).and_then(|n| n.behaviour.as_ref()) {
            b.after_move(self, &mut out, h, x, y);
        }
        self.requests = out;
    }

    /// Resize: clamp to **all four** size attributes,
    /// resize, re-anchor every child, and — if notify-on-resize is set — raise element message 0x24 and
    /// global message 7.
    ///
    /// **All four clamps, each only when its attribute is present** — an absent attribute does
    /// not clamp at all. The order is the client's, so a minimum wins over a maximum that
    /// contradicts it. See [`props::attr::MIN_WIDTH`] for which clamp is which.
    pub fn resize_to(&mut self, h: ElemHandle, w: i32, hgt: i32) {
        let Some(n) = self.node(h) else { return };
        let p = n.merged_properties();
        let clamp = |v: i32, max: u32, min: u32| {
            let mut v = v;
            if let Some(m) = p.get_int(max) {
                v = v.min(m);
            }
            if let Some(m) = p.get_int(min) {
                v = v.max(m);
            }
            v
        };
        let hgt = clamp(hgt, props::attr::MAX_HEIGHT, props::attr::MIN_HEIGHT);
        let w = clamp(w, props::attr::MAX_WIDTH, props::attr::MIN_WIDTH);
        let b = n.region.box_;
        if b.width() == w && b.height() == hgt {
            let mut out = std::mem::take(&mut self.requests);
            if let Some(b) = self.node(h).and_then(|n| n.behaviour.as_ref()) {
                b.after_resize(self, &mut out, h);
            }
            self.requests = out;
            return;
        }
        let notify = n.flags.notify_on_resize();
        if let Some(n) = self.node_mut(h) {
            n.region.box_ = Box2D::from_xywh(b.x0, b.y0, w, hgt);
        }
        for c in self.children(h) {
            self.update_for_parent_size_change(c);
        }
        if notify {
            self.broadcast_element_message(h, msg::element::id::RESIZED, 0, 0);
            self.broadcast_global(msg::global::ELEMENT_GEOMETRY, 0);
        }
        let mut out = std::mem::take(&mut self.requests);
        if let Some(b) = self.node(h).and_then(|n| n.behaviour.as_ref()) {
            b.after_resize(self, &mut out, h);
        }
        self.requests = out;
    }

    /// The text element's resize-to-paper — grow the element to the text it holds.
    ///
    /// It uses authored maximum width `0x3D` or the display width, subtracts the horizontal
    /// margins before laying out the glyphs, then resizes to the measured dimensions plus all
    /// four margins.
    ///
    /// Its two callers in the client are the fit-to-text setter — attribute **`0x29`**'s arm —
    /// and the text-insert path's tail, which resizes to paper when text bit `0x400` is set, so
    /// an element with the attribute re-sizes after **every** text change. `0x0400` is
    /// [`text::TextBits::fit_to_text`], and this function is its consumer.
    ///
    /// What it matters for, measured: the house panel's add-text sets `0x29` on every
    /// row of the House tab, and the row template `0x100001E6` ships `0x3D`/`0x3F` = **280**. Row
    /// 8's sentence wraps to **three** 16-pixel lines at that width; without the resize the row
    /// element stays at the template's 27, and the list box — which stacks rows by
    /// `n.region.box_.height()` — draws one line and clips the rest.
    ///
    /// Returns false only when `h` is not a text element; the client's own success test cannot
    /// fail (see [`text::TextElement::recalculate_to_paper`]).
    pub fn resize_to_paper(&mut self, h: ElemHandle) -> bool {
        let max_width = self
            .node(h)
            .and_then(|n| n.merged_properties().get_int(props::attr::MAX_WIDTH))
            .unwrap_or(self.display.0);
        let Some((w, hgt)) = self
            .text_element_mut(h)
            .map(|t| t.recalculate_to_paper(max_width))
        else {
            return false;
        };
        self.resize_to(h, w, hgt);
        true
    }

    /// The current-UI-object-mode walk, transcribed.
    ///
    /// The reader the element's resize, its object-scale query and its object-scale update
    /// all use, and the one place where "which element's `0xCD`"
    /// is not "this element's": it walks **up** to the nearest self-or-ancestor that actually owns
    /// a UI object and reads *that* element's attribute.
    ///
    /// Starting at the element, the client walks through parents until it finds one whose
    /// should-own-object flag is set. A color picker always yields mode 3; any other owner yields
    /// authored enum attribute `0xCD`, with mode 3 when the attribute is absent. Reaching the top
    /// without an owner leaves the caller's preinitialized mode 3 unchanged.
    ///
    /// **The default is the caller's, not the function's**, and it matters: two of the four exits
    /// leave the output alone. Every caller in the client writes `3` into it first —
    /// the element's resize, its object-scale query and its object-scale update — so a walk that
    /// falls off the top of the tree answers [`props::UiObjectMode::ElementSize`], which is what
    /// this returns. Written as a `const` default here rather than an `Option` because the
    /// original has no third answer.
    ///
    /// The colour-picker test is the element's type-cast query for type `0x10`: the colour picker
    /// answers itself for that id and null otherwise, and the base answers null for every id.
    /// `0x10` is [`factory::ty::COLOR_PICKER`].
    #[must_use]
    pub fn current_ui_object_mode(&self, h: ElemHandle) -> props::UiObjectMode {
        let mut cur = h;
        loop {
            let Some(n) = self.node(cur) else {
                return props::UiObjectMode::ElementSize;
            };
            if n.flags.should_own_object() {
                // The colour-picker cast, then the attribute, then the default -- in that order.
                if n.desc.ty == factory::ty::COLOR_PICKER
                    || n.desc.engine_ty == factory::ty::COLOR_PICKER
                {
                    return props::UiObjectMode::ElementSize;
                }
                return n
                    .merged_properties()
                    .get_enum(props::attr::UI_OBJECT_MODE)
                    .and_then(props::UiObjectMode::from_value)
                    .unwrap_or(props::UiObjectMode::ElementSize);
            }
            // Having no parent (either way the client asks) is the two exits that leave the
            // caller's `3` standing.
            let Some(p) = n.region.parent else {
                return props::UiObjectMode::ElementSize;
            };
            cur = p;
        }
    }

    /// The element's should-own-object setter, transcribed.
    ///
    /// Two writes of flag bit 14, and **the second overrides the first**:
    ///
    /// The setter first writes flag bit 14 from its argument. A false argument finishes there. A
    /// true argument then reads enum attribute `0xCD`; when authored, its nonzero state replaces
    /// the first write, while an absent attribute leaves the argument in place.
    ///
    /// So [`Self::set_should_own_object`]`(true)` on an element authoring `0xCD = 0` leaves the bit **clear** —
    /// which is exactly the case the manager's root-element creation hits on the nine
    /// shipped mode-0 sites, and the reason those roots own no UI surface. The re-read is not a
    /// belt-and-braces duplicate of `on_set_attribute`'s write: the two run in either order
    /// depending on whether the element is a root, and this one is what makes them agree.
    ///
    pub fn set_should_own_object(&mut self, h: ElemHandle, should: bool) {
        let authored = self
            .node(h)
            .and_then(|n| n.merged_properties().get_enum(props::attr::UI_OBJECT_MODE));
        let Some(n) = self.node_mut(h) else { return };
        n.flags.set_should_own_object(should);
        if !should {
            return;
        }
        if let Some(v) = authored {
            let mode =
                props::UiObjectMode::from_value(v).unwrap_or(props::UiObjectMode::ElementSize);
            n.flags.set_should_own_object(mode.owns_object());
            n.region.object_mode = mode;
        }
    }
    /// The element's update-for-parent-size-change.
    ///
    /// The reference boxes: for a **root** element (or one with no parent element) the old box is
    /// the layout's design box and the new box is the live display; otherwise the parent's
    /// original and current positions. The transform runs from the element's
    /// **design** rectangle, and then the mode-0 rule puts the live coordinate back.
    pub fn update_for_parent_size_change(&mut self, h: ElemHandle) {
        let Some(n) = self.node(h) else { return };
        let design = n.original_position();
        let edges = n.desc.edges;
        let live = n.region.box_;
        let has_size_or_init = live.width() != 0 || live.height() != 0 || n.flags.is_initialized();
        let parent = n.region.parent;
        let is_root = n.flags.is_root_element();
        let layout_design = n.layout_design;

        let (old_ref, new_ref) = match parent {
            Some(p) if !is_root => match self.node(p) {
                Some(pn) => (pn.original_position(), pn.current_position()),
                None => (layout_design, self.display_box()),
            },
            _ => (layout_design, self.display_box()),
        };

        let computed = layout::update_size_and_position(design, old_ref, new_ref, edges);
        let final_box = layout::apply_live_mode_zero(computed, live, edges, has_size_or_init);
        self.move_to(h, final_box.x0, final_box.y0);
        self.resize_to(h, final_box.width(), final_box.height());
    }

    /// Behavior: broadcast **global message 5**, then re-query
    /// the display size and resize the root, which cascades through the whole tree.
    ///
    /// **The cursor bracket, which is deliberate and not decoration.** Retail saves
    /// the last cursor id, sets the last-cursor cache to the invalid id (defeating the set-cursor's
    /// own equality check), draws the dirty regions, and then sets the cursor with the saved id,
    /// the last hotspot and make-default `false`.
    ///
    /// A device reset destroys the `HCURSOR` built from the surface, so the cursor has to be
    /// rebuilt — but the set-cursor returns early when the did and both hotspots match the last
    /// cursor, which they would. Clearing the cache first is what makes the re-push
    /// happen at all; skipping it leaves a stale (or destroyed) cursor on screen. The `false`
    /// matters too: this must not overwrite the default cursor with whatever an element had
    /// overridden it to.
    pub fn refresh_event(&mut self, display: (i32, i32)) {
        let saved = self.last_cursor;
        self.last_cursor = None;
        self.broadcast_global(msg::global::REFRESH, 0);
        self.display = display;
        if let Some(n) = self.node_mut(self.root) {
            n.layout_design = Box2D::new(0, 0, display.0 - 1, display.1 - 1);
        }
        let root = self.root;
        self.resize_to(root, display.0, display.1);
        for c in self.children(root) {
            self.update_for_parent_size_change(c);
        }
        // the dirty-region draw has run (this rebuild redraws every frame); re-push.
        if let Some((did, x, y)) = saved {
            self.set_cursor(did, x, y, false);
        }
    }

    /// Take the cursor push [`UiSystem::set_cursor`] let through, if any.
    ///
    /// The client's half. `None` means nothing changed since the last
    /// call — which is the common case, because the last-cursor cache swallows every redundant push.
    pub fn take_pending_cursor(&mut self) -> Option<(DataId, i32, i32)> {
        self.pending_cursor.take()
    }

    /// Behavior: write the one `Device` clipboard.
    ///
    /// Both halves, because the client's clipboard *is* the system's: the in-process mirror that
    /// `Paste` reads without a round trip, and the record the host drains to reach Win32.
    pub fn send_string_to_clipboard(&mut self, text: String) {
        self.clipboard.clone_from(&text);
        self.pending_clipboard = Some(text);
    }

    /// Take the clipboard write `Copy` or `Cut` let through, if any.
    ///
    /// The client's half, and a **take** rather than a peek:
    /// `SetClipboardData` calls `EmptyClipboard` first and bumps the clipboard sequence number, so
    /// re-sending an undrained value every frame would make this client fight every other
    /// application on the desktop for the clipboard.
    pub fn take_pending_clipboard(&mut self) -> Option<String> {
        self.pending_clipboard.take()
    }

    // ---- the frame ------------------------------------------------------------------------

    /// Behavior: the seven fixed steps, in order: clean the delete queue; process the deferred
    /// message removals; update the mouse if a hit test is pending; check the tooltip; broadcast
    /// global message 3; advance the input manager's time; draw the dirty regions.
    ///
    /// The last step is a no-op here: a D3D12 rebuild redraws the whole UI each frame
    /// ([`Self::draw`]), while preserving the three relevant drawing controls:
    /// draw-after-children, `BlitMode` and the alpha blend modifier.
    pub fn use_time(&mut self, now: LocalTime, input: &mut dyn InputPump) {
        self.now = now;
        // The client polls the input manager's shift-key query inside the two geometry functions that
        // want it. Nothing here can reach the pump from there, so the answer is latched once a
        // frame instead — the same value those functions would have read, sampled at step 0 of
        // `use_time` rather than mid-frame.
        self.shift_key_down = input.shift_key_down();
        self.clean_delete_queue();
        self.process_removal_data();
        if self.mouse.perform_hit_test {
            self.do_mouse_update(now);
        }
        // The truncation recalculation — see
        // [`Self::recalculate_truncation_tooltips`] for why it runs here rather than in the draw.
        // It must precede `check_tooltip`, which reads the has-tooltip flag it writes.
        self.recalculate_truncation_tooltips();
        self.check_tooltip(now);
        self.tick_media(now);
        self.broadcast_global(msg::global::TICK, 0);
        input.use_time(now);
    }

    /// The media machines' half of message 3. Split out only so it can be driven in a test without
    /// a listener table; `MediaPlayback` registers for message 3 exactly as the widgets do.
    fn tick_media(&mut self, now: LocalTime) {
        self.media_effects.clear();
        let live: Vec<ElemHandle> = self.element_list.clone();
        for h in live {
            let Some(n) = self.node(h) else { continue };
            if !n.media.registered_for_tick {
                continue;
            }
            let init = n.flags.is_initialized();
            let mut machine = match self.node_mut(h) {
                Some(n) => std::mem::take(&mut n.media),
                None => continue,
            };
            let fx = machine.update(now.0, &mut self.rng, init);
            if let Some(n) = self.node_mut(h) {
                n.media = machine;
            }
            for e in fx {
                self.apply_media_effect(h, e);
            }
        }
    }

    /// Apply one [`MediaEffect`] to its owning element, exactly as a media-machine update would.
    ///
    /// Public so a test can drive the client's two
    /// arms without building a media track for them.
    pub fn apply_media_effect(&mut self, h: ElemHandle, e: MediaEffect) {
        match &e {
            MediaEffect::SetImage { file, draw_mode } => {
                if let Some(n) = self.node_mut(h) {
                    n.region.image = file.map(|d| GraphicRef::opaque_surface(d, 0, 0));
                    n.region.blit_mode = media_image_blit_mode(*draw_mode);
                }
            }
            MediaEffect::SetAlphaImage { file } => {
                if let Some(n) = self.node_mut(h) {
                    n.region.alpha_image = file.map(|d| GraphicRef::opaque_surface(d, 0, 0));
                }
            }
            // The media machine's cursor step -> the element's set-cursor,
            // whose tail is the manager's cursor check. Without the
            // second line an animated cursor track would write the element's cursor and nothing
            // would ever look at it again.
            MediaEffect::SetCursor { file, hot_x, hot_y } => {
                self.element_set_cursor(h, *file, *hot_x, *hot_y);
            }
            // The cursor step's `INVALID_DID` arm -> the element's unset-cursor.
            MediaEffect::UnSetCursor => {
                self.element_unset_cursor(h);
            }
            MediaEffect::SetObjectAlpha(a) => {
                if let Some(n) = self.node_mut(h) {
                    n.region.alpha_blend_mod = *a;
                }
            }
            MediaEffect::BroadcastMessage { id } => {
                self.broadcast_element_message(h, *id, 0, 0);
            }
            MediaEffect::SetState { id } => {
                self.set_state(h, *id);
            }
            // The host's media player's, raised as a request.
            MediaEffect::PlaySound { .. } | MediaEffect::PlayMovie { .. } => {}
        }
        self.media_effects.push((h, e));
    }

    /// Behavior: replace an element's media machine with one
    /// immediate image and apply its draw mode.
    ///
    /// The original cleans up the media machine, inserts one image step, resets the
    /// current index to zero, and updates it. The resulting image effect reaches the draw path,
    /// where draw modes 2 and 3 select the three-alpha
    /// and four-alpha blits; every other value selects the normal blit. The immediate end state is
    /// enough here: a single image has no later tick to service.
    pub fn set_media_image(&mut self, h: ElemHandle, file: DataId, draw_mode: u32) {
        let Some(n) = self.node_mut(h) else { return };
        n.media.reset(&[]);
        n.region.image = (file.0 != 0).then(|| GraphicRef::opaque_surface(file, 0, 0));
        n.region.blit_mode = media_image_blit_mode(draw_mode);
    }

    /// Behavior: replace one declared state's complete
    /// media list with a single image, and update the live region immediately only when that is
    /// the element's current state.
    ///
    /// An undeclared state is a no-op, matching retail's state-table lookup.
    /// The state-media replacement clears every old media entry before installing the
    /// new type-5 image step; a later [`Self::set_state`] therefore runs exactly this image.
    pub fn set_media_image_for_state(
        &mut self,
        h: ElemHandle,
        file: DataId,
        draw_mode: u32,
        state: StateId,
    ) {
        let current = {
            let Some(n) = self.node_mut(h) else { return };
            let Some(desc) = n.desc.states.get_mut(&state) else {
                return;
            };
            desc.media.clear();
            desc.media.push(crate::desc::MediaDesc {
                media_type: 5,
                type_echo_ok: true,
                fields: crate::desc::MediaFields::Image { file, draw_mode },
            });
            n.state == state
        };
        if current {
            self.set_media_image(h, file, draw_mode);
        }
    }

    /// Redraw the whole UI. Emits one [`UiDrawCmd`] per visible region in
    /// the region's draw order.
    ///
    /// **The layout pass runs here, because this is where the client runs it.**
    /// The original draw path recalculates a text control's glyph list at draw start, so no
    /// panel has to call [`crate::scrollable::recalculate_dirty_text`] by hand from its
    /// `set_text`; see that function's header for the whole reading.
    pub fn draw(&mut self, back: &mut dyn UiDrawBackend) {
        let root = self.root;
        crate::scrollable::recalculate_dirty_text(self, root);
        self.draw_region(root, None, 1.0, back);
    }

    /// The surface object's material-opacity setter, reached the way
    /// the chat interface's set-opacity reaches it.
    ///
    /// The client uses the element's surface object when present; otherwise it walks parents to
    /// the first surface object. If the walk reaches the top, it writes nothing. For the object it
    /// finds, it updates the material opacity through the object's material.
    ///
    /// So the value lands on the **nearest ancestor that owns a UI object, starting at the
    /// element itself** — the should-own-object flag, bit 14,
    /// which only a layout root and an authored `0xCD != 0` ever set. Every region below that
    /// ancestor composes into its surface and is multiplied by this alpha when the surface's quad is
    /// blitted; [`Self::draw`] carries exactly that product down the recursion.
    ///
    /// Returns the element that took it, so a caller can say *which* surface moved rather than
    /// only that something did.
    pub fn set_material_opacity(&mut self, h: ElemHandle, v: f32) -> Option<ElemHandle> {
        let mut cur = h;
        loop {
            let n = self.node(cur)?;
            if n.flags.should_own_object() {
                break;
            }
            cur = n.region.parent?;
        }
        self.node_mut(cur)?.region.material_opacity = v;
        Some(cur)
    }

    /// The material alpha of the surface `h` composes into — [`Self::set_material_opacity`]'s
    /// reader, over the same walk.
    #[must_use]
    pub fn material_opacity(&self, h: ElemHandle) -> f32 {
        let mut cur = h;
        loop {
            let Some(n) = self.node(cur) else { return 1.0 };
            if n.flags.should_own_object() {
                return n.region.material_opacity;
            }
            let Some(p) = n.region.parent else { return 1.0 };
            cur = p;
        }
    }

    /// `extra_clip` is the narrowing an ancestor's child-draw override imposed — see
    /// [`element::Element::child_clip`]. It rides down the subtree because the client's clip box is
    /// a parameter of the per-element draw and is passed on to every child.
    ///
    /// `surface_alpha` is the material opacity of the UI object this element composes into —
    /// see [`Self::set_material_opacity`]. It replaces itself at every element that owns an
    /// object, and every command below that element carries `alpha_blend_mod * surface_alpha`:
    /// the region's own blit into the surface, then the surface's own quad.
    fn draw_region(
        &self,
        h: ElemHandle,
        extra_clip: Option<Box2D>,
        surface_alpha: f32,
        back: &mut dyn UiDrawBackend,
    ) {
        let Some(n) = self.node(h) else { return };
        if !n.region.flags.visible {
            return;
        }
        // The UI-object construction returns false without bit 14, and
        // such an element has no material of its own to modulate.
        let surface_alpha = if n.flags.should_own_object() {
            n.region.material_opacity
        } else {
            surface_alpha
        };
        let alpha = n.region.alpha_blend_mod * surface_alpha;
        // Step 1: bail out when the clip box is empty.
        let mut clip = self.screen_clip_box(h);
        if let Some(e) = extra_clip {
            clip = clip.intersect(&e);
        }
        if !clip.is_valid() {
            return;
        }
        let after = n.region.flags.draw_after_children;
        if after {
            self.draw_children(h, extra_clip, surface_alpha, back);
        }
        let screen = self.screen_box(h);
        // In the original draw path, the base operation blits the image while text-bearing
        // controls draw glyphs separately. The rebuild represents that relationship through
        // behavior composition, so a transparent element still draws its text: transparency
        // suppresses the background blit, not the glyph pass.
        let glyphs = n
            .behaviour
            .as_ref()
            .map(|b| b.compose_text(screen))
            .unwrap_or_default();
        // `TextElement`'s outline pass is a property of the element, read once per
        // self-draw alongside its glyphs, not once per glyph.
        let text_outline = n.behaviour.as_ref().and_then(|b| b.text_outline_color());
        // The self-draw's selection arm intersects each selected glyph's cell with the
        // **surface window** — against `(0, 0, clip_w, clip_h)`,
        // which is the clip box translated to the surface origin — and skips a cell that falls
        // outside it entirely. Done here, where both boxes are in hand, so the draw list carries
        // what retail computed rather than a rectangle the renderer has to re-clip.
        let invert: Vec<Box2D> = n
            .behaviour
            .as_ref()
            .map(|b| b.selection_boxes(screen))
            .unwrap_or_default()
            .into_iter()
            .filter_map(|r| {
                let hit = r.intersect(&clip).intersect(&screen);
                hit.is_valid().then_some(hit)
            })
            .collect();
        // Step 2: `EraseSelf` if `erase_background`, then whatever the widget itself
        // generated into its surface. The client erases the background
        // once per dirty rectangle; with no dirty-rectangle machinery the dirty region
        // is the whole clip box, which is the one rectangle the client produces for a full redraw.
        let mut fills: Vec<UiFill> = Vec::new();
        if n.region.flags.erase_background && clip.is_valid() {
            fills.push(UiFill {
                x: clip.x0,
                y: clip.y0,
                w: clip.width(),
                h: clip.height(),
                color: UiFill::NULL,
            });
        }
        if !n.region.flags.transparent || !glyphs.is_empty() || !fills.is_empty() {
            let transparent = n.region.flags.transparent;
            back.draw_region(&UiDrawCmd {
                who: h,
                screen,
                clip,
                image: if transparent {
                    None
                } else {
                    n.region.image.as_ref().map(|g| g.did)
                },
                image_op: if transparent {
                    None
                } else {
                    n.region.image.as_ref().and_then(|g| g.op)
                },
                image_source: n
                    .region
                    .image
                    .as_ref()
                    .map_or(ImageSource::Interface, |g| g.source),
                blit_mode: n.region.blit_mode,
                alpha_blend_mod: alpha,
                tiling_offset: n.region.tiling_offset,
                rotation_z_degrees: 0,
                color: diffuse_color(alpha),
                glyphs,
                text_outline,
                invert,
                fills,
            });
        }
        // **The caret.** The draw fills it *after* the
        // glyph loop, only when the element has focus and text bits `1` and `0x200` are both set,
        // and the caret is intersected with the surface window first.
        // It is a command of its own rather than an entry in `fills` above
        // because the renderer paints a command's fills *before* its glyphs — that is where
        // the self-erase belongs — and the caret must paint over the glyph it sits against. Issued
        // here, still inside the self-draw's turn, so it lies under this element's children exactly
        // as retail's surface does. The focus gate is the manager's focused element, applied
        // here because the behaviour cannot see it.
        if self.focus_element() == Some(h) {
            if let Some((caret, color)) = n.behaviour.as_ref().and_then(|b| b.caret(screen)) {
                let hit = caret.intersect(&clip);
                if hit.is_valid() {
                    back.draw_region(&UiDrawCmd {
                        who: h,
                        screen,
                        clip,
                        image: None,
                        image_op: None,
                        image_source: ImageSource::Interface,
                        blit_mode: n.region.blit_mode,
                        alpha_blend_mod: alpha,
                        tiling_offset: (0, 0),
                        rotation_z_degrees: 0,
                        color: diffuse_color(alpha),
                        glyphs: Vec::new(),
                        text_outline: None,
                        invert: Vec::new(),
                        fills: vec![UiFill {
                            x: hit.x0,
                            y: hit.y0,
                            w: hit.width(),
                            h: hit.height(),
                            color,
                        }],
                    });
                }
            }
        }
        if !after {
            self.draw_children(h, extra_clip, surface_alpha, back);
        }
        // **The generated pixels come after the children**, which is the one shipped draw-order
        // override that matters here: the radar draws its children first, then its blips, then the
        // four fills of the centre cross.
        // and the radar's own children include the dial face `0x1000003F`, a 120x120 opaque
        // graphic. Emitting the blips with the element's own blit would put them *under* the
        // dial, which is exactly what happened the first time. So they are a command of their
        // own, issued at the client's object-draw step.
        //
        // Coordinates are element-relative, as the client's `sx`/`sy` are; the draw
        // list is absolute.
        if !n.region.surface_fills.is_empty() {
            let fills = n
                .region
                .surface_fills
                .iter()
                .map(|f| UiFill {
                    x: screen.x0 + f.x,
                    y: screen.y0 + f.y,
                    w: f.w,
                    h: f.h,
                    color: f.color,
                })
                .collect();
            back.draw_region(&UiDrawCmd {
                who: h,
                screen,
                clip,
                image: None,
                image_op: None,
                image_source: ImageSource::Interface,
                blit_mode: n.region.blit_mode,
                alpha_blend_mod: alpha,
                tiling_offset: (0, 0),
                rotation_z_degrees: 0,
                color: diffuse_color(alpha),
                glyphs: Vec::new(),
                text_outline: None,
                invert: Vec::new(),
                fills,
            });
        }
    }

    fn draw_children(
        &self,
        h: ElemHandle,
        extra_clip: Option<Box2D>,
        surface_alpha: f32,
        back: &mut dyn UiDrawBackend,
    ) {
        // The child list iterates head to tail, so later children paint on top.
        for c in self.children(h) {
            // The one shipped override that narrows a child's clip is the meter's child-image
            // clip. Everything else returns `None` and pays nothing.
            let own = self
                .node(h)
                .and_then(|n| n.behaviour.as_ref())
                .and_then(|b| {
                    let cn = self.node(c)?;
                    b.child_clip(cn.element_id(), self.screen_box(c))
                });
            let ex = match (extra_clip, own) {
                (Some(a), Some(b)) => Some(a.intersect(&b)),
                (Some(a), None) => Some(a),
                (None, b) => b,
            };
            self.draw_region(c, ex, surface_alpha, back);
        }
    }

    /// The nine-step draw trace over the whole tree, for tests.
    #[must_use]
    pub fn draw_trace(&self) -> Vec<DrawEvent> {
        let mut out = Vec::new();
        self.trace_region(self.root, &mut out);
        out
    }

    fn trace_region(&self, h: ElemHandle, out: &mut Vec<DrawEvent>) {
        let Some(n) = self.node(h) else { return };
        if !n.region.flags.visible || !self.screen_clip_box(h).is_valid() {
            return;
        }
        for step in region::draw_steps(
            n.region.flags.erase_background,
            n.region.flags.draw_after_children,
        ) {
            out.push(DrawEvent { who: h, step });
            if step == DrawStep::DrawChildren {
                for c in self.children(h) {
                    self.trace_region(c, out);
                }
            }
        }
    }

    // ---- messages ---------------------------------------------------------------------------

    /// The listener's register-for-element-message.
    ///
    /// Registering for 0x19 / 0x1C / 0x1D / 0x40 also walks the element list and calls
    /// [`Self::set_mouse_visible`]`(true)` on every live element with that id, so an element becomes
    /// hit-testable retroactively.
    /// The element listener table's entry for `(element_id, message_id)` — who is registered, in registration
    /// order. [`Self::broadcast_element_message`] consults exactly this list, **before** the source's own
    /// ancestors. Empty for a pair nobody registered.
    ///
    /// The registrations are keyed on an `ElementId`, which is shared across every layout in the
    /// build, so one left behind by a destroyed screen answers a later screen's dialog. That is
    /// what this exists to let a caller check.
    #[must_use]
    pub fn element_message_listeners(
        &self,
        element: ElementId,
        message: MessageId,
    ) -> &[msg::element::ListenerReg] {
        self.element_listeners
            .get(&(element, message))
            .map_or(&[], Vec::as_slice)
    }

    pub fn register_for_element_message(
        &mut self,
        element: ElementId,
        message: MessageId,
        who: ListenerId,
    ) {
        msg::element::register(
            self.element_listeners
                .entry((element, message))
                .or_default(),
            who,
        );
        if msg::element::id::AUTO_MOUSE_VISIBLE.contains(&message) {
            for h in self.element_list.clone() {
                if self.node(h).is_some_and(|n| n.element_id() == element) {
                    self.set_mouse_visible(h, true);
                }
            }
        }
    }

    /// Behavior: **deferred** while a broadcast is in progress.
    pub fn unregister_for_element_message(
        &mut self,
        element: ElementId,
        message: MessageId,
        who: ListenerId,
    ) {
        if self.broadcasting {
            self.removals.push(msg::element::Removal::ElementMessage {
                element,
                message,
                who,
            });
            return;
        }
        if let Some(v) = self.element_listeners.get_mut(&(element, message)) {
            msg::element::unregister(v, who);
        }
    }

    /// The listener's register-for-global-message.
    pub fn register_for_global_message(&mut self, id: MessageId, who: ListenerId) {
        msg::element::register(self.global_listeners.entry(id).or_default(), who);
    }

    /// Unregister for one global message — deferred while broadcasting.
    pub fn unregister_for_global_message(&mut self, id: MessageId, who: ListenerId) {
        if self.broadcasting {
            self.removals
                .push(msg::element::Removal::GlobalMessage { message: id, who });
            return;
        }
        if let Some(v) = self.global_listeners.get_mut(&id) {
            msg::element::unregister(v, who);
        }
    }

    /// Unregister for all messages — deferred while broadcasting.
    pub fn unregister_for_all_messages(&mut self, who: ListenerId) {
        if self.broadcasting {
            self.removals
                .push(msg::element::Removal::AllMessages { who });
            return;
        }
        for v in self.element_listeners.values_mut() {
            v.retain(|r| r.who != who);
        }
        for v in self.global_listeners.values_mut() {
            v.retain(|r| r.who != who);
        }
        self.notices.unregister_all(who);
        for h in self.element_list.clone() {
            if let Some(n) = self.node_mut(h) {
                n.listeners.retain(|l| *l != who);
            }
        }
    }

    /// The spec's name for [`Self::unregister_for_all_messages`].
    pub fn unregister_deferred(&mut self, who: ListenerId) {
        self.unregister_for_all_messages(who);
    }

    /// Behavior: register on the element *pointer*, the
    /// second of the two independent mechanisms. This is what the main UI framework uses on its root
    /// elements and what every game UI panel uses on its children.
    pub fn register_for_element_messages(&mut self, h: ElemHandle, who: ListenerId) {
        if let Some(n) = self.node_mut(h) {
            if !n.listeners.contains(&who) {
                n.listeners.push(who);
            }
        }
    }

    pub fn unregister_from_element(&mut self, h: ElemHandle, who: ListenerId) {
        if self.broadcasting {
            self.removals
                .push(msg::element::Removal::FromElement { element: h, who });
            return;
        }
        if let Some(n) = self.node_mut(h) {
            n.listeners.retain(|l| *l != who);
        }
    }

    /// The removal-data pass, run at the top of [`Self::use_time`].
    ///
    /// This is the only protection against a listener destroying itself inside its own callback.
    pub fn process_removal_data(&mut self) {
        let pending = std::mem::take(&mut self.removals);
        for r in pending {
            match r {
                msg::element::Removal::ElementMessage {
                    element,
                    message,
                    who,
                } => {
                    if let Some(v) = self.element_listeners.get_mut(&(element, message)) {
                        msg::element::unregister(v, who);
                    }
                }
                msg::element::Removal::GlobalMessage { message, who } => {
                    if let Some(v) = self.global_listeners.get_mut(&message) {
                        msg::element::unregister(v, who);
                    }
                }
                msg::element::Removal::AllMessages { who } => {
                    self.unregister_for_all_messages(who);
                }
                msg::element::Removal::FromElement { element, who } => {
                    if let Some(n) = self.node_mut(element) {
                        n.listeners.retain(|l| *l != who);
                    }
                }
            }
        }
    }

    /// Who is registered for one global message right now — the global-message listener table's entry for `id`.
    ///
    /// **It makes an *unregistration* falsifiable.** Every listener in this
    /// crate registers for message 3 only while it has work and drops out when it is idle
    /// (`msg::global::TICK`'s own documentation says so of seven element types), and that half is
    /// invisible from behaviour: clears
    /// state bit `0x400000` *and* unregisters, so deleting the unregistration changes nothing
    /// a test of the position could see. Deleting it is still a defect — a screen full of
    /// finished scrollbars would tick for ever — and this is the only way to say so.
    #[must_use]
    pub fn global_message_listeners(&self, id: MessageId) -> Vec<ListenerId> {
        self.global_listeners
            .get(&id)
            .map(|v| v.iter().map(|r| r.who).collect())
            .unwrap_or_default()
    }

    /// Behavior: flat, no bubbling, no serial filter.
    pub fn broadcast_global(&mut self, id: MessageId, param: u32) {
        let listeners: Vec<ListenerId> = self
            .global_listeners
            .get(&id)
            .map(|v| v.iter().map(|r| r.who).collect())
            .unwrap_or_default();
        if listeners.is_empty() {
            return;
        }
        let was = self.broadcasting;
        self.broadcasting = true;
        for who in listeners {
            match who {
                ListenerId::Element(h) => {
                    let Some(mut b) = self.take_behaviour(h) else {
                        continue;
                    };
                    let mut ctx = ElemCtx { ui: self, me: h };
                    b.listen_to_global_message(&mut ctx, id, param);
                    self.put_behaviour(h, b);
                }
                ListenerId::External(_) => {
                    self.outbox.push(Delivery::Global { to: who, id, param });
                }
            }
        }
        self.broadcasting = was;
    }

    /// Latest element-message serial. A screen adapter may retain a synchronous retail
    /// operation's ending serial while its External deliveries wait in the outbox.
    #[must_use]
    pub fn element_message_serial(&self) -> u32 {
        self.serial
    }

    /// The element's broadcast, which forwards to the manager's own
    /// broadcast.
    ///
    /// Synchronous, inside the caller's stack frame. The table listeners run first, then the source
    /// element's own listeners, then its parent's, then its grandparent's — and the serial number
    /// stops a listener registered on several elements in the chain from seeing it twice.
    pub fn broadcast_element_message(&mut self, src: ElemHandle, id: MessageId, p1: u32, p2: u32) {
        self.broadcast_element_message_at(src, id, p1, p2, msg::MessagePoint::default());
    }

    /// The same with the mouse points filled in, as the mouse paths do.
    pub fn broadcast_element_message_at(
        &mut self,
        src: ElemHandle,
        id: MessageId,
        p1: u32,
        p2: u32,
        point: msg::MessagePoint,
    ) {
        let Some(source_id) = self.node(src).map(ElementNode::element_id) else {
            return;
        };
        self.serial = self.serial.wrapping_add(1);
        let m = ElementMessage {
            source_id,
            source: src,
            id,
            p1,
            p2,
            point,
            serial: self.serial,
        };
        let was = self.broadcasting;
        self.broadcasting = true;

        // (a) registrations by element id.
        let by_id: Vec<ListenerId> = self
            .element_listeners
            .get(&(source_id, id))
            .map(|v| v.iter().map(|r| r.who).collect())
            .unwrap_or_default();
        for who in by_id {
            // **The last-listened serial number belongs to the *listener*, not to the
            // route.** The listener's last-listened serial `<` the message's serial check is stamped
            // wherever a listener is reached, including here: a listener that has
            // registered **both** by element id here *and* on an ancestor (which
            // `register_for_element_messages(root, ME)` is, for every screen in this build)
            // would otherwise receive one broadcast **twice**.
            //
            // A dialog answer control — `GamePlayScreen`'s logout `Yes`/`No`, and the accept/cancel
            // pairs `CharGenScreen` and `CharacterManagementScreen` register — hides the problem,
            // because `DialogElement::listen_to_element_message` answers stop-processing for
            // exactly those ids, so the bubble never reaches the root. The vitals windows register
            // by id *and* let the message keep bubbling: without this stamp one press toggles the
            // bar twice, which on screen is a bar that does not toggle at all.
            if !self.claim_serial(who, m.serial) {
                continue;
            }
            if self.deliver_element(who, &m) == ElementMessageListenResult::StopProcessing {
                self.broadcasting = was;
                return;
            }
        }

        // (b) the element itself, then the bubble up the parent chain.
        self.dispatch_and_forward(src, &m);
        self.broadcasting = was;
        // Once the outermost broadcast has unwound, send whatever a handler asked to raise
        // *after* it — see [`Self::queue_element_message`].
        if !was {
            loop {
                if !self.queued_visibility.is_empty() {
                    let (h, v) = self.queued_visibility.remove(0);
                    self.set_visible(h, v);
                    continue;
                }
                let Some((h, id, p1, p2)) = self.queued_messages.pop() else {
                    break;
                };
                self.broadcast_element_message(h, id, p1, p2);
            }
        }
    }

    /// Raise an element message **after** the broadcast currently in flight has finished, rather
    /// than inside it.
    ///
    /// **This is required, not a convenience.** Serial numbers stop a
    /// listener registered on both a child and its parent from hearing one message twice. A nested
    /// broadcast would allocate a larger serial and stamp it on the ancestors, causing the outer
    /// message to be rejected when it resumes. The client avoids that by queuing nested element
    /// messages and draining them FIFO after the current dispatch.
    ///
    /// The list-box selected-item setter and button click path both depend on this ordering. A
    /// button release produces release, click, and button-click messages in sequence; dispatching
    /// button-click from inside click would prevent ancestors from receiving the outer click.
    /// The radar padlock listens for that ancestor click, so its button would otherwise do nothing.
    pub fn queue_element_message(&mut self, h: ElemHandle, id: MessageId, p1: u32, p2: u32) {
        if self.broadcasting {
            self.queued_messages.insert(0, (h, id, p1, p2));
        } else {
            self.broadcast_element_message(h, id, p1, p2);
        }
    }

    /// [`Self::set_visible`], deferred until the broadcast in flight has unwound — the same fix as
    /// [`Self::queue_element_message`], for a handler that changes an element's *visibility*
    /// rather than raising a message.
    ///
    /// **The same serial-number problem, for visibility.** Setting visibility raises
    /// `0x18` (the element's set-visible broadcasts it),
    /// so calling it from inside a `0x18` handler allocates a **larger** serial and stamps every
    /// ancestor as it bubbles; when the outer `0x18` resumes, `claim_serial` refuses it at each
    /// one and it dies below whatever was listening. Measured over the shipped tree, that is why
    /// **six of the sixteen `PanelStack` pages — the six of element type `0x00000008`,
    /// `Panel` — never re-broadcast their own visibility**: `Panel`'s own `0x18` arm
    /// puts its open tab page back up, and that inner visibility set out-stamps the message still
    /// bubbling. Six of the seven toolbar panel buttons open one of those six pages, so with this
    /// deferral absent the toolbar cannot follow the panels once button click handling is the only
    /// thing driving them.
    ///
    /// The panel update's trailing `TAB_PAGE_CHANGED` broadcast is a separate site with the same
    /// shape; it is not deferred here, and `dereth-ui-screens` pins its behaviour.
    pub fn queue_set_visible(&mut self, h: ElemHandle, v: bool) {
        if self.broadcasting {
            self.queued_visibility.push((h, v));
        } else {
            self.set_visible(h, v);
        }
    }

    /// The element's message handler followed by its message forwarder.
    fn dispatch_and_forward(
        &mut self,
        at: ElemHandle,
        m: &ElementMessage,
    ) -> ElementMessageListenResult {
        let mut result = self.deliver_element(ListenerId::Element(at), m);
        if result == ElementMessageListenResult::StopProcessing {
            return result;
        }
        // The element's own listener table, filtered by serial number.
        let listeners = self
            .node(at)
            .map(|n| n.listeners.clone())
            .unwrap_or_default();
        for who in listeners {
            if !self.claim_serial(who, m.serial) {
                continue;
            }
            let r = self.deliver_element(who, m);
            if r == ElementMessageListenResult::StopProcessing {
                return r;
            }
            if r != ElementMessageListenResult::Default {
                result = r;
            }
        }
        // The parent, once per serial number.
        if let Some(p) = self.parent(at) {
            if self.claim_serial(ListenerId::Element(p), m.serial) {
                let r = self.dispatch_and_forward(p, m);
                if r != ElementMessageListenResult::Default {
                    return r;
                }
            }
        }
        result
    }

    /// The listener's last-listened serial number `<` the message's serial — and stamp it.
    fn claim_serial(&mut self, who: ListenerId, serial: u32) -> bool {
        let e = self.serial_seen.entry(who).or_insert(0);
        if *e < serial {
            *e = serial;
            true
        } else {
            false
        }
    }

    fn deliver_element(
        &mut self,
        who: ListenerId,
        m: &ElementMessage,
    ) -> ElementMessageListenResult {
        match who {
            ListenerId::Element(h) => {
                let Some(mut b) = self.take_behaviour(h) else {
                    return ElementMessageListenResult::Default;
                };
                let mut ctx = ElemCtx { ui: self, me: h };
                let r = b.listen_to_element_message(&mut ctx, m);
                self.put_behaviour(h, b);
                if r == ElementMessageListenResult::Default {
                    self.base_listen_to_element_message(h, m)
                } else {
                    r
                }
            }
            ListenerId::External(_) => {
                self.outbox.push(Delivery::Element {
                    to: who,
                    msg: m.clone(),
                });
                ElementMessageListenResult::Default
            }
        }
    }

    /// The four messages the base handles itself,
    /// and only when the message's source element is itself.
    ///
    /// The state ids are defined in [`element::state`].
    fn base_listen_to_element_message(
        &mut self,
        at: ElemHandle,
        m: &ElementMessage,
    ) -> ElementMessageListenResult {
        if m.source != at {
            return ElementMessageListenResult::Default;
        }
        let Some(n) = self.node(at) else {
            return ElementMessageListenResult::Default;
        };
        let st = n.state;
        use msg::element::id;
        if m.id == id::ACTIVATED {
            if st == element::state::NORMAL || st == element::state::ROLLOVER {
                self.set_state(at, element::state::ACTIVE);
            }
        } else if m.id == id::DEACTIVATED {
            if st == element::state::ACTIVE {
                self.set_state(at, element::state::NORMAL);
            }
        } else if m.id == id::FOCUS_CHANGED {
            let gained = m.p1 != 0;
            if gained
                && (st == element::state::NORMAL
                    || st == element::state::ROLLOVER
                    || st == element::state::ACTIVE)
            {
                self.set_state(at, element::state::FOCUSED);
            } else if !gained && st == element::state::FOCUSED {
                self.set_state(at, element::state::NORMAL);
            }
        } else if m.id == id::VISIBILITY_TOGGLE {
            // Enum `0x58` uses 1 to invert current visibility, 2 to show and 3 to hide.
            //
            // The toggle reads the region's current visible flag. A value that is not 1, 2 or 3
            // falls out and does nothing.
            //
            // **Why it matters, measured rather than argued.** All **41** element descriptions in
            // the 101 shipped layouts that carry `0x57` also carry `0x58`, and every one of the 41
            // carries `0x58 = 1` — so under the opposite reading (*1 show*) a key or a button
            // bound to a panel could open it and never close it.
            let mode = n
                .merged_properties()
                .get_enum(props::attr::VISIBILITY_TOGGLE_MODE);
            let vis = n.region.flags.visible;
            match mode {
                Some(1) => self.set_visible(at, !vis),
                Some(2) => self.set_visible(at, true),
                Some(3) => self.set_visible(at, false),
                _ => {}
            }
        }
        ElementMessageListenResult::Default
    }

    /// Take the deliveries queued for listeners this crate does not own, in dispatch order.
    pub fn drain_outbox(&mut self) -> Vec<Delivery> {
        std::mem::take(&mut self.outbox)
    }

    /// The selected menu index on an element, or **-1** when `h` is not a
    /// menu, has no list box, or has one with nothing selected — the client's own three ways of
    /// answering -1, which it does not distinguish either.
    #[must_use]
    pub fn menu_selected_index(&self, h: ElemHandle) -> i32 {
        widgets::menu::selected_index(self, h)
    }

    /// The client's tail for a context whose element the host has just built: record the element
    /// and raise the dialog-opened notice with that context.
    ///
    /// **This is `bind_element`'s production caller.** Dialog creation is split in
    /// this build because the factory has no asset source and cannot build an element: it records
    /// what it wants (in the dialog factory's pending-create queue), the screen instantiates
    /// the `Dialog` layout, and this closes the loop. It is also the producer of the
    /// dialog-opened notice.
    ///
    /// Returns `false` when no open dialog carries that context.
    pub fn bind_dialog_element(&mut self, context: u64, h: ElemHandle) -> bool {
        if !self.dialogs.bind_element(context, h) {
            return false;
        }
        // Dialog creation also assigns the context on the actual subclass, not only
        // the factory's context->element record. Older screen answer readers stay unchanged.
        if let Some(mut behaviour) = self.take_behaviour(h) {
            if let Some(dialog) = behaviour.as_dialog_mut() {
                dialog.context = context;
            }
            self.put_behaviour(h, behaviour);
        }
        self.bring_to_front(h);
        let payload = NoticePayload {
            a: u32::try_from(context).unwrap_or(u32::MAX),
            ..NoticePayload::default()
        };
        self.send_notice(NoticeId::DialogOpened, &payload);
        true
    }

    /// The client's global event handler's fan-out. An id with no handler is a no-op.
    pub fn send_notice(&mut self, id: NoticeId, payload: &NoticePayload) {
        let to: Vec<ListenerId> = self.notices.handlers(id).to_vec();
        for who in to {
            self.outbox.push(Delivery::Notice {
                to: who,
                id,
                payload: payload.clone(),
            });
        }
    }

    // ---- state and properties -----------------------------------------------------------------

    /// Move the element to a new state.
    ///
    /// Compute the old merged set and the new merged set, call `on_set_attribute` for every property
    /// that changed, appeared, or **disappeared** (value-less), then reset the media machine when
    /// the new state has media. If the state record says `pass_to_children`, recurse.
    pub fn set_state(&mut self, h: ElemHandle, s: StateId) {
        // The subclass override runs first, because the button can refuse a state outright. It
        // answers a state its own attributes contradict by
        // writing the attribute and returning, and only falls through to the base state setter
        // when they agree. Without it a button whose layout ships `Disabled = true`
        // stays disabled for ever, so the char-gen wizard's *Exit*, *Help* and
        // *Random* would refuse a real click while the retail client answers one.
        if let Some(mut b) = self.take_behaviour(h) {
            let mut ctx = ElemCtx { ui: self, me: h };
            let handled = b.set_state(&mut ctx, s);
            self.put_behaviour(h, b);
            if handled {
                return;
            }
        }
        let Some(n) = self.node(h) else { return };
        // Resolve the requested state description; an element handed a state it does not declare
        // records **state 0**, not the id. It matters for `pass_to_children`: the intro root
        // broadcasts its state to two children and only one of them has it, and the other must end
        // up in state 0 rather than remembering an id it has no description for.
        let s = if n.desc.access_state(s).is_some() {
            s
        } else {
            StateId(0)
        };
        // **Setting a state is idempotent.** After resolving an
        // undeclared id to zero, retail compares it with the current state. Equality returns
        // successfully before storing the state, applying the property diff, or recursing through
        // `pass_to_children`. A re-set of the state an element is already in is not a cheap
        // no-op in retail, it is the **absence** of one.
        //
        // **The char-gen town page shows why.** The town page's
        // set-town lights the chosen map pin and *then* puts the page itself into
        // `0x10000034 + area`. Those four page
        // states carry `pass_to_children`, so the page broadcasts one of them down its subtree;
        // the map plate `0x1000040A` does not declare it, resolves to **0**, is *already* 0 — and
        // retail stops right here. Without this gate the cascade would continue through the plate
        // to the four pins `0x1000040B/D/E/F`, which do not declare it either, so every one of
        // them would be re-stated to 0 and the pin lit one call earlier would go dark again.
        //
        // It is not only the pins. Without the gate every `pass_to_children` broadcast in the
        // shipped layouts would reach the whole subtree instead of stopping at the first
        // descendant that does not declare the state, and every redundant state set would re-run
        // `on_set_attribute` and reset a `MediaPlayback` that retail leaves alone.
        if self.node(h).is_some_and(|n| n.state == s) {
            return;
        }
        let Some(n) = self.node(h) else { return };
        let old = n.merged_properties();
        let new = n.merged_properties_for(s);
        // A state set reads the pass-to-children flag from the selected alternate state when one
        // exists, or from the element's base state when state zero selects no alternate record.
        // The state-zero arm therefore consults the base record rather than answering "no".
        //
        // **The selected-object field needs that arm.** It (`0x1000019E`) ships base
        // `pass_to_children = true` and its alternate state `0x1000000B` carries
        // a 0.25 s pause step then a state step to state 1. State 1 is not authored, so the client
        // records **0** and broadcasts 0 to the children — which is what takes `0x100001A0`'s
        // green plate (`0x06001937`) back to its base media, `INVALID_DID`. Without the base arm
        // the field alone would return to state 0 and the plate would stay green for ever.
        let pass = n
            .desc
            .access_state(s)
            .map_or_else(|| n.desc.base.pass_to_children, |d| d.pass_to_children);
        let media: Vec<desc::MediaDesc> = n.media_for(s).to_vec();
        if let Some(n) = self.node_mut(h) {
            n.state = s;
        }
        for c in props::state_diff(&old, &new) {
            self.on_set_attribute(h, c.id, c.value.as_ref());
        }
        if !media.is_empty() {
            let init = self.node(h).is_some_and(|n| n.flags.is_initialized());
            let mut machine = match self.node_mut(h) {
                Some(n) => std::mem::take(&mut n.media),
                None => return,
            };
            machine.reset(&media);
            let now = self.now.0;
            let fx = machine.update(now, &mut self.rng, init);
            if let Some(n) = self.node_mut(h) {
                n.media = machine;
            }
            for e in fx {
                self.apply_media_effect(h, e);
            }
        }
        if pass {
            for c in self.children(h) {
                self.set_state(c, s);
            }
        }
    }

    /// Whether a drag element exists, meaning something is being dragged right now.
    ///
    /// Drop-target handling tests it before it will show a drop highlight,
    /// which is what makes that highlight a *drag*-over and not a hover.
    #[must_use]
    pub fn is_dragging(&self) -> bool {
        self.drag.element.is_some()
    }

    /// Take what `MasterProperty 0x39000001` gives this system: the id->type table every layout
    /// decode needs and the per-row defaults [`Self::get_attribute_enum`] falls back to.
    pub fn install_master(&mut self, master: &dereth_assets::MasterProperty) {
        self.property_types = master.property_types();
        self.property_defaults = master
            .properties
            .iter()
            .filter_map(|(id, d)| d.default.clone().map(|v| (*id, v)))
            .collect();
    }

    /// The property description's default value for `id`, out of the installed `MasterProperty`.
    #[must_use]
    pub fn master_default(&self, id: u32) -> Option<&PropertyValue> {
        self.property_defaults.get(&id)
    }

    /// The element's enum attribute getter, whole: if the element's property query finds an enum
    /// value it returns true with it; otherwise it builds a property from the `MasterProperty`
    /// description's default and returns false with that enum; failing both, it writes `0` and
    /// returns false.
    ///
    /// The element's merged properties first (instance, current state, description -- what
    /// its property query searches), then the **master default**, then `0`. The `bool` is
    /// the one retail returns; the caller that matters, the list box's selected-item setter,
    /// never tests it (it pushes the out-slot straight
    /// into the state set), so both come back and that caller takes `.1`.
    ///
    /// The master default matters: reading only the merged properties and skipping the state
    /// set when the list box names no state would leave the eleven shipped lists that
    /// enable `0x61` without naming the pair -- Friends, Squelch, Titles, Page List among them --
    /// never entering their row templates' authored state 6.
    #[must_use]
    pub fn get_attribute_enum(&self, h: ElemHandle, id: u32) -> (bool, u32) {
        if let Some(v) = self
            .node(h)
            .and_then(|n| n.merged_properties().get_enum(id))
        {
            return (true, v);
        }
        match self.master_default(id) {
            Some(PropertyValue::Enum(v)) => (false, *v),
            _ => (false, 0),
        }
    }

    /// Behavior: write into the instance properties, then call `on_set_attribute`.
    /// Every typed setter follows this order.
    pub fn set_attribute_bool(&mut self, h: ElemHandle, id: u32, v: bool) {
        let value = PropertyValue::Bool(v);
        if let Some(n) = self.node_mut(h) {
            n.instance_properties.set(id, value.clone());
        }
        self.on_set_attribute(h, id, Some(&value));
    }

    /// Behavior: the same shape as
    /// [`UiSystem::set_attribute_bool`]. The scrollbar's own position
    /// (attribute 0x86), which is the only coordinate `Scrollbar` has, is written through it.
    pub fn set_attribute_float(&mut self, h: ElemHandle, id: u32, v: f32) {
        let value = PropertyValue::Float(v);
        if let Some(n) = self.node_mut(h) {
            n.instance_properties.set(id, value.clone());
        }
        self.on_set_attribute(h, id, Some(&value));
    }

    /// The element's int attribute setter.
    pub fn set_attribute_int(&mut self, h: ElemHandle, id: u32, v: i32) {
        let value = PropertyValue::Integer(v);
        if let Some(n) = self.node_mut(h) {
            n.instance_properties.set(id, value.clone());
        }
        self.on_set_attribute(h, id, Some(&value));
    }

    /// Set an element's enum attribute.
    ///
    /// **Not interchangeable with [`Self::set_attribute_int`]**:
    /// `base_on_set_attribute`'s `0x57` arm matches `PropertyValue::Enum` and nothing
    /// else, so writing an input action through the `Int` setter registers no listener and reports
    /// success. The shipped layouts author these properties as enums, which is why the live tree
    /// registers 40 elements and a hand-written `set_attribute_int` registers none.
    pub fn set_attribute_enum(&mut self, h: ElemHandle, id: u32, v: u32) {
        let value = PropertyValue::Enum(v);
        if let Some(n) = self.node_mut(h) {
            n.instance_properties.set(id, value.clone());
        }
        self.on_set_attribute(h, id, Some(&value));
    }

    /// Behavior: the base id table, then the subclass override.
    ///
    /// Order matters: the subclass handles its own ids and **chains to this one**, so the base runs
    /// second here.
    pub fn on_set_attribute(&mut self, h: ElemHandle, id: u32, v: Option<&PropertyValue>) {
        if let Some(mut b) = self.take_behaviour(h) {
            let mut ctx = ElemCtx { ui: self, me: h };
            b.on_set_attribute(&mut ctx, id, v);
            self.put_behaviour(h, b);
        }
        self.base_on_set_attribute(h, id, v);
    }

    fn base_on_set_attribute(&mut self, h: ElemHandle, id: u32, v: Option<&PropertyValue>) {
        use props::attr;
        let b = |v: Option<&PropertyValue>| matches!(v, Some(PropertyValue::Bool(true)));
        let i = |v: Option<&PropertyValue>| match v {
            Some(PropertyValue::Integer(x)) => *x,
            _ => 0,
        };
        let f = |v: Option<&PropertyValue>| match v {
            Some(PropertyValue::Float(x)) => *x,
            _ => 0.0,
        };
        let e = |v: Option<&PropertyValue>| match v {
            Some(PropertyValue::Enum(x)) => Some(*x),
            _ => None,
        };
        match id {
            // **`0x3B` is `UICore_Element_hide`, so the sense is inverted** — see
            // [`props::attr::HIDE`] for the four independent facts that fix it.
            // A value-less change (the property disappeared from the new state) lands here with
            // `v == None`; `b(None)` is false, so the element goes back to visible, which is the
            // constructor default and what the row's absent `MasterProperty` default implies.
            attr::HIDE => self.set_visible(h, !b(v)),
            attr::BLOCK_CLICKS => {
                if let Some(n) = self.node_mut(h) {
                    n.region.flags.block_clicks = b(v);
                }
                self.update_mouse_visibility(h);
            }
            attr::TOOLTIP_ON => {
                if let Some(n) = self.node_mut(h) {
                    n.region.flags.tooltip = b(v);
                }
            }
            attr::ALPHA_BLEND_MOD => {
                if let Some(n) = self.node_mut(h) {
                    n.region.alpha_blend_mod = f(v);
                }
            }
            attr::ERASE_BACKGROUND => {
                if let Some(n) = self.node_mut(h) {
                    n.region.flags.erase_background = b(v);
                }
            }
            attr::DRAW_AFTER_CHILDREN => {
                if let Some(n) = self.node_mut(h) {
                    n.region.flags.draw_after_children = b(v);
                }
            }
            attr::TILING_OFFSET_X => {
                if let Some(n) = self.node_mut(h) {
                    n.region.tiling_offset.0 = i(v);
                }
            }
            attr::TILING_OFFSET_Y => {
                if let Some(n) = self.node_mut(h) {
                    n.region.tiling_offset.1 = i(v);
                }
            }
            attr::TILING_OFFSET_BOTH => {
                // "set both tiling offsets (re-dispatches 0x55 and 0x54)".
                self.base_on_set_attribute(h, attr::TILING_OFFSET_Y, v);
                self.base_on_set_attribute(h, attr::TILING_OFFSET_X, v);
            }
            attr::INPUT_MAP => {
                if let Some(n) = self.node_mut(h) {
                    n.input_map = e(v);
                }
            }
            attr::INPUT_ACTION => {
                if let Some(a) = e(v) {
                    // Registration refuses action 0 and a null element, and appends uniquely, so
                    // re-setting `0x57`
                    // on an element already in the bucket does not put it in twice. A bare `push`
                    // would deliver `0x31` once per registration.
                    if a != 0 {
                        let bucket = self.input_action_listeners.entry(a).or_default();
                        if !bucket.contains(&h) {
                            bucket.push(h);
                        }
                    }
                }
            }
            attr::DROP_CATCHER => {
                if let Some(n) = self.node_mut(h) {
                    n.drop_catcher = b(v);
                }
            }
            attr::CONTEXT_MENU => {
                if let Some(n) = self.node_mut(h) {
                    n.flags.set_context_menu(b(v));
                }
                self.update_mouse_visibility(h);
            }
            attr::UI_OBJECT_MODE => {
                // `0xCD` is an enum, not a bool. The element's attribute setter
                // reads it into a local initialised to `3`, passes
                // `value != 0` to the should-own-object setter and then
                // updates for the parent size. The bool is kept because it is a
                // real field — flag word bit 14 — but it is *derived* from the mode
                // rather than being the whole of what `0xCD` says.
                //
                // An absent or non-enum value is mode 3, not mode 0: that is the initial value in
                // the setter, the UI-object construction's fall-back and
                // the current-mode walk's caller-supplied default. A value outside `0..=3` is
                // left at 3 for the same reason — retail's readers compare against 1, 2 and 3 and
                // treat anything else as the element-size arm.
                let mode = e(v)
                    .and_then(props::UiObjectMode::from_value)
                    .unwrap_or(props::UiObjectMode::ElementSize);
                if let Some(n) = self.node_mut(h) {
                    n.flags.set_should_own_object(mode.owns_object());
                    n.region.object_mode = mode;
                }
                self.update_for_parent_size_change(h);
            }
            _ => {
                if let Some(n) = self.node_mut(h) {
                    match id {
                        attr::ACTIVATABLE => n.flags.set_activatable(b(v)),
                        attr::ACTIVATE_ON_SHOW => n.flags.set_activate_on_show(b(v)),
                        attr::DRAGABLE => n.flags.set_dragable(b(v)),
                        attr::NOTIFY_ON_RESIZE => n.flags.set_notify_on_resize(b(v)),
                        attr::NOTIFY_ON_MOVE => n.flags.set_notify_on_move(b(v)),
                        attr::NOTIFY_ON_CREATE => n.flags.set_notify_on_create(b(v)),
                        attr::RESIZE_LINE => n.flags.set_resize_line(b(v)),
                        attr::SAVE_LOCATION => n.flags.set_save_location(b(v)),
                        attr::SAVE_SIZE => n.flags.set_save_size(b(v)),
                        // The full group-8 id→name map is unknown. Anything else passes through
                        // to the property bag unchanged: the value is already in the merged
                        // collection and nothing else needs doing.
                        _ => {}
                    }
                }
            }
        }
    }

    /// The visibility test: every own-visible bit must be set and the
    /// parent chain must end at the manager's root. An unattached element is not visible.
    #[must_use]
    pub fn is_visible(&self, h: ElemHandle) -> bool {
        let mut at = h;
        loop {
            let Some(n) = self.node(at) else { return false };
            if !n.region.flags.visible {
                return false;
            }
            let Some(parent) = n.region.parent else {
                return at == self.root();
            };
            at = parent;
        }
    }

    /// The element's set-visible, including the activation half of
    /// the show. Its guard is an effective visibility edge, not
    /// the argument alone: a repeated show or a show below a hidden parent cannot raise a window.
    pub fn set_visible(&mut self, h: ElemHandle, v: bool) {
        let was_visible = self.is_visible(h);
        let Some(n) = self.node_mut(h) else { return };
        if n.region.flags.visible == v {
            let mut out = std::mem::take(&mut self.requests);
            if let Some(b) = self.node(h).and_then(|n| n.behaviour.as_ref()) {
                b.after_set_visible(self, &mut out, h, v);
            }
            self.requests = out;
            return;
        }
        n.region.flags.visible = v;
        let visible = self.is_visible(h);
        self.broadcast_element_message(h, msg::element::id::VISIBILITY_CHANGED, u32::from(v), 0);
        self.broadcast_global(msg::global::ELEMENT_GEOMETRY, 0);
        if was_visible != visible {
            if visible {
                if self.node(h).is_some_and(|n| n.flags.activate_on_show()) {
                    self.activate(h);
                }
            } else {
                // Retail unregisters BEFORE deactivation/fallback. A hidden popup must not
                // remain a candidate when activate-next searches the existing registration order.
                self.activatable.retain(|&candidate| candidate != h);
                if self.node(h).is_some_and(|n| n.flags.is_active()) {
                    self.deactivate(h);
                    if self.node(h).is_some_and(|n| n.flags.is_root_element()) {
                        self.activate_next();
                    }
                }
            }
        }
        let mut out = std::mem::take(&mut self.requests);
        if let Some(b) = self.node(h).and_then(|n| n.behaviour.as_ref()) {
            b.after_set_visible(self, &mut out, h, v);
        }
        self.requests = out;
    }

    /// Set whether the element should be mouse-visible.
    pub fn set_mouse_visible(&mut self, h: ElemHandle, v: bool) {
        if let Some(n) = self.node_mut(h) {
            n.should_be_mouse_visible = v;
            n.is_mouse_visible = v;
        }
    }

    /// Recompute mouse visibility — the element is mouse-visible if it should be *or* if the
    /// subclass's should-be-mouse-visible override says so.
    ///
    /// **Deferred while the element's own behaviour is lifted out of its slot.** In the
    /// client the should-be-mouse-visible query is a virtual call on a live object and is always
    /// answerable; here a widget running one of its own callbacks has been moved out of the arena,
    /// so asking the node for it answers `false` — and `false` is not "no opinion", it is
    /// `Button`'s folded `return true;` stub silently becoming a `return
    /// false;`. That is not theoretical: the button's attribute setter calls
    /// the mouse-visibility update for attribute `0x0D` from *inside* itself, so **every button whose
    /// `Disabled` was set after creation would stop being hit-testable** — the wizard's Profession,
    /// Skills and Town tabs, which the wizard's progress-state setter writes `0x0D` on
    /// every time it runs, would be inert to a real pointer while every other button on the same
    /// screen answered one. The same queue [`Self::queue_set_state`] uses, for the same reason.
    pub fn update_mouse_visibility(&mut self, h: ElemHandle) {
        if self.node(h).is_some_and(|n| n.behaviour.is_none()) {
            self.deferred_mouse_visibility.push(h);
            return;
        }
        // The base implementation makes elements with a context menu or tooltip mouse-visible.
        // Every type that does not always return true inherits this behavior.
        //
        // Without it an element that is mouse-visible *because it has a
        // tooltip* — which is how `Radar` becomes clickable over a blip, and how any plain
        // region with hover text becomes hoverable — would be mouse-invisible, and the pointer would
        // go straight through it. The subclass override is OR-ed on top, not instead: the five
        // grab-with-the-mouse types return true unconditionally.
        let base = self
            .node(h)
            .is_some_and(|n| n.flags.context_menu() || n.tooltip_text.is_some());
        let sub = self
            .node(h)
            .and_then(|n| n.behaviour.as_ref().map(|b| b.should_be_mouse_visible()))
            .unwrap_or(false);
        if let Some(n) = self.node_mut(h) {
            n.is_mouse_visible = n.should_be_mouse_visible || base || sub;
        }
    }

    // ---- behaviour borrowing ------------------------------------------------------------------

    pub(crate) fn take_behaviour(&mut self, h: ElemHandle) -> Option<Box<dyn Element>> {
        let b = self.node_mut(h).and_then(|n| n.behaviour.take());
        if b.is_some() {
            self.lifted += 1;
        }
        b
    }

    pub(crate) fn put_behaviour(&mut self, h: ElemHandle, b: Box<dyn Element>) {
        if let Some(n) = self.node_mut(h) {
            n.behaviour = Some(b);
        }
        self.lifted = self.lifted.saturating_sub(1);
        if self.lifted == 0 {
            // Before the states: a state change can raise another `on_set_attribute`, and the queue
            // below must not be left holding a handle across it.
            self.flush_deferred_relinquish();
            self.flush_deferred_mouse_visibility();
            self.flush_deferred_states();
        } else {
            // A restored child is safe even while its caller (e.g. GroupBox) is still lifted.
            // Finish this child's derived restates before the caller makes its next state change:
            // A group box can clear and reselect the SAME child. Waiting
            // for the parent would leave states 1/6 queued against the final toggle value,
            // recursively alternating the button's state-change attribute writes without finishing.
            // Do not drain other handles: an ancestor's behaviour can still be absent.
            self.flush_deferred_states_for(h);
        }
    }

    /// Relinquish focus, deferred until the element's behaviour is back in
    /// its slot. It is the same queue-shaped problem as
    /// [`Self::update_mouse_visibility`]'s.
    ///
    /// The text element's set-editable and set-selectable relinquish from
    /// *inside* `on_set_attribute`, so in this crate they would fire while the text element's own
    /// behaviour is lifted out of the arena. The manager's set-focus-element broadcasts `0x2F` with
    /// `p1 == 0` on the element it is taking the caret from, and
    /// the text element's own message handler's losing arm is where the client
    /// unregisters the caret tick and deselects — on **that same element**.
    /// Delivered synchronously the message would find the slot empty and both would be skipped,
    /// leaving a box that is no longer focused still blinking and still holding a highlight.
    pub(crate) fn queue_relinquish_focus(&mut self, h: ElemHandle) {
        self.deferred_relinquish.push(h);
    }

    fn flush_deferred_relinquish(&mut self) {
        for h in std::mem::take(&mut self.deferred_relinquish) {
            self.relinquish_focus(h);
        }
    }

    /// Apply the [`Self::update_mouse_visibility`] calls that were made while their
    /// element's behaviour was lifted.
    fn flush_deferred_mouse_visibility(&mut self) {
        for h in std::mem::take(&mut self.deferred_mouse_visibility) {
            self.update_mouse_visibility(h);
        }
    }

    /// Set an element state after its target behavior is back in its slot.
    ///
    /// **Why a queue.** A widget runs with its behaviour object moved out of the arena, which is
    /// this crate's stand-in for the client's in-flight listener iteration; a state change performed *inside* that window
    /// diffs the property set and calls `on_set_attribute` for every change, and the subclass half
    /// of `on_set_attribute` would be skipped because the slot is empty. That is not theoretical:
    /// **670 of the 3,275 state records in the gameplay tree carry properties**, and states 1, 3
    /// and 6 of the toolbar's buttons carry attribute 0x1B (the font-colour array), so a button
    /// that changed state from inside its own mouse-down would keep the wrong caption colour.
    /// The queued setter is called from exactly those five places.
    pub(crate) fn queue_set_state(&mut self, h: ElemHandle, s: StateId) {
        self.deferred_states.push((h, s));
    }

    fn flush_deferred_states_for(&mut self, h: ElemHandle) {
        while let Some(i) = self
            .deferred_states
            .iter()
            .position(|(target, _)| *target == h)
        {
            let (_, state) = self.deferred_states.remove(i);
            self.set_state(h, state);
        }
    }

    fn flush_deferred_states(&mut self) {
        // Drained from the front: a state change can queue another, and the client applies them in
        // the order they were asked for.
        let mut guard = 0;
        while !self.deferred_states.is_empty() && guard < 256 {
            guard += 1;
            let (h, s) = self.deferred_states.remove(0);
            self.set_state(h, s);
        }
    }

    // ---- lifecycle -----------------------------------------------------------------------------

    /// Initialize an element in the documented order.
    pub fn initialize(&mut self, h: ElemHandle) {
        let Some(n) = self.node(h) else { return };
        let media: Vec<desc::MediaDesc> = n.desc.base.media.clone();
        let default_state = n.desc.default_state;
        // 1. Reset the media machine from the element description.
        let mut machine = match self.node_mut(h) {
            Some(n) => std::mem::take(&mut n.media),
            None => return,
        };
        machine.reset(&media);
        if let Some(n) = self.node_mut(h) {
            n.media = machine;
        }
        // 2. mark the element initialised
        if let Some(n) = self.node_mut(h) {
            n.flags.set_is_initialized(true);
        }
        // 3. Enter the description's default state.
        self.set_state(h, default_state);
        // 4. on_set_attribute for every property in the desc's collection
        let props: Vec<(u32, PropertyValue)> = self
            .node(h)
            .map(|n| n.merged_properties().0.into_iter().collect())
            .unwrap_or_default();
        for (id, v) in props {
            self.on_set_attribute(h, id, Some(&v));
        }
        // 5. mouse-visible = should-be-mouse-visible || the subclass override
        self.update_mouse_visibility(h);
        // 6. if notify-on-create is set, broadcast global message 6 with this element
        if self.node(h).is_some_and(|n| n.flags.notify_on_create()) {
            self.broadcast_global(msg::global::ELEMENT_CREATED, h.0);
        }
        // Run the media machine once so a state with an image shows it immediately.
        let init = true;
        let mut machine = match self.node_mut(h) {
            Some(n) => std::mem::take(&mut n.media),
            None => return,
        };
        let fx = machine.update(0.0, &mut self.rng, init);
        if let Some(n) = self.node_mut(h) {
            n.media = machine;
        }
        for e in fx {
            self.apply_media_effect(h, e);
        }
    }

    /// Initialise a whole subtree, then post-init — which is what a framework does
    /// after [`Self::register_for_element_messages`].
    ///
    /// **The post-init half is essential.** The base post-init walks
    /// the children calling each child's post-init, and a subclass chains to the base first, so
    /// the effective order is children before parent; the button's own post-init then
    /// runs its state update, which is the only thing that ever moves a button out of state 0.
    ///
    /// **The initialise half walks the tree bottom-up.** Retail initialises a
    /// created subtree **bottom-up**, and there is only one routine that ever does it: the
    /// recursive builder initialises each child as it makes it, and whichever of
    /// the root-element create or the child-element create asked for the tree
    /// initialises the subtree root **last**, after every descendant is already attached and
    /// already initialised.
    ///
    /// The recursive builder creates and initializes each child before attaching it. The caller
    /// then initializes the subtree root, attaches that root to its parent, and finally runs the
    /// recursive post-initialization step.
    ///
    /// The root-element create — which is what the **load** path reaches, through
    /// the framework's create-and-add-root-element — has exactly the same shape:
    /// the partial-description recursive create, then the root's initialise, the should-own-object
    /// set, the root-element flag, then its post-init. **There is no second, parent-first order in the client**,
    /// so this one routine serves both paths.
    ///
    /// Element initialization is **not** recursive and no subclass overrides it,
    /// so the only recursion is the one above. Step 3 of it is
    /// setting the description's default state and
    /// step 4 re-dispatches every property through `on_set_attribute`. Both of those reach the
    /// element's **children** — the state set through `pass_to_children`, and `on_set_attribute`
    /// through subclasses that restate a child. The panel's attribute setter reaches its own
    /// update and selection, behavior that every tabbed page in the client depends on. Walking
    /// parent-first, the child's own
    /// default-state set would run *after* the parent had restated it and throw the result
    /// away: every tab caption in the client would come up in its base state rather than
    /// `0x0B`/`0x0C`, and every fresh map note would come up framed.
    pub fn initialize_tree(&mut self, h: ElemHandle) {
        self.initialize_subtree(h);
        self.post_init_tree(h);
    }

    /// The `Initialize` half of [`Self::initialize_tree`]: every descendant in child order,
    /// deepest first, and this element last. See that function for the addresses.
    fn initialize_subtree(&mut self, h: ElemHandle) {
        for c in self.children(h) {
            self.initialize_subtree(c);
        }
        self.initialize(h);
    }

    /// Behavior: children first, then this element's own override.
    pub fn post_init_tree(&mut self, h: ElemHandle) {
        for c in self.children(h) {
            self.post_init_tree(c);
        }
        if let Some(mut b) = self.take_behaviour(h) {
            let mut ctx = ElemCtx { ui: self, me: h };
            b.post_init(&mut ctx);
            self.put_behaviour(h, b);
        }
    }

    /// The element's add-to-delete-queue, which forwards to the manager's.
    /// Elements are **never** deleted inline while the tree is being walked.
    pub fn add_to_delete_queue(&mut self, h: ElemHandle) {
        if let Some(n) = self.node_mut(h) {
            if n.queued_for_delete {
                return;
            }
            n.queued_for_delete = true;
        } else {
            return;
        }
        self.delete_queue.push(h);
    }

    #[must_use]
    pub fn delete_queue_len(&self) -> usize {
        self.delete_queue.len()
    }

    /// Drain the delete queue, at the very top of [`Self::use_time`].
    pub fn clean_delete_queue(&mut self) {
        while let Some(h) = self.delete_queue.pop() {
            self.destroy_now(h);
        }
    }

    /// The framework's remove-root-element, which forwards to the manager's
    /// remove-and-delete-root.
    pub fn remove_and_delete_root(&mut self, h: ElemHandle) {
        self.add_to_delete_queue(h);
        self.clean_delete_queue();
    }

    /// The element's destructor plus the manager's deleting-element hook.
    fn destroy_now(&mut self, h: ElemHandle) {
        // The subclass destructor runs first, as it does in C++: a `Menu` hands its
        // popup back here. See [`element::Element::on_destroy`].
        if let Some(mut b) = self.take_behaviour(h) {
            let mut ctx = ElemCtx { ui: self, me: h };
            b.on_destroy(&mut ctx);
            self.put_behaviour(h, b);
        }
        for c in self.children(h) {
            self.destroy_now(c);
        }
        // Element deletion: clear the element out of every manager pointer.
        let who = ListenerId::Element(h);
        for v in self.element_listeners.values_mut() {
            v.retain(|r| r.who != who);
        }
        for v in self.global_listeners.values_mut() {
            v.retain(|r| r.who != who);
        }
        self.notices.unregister_all(who);
        self.serial_seen.remove(&who);
        for v in self.input_action_listeners.values_mut() {
            v.retain(|e| *e != h);
        }
        self.activatable.retain(|e| *e != h);
        for slot in [
            &mut self.mouse.last_over,
            &mut self.mouse.last_entered,
            &mut self.mouse.capture,
            &mut self.drag.last_drag_cursor_over,
            &mut self.drag.element,
            &mut self.drag.owner,
            &mut self.drag.potential,
            &mut self.focus_element,
            &mut self.active_element,
            &mut self.tooltip.owner,
            &mut self.tooltip.element,
        ] {
            if *slot == Some(h) {
                *slot = None;
            }
        }
        if let Some(p) = self.parent(h) {
            if let Some(n) = self.node_mut(p) {
                n.region.remove_child(h);
            }
        }
        self.element_list.retain(|e| *e != h);
        self.delete_queue.retain(|e| *e != h);
        let idx = h.index();
        if let Some(s) = self.slots.get_mut(idx) {
            if s.generation == h.generation() {
                s.node = None;
                s.generation = s.generation.wrapping_add(1);
                if let Ok(i) = u32::try_from(idx) {
                    self.free.push(i);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
