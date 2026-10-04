//! The creature-mode preview space, drawn.
//!
//! The model is [`dereth_world_render::creature_mode`]; this is the half that has a device in it:
//! the objects the space holds, their sequence counters, their part meshes, and the draw pass inside one
//! UI element's rectangle.
//!
//! # Why this exists as a host module and not as a screen
//!
//! Two screens want the same thing and neither may draw. The portal screen wants the
//! portal object spinning behind a hidden world; the character preview viewport
//! wants the heritage model on a turntable; `SummaryPage` and the inventory paper-doll want it
//! after them. In the client all four are the *same* `Viewport`, and the space is a
//! member of the element rather than of the screen. This module is that member.
//!
//! # The two ids the client never writes down, and where they came from
//!
//! Preview initialization resolves enums `0x10000001` and `0x10000002` through the object cache.
//! That is [`dereth_client::assets::enum_did`](dereth_client_runtime::assets::enum_did)'s two-level
//! lookup, and the **group is 7, `UIASSET`** -- the same group the UI sound table
//! (`0x10000003`) comes from, which is what pins it. In this dat build the master `DidMapper`
//! `0x25000000` sends group 7 to `0x25000010`, and that mapper's own enum-to-name table names
//! them: `0x10000001` is **`portalspace_background`**, setup `0x02000306`, and `0x10000002` is
//! **`portalspace_animation`**, `Animation 0x030005AC`. Neither id is written at a call site here:
//! a DDD patch may move either, and the enum is what the client carries.
//!
//! The same mapper holds the char-gen turntable's animations (`CharGenAnimation 0x10000006` and
//! the five idle variations), the paper-doll's (`PaperDollAnimation 0x10000005`) and the four
//! images the char-gen colour wheel is built from.
//!
//! # Frame counting is real here
//!
//! The portal's frame is not derived from elapsed time at a fixed rate; it is the real sequence
//! counter. `PreviewSpace::set_sequence_animation` builds an `AnimData` (clearing first if
//! asked) and `PreviewSpace::curr_frame_number` reads the current frame. The player is
//! `dereth_animation::seq::Sequence`, which crosses whole animation frames against **elapsed
//! seconds** and accumulates nothing per frame.

use dereth_animation::parts::PartArray;

/// The selection mask retail computes for the player: the whole doll.
pub const WHOLE_DOLL_MASK: u32 = 0x7FFF_FFFF;

/// The asset cache's `UIASSET` group, through which both preview enums resolve.
pub const UIASSET_GROUP: u32 = 7;

/// Enum `0x10000001` -- `portalspace_background`, the setup the teleport tunnel's object is
/// made from.
pub const ENUM_PORTALSPACE_BACKGROUND: u32 = 0x1000_0001;

/// Enum `0x10000002` -- `portalspace_animation`, the sequence it runs at 40 fps.
pub const ENUM_PORTALSPACE_ANIMATION: u32 = 0x1000_0002;

/// `PaperDollAnimation`. The inventory paper-doll wants this one; recorded here because it is the
/// same table and finding it again is the expensive part.
pub const ENUM_PAPERDOLL_ANIMATION: u32 = 0x1000_0005;

/// `CharGenAnimation` -- for every non-Olthoi heritage.
pub const ENUM_CHARGEN_ANIMATION: u32 = 0x1000_0006;

// =================================================================================================
// The character-generation update tail that dresses the preview model
// =================================================================================================

use std::collections::BTreeMap;

use dereth_animation::parts::{AnimPartChange, ObjDesc, PaletteRange, TextureMapChange};
use dereth_assets::motion::ClothingTable;
use dereth_assets::tables::{CharGen, GearItem, ObjDesc as CgObjDesc};
use dereth_chargen::CharGenState;
use dereth_primitives::DataId;

/// A `Subpalette`'s `offset` / `numcolors` are palette **entries** everywhere the client keeps
/// them in memory -- `ClothingTable`'s ranges, the char-gen table's `ObjDesc`s, and the three
/// literal ranges -- but [`PaletteRange`] carries the **wire**
/// form, which is those values divided by eight. Every value that
/// crosses into an [`ObjDesc`] here is divided by this; `ExpandedPalette::apply_subpalette` multiplies
/// it back.
const PALETTE_UNIT: u32 = 8;

/// The whole 2,048-entry palette expressed in wire units, which is 256.
const WHOLE_PALETTE_UNITS: u32 = 2048 / PALETTE_UNIT;

/// `ObjDesc`'s three list caps: a list count of `0xff` discards the new entry and fails.
const OBJDESC_LIST_MAX: usize = 0xFF;

/// Add an animation-part change after removing the first earlier change for the same part.
///
/// The duplicate test compares the two entries' identity fields; at that
/// offset an `AnimPartChange` holds its `part_index`, so **a later change for the same part
/// replaces the earlier one and moves to the end of the list**. Only the first match is removed,
/// which is the client's loop.
pub fn add_anim_part_change(od: &mut ObjDesc, c: AnimPartChange) {
    if let Some(i) = od
        .part_changes
        .iter()
        .position(|x| x.part_index == c.part_index)
    {
        od.part_changes.remove(i);
    }
    if od.part_changes.len() < OBJDESC_LIST_MAX {
        od.part_changes.push(c);
    }
}

/// Add a texture-map change, whose duplicate identity is the same `part_index` **and** the same
/// `old_tex_id`.
/// Two garments substituting different original textures on one part therefore both survive.
pub fn add_texture_map_change(od: &mut ObjDesc, c: TextureMapChange) {
    if let Some(i) = od
        .texture_changes
        .iter()
        .position(|x| x.part_index == c.part_index && x.old_texture == c.old_texture)
    {
        od.texture_changes.remove(i);
    }
    if od.texture_changes.len() < OBJDESC_LIST_MAX {
        od.texture_changes.push(c);
    }
}

/// the one of the three with real precedence rules.
///
/// If any existing subpalette supercedes the new one, the new one is dropped (and that counts as
/// success). Otherwise **every** entry the new one replaces is removed, not just the first; a
/// list already at `0xff` entries refuses the new one; else it is appended. Here
/// `supercedes(a, b)` = *a covers the whole palette and b does not*, and
/// `replaces(new, old)` = *same (offset, numcolors)* **or** *new covers the whole palette*. So a
/// full-palette range already present wins over anything narrower, and a full-palette range being
/// added sweeps every narrower one away. Ranges are in wire units here, hence
/// [`WHOLE_PALETTE_UNITS`].
pub fn add_subpalette(od: &mut ObjDesc, s: PaletteRange) {
    let whole = |r: &PaletteRange| r.offset == 0 && r.length == WHOLE_PALETTE_UNITS;
    if od.subpalettes.iter().any(|e| whole(e) && !whole(&s)) {
        return;
    }
    od.subpalettes
        .retain(|e| !((s.offset == e.offset && s.length == e.length) || whole(&s)));
    if od.subpalettes.len() < OBJDESC_LIST_MAX {
        od.subpalettes.push(s);
    }
}

/// Merging an object descriptor applies subpalettes, then texture changes, then part changes, each
/// through its own add step. **It does not copy the palette id**, which is why the client's
/// update assigns the sex record's base palette by hand before the first merge.
fn merge_objdesc(od: &mut ObjDesc, src: &CgObjDesc) {
    for s in &src.subpalettes {
        add_subpalette(
            od,
            PaletteRange {
                palette_set: s.subpalette,
                offset: s.offset / PALETTE_UNIT,
                length: s.num_colors / PALETTE_UNIT,
            },
        );
    }
    for (part, old, new) in &src.texture_changes {
        add_texture_map_change(
            od,
            TextureMapChange {
                part_index: u32::from(*part),
                old_texture: *old,
                new_texture: *new,
            },
        );
    }
    for (part, id) in &src.anim_part_changes {
        add_anim_part_change(
            od,
            AnimPartChange {
                part_index: u32::from(*part),
                part_id: *id,
            },
        );
    }
}

/// Select a palette for a `shade` from an already-loaded palette list.
///
/// An empty list, or a shade outside `[0, 1]`, gives `INVALID_DID`; otherwise the index is
/// `(num_pals - 0.000001) * shade`, truncated.
///
/// The `- 0.000001` is what keeps `shade == 1.0` off the end. A shade of **-1** -- the reset value
/// of the skin and hair shades until something rolls
/// them -- is rejected, and the client still adds the resulting `INVALID_DID` subpalette.
#[must_use]
pub fn pal_set_palette_id(palettes: &[DataId], shade: f64) -> DataId {
    if palettes.is_empty() || !(0.0..=1.0).contains(&shade) {
        return DataId(0);
    }
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    // LINT-OK: the client's own truncation of `(num_pals - 0.000001) * shade`. The product is in
    // `[0, num_pals)` by construction, so the truncation is neither negative nor out of range.
    let i = ((palettes.len() as f64 - 0.000_001) * shade) as usize;
    palettes.get(i).copied().unwrap_or(DataId(0))
}

/// Build an object descriptor from `setup`, `template_key`, a shade package, and an output value.
///
/// Apply one `AnimPartChange` per object effect before the `TextureMapChange`s nested beneath it.
/// When `template_key != 0`, apply the palette template's sub-palette effects afterward, resolving
/// each through [`pal_set_palette_id`].
///
/// A shade package copies its single `double` into all four slots and
/// reading a value clamps the index to 3, so **every** effect of one garment takes the same
/// shade; the four-value form exists only for the server's dye packages. Hence the scalar.
///
/// Returns false exactly where the client returns 0: no clothing base for this setup, no such
/// palette template, or a `PalSet` that will not load. Whatever was applied before the failure
/// stays applied, as it does in the client, whose preview update ignores the return.
///
/// **The client's fallback chain is not reproduced.** When the clothing table's per-setup base
/// map has no entry for the wearer's setup, the client retries with one of ~30 hard-coded per-race/sex defaults
/// (the human-male default, the Umbraen-female default, …) chosen by a chain of
/// setup-id comparisons against globals this build does not carry. Every char-gen garment is
/// listed for the body that offers it, so the chain is unreachable from the wizard; the miss is
/// **counted** rather than swallowed -- [`ChargenDressStats::clothing_base_missing`].
pub fn build_obj_desc(
    table: &ClothingTable,
    setup: DataId,
    template_key: u32,
    shade: f64,
    palettes: &mut dyn FnMut(DataId) -> Vec<DataId>,
    out: &mut ObjDesc,
) -> bool {
    let Some(base) = table.clothing_bases.get(&setup) else {
        return false;
    };
    for eff in base {
        add_anim_part_change(
            out,
            AnimPartChange {
                part_index: eff.part_num,
                part_id: eff.object_id,
            },
        );
        for t in &eff.texture_effects {
            add_texture_map_change(
                out,
                TextureMapChange {
                    part_index: eff.part_num,
                    old_texture: t.old_texture,
                    new_texture: t.new_texture,
                },
            );
        }
    }
    if template_key == 0 {
        // "No dye": the object and texture changes only.
        return true;
    }
    let Some(tpl) = table.palette_templates.get(&template_key) else {
        return false;
    };
    let mut ok = true;
    for eff in &tpl.subpalette_effects {
        let set = palettes(eff.palette_set);
        if set.is_empty() {
            ok = false;
            continue;
        }
        let sub = pal_set_palette_id(&set, shade);
        for r in &eff.ranges {
            add_subpalette(
                out,
                PaletteRange {
                    palette_set: sub,
                    offset: r.offset / PALETTE_UNIT,
                    length: r.length / PALETTE_UNIT,
                },
            );
        }
    }
    ok
}

/// What [`chargen_objdesc`] reached, so a caller -- and a test -- can tell an empty descriptor
/// from a descriptor that was never asked for.
///
/// Every field is a count of a stage that *ran*, except the two that count failures. A zero without
/// a denominator cannot distinguish inactivity from a missing path; these are the denominators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChargenDressStats {
    /// How many `+=` merges ran after the sex's base object description: hair style, eyes strip,
    /// nose strip, mouth strip -- four call sites over the row's three descriptor *families*.
    pub merges: u32,
    /// How many of the **four** calls were reached. A style of -1
    /// ("nothing worn") reaches none, which is the client's own `style != -1` test.
    pub clothing_calls: u32,
    /// How many of those returned true.
    pub clothing_ok: u32,
    /// Calls that found no `ClothingBase` for the wearer's setup -- the arm whose hard-coded
    /// fallback chain is not reproduced -- plus any whose `ClothingTable` was not in the closed
    /// set the host loaded.
    pub clothing_base_missing: u32,
    /// How many of the **three** `Subpalette`s were added: skin (offset 0, `0xC0` colours), hair
    /// (`0xC0`, `0x40`) and the eye colour (`0x100`, `0x40`).
    pub subpalettes: u32,
    /// Subpalettes whose palette resolved to `INVALID_DID` -- an unrolled shade of -1, or a
    /// `PalSet` that would not load. The client adds them anyway and so does this.
    pub subpalette_unresolved: u32,
}

/// A garment row: the style list it comes from, the chosen style, the palette-template ids
/// the colour spots index, the chosen colour and the chosen shade -- one tuple per
/// clothing-description build in the character-generation preview update.
type GearRow<'a> = (&'a [GearItem], i32, &'a [u32], i32, f64);

/// Assemble the character-generation preview's appearance.
///
/// This is the whole of what dresses the char-gen model, in the client's own order:
///
/// 1. the palette id is the sex record's base palette, then its base object description is
///    merged in;
/// 2. the chosen hair style's object description is merged in;
/// 3. four clothing-description builds from the setup, the selected colour's palette-template id,
///    and the item's selected shade — **headgear, trousers, shirt, footwear, in that order** —
///    with any item whose style is -1 skipped;
/// 4. the eye strip's normal or bald object description according to
///    the chosen *hair style*'s `bald` byte, then the nose and mouth
///    strips' object descriptions;
/// 5. three `Subpalette`s -- skin selected from the sex record's skin `PalSet` at the skin shade, at
///    offset **0** for **0xC0** colours; hair from the chosen hair *colour*'s `PalSet` at
///    **0xC0**/**0x40**; and the chosen eye colour, which is a `Palette` id needing no `PalSet`
///    lookup at all, at **0x100**/**0x40**.
///
/// The ordering is load-bearing: the clothing goes on **between** the hair and the face, and the
/// three subpalettes come last, after every subpalette a garment's dye contributed.
///
/// The skin subpalette is added **unconditionally** -- it is the one of the three with no
/// `!= -1` guard -- so a wizard that has rolled nothing still gets an entry, with an
/// `INVALID_DID` palette that `PaletteComposition::build` then counts as a failed range. That is the
/// client.
///
/// `palettes` resolves a `PalSet` from `(id, 0x18)` to its palette id list. It is a
/// closure so the dat read stays with the caller and the block itself is testable against a stub.
#[must_use]
pub fn chargen_objdesc(
    cg: &CharGen,
    state: &CharGenState,
    clothing: &BTreeMap<DataId, ClothingTable>,
    setup: DataId,
    palettes: &mut dyn FnMut(DataId) -> Vec<DataId>,
) -> (ObjDesc, ChargenDressStats) {
    let mut od = ObjDesc::default();
    let mut st = ChargenDressStats::default();
    // Retail requires a non-zero heritage group and gender -- the whole update is inside that test.
    let Some(hg) = cg.heritage_groups.get(&state.heritage_group) else {
        return (od, st);
    };
    let Some(sx) = hg.sexes.get(&state.gender) else {
        return (od, st);
    };

    od.palette_id = sx.base_palette;
    merge_objdesc(&mut od, &sx.base_objdesc);

    let pick = |i: i32, n: usize| usize::try_from(i).ok().filter(|i| *i < n);
    if let Some(h) = pick(state.hair_style, sx.hair_styles.len()) {
        merge_objdesc(&mut od, &sx.hair_styles[h].objdesc);
        st.merges += 1;
    }

    // The four garments, in the client's update order. Each garment's palette-template lookup is
    // `(0 <= colour && colour < colour_count) ? ids[colour] : 0`, and a key of 0 is the clothing
    // build's own "no dye".
    let gear: [GearRow<'_>; 4] = [
        (
            &sx.headgear,
            state.headgear_style,
            &state.headgear_palette_template_ids,
            state.headgear_color,
            state.headgear_shade,
        ),
        (
            &sx.pants,
            state.trousers_style,
            &state.trousers_palette_template_ids,
            state.trousers_color,
            state.trousers_shade,
        ),
        (
            &sx.shirts,
            state.shirt_style,
            &state.shirt_palette_template_ids,
            state.shirt_color,
            state.shirt_shade,
        ),
        (
            &sx.footwear,
            state.footwear_style,
            &state.footwear_palette_template_ids,
            state.footwear_color,
            state.footwear_shade,
        ),
    ];
    for (list, style, ids, color, shade) in gear {
        let Some(item) = pick(style, list.len()).map(|i| &list[i]) else {
            continue;
        };
        st.clothing_calls += 1;
        let Some(table) = clothing.get(&item.clothing_table) else {
            st.clothing_base_missing += 1;
            continue;
        };
        if !table.clothing_bases.contains_key(&setup) {
            st.clothing_base_missing += 1;
        }
        let key = pick(color, ids.len()).map_or(0, |i| ids[i]);
        if build_obj_desc(table, setup, key, shade, palettes, &mut od) {
            st.clothing_ok += 1;
        }
    }

    // The bald state is the chosen **hair style**'s `bald` byte, not a state of its
    // own: a bald head needs the eyes strip drawn onto scalp rather than under a fringe.
    if let Some(e) = pick(state.eyes_strip, sx.eye_strips.len()) {
        let bald = pick(state.hair_style, sx.hair_styles.len())
            .is_some_and(|h| sx.hair_styles[h].bald != 0);
        let strip = &sx.eye_strips[e];
        merge_objdesc(
            &mut od,
            if bald {
                &strip.objdesc_bald
            } else {
                &strip.objdesc
            },
        );
        st.merges += 1;
    }
    if let Some(n) = pick(state.nose_strip, sx.nose_strips.len()) {
        merge_objdesc(&mut od, &sx.nose_strips[n].1);
        st.merges += 1;
    }
    if let Some(m) = pick(state.mouth_strip, sx.mouth_strips.len()) {
        merge_objdesc(&mut od, &sx.mouth_strips[m].1);
        st.merges += 1;
    }

    // The three `Subpalette`s. Offsets and lengths are the client's literals in palette entries;
    // `add_subpalette` takes wire units, hence the divisions.
    let sub = |od: &mut ObjDesc, st: &mut ChargenDressStats, pal: DataId, off: u32, n: u32| {
        if pal.0 == 0 {
            st.subpalette_unresolved += 1;
        }
        add_subpalette(
            od,
            PaletteRange {
                palette_set: pal,
                offset: off / PALETTE_UNIT,
                length: n / PALETTE_UNIT,
            },
        );
        st.subpalettes += 1;
    };
    // Skin -- unguarded in the client, so unguarded here.
    let skin = pal_set_palette_id(&palettes(sx.skin_palset), state.skin_shade);
    sub(&mut od, &mut st, skin, 0, 0xC0);
    if let Some(c) = pick(state.hair_color, sx.hair_colors.len()) {
        let set = palettes(DataId(sx.hair_colors[c]));
        let hair = pal_set_palette_id(&set, state.hair_shade);
        sub(&mut od, &mut st, hair, 0xC0, 0x40);
    }
    // The eye-colour list holds `Palette` ids, not `PalSet`s: `Update` uses the entry directly and
    // makes no call for it at all.
    if let Some(c) = pick(state.eye_color, sx.eye_colors.len()) {
        sub(&mut od, &mut st, DataId(sx.eye_colors[c]), 0x100, 0x40);
    }
    (od, st)
}

/// Memoized palette-set lookup over the retail dat, the one dat read [`chargen_objdesc`] needs.
///
/// The client resolves property `0x18` for the palette-set id and lets the object cache absorb repeats;
/// this memoises the id list, which is what a repaint asks for over and over.
#[derive(Debug, Default)]
pub struct PaletteSetCache {
    sets: BTreeMap<DataId, Vec<DataId>>,
}

impl PaletteSetCache {
    /// The palette ids of one `PalSet`, or an empty list when it will not load -- which is the
    /// client's null, and which [`build_obj_desc`] reports as a failure.
    pub fn palettes(&mut self, store: &dereth_dat::RetailDatStore, pal_set: DataId) -> Vec<DataId> {
        if let Some(hit) = self.sets.get(&pal_set) {
            return hit.clone();
        }
        let v = (|| {
            use dereth_assets::Decode;
            let bytes = dereth_primitives::AssetSource::read(store, pal_set).ok()?;
            let s = dereth_assets::material::PaletteSet::decode_payload(pal_set, &bytes).ok()?;
            Some(s.palette_ids)
        })()
        .unwrap_or_default();
        self.sets.insert(pal_set, v.clone());
        v
    }
}

/// Part-selection lighting for the equipment doll: it blinks whenever the selected-
/// item notice names a worn item or the player.
///
/// The original panel stores a flip count, the next-flip time, and the selection mask. It lights
/// the doll's preview physics body, never the world body, so this state lives with the preview
/// space and receives that object's part array from the caller.
///
/// On a global-message-3 tick once the current time reaches the stored next-flip time, count 1
/// applies `(mask, 0.99, 1.0)` for **bright**, count 2 applies `(mask, 0.0, 0.35)` for **dim**,
/// and count 3 restores the original lighting and unregisters the update. Each flip re-arms the
/// next-flip time as `cur_time + 0.2`; beginning the sequence sets it to `cur_time`,
/// so the bright flip lands on the **next** tick, not synchronously — unlike the world
/// producer, which lights on the notice itself. Bright 0.2 s, dim 0.2 s, restored: 0.4 s.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PaperDollSelectionLighting {
    /// Flip count, 0 when idle.
    pub flip_count: u32,
    /// Next-flip time, in client-clock seconds.
    pub time_next_flip: f64,
    /// Selection mask: `0x7FFFFFFF` for the whole doll, else the OR of the doll regions.
    pub selection_mask: u32,
}
impl PaperDollSelectionLighting {
    /// Compute the selection mask for the player or a worn object.
    ///
    /// `0x7FFFFFFF` when `id` is the player; otherwise the OR of the
    /// region bits whose upper inventory object at `loc` is `id`, in the client's own order:
    /// `0x404 → 4`, `0x2040 → 0x40`, `0x4080 → 0x80`, `0x100 → 0x100`, `0x202 → 2`,
    /// `0x808 → 8`, `0x1010 → 0x10`, `0x20 → 0x20`, `0x404 → 4` (asked twice), `1 → 1`.
    /// `upper_inv_obj` is `dereth_client_model::objects::ObjectInventory::upper_inv_obj`, whose `None`
    /// is the client's fall-through to the player id — which cannot equal a non-player `id`,
    /// so `None` is simply no match.
    #[must_use]
    pub fn selection_mask_from_object(
        id: dereth_primitives::ObjectId,
        player: Option<dereth_primitives::ObjectId>,
        upper_inv_obj: &dyn Fn(u32) -> Option<dereth_primitives::ObjectId>,
    ) -> u32 {
        if Some(id) == player {
            return WHOLE_DOLL_MASK;
        }
        const REGIONS: [(u32, u32); 10] = [
            (0x404, 4),
            (0x2040, 0x40),
            (0x4080, 0x80),
            (0x100, 0x100),
            (0x202, 2),
            (0x808, 8),
            (0x1010, 0x10),
            (0x20, 0x20),
            (0x404, 4),
            (1, 1),
        ];
        let mut mask = 0;
        for (loc, bit) in REGIONS {
            if upper_inv_obj(loc) == Some(id) {
                mask |= bit;
            }
        }
        mask
    }

    /// Begin part-selection lighting: a zero mask does nothing; otherwise set the flip count to 1
    /// and the next flip time to `cur_time`, store the mask, and restore the doll's lighting so a
    /// blink already in flight starts over from the shared material.
    /// Answers whether a blink was begun.
    pub fn begin(&mut self, mask: u32, cur_time: f64, doll: Option<&mut PartArray>) -> bool {
        if mask == 0 {
            return false;
        }
        self.flip_count = 1;
        self.time_next_flip = cur_time;
        self.selection_mask = mask;
        if let Some(doll) = doll {
            doll.restore_lighting_internal();
        }
        true
    }

    /// The part-selection lighting update, one global-message-3 tick.
    pub fn update(&mut self, cur_time: f64, doll: Option<&mut PartArray>) {
        if self.flip_count == 0 || self.time_next_flip > cur_time {
            return;
        }
        let (l, d) = match self.flip_count {
            1 => dereth_animation::parts::SELECTION_HIGH_LIGHTING,
            2 => dereth_animation::parts::SELECTION_LOW_LIGHTING,
            _ => {
                self.end(doll);
                return;
            }
        };
        if let Some(doll) = doll {
            Self::apply(self.selection_mask, l, d, doll);
        }
        self.flip_count += 1;
        self.time_next_flip = cur_time + dereth_animation::parts::SELECTION_FLIP_INTERVAL;
    }

    /// Ending the part-selection lighting: count and mask to zero, lighting restored on
    /// the doll, and the tick unregistered (here: `update` early-outs on a zero count).
    pub fn end(&mut self, doll: Option<&mut PartArray>) {
        self.flip_count = 0;
        self.selection_mask = 0;
        if let Some(doll) = doll {
            doll.restore_lighting_internal();
        }
    }

    /// Apply part-selection lighting: light the whole doll for the player's `0x7FFFFFFF`, else
    /// light each part named by a region bit of the selection mask.
    /// The part indices are the human setup's: 0 abdomen, 1/5 upper legs, 2/6 lower legs,
    /// 3/7 feet, 4/8 toes, 9 chest, 10/13 upper arms, 11/14 lower arms, 12/15 hands, 16 head.
    pub fn apply(mask: u32, luminosity: f32, diffuse: f32, doll: &mut PartArray) {
        if mask == WHOLE_DOLL_MASK {
            doll.set_lighting_internal(luminosity, diffuse);
            return;
        }
        const TABLE: [(u32, &[u32]); 9] = [
            (0x1, &[0x10]),
            (0x2 | 0x200, &[9]),
            (0x4 | 0x400, &[0]),
            (0x8 | 0x800, &[10, 13]),
            (0x10 | 0x1000, &[11, 14]),
            (0x40 | 0x2000, &[1, 5]),
            (0x80 | 0x4000, &[2, 6]),
            (0x20, &[12, 15]),
            (0x100, &[3, 7, 4, 8]),
        ];
        for (bits, parts) in TABLE {
            if mask & bits != 0 {
                for &p in parts {
                    doll.set_part_lighting_internal(p, luminosity, diffuse);
                }
            }
        }
    }
}

#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
pub use imp::*;

#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
mod imp {
    use std::sync::Arc;

    use dereth_animation::data::{AnimAssets, AnimData};
    use dereth_animation::parts::PartArray;
    use dereth_animation::seq::Sequence;
    use dereth_dat::RetailDatStore;
    use dereth_physics::BBoxExt;
    use dereth_primitives::{DataId, Frame, Quat, Vec3};
    use dereth_render::camera::{compute_aspect_for_viewport, view_distance_override, Viewport};
    use dereth_render::device::{Gpu, PerFrameConstants};
    use dereth_render::{DrawConstants, RenderError, ViewParams};
    use dereth_world_render::creature_mode::{CreatureMode, PreviewProjection};
    use dereth_world_render::lighting::LightType;

    use crate::world::{
        build_meshes, material_lighting, material_texture_factor, world_constants_scaled,
        BakeCache, PartMesh,
    };

    /// One physics body inside a preview space.
    ///
    /// A setup-backed object with no world entity: a part array, sequence, placement frame, and
    /// nothing else. The physics half is absent on purpose — nothing in a preview space collides,
    /// and the space's own loop over its objects
    /// calls `update_position` only to move the sound listener.
    #[derive(Debug)]
    pub struct PreviewObject {
        /// The setup the object was made from.
        pub setup: DataId,
        /// The object's part array.
        pub part_array: PartArray,
        /// The object's animation sequence.
        pub sequence: Sequence,
        /// The preview object's frame. Adding the object places it at the cell origin;
        /// the preview heading setter turns it.
        pub frame: Frame,
        /// `meshes[i]` draws `part_array.parts[i]`. Built once per object, in object space, and
        /// re-placed every frame in the same way as world objects.
        meshes: Vec<Vec<PartMesh>>,
        /// The `GfxObj` id each entry of [`Self::meshes`] was actually built from.
        ///
        /// This is not the same claim as `part_array.parts[i].gfxobj_id`: that is the *model*, and
        /// object-description changes applied after the meshes were baked would leave the two
        /// disagreeing while every model assertion still passed. A test that asserts only the
        /// model list can pass while the drawn geometry is wrong; this is the record that lets a
        /// test assert the geometry instead.
        built_from: Vec<DataId>,
        /// What applying the object description returned, or `None` when the object was added
        /// undressed.
        ///
        /// The client discards this return and so does the draw; it is kept because a descriptor
        /// naming a part the setup does not have is exactly the "silently wrong body" failure
        /// `WorldScene::objdesc_failures` was added for, and a value nothing can read is a value
        /// nothing can assert.
        pub dressed: Option<bool>,
    }

    impl PreviewObject {
        /// How many of this object's parts contributed geometry, for the tests. An object whose
        /// every part refused to draw is the failure this count exists to make visible.
        #[must_use]
        pub fn drawn_parts(&self) -> usize {
            self.meshes.iter().filter(|m| !m.is_empty()).count()
        }

        /// The `GfxObj` ids the meshes were baked from, in part order.
        ///
        /// See [`Self::built_from`]. A test comparing this against
        /// `part_array.parts[i].gfxobj_id` is asserting that the geometry on the GPU is the
        /// geometry the `ObjDesc` asked for, rather than that the model was updated.
        #[must_use]
        pub fn built_from(&self) -> &[DataId] {
            &self.built_from
        }

        /// How many parts carry a surface override — the texture-map half of an
        /// `ObjDesc`, which is what puts a tunic's colours on a body part whose `GfxObj` never
        /// changed.
        #[must_use]
        pub fn parts_with_surface_overrides(&self) -> usize {
            self.part_array
                .parts
                .iter()
                .filter(|p| p.surface_overrides.is_some())
                .count()
        }

        /// Compute the object's bounding box when its part array exists.
        ///
        /// For every present part, the client takes the part's graphics-object bounding box,
        /// scales both corners by the part's scale, transforms it by the part's position, and
        /// unions it into the result.
        ///
        /// Two details that are easy to miss:
        ///
        /// * The graphics-object lookup dereferences the array base, so the box is always
        ///   degrade **level 0**'s, never the level currently being drawn.
        /// * The function does not initialize `out`; the caller seeds both corners at the origin,
        ///   so the result always contains the object's origin, and a part array that loads nothing yields the zero box
        ///   rather than an inverted one. That seed is reproduced here.
        ///
        /// `gfx_bound_box` itself is min/max over the `GfxObj`'s **vertex array**
        /// ([`dereth_client_runtime::object_physics::gfx_bound_box`]).
        #[must_use]
        pub fn bounding_box(&self, store: &RetailDatStore) -> dereth_physics::geom::BBox {
            use dereth_assets::Decode;
            let mut out = dereth_physics::geom::BBox::default();
            for part in &self.part_array.parts {
                // Use the graphics-object array base, `gfxobj[0]`, not `part.gfxobj_id`: on the
                // player body that is a different, smaller mesh for 16 of 34 parts, and the
                // identify portrait's camera is placed from this box.
                let Some(id) = part.gfxobj_at(0) else {
                    continue;
                };
                let Ok(bytes) = store.read_typed(dereth_dat::DbType::GfxObj, id) else {
                    continue;
                };
                let Ok(g) = dereth_assets::GfxObj::decode_payload(id, &bytes) else {
                    continue;
                };
                let b = dereth_client_runtime::object_physics::gfx_bound_box(&g);
                let s = part.gfxobj_scale;
                let local = dereth_physics::geom::BBox::new(
                    Vec3::new(b.min.x * s.x, b.min.y * s.y, b.min.z * s.z),
                    Vec3::new(b.max.x * s.x, b.max.y * s.y, b.max.z * s.z),
                );
                local
                    .convert_to_global(&part.pos)
                    .build_bounding_box(&mut out);
            }
            out
        }

        /// Total triangles, so a test can say the space drew *something* without a frame capture.
        ///
        /// **Mind the divisor.** A [`PartMesh`] is an *object* mesh — the FVF `0x152` layout
        /// [`crate::world::OBJECT_VERTEX_STRIDE`] describes, 36 bytes a vertex. Dividing by
        /// the client shell's UI vertex size, `UI_VERTEX_BYTES` (24), instead would report exactly 1.5 times the
        /// truth, and a caller that asks only whether the count is positive would not notice.
        #[must_use]
        pub fn triangles(&self) -> usize {
            self.meshes
                .iter()
                .flatten()
                .map(|m| m.vertices.len() / crate::world::OBJECT_VERTEX_STRIDE / 3)
                .sum()
        }
    }

    /// `Viewport`'s `CreatureMode` plus the device resources its objects need.
    pub struct PreviewSpace {
        /// The model: camera, lights, FOV mode. See [`dereth_world_render::creature_mode`].
        pub mode: CreatureMode,
        objects: Vec<PreviewObject>,
        cache: BakeCache,
        /// The surfaces of the parts drawn with another era's look (`[Render] Objects`), apart
        /// from [`Self::cache`] because the two eras hold different records under the same ids.
        look_cache: BakeCache,
        assets: Arc<dereth_client_runtime::anim_assets::DatAnimAssets>,
    }

    // `BakeCache` and `DatAnimAssets` are decode memos with no `Debug`, which is why this is
    // written out rather than derived.
    impl std::fmt::Debug for PreviewSpace {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("PreviewSpace")
                .field("objects", &self.objects.len())
                .field("camera", &self.mode.view_frame.origin)
                .field("lights", &self.mode.lights.len())
                .field("use_world_fov", &self.mode.use_world_fov)
                .field("use_sharp_mode", &self.mode.use_sharp_mode)
                .finish()
        }
    }

    impl PreviewSpace {
        /// Construct the current preview space and initialize its private cell, preserving the
        /// original construction-then-initialization order.
        #[must_use]
        pub fn new(assets: Arc<dereth_client_runtime::anim_assets::DatAnimAssets>) -> Self {
            let mut mode = CreatureMode::new();
            mode.initialize_scene();
            Self {
                mode,
                objects: Vec::new(),
                cache: BakeCache::default(),
                look_cache: BakeCache::default(),
                assets,
            }
        }

        /// Create a physics object for the setup with object id zero and dynamic state.
        ///
        /// Returns the object's index, which is what the by-index lookup takes. `None` when the
        /// setup will not load, which is what a failed object creation does -- the client tests
        /// the pointer and adds nothing.
        ///
        /// # Errors
        /// [`RenderError`] when a texture or a pipeline state cannot be created.
        pub fn add_object(
            &mut self,
            store: &RetailDatStore,
            gpu: &mut Gpu,
            setup: DataId,
        ) -> Result<Option<usize>, RenderError> {
            self.add_object_dressed(store, gpu, setup, None)
        }

        /// Apply the object-description changes from the defaults.
        ///
        /// The client keeps the two apart: preview creation clones the player's physics body and
        /// applying the player's visual descriptor to the clone dresses it, and
        /// the second is re-run on every `Notice::PlayerObjDescChanged`. They are one call here
        /// because **the mesh bake happens between them**: `build_part_meshes` reads
        /// `part.gfxobj_id` and `part.surface_overrides`, so an `ObjDesc` applied after the bake
        /// changes the model and nothing on the GPU. That ordering is the defect this signature
        /// exists to make impossible, and [`PreviewObject::built_from`] is how a test sees it.
        ///
        /// `None` for the descriptor adds the object with nothing to apply — the char-gen
        /// turntable and the portal swirl, whose objects are never dressed.
        ///
        /// # Errors
        /// [`RenderError`] when a texture or a pipeline state cannot be created.
        pub fn add_object_dressed(
            &mut self,
            store: &RetailDatStore,
            gpu: &mut Gpu,
            setup: DataId,
            objdesc: Option<&dereth_animation::parts::ObjDesc>,
        ) -> Result<Option<usize>, RenderError> {
            self.add_object_dressed_in_look(store, gpu, setup, objdesc, None)
        }

        /// [`Self::add_object_dressed`] with the objects' look (`[Render] Objects`): `look` is
        /// the other era's files and the verdicts of which of their records stand for the
        /// world's, and each part is drawn wholly from one era exactly as the same object is in
        /// the world ([`dereth_client_runtime::models::parts_for_look`]). `None` draws the
        /// world's own.
        ///
        /// # Errors
        /// As [`Self::add_object_dressed`].
        pub fn add_object_dressed_in_look(
            &mut self,
            store: &RetailDatStore,
            gpu: &mut Gpu,
            setup: DataId,
            objdesc: Option<&dereth_animation::parts::ObjDesc>,
            look: Option<(
                &RetailDatStore,
                &dereth_client_runtime::object_identity::ObjectIdentity,
            )>,
        ) -> Result<Option<usize>, RenderError> {
            let Some(data) = self.assets.setup(setup) else {
                return Ok(None);
            };
            let mut sequence = Sequence::new();
            let Some(mut part_array) =
                PartArray::create_setup(Arc::clone(&data), true, &mut sequence, &*self.assets)
            else {
                return Ok(None);
            };
            // the setup's own default animation, queued
            // at the client's default 30 fps. A caller that wants another one calls
            // `set_sequence_animation`, which is exactly what both consumers do.
            if let Some(anim) = data.default_animation {
                sequence.clear_animations();
                sequence.append_animation(
                    AnimData {
                        anim_id: anim,
                        low_frame: 0,
                        high_frame: -1,
                        framerate: 30.0,
                    },
                    &*self.assets,
                );
            }
            // Adding an object sets placement frame 0.
            part_array.set_placement_frame(0, &mut sequence);
            let frame = Frame::new(Vec3::ZERO, Quat::IDENTITY);
            part_array.update_parts(&frame, &sequence);

            // The creature redress's tail, **before** the bake. A descriptor naming a part index the
            // setup does not have returns false; the client ignores that return and so does this,
            // because a partly-applied `ObjDesc` still dresses the parts it did reach.
            // `…_with(&*self.assets)`: there is no `MotionDriver` here — the preview
            // owns the part array directly — so the asset source is passed by hand, and it is the
            // same one `create_setup` above was given.
            let dressed = objdesc
                .map(|od| part_array.do_obj_desc_changes_from_default_with(od, &*self.assets));

            let (meshes, built_from) = self.build_part_meshes(store, gpu, &part_array, look)?;
            self.objects.push(PreviewObject {
                setup,
                part_array,
                sequence,
                frame,
                meshes,
                built_from,
                dressed,
            });
            Ok(Some(self.objects.len() - 1))
        }

        /// Remove every object from the preview space.
        pub fn remove_all_objects(&mut self) {
            self.objects.clear();
        }

        /// one object out of the space, by index.
        ///
        /// Character-generation redress removes the player object before rebuilding the
        /// body; here it is also how a test can take the background object away and leave the
        /// model, which is the only paired frame that isolates the background object.
        ///
        /// Removing anything but the last object renumbers the ones after it, exactly as the
        /// client\x27s own array does.
        pub fn remove_object(&mut self, i: usize) -> bool {
            if i >= self.objects.len() {
                return false;
            }
            self.objects.remove(i);
            true
        }

        /// Look up an object by its preview-space index.
        #[must_use]
        pub fn object(&self, i: usize) -> Option<&PreviewObject> {
            self.objects.get(i)
        }

        /// How many objects the space holds. The preview draws nothing when object zero is absent,
        /// so this being zero is the whole of "the panel
        /// stays as the UI left it".
        #[must_use]
        pub fn object_count(&self) -> usize {
            self.objects.len()
        }

        /// Replace or append the object's sequence animation.
        ///
        /// `AnimData{anim_id, framerate, low_frame}` with `high_frame` left at the constructor's
        /// `-1`, then clears the sequence if `clear` is set and appends the animation. Returns false when
        /// the object index is out of range or the animation is not in the dat -- the client's
        /// `append_animation` drops an unloadable animation silently, and saying so is what lets a
        /// test assert the sequence really started.
        pub fn set_sequence_animation(
            &mut self,
            i: usize,
            anim: DataId,
            clear: bool,
            low_frame: i32,
            framerate: f32,
        ) -> bool {
            let Some(o) = self.objects.get_mut(i) else {
                return false;
            };
            if clear {
                o.sequence.clear();
            }
            o.sequence.append_animation(
                AnimData {
                    anim_id: anim,
                    low_frame,
                    high_frame: -1,
                    framerate,
                },
                &*self.assets,
            );
            o.sequence.has_anims()
        }

        /// Whether the object's sequence currently has animations.
        ///
        /// The portal screen starts its sequence on the edge
        /// "the portal space is not visible", i.e. exactly once per tunnel. This build has
        /// no visibility flag on the space, so the same edge is "the tunnel is up and the object
        /// is not playing anything" -- which fires once for the same reason.
        #[must_use]
        pub fn has_anims(&self, i: usize) -> bool {
            self.objects.get(i).is_some_and(|o| o.sequence.has_anims())
        }

        /// Clear the object's queued sequence animations.
        pub fn clear_sequence_anims(&mut self, i: usize) {
            if let Some(o) = self.objects.get_mut(i) {
                o.sequence.clear_animations();
            }
        }

        /// The current sequence frame as an unsigned **`ulong`**.
        ///
        /// The unsignedness is load-bearing: the tunnel exit check computes
        /// `(0x78 - frame)` in it, and a frame past 120 wraps to about 4.29e9 rather than going
        /// negative, which is what keeps the tunnel from exiting early. See
        /// [`dereth_client_contract::teleport::in_exit_window`].
        #[must_use]
        pub fn curr_frame_number(&self, i: usize) -> u32 {
            #[allow(clippy::cast_sign_loss)]
            // LINT-OK: the client's own `int -> ulong`; a negative frame number is the wrap the
            // exit window relies on.
            self.objects
                .get(i)
                .map_or(0, |o| o.sequence.curr_frame_number() as u32)
        }

        /// Set the preview object's heading in degrees.
        pub fn set_heading(&mut self, i: usize, degrees: f32) {
            if let Some(o) = self.objects.get_mut(i) {
                dereth_world_render::math::set_heading(&mut o.frame, degrees);
            }
        }

        /// Apply object scale on top of each part's setup scale, including its animated offset.
        pub fn set_scale(&mut self, i: usize, scale: f32) {
            if let Some(o) = self.objects.get_mut(i) {
                let scale = if scale.is_finite() && scale > 0.0 {
                    scale
                } else {
                    1.0
                };
                o.part_array
                    .set_scale_internal(Vec3::new(scale, scale, scale));
                o.part_array.update_parts(&o.frame, &o.sequence);
            }
        }

        /// Advance the sequence and update the parts for every object in the space.
        ///
        /// **`dt` is elapsed seconds and nothing here accumulates per frame**:
        /// Sequence update advances `frame_number` by `framerate * dt` and crosses whole animation frames, so the
        /// same wall-clock interval reaches the same frame whatever the frame rate. That is the
        /// sky-scroll bug this project fixed twice, refused a third time.
        ///
        /// The root-motion frame argument is `None` here, which is the display-only
        /// path: hooks fire and frames advance, but no root motion is integrated. A preview object
        /// does not walk anywhere.
        pub fn use_time(&mut self, dt: f64) {
            let mut hooks = Vec::new();
            for o in &mut self.objects {
                o.sequence.update(dt, None, &mut hooks);
                hooks.clear();
                o.part_array.update_parts(&o.frame, &o.sequence);
            }
        }

        /// Draw the preview space inside the viewport bracket.
        ///
        /// `rect` is the element's screen box; `world_view_distance` is used while the preview
        /// has view-distance positioning enabled (the portal swirl's ramp), and `None` otherwise.
        ///
        /// The order is the client's: store the viewport, set the element's, **clear depth only**,
        /// render, put the viewport back. Nothing clears colour, so the panel art underneath shows
        /// through everywhere the model does not cover it.
        ///
        /// # Errors
        /// Any failure from the runtime.
        pub fn draw(
            &self,
            gpu: &mut Gpu,
            rect: Viewport,
            world_view_distance: Option<f32>,
            world_fov: f32,
        ) -> Result<(), RenderError> {
            // The render body runs only when preview object 0 exists.
            if self.objects.is_empty() || rect.width == 0 || rect.height == 0 {
                return Ok(());
            }
            // Compute the viewport aspect ratio from the element's rectangle, not
            // on the back buffer: a tall preview panel is a tall frustum.
            let display_ar = dereth_render::camera::AspectPreference::Normal
                .display_aspect_ratio(rect.width as f32, rect.height as f32);
            let aspect = compute_aspect_for_viewport(
                rect.width as f32,
                rect.height as f32,
                display_ar,
                false,
            );
            let (fov, vdist) = match self.mode.projection(world_view_distance, world_fov, aspect) {
                PreviewProjection::FovRadians(f) => (f, None),
                PreviewProjection::ViewDistance(d) => (self.mode.fov_radians, Some(d)),
            };
            let view = ViewParams {
                view: dereth_render::camera::view_from_frame(&self.mode.view_frame),
                fov_y_rad: fov,
                aspect,
                viewport: rect,
                // Preview rendering writes the space's ambient light colour
                // straight into the world-light ambient color, and the light update quantises it
                // into `D3DRS_AMBIENT`.
                lights: dereth_render::LightBlock {
                    ambient: dereth_world_render::lighting::ambient_render_state(self.mode.ambient),
                    ..dereth_render::LightBlock::default()
                },
                ..ViewParams::default()
            };

            gpu.set_viewport(rect);
            gpu.clear_depth(rect);
            let r = gpu.with_preview_sharp(self.mode.use_sharp_mode, |gpu| {
                view_distance_override::with(vdist, || {
                    let per_frame = PerFrameConstants::from_view(&view);
                    self.draw_objects(gpu, &per_frame)
                })
            });
            // `set_viewport(saved)` -- the element's rectangle must not leak into the rest of the
            // overlay, and it must be put back even if a draw failed.
            gpu.reset_viewport();
            r
        }

        /// The pass proper: every part of every object, opaque first and the alpha list after.
        ///
        /// Flushing the alpha list at depth `0.0` is the second half. The client defers blended
        /// subsets to a list and flushes it at the end of the space's own pass, inside the
        /// viewport; this draws them in the same two groups without the sort, which is
        /// `WorldScene::draw`'s own approximation and has the same one visible consequence -- two
        /// overlapping translucent subsets of one object are not depth-sorted against each other.
        fn draw_objects(
            &self,
            gpu: &mut Gpu,
            per_frame: &PerFrameConstants,
        ) -> Result<(), RenderError> {
            // The client enables sunlight, clears both light pools, disables
            // sunlight, then visits the private preview cell. Its creature-mode lights enter the
            // **static** pool, and every object is drawn through light minimization, which never
            // drops a directional light. Adding the light and setting its direction produces the
            // `LIGHTINFO` below; [`insert_light`] sees it relative to the identity frame.
            use dereth_world_render::lighting::{
                enabled_lights, minimize_object_lighting, set_direction, LightInfo, LightPools,
            };
            let mut pools = LightPools::new(0.0);
            let identity = Frame::new(Vec3::ZERO, dereth_primitives::Quat::IDENTITY);
            for l in &self.mode.lights {
                let mut info = LightInfo {
                    light_type: l.light_type,
                    offset: identity,
                    viewerspace_location: Vec3::ZERO,
                    color: l.color,
                    intensity: l.intensity,
                    falloff: l.falloff,
                    cone_angle: l.cone_angle,
                };
                set_direction(&mut info, l.direction);
                pools.add_static_from(&info, dereth_primitives::CellId(0), &identity, Vec3::ZERO);
            }
            for alpha_pass in [false, true] {
                for o in &self.objects {
                    for (i, meshes) in o.meshes.iter().enumerate() {
                        let Some(part) = o.part_array.parts.get(i) else {
                            continue;
                        };
                        if part.no_draw() || meshes.is_empty() {
                            continue;
                        }
                        let mut world = world_constants_scaled(&part.pos, part.gfxobj_scale);
                        // Preview drawing also applies the part clone's `Emissive` and
                        // `Diffuse` through current-material setup, on the doll; without this
                        // nothing of a part's material reaches the preview.
                        world.material_lighting = material_lighting(part.material);
                        // The material binding's other half: a cloned
                        // material selects its diffuse/alpha from `D3DRS_TEXTUREFACTOR` instead
                        // of the vertex, exactly as the world object's common part path does.
                        let factor = material_texture_factor(part.material);
                        if factor.is_some() {
                            world.draw_params[3] = 1.0;
                        }
                        // The same eight-slot selection the world path binds.
                        let active = minimize_object_lighting(&pools, part.pos.origin, 0.0);
                        let set = enabled_lights(&active, &pools, None);
                        for m in meshes {
                            let key = if factor.is_some() {
                                &m.key_material_alpha
                            } else {
                                &m.key
                            };
                            // The alpha list's membership test, as `WorldScene::draw` spells
                            // it: blended and not alpha-tested. The material-bearing key matters:
                            // a translucent clone makes an otherwise opaque surface an alpha-list
                            // member before the factor reaches the shader.
                            if (key.alpha_blend && !key.alpha_test) != alpha_pass {
                                continue;
                            }
                            if let Some(slot) = m.texture {
                                gpu.bind_texture(slot, m.sampler);
                            }
                            // The mesh-subset draw's emissive -- the surface's own luminosity
                            // when positive, else the clone's simple luminosity setting.
                            let mut lit = world;
                            let clone = material_lighting(part.material)[0];
                            let emissive = if m.luminosity > 0.0 {
                                m.luminosity
                            } else {
                                clone
                            };
                            crate::world::bind_lights(&mut lit, &set, emissive, false);
                            gpu.draw_dynamic(
                                key,
                                &DrawConstants {
                                    alpha_ref: m.alpha_ref,
                                    texture_factor: factor.unwrap_or(0),
                                    ..DrawConstants::default()
                                },
                                per_frame,
                                &lit,
                                &m.vertices,
                            )?;
                        }
                    }
                }
            }
            Ok(())
        }

        /// One entry per part, in part order, exactly as `WorldScene::build_part_meshes` does --
        /// a part that contributes nothing keeps its slot, because `meshes[i]` must line up with
        /// `part_array.parts[i]` and dropping an entry slides every later part onto the wrong bone.
        ///
        /// # Which level of detail a panel draws, and why it is not a distance
        ///
        /// Preview drawing pins the game's `degrades_disabled` flag to 1 for the whole pass, and
        /// degrade selection tests that flag first and returns level 0 before reading a distance
        /// at all. So every model in a panel is at level 0 always — not because the panel has no
        /// camera distance, but because the distance arms are never reached.
        /// Automatic-degrade settings and the global degrade distance are not consulted either.
        ///
        /// Level 0 is `PhysicsPart::gfxobj_at`, **not** `part.gfxobj_id`: graphics-object-array
        /// loading fills the array from `degrades[i].gfxobj_id`. The part's own id, on the player
        /// body (setup `0x02000001`), is level **1** for 16 of its 34 parts — baking from it would
        /// draw the doll with 417 triangles where the same body in the world, built level by level
        /// by [`crate::world::WorldScene`], draws 772: a lower-detail model one step down the
        /// ladder rather than its bottom level.
        ///
        /// The world path's degrade guard is **not** applied here: `draws_at_near_band` asks `get_degrade` with `degrades_disabled = 0`, which for
        /// the eleven `[self, terminator]` marker records answers *the terminator* — draw nothing.
        /// Inside a preview pass the flag is pinned and those markers draw.
        ///
        /// Returns the meshes and, alongside them, the graphics object each slot was actually baked
        /// from: one loop, so [`PreviewObject::built_from`] cannot drift from the geometry.
        fn build_part_meshes(
            &mut self,
            store: &RetailDatStore,
            gpu: &mut Gpu,
            array: &PartArray,
            look: Option<(
                &RetailDatStore,
                &dereth_client_runtime::object_identity::ObjectIdentity,
            )>,
        ) -> Result<(Vec<Vec<PartMesh>>, Vec<DataId>), RenderError> {
            let textures = crate::textures::TextureStore::new(store);
            let chosen = look.map(|(files, identity)| {
                dereth_client_runtime::models::parts_for_look(
                    store,
                    files,
                    identity,
                    None,
                    &array.parts,
                )
            });
            let look_textures = look.map(|(files, _)| crate::textures::TextureStore::new(files));
            let mut out = Vec::with_capacity(array.parts.len());
            let mut ids = Vec::with_capacity(array.parts.len());
            for (i, part) in array.parts.iter().enumerate() {
                if let (Some(Some(p)), Some((files, _)), Some(lt)) = (
                    chosen.as_ref().map(|c| c[i].as_ref()),
                    look,
                    look_textures.as_ref(),
                ) {
                    // Drawn wholly from the other era's files, through the look's own cache, at
                    // the look's own nearest level of the part.
                    let gfxobj = Some(
                        self.look_cache
                            .degrade_record(files, p.gfxobj_id)
                            .map_or(p.gfxobj_id, |d| d.degrades[0].gfxobj_id),
                    )
                    .filter(|g| g.0 != 0);
                    let groups = gfxobj
                        .map(|g| dereth_client_runtime::models::build_gfxobj(files, g))
                        .unwrap_or_default();
                    ids.push(gfxobj.unwrap_or(DataId(0)));
                    out.push(build_meshes(
                        files,
                        &mut self.look_cache,
                        lt,
                        gpu,
                        &groups,
                        p.surface_overrides.as_ref(),
                        None,
                    )?);
                    continue;
                }
                // The part takes its degrade level's gfxobj and returns if there is none — at
                // the level the pinned flag chose. A part that draws nothing keeps its slot: the
                // index is the one writes into, and dropping an entry
                // would slide every later part onto the wrong bone.
                let gfxobj = part.gfxobj_at(0);
                let groups = gfxobj
                    .map(|g| dereth_client_runtime::models::build_gfxobj(store, g))
                    .unwrap_or_default();
                ids.push(gfxobj.unwrap_or(DataId(0)));
                // A part the world draws beside the look's parts takes the look's colours where
                // the look has them, as it does in the world.
                let coloured = match (look, chosen.as_ref()) {
                    (Some((files, identity)), Some(c)) if c.iter().any(Option::is_some) => {
                        dereth_client_runtime::models::colours_for_look(files, identity, part).map(
                            |(p, ranges)| {
                                let t = crate::textures::TextureStore::new(store)
                                    .with_colours_from(files, ranges);
                                (p, t)
                            },
                        )
                    }
                    _ => None,
                };
                let (overrides, textures) = match &coloured {
                    Some((p, t)) => (p.surface_overrides.as_ref(), t),
                    None => (part.surface_overrides.as_ref(), &textures),
                };
                out.push(build_meshes(
                    store,
                    &mut self.cache,
                    textures,
                    gpu,
                    &groups,
                    overrides,
                    None,
                )?);
            }
            Ok((out, ids))
        }

        /// forwarded so a consumer need not reach into
        /// [`Self::mode`].
        pub fn set_light(&mut self, light_type: LightType, intensity: f32, direction: Vec3) {
            self.mode.set_light(light_type, intensity, direction);
        }

        /// Returns the part array of object `i` for callers that light it. The paper-doll inventory
        /// object is object 0 of the doll's space.
        pub fn part_array_mut(&mut self, i: usize) -> Option<&mut PartArray> {
            self.objects.get_mut(i).map(|o| &mut o.part_array)
        }
    }
}
