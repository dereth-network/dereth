//! The item slot's layout, child, state and attribute ids, and its icon recipes.

use super::*;

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
///   machine, transcribed in `dereth_desktop::cursor`, has **no drag arm**. Its nine
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
    ///
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
/// the red / blue / teal / green / brown tiles in the recorded retail backpack frame.
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
/// and the compositing is `dereth_client_shell::ui_draw::composite`.
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
    /// duplicated id in any of them keeps two rings **in retail too**.
    pub const SINGLE_SELECTION: u32 = 0x1000_0052;
    /// `UICore_ListBox_horizontal` — bit 0 of the list box's flags.
    pub const HORIZONTAL: u32 = 0x5C;
    /// `UICore_ListBox_max_columns`.
    ///
    /// The retail `MasterProperty` names are `UICore_ListBox_max_columns` (`0x5F`) and
    /// `UICore_ListBox_max_rows` (`0x60`) — not "rows/cols" — and the list-box layout pass reads
    /// **only** `0x5F`, straight into the column count.
    pub const MAX_COLUMNS: u32 = 0x5F;
    /// `UICore_ListBox_max_rows`. Read by nothing in `ListBox`; carried for completeness.
    pub const MAX_ROWS: u32 = 0x60;

    /// `UI_ItemList_ShortcutOverlayArray` — the **eighteen numerals**
    /// indexes by slot number when the slot holds something and is not ghosted.
    ///
    /// Despite the `UI_ItemList_` prefix the property is carried by
    /// [`child::SHORTCUT_NUM`](super::super::child::SHORTCUT_NUM) — the shortcut numeral, the element the
    /// shortcut-number write reads it off and sets the image of — and by nothing else in the
    /// shipped gameplay tree. In `0x21000005` the first nine entries are the distinct
    /// numerals `0x0600109E`…`0x060010A6` and entries 9…17 are nine copies of `0x060074D3`, which
    /// is why bank 2 has no numbers of its own.
    pub const SHORTCUT_OVERLAY_ARRAY: u32 = 0x1000_0042;
    /// `UI_ItemList_ShortcutOverlayArray_Ghosted` — the same eighteen in the greyed form
    /// (`0x06001ACC`…`0x06001AD4`, then `0x060074D2`), used while the toolbar is inactive.
    pub const SHORTCUT_OVERLAY_ARRAY_GHOSTED: u32 = 0x1000_0043;
    /// `UI_ItemList_ShortcutOverlayArray_Empty` — the array the shortcut-number write picks when
    /// the icon is in state `0x1000001C`.
    ///
    /// **No element in the shipped `0x21000005` tree carries it**, so the empty arm of
    /// the shortcut-number write finds no array and — because both the image write and the show
    /// happen only when the array lookup succeeds — does nothing at all.
    /// An empty quickbar tile therefore gets its number from somewhere else entirely — its own
    /// `ItemSlot` root's `0x1000001C` frame; see
    /// [`ShortcutBar`](crate::toolbar::shortcuts::ShortcutBar).
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
