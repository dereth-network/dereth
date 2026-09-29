//! Element behavior: the flags word, state machine, property bag, and lifecycle.
//!
//! The element field table, 25-bit flags word, and lifecycle.
//!
//! Elements are **never deleted inline** while the tree is being walked: the delete-queue push
//! appends to the manager's delete queue and the queue is drained at the very top of
//! the UI system's per-frame update.

use dereth_client_contract::requests::Outbox;
use dereth_primitives::DataId;

use crate::desc::{ElementDesc, StateDesc};
use crate::layout::BorderLocation;
use crate::media::MediaPlayback;
use crate::props::{PropertyCollection, PropertyValue};
use crate::region::{Box2D, Region};
use crate::{ElemHandle, ElementId, ElementType, ListenerId, MessageId, StateId};

/// The original element's flags word; bit 0 is the LSB.
///
/// Twenty-five bits are named; the rest of the dword is unused in this build.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ElementFlags(pub u32);

macro_rules! flag {
    ($get:ident, $set:ident, $bit:expr, $doc:expr) => {
        #[doc = $doc]
        #[must_use]
        pub const fn $get(self) -> bool {
            self.0 & (1 << $bit) != 0
        }
        #[doc = $doc]
        pub fn $set(&mut self, v: bool) {
            if v {
                self.0 |= 1 << $bit;
            } else {
                self.0 &= !(1 << $bit);
            }
        }
    };
}

impl ElementFlags {
    flag!(
        resize_line,
        set_resize_line,
        0,
        "bit 0 — resize line, attribute 0x44."
    );
    flag!(
        dragable,
        set_dragable,
        1,
        "bit 1 — dragable, attribute 0x3A."
    );
    flag!(
        activatable,
        set_activatable,
        2,
        "bit 2 — activatable, attribute 0x33."
    );
    flag!(
        activate_on_show,
        set_activate_on_show,
        3,
        "bit 3 — activate on show, attribute 0x34."
    );
    flag!(
        save_location,
        set_save_location,
        4,
        "bit 4 — save location, attribute 0x45."
    );
    flag!(
        save_size,
        set_save_size,
        5,
        "bit 5 — save size, attribute 0x46."
    );
    flag!(save_visible, set_save_visible, 6, "bit 6 — save visible.");
    flag!(
        context_menu,
        set_context_menu,
        7,
        "bit 7 — context menu, attribute 0x37."
    );
    flag!(
        notify_on_draw,
        set_notify_on_draw,
        8,
        "bit 8 — notify on draw."
    );
    flag!(
        notify_on_resize,
        set_notify_on_resize,
        9,
        "bit 9 — notify on resize, attribute 0x41."
    );
    flag!(
        notify_on_move,
        set_notify_on_move,
        10,
        "bit 10 — notify on move, attribute 0x42."
    );
    flag!(
        notify_on_parent_change,
        set_notify_on_parent_change,
        11,
        "bit 11 — notify on parent change."
    );
    flag!(
        notify_on_create,
        set_notify_on_create,
        12,
        "bit 12 — notify on create, attribute 0x43."
    );
    flag!(
        notify_on_mouse_move,
        set_notify_on_mouse_move,
        13,
        "bit 13 — notify on mouse move."
    );
    flag!(
        should_own_object,
        set_should_own_object,
        14,
        "bit 14 — should own object, attribute 0xCD."
    );
    flag!(
        object_is_temporary,
        set_object_is_temporary,
        15,
        "bit 15 — object is temporary."
    );
    flag!(
        does_own_object,
        set_does_own_object,
        16,
        "bit 16 — does own object."
    );
    flag!(
        is_initialized,
        set_is_initialized,
        17,
        "bit 17 — is initialized, set by `initialize`."
    );
    flag!(
        is_moving,
        set_is_moving,
        18,
        "bit 18 — is moving, set by `start_movement`."
    );
    flag!(
        is_resizing,
        set_is_resizing,
        19,
        "bit 19 — is resizing, set by `start_resizing`."
    );
    flag!(
        is_active,
        set_is_active,
        20,
        "bit 20 — is active, set by `activate`."
    );
    flag!(
        is_root_element,
        set_is_root_element,
        21,
        "bit 21 — is root element."
    );
    flag!(wants_focus, set_wants_focus, 22, "bit 22 — wants focus.");
    flag!(
        wants_dbl_clicks,
        set_wants_dbl_clicks,
        23,
        "bit 23 — wants double clicks."
    );
    flag!(
        notify_on_hover,
        set_notify_on_hover,
        24,
        "bit 24 — notify on hover."
    );
}

/// The result returned by an element-message listener.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ElementMessageListenResult {
    /// 0 — not handled; keep bubbling and run the element's default handler.
    #[default]
    Default,
    /// 1 — handled; keep bubbling but skip the default element-message handler.
    DontDoDefault,
    /// 2 — handled; stop bubbling immediately.
    StopProcessing,
}

/// `CallbackLoseFocusResult` — the return of the input-action callback's lose-focus hook.
///
/// # *Block* **is** what this build and retail return, and focus is still never blocked
///
/// **0** is *block* — *"refuse to give up focus"* — and 1 is *transfer*. Retail's hook does
/// nothing but return 0, so it returns **block**; yet transfers are never blocked.
///
/// **Settled at the caller, and the caller does not exist.** An exhaustive review of the original
/// callback objects and their call sites found only action delivery: a missing callback is skipped,
/// and a callback returning true consumes the action. None of those sites asks a callback to
/// release focus.
///
/// The neighboring action call is the calibration positive: the same review, on the same object
/// and in the same function, finds action delivery, so the missing lose-focus call is a measurement
/// rather than silence. It is also the only callback result used as a flag there.
///
/// So the enum mapping is right, every original element uses the same refusal stub, and focus is
/// unblockable **because nothing asks** — not because the answer means "go ahead". The distinction
/// is load-bearing: an override returning block would change nothing, and a rebuild that wires a
/// caller "the obvious way" would find every transfer refused, since the stub's own answer is the
/// refusing one.
///
/// [`Default`] is therefore [`Self::Block`], which is what the stub returns. It is safe only
/// because there is no consumer; see [`Element::on_lose_focus`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CallbackLoseFocusResult {
    /// 0 — *"refuse to give up focus"*, and **what the shipped stub returns**.
    #[default]
    Block,
    /// 1 — allow the transfer. Nothing in the shipped client ever produces it.
    Transfer,
}

/// The element states the base transitions between.
///
/// **Inferred**: the layout tool's state enum has not been recovered, and
/// these meanings come only from the transitions the base performs.
pub mod state {
    use crate::StateId;
    /// 0 — normal.
    pub const NORMAL: StateId = StateId(0);
    /// 1 — rollover / hot.
    pub const ROLLOVER: StateId = StateId(1);
    /// 4 — focused.
    pub const FOCUSED: StateId = StateId(4);
    /// 5 — active / pressed.
    pub const ACTIVE: StateId = StateId(5);
}

/// What a concrete widget adds on top of the generic element.
///
/// [`ElementNode`] and the manager handle shared element operations; the hooks below preserve the
/// behavior that concrete widgets add.
pub trait Element: std::fmt::Debug {
    /// The position constraint before a subclass chains to the base move operation.
    /// The base has none. Derived widgets may constrain against their parent, including moves
    /// dispatched during refresh.
    /// This read-only hook does not implement subclass persistence tails or resize constraints.
    fn constrain_move(&self, _ui: &crate::UiSystem, _me: ElemHandle, x: i32, y: i32) -> (i32, i32) {
        (x, y)
    }

    /// Subclass tails after the base virtual operation, including base no-ops. Read-only so a
    /// local model request can be emitted without lifting the behavior during geometry changes:
    /// `out` is the UI's own request queue ([`crate::UiSystem::requests`]), lent for the call, so
    /// what a tail emits lands in the queue at the point the geometry call made it.
    fn after_move(
        &self,
        _ui: &crate::UiSystem,
        _out: &mut Outbox,
        _me: ElemHandle,
        _x: i32,
        _y: i32,
    ) {
    }
    fn after_resize(&self, _ui: &crate::UiSystem, _out: &mut Outbox, _me: ElemHandle) {}
    fn after_set_visible(
        &self,
        _ui: &crate::UiSystem,
        _out: &mut Outbox,
        _me: ElemHandle,
        _visible: bool,
    ) {
    }

    /// The set-attribute hook. `v == None` is the value-less form the `set_state` diff uses
    /// for a property that **disappeared**, so the subclass can reset it.
    fn on_set_attribute(&mut self, _ctx: &mut ElemCtx<'_>, _id: u32, _v: Option<&PropertyValue>) {}

    /// The element-message listener.
    fn listen_to_element_message(
        &mut self,
        _ctx: &mut ElemCtx<'_>,
        _m: &crate::msg::ElementMessage,
    ) -> ElementMessageListenResult {
        ElementMessageListenResult::Default
    }

    /// The global-message listener.
    fn listen_to_global_message(&mut self, _ctx: &mut ElemCtx<'_>, _id: MessageId, _param: u32) {}

    /// The post-init hook, run after the whole subtree is initialised.
    ///
    /// The base implementation walks the children and calls each child's post-init; a
    /// subclass chains to the base **first** and does its own work after, so the effective order is
    /// children before parent. [`crate::UiSystem::initialize_tree`] performs that walk.
    ///
    /// Button initialization runs its inherited post-init and then updates its
    /// state; without that step **no button in
    /// any screen ever leaves state 0**, where the shipped layouts put no image — so every button
    /// on the character screen would draw its caption over the artwork with no background.
    fn post_init(&mut self, _ctx: &mut ElemCtx<'_>) {}

    /// The subclass half of the element's destructor — what a widget has to give back
    /// **before** the manager unhooks it.
    ///
    /// Only one widget in this crate needs it and it is the reason it exists:
    /// the menu's own destructor ends by unregistering from its popup's element messages, queueing
    /// the popup for deletion and forgetting it — the popup is a **root** element, so nothing else in the tree owns it and destroying the
    /// menu otherwise leaks the whole popup subtree. A drop-down with fourteen rows leaks sixteen
    /// elements per screen teardown, which the screen conformance tests' arena check catches.
    ///
    /// Called once, with the behaviour lifted out of its slot as every other hook is, immediately
    /// before the element's children are destroyed.
    fn on_destroy(&mut self, _ctx: &mut ElemCtx<'_>) {}

    /// The input-action callback. `true` means consumed.
    ///
    /// **Read the ordering note on [`Self::on_child_action`] before adding an arm here.** Every
    /// subclass in the client chains to the base handler first, and that base is
    /// nothing but the bubble to the parent, so a container's child-action hook outranks this.
    /// [`crate::UiSystem::dispatch_action`] runs the two in that order.
    fn on_action(&mut self, _ctx: &mut ElemCtx<'_>, _e: &crate::focus::InputEvent) -> bool {
        false
    }

    /// The child-action callback — a **container's**
    /// chance to take an action away from the descendant that has focus. `true` = consumed.
    ///
    /// The base action handler asks
    /// its parent to handle `(this, event)` and returns false when there is no parent. The text
    /// handler opens by asking that base handler
    /// before its own sixteen-arm `switch` — so the container is asked **first** and the text box's
    /// own Enter/Escape/arrow arms are the fallback. That order is the whole reason
    /// The chat container can make Enter *send* rather than
    /// `TextElement`'s `0x25` merely dropping focus.
    ///
    /// `child` is the element that **raised** the action, not the immediate child of this one:
    /// the base child-action handler passes the same `(child, event)` pair straight on to its
    /// parent, so it travels the whole chain unchanged. [verified against retail]
    fn on_child_action(
        &mut self,
        _ctx: &mut ElemCtx<'_>,
        _child: crate::ElemHandle,
        _e: &crate::focus::InputEvent,
    ) -> bool {
        false
    }

    /// The input-action callback's lose-focus hook — in retail it does nothing but return 0.
    ///
    /// The default is the stub's own answer, block, and that is only correct
    /// because **nothing calls this, in either build**. Every original element uses the same stub,
    /// while the input manager and focus machinery never invoke this operation. In this workspace
    /// it has one definition, zero overrides and zero callers. See [`CallbackLoseFocusResult`] for
    /// the call-site review and its action-delivery calibration.
    ///
    /// It is kept rather than deleted so that the seam a future focus manager would use is named
    /// where the evidence is — but a caller must not be added on the strength of the enum: wiring
    /// this up as a veto would refuse every focus transfer, because the value everything answers
    /// is the refusing one.
    fn on_lose_focus(&mut self, _ctx: &mut ElemCtx<'_>) -> CallbackLoseFocusResult {
        CallbackLoseFocusResult::Block
    }

    /// Whether initialization step 5 should make this element visible to mouse hit testing.
    fn should_be_mouse_visible(&self) -> bool {
        false
    }

    /// The state setter — the subclass override, which may **refuse** the state and
    /// write an attribute instead. `true` means "handled; do not run the base setter".
    ///
    /// Only `Button` overrides it in the shipped build
    /// (the button's own state setter). See [`crate::widgets::button::Button::set_state`].
    fn set_state(&mut self, _ctx: &mut ElemCtx<'_>, _s: crate::StateId) -> bool {
        false
    }

    /// The character handler of the input-handler interface. Only `TextElement` implements it.
    fn character(&mut self, _ctx: &mut ElemCtx<'_>, _ch: u16) {}

    /// Whether a mouse press on this element makes it the **focus element**.
    ///
    /// The original scrollable mouse-down first runs shared element handling, then finds the root;
    /// when the root is activatable it activates the root and takes focus. The text control's
    /// escape and accept paths release focus through the matching operation.
    ///
    /// Without this no click anywhere could focus a text box. `UiSystem::mouse_down` asks this
    /// question of the element it hit.
    ///
    /// **It covers the whole `Scrollable` subtree.** The mouse-down handler is
    /// overridden by exactly three links of that chain in the shipped client —
    /// the text element, the button and
    /// the list box — and **every one of them calls its base first**, so the
    /// unconditional take-focus at the bottom runs for all of them. The class hierarchy of the
    /// subtree is `Scrollable -> {ListBox -> ItemListWidget,
    /// TextElement -> Button -> {Menu, Scrollbar}}`, and the
    /// engine half of it answers `true` here:
    /// [`crate::text::TextElement`], [`crate::widgets::button::Button`],
    /// [`crate::widgets::listbox::ListBox`], [`crate::widgets::menu::Menu`] and
    /// [`crate::widgets::scrollbar::Scrollbar`].
    ///
    /// Everything else — `Field`, `Panel`, `GroupBox`, `Meter`, `Viewport`, `Dragbar`,
    /// `Resizebar`, `ColorPicker`, `Browser`, the dialogs — uses the element's own mouse-down,
    /// which has no take-focus at all, and keeps the `false` here.
    ///
    /// **Two game types in the shipped tree are in the subtree and cannot be reached from this
    /// crate**: `ItemListWidget` (`0x10000031`, a `ListBox`) and the six
    /// indicator types (`0x10000001`…`0x10000006`, all `Button`s).
    /// `dereth_ui_screens::register_all` builds them as `PlainElement`, so they answer `false`.
    fn takes_focus_on_press(&self) -> bool {
        false
    }

    /// The concrete widget behind the trait object, for a caller that needs one class's own state.
    ///
    /// The client reaches this by dynamically casting to the requested type id before it touches a
    /// child's fields. Its three item-list casts behave this way, for instance. A `None` here is
    /// that cast returning null.
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        None
    }

    /// [`Self::as_any`] mutably — a type cast that is written through.
    ///
    /// The original cast is not const and several members write through it:
    /// the menu's set-selected-item reaches
    /// the list box's selection setter through exactly this cast, which is what
    /// the menu dialog's `set_data` needs to seed property `0xA4`'s initial selection.
    /// `None` by default and on every widget that does not need it.
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        None
    }

    /// A cast to a **`Dialog`**, mutably — the one cast that has to be `&mut` because
    /// each subclass's `set_data` writes through it. `None` for anything that is not a dialog.
    fn as_dialog_mut(&mut self) -> Option<&mut crate::dialog::types::DialogElement> {
        None
    }

    /// The `TextElement` base, when this widget has one.
    ///
    /// `Button : TextElement`, `Menu : Button` and
    /// `Scrollbar : Button` all do, so a caller that wants to set a caption does not have to know
    /// which of the four it is holding — which is what an upcast to the base gives you and what
    /// this gives you here.
    fn as_text_mut(&mut self) -> Option<&mut crate::text::TextElement> {
        None
    }

    /// The height the wrapped text actually needs, for the dialog's popup
    /// size-and-position update.
    ///
    /// That function recalculates the glyph list and then reads the text element's measured text
    /// height; it is the sum of the wrapped line heights, which is what `crate::text::place`
    /// computes as `text_h` and justifies within the content box. `None` is a widget with no text,
    /// which is the text-element cast failing.
    fn measured_text_height(&self, _screen: Box2D) -> Option<i32> {
        None
    }

    /// The text-composition half of drawing this element.
    ///
    /// The original base implementation blits the background image, while text-bearing controls
    /// also compose their glyph lists into the element surface. `screen` is the
    /// element's absolute box. Returns the placements, which
    /// [`crate::UiSystem::draw`] carries on the element's own [`crate::UiDrawCmd`] so the text is
    /// drawn in tree order with everything else.
    fn compose_text(&self, _screen: Box2D) -> Vec<crate::text::PlacedGlyph> {
        Vec::new()
    }

    /// The self-draw's **outline** half: the colour [`compose_text`](Self::compose_text)'s glyphs are
    /// outlined in, or `None` for no outline.
    ///
    /// The original text glyph loop runs twice when bit `0x10` is set, making an outline pass in
    /// the element's outline color before the ordinary foreground pass over the same pen. The bit comes
    /// from attribute `0x21` (`UICore_Text_outline`) and the colour from `0x22`.
    ///
    /// It is a property of the **element**, not of a glyph, which is why it rides here and not on
    /// [`crate::text::PlacedGlyph`]: the original per-glyph loop reads the element outline color,
    /// never the glyph's own color for the outline.
    ///
    /// **69 of the shipped layouts' text elements ask for it** (71 declare attribute `0x21` and
    /// two declare it `false`), counted over all 101 layouts in `client_local_English.dat`.
    fn text_outline_color(&self) -> Option<u32> {
        None
    }

    /// The self-draw's **selection** half: the rectangles the foreground glyph pass inverts by
    /// flipping the colour bits, one per selected glyph, absolute and inclusive.
    ///
    /// Empty for every element that is not a `TextElement`, and for a text element
    /// whose selecting bit (0x80) is clear — the selection query answers **false** then, and
    /// this arm is the reason that gate exists. See
    /// [`crate::text::TextElement::selection_boxes`].
    fn selection_boxes(&self, _screen: Box2D) -> Vec<Box2D> {
        Vec::new()
    }

    /// The self-draw's **caret** half: the one-pixel, one-glyph-high bar `TextElement` fills
    /// with its current font colour after its glyph loop, as `(inclusive screen box, ARGB colour)`, or
    /// `None`.
    ///
    /// `None` for every element that is not a `TextElement`, for one that is not editable
    /// (text bit `1`) and for one whose caret is on the dark half of its blink
    /// (text bit `0x200` clear); the third gate, having focus, is applied by
    /// [`crate::UiSystem::draw`], which is the only place that can answer it. See
    /// [`crate::text::TextElement::caret_box`].
    fn caret(&self, _screen: Box2D) -> Option<(Box2D, u32)> {
        None
    }

    /// The child draw — the *clipping* half.
    ///
    /// The base hands every child the same clip box. One shipped override narrows the box for
    /// **one** child: the meter intersects its child image's rectangle with the fraction of itself that
    /// attribute `0x69` names, which is how a continuous meter fills. Return `None` for "no extra
    /// clip", which is what every other widget does.
    ///
    /// `child_screen` is the child's own absolute box; the returned box is absolute too, and
    /// [`crate::UiSystem::draw`] carries it down the child's whole subtree exactly as the client's
    /// clip-box parameter does.
    fn child_clip(&self, _child: crate::ElementId, _child_screen: Box2D) -> Option<Box2D> {
        None
    }
}

/// A widget with no behaviour: `Field`(3) and everything the game layer has not taught
/// this track about. The hollow root is represented by the same type.
#[derive(Debug, Default, Clone, Copy)]
pub struct PlainElement;
impl Element for PlainElement {}

/// The mutable context a widget gets while it runs.
///
/// The behaviour object is lifted **out** of its arena slot for the duration of the call, which is
/// how a widget can touch the rest of the tree — including deleting itself — without aliasing. The
/// slot is put back on return. Reentrant messages to the same element therefore see an empty slot
/// and are skipped, which is the same protection the original's listener iteration guard gives.
#[derive(Debug)]
pub struct ElemCtx<'a> {
    pub ui: &'a mut crate::UiSystem,
    pub me: ElemHandle,
}

/// Where a move or resize began — the six drag values the original records.
///
/// Starting a move or resize writes all six: the pointer position it was handed, the element's
/// own box origin, and its width and height. Subsequent pointer motion then puts the
/// element at `drag_start + (mouse - mouse_initial)`, which is why a window picked up by its handle
/// does not jump so that the pointer lands in its corner.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct MovementOrigin {
    /// The initial mouse position — **screen** coordinates, the window point in the message.
    pub mouse: (i32, i32),
    /// The element's box origin at the press.
    pub start: (i32, i32),
    /// The element's width and height at the press.
    pub size: (i32, i32),
}

/// One live element: region state, shared element state, and the concrete widget behavior.
#[derive(Debug)]
pub struct ElementNode {
    pub region: Region,
    /// A **by-value copy** of the resolved `ElementDesc`.
    pub desc: ElementDesc,
    /// The owning layout's data id — kept for re-layout.
    pub layout_did: DataId,
    /// The owning layout's design reference box, `(0,0,w-1,h-1)`.
    pub layout_design: Box2D,
    /// The current state.
    pub state: StateId,
    /// The per-instance properties.
    pub instance_properties: PropertyCollection,
    /// The flags word.
    pub flags: ElementFlags,
    /// The default location — the design-time rectangle.
    pub default_location: Box2D,
    /// The stored tooltip text.
    pub tooltip_text: Option<String>,
    /// The cursor data id and hot spot — this element's cursor
    /// **override**, or `None` for an invalid data id.
    ///
    /// Written only by the cursor set and unset entry points
    /// ([`crate::UiSystem::element_set_cursor`] / [`crate::UiSystem::element_unset_cursor`]),
    /// both of which end by checking the active cursor. `None` reports "no cursor", so an
    /// element with no override is skipped rather than pushing an invalid one.
    pub cursor: Option<(DataId, i32, i32)>,
    /// Whether the element should be, and is, visible to the mouse.
    pub should_be_mouse_visible: bool,
    pub is_mouse_visible: bool,
    /// The border currently being dragged.
    pub current_border: BorderLocation,
    /// The initial mouse position, drag-start origin and drag-start size — the six values
    /// recorded when movement starts, and the only state read to work out where the window should go.
    ///
    /// [`crate::UiSystem::start_movement`] records these as well as setting
    /// [`ElementFlags::is_moving`]; without them a `Dragbar` press would mark its parent as moving
    /// and no pointer motion could ever move it.
    pub movement: MovementOrigin,
    /// The input map pushed while this element has focus, attribute 0x4E.
    pub input_map: Option<u32>,
    /// The media machine.
    pub media: MediaPlayback,
    /// The listeners — who receives this element's messages.
    pub listeners: Vec<ListenerId>,
    /// The focus element — the descendant that currently holds keyboard focus.
    pub focus_descendant: Option<ElemHandle>,
    /// The drag-drop callback, reduced to a flag; the panel owns the policy.
    pub drop_catcher: bool,
    /// The concrete widget. `None` only while it is on the stack inside a callback.
    pub behaviour: Option<Box<dyn Element>>,
    /// Set once the element is on the delete queue, so it is queued at most once.
    pub queued_for_delete: bool,
}

impl ElementNode {
    /// The element's constructor: build the region from the desc's `x, y, width,
    /// height`, copy the desc, record the design rectangle, leave the state at 0 and
    /// is-initialized clear.
    #[must_use]
    pub fn new(
        layout_did: DataId,
        layout_design: Box2D,
        desc: ElementDesc,
        behaviour: Box<dyn Element>,
    ) -> Self {
        let b = desc.box_();
        let mut region = Region::new(b);
        region.z_level = desc.base.z_level;
        Self {
            region,
            default_location: b,
            layout_did,
            layout_design,
            state: StateId(0),
            instance_properties: PropertyCollection::new(),
            flags: ElementFlags::default(),
            tooltip_text: None,
            cursor: None,
            should_be_mouse_visible: false,
            is_mouse_visible: false,
            current_border: BorderLocation::None,
            movement: MovementOrigin::default(),
            input_map: None,
            media: MediaPlayback::default(),
            listeners: Vec::new(),
            focus_descendant: None,
            drop_catcher: false,
            behaviour: Some(behaviour),
            queued_for_delete: false,
            desc,
        }
    }

    #[must_use]
    pub fn element_id(&self) -> ElementId {
        self.desc.element_id
    }

    #[must_use]
    pub fn ty(&self) -> ElementType {
        self.desc.ty
    }

    /// The design rectangle the element was authored at.
    #[must_use]
    pub fn original_position(&self) -> Box2D {
        self.default_location
    }

    /// The live box.
    #[must_use]
    pub fn current_position(&self) -> Box2D {
        self.region.box_
    }

    /// The state-description accessor for the element's *current* state.
    #[must_use]
    pub fn current_state_desc(&self) -> Option<&StateDesc> {
        self.desc.access_state(self.state)
    }

    /// The three-collection merge for a given state.
    #[must_use]
    pub fn merged_properties_for(&self, s: StateId) -> PropertyCollection {
        crate::props::merge_three(
            &self.desc.base.properties,
            self.desc.access_state(s).map(|d| &d.properties),
            &self.instance_properties,
        )
    }

    /// The merged set for the current state.
    #[must_use]
    pub fn merged_properties(&self) -> PropertyCollection {
        self.merged_properties_for(self.state)
    }

    /// The media list that a `set_state` into `s` would run.
    ///
    /// The element's initialisation, step 1, uses the desc's base media — the element's own base
    /// state. `set_state` picks between the two on **whether the alternate `StateDesc`
    /// exists**, never on whether that record's list has anything in it:
    ///
    /// The implementation chooses the alternate state's media array whenever that state exists;
    /// otherwise it chooses the base array. An empty chosen array performs no reset.
    ///
    /// So a state the layout **declares** with an empty media array runs **nothing**. Returning
    /// the base list in that case would make every state transition on an element carrying base
    /// media re-run that base media — and where the base list holds a state media entry,
    /// the media machine's state step ends in a **virtual** state set, which on a button is the
    /// button's own state setter. Its untoggle arm would then take the tick the player had just
    /// made on an option checkbox straight back off, inside that tick's own set to state 6.
    #[must_use]
    pub fn media_for(&self, s: StateId) -> &[crate::desc::MediaDesc] {
        match self.desc.access_state(s) {
            Some(sd) => &sd.media,
            None => &self.desc.base.media,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the element flags table, which gives bit and mask for all 25 flags.
    #[test]
    fn every_documented_flag_sits_on_its_documented_bit() {
        type Row = (u32, fn(&mut ElementFlags, bool), fn(ElementFlags) -> bool);
        let cases: &[Row] = &[
            (
                0x000001,
                ElementFlags::set_resize_line,
                ElementFlags::resize_line,
            ),
            (0x000002, ElementFlags::set_dragable, ElementFlags::dragable),
            (
                0x000004,
                ElementFlags::set_activatable,
                ElementFlags::activatable,
            ),
            (
                0x000008,
                ElementFlags::set_activate_on_show,
                ElementFlags::activate_on_show,
            ),
            (
                0x000010,
                ElementFlags::set_save_location,
                ElementFlags::save_location,
            ),
            (
                0x000020,
                ElementFlags::set_save_size,
                ElementFlags::save_size,
            ),
            (
                0x000040,
                ElementFlags::set_save_visible,
                ElementFlags::save_visible,
            ),
            (
                0x000080,
                ElementFlags::set_context_menu,
                ElementFlags::context_menu,
            ),
            (
                0x000100,
                ElementFlags::set_notify_on_draw,
                ElementFlags::notify_on_draw,
            ),
            (
                0x000200,
                ElementFlags::set_notify_on_resize,
                ElementFlags::notify_on_resize,
            ),
            (
                0x000400,
                ElementFlags::set_notify_on_move,
                ElementFlags::notify_on_move,
            ),
            (
                0x000800,
                ElementFlags::set_notify_on_parent_change,
                ElementFlags::notify_on_parent_change,
            ),
            (
                0x001000,
                ElementFlags::set_notify_on_create,
                ElementFlags::notify_on_create,
            ),
            (
                0x002000,
                ElementFlags::set_notify_on_mouse_move,
                ElementFlags::notify_on_mouse_move,
            ),
            (
                0x004000,
                ElementFlags::set_should_own_object,
                ElementFlags::should_own_object,
            ),
            (
                0x008000,
                ElementFlags::set_object_is_temporary,
                ElementFlags::object_is_temporary,
            ),
            (
                0x010000,
                ElementFlags::set_does_own_object,
                ElementFlags::does_own_object,
            ),
            (
                0x020000,
                ElementFlags::set_is_initialized,
                ElementFlags::is_initialized,
            ),
            (
                0x040000,
                ElementFlags::set_is_moving,
                ElementFlags::is_moving,
            ),
            (
                0x080000,
                ElementFlags::set_is_resizing,
                ElementFlags::is_resizing,
            ),
            (
                0x100000,
                ElementFlags::set_is_active,
                ElementFlags::is_active,
            ),
            (
                0x200000,
                ElementFlags::set_is_root_element,
                ElementFlags::is_root_element,
            ),
            (
                0x400000,
                ElementFlags::set_wants_focus,
                ElementFlags::wants_focus,
            ),
            (
                1 << 23,
                ElementFlags::set_wants_dbl_clicks,
                ElementFlags::wants_dbl_clicks,
            ),
            (
                0x1000000,
                ElementFlags::set_notify_on_hover,
                ElementFlags::notify_on_hover,
            ),
        ];
        assert_eq!(cases.len(), 25);
        for (mask, set, get) in cases {
            let mut f = ElementFlags::default();
            set(&mut f, true);
            assert_eq!(f.0, *mask, "mask {mask:#x}");
            assert!(get(f));
            set(&mut f, false);
            assert_eq!(f.0, 0);
        }
    }
}
