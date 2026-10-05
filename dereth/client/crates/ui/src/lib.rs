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
/// `dereth_client_shell::input::InputShell` can implement it: with the type in one crate and the
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

mod system;

#[cfg(test)]
mod tests;
