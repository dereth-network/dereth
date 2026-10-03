//! The inventory, backpack, paper-doll, and 3D-item elements of the backpack panel.
//!
//! # The four lists, and which is which
//!
//! Inventory page `0x1000018B` holds three sub-panels, and between
//! them they own **twenty-eight** item lists:
//!
//! | list | id | shape | holds |
//! |---|---|---|---|
//! | top container ([`TOP_CONTAINER`]) | `0x100001C9` | 1 slot, containers only | the player himself — the "main pack" icon |
//! | container list ([`CONTAINER_LIST`]) | `0x100001CA` | 7 slots, containers only | the player's side packs |
//! | item list ([`ITEM_LIST`]) | `0x100001C6` | 102 slots, 6 columns | the **contents of whichever pack is open** |
//! | paper-doll slots | 24 ids | 1 slot each | the equipped item whose `loc` the slot's mask selects |
//!
//! The inventory panel's set-display-inventory notice is what joins them: it makes the item list
//! the child list of the top container and of the container list, so both container strips drive
//! the one grid, and opening the player in the top container is what puts the player's own loose
//! items in it on the first open.
//!
//! # Why nothing here writes
//!
//! Every list is filled from the [`GameView`] and from nothing else. A drag emits a
//! [`UiRequest::DragDrop`] and the source slot ghosts (enters its waiting state); the item
//! moves when — and only when — `Item_ServerSaysMoveItem` changes what the view returns.

use dereth_primitives::ObjectId;
use dereth_ui::{ElemHandle, ElementId, UiSystem};

use crate::bind::{attr, set_attr_float};
use crate::items::widget::{ItemListWidget, SlotInfo, TileInfo};
use crate::view::{DropTarget, GameView, UiRequest};

/// The top container — the one-slot "main pack" strip.
pub const TOP_CONTAINER: ElementId = ElementId(0x1000_01C9);
/// The container list — the side-pack strip.
pub const CONTAINER_LIST: ElementId = ElementId(0x1000_01CA);
/// The item list — the grid the open pack's contents go in.
pub const ITEM_LIST: ElementId = ElementId(0x1000_01C6);
/// The burden meter, filled through attribute `0x69`.
pub const BURDEN_METER: ElementId = ElementId(0x1000_01D9);
/// The burden text — the `"69%"` beside the meter.
pub const BURDEN_TEXT: ElementId = ElementId(0x1000_01D8);

/// The client's scale: **the meter's full height is 300% burden**, so
/// a fully-laden character fills a third of it.
///
/// The constant is the `double` nearest one third, and it is written out rather than computed
/// because the arithmetic below is exact and `1.0/3.0` is not the same bit pattern in every
/// evaluation order.
pub const LOAD_METER_SCALE: f64 = 0.333_333_333_333_333_3;

/// `(Meter position, percentage the text shows)`.
///
/// Retail multiplies the load by one third, clamps it to `0.0..=1.0`, sets the meter's float
/// attribute `0x69` to it (as `float`), and writes the text `"%d%%"` of `floor(load * 300.0)`.
///
/// The format is `"%d%%"` — the number and a per-cent sign, which is exactly what the recorded
/// retail backpack shows: *Burden* over *69%*. [verified against retail]
///
/// Two consequences of reproducing rather than tidying, both measured:
///
/// * **The clamp comes before the text, not after**, so the text saturates at `300%` however
///   overloaded the character is — the meter and the number cannot disagree.
/// * **The rounding is the client's, and it is not the obvious one.** The load arrives as an
///   `f32`, is widened to `double`, multiplied by the `double` nearest one third and then by 300.
///   A load of exactly `0.69f32` therefore prints **68**, not 69, because the nearest `f32` to
///   0.69 is below it and the product lands at `68.99999976`. Computing `load * 100.0` instead
///   would print 69 and disagree with the client.
#[must_use]
pub fn load_level(load: f32) -> (f32, i32) {
    let p = (f64::from(load) * LOAD_METER_SCALE).clamp(0.0, 1.0);
    #[allow(clippy::cast_possible_truncation)] // the client's own float narrowing and truncation
    {
        (
            p as f32,
            dereth_primitives::num::to_i32_f64((p * 300.0).floor()),
        )
    }
}

/// The text the load-level update writes — `"%d%%"`.
#[must_use]
pub fn load_text(percent: i32) -> String {
    format!("{percent}%")
}
/// The paper-doll sub-panel.
pub const PAPER_DOLL: ElementId = ElementId(0x1000_01CD);

/// The title text — the panel's own title bar.
///
/// Bound by the inventory panel's post-init (child `0x100001D3`) and written by
/// its player-description-received notice and by nothing else, which is why an unwritten one
/// is *empty* rather than defaulted: the shipped layout authors no text into it.
pub const TITLE_TEXT: ElementId = ElementId(0x1000_01D3);

/// The contents text — the *Contents of …* heading over the grid.
pub const CONTENTS_TEXT: ElementId = ElementId(0x1000_01C5);

/// The paper-doll viewport, in which the doll's preview-scene space draws
/// inside.
pub const PAPER_DOLL_VIEWPORT: ElementId = ElementId(0x1000_01D5);

/// The paper-doll drag mask — the transparent region over the doll that catches a drop
/// on the body itself.
///
/// It lives in [`dereth_client_contract::panels::inventory`], because
/// `dereth_client::interaction` makes the other side of the drop-target comparison.
pub use dereth_client_contract::panels::inventory::PAPER_DOLL_DRAG_MASK;

/// The paper doll panel's refusal when it will not accept a dragged object.
///
/// It lives in [`dereth_client_contract::panels::inventory`], because
/// `dereth_client::interaction` is what speaks it. Verbatim, apostrophe included.
pub use dereth_client_contract::panels::inventory::CANNOT_PUT_THAT_ITEM_THERE;

/// The *Slots* checkbox under the doll.
pub const SLOT_CHECKBOX: ElementId = ElementId(0x1000_05BE);

/// The doll's drag icon — the element the panel **creates** rather than finds,
/// and the one the figure's drag is started from.
///
/// Retail resolves the ItemSlot layout (enum `0x10000038`), creates element `0x10000345` from it
/// as a child of the drag mask, and hides it.
///
/// It is the **same** `0x10000038` layout and the **same** `0x10000345` element id
/// every inventory slot gets its drag icon from
/// ([`crate::items::widget::child::DRAG_ICON`]), which is why the drag start finds
/// a draggable element at once and `(0x10, 0x10)` centres a 32x32 icon on the pointer.
pub const PAPER_DOLL_DRAG_ICON: ElementId = ElementId(0x1000_0345);

/// The paper-doll drag overlay — the whole-doll drop-accept overlay, the last of the panel's
/// 28 bound ids.
///
/// The paper doll's post-init binds child `0x1000046d`. It is the doll's
/// exact counterpart to the backpack icon's drag overlay `0x1000046C`, which
/// `screens/gameplay.rs` drives — same shape, same three state-setting routes, a different
/// predicate.
///
/// **Like the backpack icon's, it is never shown or hidden — it is put in a *state*.** The
/// five calls occur in the paper doll's element-message and drag-over handlers.
/// For message `0x3E` on drag mask `0x100001D6`, leaving (first parameter `0`) or having no
/// drag element sets state `0x1000003F`; otherwise the drag-over predicate runs. Message
/// `0x15` on that same mask sets `0x1000003F` before handling the drop release.
///
/// The predicate reads the proxy's drop information. A zero item id, alias flags masked
/// by `0xE`, or an unavailable player system returns without changing the overlay. It
/// initializes the already-worn output to zero and checks automatic wear legality with
/// `quiet = 1`. Legal wear sets accept state `0x10000040`. Illegal wear leaves the state
/// unchanged if already worn, and otherwise sets refusal state `0x10000041`.
///
/// Three things separate it from the doll *slots*' handler
/// ([`InventoryPanels::on_paper_doll_drag_over`]):
///
/// * **it checks automatic wear legality** — wearing armour
///   and clothing, with the clothing-priority clash, rather than wielding a weapon with the
///   combat-mode and shield rules;
/// * **there is no slot mask.** The mask is the whole figure, so retail asks only "can this be
///   put on at all", which is why dropping on the body auto-places rather than filling one slot;
/// * **the "already worn" answer is silence, not a refusal.** The legality check's already-worn output
///   is set only when the item has a location on the player, and the client turns that
///   one false into *no state at all*. Dragging a worn piece back over your own figure leaves the
///   overlay exactly as it was, matching the spell-casting list's no-state drag-over
///   case and the slot handler's unknown-object case.
pub const PAPER_DOLL_DRAG_OVERLAY: ElementId = ElementId(0x1000_046D);

/// The three states the paper-doll handler puts [`PAPER_DOLL_DRAG_OVERLAY`] in. Pinned as literals so a
/// test can read them without going through the symbol that wrote them, so a wrong constant on
/// either side is visible.
pub mod paper_doll_drag_overlay {
    use dereth_ui::StateId;
    /// `0x10000040` — the drop is legal.
    pub const ACCEPT: StateId = StateId(0x1000_0040);
    /// `0x10000041` — refused, and not because it is already worn.
    pub const REFUSE: StateId = StateId(0x1000_0041);
    /// `0x1000003f` — nothing showing: the drop,
    /// leaving the mask, and a `0x3E` raised with no drag element.
    pub const DOWN: StateId = StateId(0x1000_003F);
}

// -----------------------------------------------------------------------------------------------
// The click map — the paper-doll panel's doll-mode hit test.
// -----------------------------------------------------------------------------------------------

/// The enum lookup for group `0x1000000C`, entry 7, that retail resolves the click map
/// through. On this dat build it resolves to
/// `0x06004D8B`, a 100x214 `PFID_R8G8B8` surface. **Never hard-code the DataID**: a DDD patch can
/// move it, which is the whole reason the enum hop exists.
const CLICK_MAP_ENUM: (u32, u32) = (7, 0x1000_000C);

/// Pixel format 20, `D3DFMT_R8G8B8`: three bytes per texel in memory order **B, G, R**. The
/// only format the shipped click map uses, and the only one this decoder accepts — anything else
/// answers `None` rather than guessing at a stride.
const PFID_R8G8B8: u32 = 20;

/// The nine global colour values the hit test compares against, and the
/// `INVENTORY_LOC` mask each one resolves to.
///
/// Each is four `float`s `(r, g, b, a)`. Every one is `a = 1.0` and every channel is `0.0`, `1.0`
/// or `0.50196081`, which is `128 / 255` to the bit — so the colour comparison is an
/// exact match on the three source bytes and can be made on the bytes themselves.
///
/// | region | RGB | mask |
/// |---|---|---|
/// | head | `00 00 FF` | `0x0001` |
/// | chest | `00 FF 00` | `0x0202` |
/// | abdomen | `FF 00 00` | `0x0404` |
/// | upper arm | `00 FF FF` | `0x0808` |
/// | lower arm | `FF 00 FF` | `0x1010` |
/// | upper leg | `FF FF 00` | `0x2040` |
/// | lower leg | `00 00 80` | `0x4080` |
/// | hand | `00 80 00` | `0x0020` |
/// | foot | `80 00 00` | `0x0100` |
///
/// Each mask is a **pair** of `INVENTORY_LOC` bits — the clothing bit and the armour bit for the
/// same body region (chest is `0x200 | 0x0002`) — except the head, the hand and the foot, which
/// have one each. That pairing is why the hit test needs a priority walk:
/// two items can cover one colour.
pub const HIT_TEST_COLOURS: [([u8; 3], u32); 9] = [
    ([0x00, 0x00, 0xFF], 0x0001),
    ([0x00, 0xFF, 0x00], 0x0202),
    ([0xFF, 0x00, 0x00], 0x0404),
    ([0x00, 0xFF, 0xFF], 0x0808),
    ([0xFF, 0x00, 0xFF], 0x1010),
    ([0xFF, 0xFF, 0x00], 0x2040),
    ([0x00, 0x00, 0x80], 0x4080),
    ([0x00, 0x80, 0x00], 0x0020),
    ([0x80, 0x00, 0x00], 0x0100),
];

/// The click map — the CPU-side surface the doll's hit test reads.
///
/// Retail releases the old one, resolves `CLICK_MAP_ENUM`, asks the render
/// device for a *local* (system-memory) surface of the record's own extent and format, and blits
/// the record into it. This build keeps the record's pixels directly: the blit is
/// a normal blit at scale `1.0` between two surfaces of identical extent and format, which is a
/// copy, and there is no GPU resource on either side of it here.
#[derive(Debug, Clone)]
pub struct ClickMap {
    pub did: dereth_primitives::DataId,
    pub width: i32,
    pub height: i32,
    /// `width * height * 3`, in `PFID_R8G8B8`'s B, G, R order.
    bgr: Vec<u8>,
}

impl ClickMap {
    /// The colour at `(x, y)` followed by the nine colour comparisons — the whole of the
    /// paper-doll item-under-mouse lookup except its final top-level-object step.
    ///
    /// A point outside the surface reads no colour, which the client answers with
    /// `0`; a colour that matches none of the nine leaves the mask at the value the *first*
    /// comparison wrote, which is `0` for anything that is not the head. Both are `0` here, and
    /// the top-level object of `0` is `0` — the client's own "nothing under the mouse".
    #[must_use]
    pub fn mask_at(&self, x: i32, y: i32) -> u32 {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return 0;
        }
        let i = (y as usize * self.width as usize + x as usize) * 3;
        let Some(px) = self.bgr.get(i..i + 3) else {
            return 0;
        };
        let rgb = [px[2], px[1], px[0]];
        HIT_TEST_COLOURS
            .iter()
            .find(|(c, _)| *c == rgb)
            .map_or(0, |(_, m)| *m)
    }
}

/// **The nine armour-coverage slots the post-init hides.**
///
/// Left visible, these are armour-coverage slot icons laid out in a grid where the doll should
/// be. The post-init binds each of these nine, registers its drag handler and its tooltip, and
/// then hides each slot icon, repeating the same operation for all nine. \[verified\]
///
/// They are not decoration and they are not dead: the client's message-1
/// arm on [`SLOT_CHECKBOX`] swaps them for the doll and back — see [`InventoryPanels::set_slot_view`].
/// A rebuild that bound them and left them visible draws nine icon grids on top of the figure,
/// unlike the recorded retail backpack's upper panel.
///
/// Order is the post-init's. The masks are [`PAPER_DOLL_SLOTS`]'.
pub const ARMOUR_COVERAGE_SLOTS: [ElementId; 9] = [
    ElementId(0x1000_05AB), // head
    ElementId(0x1000_05AC), // chest
    ElementId(0x1000_05AD), // abdomen
    ElementId(0x1000_05AE), // upper arm
    ElementId(0x1000_05AF), // lower arm
    ElementId(0x1000_05B0), // hand
    ElementId(0x1000_05B1), // upper leg
    ElementId(0x1000_05B2), // lower leg
    ElementId(0x1000_05B3), // foot
];

/// The three aetheria sigils, also hidden in the post-init (on the second lookup of each id,
/// which is what the panel's sigil item fields hold).
///
/// The player-description notice restores the aetheria slots according to
/// `PropertyInt 0x142`'s bits 0, 1 and 2 — the character's aetheria slot count. The corpus's
/// characters have none, and retail's frame shows none, which is why they are hidden here and why
/// the notice arm is not reproduced rather than guessed at.
pub const SIGIL_SLOTS: [ElementId; 3] = [
    ElementId(0x1000_0595),
    ElementId(0x1000_0596),
    ElementId(0x1000_0597),
];

/// The inventory panel's player desc received notice — `"Inventory of %s"`.
///
/// The `%s` is the player object's singular name,
/// i.e. the player's own weenie name. [verified against retail]
#[must_use]
pub fn title_text(name: &str) -> String {
    format!("Inventory of {name}")
}

/// The client writes this literal into the contents text before any
/// container has been opened, and
/// writes it again whenever the open container **is** the player. [verified against retail]
pub const BACKPACK_CONTENTS: &str = "Contents of Backpack";

/// The inventory panel's new parent container notice.
///
/// If the new container is the player, the contents text becomes `"Contents of Backpack"`;
/// otherwise `"Contents of %s"` with the container's name.
///
/// `None` is the player's own pack — the client's is-the-player arm — and is **not** the same as a
/// container whose name has not arrived, which keeps the previous heading up because
/// the new-parent-container notice has not fired for it.
#[must_use]
pub fn contents_text(container_name: Option<&str>) -> String {
    match container_name {
        None => BACKPACK_CONTENTS.to_string(),
        Some(n) => format!("Contents of {n}"),
    }
}

/// The paper-doll panel's preview-scene space, as post-initialization and creature redress
/// set it up.
///
/// The model half only: the ids, the camera, the light and the pose. The host resolves the
/// animation enum through the enum-to-DataID lookup and owns the space itself, exactly as it does
/// for the smart-box and character-preview views.
pub mod paper_doll {
    /// The client's `0x200000` — the shield slot,
    /// which is the one slot whose mask is widened for the item over it. Pinned as
    /// a literal rather than read through `dereth_client_model`'s `loc::SHIELD`, so a wrong constant on
    /// either side is visible.
    pub const SHIELD_LOC: u32 = 0x0020_0000;
    /// The same check's `0x100000` — a one-handed weapon's own
    /// location, which the shield slot accepts (the two are or-ed together).
    pub const MELEE_WEAPON_LOC: u32 = 0x0010_0000;

    /// The paper doll's camera position —
    /// `(0.12, -2.4, 0.88)` looking at the origin.
    ///
    /// Initialization sets the position and zeroes the target. \[verified\]
    pub const CAMERA_POSITION: (f32, f32, f32) = (0.12, -2.4, 0.88);

    /// The second `Vector3` handed to the camera is `(0, 0, 0)`, the same target
    /// re-issued for every known heritage.
    pub const CAMERA_TARGET: (f32, f32, f32) = (0.0, 0.0, 0.0);

    /// The paper doll's light: a distant light of intensity 2.0 in this direction.
    ///
    /// The direction is `(0.3, **+1.9**, 0.65)`.
    /// Positive y, like the character preview's and unlike the portal space's.
    /// \[verified\]
    pub const LIGHT_DIRECTION: (f32, f32, f32) = (0.3, 1.9, 0.65);

    /// The light's intensity. \[verified\]
    pub const LIGHT_INTENSITY: f32 = 2.0;

    /// The doll object's heading, set when the creature is redressed — the fixed three-quarter
    /// pose, and the reason the doll is not seen edge-on.
    pub const HEADING_DEGREES: f32 = 191.3679;

    /// The doll's animation is set by clearing the
    /// sequence, then starting at frame **1**, at **0 fps**.
    ///
    /// Zero is not a mistake and not "unset". The paper doll is a *pose*, not a loop:
    /// update advances `framerate * dt`, so a zero framerate holds frame 1 for ever.
    /// A rebuild that "helpfully" ran it at 30 fps would animate a figure retail keeps still.
    pub const LOW_FRAME: i32 = 1;
    /// See [`LOW_FRAME`].
    pub const FRAMERATE: f32 = 0.0;

    /// The heritages that move the camera, keyed by
    /// `PropertyInt 0xBC HeritageGroup`.
    ///
    /// The `switch` has **no default arm that sets the camera**: heritages 1–5, 10 and 11 fall
    /// through to the `return` and keep the post-init's [`CAMERA_POSITION`]. That is why an Aluvian
    /// looks right with the constant above and why this table is short.
    ///
    /// Cases 12 (`0x10000011`) and 13 (`0x10000013`) also swap the doll's animation for their own
    /// `UIASSET` enum before moving the camera — the Olthoi heritages, which have no humanoid
    /// idle. Those two enums are the second element of each row; `None` keeps
    /// [`super::PAPER_DOLL_ANIMATION_ENUM`].
    /// One row of [`RACE_CAMERA`]: the heritage, the camera position, and the `UIASSET` enum that
    /// replaces the doll's animation (only the two Olthoi heritages have one).
    pub type RaceCamera = (u32, (f32, f32, f32), Option<u32>);

    /// See [`RaceCamera`].
    pub const RACE_CAMERA: [RaceCamera; 6] = [
        (6, (0.12, -3.0, 0.88), None),
        (7, (0.12, -3.0, 0.88), None),
        (8, (0.12, -3.4, 1.0), None),
        (9, (0.12, -3.4, 0.88), None),
        (12, (0.12, -3.4, 0.88), Some(0x1000_0011)),
        (13, (0.12, -3.4, 0.88), Some(0x1000_0013)),
    ];

    /// [`RACE_CAMERA`] as the per-race update asks it: the camera and the animation enum for a
    /// heritage, or `None` when the `switch` falls through and nothing is re-issued.
    #[must_use]
    pub fn for_race(heritage: u32) -> Option<((f32, f32, f32), Option<u32>)> {
        RACE_CAMERA
            .iter()
            .find(|(h, _, _)| *h == heritage)
            .map(|(_, c, a)| (*c, *a))
    }
}

/// The paper doll panel's constructor's last step — the doll's animation is enum
/// `0x10000005`, the `UIASSET` group's
/// `PaperDollAnimation`. Resolved by the host through `dereth_client::assets::enum_did`; this crate
/// carries the enum because the client does.
pub const PAPER_DOLL_ANIMATION_ENUM: u32 = 0x1000_0005;

/// `PropertyInt 0xBC` — `HeritageGroup`.
///
/// It lives in [`dereth_client_contract::panels::inventory`]; `dereth_client::hud` reads
/// the property off the qualities.
pub use dereth_client_contract::panels::inventory::HERITAGE_GROUP_PROPERTY;

/// The slot element id and the
/// `INVENTORY_LOC` mask the client tests it with.
///
/// Taken from the client's two `switch`es. The masks are `dereth_client_model`'s `inventory::slots::loc`
/// values; they are written literally here because this crate does not depend on `dereth-client-model`.
///
/// Three entries are **not** a single bit. `0x100001DF` (the ready weapon) is `0x03500000` —
/// melee | missile | held | two-handed — so a two-handed sword lands in the same slot a dagger
/// does; the neck slot's test is on bit 15; and the clearing
/// pass (item 0 into location `0x7FFFFFFF`) runs every arm at once.
pub const PAPER_DOLL_SLOTS: [(u32, u32); 24] = [
    (0x1000_01DA, 0x0000_8000), // neck
    (0x1000_01DB, 0x0001_0000), // left wrist
    (0x1000_01DC, 0x0004_0000), // left ring
    (0x1000_01DD, 0x0002_0000), // right wrist
    (0x1000_01DE, 0x0008_0000), // right ring
    (0x1000_01DF, 0x0350_0000), // weapon (ready)
    (0x1000_01E0, 0x0080_0000), // ammo (ready)
    (0x1000_01E1, 0x0020_0000), // shield (ready)
    (0x1000_01E2, 0x0000_0002), // shirt
    (0x1000_01E3, 0x0000_0040), // pants
    (0x1000_058E, 0x0400_0000), // trinket
    (0x1000_0595, 0x1000_0000), // sigil 1
    (0x1000_0596, 0x2000_0000), // sigil 2
    (0x1000_0597, 0x4000_0000), // sigil 3
    (0x1000_05AB, 0x0000_0001), // head
    (0x1000_05AC, 0x0000_0200), // chest
    (0x1000_05AD, 0x0000_0400), // abdomen
    (0x1000_05AE, 0x0000_0800), // upper arm
    (0x1000_05AF, 0x0000_1000), // lower arm
    (0x1000_05B0, 0x0000_0020), // hand
    (0x1000_05B1, 0x0000_2000), // upper leg
    (0x1000_05B2, 0x0000_4000), // lower leg
    (0x1000_05B3, 0x0000_0100), // foot
    (0x1000_05E9, 0x0800_0000), // cloak
];

/// The location mask the client clears the doll with, by putting item 0 into every location.
pub const CLEAR_ALL_LOCATIONS: u32 = 0x7FFF_FFFF;

/// Every live item list of the inventory page, plus the container the grid is currently showing.
#[derive(Debug, Default)]
pub struct InventoryPanels {
    /// [`TOP_CONTAINER`].
    pub top_container: Option<ItemListWidget>,
    /// [`CONTAINER_LIST`].
    pub container_list: Option<ItemListWidget>,
    /// [`ITEM_LIST`] — the child list of both of the above.
    pub item_list: Option<ItemListWidget>,
    /// The twenty-four paper-doll slots, in [`PAPER_DOLL_SLOTS`] order.
    pub doll: Vec<(u32, ItemListWidget)>,
    /// [`BURDEN_METER`].
    pub burden_meter: Option<ElemHandle>,
    /// [`BURDEN_TEXT`].
    pub burden_text: Option<ElemHandle>,
    /// The `(position, percentage)` [`load_level`] last produced, so a test can read the pair back
    /// without going through the elements, and so the write is skipped when nothing moved —
    /// which is the client's own arrangement, since the load-level update is driven by the two notices
    /// load-changed and player-description-received rather
    /// than by a frame walk.
    pub burden: Option<(f32, i32)>,
    /// The load the pair above was computed from.
    last_load: Option<f32>,
    /// [`TITLE_TEXT`].
    title: Option<ElemHandle>,
    /// [`CONTENTS_TEXT`].
    contents: Option<ElemHandle>,
    /// The paper-doll viewport, in which the host draws the preview-scene space.
    /// The panel holds the *element*; the space itself belongs to the renderer.
    pub paper_doll_viewport: Option<ElemHandle>,
    /// What [`Self::update`] last wrote into [`Self::title`], so a test can read the string back
    /// without going through the element. `None` is "no player description has arrived", which is
    /// the client's own state before the player-description-received notice fires and is what
    /// leaves the bar blank.
    pub title_shown: Option<String>,
    /// [`SLOT_CHECKBOX`].
    slot_checkbox: Option<ElemHandle>,
    /// [`PAPER_DOLL_DRAG_MASK`] — hides and shows with the doll.
    doll_drag_mask: Option<ElemHandle>,
    /// The paper-doll drag overlay; see [`PAPER_DOLL_DRAG_OVERLAY`].
    pub doll_drag_overlay: Option<ElemHandle>,
    /// The doll's drag icon — see [`PAPER_DOLL_DRAG_ICON`].
    pub doll_drag_icon: Option<ElemHandle>,
    /// The paper-doll click map, loaded during setup and sampled to find the item
    /// under the mouse.
    pub click_map: Option<ClickMap>,
    /// Which of the two the doll's rectangle is showing: `false` is the doll (the checkbox
    /// unchecked, and the post-init's state), `true` is the nine armour grids.
    pub slots_view: bool,
    /// What was last written into [`Self::contents`]. Never `None` after
    /// [`Self::post_init`], because it writes the literal.
    pub contents_shown: Option<String>,
    /// How many slots the last [`Self::update`] decorated — the denominator for "the tooltips
    /// went on". Zero with a non-zero [`Self::slots_filled`] is a silent failure and is why this
    /// is a number.
    pub slots_decorated: usize,
    /// The rendered or requested container, refreshed from the shared view on each update.
    pub open_container: Option<ObjectId>,
    /// The last `(player, contents, containers, equipment)` shape the panel was filled for, so a
    /// per-frame drive is a no-op until the server changes something.
    last: Option<Snapshot>,
    /// The other half of that memory: everything [`ItemListWidget::decorate`] read for each slot
    /// on the last fill, in fill order.
    ///
    /// It is a *separate* field rather than a member of [`Snapshot`] for one reason, the same one
    /// the quickbar gives: a `TileInfo` owns two `String`s, so building one per slot per
    /// frame in order to compare it would allocate on **every** frame the gate is shut, which is
    /// almost all of them. [`TileInfo::matches`] asks the view field by field instead, and the
    /// clone happens only on a frame that rebuilds. It is written and cleared with [`Self::last`], never
    /// apart from it.
    last_tiles: Vec<Option<TileInfo>>,
    /// How many item-slot elements the four lists have between them. Zero means the lists were
    /// never populated, so it is a number and not a feeling.
    pub slots_created: u32,
    /// Slots that ended up holding an object on the last fill.
    pub slots_filled: usize,
    /// How many of the two container strips' open-container frames the last
    /// pass actually changed. A number rather than a feeling because the frame exists on exactly
    /// one slot root and a rebuild that pointed the pass at the grid instead would report zero for
    /// ever.
    pub open_frames: usize,
}

// **The tile's three frozen-world inputs, and the plural-name mapping that decides one of them,
// live in `items/widget.rs` and nowhere else.**
//
// This panel and `toolbar/shortcuts.rs` both use them. The reader fills the guard's memory and
// the matcher compares it, so they are the two halves of one early-out; if two copies diverged,
// the gate would flap open every frame (four lists, the drag ghost and the selection ring flushed
// and refilled, at a cost invisible on screen) or close for good (every tooltip in the pack
// frozen) — and neither fails anything visibly, which is why there is one copy rather than a test
// that two agree. See [`crate::items::widget::TileInfo`].

#[derive(Debug, Clone, PartialEq)]
struct Snapshot {
    player: Option<ObjectId>,
    open: Option<ObjectId>,
    items: Vec<ObjectId>,
    containers: Vec<ObjectId>,
    equipment: Vec<(ObjectId, u32)>,
    /// **The waiting flag is not here**, although the gate must see it: it is set when a request
    /// goes out and **nothing else changes at that moment**, so a gate that could not see it would
    /// fire and the ghost would never appear. The client has exactly one mechanism for it — the
    /// compare at the item update, which runs for every item slot of every list — so `waiting` is
    /// a field of `SlotDecoration`, inside [`InventoryPanels::last_tiles`] like the other three
    /// object flags. A separate per-list flag vector would leave the container list and the top
    /// container with no route, and a **backpack** ghosted in a container strip could never
    /// un-ghost.
    ///
    /// The items capacity of the open container and
    /// the containers capacity of the player — [`ItemListWidget::set_contents`]'s
    /// fixed-list-size argument, which decides how many empty slots each list draws.
    ///
    /// **These two were already contained, and they are here anyway.** Every id `update` asks
    /// about also has a `SlotDecoration` in [`InventoryPanels::last_tiles`], and `SlotDecoration`
    /// carries `items_capacity` and `containers_capacity` off the same `PublicWeenieDesc` the two
    /// accessors read — so on the production host a capacity change already moved the snapshot.
    /// That is a **coupling between two accessors**, not a property of the guard: a host that
    /// answered `GameView::items_capacity` from a different source than `GameView::slot_decoration`
    /// would reopen the hole in silence. The rule is that the compared value is a superset of
    /// the rebuild's inputs, and a superset that holds only while two seam methods agree is not
    /// one. Two `Option<i32>` make it a superset by construction, and cost two integer compares.
    items_capacity: Option<i32>,
    containers_capacity: Option<i32>,
}

impl InventoryPanels {
    /// The inventory panel's post-init + the backpack panel's post-init +
    /// the 3D-items panel's post-init + the paper-doll panel's post-init, reduced to the
    /// part that has to happen for anything to draw: every item list runs
    /// initialization, which creates its item-slot children.
    ///
    /// `page` is the inventory page `0x1000018B`.
    pub fn post_init(&mut self, ui: &mut UiSystem, page: ElemHandle) {
        let init = |ui: &mut UiSystem, id: ElementId| -> Option<ItemListWidget> {
            let h = ui.get_child_recursive(page, id)?;
            Some(ItemListWidget::init(ui, h))
        };
        self.top_container = init(ui, TOP_CONTAINER);
        self.container_list = init(ui, CONTAINER_LIST);
        self.item_list = init(ui, ITEM_LIST);
        self.doll.clear();
        for (id, mask) in PAPER_DOLL_SLOTS {
            if let Some(w) = init(ui, ElementId(id)) {
                self.doll.push((mask, w));
            }
        }
        // The client binds these two and then sets the load level to 0.0
        // **before** any player description has arrived, which is why retail's backpack reads
        // `0%` rather than blank on the first open.
        self.burden_text = ui.get_child_recursive(page, BURDEN_TEXT);
        self.burden_meter = ui.get_child_recursive(page, BURDEN_METER);
        self.burden = None;
        self.last_load = None;
        self.set_load_level(ui, 0.0);

        // Retail binds the title text and leaves it
        // empty, and binds the contents text and immediately writes
        // `"Contents of Backpack"` into it. Both are on the page, so both are found from here.
        self.title = ui.get_child_recursive(page, TITLE_TEXT);
        self.contents = ui.get_child_recursive(page, CONTENTS_TEXT);
        self.paper_doll_viewport = ui.get_child_recursive(page, PAPER_DOLL_VIEWPORT);
        self.title_shown = None;
        self.contents_shown = None;
        self.set_contents_title(ui, None);

        // The post-init's nine (plus three) hides, and the state they leave the
        // panel in: the doll up, the armour grids down. `false` is the checkbox's own unchecked
        // state, which is what the shipped layout authors and what retail's frame shows.
        self.slot_checkbox = ui.get_child_recursive(page, SLOT_CHECKBOX);
        self.doll_drag_mask = ui.get_child_recursive(page, PAPER_DOLL_DRAG_MASK);
        // The post-init's lookup of child `0x1000046d`, the drag overlay.
        self.doll_drag_overlay = ui.get_child_recursive(page, PAPER_DOLL_DRAG_OVERLAY);
        for id in SIGIL_SLOTS {
            if let Some(h) = ui.get_child_recursive(page, id) {
                ui.set_visible(h, false);
            }
        }
        self.set_slot_view(ui, false);
        // The post-init's own drag icon, created just before the click map — the element the
        // figure's drag is started from; see [`PAPER_DOLL_DRAG_ICON`].
        self.doll_drag_icon = self.doll_drag_mask.and_then(|m| {
            let h = ui
                .require_env()
                .and_then(|e| {
                    e.create_child_element_by_enum(
                        ui,
                        m,
                        crate::items::widget::ITEM_SLOT_LAYOUT,
                        PAPER_DOLL_DRAG_ICON,
                    )
                })
                .ok()?;
            ui.set_visible(h, false);
            Some(h)
        });
        // The post-init's click-map creation — the last thing it does before
        // clearing the slot checkbox's `0x0E` attribute.
        self.click_map = create_click_map(ui);

        self.slots_created = self.lists().map(|w| w.created).sum();
        self.last = None;
        self.last_tiles.clear();
    }

    /// The client's message-1 arm on the slot checkbox.
    ///
    /// It reads the checkbox's `0x0E` attribute. Unchecked shows the paper doll and the drag mask
    /// and hides the nine armour slots; checked does the reverse.
    ///
    /// The two halves are mirror images and the *unchecked* one is the panel's opening state, which
    /// the post-init reaches by the other route — hiding the nine outright. Both are here so that
    /// the initial state and the toggle cannot drift apart, which is the failure the nine separate
    /// visibility calls in the client invite.
    pub fn set_slot_view(&mut self, ui: &mut UiSystem, checked: bool) {
        self.slots_view = checked;
        for h in [self.paper_doll_viewport, self.doll_drag_mask]
            .into_iter()
            .flatten()
        {
            ui.set_visible(h, !checked);
        }
        for (_, w) in self
            .doll
            .iter()
            .filter(|(_, w)| ARMOUR_COVERAGE_SLOTS.contains(&w.element))
        {
            ui.set_visible(w.handle, checked);
        }
    }

    /// The paper doll's "item under mouse" answer, all of it except the closing
    /// upper-inventory-object read — which needs the player's inventory-placement list and
    /// therefore belongs to whoever holds the game state.
    ///
    /// With no drag mask or no click map the answer is `0`. Otherwise the point is made relative
    /// to the mask's screen origin, the click map's colour there is compared against the nine
    /// stored colours to choose the slot mask (`0` if outside the map), and the top-level
    /// inventory object for that mask is the answer.
    ///
    /// The point is the **live cursor**, read in the element-message handler's `0x1C` arm, not
    /// the point the message carries. `dereth_ui::UiSystem::mouse_pos` is that pair.
    ///
    /// **It is deliberately not gated on [`Self::slots_view`].** The client has no such gate
    /// either: `set_slot_view` hides the mask, and the UI refuses a
    /// hidden element before any message is raised, so slot mode never reaches this function.
    /// Answering `0` when the mask is hidden would be a *second* gate saying the same thing, and
    /// the two could drift.
    #[must_use]
    pub fn paper_doll_region_under_mouse(&self, ui: &UiSystem, x: i32, y: i32) -> u32 {
        let (Some(mask), Some(map)) = (self.doll_drag_mask, self.click_map.as_ref()) else {
            return 0;
        };
        let (x0, y0) = ui.screen_origin(mask);
        map.mask_at(x - x0, y - y0)
    }

    /// The drag-mask check the paper doll's element-message handler makes before it does anything.
    #[must_use]
    pub fn is_doll_drag_mask(&self, h: ElemHandle) -> bool {
        self.doll_drag_mask == Some(h)
    }

    /// The paper doll's upper-inventory-object read, answered over the twenty-four doll widgets
    /// rather than over the player's inventory-placement list.
    ///
    /// A zero location answers `0`. Otherwise it starts from an empty best entry and walks the
    /// placement list, and every placement whose location overlaps the mask competes with the best
    /// so far on priority.
    ///
    /// Each doll widget **is** one location and holds at most one object, so the placement list
    /// and the widget list are the same list seen twice — [`Self::update`]'s doll pass fills each
    /// widget from the placement whose location its mask selects. Walking the widgets is therefore
    /// the same walk, and it hands back the index this crate needs to paint the drag icon.
    ///
    /// # The placement priority this seam does not carry, stated rather than hidden
    ///
    /// The priority comparison uses each placement's priority, and
    /// [`crate::view::GameView::equipment`] is `(iid, loc)` — the third field never crosses the
    /// seam. What *is* reproduced is the rule that decides every case the click map can produce:
    /// a list entry with zero priority whose overlap with the mask lies in `0x200..=0x4000` is
    /// promoted to `0x7F`; if the current best has zero priority and a non-zero overlap outside
    /// that band, the list entry replaces it outright, and a best inside the band is promoted to
    /// `0x7F` too; otherwise the higher priority wins, a tie going to the list entry —
    /// with every priority zero. Seven of the click map's nine colours are a **pair** of
    /// `INVENTORY_LOC` bits — the clothing bit and the armour bit of one body region — so the only
    /// contest the figure can ever stage is *clothing against armour over the same colour*, and
    /// the `0x7F` promotion inside `0x200..=0x4000` settles it: the breastplate wins the chest
    /// colour over the shirt. A shard that sent a non-zero priority could in principle order
    /// two items **inside** one of those bands differently, and no doll widget's mask overlaps
    /// another's, so that case cannot arise here either. Carrying the priority through
    /// `GameView::equipment` is the fix if one ever does.
    ///
    /// `None` is the client's "nothing covers that colour", which the host answers with the
    /// player's own id for a *selection* — and which is
    /// correctly nothing at all for a drag, because a player is not an item you can pick up.
    #[must_use]
    pub fn upper_inv_slot(&self, loc_mask: u32) -> Option<usize> {
        if loc_mask == 0 {
            return None;
        }
        let mut best: Option<(usize, u32)> = None;
        for (i, (mask, w)) in self.doll.iter().enumerate() {
            let covered = mask & loc_mask;
            if covered == 0 || w.item_at(0).is_none() {
                continue;
            }
            let priority = u32::from(covered > 0x1FF && covered < 0x4001) * 0x7F;
            // The candidate wins a tie, so the last equal entry stands.
            if best.is_none_or(|(_, best_priority)| best_priority <= priority) {
                best = Some((i, priority));
            }
        }
        best.map(|(i, _)| i)
    }

    /// The client's **`0x21` arm** — the drag the manager
    /// refused is the drag the paper doll starts for itself.
    ///
    /// On message `0x21` from the drag mask `0x100001d6`, if there is an item under the message's
    /// point and its drag icon can be prepared, retail starts a drag of the drag icon grabbed at
    /// `(0x10, 0x10)` and raises the begin-drag notice for the item.
    ///
    /// Without this arm paper-doll items can be selected but not dragged. The drag mask
    /// `0x100001D6` declares no `0x3A` \[measured\], so drag-catcher lookup walks from it to the
    /// root, finds nothing draggable, and raises `0x21` on the mask — and
    /// [`crate::items::widget::begin_drag_from_rejected`] answers `None` for it, because the mask
    /// is not a slot of any item list.
    ///
    /// **The point is the message's window point, not the live cursor.** That is the one place
    /// this arm differs from the `0x1C` arm, which reads the live cursor; here the message's own
    /// window point is passed straight into the item-under-mouse lookup, and it is the **press**
    /// point the drag start recorded rather than where the pointer has since travelled. Kept as
    /// retail has it, not unified.
    ///
    /// Returns what the begin-drag notice's three arguments would have been.
    pub fn begin_paper_doll_drag(
        &mut self,
        ui: &mut UiSystem,
        source: ElemHandle,
        x: i32,
        y: i32,
    ) -> Option<PaperDollDrag> {
        // The message must come from the drag mask, `0x100001d6`.
        if !self.is_doll_drag_mask(source) {
            return None;
        }
        let mask = self.paper_doll_region_under_mouse(ui, x, y);
        let slot = self.upper_inv_slot(mask)?;
        let item = self.doll.get(slot).and_then(|(_, w)| w.item_at(0))?;
        // No drag if the drag icon cannot be prepared.
        let icon = self.prepare_doll_drag_icon(ui, slot, item)?;
        // Start the drag of the drag icon, grabbed at `(0x10, 0x10)`.
        if !ui.start_drag_and_drop_at(
            icon,
            crate::items::widget::DRAG_GRAB.0,
            crate::items::widget::DRAG_GRAB.1,
        ) {
            return None;
        }
        Some(PaperDollDrag {
            item,
            mask,
            drag_icon: icon,
        })
    }

    /// The paper doll panel's drag-icon preparation.
    ///
    /// A zero item fails. Otherwise the drag icon's image is cleared; if the item's object is
    /// known, the icon gets blit mode `Alpha3` and the object's drag-icon picture, and the item
    /// counts as a container when its `PublicWeenieDesc` bitfield has `0x800000` or it has an
    /// items or containers capacity. Then the icon's attributes `0x1000000F` (item id) and
    /// `0x10000011` (is-container) are set, and it succeeds.
    ///
    /// **Two properties, not five.** A list-based drag icon writes three
    /// more — `0x10000012` shortcut, `0x10000013` vendor, `0x10000014` salvage — because it has a
    /// list whose flavour to report. The doll has none, and a drag off the figure is therefore an
    /// ordinary inventory move: the client's `(flags & 0xE) == 0`.
    ///
    /// The picture is the source slot's own icon rather than a second composite: this build's
    /// drag-icon picture is
    /// [`IconRecipe::drag_surface`](dereth_ui::region::IconRecipe::drag_surface) — the drag icon,
    /// the same recipe without the item-type tile and the custom underlay — and it is already on
    /// the doll widget's slot; this reads it back rather than deciding again.
    fn prepare_doll_drag_icon(
        &mut self,
        ui: &mut UiSystem,
        slot: usize,
        item: ObjectId,
    ) -> Option<ElemHandle> {
        let d = self.doll_drag_icon?;
        let s = self.doll.get(slot).and_then(|(_, w)| w.slots.first())?;
        let is_container = s.is_container || s.is_container_holder || s.items_capacity != 0;
        let mut image = s
            .icon
            .and_then(|h| ui.node(h).and_then(|n| n.region.image.clone()));
        if let Some(g) = image.as_mut() {
            if let Some(dereth_ui::region::SurfaceOp::Icon(r)) = g.op {
                let drag = r.drag_surface();
                g.op = Some(dereth_ui::region::SurfaceOp::Icon(drag));
                g.did = drag.base().unwrap_or(g.did);
            }
        }
        if let Some(n) = ui.node_mut(d) {
            n.region.image = image;
            n.region.blit_mode = dereth_ui::region::BlitMode::Alpha3;
        }
        crate::items::widget::set_attr(
            ui,
            d,
            crate::items::widget::drag_attr::ITEM_ID,
            dereth_assets::ui::PropertyValue::InstanceId(item.0),
        );
        crate::items::widget::set_attr(
            ui,
            d,
            crate::items::widget::drag_attr::IS_CONTAINER,
            dereth_assets::ui::PropertyValue::Bool(is_container),
        );
        Some(d)
    }

    /// The checkbox's own `0x0E` state, read the way the client reads it, then
    /// [`Self::set_slot_view`]. Returns false when this page has no such checkbox.
    pub fn on_slot_checkbox(&mut self, ui: &mut UiSystem, h: ElemHandle) -> bool {
        if self.slot_checkbox != Some(h) {
            return false;
        }
        let checked = crate::bind::attr_bool(ui, h, attr::CHECKED).unwrap_or(false);
        self.set_slot_view(ui, checked);
        true
    }

    /// The client's first act — the title bar.
    ///
    /// Returns true when the text changed. The guard is the client's own arrangement: the notice
    /// fires once per player description, not once per frame.
    pub fn set_title(&mut self, ui: &mut UiSystem, name: Option<&str>) -> bool {
        let want = name.map(title_text);
        if self.title_shown == want {
            return false;
        }
        if let Some(t) = self.title.and_then(|h| ui.text_element_mut(h)) {
            t.set_text(want.as_deref().unwrap_or(""));
        }
        self.title_shown = want;
        true
    }

    /// The heading over the grid.
    ///
    /// `None` is the player's own pack. Returns true when the text changed.
    pub fn set_contents_title(&mut self, ui: &mut UiSystem, container_name: Option<&str>) -> bool {
        let want = contents_text(container_name);
        if self.contents_shown.as_deref() == Some(want.as_str()) {
            return false;
        }
        if let Some(t) = self.contents.and_then(|h| ui.text_element_mut(h)) {
            t.set_text(&want);
        }
        self.contents_shown = Some(want);
        true
    }

    /// The backpack panel's load-level write, applied to the two live elements.
    ///
    /// The meter's fill is written as attribute `0x69` and **nothing here scales a box**: the
    /// meter narrows the *clip* of its id-2 child image instead, so a scaled implementation
    /// draws the vials empty and looks right only at 1.0. See
    /// [`dereth_ui::widgets::meter::Meter::child_clip`].
    ///
    /// Returns true when the value changed.
    pub fn set_load_level(&mut self, ui: &mut UiSystem, load: f32) -> bool {
        if self
            .last_load
            .is_some_and(|l| l.to_bits() == load.to_bits())
        {
            return false;
        }
        self.last_load = Some(load);
        let (position, percent) = load_level(load);
        self.burden = Some((position, percent));
        if let Some(h) = self.burden_meter {
            set_attr_float(ui, h, attr::METER_LEVEL, position);
        }
        if let Some(t) = self.burden_text.and_then(|h| ui.text_element_mut(h)) {
            t.set_text(&load_text(percent));
        }
        true
    }

    /// The same four in the same order, mutably — what the two per-frame passes
    /// each item list runs for itself in the client need.
    pub fn lists_mut(&mut self) -> impl Iterator<Item = &mut ItemListWidget> {
        self.top_container
            .iter_mut()
            .chain(self.container_list.iter_mut())
            .chain(self.item_list.iter_mut())
            .chain(self.doll.iter_mut().map(|(_, w)| w))
    }

    fn lists(&self) -> impl Iterator<Item = &ItemListWidget> {
        self.top_container
            .iter()
            .chain(self.container_list.iter())
            .chain(self.item_list.iter())
            .chain(self.doll.iter().map(|(_, w)| w))
    }

    /// The inventory panel's set-display-inventory notice +
    /// the paper doll's inventory remake, run against the current world.
    ///
    /// Returns true when something actually changed on screen.
    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        // The backpack panel's load-changed notice and
        // its player-description-received notice are a **separate** pair of notices from the
        // three the item lists listen to, so the burden is deliberately outside the item snapshot
        // below: a load that changes while nothing moved in the pack still redraws the meter, and
        // a pack that is rearranged at the same burden does not.
        let burden_changed = self.set_load_level(ui, view.load().unwrap_or(0.0));
        let player = view.player();

        // A rebuilt interface resumes the shared selection; the local field only caches its draw.
        let open = view.open_inventory_container();
        self.open_container = open;

        // The title and the heading are driven by two *more* notices —
        // the player-description-received notice and the new-parent-container notice.
        // Like the burden, they sit outside the item snapshot below and guard on
        // their own value. The player's singular name is `GameView::name` on
        // the player; `character_name` is the same string when the weenie has arrived and the
        // `0x0013` qualities' `CHARACTER_NAME` when only that has, which is the one place this
        // rebuild's seam is wider than the client's — this view can be fed by both.
        let name = player
            .and_then(|p| view.name(p))
            .or_else(|| view.character_name());
        let title_changed = self.set_title(ui, name);
        // The is-it-the-player fork. `open` is the open item, which
        // opening a container sets and which starts as the player himself.
        let container_name = match open {
            Some(c) if Some(c) != player => view.name(c),
            _ => None,
        };
        let heading_changed = self.set_contents_title(ui, container_name);
        let chrome_changed = burden_changed | title_changed | heading_changed;
        let mut items = open
            .map(|c| view.container_contents(c).to_vec())
            .unwrap_or_default();
        let mut containers = player
            .map(|p| view.contained_containers(p).to_vec())
            .unwrap_or_default();
        // **The item list element's insert-item step, the optimistic row.** Retail's lists hold
        // their own rows, so they simply insert one and the next refill takes it out again. This
        // panel *is* the refill, so the row arrives as part of what it fills from and is spliced in
        // here — **before** the snapshot and the tile read below, so that arming or clearing a
        // pending row is itself a frame that rebuilds, and so the tile the splice adds gets its
        // `TileInfo` like any other.
        //
        // The two lists are the two (parent container, is-container-list) pairs this panel owns:
        // the grid shows `open` and is an item list, the side-pack strip shows the player and is a
        // container list. `insert` clamps because retail's insert index is already
        // clamped to the number of existing rows.
        if let Some((item, at)) = view.pending_row(open, false) {
            let at = (at as usize).min(items.len());
            items.insert(at, item);
        }
        if let Some((item, at)) = view.pending_row(player, true) {
            let at = (at as usize).min(containers.len());
            containers.insert(at, item);
        }
        let equipment = player
            .map(|p| view.equipment(p).to_vec())
            .unwrap_or_default();
        // **The item-attributes-changed half of the gate.**
        //
        // The four fields above are the *id lists*, and an id list alone cannot see the one edge
        // that matters most to an awarded item: `0x0022 Item_ServerSaysContainID` pre-places an id
        // whose object does not exist yet, so the slot is filled while
        // `GameView::icon` and `GameView::slot_decoration` both answer `None`; the item's own
        // `0xF745` lands a datagram later and **changes no id list at all**. Without the
        // decorations here the early-out below fires, [`ItemListWidget::decorate`] never runs
        // again for that slot, and the item sits in the pack for the rest of the session with its
        // name resolving and no picture. The recorded academy-exit award is exactly this case.
        //
        // In the client that refresh is a notice, not a diff: the object maintainer's
        // weenie-desc write -> the client weenie object's declare-valid ends in an
        // item-attributes-changed notice `(id, 1)` (and `(id, 2)` as well when any
        // of the icon update's five inputs moved), which reaches
        // *"For every UI item whose
        // item id is this id, run the item update"*. This build has no notice bus reaching the
        // panels, so the snapshot is the stand-in, and it
        // has to carry the fields the item update reads or it is a stand-in for nothing.
        //
        // **`SlotDecoration` alone is not that field set.** It is three of the four things
        // [`ItemListWidget::decorate`] consumes per slot; the other two are the name and the
        // plural name, which the `info` closure below reads off the view. An item renamed after
        // creation — a `0x0013` re-send, an `Item_SetAppraiseInfo`-driven rename, any descriptor
        // replacement that keeps the id — moves no id list either, so a guard without the names
        // would keep the old tooltip in the pack for the rest of the session. The quickbar has
        // the same guard for the same reason.
        //
        // The compared value is therefore a whole [`TileInfo`] per slot, held in
        // [`Self::last_tiles`] rather than in [`Snapshot`] so that a still frame allocates no
        // `String` — see that field's own note.
        let tile_ids: Vec<ObjectId> = player
            .into_iter()
            .chain(containers.iter().copied())
            .chain(items.iter().copied())
            .chain(equipment.iter().map(|(id, _)| *id))
            .collect();
        // Asked without allocating: on a still frame this walks the slots and builds no `String`.
        //
        // **The length term is unfalsifiable on this host and is kept anyway**, for the same
        // reason the capacities are compared directly: `zip` stops at the
        // shorter side, so a `last_tiles` shorter than `tile_ids` would make `any` answer *false*
        // and the gate could close over a fill that never happened. It cannot happen today,
        // because every id in `tile_ids` comes from a `Vec` that is itself in `snap` — so an id
        // count that moved has already moved the struct compare — and because the two fields are
        // written and cleared together. That is containment through another field, not by
        // construction, so the term stays.
        let tiles_changed = self.last_tiles.len() != tile_ids.len()
            || tile_ids
                .iter()
                .zip(&self.last_tiles)
                .any(|(id, last)| !TileInfo::matches(view, *id, last.as_ref()));
        // **The remaining inputs the rebuild reads from outside the tiles.**
        //
        // Beyond the descriptors, the rebuild reads `item_waiting`, `items_capacity`,
        // `containers_capacity` and `cooldown_remaining`. The two capacities are ordinary inputs
        // and belong in the compared value; they are gathered here, before the early-out, so that
        // the guard is a superset of what the fill loop below reads. The cooldown is a different
        // kind of thing and is exempted rather than added — see the note above the `info` closure.
        //
        // `waiting` is inside [`SlotDecoration`], so it is covered by `tiles_changed` above: the
        // client keeps `waiting` on the object beside `selected`, the sell state and the trade
        // state and mirrors all four in one place (the item update's tail), and this panel does
        // the same. `tile_ids` covers the player, the side packs, the loose items **and** the
        // equipment.
        let items_capacity = open.and_then(|c| view.items_capacity(c));
        let containers_capacity = player.and_then(|p| view.containers_capacity(p));
        let snap = Snapshot {
            player,
            open,
            items,
            containers,
            equipment,
            items_capacity,
            containers_capacity,
        };
        if self.last.as_ref() == Some(&snap) && !tiles_changed {
            return chrome_changed;
        }
        // One read of the seam per slot, used by the `info` closure below and stored as the next
        // pass's memory — so the value the guard compared and the value the screen draws are one
        // read — the same shape the quickbar uses.
        let tiles: Vec<Option<TileInfo>> = tile_ids
            .iter()
            .map(|id| TileInfo::read(view, *id))
            .collect();
        let icon = |id: ObjectId| view.icon(id);

        // 1. The top container: one slot holding the player, i.e. the main pack.
        if let Some(w) = self.top_container.as_mut() {
            let ids: Vec<ObjectId> = player.into_iter().collect();
            w.set_contents(ui, player, None, &ids, &icon);
            w.open_item_id = open.filter(|id| w.is_in_list(*id));
        }
        // 2. The container list: the side packs, containers-capacity slots of them.
        //
        // The capacity comes out of the snapshot rather than off the view a second time: the
        // value the guard compared and the value the fill uses must be the same read,
        // or the two can disagree within one frame and the guard is a superset of nothing.
        if let Some(w) = self.container_list.as_mut() {
            w.set_contents(
                ui,
                player,
                snap.containers_capacity,
                &snap.containers,
                &icon,
            );
            w.open_item_id = open.filter(|id| w.is_in_list(*id));
        }
        // 3. The item list: the open container's loose items.
        if let Some(w) = self.item_list.as_mut() {
            let changed_container = w.parent_container != open;
            self.slots_filled = w.set_contents(ui, open, snap.items_capacity, &snap.items, &icon);
            // The item list's element-message handler and the open-first-container path
            // reset the child list only when its container changes.
            if changed_container && !w.slots.is_empty() {
                w.scroll_to_show(ui, 0);
            }
        }
        // 4. The doll: clear every slot, then place one item per placement.
        for (mask, w) in &mut self.doll {
            let mut held: Vec<ObjectId> = Vec::new();
            for (iid, loc) in &snap.equipment {
                if loc & *mask != 0 {
                    held.clear();
                    held.push(*iid);
                }
            }
            w.set_contents(ui, player, None, &held, &icon);
        }
        // **There is no separate ghost pass.** The client has none: `waiting` is mirrored exactly
        // where `selected`, the sell state and the trade state are — the tail of the item update —
        // which the item-list insert runs for every slot of every list. That is the decorate pass
        // below, and it carries `waiting` with the other three. A pass over only the item list and
        // the doll would leave a pack ghosted in the container list or the top container grey
        // after the server-says-attempt-failed or server-says-move-item path cleared its flag.
        // See [`crate::items::widget::ItemListWidget::decorate`].
        //
        // Run the item-update tail on every slot of every list.
        //
        // Retail runs it per slot as the list fills; here the fill is a
        // batch, so the decoration is a second pass over the same slots and the observable result
        // is identical. Nothing in it can move an item — every one of the four display updates
        // and the tooltip update only reads the weenie.
        //
        // `now` is the current time, read before the borrow because the cooldown
        // display asks the registry at draw time and not from the
        // snapshot — see [`GameView::cooldown_remaining`].
        //
        // **`cooldown_remaining` is the one input above that a snapshot CANNOT contain, and this
        // is why.** Every other value the gate compares is a fact about a frozen
        // world: it changes when a datagram lands and is otherwise constant, so "the snapshot did
        // not move" and "nothing happened" are the same statement. A cooldown is a function of the
        // **clock**. It takes a different value on every frame of its own accord, with no datagram
        // and no world change behind it. Putting it in the compared value would therefore make the
        // early-out fire **never** — every frame would rebuild all four lists, flush and refill
        // every slot and re-decorate them, for as long as any item in the pack was on cooldown.
        // That is a worse defect than the one it would be fixing: a guard that cannot fire is not
        // a guard, and the flush would take the drag ghost and the selection ring with it every
        // frame. The same is true of anything else continuously varying that a panel might come to
        // draw — an animation phase, a spell timer, a hover fade.
        //
        // So the wedge is driven **outside the gate**, by the mechanism the client uses for
        // exactly this and for nothing else: an item heartbeat that does nothing but update the
        // cooldown display, once a second off global message 3. This build's copy is
        // [`ItemListWidget::do_heartbeat`], reached from `GamePlayScreen::do_item_heartbeat`,
        // which `Hud::drive` calls every frame deliberately outside both guarded passes. The read
        // below is not that mechanism and is not load-bearing for the countdown: it is the fresh
        // value the item update's own tail would have used on a frame that really did rebuild, so
        // that a slot refilled mid-cooldown draws the right wedge immediately rather than at the
        // next beat.
        let now = ui.now.0;
        let info = |id: ObjectId| -> Option<SlotInfo> {
            // **Out of the snapshot, not off the view a second time.** The three
            // frozen-world fields come from the same read the guard compared, so the value that
            // decided to rebuild and the value that reaches the screen cannot disagree within a
            // frame; only the clock-driven fourth is read fresh, because it has to be.
            let t = tile_ids
                .iter()
                .zip(&tiles)
                .find_map(|(tid, t)| (*tid == id).then_some(t.as_ref()))??;
            Some(t.to_slot_info(view, now))
        };
        let mut decorated = 0;
        if let Some(w) = self.top_container.as_mut() {
            decorated += w.decorate(ui, &info);
        }
        if let Some(w) = self.container_list.as_mut() {
            decorated += w.decorate(ui, &info);
        }
        if let Some(w) = self.item_list.as_mut() {
            decorated += w.decorate(ui, &info);
        }
        for (_, w) in &mut self.doll {
            decorated += w.decorate(ui, &info);
        }
        self.slots_decorated = decorated;

        // The item list element's open container indicator update.
        //
        // The client reaches it from the flush (with 0) and from setting the open item
        // (with the new id); this pass has just flushed and
        // refilled every list, so both are the same call with the open item — which for the
        // backpack is the shared open-container selection.
        //
        // It is run over **every** list because the client does, even though only the two
        // `UI_ItemList_IsContainer` strips have an open-container frame to show.
        self.open_frames = 0;
        for w in self
            .top_container
            .iter_mut()
            .chain(self.container_list.iter_mut())
        {
            self.open_frames += w.update_open_container_indicator(ui, open);
        }
        if let Some(w) = self.item_list.as_mut() {
            w.update_open_container_indicator(ui, open);
        }

        self.slots_created = self.lists().map(|w| w.created).sum();
        self.last_tiles = tiles;
        self.last = Some(snap);
        true
    }

    /// Point the grid at a different
    /// pack. Returns false when the id is not one of the containers on show, which is the client's
    /// own is-in-list guard, and false when it is already open, which is the
    /// already-open early-out.
    pub fn open_container(
        &mut self,
        requests_out: &mut crate::requests::Outbox,
        id: ObjectId,
    ) -> bool {
        let known = self
            .top_container
            .iter()
            .chain(self.container_list.iter())
            .any(|w| w.slots.iter().any(|s| s.item == Some(id)));
        if !known {
            return false;
        }
        if self.open_container == Some(id) {
            return false;
        }
        self.open_container = Some(id);
        self.last = None;
        self.last_tiles.clear();
        // The item list's set-parent-container tail raises the new-parent-container notice
        // **only** on a real change,
        // which is what the two early-outs above are.
        requests_out.emit(UiRequest::NewParentContainer(id));
        true
    }

    /// A container-strip click selects its item and requests that shared inventory parent.
    pub fn on_slot_clicked(
        &mut self,
        requests_out: &mut crate::requests::Outbox,
        h: ElemHandle,
    ) -> Option<ObjectId> {
        let hit_on = |w: &Option<ItemListWidget>| {
            w.as_ref()
                .filter(|w| w.container_list)
                .and_then(|w| w.slot_of(h).and_then(|i| w.item_at(i)))
        };
        let (hit, clicked, left) = if let Some(id) = hit_on(&self.top_container) {
            (id, &mut self.top_container, &mut self.container_list)
        } else {
            let id = hit_on(&self.container_list)?;
            (id, &mut self.container_list, &mut self.top_container)
        };
        if let Some(w) = left.as_mut() {
            w.open_item_id = None;
        }
        if let Some(w) = clicked.as_mut() {
            w.open_item_id = Some(hit);
        }
        self.open_container(requests_out, hit).then_some(hit)
    }

    /// The object under an element handle, and where a drop on it lands.
    ///
    /// **This is the map from drop target to container**: `DropTarget::ItemList` names a list
    /// element and a slot index, and only the inventory panel knows which container object that
    /// list is showing. Without it a drop is counted and dropped.
    #[must_use]
    pub fn resolve_drop(&self, target: DropTarget) -> Option<ObjectId> {
        match target {
            DropTarget::ItemList { list, slot } => {
                let w = self.lists().find(|w| w.element == list)?;
                // A drop on an occupied slot is a drop **on that object** (a merge or a
                // put-in-container); a drop on an empty slot is a drop on the list's own
                // container.
                let slot = usize::try_from(slot).ok()?;
                w.item_at(slot).or(w.parent_container)
            }
            DropTarget::EquipSlot(id) => self
                .doll
                .iter()
                .find(|(_, w)| w.element == id)
                .and_then(|(_, w)| w.item_at(0)),
            DropTarget::Container(id) => Some(id),
            // The destination of an `ItemListSlot` is not a single object — the
            // object under the pointer may be a *position* rather than a container, and which it
            // is needs the items capacity, which this crate may not read. `dereth_client_model`'s
            // item_list_accept_drag decides; this method deliberately declines rather than
            // guessing, so that a caller that wants a single object cannot silently get a wrong
            // one.
            DropTarget::ItemListSlot { .. } => None,
            DropTarget::BackpackButton
            | DropTarget::ShortcutSlot(_)
            | DropTarget::ShortcutAlias { .. }
            | DropTarget::World => None,
        }
    }

    /// The client's four element-tree reads, gathered off
    /// the list the drop landed on.
    ///
    /// This replaces the `ItemList` arm of [`Self::resolve_drop`] on the live drop path. That
    /// method answers a single `ObjectId` — the object in the slot, or the list's own container —
    /// and **the slot index does not survive it**, so every drag into a pack would be sent with
    /// `place = 0` (the head) and a drop onto a plain item would become a `PutItemInContainer`
    /// request of the item into that item at place 0, which no server answers.
    ///
    /// The three decisions that need a weenie — is the object under the pointer a container with
    /// room (items / containers capacity), where does the dragged object sit in the
    /// destination's list, and is it on the trade window — stay
    /// on the far side of the seam in `item_list_accept_drag`. This crate is not
    /// allowed to read the object table and does not need to: the "what num" and
    /// "number of UI items" reads are questions about the element tree and nothing else.
    ///
    /// A target that is not one of this panel's lists is returned unchanged.
    #[must_use]
    pub fn resolve_item_list_drop(
        &self,
        target: DropTarget,
        dragged_is_container: bool,
    ) -> DropTarget {
        let DropTarget::ItemList { list, slot } = target else {
            return target;
        };
        let Some(w) = self.lists().find(|w| w.element == list) else {
            return target;
        };
        let Some(container) = w.parent_container else {
            return target;
        };
        let index = slot;
        DropTarget::ItemListSlot {
            container,
            // Slot lookup answers the item-slot element whether or not
            // it holds anything; the item id is 0 on an empty one and every arm of the drop
            // acceptance that reads it guards on there being no object for id 0.
            under: usize::try_from(slot).ok().and_then(|i| w.item_at(i)),
            index,
            num_ui_items: u32::try_from(w.num_ui_items()).unwrap_or(u32::MAX),
            dragged_is_container,
            container_list: w.container_list,
        }
    }

    /// The is-container flag on whichever slot owns this handle — the field the drag start
    /// copies into the drag proxy's `0x10000011`.
    ///
    /// Used only as the fallback when the handle carries no instance properties of its own, which
    /// is the case for a direct call rather than a real drag. Returns `false` for a handle none of
    /// this panel's lists knows.
    #[must_use]
    pub fn slot_is_container(&self, ui: &UiSystem, h: ElemHandle) -> bool {
        // The same one-step walk `GamePlayScreen::locate_drag_owner` makes: a real drag's owner is
        // the slot's drag icon, a direct call's is the slot itself.
        let mut cur = Some(h);
        while let Some(c) = cur {
            for w in self.lists() {
                if let Some(i) = w.slot_of(c) {
                    return w.slots.get(i).is_some_and(|s| s.is_container);
                }
            }
            cur = ui.parent(c);
        }
        false
    }

    /// The `(list element, slot index, object)` a live element handle belongs to, for turning a
    /// real click or drop back into a [`DropTarget`].
    ///
    /// The handle may be **either** a slot's item element or the list itself: only the list
    /// carries attribute `0x36` in the shipped data, so catcher lookup walks up
    /// to it and the drop target this screen is handed is a list, not a slot.
    /// The client recovers the slot from the pointer position; here it
    /// is the handle when the caller has one and slot 0 when it does not.
    #[must_use]
    pub fn locate(&self, h: ElemHandle) -> Option<(ElementId, u32, Option<ObjectId>)> {
        for w in self.lists() {
            if let Some(i) = w.slot_of(h) {
                return Some((w.element, u32::try_from(i).unwrap_or(0), w.item_at(i)));
            }
            if w.handle == h {
                return Some((w.element, 0, None));
            }
        }
        None
    }

    /// The item list element's element-message handler's single-selection step, run only when
    /// the list is single-selection — on the list the
    /// click landed in, and on no other.
    ///
    /// The gate is folded in here rather than left at the call site because only this panel knows
    /// *which* of its twenty-seven lists the handle belongs to, and single-selection is a
    /// per-list attribute. `None` means the handle is in none of them (the caller then tries the
    /// quickbar); `Some(0)` means the list was found and either is not single-selection — which is
    /// every list this panel owns, in retail as here — or held no duplicate.
    pub fn handle_single_selection(&mut self, ui: &mut UiSystem, h: ElemHandle) -> Option<usize> {
        for w in self.lists_mut() {
            if let Some(i) = w.slot_of(h) {
                return Some(if w.single_selection {
                    w.handle_single_selection(ui, i)
                } else {
                    0
                });
            }
        }
        None
    }

    /// Where a drop on this element **lands** — the one producer of [`DropTarget::EquipSlot`].
    ///
    /// [`Self::locate`] answers every list it recognises the same way, a `(list, slot, object)`
    /// triple, which its callers turn into a [`DropTarget::ItemList`]. But `lists()` chains the
    /// twenty-four paper-doll widgets in with the grid and the two container strips, so a drag
    /// released on a doll slot and resolved through the `ItemList` arm would name `item_at(slot)`
    /// or the list's `parent_container` — the item already worn there, or the **player** — and
    /// become a `PutItemInContainer` request, putting the item into a pack instead of onto the
    /// body, silently.
    ///
    /// The client makes the same split one level up and by a different route: the doll's slots are
    /// children of the paper-doll panel, whose drop-release handler looks up the location of the
    /// element under the cursor. A location takes the slot-equip path; without one,
    /// only the `0x100001D6` drag mask takes the automatic-wear path.
    /// This screen has one flat map of item lists rather than two panels, so the same fork is the
    /// same question asked of [`Self::location_of_slot`].
    ///
    /// # The canvas
    ///
    /// The drop-release handler's no-location arm compares the target's own element id
    /// with `0x100001D6` before accepting the paper-doll drop. Other ids fall through.
    ///
    /// So the *only* non-slot element of the whole panel that accepts a drop is
    /// [`PAPER_DOLL_DRAG_MASK`]; the viewport `0x100001D5`, the overlay `0x1000046D`, the *Slots*
    /// checkbox and the panel background all fall to clearing the waiting state. This answers with
    /// the mask's **own** element id, exactly as the client hands the same element id down both
    /// arms and lets the location lookup on the far side pick which one runs.
    ///
    /// It is deliberately **not** gated on [`Self::slots_view`]: the client has no such gate
    /// either, because `set_slot_view` hides the mask and
    /// returns `None` for a hidden element before a drop target is ever named.
    ///
    /// **The consumer of this answer is the paper-doll drop acceptance,**
    /// `dereth_client::interaction::Interaction::accept_paper_doll_drag_object`: an item whose
    /// valid locations `& 0x08007FFF == 0` speaks [`CANNOT_PUT_THAT_ITEM_THERE`] and refuses;
    /// everything else attempts automatic wear **loudly** (`quiet = 0`) with the item's whole
    /// mask. The hint [`InventoryPanels::on_paper_doll_canvas_message`] paints is the same
    /// predicate asked quietly, one gesture earlier.
    ///
    /// Returns `None` for an element that is none of this page's lists, which is the caller's
    /// is-ancestor guard.
    #[must_use]
    pub fn drop_target(&self, h: ElemHandle) -> Option<DropTarget> {
        if self.doll_drag_mask == Some(h) {
            return Some(DropTarget::EquipSlot(PAPER_DOLL_DRAG_MASK));
        }
        let (list, slot, _) = self.locate(h)?;
        if Self::location_of_slot(list).is_some() {
            return Some(DropTarget::EquipSlot(list));
        }
        Some(DropTarget::ItemList { list, slot })
    }

    /// Put whichever list holds `item` into its waiting state.
    ///
    /// The icon is ghosted and nothing else happens: the item stays in its slot until
    /// `Item_ServerSaysMoveItem` says otherwise, and there is **no timeout** on the wait.
    pub fn ghost_item(&mut self, ui: &mut UiSystem, item: ObjectId) -> bool {
        let mut any = false;
        if let Some(w) = self.item_list.as_mut() {
            any |= w.ghost(ui, item);
        }
        if let Some(w) = self.container_list.as_mut() {
            any |= w.ghost(ui, item);
        }
        for (_, w) in &mut self.doll {
            any |= w.ghost(ui, item);
        }
        any
    }

    /// The `INVENTORY_LOC` mask a paper-doll slot element stands for —
    /// the paper doll panel's element-id-to-location lookup.
    #[must_use]
    pub fn location_of_slot(element: ElementId) -> Option<u32> {
        PAPER_DOLL_SLOTS
            .iter()
            .find(|(id, _)| *id == element.0)
            .map(|(_, m)| *m)
    }

    /// Which of the twenty-four doll lists holds the tile `h`, if any.
    ///
    /// The doll's lists each carry the paper-doll drag handler
    /// registered during post-initialization, so the item-list drag over
    /// never reaches its default for them; [`Self::on_paper_doll_drag_over`] is that handler and
    /// this is how the screen's default arm knows to step aside.
    #[must_use]
    pub fn doll_slot_of(&self, h: ElemHandle) -> Option<usize> {
        self.doll.iter().position(|(_, w)| w.slot_of(h).is_some())
    }

    /// The doll's item-list drag handler, asked
    /// first by the list's drag-over and **always answering true**, so the default
    /// three-way hint never runs on a doll slot. Retail shows the red hint over a slot that cannot
    /// accept the item; the default's not-a-container-list accept arm would show the green one
    /// for any item.
    ///
    /// Retail does nothing for a zero item, an alias drag, an unknown object (no state at all) or
    /// a missing player system. It looks up the slot's location; for the shield slot (`0x200000`) an item
    /// whose valid locations include a one-handed weapon (`0x100000`) also counts as valid there.
    /// If the location and the valid locations do not overlap it refuses (`0x10000041`);
    /// otherwise it asks quietly whether auto-wield is legal and accepts (`0x10000040`) or
    /// refuses.
    ///
    /// Two things the default arm could not have known and this one must:
    ///
    /// * **Auto-wield legality is asked, quietly.** The mask test alone would accept a
    ///   torch in the weapon slot while in combat, and retail refuses it. That
    ///   check needs the ready slot, the combat mode and the weenie table, which is why this
    ///   handler takes a [`GameView`] and is delivered one hop later than the screen's own arm,
    ///   the same one-frame hop `panel_messages` gives the vendor's.
    /// * **An object the client does not know writes no state at all** — not a refusal —
    ///   which is what [`GameView::item_valid_locations`]'s `None` stands for.
    ///
    /// The location comes off the widget's own mask rather than a second
    /// element-id-to-location lookup: each doll widget **is** one entry of
    /// [`PAPER_DOLL_SLOTS`], taken from that lookup.
    ///
    /// Returns true when the message was a doll tile's `0x3E`.
    pub fn on_paper_doll_drag_over(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::ElementMessage,
        view: &dyn GameView,
    ) -> bool {
        use crate::items::widget::{drag_accept_state, inq_drop_icon_info};
        // The figure's own overlay, which is the *same* `0x3E` fan-out and one
        // arm earlier in than the slot lists'.
        // Delivered from here rather than through a second line in `screens/gameplay.rs` because
        // retail's two arms are in one function and both already arrive here: the host hands this
        // method *every* panel message (`hud.rs`'s two `screen.on_paper_doll_drag_over` calls), so
        // the `0x15` reaches it too.
        if self.on_paper_doll_canvas_message(ui, m, view) {
            return true;
        }
        if m.id != dereth_ui::msg::element::id::DRAG_CURSOR_OVER {
            return false;
        }
        let Some(i) = self.doll_slot_of(m.source) else {
            return false;
        };
        let (location, w) = &mut self.doll[i];
        let location = *location;
        let Some(slot) = w.slot_of(m.source) else {
            return false;
        };
        // The client's two clearing routes, which act
        // on the tile before the list — and so the handler — is asked.
        let proxy = ui.drag_state().element.filter(|_| m.p1 != 0);
        let Some(proxy) = proxy else {
            w.slots[slot].set_drag_accept_state(ui, drag_accept_state::NONE);
            return true;
        };
        let info = inq_drop_icon_info(ui, proxy);
        let Some(item) = info.item.filter(|_| info.is_inventory_move()) else {
            return true;
        };
        let Some(mut valid) = view.item_valid_locations(item) else {
            return true;
        };
        if location == crate::panels::inventory::paper_doll::SHIELD_LOC
            && valid & crate::panels::inventory::paper_doll::MELEE_WEAPON_LOC != 0
        {
            valid |= crate::panels::inventory::paper_doll::SHIELD_LOC;
        }
        let s = if location & valid != 0 && view.auto_wield_is_legal(item) {
            drag_accept_state::ACCEPT
        } else {
            drag_accept_state::REFUSE
        };
        w.slots[slot].set_drag_accept_state(ui, s);
        true
    }

    /// The client's **`0x3E` and `0x15` arms on the drag
    /// mask `0x100001D6`**, and behind the first of them —
    /// the figure's whole-body drop-accept overlay.
    ///
    /// The full description of all five state writes and of the predicate is on
    /// [`PAPER_DOLL_DRAG_OVERLAY`]. In short:
    ///
    /// * `0x3E` with first parameter `0` (leaving), or with no drag element
    ///   -> [`paper_doll_drag_overlay::DOWN`];
    /// * `0x3E` carrying a proxy -> read its drop info, then no item or an alias flag
    ///   (`flags & 0x0E`) writes **nothing**, auto-wear legal writes
    ///   [`paper_doll_drag_overlay::ACCEPT`], false-and-not-already-worn writes
    ///   [`paper_doll_drag_overlay::REFUSE`], and false-because-already-worn writes **nothing**;
    /// * `0x15` on the mask -> [`paper_doll_drag_overlay::DOWN`], before the doll's drop
    ///   handling runs.
    ///
    /// **The message is keyed on the mask and on nothing else** (both arms compare the source
    /// against `0x100001d6`), which is the same element [`Self::drop_target`] already answers `EquipSlot` for and
    /// the only non-slot child of the panel that accepts a drop at all. A `0x3E` on
    /// a doll *slot* falls through to [`Self::on_paper_doll_drag_over`]'s own handler.
    ///
    /// Returns true when the message was the mask's `0x3E` or `0x15`. **The `0x15` still returns
    /// true here and is still delivered to the drop path**, because the host calls this method
    /// separately from `RemainingPanels::on_element_message` and the two do not consume each
    /// other — exactly as the paper-doll panel clears the overlay and *then* handles the drop.
    pub fn on_paper_doll_canvas_message(
        &mut self,
        ui: &mut UiSystem,
        m: &dereth_ui::ElementMessage,
        view: &dyn GameView,
    ) -> bool {
        use crate::items::widget::inq_drop_icon_info;
        if self.doll_drag_mask != Some(m.source) {
            return false;
        }
        let Some(overlay) = self.doll_drag_overlay else {
            // The post-init leaves the field null when the layout has no such child, and every
            // state write in the client uses it unguarded; answering "handled" here
            // would be a lie about an element that does not exist.
            return false;
        };
        // The drop. Clear first, then let the message go on to the drop-release handling.
        if m.id == dereth_ui::msg::element::id::DROP_FAILED {
            ui.set_state(overlay, paper_doll_drag_overlay::DOWN);
            return true;
        }
        if m.id != dereth_ui::msg::element::id::DRAG_CURSOR_OVER {
            return false;
        }
        // Leaving, or a `0x3E` raised while nothing is being dragged.
        let Some(proxy) = ui.drag_state().element.filter(|_| m.p1 != 0) else {
            ui.set_state(overlay, paper_doll_drag_overlay::DOWN);
            return true;
        };
        let info = inq_drop_icon_info(ui, proxy);
        // A spell, a shortcut, a vendor or a salvage proxy writes no
        // state at all — not a refusal.
        let Some(item) = info.item.filter(|_| info.is_inventory_move()) else {
            return true;
        };
        match view.auto_wear_is_legal(item) {
            Some(true) => ui.set_state(overlay, paper_doll_drag_overlay::ACCEPT),
            Some(false) => ui.set_state(overlay, paper_doll_drag_overlay::REFUSE),
            // Already worn: the piece is already on the figure. Leave the state unchanged.
            None => {}
        }
        true
    }
}

/// What the client's `0x21` arm picked up off the figure,
/// which is what its one notice carries.
///
/// The begin-drag notice is raised as `(item, 0, false)` — the spell id is a literal `0` (a body region
/// is never a spell) and the "not an inventory move" flag a literal `false`, so a drag off the
/// doll is an ordinary move. The notice itself is the client telling the rest of itself a drag
/// began, and none of the five windows that listen for it exists in this build; this is what they
/// would have been told. See [`crate::items::widget::DragStart`], which is the item list's
/// equivalent and carries three fields more because a list has a flavour to report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaperDollDrag {
    /// The begin-drag notice's item — the top-level inventory object lookup's answer.
    pub item: ObjectId,
    /// The `INVENTORY_LOC` pair the click map's colour resolved to, kept for the same reason
    /// [`UiRequest::PaperDollRegion`] carries it: it is what the host needs to say *where* the
    /// item came off.
    pub mask: u32,
    /// The element the drag was started with — the doll's drag icon. The live proxy is a copy of it.
    pub drag_icon: ElemHandle,
}

/// The paper doll panel's click-map creation.
///
/// Retail releases any old click map, resolves enum `(0x1000000C, 7)` to a DataID, loads it as a
/// render surface (none -> the panel simply has no click map), creates a local surface of the
/// record's width, height and format, and blits the record into it at scale 1.0.
///
/// The create-and-blit pair is a system-memory copy of a record of identical extent and format, so
/// this keeps the record's own pixels. Every failure arm the client has — no enum entry, no
/// record, a local-surface creation that fails — leaves no click map, and
/// the item-under-mouse lookup's first check then answers `0` for every point. `None` here is the
/// same state, reached the same way; the doll is simply not hit-testable, which is what a client
/// whose dats do not carry the map does.
fn create_click_map(ui: &UiSystem) -> Option<ClickMap> {
    use dereth_assets::Decode;
    let (group, value) = CLICK_MAP_ENUM;
    let did = ui
        .env()
        .cloned()
        .and_then(|e| e.did_by_enum(group, value))?;
    ui.env().cloned().and_then(|e| {
        e.with_assets(|assets| {
            let bytes = assets.read(did).ok()?;
            let s = <dereth_assets::RenderSurface as Decode>::decode_payload(did, &bytes).ok()?;
            if s.format != PFID_R8G8B8 {
                return None;
            }
            let (w, h) = (i32::try_from(s.width).ok()?, i32::try_from(s.height).ok()?);
            let want = (s.width as usize)
                .checked_mul(s.height as usize)?
                .checked_mul(3)?;
            let payload = s.payload(&bytes)?;
            if payload.len() < want {
                return None;
            }
            Some(ClickMap {
                did,
                width: w,
                height: h,
                bgr: payload[..want].to_vec(),
            })
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the client's element-id-to-location lookup and its two
    /// `switch`es, cross-checked against the recovered toolbar and panel behavior's slot table and against
    /// `dereth_client_model::inventory::slots::loc`.
    #[test]
    fn the_paper_doll_slot_masks_are_the_ones_the_client_switches_on() {
        assert_eq!(PAPER_DOLL_SLOTS.len(), 24);
        // The three that are not a single bit or an obvious one.
        assert_eq!(
            InventoryPanels::location_of_slot(ElementId(0x1000_01DF)),
            Some(0x0350_0000)
        );
        assert_eq!(
            InventoryPanels::location_of_slot(ElementId(0x1000_01DA)),
            Some(0x0000_8000)
        );
        assert_eq!(
            InventoryPanels::location_of_slot(ElementId(0x1000_05B3)),
            Some(0x0000_0100)
        );
        // Shirt (CLOTHING chest) and chest armour are different slots with different bits, which
        // is the pair the "show armour slots" checkbox switches between.
        assert_eq!(
            InventoryPanels::location_of_slot(ElementId(0x1000_01E2)),
            Some(0x0000_0002)
        );
        assert_eq!(
            InventoryPanels::location_of_slot(ElementId(0x1000_05AC)),
            Some(0x0000_0200)
        );
        // Every mask is distinct, and every one is covered by the clearing pass.
        let mut seen = std::collections::BTreeSet::new();
        for (_, m) in PAPER_DOLL_SLOTS {
            assert!(seen.insert(m), "{m:#X} twice");
            assert_ne!(m & CLEAR_ALL_LOCATIONS, 0, "{m:#X} survives the clear");
        }
        // …except the two sigil bits above 0x40000000 — 0x7FFFFFFF does not reach bit 31, and
        // nothing in the shipped enum uses bit 31 either.
        assert!(PAPER_DOLL_SLOTS.iter().all(|(_, m)| *m <= 0x4000_0000));
    }
}
