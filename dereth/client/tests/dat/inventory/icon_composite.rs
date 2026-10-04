//! Object-icon and spell-icon composition. An object icon is one generated 32x32 image on the
//! slot's icon child (`IconRecipe` through `GraphicRef::op`): base icon, custom overlay and
//! exact-white replacement by the effects surface on a drag surface, then the item-type background,
//! custom underlay and the drag surface on the main surface. A spell icon is the power-level
//! background, the spell icon, a plain or reversed tint and a fellowship or self-targeted badge.
//! The tests pin the group index arithmetic, the blend arithmetic, the layer order on real and
//! synthetic surfaces, the recipe every filled cell and spell slot carries, and the pixels.
//! Fixture: the retail dats' mapper groups and surfaces, and every recording on disk replayed into
//! a GPU-less gameplay screen; no device. The pixels through the device are the gpu tier's
//! `inventory::icon_composite`.

use crate::common::recorded_sessions;

use super::icon_bench::*;

use std::collections::{BTreeMap, BTreeSet};

use dereth_client::textures::TextureStore;
use dereth_client::ui_draw::composite;
use dereth_primitives::{AssetSource, DataId, TextureData, TextureFormat};
use dereth_ui::region::{IconRecipe, SurfaceOp};
use dereth_ui_screens::items::widget::{icon_background, spell_recipe, ItemSlot};

// =================================================================================================
// 1. The index arithmetic of the four groups
// =================================================================================================

/// Type and effects mapping use different zero handling.
/// Both begin with lowest-set-bit plus one. The type group substitutes index 0x21 for a zero
/// mask; the effects group queries row 0 and falls back only when resolution returns no surface.
/// Missing nonzero effects rows also use that fallback. Sharing the type helper would skip the
/// row-0 lookup, accidentally producing the same image here with the wrong selection rule.
/// Returning DEFAULT_INDEX for zero in icon_background::effect_index fails the first check;
/// removing the +1 fails the second.
#[test]
fn the_effects_index_is_lowest_set_bit_plus_one_with_no_substitution_at_all() {
    assert_eq!(
        icon_background::effect_index(0),
        0,
        "row 0, and the fallback is on the null"
    );
    assert_eq!(
        icon_background::effect_index(0x0000_0001),
        1,
        "UiEffects bit 0 is row 1"
    );
    assert_eq!(icon_background::effect_index(0x0000_0002), 2);
    assert_eq!(icon_background::effect_index(0x8000_0000), 32);
    // The lowest bit wins, exactly as it does for the item type.
    assert_eq!(icon_background::effect_index(0x0000_0005), 1);
    // And it is a *different* function from the type one, which substitutes at index 0.
    assert_ne!(
        icon_background::effect_index(0),
        icon_background::enum_index(0),
        "the two groups' zero handling is not the same and must not be shared"
    );
    assert_eq!(
        icon_background::enum_index(0),
        icon_background::DEFAULT_INDEX
    );
}

/// Effects use the fallback on a missing result, both for zero effects and missing nonzero rows.
#[test]
fn the_effects_fallback_row_is_reached_for_zero_and_missing_effect_rows() {
    let live = shipped_gameplay();
    let store = open_store();
    let default_row = dereth_assets::did_by_enum(&store, 0x1000_0005, 0x21).unwrap();
    assert_eq!(dereth_assets::did_by_enum(&store, 0x1000_0005, 0), None);
    assert_eq!(
        icon_background::effect_surface(&live.0, 0),
        Some(default_row)
    );
    let magical = dereth_assets::did_by_enum(&store, 0x1000_0005, 1).unwrap();
    assert_eq!(icon_background::effect_surface(&live.0, 1), Some(magical));
    assert_ne!(default_row, magical);
    for bit in 13..32u32 {
        assert_eq!(
            dereth_assets::did_by_enum(&store, 0x1000_0005, bit + 1),
            None
        );
        assert_eq!(
            icon_background::effect_surface(&live.0, 1 << bit),
            Some(default_row)
        );
    }
}

/// Spell tint selection is group 0x10000007 row `2 - (Reversed != 0)`, where Reversed
/// is bit 0x10. Badge selection tests Fellowship 0x2000 first (row 4), then SelfTargeted 0x8
/// (row 3), otherwise none. A spell with both flags gets only row 4. The combined-flags assertion
/// distinguishes that precedence from swapping the two branches.
#[test]
fn the_spell_overlay_rows_are_the_clients_and_fellowship_outranks_self_targeted() {
    assert_eq!(icon_background::spell_tint_index(0), 2, "not reversed");
    assert_eq!(
        icon_background::spell_tint_index(icon_background::SPELL_REVERSED),
        1,
        "reversed"
    );
    assert_eq!(icon_background::spell_overlay_index(0), None);
    assert_eq!(
        icon_background::spell_overlay_index(icon_background::SPELL_SELF_TARGETED),
        Some(3)
    );
    assert_eq!(
        icon_background::spell_overlay_index(icon_background::SPELL_FELLOWSHIP),
        Some(4)
    );
    assert_eq!(
        icon_background::spell_overlay_index(
            icon_background::SPELL_FELLOWSHIP | icon_background::SPELL_SELF_TARGETED
        ),
        Some(4),
        "the client tests 0x2000 first and returns; it never reaches the 0x8 arm"
    );
}

/// Walk master DidMapper 0x25000000 to secondary mappers 0x25000009, 0x2500000A and 0x2500000B
/// in client_portal.dat. Indexed rows must resolve RenderSurface ids (type 0x06).
/// The effects surfaces are opaque washes sampled through the icon's exact-white contour;
/// alpha histograms alone cannot identify that consumer. The contour tests check it directly.
/// Group-size checks compare effects versus spell backgrounds and spell backgrounds versus
/// spell overlays. They do not assert every pair differs or prove global mapper uniqueness.
#[test]
fn the_effect_and_spell_groups_answer_render_surfaces_for_every_row_a_composite_indexes() {
    let live = shipped_gameplay();
    let store = open_store();
    let rows = |group: u32| -> usize {
        let master = <dereth_assets::DidMapper as dereth_assets::Decode>::decode_payload(
            dereth_assets::MASTER_DID_MAPPER,
            &store
                .read(dereth_assets::MASTER_DID_MAPPER)
                .expect("the master DidMapper"),
        )
        .expect("decode");
        let second = master
            .enum_to_id
            .iter()
            .find(|(k, _)| *k == group)
            .map(|(_, v)| DataId(*v))
            .unwrap_or_else(|| panic!("the master mapper does not name {group:#010X}"));
        <dereth_assets::DidMapper as dereth_assets::Decode>::decode_payload(
            second,
            &store.read(second).expect("the second mapper"),
        )
        .expect("decode")
        .enum_to_id
        .len()
    };
    let n_effects = rows(icon_background::EFFECT_GROUP);
    let n_spell_bg = rows(icon_background::SPELL_BACKGROUND_GROUP);
    let n_spell_ov = rows(icon_background::SPELL_OVERLAY_GROUP);
    println!(
        "UIEffectIcons {n_effects} rows, UISpellBackgrounds {n_spell_bg} rows, \
         UISpellOverlays {n_spell_ov} rows"
    );
    // Pin the two group-size distinctions asserted below, alongside nontrivial populations.
    assert!(n_effects > 1 && n_spell_bg > 1 && n_spell_ov > 1);
    assert_ne!(n_effects, n_spell_bg);
    assert_ne!(n_spell_bg, n_spell_ov);

    // Every UiEffects bit the group declares resolves a RenderSurface, and so does the fallback.
    let mut effect_surfaces = BTreeSet::new();
    for bit in 0..32u32 {
        if let Some(d) = dereth_assets::did_by_enum(&store, icon_background::EFFECT_GROUP, bit + 1)
        {
            assert_eq!(
                d.0 >> 24,
                0x06,
                "effects row {} is not a RenderSurface: {d:?}",
                bit + 1
            );
            effect_surfaces.insert(d.0);
        }
    }
    let fallback = dereth_assets::did_by_enum(
        &store,
        icon_background::EFFECT_GROUP,
        icon_background::DEFAULT_INDEX,
    )
    .expect("the 0x21 fallback resolves");
    assert_eq!(fallback.0 >> 24, 0x06);
    println!(
        "{} distinct effect surfaces, fallback {fallback}",
        effect_surfaces.len()
    );
    assert!(
        !effect_surfaces.is_empty(),
        "no UiEffects bit resolves anything"
    );

    // The default contour source is opaque black, not a transparent no-op. Composition samples
    // it only at exact-white icon pixels instead of painting it across the whole background.
    let tex = TextureStore::new(&store);
    let (_, rs, _) = tex.resolve(fallback).expect("the fallback resolves");
    let px = pixels(&tex.texture_data(fallback).expect("the fallback decodes"));
    let opaque = px.iter().filter(|t| (*t >> 24) == 0xFF).count();
    let colours: BTreeSet<u32> = px.iter().copied().collect();
    println!(
        "the 0x21 effects default {fallback} is {}x{} format {} -- {opaque} of {} texels \
         opaque, {} distinct colour(s): {:?}",
        rs.width,
        rs.height,
        rs.format,
        px.len(),
        colours.len(),
        colours
            .iter()
            .take(4)
            .map(|c| format!("{c:#010X}"))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        rs.format, 21,
        "PFID_A8R8G8B8 -- so its alpha byte is real data, not a decode default"
    );
    assert_eq!(
        opaque,
        px.len(),
        "the 0x21 default is fully opaque, so it is NOT a no-op"
    );
    assert_eq!(colours.len(), 1, "and it is one flat colour");
    assert_eq!(
        *colours.iter().next().expect("one colour"),
        0xFF00_0000,
        "opaque black"
    );
    // The composite uses this opaque black surface at exact-white contour pixels.
    assert_eq!(
        icon_background::effect_surface(&live.0, 0),
        Some(fallback),
        "an object with no effects uses the default contour surface"
    );

    // These are opaque washes sampled only through the icon's white contour mask.
    let mut washes = 0usize;
    for bit in 0..12u32 {
        let Some(d) = dereth_assets::did_by_enum(&store, icon_background::EFFECT_GROUP, bit + 1)
        else {
            continue;
        };
        let p = pixels(&tex.texture_data(d).expect("decodes"));
        let op = p.iter().filter(|t| (*t >> 24) == 0xFF).count();
        let cols: BTreeSet<u32> = p.iter().copied().collect();
        assert_eq!(op, p.len(), "effects row {} ({d}) is not opaque", bit + 1);
        assert!(
            cols.len() > 1,
            "effects row {} ({d}) is a flat colour like the default",
            bit + 1
        );
        washes += 1;
    }
    println!("{washes} UIEffectIcons rows are opaque multi-colour washes");
    assert!(washes >= 10, "only {washes} real effect rows");

    // Every UiEffects row, with its name and its alpha histogram -- the measurement that says
    // whether this group is alpha-shaped rings or opaque washes.
    {
        let master = <dereth_assets::DidMapper as dereth_assets::Decode>::decode_payload(
            dereth_assets::MASTER_DID_MAPPER,
            &store
                .read(dereth_assets::MASTER_DID_MAPPER)
                .expect("master"),
        )
        .expect("decode");
        let second = master
            .enum_to_id
            .iter()
            .find(|(k, _)| *k == icon_background::EFFECT_GROUP)
            .map(|(_, v)| DataId(*v))
            .expect("UIEffectIcons");
        let m = <dereth_assets::DidMapper as dereth_assets::Decode>::decode_payload(
            second,
            &store.read(second).expect("read"),
        )
        .expect("decode");
        for (k, v) in &m.enum_to_id {
            let name = m
                .enum_to_name
                .iter()
                .find(|(n, _)| n == k)
                .map(|(_, s)| s.clone())
                .unwrap_or_default();
            let did = DataId(*v);
            let (mut op, mut cl) = (0usize, 0usize);
            let mut fmt = 0u32;
            if let Ok((_, rs, _)) = tex.resolve(did) {
                fmt = rs.format;
            }
            if let Ok(t) = tex.texture_data(did) {
                if t.format == TextureFormat::Bgra8 {
                    for x in pixels(&t) {
                        if x >> 24 == 0xFF {
                            op += 1;
                        } else if x >> 24 == 0 {
                            cl += 1;
                        }
                    }
                }
            }
            let mut cols: BTreeSet<u32> = BTreeSet::new();
            let mut black = 0usize;
            if let Ok(t) = tex.texture_data(did) {
                if t.format == TextureFormat::Bgra8 {
                    for x in pixels(&t) {
                        cols.insert(x);
                        if x == 0xFF00_0000 {
                            black += 1;
                        }
                    }
                }
            }
            println!(
                "  UIEffectIcons[{k:#04X}] {name:24} {did} fmt {fmt} opaque {op} clear {cl} \
                 colours {} black {black}",
                cols.len()
            );
        }
    }

    // Resolve the spell recipe's power-level rows 1..=9 and overlay rows 1..=4.
    for level in 1..=9u32 {
        let d = dereth_assets::did_by_enum(&store, icon_background::SPELL_BACKGROUND_GROUP, level)
            .unwrap_or_else(|| panic!("UISpellBackgrounds has no row for power level {level}"));
        assert_eq!(
            d.0 >> 24,
            0x06,
            "spell background {level} is not a RenderSurface: {d:?}"
        );
    }
    for row in 1..=4u32 {
        let d = dereth_assets::did_by_enum(&store, icon_background::SPELL_OVERLAY_GROUP, row)
            .unwrap_or_else(|| panic!("UISpellOverlays has no row {row}"));
        assert_eq!(
            d.0 >> 24,
            0x06,
            "spell overlay {row} is not a RenderSurface: {d:?}"
        );
    }
}

// =================================================================================================
// 2. The blend arithmetic
// =================================================================================================

/// Three-channel and four-channel alpha blends differ in destination-alpha handling.
/// Three-channel blending preserves destination alpha; four-channel blending can change it.
/// That distinction keeps an opaque item-type background opaque beneath icon artwork.
///
/// Preserve the client's rounding form `d - (((d*k)>>8) - ((s*k)>>8))`: two separate shifts are
/// not equivalent to `d + (((s-d)*k)>>8)`. The final input distinguishes them. Writing source
/// alpha in blit_3alpha fails the opaque-source/different-destination-alpha comparison.
#[test]
fn the_three_and_four_channel_alpha_blits_are_the_clients_and_differ_on_the_alpha_byte() {
    // a == 0: the destination is untouched, in both.
    assert_eq!(
        SurfaceOp::blit_3alpha(0xFF00_0000, 0x00FF_FFFF),
        0xFF00_0000
    );
    assert_eq!(
        SurfaceOp::blit_4alpha(0xFF00_0000, 0x00FF_FFFF),
        0xFF00_0000
    );

    // a == 255: three channels keeps the destination's alpha; four channels takes the source whole.
    // Same inputs, different answers -- this is the pair that makes the composite work.
    assert_eq!(
        SurfaceOp::blit_3alpha(0x8012_3456, 0xFFAA_BBCC),
        0x80AA_BBCC
    );
    assert_eq!(
        SurfaceOp::blit_4alpha(0x8012_3456, 0xFFAA_BBCC),
        0xFFAA_BBCC
    );

    // Normal copy stores all 32 source bits, including alpha.
    assert_eq!(
        SurfaceOp::blit_normal(0x8012_3456, 0x11AA_BBCC),
        0x11AA_BBCC
    );

    // Four-channel blending takes the source outright when destination alpha is zero. An icon
    // on a transparent target therefore retains its own color and alpha.
    assert_eq!(
        SurfaceOp::blit_4alpha(0x0000_0000, 0x80FF_0000),
        0x80FF_0000
    );

    // The partial blend, three channels, against an opaque destination: the destination's alpha
    // byte survives and the colour moves toward the source.
    let got = SurfaceOp::blit_3alpha(0xFF00_0000, 0x8000_00FF);
    assert_eq!(got >> 24, 0xFF, "the destination's alpha must not move");
    let b = got & 0xFF;
    assert!(
        b > 0 && b < 0xFF,
        "the blue channel should land between the two, got {b:#04X}"
    );

    // The two-shift rounding is the client's and is not the algebraic lerp. `d = 1, s = 0,
    // a = 0x7F` (k = 0x80): the client computes `1 - ((1*128)>>8) + ((0*128)>>8) = 1 - 0 + 0 = 1`;
    // the naive `d + ((s-d)*k >> 8)` is `1 + ((-1*128)>>8) = 1 - 1 = 0`.
    assert_eq!(SurfaceOp::blit_3alpha(0xFF00_0001, 0x7F00_0000) & 0xFF, 1);
}

// =================================================================================================
// 3. The composite, against real surfaces
// =================================================================================================
//
// The object recipe, on A8R8G8B8 local surfaces:
//
// ```text
// drag = transparent surface(32, 32)
//   copy base icon, if present
//   four-channel-alpha blend overlay, if present
//   replace exact white with effects-surface pixels, including the default effect
// main = transparent surface(32, 32)
//   copy background from group 0x10000004[lowest_set_bit(type) + 1], with zero fallback 0x21
//   three-channel-alpha blend custom underlay, if present
//   three-channel-alpha blend drag
// ```
//
// Without a background the final pass copies the drag surface, so the icon stays visible on an
// unbased target. The spell recipe:
//
// ```text
// copy power-level background from group 0x10000006
// four-channel-alpha blend spell icon, if present
// replace exact white with group 0x10000007[Reversed ? 1 : 2]
// if Fellowship: four-channel-alpha blend group 0x10000007[4]
// else if SelfTargeted: four-channel-alpha blend group 0x10000007[3]
// ```

fn pixels(t: &TextureData) -> Vec<u32> {
    assert_eq!(
        t.format,
        TextureFormat::Bgra8,
        "the composite is always BGRA8"
    );
    t.levels[0]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect()
}

/// How many of two equal-length images' texels differ. The **calibrated differ** used everywhere
/// below; see [`the_image_differ_is_calibrated_in_both_directions`].
fn differ(a: &[u32], b: &[u32]) -> usize {
    assert_eq!(
        a.len(),
        b.len(),
        "two images of different sizes cannot be compared"
    );
    a.iter().zip(b.iter()).filter(|(x, y)| x != y).count()
}

/// The effects surface and the custom underlay are treated differently: effects replace exact-white pixels in the drag surface;
/// underlay is alpha-blended onto the main background before the drag surface. Check ordinary
/// and magical DAT contours against white, transparent and opaque non-white source classes.
#[test]
fn shipped_icon_contours_use_effect_pixels_without_repainting_transparent_background() {
    let store = open_store();
    let tex = TextureStore::new(&store);
    let fetch = |id| tex.texture_data(id).ok();
    let icon = an_icon_from_the_corpus();
    let raw = pixels(&fetch(icon).unwrap());
    let background = dereth_assets::did_by_enum(&store, 0x1000_0004, 1).unwrap();
    let bg = pixels(&fetch(background).unwrap());
    let white = raw.iter().filter(|p| **p == 0xFFFF_FFFF).count();
    let transparent = raw.iter().filter(|p| **p >> 24 == 0).count();
    let solid = raw
        .iter()
        .filter(|p| **p >> 24 == 255 && **p != 0xFFFF_FFFF)
        .count();
    assert!(
        white > 0 && transparent > 0 && solid > 0,
        "all three DAT pixel classes must be exercised"
    );
    for row in [0x21, 1] {
        let effects = dereth_assets::did_by_enum(&store, 0x1000_0005, row).unwrap();
        let effect = pixels(&fetch(effects).unwrap());
        let actual = pixels(
            &composite(
                IconRecipe::Object {
                    background: Some(background),
                    effects: Some(effects),
                    icon: Some(icon),
                    overlay: None,
                    underlay: None,
                },
                &fetch,
            )
            .unwrap(),
        );
        for (i, p) in raw.iter().copied().enumerate() {
            if p == 0xFFFF_FFFF {
                assert_eq!(actual[i], effect[i], "row {row:#x}, contour pixel {i}");
            } else if p >> 24 == 0 {
                assert_eq!(
                    actual[i], bg[i],
                    "effects must not repaint the background at {i}"
                );
            } else if p >> 24 == 255 {
                assert_eq!(
                    actual[i], p,
                    "non-white object artwork at {i} stays unchanged"
                );
            }
        }
    }
    println!("{icon}, {white} contour / {transparent} transparent / {solid} opaque non-white pixels per effects row");
}

/// Calibrate both answers of the differ on real decoded surfaces. A constant-zero instrument
/// would accept all equality checks, while a constant-nonzero instrument would accept all
/// differences. Distinct tiles must differ and a tile compared with itself must not.
#[test]
fn the_image_differ_is_calibrated_in_both_directions() {
    let store = open_store();
    let tex = TextureStore::new(&store);
    let a = dereth_assets::did_by_enum(&store, icon_background::ITEM_TYPE_GROUP, 1)
        .expect("MELEE_WEAPON's tile");
    let b = dereth_assets::did_by_enum(&store, icon_background::ITEM_TYPE_GROUP, 2)
        .expect("ARMOR's tile");
    assert_ne!(
        a, b,
        "the two tiles must be different surfaces for this to calibrate anything"
    );
    let pa = pixels(&tex.texture_data(a).expect("decodes"));
    let pb = pixels(&tex.texture_data(b).expect("decodes"));
    let known_different = differ(&pa, &pb);
    let known_same = differ(&pa, &pa);
    println!(
        "differ calibration: {a} vs {b} = {known_different} px; {a} vs itself = {known_same} px"
    );
    assert!(
        known_different > 0,
        "the differ reports 0 on two surfaces that are not the same"
    );
    assert_eq!(
        known_same, 0,
        "the differ reports a difference between a file and itself"
    );
}

/// The type tile is beneath the icon, and this selected pair makes the ordering observable.
/// Compare a background-only recipe with the tile, then a tile-plus-icon recipe with both raw
/// inputs. Every transparent icon texel must show its tile texel, with a nonzero denominator.
/// Finally copy the opaque tile over the icon as the wrong-order control and require a different
/// result. This test uses two real surfaces, not the complete five-field object recipe.
#[test]
fn the_composite_puts_the_type_tile_under_the_icon_and_the_order_is_observable() {
    let store = open_store();
    let tex = TextureStore::new(&store);
    let fetch = |d: DataId| tex.texture_data(d).ok();

    // A real pair: MELEE_WEAPON's tile and a real 32x32 icon out of the corpus.
    let tile = dereth_assets::did_by_enum(&store, icon_background::ITEM_TYPE_GROUP, 1)
        .expect("MELEE_WEAPON's tile");
    let icon = an_icon_from_the_corpus();
    let tile_px = pixels(&tex.texture_data(tile).expect("the tile decodes"));
    let icon_px = pixels(&tex.texture_data(icon).expect("the icon decodes"));

    let both = composite(
        IconRecipe::Object {
            background: Some(tile),
            effects: None,
            icon: Some(icon),
            overlay: None,
            underlay: None,
        },
        &fetch,
    )
    .expect("the composite builds");
    let both_px = pixels(&both);
    assert_eq!(
        (both.width, both.height),
        (32, 32),
        "the composite is 32 by 32"
    );

    // Run the tile-only control through the same compositor and compare it with the decoded tile.
    let tile_only = pixels(
        &composite(
            IconRecipe::Object {
                background: Some(tile),
                effects: None,
                icon: None,
                overlay: None,
                underlay: None,
            },
            &fetch,
        )
        .expect("builds"),
    );
    assert_eq!(
        differ(&tile_only, &tile_px),
        0,
        "a background-only composite is the tile itself"
    );

    // The composite is neither of its inputs...
    assert!(
        differ(&both_px, &tile_px) > 0,
        "the icon did not reach the composite"
    );
    assert!(
        differ(&both_px, &icon_px) > 0,
        "the tile did not reach the composite"
    );

    // ...and where the icon is transparent, the tile wins. `blit_3alpha` with a == 0 leaves the
    // destination alone, so every texel whose icon alpha is 0 must equal the tile exactly.
    let (mut clear, mut agree) = (0usize, 0usize);
    for i in 0..both_px.len() {
        if icon_px[i] >> 24 == 0 {
            clear += 1;
            if both_px[i] == tile_px[i] {
                agree += 1;
            }
        }
    }
    println!(
        "{clear} of {} texels of {icon} are transparent; {agree} of them show the tile",
        icon_px.len()
    );
    assert!(
        clear > 0,
        "the icon covers all 1024 texels, so this cannot test the ordering"
    );
    assert_eq!(
        agree, clear,
        "a transparent icon texel did not show the tile beneath it"
    );

    // The wrong order is a different picture, which is what makes the assertion above a test of the
    // ordering rather than of the presence of two surfaces.
    let wrong = {
        let mut m = icon_px.clone();
        for (d, s) in m.iter_mut().zip(tile_px.iter()) {
            *d = SurfaceOp::blit_normal(*d, *s);
        }
        m
    };
    assert!(
        differ(&both_px, &wrong) > 0,
        "compositing the tile last gives the same picture, so the order is not observable here"
    );
}

/// Behaviour: inventory.icons.the-effects-ring-replaces-the-icons-white-contour
///
/// The default is not a no-op: it makes an ordinary icon's white contour black.
/// Magical effects replace the same pixels with their DAT gradient, not the entire type tile.
#[test]
fn ordinary_and_magical_effects_both_replace_the_icons_white_contour() {
    let live = shipped_gameplay();
    let store = open_store();
    let tex = TextureStore::new(&store);
    let icon = an_icon_from_the_corpus();
    let raw = pixels(&tex.texture_data(icon).unwrap());
    let background = dereth_assets::did_by_enum(&store, 0x1000_0004, 1).unwrap();
    let build = |effects| {
        pixels(
            &composite(
                IconRecipe::Object {
                    background: Some(background),
                    effects,
                    icon: Some(icon),
                    overlay: None,
                    underlay: None,
                },
                &|d| tex.texture_data(d).ok(),
            )
            .unwrap(),
        )
    };
    let bare = build(None);
    let ordinary = build(icon_background::effect_surface(&live.0, 0));
    let magical = build(icon_background::effect_surface(&live.0, 1));
    let white = raw.iter().filter(|p| **p == 0xFFFF_FFFF).count();
    assert!(white > 0);
    assert_eq!(differ(&bare, &ordinary), white);
    assert_eq!(differ(&ordinary, &magical), white);
}

/// Blend the custom overlay over the icon inside the drag surface before that surface reaches
/// the background. Removing the overlay must change the selected real pair. Whether flattening
/// the overlay onto the main surface is distinguishable depends on alpha: the synthetic
/// partial-alpha control below establishes that structural difference independently.
#[test]
fn the_icon_overlay_is_blitted_over_the_icon_inside_the_drag_surface() {
    let store = open_store();
    let tex = TextureStore::new(&store);
    let fetch = |d: DataId| tex.texture_data(d).ok();
    let tile =
        dereth_assets::did_by_enum(&store, icon_background::ITEM_TYPE_GROUP, 1).expect("tile");
    let icon = an_icon_from_the_corpus();
    // Choose a real corpus overlay different from the selected icon, with an effects-row fallback
    // if none is available. Many overlays are their object's own icon (the four Foci), and an
    // overlay blitted over an identical icon changes zero texels; the explicit inequality keeps
    // identical artwork from making the test vacuous.
    let overlay = corpus_overlays()
        .into_iter()
        .find(|d| *d != icon)
        .unwrap_or_else(|| {
            dereth_assets::did_by_enum(&store, icon_background::EFFECT_GROUP, 1).expect("bit 0")
        });
    assert_ne!(
        overlay, icon,
        "an overlay blitted over an identical icon cannot show anything"
    );

    let base = IconRecipe::Object {
        background: Some(tile),
        effects: None,
        icon: Some(icon),
        overlay: None,
        underlay: None,
    };
    let with = IconRecipe::Object {
        background: Some(tile),
        effects: None,
        icon: Some(icon),
        overlay: Some(overlay),
        underlay: None,
    };
    let a = pixels(&composite(base, &fetch).expect("builds"));
    let b = pixels(&composite(with, &fetch).expect("builds"));
    let d = differ(&a, &b);
    println!("overlay {overlay} changes {d} texels of the composite");
    assert!(d > 0, "the overlay did not reach the composite");

    // Measure whether flattening is visible with this real pair. The drag surface is completed
    // before blending it onto the main background. Binary-alpha artwork hides the difference
    // (zero texels). Assert zero only when
    // the selected icon has no partial-alpha texels; otherwise report the measured difference.
    let flattened = {
        let mut m = a.clone();
        let ov = pixels(&tex.texture_data(overlay).expect("decodes"));
        for (d, s) in m.iter_mut().zip(ov.iter()) {
            *d = SurfaceOp::blit_4alpha(*d, *s);
        }
        m
    };
    let partial = pixels(&tex.texture_data(icon).expect("decodes"))
        .iter()
        .filter(|t| {
            let a = *t >> 24;
            a != 0 && a != 0xFF
        })
        .count();
    let d2 = differ(&b, &flattened);
    println!(
        "{partial} of 1024 texels of {icon} have partial alpha; flattening the overlay onto \
         the main surface instead differs by {d2} texels"
    );
    if partial == 0 {
        assert_eq!(
            d2, 0,
            "with a binary-alpha icon the two structures must agree exactly; a difference here \
             means one of the two blit transcriptions is wrong"
        );
    }

    // Drive the structural distinction with synthetic partial-alpha artwork through the same
    // production compositor: half-alpha green icon, half-alpha red overlay and opaque blue tile.
    let half = DataId(0x0600_0011);
    let ov2 = DataId(0x0600_0012);
    let bg2 = DataId(0x0600_0013);
    let flat = |c: u32| -> TextureData {
        let mut bytes = Vec::with_capacity(32 * 32 * 4);
        for _ in 0..32 * 32 {
            bytes.extend_from_slice(&c.to_le_bytes());
        }
        TextureData {
            width: 32,
            height: 32,
            format: TextureFormat::Bgra8,
            levels: vec![bytes],
        }
    };
    let synth = |id: DataId| -> Option<TextureData> {
        if id == half {
            Some(flat(0x8000_FF00)) // half-alpha green: the icon
        } else if id == ov2 {
            Some(flat(0x80FF_0000)) // half-alpha red: the overlay
        } else if id == bg2 {
            Some(flat(0xFF00_00FF)) // opaque blue: the type tile
        } else {
            None
        }
    };
    let two_surface = pixels(
        &composite(
            IconRecipe::Object {
                background: Some(bg2),
                effects: None,
                icon: Some(half),
                overlay: Some(ov2),
                underlay: None,
            },
            &synth,
        )
        .expect("builds"),
    );
    let one_surface = {
        let mut m = pixels(
            &composite(
                IconRecipe::Object {
                    background: Some(bg2),
                    effects: None,
                    icon: Some(half),
                    overlay: None,
                    underlay: None,
                },
                &synth,
            )
            .expect("builds"),
        );
        let ov = pixels(&synth(ov2).expect("synthetic"));
        for (d, s) in m.iter_mut().zip(ov.iter()) {
            *d = SurfaceOp::blit_4alpha(*d, *s);
        }
        m
    };
    let d3 = differ(&two_surface, &one_surface);
    println!(
        "driven by hand on a half-alpha icon, the two-surface structure differs from the \
         flattened one by {d3} of 1024 texels ({:#010X} vs {:#010X})",
        two_surface[0], one_surface[0]
    );
    assert_eq!(
        d3, 1024,
        "the two-surface structure is unobservable even on a partial-alpha icon, which would mean \
         IconRecipe could have been a flat layer list after all"
    );
}

/// Recorded underlays exist (the first is in house-purchase-and-trade, pinned by the nonempty
/// check below); synthetic artwork isolates their ordering. The hand-built half-icon control
/// requires underlay to blend over the type tile but beneath the drag icon, with exactly 512
/// changed texels.
#[test]
fn the_corpus_carries_an_icon_underlay_and_it_blends_over_the_tile_beneath_the_icon() {
    // The synthetic drive below is the only thing that exercises the blit order; the corpus-side
    // fact that recorded underlays exist is pinned here.
    assert!(
        !corpus_underlays().is_empty(),
        "house-purchase-and-trade carries a recorded icon underlay id, so capture-backed coverage is \
         possible"
    );
    let icon = DataId(0x0600_0001);
    let under = DataId(0x0600_0002);
    let background = DataId(0x0600_0003);
    let fetch = |id| {
        let mut bytes = Vec::new();
        for _y in 0..32 {
            for x in 0..32 {
                let p: u32 = if id == icon {
                    if x < 16 {
                        0xFFFF_FFFF
                    } else {
                        0
                    }
                } else if id == under {
                    0xFF00_00FF
                } else {
                    0xFFFF_0000
                };
                bytes.extend_from_slice(&p.to_le_bytes());
            }
        }
        Some(TextureData {
            width: 32,
            height: 32,
            format: TextureFormat::Bgra8,
            levels: vec![bytes],
        })
    };
    let build = |underlay| {
        pixels(
            &composite(
                IconRecipe::Object {
                    background: Some(background),
                    effects: None,
                    icon: Some(icon),
                    overlay: None,
                    underlay,
                },
                &fetch,
            )
            .unwrap(),
        )
    };
    let without = build(None);
    let with = build(Some(under));
    assert_eq!(differ(&without, &with), 512);
    for (i, p) in with.iter().enumerate() {
        assert_eq!(
            *p,
            if i % 32 < 16 {
                0xFFFF_FFFF
            } else {
                0xFF00_00FF
            },
            "underlay is visible through transparent artwork, not substituted into white pixels"
        );
    }
}

/// Exact-white replacement follows the custom overlay and compares all 32 bits, not just RGB.
/// Later blending leaves the custom underlay visible through transparent or partial artwork.
#[test]
fn effects_replace_exact_white_after_overlay_and_preserve_near_white_and_alpha() {
    let background = DataId(1);
    let icon = DataId(2);
    let overlay = DataId(3);
    let effects = DataId(4);
    let underlay = DataId(5);
    let fetch = |id| {
        let mut bytes = Vec::new();
        for i in 0..1024 {
            let p: u32 = match id {
                DataId(1) => 0xFFFF_0000,
                DataId(2) => [
                    0xFFFF_FFFF,
                    0xFFFF_FFFE,
                    0x7FFF_FFFF,
                    0xFF77_8899,
                    0,
                    0xFF77_8899,
                ][i % 6],
                DataId(3) => {
                    if i % 6 == 5 {
                        0xFFFF_FFFF
                    } else {
                        0
                    }
                }
                DataId(4) => {
                    if i % 6 == 5 {
                        0xFF00_00FF
                    } else {
                        0xFF00_00AA
                    }
                }
                DataId(5) => 0xFF00_FF00,
                _ => return None,
            };
            bytes.extend_from_slice(&p.to_le_bytes());
        }
        Some(TextureData {
            width: 32,
            height: 32,
            format: TextureFormat::Bgra8,
            levels: vec![bytes],
        })
    };
    let actual = pixels(
        &composite(
            IconRecipe::Object {
                background: Some(background),
                effects: Some(effects),
                icon: Some(icon),
                overlay: Some(overlay),
                underlay: Some(underlay),
            },
            &fetch,
        )
        .unwrap(),
    );
    for (i, p) in actual.iter().enumerate() {
        let expected = [
            0xFF00_00AA,
            0xFFFF_FFFE,
            0xFF7F_FF7F,
            0xFF77_8899,
            0xFF00_FF00,
            0xFF00_00FF,
        ][i % 6];
        assert_eq!(*p, expected, "source-ordered composite pixel {i}");
    }
}

/// Exercise spell icon, tint selection and badge with decoded DAT surfaces.
/// The background-only arm is the baseline, then adding the icon, switching plain/reversed tint
/// and adding the badge must each change pixels. There is no independent background-removal
/// differential here; group resolution and recipe joining check its presence elsewhere.
#[test]
fn the_spell_composite_layers_the_background_the_icon_the_tint_and_the_badge() {
    let store = open_store();
    let tex = TextureStore::new(&store);
    let fetch = |d: DataId| tex.texture_data(d).ok();
    let g = |group: u32, row: u32| dereth_assets::did_by_enum(&store, group, row);

    let bg = g(icon_background::SPELL_BACKGROUND_GROUP, 1).expect("power level 1");
    let tint_plain = g(icon_background::SPELL_OVERLAY_GROUP, 2).expect("non-reversed");
    let tint_rev = g(icon_background::SPELL_OVERLAY_GROUP, 1).expect("reversed");
    let badge = g(icon_background::SPELL_OVERLAY_GROUP, 4).expect("fellowship");
    let icon = a_spell_icon_from_the_corpus();

    let build = |r: IconRecipe| pixels(&composite(r, &fetch).expect("builds"));
    let bare = build(IconRecipe::Spell {
        background: Some(bg),
        icon: None,
        tint: None,
        overlay: None,
    });
    let with_icon = build(IconRecipe::Spell {
        background: Some(bg),
        icon: Some(icon),
        tint: None,
        overlay: None,
    });
    assert!(
        differ(&bare, &with_icon) > 0,
        "the spell icon did not reach the composite"
    );

    let plain = build(IconRecipe::Spell {
        background: Some(bg),
        icon: Some(icon),
        tint: Some(tint_plain),
        overlay: None,
    });
    let reversed = build(IconRecipe::Spell {
        background: Some(bg),
        icon: Some(icon),
        tint: Some(tint_rev),
        overlay: None,
    });
    let d_tint = differ(&plain, &reversed);
    println!("the reversed tint ({tint_rev} vs {tint_plain}) changes {d_tint} texels");
    assert!(
        d_tint > 0,
        "the two UISpellOverlays washes give the same picture -- the harmful-spell tint is then \
         unobservable in this build and must be reported as such, not as a pass"
    );

    let with_badge = build(IconRecipe::Spell {
        background: Some(bg),
        icon: Some(icon),
        tint: Some(tint_plain),
        overlay: Some(badge),
    });
    let d_badge = differ(&plain, &with_badge);
    println!("the fellowship badge ({badge}) changes {d_badge} texels");
    assert!(d_badge > 0, "the badge did not reach the composite");
}

// =================================================================================================
// 4. The corpus coverage
// =================================================================================================

/// Collect each object's five icon fields at the first busiest instant of every discovered recording.
fn corpus_objects() -> Vec<(u32, u32, u32, Option<DataId>, Option<DataId>)> {
    let mut out = Vec::new();
    for session in corpus_sessions() {
        let (objects, _) = replay_to(&session, busiest_instant(&session));
        for (_, w) in objects.world.tables.weenies.iter() {
            out.push((
                w.pwd.icon_id,
                w.pwd.obj_type,
                w.pwd.effects.unwrap_or(0),
                w.pwd.icon_overlay_id.map(DataId).filter(|d| d.0 != 0),
                w.pwd.icon_underlay_id.map(DataId).filter(|d| d.0 != 0),
            ));
        }
    }
    out
}

fn corpus_overlays() -> Vec<DataId> {
    corpus_objects().into_iter().filter_map(|o| o.3).collect()
}

fn corpus_underlays() -> Vec<DataId> {
    corpus_objects().into_iter().filter_map(|o| o.4).collect()
}

/// Every recorded icon overlay is a RenderSurface, and the corpus carries both kinds: overlays
/// that are the object's own base icon (the Foci) and overlays that differ from it (the ones
/// `house-purchase-and-trade` brought). The overlay-order test selects a differing surface
/// globally rather than a matched recorded object pair.
#[test]
fn the_corpus_carries_overlays_that_are_the_objects_own_icon_and_overlays_that_differ() {
    let mut cells = 0usize;
    let mut own_icon = 0usize;
    let mut distinct: Vec<u32> = Vec::new();
    let mut differing: Vec<(u32, u32)> = Vec::new();
    for (icon, _, _, overlay, _) in corpus_objects() {
        let Some(ov) = overlay else { continue };
        if ov.0 == icon {
            own_icon += 1;
        } else if !differing.contains(&(icon, ov.0)) {
            differing.push((icon, ov.0));
        }
        if !distinct.contains(&ov.0) {
            distinct.push(ov.0);
        }
        cells += 1;
    }
    distinct.sort_unstable();
    differing.sort_unstable();
    assert!(cells > 0, "no recorded object carries an icon overlay");
    for ov in &distinct {
        assert_eq!(
            ov >> 24,
            0x06,
            "recorded overlay {ov:#010X} is not a RenderSurface"
        );
    }
    assert!(
        own_icon > 0,
        "no recorded overlay is its object's own base icon"
    );
    assert!(
        !differing.is_empty(),
        "no recorded (icon, overlay) pair has an overlay that differs from the object's own icon"
    );
}

/// A real 32×32 icon out of the corpus, so the composite tests are built on dat art rather than on
/// a synthetic gradient.
fn an_icon_from_the_corpus() -> DataId {
    let store = open_store();
    let tex = TextureStore::new(&store);
    for (icon, ..) in corpus_objects() {
        if icon == 0 {
            continue;
        }
        let d = DataId(icon);
        if let Ok(t) = tex.texture_data(d) {
            if t.width == 32 && t.height == 32 && t.format == TextureFormat::Bgra8 {
                // It must have both transparent and opaque texels, or it cannot show an ordering.
                let px = pixels(&t);
                let clear = px.iter().filter(|x| *x >> 24 == 0).count();
                let solid = px.iter().filter(|x| *x >> 24 == 0xFF).count();
                if clear > 0 && solid > 0 {
                    return d;
                }
            }
        }
    }
    panic!("no corpus object has a 32x32 icon with both transparent and opaque texels");
}

fn a_spell_icon_from_the_corpus() -> DataId {
    let store = open_store();
    let tex = TextureStore::new(&store);
    let (_, hud) = scene("first-login-walk-jump");
    assert!(
        !hud.spells.is_empty(),
        "the capture's own spell book is empty"
    );
    for s in &hud.spells {
        if let Some(d) = s.icon {
            if tex.texture_data(d).is_ok() {
                return d;
            }
        }
    }
    panic!("the capture's spell book has no spell whose icon id decodes");
}

/// **The denominators, and the join.** For every filled cell in every capture, the recipe on the
/// element must be exactly the recipe the object's own five fields name — and the counts of how
/// many cells exercise each arm are printed and asserted.
///
/// This is the assertion that would catch a recipe assembled from the wrong object, or one whose
/// overlay and underlay were swapped (they are both `Option<DataId>` and adjacent, which is exactly
/// the mistake a type checker cannot see).
///
/// Removing composite assignment in ItemListWidget::decorate leaves filled recipes absent and
/// triggers the per-object missing-recipe failure before the final filled-cell count.
#[test]
fn every_filled_cell_carries_the_recipe_its_own_five_fields_name() {
    let mut filled = 0usize;
    let mut with_effects = 0usize;
    let mut with_overlay = 0usize;
    let mut with_underlay = 0usize;
    let mut no_weenie = 0usize;
    let mut sessions = 0usize;
    for session in corpus_sessions() {
        let mut b = Bench::open(&session);
        let cells = b.cells();
        let mut here = 0usize;
        for (item, recipe) in &cells {
            let Some(id) = item else {
                assert_eq!(
                    *recipe, None,
                    "{session}: an empty cell carries a composite"
                );
                continue;
            };
            let Some(w) = b.objects.world.weenie(*id) else {
                // An item whose object is absent gets no recipe. Count this separate state
                // without treating it as a successful recipe join.
                no_weenie += 1;
                continue;
            };
            let Some(IconRecipe::Object {
                background,
                effects,
                icon,
                overlay,
                underlay,
            }) = *recipe
            else {
                panic!("{session}: {id:?} holds an object and carries no Object recipe");
            };
            // The join, resolved from the dat inside the loop rather than from a table of ids.
            let is_local_player = Some(*id) == b.objects.world.player.or(b.hud.player);
            assert_eq!(
                icon,
                if is_local_player {
                    // The local player's icon is backpack group 7, row 0x10000004, resolving
                    // public surface 0x0600127E. The environment API takes group then value; its
                    // result is independently filtered against 0x0600127E here, so a reversed
                    // mapper call in production cannot pass by agreeing with itself.
                    dereth_ui_screens::env::did_by_enum(
                        &b.ui,
                        icon_background::PLAYER_ICON_GROUP,
                        icon_background::ITEM_TYPE_GROUP,
                    )
                    .filter(|d| *d == DataId(0x0600_127E))
                } else {
                    (w.pwd.icon_id != 0).then_some(DataId(w.pwd.icon_id))
                },
                "{session}: {id:?} draws the wrong icon id"
            );
            assert_eq!(
                overlay,
                w.pwd.icon_overlay_id.map(DataId).filter(|d| d.0 != 0),
                "{session}: {id:?} draws the wrong icon overlay id"
            );
            assert_eq!(
                underlay,
                w.pwd.icon_underlay_id.map(DataId).filter(|d| d.0 != 0),
                "{session}: {id:?} draws the wrong icon underlay id"
            );
            assert_eq!(
                background,
                dereth_ui_screens::env::did_by_enum(
                    &b.ui,
                    icon_background::ITEM_TYPE_GROUP,
                    if is_local_player {
                        10
                    } else {
                        icon_background::enum_index(w.pwd.obj_type)
                    }
                ),
                "{session}: {id:?} draws the wrong UIIconBackgrounds row"
            );
            assert_eq!(
                effects,
                icon_background::effect_surface(&b.ui, w.pwd.effects.unwrap_or(0)),
                "{session}: {id:?} draws the wrong UIEffectIcons row"
            );
            assert!(
                effects.is_some(),
                "{session}: {id:?} resolves an effects surface, including the zero-effects default"
            );
            filled += 1;
            here += 1;
            if w.pwd.effects.unwrap_or(0) != 0 {
                with_effects += 1;
            }
            if overlay.is_some() {
                with_overlay += 1;
            }
            if underlay.is_some() {
                with_underlay += 1;
            }
        }
        println!(
            "{session} -- {here} filled cells of {} carry a recipe",
            cells.len()
        );
        sessions += 1;
    }
    println!(
        "{filled} filled cells across {sessions} captures -- {with_effects} carry non-zero \
         effects, {with_overlay} carry an icon overlay id, {with_underlay} carry an \
         icon underlay id; {no_weenie} named an object with no weenie"
    );
    // The corpus index's own count, not an integer to re-type.
    assert_eq!(
        sessions,
        recorded_sessions(),
        "every recorded capture was walked"
    );
    // **The denominators, asserted rather than printed**, so no claim above is over nothing.
    assert!(filled > 0, "no recording fills a cell");
    assert!(
        with_effects > 0,
        "no corpus cell exercises the effects ring"
    );
    assert!(
        with_overlay > 0,
        "no corpus cell exercises the icon overlay"
    );
    assert!(
        with_underlay > 0,
        "no corpus cell exercises the icon underlay"
    );
}

/// Behaviour: inventory.icons.every-spell-in-the-book-draws-its-background-row
///
/// Every spell in first-login-walk-jump's recorded book must acquire a background recipe, not
/// only the spell's base icon. Removing the recipe argument from ItemSlot::set_spell fails the per-slot recipe expectation.
/// Expected full recipes use spell_recipe, so independent row/flag tests above establish its
/// arithmetic; this check establishes installation and nonempty background/tint coverage.
#[test]
fn every_spell_in_the_captures_book_draws_a_spell_background_row() {
    // Hud::drive populates the spellbook panel through the application's normal producer.
    // The test does not substitute manually assembled list rows for that delivery.
    let mut b = Bench::open("first-login-walk-jump");
    let book = b.hud.spells.clone();
    assert!(
        !book.is_empty(),
        "the capture's 0x0013 carries a spell book"
    );
    b.hud
        .drive(&mut b.ui, as_gameplay(&mut b.screen), 1, &b.objects);
    let ui = &b.ui;
    let list = b
        .hud
        .panels
        .spellbook
        .list
        .as_ref()
        .expect("the spellbook list");
    let mut drawn = 0usize;
    let mut tinted = 0usize;
    let mut badged = 0usize;
    for s in &list.slots {
        let Some(spell) = s.spell else { continue };
        let e = book
            .iter()
            .find(|e| e.id == spell)
            .expect("the slot holds a spell in the book");
        let recipe = s.icon_recipe(ui).expect("a spell slot carries a composite");
        assert_eq!(
            recipe,
            spell_recipe(ui, e.icon_power, e.icon, e.bitfield),
            "spell {spell} draws the wrong recipe"
        );
        let IconRecipe::Spell {
            background,
            tint,
            overlay,
            ..
        } = recipe
        else {
            panic!("spell {spell} carries an Object recipe");
        };
        assert!(
            background.is_some(),
            "spell {spell} (level {}) has no background row",
            e.level
        );
        drawn += 1;
        if tint.is_some() {
            tinted += 1;
        }
        if overlay.is_some() {
            badged += 1;
        }
    }
    println!(
        "{drawn} of {} spells in the capture's book draw a UISpellBackgrounds row; \
         {tinted} carry a wash, {badged} a fellowship/self badge",
        book.len()
    );
    assert_eq!(drawn, book.len(), "a spell in the book drew no composite");
    assert_eq!(
        tinted, drawn,
        "the spell compositor supplies a white-replacement tint on every spell"
    );
}

/// Unchanged objects must produce equal recipes, the keys used by the texture cache.
/// The icon update compares the same five fields before rebuilding. Here a second
/// decoration pass must leave the full cell list equal, and filled cells must contain more than
/// one distinct recipe. This is a stability/diversity check, not a mutation of every field or a
/// direct measurement of cache rebuilds; removing one field need not collapse all recipes.
#[test]
fn an_unchanged_object_produces_an_equal_recipe_and_a_changed_one_does_not() {
    let mut b = Bench::open("first-login-walk-jump");
    let first = b.cells();
    b.hud
        .drive(&mut b.ui, as_gameplay(&mut b.screen), 2, &b.objects);
    let second = b.cells();
    assert_eq!(
        first, second,
        "a second decoration pass changed a recipe with no object change"
    );

    let mut seen: BTreeMap<IconRecipe, usize> = BTreeMap::new();
    for (item, r) in &first {
        if item.is_some() {
            if let Some(r) = r {
                *seen.entry(*r).or_default() += 1;
            }
        }
    }
    println!(
        "first-login-walk-jump's filled cells resolve {} distinct recipes",
        seen.len()
    );
    assert!(
        seen.len() > 1,
        "every cell has the same recipe, so equality proves nothing here"
    );
}

/// Clearing a slot replaces its icon-child composite through the authored empty state: state
/// 0x1000001C selects frame 0x06004D20 on the icon child. Slot clearing resets four data fields;
/// element changes come through state handling.
/// This test flushes the grid and checks each remaining slot has no item or composite, with a
/// positive slot denominator. It does not independently compare the selected empty-frame id.
/// Removing the empty-state transition also breaks the backpack grid's side-pack clearing control.
#[test]
fn clearing_a_slot_puts_the_empty_frame_back_over_the_composite_with_no_help() {
    let mut b = Bench::open("first-login-walk-jump");
    let before = b.cells();
    let backed = before
        .iter()
        .filter(|(i, r)| i.is_some() && r.is_some())
        .count();
    assert!(
        backed > 0,
        "first-login-walk-jump has no composited cell to clear"
    );

    // Flush the grid through its normal empty-contents update. Other inventory lists remain
    // untouched; only the flushed grid's slots are asserted below.
    {
        let g = as_gameplay(&mut b.screen);
        let p = &mut g.inventory;
        for w in p.item_list.iter_mut() {
            w.set_contents(&mut b.ui, None, None, &[], &|_| None);
        }
    }
    let after = b.cells();
    let still: Vec<_> = after
        .iter()
        .enumerate()
        .filter(|(_, (i, r))| i.is_none() && r.is_some())
        .collect();
    println!(
        "{backed} composited cells before the flush, {} cells still carry one after",
        still.len()
    );
    // Only the lists actually flushed can be asserted on; the doll and the strips were untouched.
    let g = as_gameplay(&mut b.screen);
    let grid = g.inventory.item_list.as_ref().expect("the inventory grid");
    let handles: Vec<_> = grid.slots.iter().map(|s| (s.item, s.icon)).collect();
    let mut checked = 0usize;
    for (item, icon) in handles {
        assert_eq!(item, None, "the flushed grid still holds an object");
        let recipe = icon
            .and_then(|h| b.ui.node(h))
            .and_then(|n| n.region.image.as_ref())
            .and_then(|g| g.op)
            .and_then(SurfaceOp::icon_recipe);
        assert_eq!(recipe, None, "a cleared cell kept its composite");
        checked += 1;
    }
    println!("{checked} cells of the flushed grid have no composite");
    assert!(checked > 0, "the grid has no cells, so nothing was checked");
}

/// `ItemSlot` is re-exported so the two helpers above compile against the same type the widget
/// crate uses; this keeps the import honest rather than `#[allow(unused)]`.
#[allow(dead_code)]
fn _type_check(_: &ItemSlot) {}

/// Behaviour: spellbar.icons.raw-power-background
#[test]
fn spell_bar_backgrounds_use_runtime_raw_power_before_display_level_collapse() {
    let store = open_store();
    let mut hud = dereth_client::hud::Hud::new();
    let mut objects = dereth_client::objects::ObjectStream::new();
    hud.load_tables(&store, &objects.world);
    let ids: Vec<_> = [0x6e, 0x70, 0xc0, 0xc1]
        .into_iter()
        .map(|component| {
            *hud.spell_table
                .as_ref()
                .unwrap()
                .spells
                .iter()
                .find(|(_, b)| {
                    dereth_client_contract::spellbook::power_component(b.raw_comps[0], b.comp_key)
                        == component
                        && b.icon != 0
                })
                .unwrap()
                .0
        })
        .collect();
    hud.spells = ids.iter().map(|id| hud.spell_entry(*id).unwrap()).collect();
    objects.world.player_system.spell_tabs[0] = ids;
    let (mut ui, screen) = shipped_gameplay();
    let mut bar = dereth_ui_screens::panels::spellcasting::SpellcastingPanel::default();
    bar.post_init(&mut ui, screen.roots()[0]);
    bar.update(&mut ui, &hud.view(&objects));
    for (i, (power, level, background)) in [
        (7, 6, 0x060013f6),
        (8, 7, 0x06001f63),
        (9, 7, 0x060013f6),
        (10, 8, 0x060067a6),
    ]
    .into_iter()
    .enumerate()
    {
        let entry = &hud.spells[i];
        assert_eq!((entry.icon_power, entry.level), (power, level));
        let recipe = bar.lists[0].as_ref().unwrap().slots[i]
            .icon_recipe(&ui)
            .unwrap();
        assert!(
            matches!(recipe, IconRecipe::Spell { background: Some(id), .. } if id == DataId(background)),
            "{recipe:?}"
        );
    }
}
