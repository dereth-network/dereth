//! `ItemListWidget` and `UiItemWidget` as **live elements**, not as a data model.
//!
//! # What was missing, and why nothing drew
//!
//! An empty quickbar and an empty backpack have one cause: a [`super::ItemList`] alone is a
//! *data model that creates no elements*. In the client an item
//! slot is a real subtree:
//!
//! ```text
//! item-list initialisation
//!   -> create one slot
//!        resolve layout enum 0x10000038 (type 0x23)   // the `ItemSlot` layout 0x21000037
//!        create the slot from that layout, rooted at   // a second layout, at run time
//!          the list's own enum attribute 0x1000000E
//!        clear the slot; set its state to 0x1000001C  // the empty-slot state
//! ```
//!
//! so a list whose `ItemListWidget` creates no children has nothing to draw — no icon element,
//! no quantity text, no ghost, no shortcut numeral. Every one of the shipped
//! `0x1000000E UI_ItemList_ItemSlotID` values is a **root element of `ItemSlot`**, one per slot,
//! and all 56 of them are `UiItemWidget` (type `0x10000032`) carrying the 23 icon-layer
//! children.
//!
//! # Why this is a plain struct and not an `Element` behaviour
//!
//! Everything an item slot displays is a fact about a game object — its icon, its stack size,
//! whether a move is outstanding — and a `dereth_ui::Element` handler is handed only
//! `&mut UiSystem`. So the *widget* here owns the tree surgery and the screen that owns the
//! subtree drives it from a [`GameView`](crate::view::GameView), as [`crate::panels`] and
//! [`crate::toolbar`] already do. The factory retains the inherited `ListBox` behavior while
//! the screen supplies game facts.
//!
//! Nothing here moves an item. A drag emits a
//! [`UiRequest::DragDrop`](crate::view::UiRequest) and the slot ghosts, and only the server's own
//! `Item_ServerSaysMoveItem` changes what a slot holds.

use dereth_primitives::{DataId, ObjectId};
use dereth_ui::framework::LayoutEnum;
use dereth_ui::{ElemHandle, ElementId, PropertyValue, StateId, UiSystem};

use crate::bind::{attr_bool, attr_enum, attr_int, set_attr_float};

/// The layout enum `0x10000038` (DAT type `0x23`) resolves to the shipped `ItemSlot` layout
/// `0x21000037`, which is never named here: a screen
/// names an enum and the installed resolver answers it.
pub const ITEM_SLOT_LAYOUT: LayoutEnum = LayoutEnum(0x1000_0038);

/// The two `UiItemWidget` states, from `ItemListWidget`'s own call sites.
pub mod item_state {
    use dereth_ui::StateId;
    /// Slot state `0x1000001C` — the empty slot. The item-list flush, the empty-slot insert
    /// and slot creation all end here, and a slot state of `0x1000001C` is the client's own
    /// test for "this slot is free".
    pub const EMPTY: StateId = StateId(0x1000_001C);
    /// Slot state `0x1000001D` — the slot holds an object.
    pub const OCCUPIED: StateId = StateId(0x1000_001D);
}

/// The four states the item list writes onto
/// the slot's drop-hint element, and the *whole* of what the retail client shows while an item is
/// being dragged.
///
/// # It is not a cursor
///
/// The green arrows, circles and red crosses shown while moving an item (for instance into a
/// pack) are **not a mouse cursor**:
///
/// * The client's cursor state
///   machine, transcribed in `dereth_client::cursor`, has **no drag arm**. Its nine
///   branches are busy / target-mode / combat-mode only, and none of them reads the drag state.
/// * The per-element override has exactly **one** caller in the
///   whole retail client (an animated-cursor media track).
///   Nothing on any drag path sets a cursor.
/// * The shipped `UICURSOR` mapper's 41 names, read out of `client_portal.dat`, contain no drag
///   cursor at all: 1..15 are the `Default`/`Combat`/`Spellcast`/`Examine`/`Use`/`Wait` pairs,
///   16..38 are the `Move_*` family (`Move_Forward_Run` … `Move_StandStill`) and 39..41 are the
///   `TargetedUse` triple.
///
/// The hint is a **child element of the slot under the pointer**, put into one of these states.
/// The element manager's mouse-over switch is the only producer of element message `0x3E`
/// in the client (`p1 = 0` leaving and `p1 = 1` entering), and
/// the UI item element's element-message handler turns its `0x3E` arm into
/// the item list's drag-over, which chooses one of the three live states
/// below. See [`ItemListWidget::drag_over`].
pub mod drag_accept_state {
    use dereth_ui::StateId;
    /// `0x1000003F` — **no hint**. The slot's element-message handler, on `p1 == 0` (leave),
    /// resets the drop hint to this when its state is anything else, and its `0x15` arm sets
    /// this state on the slot the drop landed on.
    pub const NONE: StateId = StateId(0x1000_003F);
    /// `0x10000040` — **this slot will take the drop.**.
    pub const ACCEPT: StateId = StateId(0x1000_0040);
    /// `0x10000041` — **this slot will refuse it.**.
    pub const REFUSE: StateId = StateId(0x1000_0041);
    /// `0x10000046` — **into this container.**, reached only on a
    /// container list, over a non-empty slot holding a pack with a free slot in it.
    pub const INTO_CONTAINER: StateId = StateId(0x1000_0046);
}

/// The item-slot child ids bound during initialization, in call order.
///
/// The range is `0x1000033B`…`0x10000558` and the count 23; these are the ids slot
/// initialisation actually names. `0x10000349` is fetched **twice** — the retail client assigns the
/// ghost element at the fourth lookup and again at the tenth — which is why the list has 22
/// distinct entries plus the drag icon `0x10000345` that slot initialisation *creates* rather
/// than finds.
pub mod child {
    /// The base icon. Setting the slot's state sets **this** element's state, so it is
    /// the one element an item slot cannot do without.
    pub const ICON: u32 = 0x1000_033B;
    /// The overlay container.
    pub const OVERLAYS: u32 = 0x1000_033C;
    /// The selection ring.
    pub const SELECTED: u32 = 0x1000_0342;
    /// The ghost — what the waiting state (an outstanding move) shows.
    pub const GHOSTED: u32 = 0x1000_0349;
    /// The shortcut numeral.
    pub const SHORTCUT_NUM: u32 = 0x1000_034A;
    /// The sell marker.
    pub const SELL_STATE: u32 = 0x1000_0437;
    /// The trade marker.
    pub const TRADE_STATE: u32 = 0x1000_0438;
    /// The open-container frame.
    pub const OPEN_CONTAINER: u32 = 0x1000_0450;
    /// The drop hint.
    pub const DRAG_ACCEPT: u32 = 0x1000_045A;
    /// The capacity bar — a `Meter`, filled through attribute `0x69`.
    pub const CAPACITY_BAR: u32 = 0x1000_0347;
    /// The structure bar — ditto.
    pub const STRUCTURE_BAR: u32 = 0x1000_0348;
    /// The text — the spell name on a spell shortcut.
    pub const TEXT: u32 = 0x1000_0344;
    /// The stack count.
    pub const QUANTITY: u32 = 0x1000_04F5;
    /// The drag icon, which slot initialisation **creates** from the same `ItemSlot` layout
    /// rather than finding, and then hides.
    pub const DRAG_ICON: u32 = 0x1000_0345;
    /// The ten cooldown wedges (10 %…100 %), in the order the slot binds them and in the
    /// order the cooldown display tests them.
    ///
    /// The ten are one **wedge each**, not ten layers of a pie: the shipped images are
    /// `0x060067CF`…`0x060067D8`, each a full 32×32 overlay, and the display function shows
    /// **exactly one** of them (see `ItemSlot::update_cooldown_display`).
    /// [verified against the live `ItemSlot` tree built from `client_local_English.dat`]
    pub const COOLDOWN: [u32; 10] = [
        0x1000_054F,
        0x1000_0550,
        0x1000_0551,
        0x1000_0552,
        0x1000_0553,
        0x1000_0554,
        0x1000_0555,
        0x1000_0556,
        0x1000_0557,
        0x1000_0558,
    ];
}

/// The icon data's render icons and the client magic system's composite spell icon —
/// **the enum lookups a slot's picture is composited out of.**
///
/// # What retail composites
///
/// A slot's picture is not the raw icon id. It is the object's main icon surface, a 32×32 local surface
/// the icon renderer builds once per object out of **six** blits into two surfaces:
///
/// ```text
/// drag surface = new 32x32
///   blit the icon, normal
///   blit the custom overlay, 4-alpha
///   replace white with UIEffectIcons[effects row or Default]
/// main surface = new 32x32
///   blit UIIconBackgrounds[lowest set bit of item type + 1], normal
///   blit the custom underlay, 3-alpha
///   blit the drag surface, 3-alpha
/// ```
///
/// The **fourth** blit is the background: an opaque normal blit under everything else, keyed
/// by the item's own `ITEM_TYPE`. `0x10000004` is `UIIconBackgrounds`
/// (mapper `0x25000008`, 34 rows)
/// and its fourteen distinct surfaces `0x060011CB`…`0x060011F4` plus `0x06005E23` are exactly
/// the red / blue / teal / green / brown tiles in the recorded retail backpack frame. [verified by
/// resolving all 34 rows out of `client_portal.dat` and decoding them]
///
/// **Why a composite and not layered elements.** A region carries one `GraphicRef`, so painting
/// the tile on the `UiItemWidget` **root** — the only element drawn beneath the base icon — gives
/// one spare layer and no more. The client's own answer to "where do the other five layers go"
/// is not a second region: it is a second **surface**. All six composite into one generated
/// image through [`dereth_ui::region::IconRecipe`], which is the object icon's literal value, and
/// the root carries no picture — which is what the shipped layout says it carries.
///
/// **Why the order cannot be flattened.** The drag surface is finished *before* it is
/// alpha-blended onto the main surface. Writing the six as one ordered list would put
/// a normal blit of the icon straight onto the tile and erase it; the tile survives only because
/// the icon reaches it through the drag surface's own alpha.
///
/// This module owns the **enum lookups** — the four `DidMapper` groups the two composites
/// index and the index arithmetic for each. The blending is
/// [`dereth_ui::region::SurfaceOp::blit_3alpha`] / [`blit_4alpha`](dereth_ui::region::SurfaceOp::blit_4alpha)
/// and the compositing is `dereth_client::ui_draw::composite`.
pub mod icon_background {
    /// The `DidMapper 0x25000000` group `UIIconBackgrounds`.
    pub const ITEM_TYPE_GROUP: u32 = 0x1000_0004;

    /// The **local player's** backpack picture, and it is not a row of
    /// [`ITEM_TYPE_GROUP`] at all.
    ///
    /// The icon renderer asks for the enum pair `(0x10000004, 7)`, and the two arguments are
    /// **(value, group)**, not (group, value). Both enum-lookup helpers hand their pair straight
    /// to the DID-from-enum cache in the same order, so the surface lookup `(value, group, type)`
    /// and the DID lookup `(value, group)` are the *same* convention. The type-tile lookup in the
    /// same renderer, `(lowest set bit of type + 1, 0x10000004, 0x0C)`, is the control: there the
    /// row is first and `0x10000004` is second, exactly as here.
    ///
    /// So the substitution is group **7**, row **`0x10000004`** — and the two orders resolve to
    /// two different pictures in the shipped dat: `(7, 0x10000004)` is `0x0600127E`, a backpack on
    /// a transparent field, while `(0x10000004, 7)` is `0x060011F4`, another opaque
    /// `UIIconBackgrounds` tile. Reading the pair the wrong way round therefore blitted a second
    /// *background* over the container tile through a 3-alpha blit, which is why the cell's
    /// backdrop still looked right and no backpack was ever drawn on it.
    pub const PLAYER_ICON_GROUP: u32 = 7;

    /// The group's own catch-all row.
    ///
    /// The icon renderer takes the item type's lowest set bit + 1 and substitutes `0x21` when that is
    /// 0, and the lowest-set-bit helper returns **-1** for a zero mask (it walks bits 0…31 and
    /// falls off the end), so `0x21` is reached by exactly one input: an item type of 0. Every other
    /// row is `bit + 1`, which is why the mapper has 34 entries for 32 bits.
    pub const DEFAULT_INDEX: u32 = 0x21;

    /// The item type's lowest set bit + 1, with the client's `== 0` fallback.
    #[must_use]
    pub const fn enum_index(item_type: u32) -> u32 {
        if item_type == 0 {
            DEFAULT_INDEX
        } else {
            item_type.trailing_zeros() + 1
        }
    }

    // ---- the other four groups the two composites read -------------------------------------------

    /// `UIEffectIcons` (mapper `0x25000009`, 14 rows): the drag icon's contour replacement.
    pub const EFFECT_GROUP: u32 = 0x1000_0005;
    /// `UISpellBackgrounds` (mapper `0x2500000A`, 12 rows) — the spell-icon composite's opaque
    /// base, indexed by the power level of the spell's power component.
    pub const SPELL_BACKGROUND_GROUP: u32 = 0x1000_0006;
    /// `UISpellOverlays` (mapper `0x2500000B`) — both the reversed/non-reversed wash and the
    /// fellowship / self-targeted badge come out of this one group at four different rows.
    pub const SPELL_OVERLAY_GROUP: u32 = 0x1000_0007;

    /// The effects mask's lowest set bit + 1, with no substitution into the index.
    /// Zero asks row 0 (UNDEF); a missing/zero result falls back to row 0x21.
    #[must_use]
    pub const fn effect_index(effects: u32) -> u32 {
        if effects == 0 {
            0
        } else {
            effects.trailing_zeros() + 1
        }
    }

    /// Icon rendering resolves the effects row, then falls back
    /// on a null result, including the zero-effects case.
    ///
    /// The shipped Default is opaque black. This does not erase the type tile: the effect
    /// surface drives a replace-colour pass inside the drag icon, replacing only opaque-white
    /// contour pixels. The custom underlay is a separate alpha blit onto the main surface.
    /// Earlier code swapped those two surfaces, then incorrectly suppressed the default to
    /// compensate for black backgrounds.
    #[must_use]
    pub fn effect_surface(
        ui: &dereth_ui::UiSystem,
        effects: u32,
    ) -> Option<dereth_primitives::DataId> {
        ui.env()
            .cloned()
            .and_then(|e| e.did_by_enum(EFFECT_GROUP, effect_index(effects)))
            .or_else(|| {
                ui.env()
                    .cloned()
                    .and_then(|e| e.did_by_enum(EFFECT_GROUP, DEFAULT_INDEX))
            })
    }

    /// The spell bitfield's `SelfTargeted` bit.
    pub const SPELL_SELF_TARGETED: u32 = 0x0000_0008;
    /// The spell bitfield's `Reversed` bit — the harmful-spell tint.
    pub const SPELL_REVERSED: u32 = 0x0000_0010;
    /// The spell bitfield's `FellowshipSpell` bit.
    pub const SPELL_FELLOWSHIP: u32 = 0x0000_2000;

    /// `2 - (Reversed ? 1 : 0)` — row 1 reversed, row 2 not, as the retail client
    /// computes the row.
    #[must_use]
    pub const fn spell_tint_index(bitfield: u32) -> u32 {
        dereth_presentation::spell::SpellIconLayers::new(0, bitfield).tint_key()
    }

    /// `if (bitfield & 0x2000) 4; else if (bitfield & 8) 3; else none` — **in that order**, and the
    /// order is the client's: a fellowship spell that is also self-targeted takes row 4.
    #[must_use]
    pub const fn spell_overlay_index(bitfield: u32) -> Option<u32> {
        dereth_presentation::spell::SpellIconLayers::new(0, bitfield).badge_key()
    }
}

/// The icon renderer, as a recipe: the five surfaces, resolved.
///
/// Every enum lookup goes through [`crate::env::did_by_enum`], which memoises, so a grid of 102
/// cells costs at most two mapper reads for the whole pass however many cells share a type.
///
/// `INVALID_DID` — `0` on the wire — is `None` here, which is the enum lookup's own answer for it and
/// what makes the icon renderer's three null guards skip the blit.
#[must_use]
pub fn object_recipe(
    ui: &UiSystem,
    d: &crate::view::SlotDecoration,
) -> dereth_ui::region::IconRecipe {
    // "Is the local player", not "is a player": the icon renderer substitutes the backpack
    // picture and TYPE_CONTAINER for the local player only. The lookup is `(0x10000004, 7)` =
    // **(value, group)** — see `icon_background::PLAYER_ICON_GROUP` for what settles the order.
    let (item_type, icon) = if d.is_player {
        (
            0x200,
            ui.env().cloned().and_then(|e| {
                e.did_by_enum(
                    icon_background::PLAYER_ICON_GROUP,
                    icon_background::ITEM_TYPE_GROUP,
                )
            }),
        )
    } else {
        (d.obj_type, (d.icon_id != 0).then_some(DataId(d.icon_id)))
    };
    dereth_ui::region::IconRecipe::Object {
        background: ui.env().cloned().and_then(|e| {
            e.did_by_enum(
                icon_background::ITEM_TYPE_GROUP,
                icon_background::enum_index(item_type),
            )
        }),
        effects: icon_background::effect_surface(ui, d.effects),
        icon,
        overlay: d.icon_overlay_id.filter(|x| x.0 != 0),
        underlay: d.icon_underlay_id.filter(|x| x.0 != 0),
    }
}

/// The spell-icon composite, as a recipe.
///
/// `power` is the first component's power, which reaches this crate as
/// [`crate::view::SpellEntry::icon_power`]; `bitfield` is the spell's bitfield.
///
/// **The background is not optional in the client** — the spell-icon composite blits it with no null
/// guard at all, so a spell whose power level names no row would blit through a null pointer. It
/// is optional here because this build does not crash on bad data; a level the mapper does not
/// know draws the icon on nothing.
#[must_use]
pub fn spell_recipe(
    ui: &UiSystem,
    power: u32,
    icon: Option<DataId>,
    bitfield: u32,
) -> dereth_ui::region::IconRecipe {
    let layers = dereth_presentation::spell::SpellIconLayers::new(power, bitfield);
    dereth_ui::region::IconRecipe::Spell {
        background: ui
            .env()
            .cloned()
            .and_then(|e| e.did_by_enum(icon_background::SPELL_BACKGROUND_GROUP, layers.power)),
        icon: icon.filter(|x| x.0 != 0),
        tint: ui
            .env()
            .cloned()
            .and_then(|e| e.did_by_enum(icon_background::SPELL_OVERLAY_GROUP, layers.tint_key())),
        overlay: layers.badge_key().and_then(|i| {
            ui.env()
                .cloned()
                .and_then(|e| e.did_by_enum(icon_background::SPELL_OVERLAY_GROUP, i))
        }),
    }
}

/// The item-list attributes read here, with their retail `MasterProperty` names.
pub mod attr {
    /// `UI_ItemList_ItemSlotID` — **the element id of the `ItemSlot` root this list's slots are
    /// built from**, per list. Bank 1 of the quickbar names `0x1000043B`…`0x10000443` and bank 2
    /// `0x100006C1`…`0x100006C9`; the default is `0x1000033A`.
    pub const ITEM_SLOT_ID: u32 = 0x1000_000E;
    /// `UI_ItemList_IsContainer` — this list shows side packs, not loose items.
    pub const IS_CONTAINER: u32 = 0x1000_0011;
    /// `UI_ItemList_IsShortcut`.
    pub const IS_SHORTCUT: u32 = 0x1000_0012;
    /// `UI_ItemList_IsVendor`.
    pub const IS_VENDOR: u32 = 0x1000_0013;
    /// `UI_ItemList_IsSalvage`.
    pub const IS_SALVAGE: u32 = 0x1000_0014;
    /// `UI_ItemList_FixedListSize` — `-1` means "grow to fill", anything else is a slot count.
    /// Written from the container's items capacity or containers capacity.
    pub const FIXED_LIST_SIZE: u32 = 0x1000_0015;
    /// `UI_ItemList_AllowDragging`.
    pub const ALLOW_DRAGGING: u32 = 0x1000_0016;
    /// `UI_ItemList_AtLeastOneEmptySlot`.
    pub const AT_LEAST_ONE_EMPTY: u32 = 0x1000_0017;
    /// `UI_ItemList_SingleSelection` — the list's single-selection flag, read by
    /// the item-list initialisation and by nothing else.
    ///
    /// It gates all three arms of the duplicate-slot rule:
    /// `ItemListWidget::handle_single_selection`, the "selectable unless already in the list" a
    /// new slot gets, and the re-enable when the list deletes an item.
    ///
    /// **It is true on exactly one shipped list.** A calibrated scan of all 101 layouts finds
    /// `0x10000052` declared on two elements — `0x100000C5` in `classic_vendor` (`0x21000012`)
    /// and `0x10000339` in the shared base layout `0x2100003D`, whose value is the template's
    /// `false` — and reading the *resolved* attribute off the live gameplay tree (which is what
    /// inheritance through `base_element`/`base_layout` makes different from the raw scan) gives
    /// **1 of 65** item lists `true`: the vendor's. So the backpack grid, both container strips,
    /// the paper doll, the spellbook and all eighteen quickbar tiles take the `false` arm, and a
    /// duplicated id in any of them keeps two rings **in retail too**. [verified against the live
    /// gameplay tree]
    pub const SINGLE_SELECTION: u32 = 0x1000_0052;
    /// `UICore_ListBox_horizontal` — bit 0 of the list box's flags.
    pub const HORIZONTAL: u32 = 0x5C;
    /// `UICore_ListBox_max_columns`.
    ///
    /// The retail `MasterProperty` names are `UICore_ListBox_max_columns` (`0x5F`) and
    /// `UICore_ListBox_max_rows` (`0x60`) — not "rows/cols" — and the list-box layout pass reads
    /// **only** `0x5F`, straight into the column count. [verified against
    /// `client_local_English.dat`'s `MasterProperty 0x39000001`]
    pub const MAX_COLUMNS: u32 = 0x5F;
    /// `UICore_ListBox_max_rows`. Read by nothing in `ListBox`; carried for completeness.
    pub const MAX_ROWS: u32 = 0x60;

    /// `UI_ItemList_ShortcutOverlayArray` — the **eighteen numerals**
    /// indexes by slot number when the slot holds something and is not ghosted.
    ///
    /// Despite the `UI_ItemList_` prefix the property is carried by
    /// [`child::SHORTCUT_NUM`](super::child::SHORTCUT_NUM) — the shortcut numeral, the element the
    /// shortcut-number write reads it off and sets the image of — and by nothing else in the
    /// shipped gameplay tree. In `classic_gameplay` the first nine entries are the distinct
    /// numerals `0x0600109E`…`0x060010A6` and entries 9…17 are nine copies of `0x060074D3`, which
    /// is why bank 2 has no numbers of its own. [verified against the live element tree]
    pub const SHORTCUT_OVERLAY_ARRAY: u32 = 0x1000_0042;
    /// `UI_ItemList_ShortcutOverlayArray_Ghosted` — the same eighteen in the greyed form
    /// (`0x06001ACC`…`0x06001AD4`, then `0x060074D2`), used while the toolbar is inactive.
    pub const SHORTCUT_OVERLAY_ARRAY_GHOSTED: u32 = 0x1000_0043;
    /// `UI_ItemList_ShortcutOverlayArray_Empty` — the array the shortcut-number write picks when
    /// the icon is in state `0x1000001C`.
    ///
    /// **No element in the shipped `classic_gameplay` tree carries it**, so the empty arm of
    /// the shortcut-number write finds no array and — because both the image write and the show
    /// happen only when the array lookup succeeds — does nothing at all.
    /// An empty quickbar tile therefore gets its number from somewhere else entirely — its own
    /// `ItemSlot` root's `0x1000001C` frame; see
    /// [`ShortcutBar`](crate::toolbar::shortcuts::ShortcutBar). [verified against the
    /// live element tree]
    pub const SHORTCUT_OVERLAY_ARRAY_EMPTY: u32 = 0x1000_005E;
    /// `UI_ItemList_ShortcutOverlay` — the member id every entry of those three arrays carries.
    pub const SHORTCUT_OVERLAY: u32 = 0x1000_0044;
}

/// The five properties written onto the drag icon and read back from that proxy.
///
/// They are the drag's whole payload: after the source instance properties are copied onto the
/// proxy, that proxy alone says which object (or spell) is in
/// flight and which kind of list it left, and uses nothing else.
/// The drop-item flags — the enum [`inq_drop_icon_info`] packs its four booleans into
/// (`NONE 0`, `IS_CONTAINER 1`, `IS_VENDOR 2`, `IS_SHORTCUT 4`,
/// `IS_SALVAGE 8`).
///
/// Pinned as literals here, and not only used through [`DropIconInfo::flags`]'s shifts, because a
/// test that reads a constant through the same symbol it wrote it through cannot detect a wrong
/// constant.
pub mod drag_flags {
    /// The container bit — the dragged **object** is a container, taken off the drag proxy's
    /// `0x10000011`, written from the source slot's container flag.
    pub const IS_CONTAINER: u32 = 1;
    /// The vendor bit.
    pub const IS_VENDOR: u32 = 2;
    /// The shortcut bit.
    pub const IS_SHORTCUT: u32 = 4;
    /// The salvage bit.
    pub const IS_SALVAGE: u32 = 8;
    /// The client's `flags & 0xE` — vendor, shortcut or salvage.
    pub const NOT_AN_INVENTORY_MOVE: u32 = IS_VENDOR | IS_SHORTCUT | IS_SALVAGE;
}

pub mod drag_attr {
    /// The dragged object's id.
    pub const ITEM_ID: u32 = 0x1000_000F;
    /// The dragged spell's id, when the slot held a spell rather than an object.
    pub const SPELL_ID: u32 = 0x1000_0010;
    /// Drop-item flag bit 0 — the slot's object is itself a container.
    pub const IS_CONTAINER: u32 = 0x1000_0011;
    /// The source list was a shortcut bar.
    pub const IS_SHORTCUT: u32 = 0x1000_0012;
    /// The source list was a vendor's.
    pub const IS_VENDOR: u32 = 0x1000_0013;
    /// The source list was the salvage window's.
    pub const IS_SALVAGE: u32 = 0x1000_0014;
}

/// `SetAttribute_* ` for a value type [`UiSystem`] has no typed setter for:
/// write into the element's `instance_properties`, then `on_set_attribute`.
/// `pub(crate)` because another screen writes the
/// same `0x1000000F`/`0x10000011` pair onto its own drag icon, and a second copy of a
/// three-line attribute writer is worse than a visibility change.
pub(crate) fn set_attr(ui: &mut UiSystem, h: ElemHandle, id: u32, v: PropertyValue) {
    if let Some(n) = ui.node_mut(h) {
        n.instance_properties.set(id, v.clone());
    }
    ui.on_set_attribute(h, id, Some(&v));
}

/// The two decoration children **the layout does not hide by itself** — see [`ItemSlot::rest`].
///
/// The other sixteen children — the ghost, the selection
/// ring, the stack count, the shortcut numeral, the sell and trade markers, the open-container
/// frame and all ten cooldown wedges — each carry `0x3B = true` in the shipped `ItemSlot` layout
/// (`0x21000037`, under overlay container `0x10000346`), and `0x3B` is `UICore_Element_hide`, so
/// **the layout hides them and no code needs to**. Hiding them here as well would only be right
/// if `0x3B` were read the other way up.
///
/// These two are different: the capacity bar (`0x10000347`) and
/// the structure bar (`0x10000348`) are the only two children of `0x10000346` that carry
/// **no** `0x3B` at all, so the layout brings them up visible under either reading. In the client
/// they are taken down by the capacity-display and structure-display updates,
/// neither of which this crate implements — both need the object's items capacity and
/// structure, which belong to the object model. "Hidden" is the answer both functions give for every object this
/// build can describe, so it is applied once at creation and named for what it stands in for.
const QUIET_AT_REST: [u32; 2] = [child::CAPACITY_BAR, child::STRUCTURE_BAR];

/// One slot: a `UiItemWidget` subtree created from the `ItemSlot` layout.
#[derive(Debug, Clone)]
pub struct ItemSlot {
    pub handle: ElemHandle,
    /// The base icon — [`child::ICON`], the element `Self::set_state` acts on.
    pub icon: Option<ElemHandle>,
    /// The ghost.
    pub ghosted: Option<ElemHandle>,
    /// The selection ring.
    pub selected_ring: Option<ElemHandle>,
    /// The stack count.
    pub quantity: Option<ElemHandle>,
    /// The capacity bar — a `Meter`.
    pub capacity_bar: Option<ElemHandle>,
    /// The structure bar — ditto.
    pub structure_bar: Option<ElemHandle>,
    /// The overlay container every decoration below hangs off.
    pub overlays: Option<ElemHandle>,
    /// The text element.
    pub text: Option<ElemHandle>,
    /// The shortcut numeral — the plate [`Self::set_shortcut_num`] writes.
    pub shortcut_num_elem: Option<ElemHandle>,
    /// The drag icon — the hidden 32×32 element `Self::post_init` **creates** (it is not in the
    /// slot's own subtree) and the drag paints the item onto.
    pub drag_icon: Option<ElemHandle>,
    /// **The drop hint**: the child `0x1000045A`.
    ///
    /// It is bound here and driven by [`Self::set_drag_accept_state`]; it is the only thing the
    /// retail client changes while an item is being dragged over a slot. See
    /// [`ItemListWidget::drag_over`].
    pub drag_accept: Option<ElemHandle>,
    /// The drop hint's state, mirrored.
    ///
    /// The client reads the element's own state to skip a
    /// redundant state write; `dereth_ui` has a setter and no getter, so the last value written is
    /// kept here. `None` is "never set", which is not the same as any of the four states.
    pub drag_accept_state: Option<StateId>,
    /// The decorations the client hides whenever it has nothing to say with them. See
    /// [`ItemSlot::rest`].
    quiet: Vec<ElemHandle>,
    /// The item id.
    pub item: Option<ObjectId>,
    /// The spell id. Setting it clears the item id, so a slot
    /// holds **either** an object or a spell and never both. The spellbook uses it: it is the
    /// same `ItemListWidget` with spells in it.
    pub spell: Option<u32>,
    /// `waiting` — the waiting state, the ghost an outstanding move leaves.
    pub waiting: bool,
    /// The slot number this item's numeral was last set for, `-1` for none.
    pub shortcut_num: i32,
    /// Whether that numeral was last set ghosted.
    pub shortcut_ghosted: bool,
    /// Construction sets the slot's quantity to **-1**, and no other client path writes it. Thus
    /// the quantity refresh hides the stack count on every inventory,
    /// quickbar, trade and spellbook slot, and a stack's count reaches the player through
    /// [`Self::tooltip`] instead.
    pub quantity_value: i32,
    /// Whether the object is a container — the client's three-term test.
    pub is_container: bool,
    /// The object's items capacity, mirrored off the same weenie read
    /// [`Self::update_capacity_display`] takes it from.
    ///
    /// It exists so that [`ItemListWidget::drag_over`]'s container branch can be answered from
    /// inside an element-message handler. The client asks the weenie store directly
    /// for the slot's item; this crate has
    /// no store and the `Screen` seam carries no [`crate::view::GameView`] on a message, so the
    /// two fields that answer it ride on the slot. See [`Self::num_empty_item_slots`].
    pub items_capacity: i32,
    /// The number of **loose** items in the object's inventory record.
    /// Mirrored for the same reason as [`Self::items_capacity`].
    pub contained_items: i32,
    /// The tooltip text [`Self::update_tooltip`] last set, mirrored so a test can read it back
    /// without a hover. `None` is a cleared tooltip.
    pub tooltip: Option<String>,

    // ---- the rest of the slot -----------------------------------
    /// The sell marker.
    pub sell_state_elem: Option<ElemHandle>,
    /// The trade marker — ditto.
    pub trade_state_elem: Option<ElemHandle>,
    /// The open-container frame.
    ///
    /// **Only one of the twenty `ItemSlot` roots carries it**: `0x1000033F`, the 36×36 root the two
    /// `UI_ItemList_IsContainer` lists (`0x100001C9` the main pack, `0x100001CA` the side-pack
    /// strip) name in `UI_ItemList_ItemSlotID`. The default 32×32 root `0x1000033A` — the backpack
    /// grid, the paper doll and every quickbar tile — has **no** `0x10000450` at all, so
    /// [`Self::set_open_container_state`] is a no-op on those and that is retail's behaviour, not a
    /// gap. [verified by resolving all 20 roots of layout `0x21000037`, and by scanning
    /// all 101 shipped layouts: `0x10000450` occurs in `0x21000037` and nowhere else]
    pub open_container: Option<ElemHandle>,
    /// The ten cooldown wedges, in [`child::COOLDOWN`] order.
    pub cooldown: [Option<ElemHandle>; 10],
    /// `selected` — the copy the slot update compares against the object's own `selected`.
    pub selected: bool,
    /// Construction sets it to `true`, so a slot is selectable until the list says otherwise.
    pub selectable: bool,
    /// The sell state, stored as the client stores it: `(object sell state != 0)`.
    pub sell_state: bool,
    /// The trade state.
    pub trade_state: bool,
    /// The delayed shortcut number, `-1` for none — see [`Self::set_delayed_shortcut_num`].
    pub delayed_shortcut_num: i32,
    /// The object's cooldown id, cached off the last decoration pass so the heartbeat can
    /// re-run [`Self::update_cooldown_display`] without another snapshot. `0` is "no cooldown".
    pub cooldown_id: u32,
    /// The object's cooldown duration, likewise.
    pub cooldown_duration: f64,
    /// The last heartbeat time — the global-message handler's gate.
    pub last_heartbeat: f64,
    /// Which of the ten wedges [`Self::update_cooldown_display`] last left up, `None` for none.
    /// Mirrored so a test can read the state machine without going through the element tree.
    pub cooldown_wedge: Option<usize>,
    /// Whether the object can be opened — the slot update's own two-armed test.
    ///
    /// **Write-only in the shipped client too**: the flag is assigned in the constructor and in
    /// the slot update and read by nothing else, so no frame can falsify it. The
    /// open-container *frame* comes from the list's open item id, which is a different
    /// mechanism — see
    /// [`ItemListWidget::update_open_container_indicator`].
    pub is_openable: bool,
    /// Whether the object holds containers — a non-zero containers capacity. Write-only in the client for the same
    /// reason.
    pub is_container_holder: bool,
}

/// Construction sets the heartbeat interval to **1.0** (a double).
pub const HEARTBEAT_INTERVAL: f64 = 1.0;

/// The global message the item slot registers for and acts on.
pub const HEARTBEAT_MESSAGE: u32 = 3;

impl ItemSlot {
    fn bind(ui: &UiSystem, handle: ElemHandle) -> Self {
        let get = |id: u32| ui.get_child_recursive(handle, ElementId(id));
        let mut quiet = Vec::new();
        for id in QUIET_AT_REST {
            if let Some(h) = get(id) {
                quiet.push(h);
            }
        }
        Self {
            handle,
            icon: get(child::ICON),
            ghosted: get(child::GHOSTED),
            selected_ring: get(child::SELECTED),
            quantity: get(child::QUANTITY),
            capacity_bar: get(child::CAPACITY_BAR),
            structure_bar: get(child::STRUCTURE_BAR),
            overlays: get(child::OVERLAYS),
            text: get(child::TEXT),
            shortcut_num_elem: get(child::SHORTCUT_NUM),
            drag_icon: None,
            drag_accept: get(child::DRAG_ACCEPT),
            drag_accept_state: None,
            quiet,
            item: None,
            spell: None,
            waiting: false,
            shortcut_num: -1,
            shortcut_ghosted: false,
            // The constructor's own value.
            quantity_value: -1,
            is_container: false,
            items_capacity: 0,
            contained_items: 0,
            tooltip: None,
            sell_state_elem: get(child::SELL_STATE),
            trade_state_elem: get(child::TRADE_STATE),
            open_container: get(child::OPEN_CONTAINER),
            cooldown: child::COOLDOWN.map(get),
            selected: false,
            selectable: true,
            sell_state: false,
            trade_state: false,
            delayed_shortcut_num: -1,
            cooldown_id: 0,
            cooldown_duration: 0.0,
            last_heartbeat: 0.0,
            cooldown_wedge: None,
            is_openable: false,
            is_container_holder: false,
        }
    }

    /// A slot with nothing bound — what [`Self::bind`] produces against an element with no
    /// children. The tests below build their synthetic slots from it so that adding a child to
    /// [`Self`] does not mean editing every one of them.
    #[must_use]
    pub fn bare(handle: ElemHandle) -> Self {
        Self {
            handle,
            icon: None,
            ghosted: None,
            selected_ring: None,
            quantity: None,
            capacity_bar: None,
            structure_bar: None,
            overlays: None,
            text: None,
            shortcut_num_elem: None,
            drag_icon: None,
            drag_accept: None,
            drag_accept_state: None,
            quiet: Vec::new(),
            item: None,
            spell: None,
            waiting: false,
            shortcut_num: -1,
            shortcut_ghosted: false,
            quantity_value: -1,
            is_container: false,
            items_capacity: 0,
            contained_items: 0,
            tooltip: None,
            sell_state_elem: None,
            trade_state_elem: None,
            open_container: None,
            cooldown: [None; 10],
            selected: false,
            selectable: true,
            sell_state: false,
            trade_state: false,
            delayed_shortcut_num: -1,
            cooldown_id: 0,
            cooldown_duration: 0.0,
            last_heartbeat: 0.0,
            cooldown_wedge: None,
            is_openable: false,
            is_container_holder: false,
        }
    }

    /// The **run-time** half of item-widget initialization, which is everything after
    /// the twenty-two child lookups [`Self::bind`] already makes:
    ///
    /// ```text
    /// create the drag icon 0x10000345 from the same ItemSlot layout (enum 0x10000038)
    /// hide the drag icon
    /// make the slot mouse-visible
    /// property 0x36 = true    // drop catcher
    /// property 0x3A = false   // not dragable
    /// property 0x39 = false   // may spawn a proxy
    /// register for global message 3
    /// ```
    ///
    /// **Without three of those lines nothing in the inventory can be touched.** List boxes have
    /// the folded hit-test predicate — `true` except in state `0x0D` — but an item
    /// slot (type `0x10000032`) is registered through
    /// [`crate::game_element`] like every other game type and therefore *has* no such override; it
    /// is mouse-visible because **slot initialisation says so explicitly**, and
    /// with that line missing, hit testing returns the panel behind the
    /// slot and every press misses: the grid `0x100001C6` and all 102 of its `0x1000033A` slots
    /// read `is_mouse_visible == false`. [verified on the live tree]
    ///
    /// `0x36` is the other half: the drop **catcher** is the slot, not the list.
    /// The item-list element-message handler's `0x15` arm takes the source element, verifies that
    /// it is an item slot, and restores drag-accept state `0x1000003F`. Drop handling raises that message on
    /// the element that accepted the drop, so for the cast to ever succeed the accepting
    /// element must be the slot. \[inferred\]
    ///
    /// `drag_icon` is the element [`crate::env::create_child_element_by_enum`] just made; it is
    /// passed in rather than created here so this stays testable without an installed environment.
    fn post_init(&mut self, ui: &mut UiSystem, drag_icon: Option<ElemHandle>) {
        self.drag_icon = drag_icon;
        if let Some(h) = drag_icon {
            ui.set_visible(h, false);
        }
        ui.set_mouse_visible(self.handle, true);
        ui.set_attribute_bool(self.handle, dereth_ui::props::attr::DROP_CATCHER, true);
        ui.set_attribute_bool(self.handle, dereth_ui::props::attr::DRAGABLE, false);
        ui.set_attribute_bool(self.handle, dereth_ui::props::attr::NO_DRAG_PROXY, false);
    }

    /// Paint the drag icon with what this slot holds and label it with what is being dragged.
    ///
    /// With no drag icon, or an empty slot, it returns false — an empty slot cannot be picked
    /// up. For an object it sets the drag icon to the object's drag surface (blit 3-alpha) and
    /// writes `0x1000000F` = item id, `0x10000011` = is-container, and `0x10000013` / `0x10000012`
    /// / `0x10000014` = the list's vendor / shortcut / salvage flags. For a spell it sets the
    /// spell icon, `0x10000010` = spell id and `0x10000012` = the shortcut flag.
    ///
    /// **The five properties are the payload, not decoration.** [`inq_drop_icon_info`] reads
    /// them straight back off the drag proxy, and that — not the drag's owner — is how
    /// the drop learns *which object* was dropped. They survive onto the
    /// proxy because the proxy copies the drag icon's instance properties.
    ///
    /// **The image is the drag surface, not the main surface.** The client paints the drag icon with the *first* of
    /// the two surfaces the icon renderer builds: icon, custom overlay and the effects colour
    /// replacement, with **no** item-type tile and **no** custom underlay. Those two belong to
    /// the main surface, the one the slot itself draws.
    ///
    /// The base icon's `GraphicRef` carries a whole [`dereth_ui::region::IconRecipe`], and cloning
    /// it wholesale would put the opaque `UIIconBackgrounds` tile under the cursor. The proxy takes
    /// [`IconRecipe::drag_surface`](dereth_ui::region::IconRecipe::drag_surface), which is the same
    /// recipe with those two layers removed and is the drag surface exactly.
    ///
    /// A plain (non-recipe) image is passed through unchanged: a spell tile and a slot whose
    /// picture was set by hand have no second surface in the client either.
    fn prepare_drag_icon(&self, ui: &mut UiSystem, list: &ItemListWidget) -> bool {
        let Some(d) = self.drag_icon else {
            return false;
        };
        if self.item.is_none() && self.spell.is_none() {
            return false;
        }
        let mut image = self
            .icon
            .and_then(|h| ui.node(h).and_then(|n| n.region.image.clone()));
        if let Some(g) = image.as_mut() {
            if let Some(dereth_ui::region::SurfaceOp::Icon(r)) = g.op {
                let drag = r.drag_surface();
                g.op = Some(dereth_ui::region::SurfaceOp::Icon(drag));
                // `GraphicRef::did` is the recipe's base and is part of the host's texture key;
                // it has to follow the recipe or the drag proxy would share a cache slot with the
                // slot's own composite.
                g.did = drag.base().unwrap_or(g.did);
            }
        }
        if let Some(n) = ui.node_mut(d) {
            n.region.image = image;
            n.region.blit_mode = dereth_ui::region::BlitMode::Alpha3;
        }
        if let Some(id) = self.item {
            set_attr(ui, d, drag_attr::ITEM_ID, PropertyValue::InstanceId(id.0));
            set_attr(
                ui,
                d,
                drag_attr::IS_CONTAINER,
                PropertyValue::Bool(list.container_list),
            );
            set_attr(
                ui,
                d,
                drag_attr::IS_VENDOR,
                PropertyValue::Bool(list.vendor_list),
            );
            set_attr(
                ui,
                d,
                drag_attr::IS_SHORTCUT,
                PropertyValue::Bool(list.shortcut_list),
            );
            set_attr(
                ui,
                d,
                drag_attr::IS_SALVAGE,
                PropertyValue::Bool(list.salvage_list),
            );
        } else if let Some(id) = self.spell {
            set_attr(ui, d, drag_attr::SPELL_ID, PropertyValue::InstanceId(id));
            set_attr(
                ui,
                d,
                drag_attr::IS_SHORTCUT,
                PropertyValue::Bool(list.shortcut_list),
            );
        }
        true
    }

    /// The state goes on **three** children and not on the item element itself: the base icon
    /// (without which it returns false), the text and the overlay container. Setting the empty
    /// state `0x1000001C` also clears the tooltip flag, the container flag and `selected`, clears the
    /// tooltip, and hides the selection ring.
    ///
    /// The overlays child matters: `0x1000033C` is the parent of the capacity and structure bars,
    /// the ghost, the selection ring, the shortcut numeral, the sell and trade markers and the ten
    /// cooldown wedges, and it declares the same two states, each carrying attribute `0x3B` —
    /// `true` (hidden) in `0x1000001C` and `false` (shown) in `0x1000001D`. A rebuild that sets
    /// only the icon's state leaves the whole container up on an empty slot, and each slot draws
    /// as a pile of overlapping markers.
    fn set_state(&mut self, ui: &mut UiSystem, s: StateId) -> bool {
        let Some(icon) = self.icon else { return false };
        ui.set_state(icon, s);
        if let Some(h) = self.text {
            ui.set_state(h, s);
        }
        if let Some(h) = self.overlays {
            // The overlays container declares `0x3B = true` in state `0x1000001C` and
            // `0x3B = false` in `0x1000001D`; `0x3B` is `UICore_Element_hide`, so this one
            // state write is what takes the decorations down on an empty slot and puts them back on
            // a full one. No hand correction of the sense belongs here: `0x3B` is read the right way
            // up at the source, and a second inversion would undo it.
            ui.set_state(h, s);
        }
        if s == item_state::EMPTY {
            // Clear `selected` and hide the selected ring.
            self.selected = false;
            if let Some(h) = self.selected_ring {
                ui.set_visible(h, false);
            }
        }
        true
    }

    /// The UI item element's shortcut-number write — put the slot's numeral on the shortcut
    /// numeral element, or take it down.
    ///
    /// A negative number hides the numeral. Otherwise it picks an array — `0x1000005E` when the
    /// base icon is in the empty state `0x1000001C`, else `0x10000043` when ghosted, else
    /// `0x10000042` — and, only if the numeral element carries that array, sets the numeral's
    /// image to entry `num` (draw mode 3) and shows it. It then records the number and the
    /// ghosted flag on the slot and, when the slot has an object, writes the pair back to it.
    ///
    /// Both the image and the show sit **inside** the array lookup, which is
    /// load-bearing: nothing in the shipped tree carries
    /// [`attr::SHORTCUT_OVERLAY_ARRAY_EMPTY`], so a call on an empty slot leaves the numeral
    /// exactly where it was. The last line — the write back to the weenie — belongs to the object
    /// model and has no place in this crate, which never writes objects; the slot number reaches a slot from
    /// [`crate::toolbar::shortcuts::ShortcutBar`] instead, which is where the client's own
    /// shortcut insert puts it.
    ///
    /// Returns true when the numeral element was written.
    pub fn set_shortcut_num(&mut self, ui: &mut UiSystem, num: i32, ghosted: bool) -> bool {
        self.shortcut_num = num;
        self.shortcut_ghosted = ghosted;
        let Some(h) = self.shortcut_num_elem else {
            return false;
        };
        if num < 0 {
            ui.set_visible(h, false);
            return true;
        }
        let empty = self
            .icon
            .and_then(|i| ui.node(i))
            .is_some_and(|n| n.state == item_state::EMPTY);
        let which = if empty {
            attr::SHORTCUT_OVERLAY_ARRAY_EMPTY
        } else if ghosted {
            attr::SHORTCUT_OVERLAY_ARRAY_GHOSTED
        } else {
            attr::SHORTCUT_OVERLAY_ARRAY
        };
        let Some(entries) = shortcut_overlay_array(ui, h, which) else {
            return false;
        };
        if let Some(did) = entries
            .get(usize::try_from(num).unwrap_or(usize::MAX))
            .copied()
        {
            // Replace the media machine with one image
            // step. The third argument is the draw mode, which this rebuild carries on the step and
            // does not use when painting.
            if let Some(n) = ui.node_mut(h) {
                n.region.image = Some(dereth_ui::GraphicRef::opaque_surface(did, 0, 0));
            }
        }
        ui.set_visible(h, true);
        true
    }

    /// The two hides that are still this crate's to make — see
    /// [`QUIET_AT_REST`], which says why the other sixteen are not.
    ///
    /// The client hides the capacity bar when the object is not a
    /// container or its items capacity is `< 1`, and does the
    /// same for the structure bar. It runs once, when the slot is created, because that is the
    /// answer both give for every object this build can describe.
    fn rest(&self, ui: &mut UiSystem) {
        for h in &self.quiet {
            ui.set_visible(*h, false);
        }
    }

    /// The ui item element's set-icon — clear the image, set blit mode normal, and set the
    /// object's icon on the base icon.
    fn set_icon(&self, ui: &mut UiSystem, did: Option<DataId>) {
        let Some(icon) = self.icon else { return };
        if let Some(n) = ui.node_mut(icon) {
            n.region.image = did.map(|d| dereth_ui::GraphicRef::world_surface(d, 0, 0));
        }
    }

    /// The client's set-icon on the base icon, with the composite the object's icon actually
    /// is rather than the raw icon id.
    ///
    /// The recipe travels on the element's own `GraphicRef` and the host composites it once per
    /// distinct recipe, which is where the icon data lives in the client: keyed by object id in
    /// the icon-data table and rebuilt only when one of the client's five compared fields has
    /// moved. Here the recipe **is** those
    /// five fields, so an unchanged object produces an equal recipe and an equal cache key, and a
    /// changed one produces a different key — the invalidation is the identity rather than a
    /// second mechanism that could disagree with it.
    ///
    /// `None`, and a `Some` whose recipe resolves no base at all, are the client's
    /// null texture: the element draws nothing. That is the client's answer too — the icon
    /// renderer leaves the main surface null when creating the local surface fails — and it is
    /// deliberately **not**
    /// "fall back to the raw icon id", because a silent fallback to a picture that is nearly
    /// right is how a composite that stopped resolving would go unnoticed.
    ///
    /// Returns true when the element's picture changed, so a caller can count it.
    pub fn set_icon_composite(
        &mut self,
        ui: &mut UiSystem,
        recipe: Option<dereth_ui::region::IconRecipe>,
    ) -> bool {
        let Some(icon) = self.icon else { return false };
        let want = recipe.and_then(|r| {
            let base = r.base()?;
            Some(dereth_ui::GraphicRef {
                did: base,
                source: dereth_ui::ImageSource::World,
                width: 0,
                height: 0,
                opaque: None,
                op: Some(dereth_ui::region::SurfaceOp::Icon(r)),
            })
        });
        let Some(n) = ui.node_mut(icon) else {
            return false;
        };
        if n.region.image == want {
            return false;
        }
        n.region.image = want;
        true
    }

    /// The recipe this slot's icon element is currently drawing, if it is a composite at all.
    #[must_use]
    pub fn icon_recipe(&self, ui: &UiSystem) -> Option<dereth_ui::region::IconRecipe> {
        ui.node(self.icon?)?
            .region
            .image
            .as_ref()?
            .op?
            .icon_recipe()
    }

    /// Clear the slot, plus the empty state the three callers always pair it with.
    ///
    /// **It deliberately does not clear the image.** Clearing a slot writes four fields and touches
    /// no element; what puts the empty-slot frame back is the state, because `0x1000033B`'s state
    /// `0x1000001C` carries **one media step** and its `0x1000001D` carries none. Clearing the
    /// image here instead would leave an empty slot as a hole — the shipped `ItemSlot` layout has
    /// no base image on the icon at all. [verified against the live element tree]
    fn clear(&mut self, ui: &mut UiSystem) {
        self.item = None;
        // The client's clear writes four fields: the item id, the spell id and the container
        // display to zero, and the object reference to null.
        self.spell = None;
        super::runtime::set_identity(ui, self.handle, ObjectId(0), 0);
        self.waiting = false;
        self.set_state(ui, item_state::EMPTY);
        if let Some(h) = self.ghosted {
            ui.set_visible(h, false);
        }
        // The client's no-object arm: clear the tooltip flag and the tooltip, which setting the
        // empty state `0x1000001C` does again for the same reason.
        //
        // **Clearing the tooltip re-evaluates mouse visibility**, so this is the line
        // that would take an empty slot back out of the hit test if the slot's mouse-visibility
        // rested on its tooltip. It does not: slot initialisation sets the explicit
        // mouse-visible flag, and the visibility update ORs the two. See
        // [`ItemSlot::update_tooltip`].
        self.tooltip = None;
        ui.set_attribute_bool(self.handle, dereth_ui::props::attr::TOOLTIP_ON, false);
        ui.clear_tooltip(self.handle);
        // The two bars the layout does not hide by itself go back down with the item — see
        // [`QUIET_AT_REST`]. In the client the same thing happens one level up, because
        // setting the slot's state puts the overlay container (their grandparent) into `0x1000001C`,
        // whose `0x3B` is `true`.
        self.is_container = false;
        self.quantity_value = -1;
        // Clearing a slot nulls its object, which is what makes the cooldown
        // display's next pass take the `on = false` arm and put all ten wedges
        // down; here the weenie's two fields are cached on the slot, so dropping them is the same
        // fact. The sell and trade states are deliberately **not** reset — the client does
        // not reset them either, and a slot reused for another object compares against what it
        // last drew.
        self.cooldown_id = 0;
        self.cooldown_duration = 0.0;
        self.is_openable = false;
        self.is_container_holder = false;
        // Clearing a slot nulls its object, and a slot
        // with no object has no icon data, so there is no main surface and no background tile —
        // the cell falls back to the base icon's own `0x1000001C` frame.
        //
        // **There is no line here for that, and its absence is the point.** A tile on the
        // `UiItemWidget` root would need taking down by hand, because the slot's state write never
        // touches that element. The composite lives on the base icon, and
        // `set_state(EMPTY)` five lines above **is** `0x1000001C`'s single media step putting
        // `0x06004D20` back over it — which is exactly why clearing an item writes four fields and
        // touches no additional element. A GPU test opens a side pack and asserts every
        // composite comes down.
        self.rest(ui);
    }

    /// Spell-shortcut initialisation + the spell arm of the ui item update + the set-icon
    /// `else` branch.
    ///
    /// Initialisation clears the item id, sets the spell id, clears the container display and
    /// sets no object. The
    /// update, with no object and a spell, sets state `0x1000001D`, sets the icon and returns. The
    /// set-icon clears the base icon's image, sets blit mode **3-alpha** (not normal), sets the
    /// spell icon and, when the slot has a text element, writes the spell name into it.
    ///
    /// Two things separate this from [`Self::set_icon`]. A spell slot uses a **3-alpha blit**
    /// where an item slot uses a normal blit, because a spell icon is a three-channel alpha
    /// surface; and a spell slot is the only caller that writes the text element (`0x10000344`),
    /// which is why an item slot's text child stays empty and a spellbook row shows a name.
    ///
    /// The slot update's spell path also never runs the weenie half — no selection ring, no capacity
    /// or structure bar, no quantity, no cooldown — because it returns before all of it.
    ///
    /// **The spell-icon read is a composite too, and a different one.** It
    /// caches in a spell-icon table and builds a miss with
    /// the magic system's spell-icon composite, which is four surfaces out of
    /// `0x10000006 UISpellBackgrounds` and `0x10000007 UISpellOverlays` — **not**
    /// the raw icon on its own. See
    /// [`spell_recipe`]. The blit mode above stays 3-alpha because that is what the
    /// set-icon's else-branch uses, whatever the picture is.
    fn set_spell(
        &mut self,
        ui: &mut UiSystem,
        id: u32,
        icon: Option<DataId>,
        name: &str,
        recipe: Option<dereth_ui::region::IconRecipe>,
    ) {
        self.item = None;
        self.spell = Some(id);
        super::runtime::set_identity(ui, self.handle, ObjectId(0), id);
        self.set_state(ui, item_state::OCCUPIED);
        if let Some(h) = self.icon {
            if let Some(n) = ui.node_mut(h) {
                n.region.blit_mode = dereth_ui::region::BlitMode::Alpha3;
                n.region.image = icon.map(|d| dereth_ui::GraphicRef::world_surface(d, 0, 0));
            }
        }
        // After the plain image write, because the two are one image write in the client and the
        // composite is what it passes. `IconRecipe::base` is `background.or(icon)`, so a recipe
        // whose `UISpellBackgrounds` row did not resolve still names the spell's own icon and
        // still composites to it: a 4-alpha blit onto a surface with alpha 0 is the source.
        self.set_icon_composite(ui, recipe);
        if let Some(t) = self.text.and_then(|h| ui.text_element_mut(h)) {
            t.set_text(name);
        }
    }

    /// Show the ghost. The client
    /// has an `unghostable` opt-out per item; nothing in this build sets it.
    fn set_waiting(&mut self, ui: &mut UiSystem, waiting: bool) {
        self.waiting = waiting;
        if let Some(h) = self.ghosted {
            ui.set_visible(h, waiting);
        }
    }

    /// Set the slot's quantity and nothing else.
    ///
    /// **Its only caller in the whole client writes the amount a vendor advertises and `-1` when
    /// that is below 1.** Nothing on the
    /// inventory path calls it, so [`Self::update_quantity_display`] hides the text on every
    /// backpack, paper-doll, quickbar and spellbook slot — which is why the recorded retail
    /// backpack shows **no numbers on its icons**, and
    /// why a rebuild that painted the stack size there would look wrong against retail rather than
    /// right.
    pub const fn set_quantity(&mut self, n: i32) {
        self.quantity_value = n;
    }

    /// The ui item element's quantity display update.
    ///
    /// A missing quantity element returns immediately. A negative signed quantity hides it;
    /// otherwise the element receives the decimal quantity text and becomes visible.
    ///
    /// The format is `"%d"` — the plain number, no separators and no brackets. [verified: the
    /// retail formatter's only format string is `"%d"`.]
    ///
    /// Returns true when the text element existed to write.
    pub fn update_quantity_display(&mut self, ui: &mut UiSystem) -> bool {
        let Some(h) = self.quantity else { return false };
        if self.quantity_value < 0 {
            ui.set_visible(h, false);
            return true;
        }
        if let Some(t) = ui.text_element_mut(h) {
            t.set_text(&format!("{}", self.quantity_value));
        }
        ui.set_visible(h, true);
        true
    }

    /// The capacity bar's display update.
    ///
    /// With no bar or no object it does nothing. A non-container, or an items capacity `< 1`, hides
    /// the bar. Otherwise `r = contained items / items capacity` (as doubles); `r == 0.0` hides
    /// the bar, and anything else shows it with `0x69 = clamp(r, 0, 1)` as a float.
    ///
    /// **An empty pack shows no bar at all**, which is the `r == 0.0` arm and is not the same as a
    /// bar at zero: the shipped `ItemSlot` layout gives `0x10000347` no `0x3B`, so a bar left up
    /// with `0x69 = 0` would draw its own frame over the icon of every empty container.
    ///
    /// A missing object is the caller's problem here: [`ItemListWidget::decorate`] runs this only
    /// for a slot that holds an object, which is exactly the guard.
    pub fn update_capacity_display(
        &mut self,
        ui: &mut UiSystem,
        items_capacity: i32,
        contained_items: i32,
    ) -> bool {
        let Some(h) = self.capacity_bar else {
            return false;
        };
        if !self.is_container || items_capacity < 1 {
            ui.set_visible(h, false);
            return true;
        }
        let r = meter_ratio(contained_items, items_capacity);
        if r == 0.0 {
            ui.set_visible(h, false);
            return true;
        }
        ui.set_visible(h, true);
        set_attr_float(ui, h, crate::bind::attr::METER_LEVEL, r.clamp(0.0, 1.0));
        true
    }

    /// The structure bar's display update.
    ///
    /// With no bar or no object it does nothing. A max structure of 0 hides the bar. Otherwise
    /// `r = structure / max structure` (as doubles); `r == 1.0` hides the bar, and anything else
    /// shows it with `0x69 = clamp(r, 0, 1)` as a float.
    ///
    /// The early-out is the **mirror image** of the capacity bar's: a container shows nothing when
    /// it is *empty*, and a structured item shows nothing when it is *whole*. Getting that pair
    /// the wrong way round draws a full bar on every undamaged lockpick in the pack.
    pub fn update_structure_display(
        &mut self,
        ui: &mut UiSystem,
        structure: u32,
        max_structure: u32,
    ) -> bool {
        let Some(h) = self.structure_bar else {
            return false;
        };
        if max_structure == 0 {
            ui.set_visible(h, false);
            return true;
        }
        let r = meter_ratio_u32(structure, max_structure);
        if r == 1.0 {
            ui.set_visible(h, false);
            return true;
        }
        ui.set_visible(h, true);
        set_attr_float(ui, h, crate::bind::attr::METER_LEVEL, r.clamp(0.0, 1.0));
        true
    }

    /// **The stack count a player actually sees.**
    ///
    /// A missing item id or object returns. Stack size zero becomes one; a stack above one selects
    /// the plural name and prefixes it with the signed count plus one space. The resulting text is
    /// assigned as the tooltip and the tooltip-present flag is set.
    ///
    /// The format is `"%d %s"` — *count then name*, no brackets. \[verified: the only string the
    /// function uses; corroborated by the examination panel's title write, which builds the
    /// examine window's title as the stack size, a space and the name for exactly the same
    /// `s >= 2` case\]
    ///
    /// `NAME_PLURAL` takes the plural name and falls back to the name when that buffer is empty
    /// in the object-name read, which is what `plural` being `None` means.
    ///
    /// # This is the line that makes a slot hit-testable, and it must not be the only one
    ///
    /// The tooltip write's tail re-evaluates whether the element should be mouse-visible (it has
    /// a context menu or valid tooltip text) and sets its mouse visibility from that — which is
    /// how the radar becomes clickable over a blip. An item slot does **not**
    /// depend on that: `Self::post_init` makes it mouse-visible outright, and the visibility
    /// update ORs that explicit flag with the tooltip-derived one, so setting and clearing the
    /// tooltip moves the slot neither into nor out of the hit test.
    /// Asserted, because that is the easy thing to get wrong.
    ///
    /// The tooltip flag is property `0x4B` `UICore_Element_tooltip`, and the tooltip builder
    /// wants **both** it and non-empty text before it
    /// will build a tooltip element — so writing the text alone shows nothing on hover.
    pub fn update_tooltip(
        &mut self,
        ui: &mut UiSystem,
        name: &str,
        plural: Option<&str>,
        stack_size: u32,
    ) -> bool {
        if self.item.is_none() {
            return false;
        }
        let s = stack_size.max(1);
        let text = if s > 1 {
            // Through [`non_empty`], not a fourth hand-written copy of the same test: this line is
            // the one that would hide a divergence between the two
            // halves of the pack's early-out, by re-normalising on the way to the screen.
            let n = non_empty(plural).unwrap_or(name);
            format!("{s} {n}")
        } else {
            name.to_string()
        };
        self.tooltip = Some(text.clone());
        ui.set_tooltip(self.handle, Some(text));
        ui.set_attribute_bool(self.handle, dereth_ui::props::attr::TOOLTIP_ON, true);
        true
    }

    // ---- slot state ------------------------------------------------------------------------

    /// The ui item element's selected state write — the selection ring.
    ///
    /// A clear always goes through; a set only when the slot is selectable. Either way it stores
    /// `selected` and shows the selection ring exactly when `v != 0`.
    ///
    /// **The selectable flag gates only the *set*, never the *clear*.** A slot that
    /// [`ItemListWidget::handle_single_selection`] has made unselectable can still be deselected,
    /// which is
    /// exactly what that function does to the duplicates of a selected object before it selects
    /// the one under the pointer.
    ///
    /// Returns true when the ring element existed to write.
    pub fn set_selected_state(&mut self, ui: &mut UiSystem, selected: bool) -> bool {
        if selected && !self.selectable {
            return false;
        }
        self.selected = selected;
        let Some(h) = self.selected_ring else {
            return false;
        };
        ui.set_visible(h, selected);
        true
    }

    /// **The drop hint.**
    ///
    /// With no drop-hint child it does nothing; when the child is already in the requested state
    /// it does nothing; otherwise it sets the child's state.
    ///
    /// Both early-outs matter and are reproduced: a slot root without the child does nothing (the
    /// `ItemSlot` layout `0x21000037` carries it on every root, so this is unreachable in the
    /// shipped tree, as measured on the live tree), and re-setting the state a slot is
    /// already in raises no state write and therefore no media step. Returns whether a state
    /// write was actually issued, so a test can see the edge rather than only the end state.
    /// The "number of empty item slots" answer, off this slot's mirrored
    /// weenie fields.
    ///
    /// An items capacity of `-1` (unbounded) answers `-1`; otherwise it is the capacity minus
    /// the loose items in the object's inventory record, or the capacity when there is no record.
    ///
    /// **`-1` is *has room*, not *no room*.** [`ItemListWidget::drag_over`]'s test is `!= 0`, and
    /// the signed convention is what makes an unbounded container answer `INTO_CONTAINER`. The
    /// no-inventory-record arm needs no separate case here: an object with no inventory record
    /// carries `contained_items == 0`, so `cap - 0 == cap` is the client's answer.
    #[must_use]
    pub const fn num_empty_item_slots(&self) -> i64 {
        match self.items_capacity {
            dereth_rules::capacity::UNLIMITED => -1,
            cap => cap as i64 - self.contained_items as i64,
        }
    }

    pub fn set_drag_accept_state(&mut self, ui: &mut UiSystem, s: StateId) -> bool {
        let Some(h) = self.drag_accept else {
            return false;
        };
        if self.drag_accept_state == Some(s) {
            return false;
        }
        self.drag_accept_state = Some(s);
        ui.set_state(h, s);
        true
    }

    /// Set the slot's selectable flag and nothing else.
    pub const fn set_selectable_state(&mut self, selectable: bool) {
        self.selectable = selectable;
    }

    /// The ui item element's delayed shortcut num write — store the number and nothing else.
    ///
    /// The client calls it when a shortcut names an object the client has not seen:
    /// there is no weenie to write the number onto, so the slot keeps it and the slot update's
    /// "delayed number is not `-1`" arm pushes it into the weenie on the first pass where one
    /// exists, then resets the field to `-1`.
    pub const fn set_delayed_shortcut_num(&mut self, n: i32) {
        self.delayed_shortcut_num = n;
    }

    /// The frame on an **open** pack's icon.
    ///
    /// With no open-container frame it does nothing; otherwise it sets the frame's visibility
    /// only when that differs from `state`.
    ///
    /// The early-out on the element's *current* visibility is the client's, and it is why this
    /// returns false both for "no such child" and for "already in that state".
    ///
    /// See [`Self::open_container`] for why "no such child" is the answer on nineteen of the
    /// twenty shipped slot roots.
    pub fn set_open_container_state(&mut self, ui: &mut UiSystem, state: bool) -> bool {
        let Some(h) = self.open_container else {
            return false;
        };
        if ui.node(h).is_some_and(|n| n.region.flags.visible) == state {
            return false;
        }
        ui.set_visible(h, state);
        true
    }

    /// The ui item element's cooldown display update — **the cooldown wedge.**
    ///
    /// With no structure bar it does nothing (see the guard discussion below). Otherwise, when
    /// the object has a cooldown id `> 0` and the player's enchantment registry reports cooldown
    /// `id + 0x8000` as running with `remaining` seconds left, it is `on`, with
    /// `n = trunc(remaining / cooldown duration * 100.0 * 0.1 + 1.0)` when the duration is
    /// positive (else `n = 0`). Wedge *k* (10 %…90 %) is shown when `on && n == k`, and the
    /// 100 % wedge when `on && n >= 10`.
    ///
    /// **Exactly one wedge is up at a time.** Nine of the ten arms are an *equality*; only the
    /// tenth is a threshold. A rebuild that read the ten as cumulative layers
    /// — the obvious reading of the names — would light ten overlays at
    /// once on a fresh cooldown, and every one of them is a full-slot 32×32 image.
    ///
    /// **The index counts down.** `remaining / duration` starts at 1 and falls to 0, so
    /// `n = trunc(10 r + 1)` runs 11 → 1: the *100* wedge (`n >= 10`) at the start, the *10* wedge
    /// (`n == 1`) at the end, and nothing once the cooldown expires. The three constants are
    /// `100.0`, `0.1` and `1.0`, applied in that order and
    /// deliberately not folded, because `r/d*100.0*0.1` and `r/d*10.0` are not the same double.
    ///
    /// **The guard is on the structure bar, and that is a shipped bug rather than a mis-read.**
    /// The retail client tests the structure bar, not a cooldown wedge — a copy-and-paste from
    /// the structure display, whose first line is the same test.
    /// It is invisible in retail because every `ItemSlot` root carries both children, and it is
    /// reproduced here so that a slot root without a structure bar behaves as the client would.
    /// [verified against retail]
    ///
    /// `remaining` is the registry's remaining-time answer: `None` means the registry said *not on
    /// cooldown* (or there is no registry), which is the `on = false` arm.
    ///
    /// Returns the index into [`child::COOLDOWN`] of the wedge left visible, or `None`.
    pub fn update_cooldown_display(
        &mut self,
        ui: &mut UiSystem,
        remaining: Option<f64>,
    ) -> Option<usize> {
        // The whole body is inside the structure-bar test.
        self.structure_bar?;
        let mut on = false;
        let mut n = 0_i32;
        if self.cooldown_id > 0 {
            if let Some(r) = remaining {
                if self.cooldown_duration > 0.0 {
                    // The client truncates toward zero; a raw `as i32` saturates instead.
                    n = dereth_primitives::num::to_i32_f64(
                        r / self.cooldown_duration * 100.0 * 0.1 + 1.0,
                    );
                }
                on = true;
            }
        }
        let mut up = None;
        for (i, h) in self.cooldown.iter().enumerate() {
            let Some(h) = *h else { continue };
            let k = i32::try_from(i).unwrap_or(0) + 1;
            let v = on && if i == 9 { n >= 10 } else { n == k };
            ui.set_visible(h, v);
            if v {
                up = Some(i);
            }
        }
        self.cooldown_wedge = up;
        up
    }

    /// The ui item element's heartbeat step, driven by
    /// the global-message handler's message 3.
    ///
    /// On message 3, once `now >= last heartbeat + interval`, it records `now` as the last
    /// heartbeat and runs the heartbeat.
    ///
    /// The heartbeat does nothing but run the cooldown-display update. So the heartbeat *is* the
    /// cooldown display and there is nothing else in it.
    ///
    /// This is the piece that makes a wedge move. Every other decoration in this file is redrawn
    /// when the panel's snapshot changes, and a cooldown ticking down changes no snapshot at all —
    /// so without a once-a-second pass the wedge would freeze on whichever step it was drawn at.
    ///
    /// Returns true when the interval had elapsed and the display was re-run.
    pub fn listen_to_global_message(
        &mut self,
        ui: &mut UiSystem,
        msg: u32,
        now: f64,
        remaining: Option<f64>,
    ) -> bool {
        if msg != HEARTBEAT_MESSAGE || self.last_heartbeat + HEARTBEAT_INTERVAL > now {
            return false;
        }
        self.last_heartbeat = now;
        self.update_cooldown_display(ui, remaining);
        true
    }

    /// The sell-state arm of the client's slot-update tail.
    ///
    /// When the object's sell state differs from the slot's stored copy, it shows the sell marker
    /// exactly when the state is non-zero and stores `(state != 0)`.
    ///
    /// Note the asymmetry the client actually has: the comparison is against the raw `int`, the
    /// stored copy is the **boolean**.
    ///
    /// The sell-state write's one caller in the retail client is
    /// the vendor panel's own sell-state write, and this build's counterpart is
    /// `add_item_to_sell`, following the vendor panel's item insert and its "the target list is
    /// element `0x100000CE`, so mark it for sale" step, reached by a drop on the sell list.
    /// The marker is the little crown-type image in the top corner, and it is drawn in the pack **and** in the
    /// sell window because the sell-state write ends in an item-attributes-changed broadcast,
    /// which every tile showing that object answers.
    pub fn set_sell_state(&mut self, ui: &mut UiSystem, v: bool) -> bool {
        if self.sell_state == v {
            return false;
        }
        if let Some(h) = self.sell_state_elem {
            ui.set_visible(h, v);
        }
        self.sell_state = v;
        true
    }

    /// The trade-state arm of the same tail, identical in shape.
    ///
    /// Its writers are the trade, salvage, and housing panels through their trade-state updates.
    /// Those windows now exist; the remaining seam is delivery of the matching state change to
    /// every tile that displays the object.
    pub fn set_trade_state(&mut self, ui: &mut UiSystem, v: bool) -> bool {
        if self.trade_state == v {
            return false;
        }
        if let Some(h) = self.trade_state_elem {
            ui.set_visible(h, v);
        }
        self.trade_state = v;
        true
    }
}

/// `a / b` as the client computes it — it converts
/// both `int`s to `double`, divides, and only then narrows to `float` for the attribute.
fn meter_ratio(a: i32, b: i32) -> f32 {
    if b == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_possible_truncation)] // the client's own `(float)` narrowing
    {
        (f64::from(a) / f64::from(b)) as f32
    }
}

/// The same for two unsigned 32-bit operands, which the client converts as unsigned (a value
/// that converts negative is corrected back with `+ 4294967296.0`).
fn meter_ratio_u32(a: u32, b: u32) -> f32 {
    if b == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_possible_truncation)] // the client's own `(float)` narrowing
    {
        (f64::from(a) / f64::from(b)) as f32
    }
}

/// Everything one slot's decoration pass reads off its object — [`ItemListWidget::decorate`]'s
/// input, and the whole of what the slot reads from its object.
///
/// It is owned rather than borrowed because the callback is a closure over a `&dyn GameView` and
/// the two names come out of it by reference; a slot is decorated at most once per changed frame,
/// so the two `String`s are cheaper than the lifetime.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SlotInfo {
    /// The object description's numbers.
    pub decoration: crate::view::SlotDecoration,
    /// The object's name.
    pub name: String,
    /// The object's plural name, when it carries a non-empty one.
    pub plural_name: Option<String>,
    /// The client's out-parameter for this object's
    /// cooldown id `+ 0x8000`, or `None` when the call returned 0.
    ///
    /// It is a *separate* field from [`Self::decoration`] because it is not a fact about the
    /// object at all: it is a fact about the **player's own enchantment registry**, read at draw
    /// time, and it changes every frame while a cooldown runs. The cooldown id and
    /// duration are the object's and travel in the decoration.
    pub cooldown_remaining: Option<f64>,
}

/// The client's empty-buffer convention, as **one expression for the whole crate**.
///
/// The client answers `NAME_PLURAL` from the plural name and falls
/// back to the name when that buffer holds only its terminator, so an empty plural means *use the
/// singular* — never *the plural is the empty string*.
///
/// **Three call sites in this crate have to agree, and two of them are the two halves of one
/// early-out.** [`TileInfo::read`] fills a guard's memory, [`TileInfo::matches`] compares it, and
/// [`ItemSlot::update_tooltip`] draws the result. A divergence between the first two would make
/// the gate either flap open on every frame — flushing four lists, the drag ghost and the
/// selection ring, at a cost that is invisible on screen — or close permanently and freeze every
/// tooltip in the pack; and the third would **hide** the flap, because it re-normalises on the way
/// to the screen, so the picture would stay right while the work ran every frame. Both failures
/// are plausible, neither reddens anything, and the third makes the first two unobservable. So the
/// possibility is **removed** rather than tested: there is one expression, and all three call it.
const fn non_empty(s: Option<&str>) -> Option<&str> {
    match s {
        Some(p) if !p.is_empty() => Some(p),
        _ => None,
    }
}

/// [`non_empty`] applied at the seam — the plural name for one object.
fn plural_name_of(view: &dyn crate::view::GameView, id: ObjectId) -> Option<&str> {
    non_empty(view.plural_name(id))
}

/// Everything [`ItemListWidget::decorate`] reads for one slot **except the clock** — the frozen
/// world half of a [`SlotInfo`].
///
/// `decorate`'s input per slot is a [`SlotInfo`], and a `SlotInfo` is **four** things, not one:
/// the whole [`crate::view::SlotDecoration`], the object's name, its plural name, and
/// `cooldown_remaining`. The first three are facts about a frozen world and belong in an
/// early-out's compared value; the fourth is `(duration + start time) - current time`, a
/// function of the **clock**, which takes a new value every frame of its own accord. Folding it
/// into a guard would make that guard fire **never**, and it would look correct on screen — so it
/// is not a field here, and the countdown is driven by the heartbeat exemption instead
/// (`GamePlayScreen::do_item_heartbeat`, which `Hud::drive` calls outside every guarded pass).
///
/// **It is one type rather than two copies deliberately.** `InventoryPanels` and `ShortcutBar`
/// both need it, and separate structs, readers and matchers would be six places for three
/// fields with the plural-name mapping written out four times. A second copy that omits a field
/// **looks exactly as fixed**. With one copy the two panels cannot drift apart.
///
/// `Eq` is not on this derive because [`crate::view::SlotDecoration`] carries
/// `cooldown_duration`, an `f64`.
#[derive(Debug, Clone, PartialEq)]
pub struct TileInfo {
    /// Every field of it — stack size, structure, icon overlay, icon underlay, effects and the
    /// rest — because `decorate` reads every field of it.
    pub decoration: crate::view::SlotDecoration,
    /// The object's name, which the tooltip update draws.
    pub name: String,
    /// The object's plural name, mapped by [`plural_name_of`] — the name a stack above one shows.
    pub plural_name: Option<String>,
}

impl TileInfo {
    /// Read one slot's frozen-world inputs off the seam.
    ///
    /// `None` is an id with no live client object yet — the pre-placement window that `0x0022
    /// Item_ServerSaysContainID` opens. The item-attributes-changed edge exists to catch the
    /// transition once that object becomes live.
    ///
    /// This is the single read both a guard's memory and its fill are built from, so the value
    /// that decided to rebuild and the value that reaches the screen cannot disagree within a
    /// frame (the same rule the capacity pass follows, applied to the decoration itself).
    #[must_use]
    pub fn read(view: &dyn crate::view::GameView, id: ObjectId) -> Option<Self> {
        Some(Self {
            decoration: view.slot_decoration(id)?,
            name: view.name(id).unwrap_or_default().to_string(),
            plural_name: plural_name_of(view, id).map(str::to_string),
        })
    }

    /// Whether a remembered tile is still exactly what the view answers for `id`.
    ///
    /// Asked field by field against the view rather than by building a fresh [`TileInfo`], so a
    /// frame on which nothing moved allocates no `String` at all: the clone happens only on a
    /// frame that rebuilds. That is a second reason for preferring a whole-value comparison
    /// to a wider tuple.
    ///
    /// `last` is `Option` because the absence of a tile is itself a compared state: a slot that
    /// held nothing and still holds nothing has not moved.
    #[must_use]
    pub fn matches(view: &dyn crate::view::GameView, id: ObjectId, last: Option<&Self>) -> bool {
        let Some(d) = view.slot_decoration(id) else {
            return last.is_none();
        };
        let Some(t) = last else { return false };
        t.decoration == d
            && t.name == view.name(id).unwrap_or_default()
            && t.plural_name.as_deref() == plural_name_of(view, id)
    }

    /// The [`SlotInfo`] `decorate` wants, with the one clock-driven field supplied fresh.
    ///
    /// The three frozen-world fields come **out of the compared value**, not off the view a
    /// second time; only `cooldown_remaining` is read again, because it has to be.
    /// The fresh read is not the countdown mechanism — that is the heartbeat — it is the value
    /// the slot update's own tail would have used on a frame that really did rebuild, so a slot
    /// refilled mid-cooldown draws the right wedge at once rather than at the next beat.
    #[must_use]
    pub fn to_slot_info(&self, view: &dyn crate::view::GameView, now: f64) -> SlotInfo {
        SlotInfo {
            cooldown_remaining: (self.decoration.cooldown_id > 0)
                .then(|| view.cooldown_remaining(self.decoration.cooldown_id, now))
                .flatten(),
            decoration: self.decoration,
            name: self.name.clone(),
            plural_name: self.plural_name.clone(),
        }
    }
}

/// One live `ItemListWidget` — type `0x10000031`.
#[derive(Debug, Clone)]
pub struct ItemListWidget {
    pub element: ElementId,
    pub handle: ElemHandle,
    /// [`attr::ITEM_SLOT_ID`].
    pub slot_id: ElementId,
    /// Whether this list shows side packs ([`attr::IS_CONTAINER`]).
    pub container_list: bool,
    /// Whether this is a shortcut list ([`attr::IS_SHORTCUT`]).
    pub shortcut_list: bool,
    /// Whether this is a vendor list — [`attr::IS_VENDOR`]. Read by the drag-icon preparation and by the
    /// three-way `!vendor && !salvage && !shortcut` test the ghost is gated on.
    pub vendor_list: bool,
    /// Whether this is a salvage list — [`attr::IS_SALVAGE`]. See [`Self::vendor_list`].
    pub salvage_list: bool,
    /// `UI_ItemList_AllowDragging` — [`attr::ALLOW_DRAGGING`]. The whole body of
    /// [`Self::begin_drag`] runs only when attribute `0x10000016` is true, so a list
    /// without it can be dropped **on** and never dragged **from**.
    pub allow_dragging: bool,
    /// Bit 0 of the list box's flags — [`attr::HORIZONTAL`].
    pub horizontal: bool,
    /// [`attr::AT_LEAST_ONE_EMPTY`].
    pub at_least_one_empty: bool,
    /// The single-selection flag — [`attr::SINGLE_SELECTION`]. True on one shipped list
    /// (the vendor's); see the attribute's own note for the measurement.
    pub single_selection: bool,
    /// Which of *this* list's items is the open container.
    ///
    /// **Every `ItemListWidget` carries its own**, and the client never shares one: a click on
    /// the side-pack strip writes the strip's, a click on the main-pack slot writes
    /// the top-container list's, and [`Self::update_open_container_indicator`] tests the one
    /// belonging to the list it is called on. It is not `InventoryPanels::open_container`, which
    /// is the *grid's* parent container and a different quantity.
    pub open_item_id: Option<ObjectId>,
    /// The column count's source. `-1` means "one row of everything".
    ///
    /// The `-1` default below is **not** the client layout pass's: the client initialises its local to 0
    /// and an absent `0x5F` therefore reads 0, which takes the `max(v, 1)` arm and means *one
    /// column*. It makes no difference here — every one of the forty-odd `ItemListWidget`s in
    /// the shipped gameplay tree carries `0x5F` explicitly, so the default is unreachable
    /// [verified against the live element tree] — but a plain `ListBox` often
    /// carries none, which is why [`crate::panels::listbox::ListBoxWidget`] defaults to 0.
    pub max_columns: i32,
    /// The live fixed list size; the container-list-size update rewrites it.
    pub fixed_list_size: i32,
    /// The cell width and height, measured off the first slot the list created.
    pub cell: (i32, i32),
    /// The list's items, in list order.
    pub slots: Vec<ItemSlot>,
    /// The slot cache: retiring a slot pushes at the tail and
    /// the client reuses from the head. Cached slots are not scroll rows.
    pub cached_slots: std::collections::VecDeque<ItemSlot>,
    /// The container whose contents this list shows.
    pub parent_container: Option<ObjectId>,
    /// How many slot creations this list has made. Counted because a list that
    /// silently creates nothing leaves an empty panel with no other symptom.
    pub created: u32,
    /// Slot creations that produced no element — a layout miss.
    pub create_failures: u32,
    /// Slots whose drag icon could not be created. Non-zero means nothing in this list can be
    /// picked up, so it is a number.
    pub drag_icon_failures: u32,
}

impl ItemListWidget {
    /// The item list's initialisation: read the attributes, then create the initial
    /// slots — one when `FixedListSize < 0` (just to measure the cell size), or exactly
    /// `FixedListSize` of them — and flush.
    pub fn init(ui: &mut UiSystem, handle: ElemHandle) -> Self {
        let element = ui
            .node(handle)
            .map_or(ElementId(0), dereth_ui::ElementNode::element_id);
        let mut w = Self {
            element,
            handle,
            slot_id: ElementId(attr_enum(ui, handle, attr::ITEM_SLOT_ID).unwrap_or(0)),
            container_list: attr_bool(ui, handle, attr::IS_CONTAINER).unwrap_or(false),
            shortcut_list: attr_bool(ui, handle, attr::IS_SHORTCUT).unwrap_or(false),
            vendor_list: attr_bool(ui, handle, attr::IS_VENDOR).unwrap_or(false),
            salvage_list: attr_bool(ui, handle, attr::IS_SALVAGE).unwrap_or(false),
            allow_dragging: attr_bool(ui, handle, attr::ALLOW_DRAGGING).unwrap_or(false),
            horizontal: attr_bool(ui, handle, attr::HORIZONTAL).unwrap_or(false),
            at_least_one_empty: attr_bool(ui, handle, attr::AT_LEAST_ONE_EMPTY).unwrap_or(false),
            // Attribute `0x10000052` — the fifth of the client's nine boolean reads.
            single_selection: attr_bool(ui, handle, attr::SINGLE_SELECTION).unwrap_or(false),
            open_item_id: None,
            max_columns: attr_int(ui, handle, attr::MAX_COLUMNS).unwrap_or(-1),
            fixed_list_size: attr_int(ui, handle, attr::FIXED_LIST_SIZE).unwrap_or(-1),
            cell: (0, 0),
            slots: Vec::new(),
            cached_slots: std::collections::VecDeque::new(),
            parent_container: None,
            created: 0,
            create_failures: 0,
            drag_icon_failures: 0,
        };
        let n = if w.fixed_list_size < 0 {
            1
        } else {
            w.fixed_list_size
        };
        for _ in 0..n {
            w.add_slot(ui);
        }
        w.flush(ui);
        w.update_layout(ui);
        w
    }

    /// Create one slot, plus the add-item that every caller pairs it with.
    ///
    /// The client keeps a free list ([`Self::cached_slots`]) and reuses a slot element before it
    /// creates one. Retired slots are hidden and retained in the same FIFO order, so repeated
    /// refills reuse their existing trees and do not allocate another cache on every filter.
    fn add_slot(&mut self, ui: &mut UiSystem) {
        if let Some(mut s) = self.cached_slots.pop_front() {
            // The client's cache hit: empty slot and no drag-accept overlay.
            s.clear(ui);
            s.set_drag_accept_state(ui, drag_accept_state::NONE);
            ui.set_visible(s.handle, true);
            self.slots.push(s);
            return;
        }
        let r = ui.require_env().and_then(|e| {
            e.create_child_element_by_enum(ui, self.handle, ITEM_SLOT_LAYOUT, self.slot_id)
        });
        match r {
            Ok(h) => {
                self.created += 1;
                let mut s = ItemSlot::bind(ui, h);
                // The drag icon is created from the
                // **same** `ItemSlot` layout under element id `0x10000345`. It carries
                // `0x3A = true` and is 32×32 [verified against the live element tree],
                // which is what makes starting a drag on it at offset (16, 16) find a draggable
                // element at once and centre it on the pointer.
                //
                // **The slot is its parent**, not a second root of that layout. The client passes element id
                // `0x10000345`, the layout, and the item slot as the parent to child creation. The
                // new drag icon therefore reports the slot as its parent, exactly as it does here,
                // and walking from the icon to ancestor type `0x10000031`
                // from the icon finds the owning list. The next two steps hide the icon and make
                // the slot mouse-visible, which is what [`ItemSlot::post_init`] does.
                let drag_icon = ui
                    .require_env()
                    .and_then(|e| {
                        e.create_child_element_by_enum(
                            ui,
                            h,
                            ITEM_SLOT_LAYOUT,
                            ElementId(child::DRAG_ICON),
                        )
                    })
                    .ok();
                if drag_icon.is_none() {
                    self.drag_icon_failures += 1;
                }
                s.post_init(ui, drag_icon);
                s.rest(ui);
                let b = ui.node(h).map(|n| n.region.box_);
                if let Some(b) = b {
                    if b.width() > 0 && b.height() > 0 {
                        self.cell = (b.width(), b.height());
                    }
                }
                s.clear(ui);
                self.slots.push(s);
            }
            Err(_) => self.create_failures += 1,
        }
    }

    /// Every slot back to empty.
    pub fn flush(&mut self, ui: &mut UiSystem) {
        for s in &mut self.slots {
            s.clear(ui);
        }
    }

    /// Grow to `FixedListSize`, or shrink to
    /// it. Runs only while `FixedListSize != -1`.
    pub fn update_fixed_slots(&mut self, ui: &mut UiSystem) {
        if self.fixed_list_size < 0 {
            return;
        }
        let want = usize::try_from(self.fixed_list_size).unwrap_or(0);
        while self.slots.len() < want {
            let before = self.slots.len();
            self.add_slot(ui);
            if self.slots.len() == before {
                break; // the layout refused; do not spin
            }
        }
        while self.slots.len() > want {
            if let Some(mut s) = self.slots.pop() {
                s.clear(ui);
                ui.set_visible(s.handle, false);
                self.cached_slots.push_back(s);
            }
        }
    }

    /// The `FixedListSize == -1` arm: fill the
    /// list's own width with cells, and honour `AtLeastOneEmptySlot`.
    ///
    /// The `max_columns == -1` arm pads in width and removes trailing empty cells beyond
    /// the viewport. Otherwise `max_rows == -1` removes every trailing empty slot.
    pub fn update_empty_slots(&mut self, ui: &mut UiSystem) {
        if self.fixed_list_size != -1 {
            return;
        }
        if !ui.node(self.handle).is_some_and(|n| n.region.flags.visible) {
            return;
        }
        let (w, cell_w) = {
            let b = ui
                .node(self.handle)
                .map(|n| n.region.box_)
                .unwrap_or_default();
            (b.width(), self.cell.0)
        };
        if cell_w <= 0 {
            return;
        }
        if self.max_columns == -1 {
            let used = i32::try_from(self.slots.len()).unwrap_or(0) * cell_w;
            if used < w {
                for _ in 0..((w - used) / cell_w) {
                    self.add_slot(ui);
                }
            } else if used > w {
                // The empty-slot update: ceil((used - width) / cell width), stopping
                // at the first occupied slot. Cached elements remain hidden, outside the list.
                for _ in 0..((used - w + cell_w - 1) / cell_w) {
                    if !self.remove_trailing_empty(ui) {
                        break;
                    }
                }
            }
            if self.at_least_one_empty && self.slots.last().is_some_and(|s| s.item.is_some()) {
                self.add_slot(ui);
            }
        } else if attr_int(ui, self.handle, 0x60) == Some(-1) {
            while self.remove_trailing_empty(ui) {}
        }
    }

    fn remove_trailing_empty(&mut self, ui: &mut UiSystem) -> bool {
        if !self
            .slots
            .last()
            .is_some_and(|s| s.item.is_none() && s.spell.is_none())
        {
            return false;
        }
        if let Some(s) = self.slots.pop() {
            ui.set_visible(s.handle, false);
            self.cached_slots.push_back(s);
        }
        true
    }

    /// The list box's layout update, for a list of equal cells.
    ///
    /// The column count is `clamp(attribute 0x5F, 1, items)`, or `items` when the
    /// attribute is negative — in which case the row count is `items != 0`, i.e. one row. The
    /// row count is otherwise `ceil(items / cols)`. The fill order then depends on bit 0 of
    /// the flags: **horizontal fills row-major across the columns, and the default fills
    /// column-major down the rows.**
    pub fn update_layout(&mut self, ui: &mut UiSystem) {
        // The item list element's internal create item / the item insert:
        // the active list and hidden cache are separate arrays. Publish only active slots to
        // the inherited behavior; cached children must not extend the scrollable paper.
        if let Some(l) = ui.node_mut(self.handle).and_then(|n| {
            n.behaviour
                .as_mut()?
                .as_any_mut()?
                .downcast_mut::<dereth_ui::widgets::listbox::ListBox>()
        }) {
            let selected = l.selected.and_then(|i| l.items.get(i).copied());
            l.items = self.slots.iter().map(|s| s.handle).collect();
            l.items_are_authoritative = true;
            l.selected = selected.and_then(|h| l.items.iter().position(|i| *i == h));
        }
        let n = i32::try_from(self.slots.len()).unwrap_or(0);
        let (cols, rows) = if self.max_columns < 0 {
            (n, i32::from(n != 0))
        } else {
            let cols = self.max_columns.max(1).min(n);
            let rows = if cols == 0 { 0 } else { (n + cols - 1) / cols };
            (cols, rows)
        };
        let (cw, ch) = self.cell;
        let (mut x, mut y) = (0, 0);
        let (mut col, mut row) = (0, 0);
        for s in &self.slots {
            ui.move_to(s.handle, x, y);
            if self.horizontal {
                if col == cols - 1 {
                    x = 0;
                    col = 0;
                    y += ch;
                    row += 1;
                } else {
                    x += cw;
                    col += 1;
                }
            } else if row == rows - 1 {
                y = 0;
                row = 0;
                x += cw;
                col += 1;
            } else {
                y += ch;
                row += 1;
            }
        }
        let _ = col;
        // The client's layout pass ends by resizing the scrollable area and places rows at
        // their grid origin minus the scroll offset, in the same pass.
        dereth_ui::widgets::listbox::refresh_scroll_of(ui, self.handle);
    }

    /// The inherited Scrollable offsets, shared with scrollbar and wheel delivery.
    #[must_use]
    pub fn scroll(&self, ui: &UiSystem) -> (i32, i32) {
        dereth_ui::widgets::listbox::scroll_offset_of(ui, self.handle).unwrap_or((0, 0))
    }

    /// Scroll-to-show aligns the row's origin, then clamps it. This is not
    /// [`Self::scroll_to_view`]'s minimal movement.
    pub fn scroll_to_show(&mut self, ui: &mut UiSystem, index: usize) -> bool {
        if index >= self.slots.len() {
            return false;
        }
        self.update_layout(ui);
        let n = i32::try_from(self.slots.len()).unwrap_or(0);
        let (cols, rows) = self.grid(n);
        let i = i32::try_from(index).unwrap_or(0);
        let (col, row) = if self.horizontal {
            (i % cols, i / cols)
        } else {
            (i / rows, i % rows)
        };
        dereth_ui::widgets::listbox::set_scroll_offset(
            ui,
            self.handle,
            col * self.cell.0,
            row * self.cell.1,
        )
    }

    /// The list box's scroll-to-view, which retail calls from the spellbook and
    /// spell bar's selection handlers. Unlike [`Self::scroll_to_show`], a fully visible item does
    /// not move.
    pub fn scroll_to_view(&mut self, ui: &mut UiSystem, index: usize) -> bool {
        let Some(item) = self.slots.get(index).map(|s| s.handle) else {
            return false;
        };
        self.update_layout(ui);
        dereth_ui::widgets::listbox::scroll_item_to_view(ui, self.handle, item)
    }

    /// The item list set parent container + the item list update container list size +
    /// the loop, folded into one pass over the ids the server says
    /// this container holds.
    ///
    /// `capacity` is the container's items capacity or containers capacity depending on
    /// [`Self::container_list`], which is exactly what the client writes into
    /// `UI_ItemList_FixedListSize` before calling [`Self::update_fixed_slots`]. A negative
    /// capacity means "unbounded", which is the [`Self::update_empty_slots`] arm.
    ///
    /// Returns how many slots ended up holding an object.
    pub fn set_contents(
        &mut self,
        ui: &mut UiSystem,
        container: Option<ObjectId>,
        capacity: Option<i32>,
        ids: &[ObjectId],
        icon: &dyn Fn(ObjectId) -> Option<DataId>,
    ) -> usize {
        self.parent_container = container;
        if let Some(c) = capacity {
            self.fixed_list_size = c;
        }
        if self.fixed_list_size < 0 {
            // Grow enough to hold everything before the width-driven padding runs.
            while self.slots.len() < ids.len() {
                let before = self.slots.len();
                self.add_slot(ui);
                if self.slots.len() == before {
                    break;
                }
            }
        } else {
            self.update_fixed_slots(ui);
        }
        self.flush(ui);
        let mut filled = 0;
        for (i, id) in ids.iter().enumerate() {
            // The client's single-selection arm: with single selection on, a slot is selectable
            // only when the list does not already hold its id, evaluated **before** the slot is
            // initialised and therefore against the list as it stood a moment ago.
            // A second slot for an id this list already holds arrives *unselectable*, which is
            // what stops a vendor's duplicate rows taking two rings. The fill here
            // is a batch, so "as it stood a moment ago" is the ids already placed in this loop.
            let selectable = !self.single_selection || !ids[..i].contains(id);
            let Some(s) = self.slots.get_mut(i) else {
                break;
            };
            s.item = Some(*id);
            s.spell = None;
            super::runtime::set_identity(ui, s.handle, *id, 0);
            s.set_selectable_state(selectable);
            // The item list's add-item order: initialise the slot, set state `0x1000001D`,
            // then the slot update sets the icon. **The state goes first**, because
            // `0x1000001C`'s media step would otherwise overwrite the icon with the empty frame.
            s.set_state(ui, item_state::OCCUPIED);
            s.set_icon(ui, icon(*id));
            ui.set_visible(s.handle, true);
            filled += 1;
        }
        self.update_empty_slots(ui);
        self.update_layout(ui);
        filled
    }

    /// The tail of the slot update — everything after the icon, run
    /// once over every slot that holds an object.
    ///
    /// The slot update's order, and it is the order here:
    ///
    /// ```text
    /// is-container = (bitfield & 0x800000) || items capacity || containers capacity
    /// capacity display
    /// structure display
    /// quantity display
    /// cooldown display
    /// … waiting, shortcut, sell and trade state …
    /// tooltip
    /// ```
    ///
    /// The order below is the slot update's own, step for
    /// step, and it is the order because two of the arms are edge-triggered: the ring and
    /// the two markers act only when their cached copy differs from the weenie's, so running them
    /// out of order relative to the state changes above them draws the wrong thing once and then
    /// never corrects it.
    ///
    /// Three of the slot update's writes are deliberately **not** wired to anything that draws,
    /// because they are not wired to anything in the client either: `effects` is copied
    /// and read by nothing here, and the is-openable and holds-containers flags are
    /// assigned in the constructor and in this function and **read by nothing else in the
    /// client**. They are computed anyway, because a later window may want them; the
    /// open-container *frame* is a different mechanism entirely
    /// ([`Self::update_open_container_indicator`]).
    ///
    /// A slot the callback answers `None` for is left exactly as it was, which is the client's
    /// no-object early-out in all four display updates. Returns how many slots
    /// were decorated.
    pub fn decorate(
        &mut self,
        ui: &mut UiSystem,
        info: &dyn Fn(ObjectId) -> Option<SlotInfo>,
    ) -> usize {
        let mut n = 0;
        for s in &mut self.slots {
            let Some(id) = s.item else { continue };
            let Some(i) = info(id) else { continue };
            let d = i.decoration;
            // All six
            // blits, as the one composite the object's icon is. In the client this is inside
            // the set-icon's single image write, so it runs at the head of the slot update
            // rather than in this tail; it is here because [`Self::set_contents`]'s fill loop is
            // handed an icon `DataId` and has none of the other four fields to resolve a recipe
            // from. The observable result is the same for the reason given for the rest of
            // this pass: the fill is a batch and this is a second pass over the very same slots,
            // before the frame is drawn.
            s.set_icon_composite(ui, Some(object_recipe(ui, &d)));
            // When the slot's copy differs from the object's (and it is a clear, or the slot is
            // selectable) — the edge, then [`ItemSlot::set_selected_state`]'s own body.
            if s.selected != d.selected {
                s.set_selected_state(ui, d.selected);
            }
            // openable = (is container && (bitfield & BF_OPENABLE)) || item is the player.
            // The player's own slot is openable regardless of its container flags.
            s.is_openable = (d.is_container && d.openable) || d.is_player;
            s.is_container = d.is_container;
            s.is_container_holder = d.containers_capacity != 0;
            // Mirror the pair the capacity display is about to consume, so
            // that [`Self::drag_over`]'s empty-slot count can be answered from a message
            // handler. Written *before* the call for no reason other than reading order; the
            // call does not change them.
            s.items_capacity = d.items_capacity;
            s.contained_items = d.contained_items;
            s.update_capacity_display(ui, d.items_capacity, d.contained_items);
            s.update_structure_display(ui, d.structure, d.max_structure);
            s.update_quantity_display(ui);
            s.cooldown_id = d.cooldown_id;
            s.cooldown_duration = d.cooldown_duration;
            s.update_cooldown_display(ui, i.cooldown_remaining);
            // **The busy / in-use overlay, and it is the slot update's own next
            // block, in its own place.** When the object's waiting state differs from the slot's,
            // the slot copies it and runs its waiting-state setter, which writes it onto the live
            // object and (unless the slot is unghostable) shows or hides the ghost.
            //
            // Edge-triggered, like the ring and the two markers around it, and for the same
            // reason: the client's waiting-state write sets the object as well as the element, and
            // this crate must not write the object — so the compare is what keeps the
            // call from firing on a frame where nothing moved.
            //
            // **This is the one mirror, and it must stay the only one.** A second mechanism that
            // walks only some lists (say the item list and the doll) leaves the container list and
            // the top-container list out, so a ghosted **backpack** has no route back: the object's
            // flag goes down (the server's attempt-failed or move-item notice, or the tile's own
            // `0x15` arm) and the element stays grey for the session. This runs for every slot of
            // every list this function is called on.
            if s.waiting != d.waiting {
                s.set_waiting(ui, d.waiting);
            }
            // The slot update's own next two blocks, in its own order: a delayed
            // number other than `-1` is written onto the object (not ghosted) and reset to `-1`;
            // then, when the slot's number or ghosted flag differs from the object's, the slot's
            // numeral is rewritten from the object.
            //
            // The two collapse into one call here, and they may: nothing writes the object's
            // shortcut number between them, and the first write sets the object's ghosted flag
            // to `false` from its literal argument — so the pair is exactly a slot numeral write
            // of `(n, false)`. **The write into the object table is the half
            // this crate never makes**, and it is not needed: this build keeps the numeral on
            // the widget (`ShortcutBar::update`) rather than round-tripping it through
            // live object, so the delayed number lands where the direct one does.
            //
            // Reaching this line *is* the mechanism: the slot update returns early when the
            // slot has no object, so a shortcut naming an object the client has not seen sets the
            // delayed number and paints nothing until the object arrives — which is the first
            // pass where `info(id)` answers `Some`, i.e. this one.
            if s.delayed_shortcut_num != -1 {
                let n = s.delayed_shortcut_num;
                s.delayed_shortcut_num = -1;
                s.set_shortcut_num(ui, n, false);
            }
            // **This block follows the delayed update and makes the quickbar numeral
            // appear on the paper doll and in the
            // packs as well as in the bar slot.** When the slot's stored number or ghosted flag
            // differs from the object's, the object's number and flag are written to the slot.
            //
            // **The assignment lives on the object, not on the bar.** The shortcut-bar insert
            // writes `(slot, not ghosted)` onto the object, its removal writes `-1`, and the
            // toolbar flips the ghosted flag through the bar's own
            // tiles into the object. Thus every tile anywhere in
            // the tree learns the assignment from the same place, and this compare is how.
            // Without it the numeral is painted only by
            // [`crate::toolbar::shortcuts::ShortcutBar::update`] on the bar's own tile, and equipped
            // or backpacked copies of the item show no number.
            //
            // The delayed block above writes the **object**, while this implementation
            // is therefore read back by this compare on the same pass; here it writes the widget
            // directly, and this compare answers the same number for it, because the host
            // resolves both from the player module's one shortcut array.
            //
            // [`crate::view::SlotDecoration::shortcut_num`] is `Option<u32>` where the client's
            // field is an `i32` whose "none" is `-1`; the mapping is here, at the one call that
            // needs the client's spelling.
            let want_num = d
                .shortcut_num
                .map_or(-1, |n| i32::try_from(n).unwrap_or(-1));
            if s.shortcut_num != want_num || s.shortcut_ghosted != d.shortcut_ghosted {
                s.set_shortcut_num(ui, want_num, d.shortcut_ghosted);
            }
            // The two markers are the tail.
            s.set_sell_state(ui, d.sell_state);
            s.set_trade_state(ui, d.trade_state);
            s.update_tooltip(ui, &i.name, i.plural_name.as_deref(), d.stack_size);
            n += 1;
        }
        n
    }

    /// **The frame on the open
    /// pack.**
    ///
    /// The client walks every positional entry, keeps only item slots, and shows the open-container
    /// frame exactly on the slot whose item id equals the nonzero open id.
    ///
    /// It is called from five places, all of which mean that the open container changed or was
    /// forgotten. Setting the parent list and flushing the list pass **0**. Opening a named
    /// container and opening the first container pass the new open item id; the click path passes
    /// the container selected by that click.
    ///
    /// **Only the two `UI_ItemList_IsContainer` lists can show anything**, because only their slot
    /// root `0x1000033F` has the child — see [`ItemSlot::open_container`]. Running it over the
    /// grid is harmless and is what the client does.
    ///
    /// **The drop hint**, and the whole of its state machine.
    ///
    /// Reached from the slot's element-message `0x3E` arm: the slot under the pointer walks up to
    /// its `ItemListWidget` (type `0x10000031`) and asks *it* what hint to show, so the decision
    /// belongs to the list and the drawing to the slot.
    ///
    /// In the client's own test order: read the drag proxy's item, spell and flags; a registered
    /// drag handler that answers true ends it; a vendor, salvage or shortcut list ends it; no item
    /// ends it; a vendor/shortcut/salvage drag (`flags & 0xE`) ends it. Then a dragged **pack**
    /// gets `0x10000040` on a container list and `0x10000041` elsewhere; a plain item gets
    /// `0x10000040` on a non-container list; and on a container list it gets `0x10000046` when
    /// the slot under it is filled and that object has a non-zero empty-slot count, else
    /// `0x10000041`.
    ///
    /// Three things worth stating because each is invisible on screen if it is wrong:
    ///
    /// * **The legal/illegal pair is decided here and nowhere else.** A pack dragged onto the item
    ///   grid and a plain item dragged onto an empty side-pack slot both get `REFUSE`, and both
    ///   are drops the drag-accept test really does refuse ("Cannot place container in
    ///   item list" / "Cannot place item in container list"). Showing `ACCEPT` over either would
    ///   be one small sprite's difference and would tell the player the drop will work.
    /// * **`INTO_CONTAINER` is a third answer, not a second `ACCEPT`.** It is reached only over a
    ///   *filled* slot of a container list whose object still has room, which is exactly the drop
    ///   the drop acceptance re-targets from the list's own container to that pack.
    /// * **The empty-slot count returns `-1` for an unbounded pack** (items capacity
    ///   `-1`), and `-1 != 0`, so unbounded reads as *has room*. `free_slots` keeps
    ///   the same signed convention.
    ///
    /// **The drag handler is the caller's, not this function's.** The first arm is answered before
    /// this is reached, by whichever owner registered a handler on the list: the vendor's sell
    /// list (the vendor sell page), the eighteen shortcut lists
    /// (the toolbar's item-list drag-over) and the twenty-four doll lists
    /// (the paper doll's own). Each of
    /// those handlers returns true on every path, so this default never runs for their lists —
    /// which is why the `shortcut_list` early-out below is unreachable in retail rather than
    /// wrong.
    ///
    /// `slot` is the index of the slot the pointer is over. Returns the state that was chosen, or
    /// `None` when one of the early-outs fired — which is *not* the same as choosing
    /// [`drag_accept_state::NONE`], and the caller must be able to tell them apart.
    pub fn drag_over(
        &mut self,
        ui: &mut UiSystem,
        slot: usize,
        info: DropIconInfo,
        free_slots: &dyn Fn(ObjectId) -> Option<i64>,
    ) -> Option<StateId> {
        if self.vendor_list || self.salvage_list || self.shortcut_list {
            return None;
        }
        info.item?;
        if !info.is_inventory_move() {
            return None;
        }
        let dragging_a_container = info.flags & drag_flags::IS_CONTAINER != 0;
        let s = if dragging_a_container {
            if self.container_list {
                drag_accept_state::ACCEPT
            } else {
                drag_accept_state::REFUSE
            }
        } else if !self.container_list {
            drag_accept_state::ACCEPT
        } else {
            // The container strip, with a plain item over it. A slot state other than `0x1000001C`
            // is "this slot holds something"; the mirror of that here is `item.is_some()`.
            let under = self.slots.get(slot).and_then(|s| s.item);
            match under.and_then(free_slots) {
                Some(n) if n != 0 => drag_accept_state::INTO_CONTAINER,
                _ => drag_accept_state::REFUSE,
            }
        };
        self.slots.get_mut(slot)?.set_drag_accept_state(ui, s);
        Some(s)
    }

    /// Returns how many slots' frames actually changed.
    pub fn update_open_container_indicator(
        &mut self,
        ui: &mut UiSystem,
        open: Option<ObjectId>,
    ) -> usize {
        let mut n = 0;
        for s in &mut self.slots {
            let on = open.is_some() && s.item == open;
            if s.set_open_container_state(ui, on) {
                n += 1;
            }
        }
        n
    }

    /// How many slots are **not** in state
    /// `0x1000001C`.
    ///
    /// It is not the length of the list's items, and the difference is load-bearing:
    /// [`Self::handle_single_selection`] uses this as a **loop bound over positional indices**, so
    /// a list whose filled slots do not sit at the front of the array is walked short. That is the
    /// client's own arithmetic and it is reproduced rather than tidied — the item-list insert
    /// fills from the first free slot, so in practice the filled ones are contiguous
    /// and at the front.
    ///
    /// **Its second reader is the one that decides where a drop lands.**
    /// The drop clamps the slot index under the pointer to this count, which is what makes a drop on the *fourth* empty slot
    /// of a five-item pack land at the end of the list rather than at position 8.
    #[must_use]
    pub fn num_ui_items(&self) -> usize {
        self.slots
            .iter()
            .filter(|s| s.item.is_some() || s.spell.is_some())
            .count()
    }

    /// Does **this** list hold `id`?
    ///
    /// One list, not the screen: the whole single-selection rule is about the same object id
    /// occupying two slots of the *same* `ItemListWidget` (a vendor listing the same wcid
    /// twice), never about two different lists.
    #[must_use]
    pub fn is_in_list(&self, id: ObjectId) -> bool {
        self.slots.iter().any(|s| s.item == Some(id))
    }

    /// **The click's deselect walk.**
    ///
    /// The client walks positional entries in order. For every other item slot with the clicked
    /// item id, it clears selectable state and then selected state. It then enables selectable
    /// state on the clicked slot and marks that slot selected.
    ///
    /// Two details the names do not give away, both observed in the retail client:
    ///
    /// * **the order is selectable-then-selected in both halves, and it is load-bearing in the
    ///   *set* half only.** The client's gate lets a clear through always and a set only when the
    ///   slot is selectable, so it covers the set and not the clear. So the clear half works in **either** order — a
    ///   slot just made unselectable can still be deselected, and a slot deselected before being
    ///   made unselectable ends in the same state. The set half cannot: a slot an earlier walk
    ///   made unselectable takes its ring back only because making it selectable runs
    ///   *first*, and swapping those two lines is what leaves a clicked item unringed.
    ///
    ///   [a mutation that swaps the clear half's order survives, and that is the explanation
    ///   rather than a gap in the test; the clear-half swap is kept in the harness as a deliberate
    ///   no-op probe and is expected to survive.]
    /// * **the comparison is on the item id alone and is not guarded against zero here.** The guard
    ///   lives in the caller (the element-message handler's "slot has an item" test), so this
    ///   function is
    ///   faithful only if it also compares empty against empty — which it does, `None == None`.
    ///
    /// `clicked` is a positional index into [`Self::slots`]. Returns how many *other* slots were
    /// deselected, which is 0 on every shipped list except the vendor's — see
    /// [`attr::SINGLE_SELECTION`].
    pub fn handle_single_selection(&mut self, ui: &mut UiSystem, clicked: usize) -> usize {
        let Some(id) = self.slots.get(clicked).map(|s| s.item) else {
            return 0;
        };
        let n = self.num_ui_items();
        let mut cleared = 0;
        for i in 0..n {
            if i == clicked {
                continue;
            }
            let Some(s) = self.slots.get_mut(i) else {
                continue;
            };
            if s.item != id {
                continue;
            }
            s.set_selectable_state(false);
            s.set_selected_state(ui, false);
            cleared += 1;
        }
        if let Some(s) = self.slots.get_mut(clicked) {
            s.set_selectable_state(true);
            s.set_selected_state(ui, true);
        }
        cleared
    }

    /// Re-open the first container in this item list, and a caller of
    /// [`Self::update_open_container_indicator`].
    ///
    /// The client examines array entry 0, rather than searching for the first occupied slot. A
    /// non-item entry clears the child list's parent container to zero and returns. An item with id
    /// zero is a no-op. Otherwise it returns when the id is already open or is no longer in this
    /// list; for a new valid id it links the child list to this list, sets that child's parent
    /// container, scrolls a nonempty child list to entry 0, stores the open id, and updates the frame.
    ///
    /// **Its two callers are the inventory and external-container move-item notice
    /// handlers**, not the click path;
    /// "the parent container changed" is a consequence rather than the trigger. The inventory arm compares the moved object with the
    /// 3D item grid's parent-container id. When they match and the new container is not the
    /// backpack list's parent container, it asks the top-container list to reopen its first item.
    ///
    /// I.e. **the pack the grid is showing has itself been moved somewhere that is not the
    /// player**, so the grid is pointing at a container that has left the inventory, and the
    /// main-pack list (whose one slot is the player) re-opens the player's own items.
    /// The top-container and container lists both receive the grid item list as their child list
    /// when the player-description notice arrives.
    ///
    /// Everything this function does to `this` happens here — the open item id and the frame. The
    /// child-list half is returned for the caller, because in this build the child list is a
    /// sibling field of the panel and not a pointer on the widget.
    pub fn open_first_container(&mut self, ui: &mut UiSystem) -> OpenFirstContainer {
        let Some(first) = self.slots.first() else {
            // Entry 0 of the list is not a `UiItemWidget`.
            return OpenFirstContainer::ClearChild;
        };
        let Some(id) = first.item else {
            return OpenFirstContainer::Unchanged;
        };
        if self.open_item_id == Some(id) {
            return OpenFirstContainer::Unchanged;
        }
        if !self.is_in_list(id) {
            return OpenFirstContainer::Unchanged;
        }
        self.open_item_id = Some(id);
        self.update_open_container_indicator(ui, Some(id));
        OpenFirstContainer::Open(id)
    }

    /// Move the ring from `old` to
    /// `new` across this list.
    ///
    /// For each item slot that is not a spell slot (a spell slot has no ring): deselect it when it
    /// holds `old`, then select it when it holds a non-zero `new`.
    ///
    /// Every `ItemListWidget` in the client registers for the selected-item notice in
    /// its post-init and reaches this through the set-selected-item notice, so
    /// a selection change moves the ring in **every** list at once — the backpack grid, the two
    /// container strips, the paper doll and all eighteen quickbar tiles. That is why the caller
    /// side of this is a single edge-triggered pass over every live list and not a per-panel
    /// refresh.
    ///
    /// The spell-slot skip is the client's: a spellbook row has no item id, so an `old` of `None`
    /// would otherwise deselect every spell in the book.
    ///
    /// Returns how many slots were written.
    pub fn set_selected_item(
        &mut self,
        ui: &mut UiSystem,
        old: Option<ObjectId>,
        new: Option<ObjectId>,
    ) -> usize {
        let mut n = 0;
        for s in &mut self.slots {
            if s.spell.is_some() {
                continue;
            }
            if s.item.is_some() && s.item == old {
                s.set_selected_state(ui, false);
                n += 1;
            }
            if new.is_some() && s.item == new {
                s.set_selected_state(ui, true);
                n += 1;
            }
        }
        n
    }

    /// Global message 3 over every slot of this list —
    /// the once-a-second pass that keeps a cooldown wedge moving.
    ///
    /// `remaining` is the player's registry: it is asked once per slot that carries a cooldown id,
    /// and not at all for the rest, which is the cooldown display's own `id > 0` guard.
    ///
    /// Returns how many slots re-ran their display.
    pub fn do_heartbeat(
        &mut self,
        ui: &mut UiSystem,
        now: f64,
        remaining: &dyn Fn(u32) -> Option<f64>,
    ) -> usize {
        let mut n = 0;
        for s in &mut self.slots {
            let r = if s.cooldown_id > 0 {
                remaining(s.cooldown_id)
            } else {
                None
            };
            if s.listen_to_global_message(ui, HEARTBEAT_MESSAGE, now, r) {
                n += 1;
            }
        }
        n
    }

    /// The client's fill, folded the same way
    /// [`Self::set_contents`] folds the inventory's.
    ///
    /// The client's version is a flush then one spell-shortcut insert per spell at the sorted
    /// index; each of those adds an empty slot and then the spell shortcut, and adding the spell
    /// shortcut finds the first slot whose state is `0x1000001C`, creating one when
    /// `FixedListSize == -1` and none is free. Feeding an already
    /// ordered list makes the insert index the position, so the loop is a plain fill — the
    /// ordering is [`SpellbookPanel::sorted`](crate::panels::spellbook::SpellbookPanel::sorted)'s.
    ///
    /// **The argument is the whole [`SpellEntry`](crate::view::SpellEntry)**, not
    /// `(id, icon, name)`, because the client's composite needs the power level and the spell's
    /// bitfield as well as the icon.
    ///
    /// Returns how many slots ended up holding a spell.
    pub fn set_spells(&mut self, ui: &mut UiSystem, spells: &[crate::view::SpellEntry]) -> usize {
        // The spell-shortcut add's "create one when the list is unbounded and nothing is free".
        if self.fixed_list_size < 0 {
            while self.slots.len() < spells.len() {
                let before = self.slots.len();
                self.add_slot(ui);
                if self.slots.len() == before {
                    break;
                }
            }
        } else {
            self.update_fixed_slots(ui);
        }
        self.flush(ui);
        let mut filled = 0;
        for (i, e) in spells.iter().enumerate() {
            let Some(s) = self.slots.get_mut(i) else {
                break;
            };
            s.set_spell(
                ui,
                e.id,
                e.icon,
                &e.name,
                Some(spell_recipe(ui, e.icon_power, e.icon, e.bitfield)),
            );
            ui.set_visible(s.handle, true);
            filled += 1;
        }
        self.update_empty_slots(ui);
        self.update_layout(ui);
        filled
    }

    /// The spell in slot `i`, if any.
    #[must_use]
    pub fn spell_at(&self, i: usize) -> Option<u32> {
        self.slots.get(i).and_then(|s| s.spell)
    }

    /// The object in slot `i`, if any.
    #[must_use]
    pub fn item_at(&self, i: usize) -> Option<ObjectId> {
        self.slots.get(i).and_then(|s| s.item)
    }

    /// Which slot a given element handle is, for routing a click or a drop.
    #[must_use]
    pub fn slot_of(&self, h: ElemHandle) -> Option<usize> {
        self.slots.iter().position(|s| s.handle == h)
    }

    /// The ui item element's shortcut num write on slot `i` — see
    /// [`ItemSlot::set_shortcut_num`].
    pub fn set_shortcut_num(
        &mut self,
        ui: &mut UiSystem,
        i: usize,
        num: i32,
        ghosted: bool,
    ) -> bool {
        let Some(s) = self.slots.get_mut(i) else {
            return false;
        };
        s.set_shortcut_num(ui, num, ghosted)
    }

    /// Set the waiting state on the slot holding `item` — the ghost an outstanding move leaves.
    pub fn ghost(&mut self, ui: &mut UiSystem, item: ObjectId) -> bool {
        let Some(i) = self.slots.iter().position(|s| s.item == Some(item)) else {
            return false;
        };
        self.slots[i].set_waiting(ui, true);
        true
    }

    /// The UI item element's element-message handler: failed drag completion clears
    /// waiting presentation without refilling/reordering the item's containing list.
    pub fn clear_waiting(&mut self, ui: &mut UiSystem, item: ObjectId) -> bool {
        let Some(i) = self.slots.iter().position(|s| s.item == Some(item)) else {
            return false;
        };
        self.slots[i].set_waiting(ui, false);
        true
    }

    /// The list box element's item index at point read, in **list-local** coordinates.
    ///
    /// An empty list, or a point outside the list's width and height, answers nothing. The point
    /// is offset by the scroll position; the column is the first whose running sum of item
    /// widths reaches `x` (else zero), and the row likewise with heights. The index is
    /// `cols * row + col` for a horizontal list and `rows * col + row` otherwise, and is valid
    /// when below the item count.
    ///
    /// Every cell in an item list is the same cell size, so the two running sums collapse to a
    /// division — and the index arithmetic is [`Self::update_layout`]'s fill order read backwards,
    /// which is why the two are asserted against each other rather than against a written table.
    #[must_use]
    pub fn item_index_at_point(&self, ui: &UiSystem, x: i32, y: i32) -> Option<usize> {
        let n = i32::try_from(self.slots.len()).unwrap_or(0);
        if n == 0 {
            return None;
        }
        let b = ui.node(self.handle).map(|n| n.region.box_)?;
        if x < 0 || x >= b.width() || y < 0 || y >= b.height() {
            return None;
        }
        let (cw, ch) = self.cell;
        if cw <= 0 || ch <= 0 {
            return None;
        }
        let (cols, rows) = self.grid(n);
        // The client uses <= at each cumulative boundary: an exact
        // cell edge belongs to the preceding band. Exhausting the array keeps its initial 0.
        let (sx, sy) = self.scroll(ui);
        let band = |point: i32, size: i32, count: i32| {
            let index = (point - 1).max(0) / size;
            if index < count {
                index
            } else {
                0
            }
        };
        let col = band(x + sx, cw, cols);
        let row = band(y + sy, ch, rows);
        let index = if self.horizontal {
            cols * row + col
        } else {
            rows * col + row
        };
        (index >= 0 && index < n).then(|| usize::try_from(index).unwrap_or(0))
    }

    /// The column and row counts, as the layout pass computes them for `n` items.
    fn grid(&self, n: i32) -> (i32, i32) {
        if self.max_columns < 0 {
            (n, i32::from(n != 0))
        } else {
            let cols = self.max_columns.max(1).min(n);
            let rows = if cols == 0 { 0 } else { (n + cols - 1) / cols };
            (cols, rows)
        }
    }

    /// Start a drag from the slot under the point.
    ///
    /// A list without `AllowDragging` (`0x10000016`) does nothing. Otherwise it finds the slot
    /// under the point (made list-local), prepares that slot's drag icon (and stops if it
    /// cannot), selects the slot's object if it is not already selected, ghosts the slot unless
    /// the list is a vendor, salvage or shortcut list, starts a drag of the drag icon at grab
    /// offset (16, 16), and sends the begin-drag notice (item, spell, list kind) and the item-list
    /// begin-drag notice (list, slot number).
    ///
    /// `x`/`y` are **window** coordinates — the press point, which reaches here as
    /// the element message's window point on element message `0x21`.
    ///
    /// Two things are deliberately not here. The selection of the object is the
    /// object model's; and the two notices tell the rest of the client a drag began.
    /// The vendor, salvage, housing, spellcasting, and toolbar consumers are implemented; the
    /// returned [`DragStart`] carries the values delivered across that host boundary.
    ///
    /// **The ghost goes on at pick-up, not at drop.** That is the client's order and it is visible:
    /// an icon dragged out of the backpack greys immediately, and the refusal clears the
    /// waiting state again when the drop is refused.
    pub fn begin_drag(&mut self, ui: &mut UiSystem, x: i32, y: i32) -> Option<DragStart> {
        if !self.allow_dragging {
            return None;
        }
        let (ox, oy) = ui.screen_origin(self.handle);
        let i = self.item_index_at_point(ui, x - ox, y - oy)?;
        let flags = (
            self.container_list,
            self.vendor_list,
            self.shortcut_list,
            self.salvage_list,
        );
        let slot = self.slots.get(i)?.clone();
        if !slot.prepare_drag_icon(ui, self) {
            return None;
        }
        let ghostable = !flags.1 && !flags.3 && !flags.2;
        if ghostable {
            if let Some(s) = self.slots.get_mut(i) {
                s.set_waiting(ui, true);
            }
        }
        let proxy_source = slot.drag_icon?;
        // **The drag start's result is discarded, and that is retail.**
        // The retail client never looks at it; both notices then go out unconditionally.
        // Returning early on a refused start would suppress them.
        //
        // Nothing a player can do reaches the refusal. Every gesture-dependent arm rejects a
        // null element, render device, or input-map manager; action 7 in progress; a negative or
        // off-display drag origin; and the 16-pixel
        // threshold — was answered by the **enclosing** drag start that
        // raised `0x21` in this same synchronous call, on manager state this function does not
        // touch: the refusal leaves the drag origin alone, and the outer
        // call re-arms the drag-started flag before broadcasting. `0x21` has exactly one
        // handler that gets here, so there is no second entry. What is left are the
        // arms that depend only on the layout of the drag icon — it carries `0x3A`, and its slot
        // does not carry `0x39`, so the drag builds the proxy — and those are pinned by an
        // inventory scenario on the shipped salvage row.
        //
        // Retail's behaviour when it *is* forced is a genuine defect and is transcribed rather
        // than repaired, because no modernisation has been chosen: the row leaves a salvage or
        // vendor list with nothing on the cursor, and the release short-circuits
        // on the null drag element, so it runs no drag-complete half and puts
        // nothing back.
        let _refused = !ui.start_drag_and_drop_at(proxy_source, DRAG_GRAB.0, DRAG_GRAB.1);
        Some(DragStart {
            list: self.element,
            slot: i,
            item: slot.item,
            spell: slot.spell,
            drag_icon: proxy_source,
            ghosted: ghostable,
        })
    }
}

/// What [`ItemListWidget::open_first_container`] decided, and what the caller still owes the
/// child list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenFirstContainer {
    /// The three early-outs: no first item, the open item id already names it, or the list does not
    /// hold it. Nothing was written.
    Unchanged,
    /// Entry 0 of the list was not a `UiItemWidget`, so the client's tail sets the child list's
    /// parent container to 0, which flushes the child list.
    ClearChild,
    /// The open item id and the frame have been written on **this** list; the caller owes the child
    /// list its parent list (this one), its parent container (`id`), and a scroll-to-show of
    /// entry 0 when the child holds anything.
    Open(ObjectId),
}

/// The constant grab offset [`ItemListWidget::begin_drag`] passes when it starts the drag, which
/// centres the 32×32 drag icon on the pointer.
pub const DRAG_GRAB: (i32, i32) = (0x10, 0x10);

/// What [`ItemListWidget::begin_drag`] picked up, which is what its two notices carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragStart {
    /// The list the drag left — the item-list begin-drag notice's first argument.
    pub list: ElementId,
    /// The slot number, its second.
    pub slot: usize,
    /// The begin-drag notice's item id.
    pub item: Option<ObjectId>,
    /// The begin-drag notice's spell id.
    pub spell: Option<u32>,
    /// The element the drag start was handed; the proxy is a copy of it.
    pub drag_icon: ElemHandle,
    /// Whether the source slot took the ghost — false on a vendor, salvage or shortcut list.
    pub ghosted: bool,
}

/// Read the drag's payload back off the proxy.
///
/// The five properties [`ItemSlot::prepare_drag_icon`] wrote, folded into the drop-item flags
/// bitfield the client builds from the last four:
///
/// ```text
/// flags = salvage * 8 | shortcut * 4 | vendor * 2 | is_container * 1
/// ```
///
/// The drop is refused outright when the item id is 0 or when any of
/// bits 1..3 is set, i.e. **a vendor, shortcut or salvage drag is not an inventory move** and is
/// somebody else's to route.
#[must_use]
pub fn inq_drop_icon_info(ui: &UiSystem, proxy: ElemHandle) -> DropIconInfo {
    let mut info = DropIconInfo::default();
    let Some(n) = ui.node(proxy) else { return info };
    let p = n.merged_properties();
    let id = |k: u32| match p.get(k) {
        Some(PropertyValue::InstanceId(v)) => Some(*v),
        _ => None,
    };
    info.item = id(drag_attr::ITEM_ID).filter(|v| *v != 0).map(ObjectId);
    info.spell = id(drag_attr::SPELL_ID).filter(|v| *v != 0);
    let b = |k: u32| u32::from(p.get_bool(k).unwrap_or(false));
    // The client's combine matches the drop-item flags exactly
    // (`IS_CONTAINER 1, IS_VENDOR 2, IS_SHORTCUT 4, IS_SALVAGE 8`). Swapping vendor and shortcut
    // would be invisible to [`DropIconInfo::is_inventory_move`]'s `& 0xE`, which covers both bits
    // either way, so [`drag_flags`] states the four numbers as literals.
    info.flags = (b(drag_attr::IS_SALVAGE) * drag_flags::IS_SALVAGE)
        | (b(drag_attr::IS_SHORTCUT) * drag_flags::IS_SHORTCUT)
        | (b(drag_attr::IS_VENDOR) * drag_flags::IS_VENDOR)
        | (b(drag_attr::IS_CONTAINER) * drag_flags::IS_CONTAINER);
    info
}

/// The out-parameters of [`inq_drop_icon_info`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DropIconInfo {
    pub item: Option<ObjectId>,
    pub spell: Option<u32>,
    /// The drop-item flags: bit 0 container, 1 vendor, 2 shortcut, 3 salvage.
    pub flags: u32,
}

impl DropIconInfo {
    /// The client's `(flags & 0xE) == 0` — an inventory move and not a
    /// vendor, shortcut or salvage transaction.
    #[must_use]
    pub const fn is_inventory_move(&self) -> bool {
        self.flags & drag_flags::NOT_AN_INVENTORY_MOVE == 0
    }

    /// The container bit — the dragged object is itself a pack. This is the flag the drop
    /// acceptance receives (the drop release passes `flags & 1`), and it chooses between the items
    /// capacity and list and the containers capacity and list throughout.
    #[must_use]
    pub const fn is_container(&self) -> bool {
        self.flags & drag_flags::IS_CONTAINER != 0
    }
}

/// The ui item element's element-message handler's `0x21` arm.
///
/// On `0x21` (the generic drag was rejected) it finds the nearest ancestor of type `0x10000031`
/// above the slot's parent and, if there is one, starts that list's drag at the message's
/// window point.
///
/// **This is the whole join.** Generic drag dispatch raises `0x21` when nothing from the
/// pressed element up to the root carries `0x3A`, and neither an item slot nor its item list
/// does — slot initialization sets `0x3A = false` on purpose. An item slot is therefore picked
/// up *by being refused* the generic drag, and its list starts a second, deliberate drag with
/// its own drag icon.
///
/// `source` is the element the message names; `lists` is every live `ItemListWidget` the screen
/// owns, in place of the client's ancestor walk.
pub fn begin_drag_from_rejected(
    ui: &mut UiSystem,
    lists: &mut [&mut ItemListWidget],
    source: ElemHandle,
    x: i32,
    y: i32,
) -> Option<DragStart> {
    // **The `0x21` must name a slot.** In the client this is the *`UiItemWidget`*
    // listener's arm: it starts from the pressed slot and finds the list by walking up from its
    // parent to type `0x10000031`. A `0x21` naming the **list** reaches
    // the item list's own element-message handler, which tests only `0x1C` and `0x15`
    // and has no `0x21` arm at all, so retail starts no drag from one. Also accepting
    // `w.handle == source` would be a second, re-entrant drag start, because the walk raises the
    // message at every level.
    let owner = lists.iter().position(|w| w.slot_of(source).is_some())?;
    lists[owner].begin_drag(ui, x, y)
}

/// One `Array` attribute of `UI_ItemList_ShortcutOverlay` entries, flattened to its `DataID`s.
///
/// The original reader extracts a `DataID` at each array index. Here the array is a
/// `Vec<BaseProperty>` whose every member carries id [`attr::SHORTCUT_OVERLAY`] and a `DataFile` value. A member of
/// another type is skipped rather than defaulted, because a defaulted `DataID` would paint an
/// arbitrary surface into a slot.
fn shortcut_overlay_array(ui: &UiSystem, h: ElemHandle, id: u32) -> Option<Vec<DataId>> {
    let props = ui.node(h)?.merged_properties();
    let dereth_assets::ui::PropertyValue::Array(members) = props.get(id)? else {
        return None;
    };
    Some(
        members
            .iter()
            .filter_map(|b| match &b.value {
                dereth_assets::ui::PropertyValue::DataFile(d) => Some(*d),
                _ => None,
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A widget with `n` synthetic slots and a known cell, for the pure layout arithmetic.
    fn widget(n: usize, cols: i32, horizontal: bool) -> ItemListWidget {
        ItemListWidget {
            element: ElementId(1),
            handle: ElemHandle::for_test(1),
            slot_id: ElementId(0x1000_033A),
            container_list: false,
            shortcut_list: false,
            vendor_list: false,
            salvage_list: false,
            allow_dragging: true,
            horizontal,
            at_least_one_empty: false,
            single_selection: false,
            open_item_id: None,
            max_columns: cols,
            fixed_list_size: i32::try_from(n).unwrap(),
            cell: (32, 32),
            slots: (0..n)
                .map(|i| ItemSlot::bare(ElemHandle::for_test(u32::try_from(i).unwrap() + 2)))
                .collect(),
            cached_slots: std::collections::VecDeque::new(),
            parent_container: None,
            created: 0,
            create_failures: 0,
            drag_icon_failures: 0,
        }
    }

    fn origins(ui: &UiSystem, w: &ItemListWidget) -> Vec<(i32, i32)> {
        w.slots
            .iter()
            .map(|s| {
                let b = ui.node(s.handle).map(|n| n.region.box_).unwrap_or_default();
                (b.x0, b.y0)
            })
            .collect()
    }

    /// Oracle: the layout pass. With the horizontal bit **clear** — the
    /// default, and what the shipped inventory lists carry — the placement loop advances `y` by
    /// the row's height until the last row and only then resets `y` and advances `x`.
    /// That is **column-major**, which is the opposite of what "max_columns" reads like.
    #[test]
    fn a_vertical_list_fills_column_major_down_m_n_rows() {
        let mut ui = UiSystem::new((800, 600));
        // Six items, three columns: 3 columns, ceil(6/3) = 2 rows.
        let mut w = widget(0, 3, false);
        for _ in 0..6 {
            let h = ui.create_hollow(None);
            ui.resize_to(h, 32, 32);
            w.slots.push(ItemSlot::bare(h));
        }
        w.fixed_list_size = 6;
        w.update_layout(&mut ui);
        assert_eq!(
            origins(&ui, &w),
            vec![(0, 0), (0, 32), (32, 0), (32, 32), (64, 0), (64, 32)],
            "down each column of two, then across"
        );
    }

    /// Oracle: the same function's other arm — the horizontal bit set fills **row-major** across
    /// the columns, which is the eighteen-slot shortcut bar's own arrangement.
    #[test]
    fn a_horizontal_list_fills_row_major_across_m_n_cols() {
        let mut ui = UiSystem::new((800, 600));
        let mut w = widget(0, 3, true);
        for _ in 0..5 {
            let h = ui.create_hollow(None);
            ui.resize_to(h, 32, 32);
            w.slots.push(ItemSlot::bare(h));
        }
        w.fixed_list_size = 5;
        w.update_layout(&mut ui);
        assert_eq!(
            origins(&ui, &w),
            vec![(0, 0), (32, 0), (64, 0), (0, 32), (32, 32)],
            "across three, then wrap"
        );
    }

    /// Oracle: the list box element's item-index-at-point read **against** the layout pass.
    /// The two are inverses, and asserting them against each other
    /// rather than against a table written here is what makes the claim checkable.
    ///
    /// **Full rigour, deliberately** (the evidence conventions, "tier the evidence bar by
    /// risk"): this arithmetic decides *which slot* a press and a drop land on. Off by one column
    /// and an icon goes into the wrong pack, which is invisible on screen and corrupts a player's
    /// inventory. So every cell of both fill orders is swept, at four points inside each cell, and
    /// each answer is checked against the position `update_layout` actually gave that slot.
    #[test]
    fn every_point_in_every_cell_resolves_to_the_slot_update_layout_put_there() {
        for horizontal in [false, true] {
            for (n, cols) in [(6usize, 3i32), (5, 3), (7, 3), (12, 6), (1, 1), (4, -1)] {
                let mut ui = UiSystem::new((800, 600));
                let list = ui.create_hollow(None);
                let mut w = widget(0, cols, horizontal);
                w.handle = list;
                // The column and row counts exactly as the layout pass computes them, so the list is
                // sized to the grid **before** anything is placed in it — resizing a parent
                // afterwards reflows its children and would be measuring the wrong thing.
                let ni = i32::try_from(n).unwrap();
                let (cols_n, rows_n) = if cols < 0 {
                    (ni, 1)
                } else {
                    let c = cols.max(1).min(ni);
                    (c, (ni + c - 1) / c)
                };
                ui.resize_to(list, cols_n * 32, rows_n * 32);
                for _ in 0..n {
                    let h = ui.create_hollow(Some(list));
                    ui.resize_to(h, 32, 32);
                    w.slots.push(ItemSlot::bare(h));
                }
                w.fixed_list_size = ni;
                w.update_layout(&mut ui);
                let (mx, my) = (cols_n * 32, rows_n * 32);

                for (i, s) in w.slots.iter().enumerate() {
                    let b = ui.node(s.handle).expect("alive").region.box_;
                    // Strictly inside the cell. Retail's <= cumulative-band comparison
                    // assigns exact top/left boundaries to the preceding band.
                    for (dx, dy) in [(1, 1), (31, 1), (1, 31), (31, 31), (16, 16)] {
                        let (x, y) = (b.x0 + dx, b.y0 + dy);
                        assert_eq!(
                            w.item_index_at_point(&ui, x, y),
                            Some(i),
                            "n={n} cols={cols} horizontal={horizontal}: ({x},{y}) is in cell {i} \
                             at {:?}",
                            (b.x0, b.y0)
                        );
                    }
                }
                // Outside the list's own rectangle is nothing at all — the index read's
                // four opening guards, which is what stops a drop just past the last row from
                // landing on the last slot.
                for (x, y) in [(-1, 0), (0, -1), (mx, 0), (0, my)] {
                    assert_eq!(
                        w.item_index_at_point(&ui, x, y),
                        None,
                        "({x},{y}) is off the list"
                    );
                }
            }
        }
    }

    /// Oracle: the layout pass's first branch — attribute `0x5F < 0` sets the column count to
    /// the item count and the row count to `(items != 0)`, i.e. **one row of everything**,
    /// whatever the box is. Six of the shipped lists are in that state.
    #[test]
    fn a_negative_max_columns_puts_everything_in_one_row() {
        let mut ui = UiSystem::new((800, 600));
        let mut w = widget(0, -1, false);
        for _ in 0..4 {
            let h = ui.create_hollow(None);
            ui.resize_to(h, 32, 32);
            w.slots.push(ItemSlot::bare(h));
        }
        w.fixed_list_size = -1;
        w.update_layout(&mut ui);
        assert_eq!(origins(&ui, &w), vec![(0, 0), (32, 0), (64, 0), (96, 0)]);
    }
}
