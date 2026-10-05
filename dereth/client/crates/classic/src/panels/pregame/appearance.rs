use super::*;
use crate::int::{i32_from, u32_from};
use dereth_chargen::palette::{PaletteLayout, PaletteSample};
use dereth_primitives::num::to_i32_f64;

fn color(c: [u8; 4]) -> u32 {
    0xff000000 | (u32::from(c[0]) << 16) | (u32::from(c[1]) << 8) | u32::from(c[2])
}
fn palette(d: &CreationData, id: u32) -> Option<&[[u8; 4]]> {
    d.appearance
        .palettes
        .get(&format!("{id:08X}"))
        .map(Vec::as_slice)
}
fn set_palette(d: &CreationData, id: u32, shade: f64) -> Option<&[[u8; 4]]> {
    let set = d.appearance.palette_sets.get(&format!("{id:08X}"))?;
    if set.is_empty() || !(0.0..=1.0).contains(&shade) {
        return None;
    }
    palette(
        d,
        set[usize::try_from(to_i32_f64((set.len() as f64 - 0.000001) * shade)).unwrap_or(0)],
    )
}
fn combined_palette(d: &CreationData, state: &SelectionView<'_>) -> Vec<[u8; 4]> {
    let sex = state.sex(d);
    let mut result = palette(d, sex.base_palette).map_or(vec![[0, 0, 0, 255]; 256], |v| v.to_vec());
    let hair = sex
        .hair_colors
        .get(state.hair_color)
        .and_then(|&id| set_palette(d, id, state.hair_shade));
    let eyes = sex
        .eye_colors
        .get(state.eye_color)
        .and_then(|&id| palette(d, id));
    for (start, end, source) in [
        (0, 24, set_palette(d, sex.skin_palette, state.skin_shade)),
        (24, 32, hair),
        (32, 40, eyes),
    ] {
        if let Some(p) = source {
            let scale = if result.len() == 2048 { 8 } else { 1 };
            let range = start * scale..end * scale;
            if let (Some(to), Some(from)) = (result.get_mut(range.clone()), p.get(range)) {
                to.copy_from_slice(from);
            }
        }
    }
    result
}
/// One entry of each palette in a palette set, in the set's order.
fn set_entry(d: &CreationData, set: u32, sample: PaletteSample) -> Vec<[u8; 4]> {
    d.appearance
        .palette_sets
        .get(&format!("{set:08X}"))
        .map_or_else(Vec::new, |ids| {
            ids.iter()
                .filter_map(|&id| {
                    palette(d, id).and_then(|v| {
                        let layout = PaletteLayout::from_entry_count(v.len())?;
                        v.get(sample.index(layout))
                    })
                })
                .copied()
                .collect()
        })
}
/// The palette entry a clothing slot's colour swatches and shade bar are drawn from: headgear,
/// shirt, trousers, footwear.
const CLOTHING_SAMPLE: [PaletteSample; 4] = [
    PaletteSample::Headgear,
    PaletteSample::Shirt,
    PaletteSample::Trousers,
    PaletteSample::Footwear,
];
/// The shade sliders' tracks: headgear, shirt, trousers, footwear.
const CLOTHING_TRACK: [&str; 4] = ["06000502", "06000504", "06000503", "06000505"];
// A colour swatch interpolates representative palette entries across its width.
fn gradient(
    f: &mut PanelFrame,
    d: &CreationData,
    set: u32,
    sample: PaletteSample,
    r: crate::widgets::Rect,
    vertical: bool,
) {
    let colors = set_entry(d, set, sample);
    if colors.is_empty() {
        return;
    }
    let extent = if vertical { r.h } else { r.w };
    for i in 0..extent {
        let p = i as f64 * (colors.len() - 1) as f64 / (extent - 1).max(1) as f64;
        let a = usize::try_from(to_i32_f64(p.floor())).unwrap_or(0);
        let b = (a + 1).min(colors.len() - 1);
        let t = p - a as f64;
        let c = std::array::from_fn(|k| {
            u8::try_from(to_i32_f64(
                (1. - t) * f64::from(colors[a][k]) + t * f64::from(colors[b][k]),
            ))
            .unwrap_or(u8::MAX)
        });
        f.fill(
            if vertical {
                rect(r.x, r.y + i, r.w, 1)
            } else {
                rect(r.x + i, r.y, 1, r.h)
            },
            color(c),
        );
    }
}

impl Pregame {
    /// The clothing page: per slot a style dropdown, a strip of colour swatches (four in view,
    /// scrolled a swatch at a time) and a shade bar with its slider.
    pub(super) fn clothing_frame(&self, f: &mut PanelFrame, d: &CreationData) {
        const SWATCH: i32 = 32;
        let state = self.view(d);
        let sex = state.sex(d);
        for (i, name) in ["Headgear:", "Shirt:", "Trousers:", "Footwear:"]
            .into_iter()
            .enumerate()
        {
            let y = 201 + i32_from(i) * 72;
            f.label(368, y, name, "16-7", COLOR, None);
            let options = sex.clothes(i).iter().map(|v| v.name.clone());
            let options = if i == 0 {
                std::iter::once("None".into()).chain(options).collect()
            } else {
                options.collect()
            };
            art_choice(
                f,
                format!("style-{i}"),
                368,
                y + 20,
                options,
                if i == 0 {
                    state.styles[i].wrapping_add(1)
                } else {
                    state.styles[i]
                },
            );
            let colors = state.clothing_colors(d, i);
            let entry = CLOTHING_SAMPLE[i];
            let strip = rect(520, y + 20, 4 * SWATCH, SWATCH);
            let max = (i32_from(colors.len()) * SWATCH - strip.w).max(0);
            let offset = self.color_scroll[i].min(max);
            let first = usize::try_from(offset / SWATCH).unwrap_or(0);
            for (k, color) in colors.iter().enumerate().skip(first).take(4) {
                let r = rect(
                    strip.x + i32_from(k - first) * SWATCH,
                    strip.y,
                    SWATCH,
                    SWATCH,
                );
                gradient(f, d, color.palette_set, entry, r, true);
                if k == state.colors[i] {
                    f.image_native("06000506", r.x, r.y, r, true);
                }
                let pick = f.button(format!("color-pick-{i}-{k}"), r, "", true);
                pick.paint = false;
                // The wheel over the swatches moves the strip a swatch a notch.
                pick.wheel_bar = Some((format!("color-scroll-{i}"), 1));
            }
            f.control(
                format!("color-scroll-{i}"),
                rect(strip.x, strip.y + SWATCH, strip.w, 20),
                ControlKind::ScrollBar {
                    min: 0,
                    max,
                    value: offset,
                    page: strip.w,
                    step: SWATCH,
                    vertical: false,
                    arrow_size: 20,
                    thumb_size: 20,
                },
                max > 0,
            );
            // The shade: the chosen colour's palettes side by side, and a slider under them.
            if let Some(color) = colors.get(state.colors[i]) {
                gradient(
                    f,
                    d,
                    color.palette_set,
                    entry,
                    rect(656, y + 20, 133, 25),
                    false,
                );
            }
            let track = rect(656, y + 45, 133, 25);
            f.image_native(CLOTHING_TRACK[i], track.x, track.y, track, false);
            f.slider(
                format!("shade-{i}"),
                track,
                0,
                1000,
                to_i32_f64(state.shades[i] * 1000.),
                1,
            )
            .paint = false;
            f.image(
                "06000501",
                rect(
                    track.x + to_i32_f64(f64::from(track.w - 23) * state.shades[i]),
                    track.y + 1,
                    23,
                    23,
                ),
                false,
                true,
            );
        }
    }
    pub(super) fn appearance_frame(&self, f: &mut PanelFrame, d: &CreationData) {
        let state = self.view(d);
        let sex = state.sex(d);
        let pal = combined_palette(d, &state);
        f.image("0600028C", rect(368, 177, 421, 140), false, false);
        f.label(390, 160, "Facial Features", "16-7", COLOR, None);
        let bald = sex
            .hair_styles
            .get(state.hair_style)
            .is_some_and(|h| h.bald != 0);
        for (part, y, h, choices) in [
            (0, 199, 40, &sex.eyes),
            (1, 239, 12, &sex.noses),
            (2, 251, 48, &sex.mouths),
        ] {
            for cell in 0..5 {
                let index = state.face[part] as isize + cell as isize - 2;
                let r = rect(417 + cell * 64, y, 64, h);
                if let Some(strip) = usize::try_from(index).ok().and_then(|i| choices.get(i)) {
                    let did = if part == 0 && bald && !strip.bald_texture.is_empty() {
                        &strip.bald_texture
                    } else {
                        &strip.texture
                    };
                    if !did.is_empty() {
                        f.screen.commands.push(crate::Command::IndexedImage {
                            did: did.clone(),
                            palette: pal.clone(),
                            x: r.x,
                            y: r.y,
                            width: r.w as u32,
                            height: r.h as u32,
                            clip: Some([417, y, 737, y + h]),
                            flip_x: false,
                        });
                    }
                    if cell != 2 {
                        f.image(
                            &format!("{:08X}", 0x0600027f + u32_from(part)),
                            r,
                            false,
                            true,
                        );
                    }
                    f.button(format!("face-pick-{part}-{index}"), r, "", true)
                        .paint = false;
                } else {
                    f.image(
                        &format!("{:08X}", 0x060002b4 + u32_from(part)),
                        r,
                        false,
                        false,
                    );
                }
            }
            let (ly, lh, ry, rw, rh, left, right) = match part {
                0 => (200, 21, 200, 20, 19, 0x060002a5, 0x060002ad),
                1 => (235, 20, 233, 19, 21, 0x060002a9, 0x060002b1),
                _ => (273, 20, 271, 19, 21, 0x060002a7, 0x060002af),
            };
            art_button(
                f,
                &format!("face-prev-{part}"),
                rect(399, ly, 16, lh),
                [left, left + 1, left],
                state.face[part] > 0 && state.face[part] < choices.len(),
            );
            art_button(
                f,
                &format!("face-next-{part}"),
                rect(737, ry, rw, rh),
                [right, right + 1, right],
                state.face[part].saturating_add(1) < choices.len(),
            );
        }
        art_button(
            f,
            "face-prev-all",
            rect(369, 196, 24, 102),
            [0x060002b3, 0x060002a4, 0x060002b3],
            true,
        );
        art_button(
            f,
            "face-next-all",
            rect(759, 195, 29, 102),
            [0x060002ab, 0x060002ac, 0x060002ab],
            true,
        );
        for (id, label, x, y, set, entry, shade) in [
            (
                "skin-shade",
                "Skin Color",
                381,
                319,
                sex.skin_palette,
                PaletteSample::Skin,
                state.skin_shade,
            ),
            (
                "hair-shade",
                "Hair Shade",
                381,
                461,
                sex.hair_colors.get(state.hair_color).copied().unwrap_or(0),
                PaletteSample::Hair,
                state.hair_shade,
            ),
        ] {
            f.label(x, y, label, "16-7", COLOR, None);
            gradient(f, d, set, entry, rect(x, y + 20, 176, 25), false);
            f.slider(
                id,
                rect(x, y + 20, 192, 25),
                0,
                1000,
                to_i32_f64(shade * 1000.),
                1,
            )
            .paint = false;
            f.image(
                "06000501",
                rect(x + to_i32_f64(169. * shade), y + 21, 23, 23),
                false,
                true,
            );
        }
        // The swatch and hairstyle frames mark the selected choice.
        f.label(381, 387, "Hair Color", "16-7", COLOR, None);
        let first = state.hair_color.saturating_sub(5);
        for (cell, (i, &set)) in sex
            .hair_colors
            .iter()
            .enumerate()
            .skip(first)
            .take(6)
            .enumerate()
        {
            let r = rect(381 + i32_from(cell) * 32, 407, 32, 32);
            gradient(f, d, set, PaletteSample::Hair, r, true);
            if i == state.hair_color {
                f.image("06000506", r, false, true);
            }
            f.button(format!("hair-color-pick-{i}"), r, "", true).paint = false;
        }
        if sex.hair_colors.len() > 6 {
            f.slider(
                "hair-color",
                rect(381, 440, 192, 19),
                0,
                i32_from(sex.hair_colors.len().saturating_sub(1)),
                i32_from(state.hair_color),
                1,
            );
        }
        f.label(586, 319, "Eye Color", "16-7", COLOR, None);
        let start = state.eye_color.saturating_sub(5);
        for (cell, (i, &id)) in sex
            .eye_colors
            .iter()
            .enumerate()
            .skip(start)
            .take(6)
            .enumerate()
        {
            let r = rect(586 + i32_from(cell) * 32, 339, 32, 32);
            if let Some(c) = palette(d, id).and_then(|p| {
                let layout = PaletteLayout::from_entry_count(p.len())?;
                p.get(PaletteSample::Eyes.index(layout))
            }) {
                f.fill(r, color(*c));
            }
            if i == state.eye_color {
                f.image("06000506", r, false, true);
            }
            f.button(format!("eye-color-pick-{i}"), r, "", true).paint = false;
        }
        f.slider(
            "eye-color",
            rect(586, 371, 192, 20),
            0,
            i32_from(sex.eye_colors.len().saturating_sub(1)),
            i32_from(state.eye_color),
            1,
        );
        f.label(586, 387, "Hairstyle", "16-7", COLOR, None);
        let mut icons = std::collections::BTreeSet::new();
        let numbered = sex
            .hair_styles
            .iter()
            .any(|style| !icons.insert(style.icon));
        let rows = i32_from(sex.hair_styles.len().div_ceil(4));
        let max = (rows * 48 - 96).max(0);
        let first = usize::try_from(self.hair_scroll.min(max) / 48).unwrap_or(0) * 4;
        for (cell, (i, hair)) in sex
            .hair_styles
            .iter()
            .enumerate()
            .skip(first)
            .take(8)
            .enumerate()
        {
            let r = rect(
                586 + i32_from(cell % 4) * 48,
                407 + i32_from(cell / 4) * 48,
                48,
                48,
            );
            if numbered {
                f.fill(
                    r,
                    if i == state.hair_style {
                        0xff493921
                    } else {
                        0xff17120d
                    },
                );
                f.text_box(
                    r,
                    (i + 1).to_string(),
                    "20-8",
                    COLOR,
                    TextAlign::Center,
                    false,
                    None,
                );
            } else {
                f.image(&format!("world:{:08X}", hair.icon), r, false, false);
            }
            if i == state.hair_style {
                f.image("06000F51", r, false, true);
            }
            let pick = f.button(format!("hair-style-pick-{i}"), r, "", true);
            pick.paint = false;
            // The wheel over the styles moves the grid a row a notch.
            pick.wheel_bar = Some(("hairstyles-scroll".into(), 1));
        }
        if max > 0 {
            f.control(
                "hairstyles-scroll",
                rect(780, 407, 16, 96),
                ControlKind::ScrollBar {
                    min: 0,
                    max,
                    value: self.hair_scroll.min(max),
                    page: 96,
                    step: 48,
                    vertical: true,
                    arrow_size: 16,
                    thumb_size: 16,
                },
                true,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    #[test]
    fn palette_endpoint_and_half_boundaries_use_retail_epsilon() {
        let mut d = super::super::tests::data();
        d.appearance
            .palette_sets
            .insert("0F000001".into(), vec![0x04000001, 0x04000002]);
        d.appearance
            .palettes
            .insert("04000001".into(), vec![[1, 2, 3, 255]; 256]);
        d.appearance
            .palettes
            .insert("04000002".into(), vec![[4, 5, 6, 255]; 256]);
        assert_eq!(set_palette(&d, 0x0f000001, 0.5).unwrap()[0], [1, 2, 3, 255]);
        assert_eq!(set_palette(&d, 0x0f000001, 1.).unwrap()[0], [4, 5, 6, 255]);
        assert!(set_palette(&d, 0x0f000001, 1.01).is_none());
    }
    #[test]
    fn clothing_swatch_runs_top_to_bottom_through_its_slots_entry_of_each_palette_in_the_set() {
        let mut d = super::super::tests::data();
        d.appearance
            .palette_sets
            .insert("0F000002".into(), vec![0x0400_0010, 0x0400_0011]);
        for (id, shade) in [
            ("04000010", [10, 20, 30, 255]),
            ("04000011", [50, 60, 70, 255]),
        ] {
            let mut p = vec![[0, 0, 0, 255]; 256];
            p[PaletteSample::Shirt.index(PaletteLayout::Indexed256)] = shade;
            d.appearance.palettes.insert(id.into(), p);
        }
        let d = std::rc::Rc::new(d);
        let mut p = Pregame::new("clothing", Ok(d.clone()), dereth_primitives::LocalTime(0.0));
        p.state.set_shirt_style(&d.tables.chargen, 0);
        p.state.shirt_color = 0;
        p.state.shirt_palette_template_ids = vec![1];
        p.state.shirt_pal_set_ids = vec![DataId(0x0f000002)];
        let mut f = PanelFrame::new(800, 600);
        p.clothing_frame(&mut f, &d);
        let fill = |x: i32, y: i32, w: u32| {
            f.screen.commands.iter().find_map(|c| match c {
                crate::Command::Fill {
                    x: fx,
                    y: fy,
                    width,
                    color,
                    ..
                } if *fx == x && *fy == y && *width == w => Some(*color),
                _ => None,
            })
        };
        // The shirt row's swatch: 32 one-pixel rows from the first palette's entry to the last's.
        assert_eq!(fill(520, 293, 32), Some(0xff0a_141e));
        assert_eq!(fill(520, 324, 32), Some(0xff32_3c46));
        // Its shade bar runs the same way from left to right.
        assert_eq!(fill(656, 293, 1), Some(0xff0a_141e));
        assert_eq!(fill(788, 293, 1), Some(0xff32_3c46));
    }
    #[test]
    fn clothing_colour_strip_scrolls_a_whole_swatch_and_picks_by_table_index() {
        let d = super::super::tests::data();
        let d = std::rc::Rc::new(d);
        super::super::tests::context(|c| {
            let mut p = Pregame::new("clothing", Ok(d.clone()), dereth_primitives::LocalTime(0.0));
            p.state.set_shirt_style(&d.tables.chargen, 0);
            p.event(
                ControlEvent::Scroll {
                    id: "color-scroll-1".into(),
                    value: 40,
                },
                c,
            );
            assert_eq!(p.color_scroll[1], 32);
            let mut f = PanelFrame::new(800, 600);
            p.clothing_frame(&mut f, &d);
            let picks: Vec<_> = f
                .controls
                .iter()
                .filter(|c| c.id.starts_with("color-pick-1-"))
                .map(|c| (c.id.as_str(), c.rect.x))
                .collect();
            assert_eq!(
                picks,
                [
                    ("color-pick-1-1", 520),
                    ("color-pick-1-2", 552),
                    ("color-pick-1-3", 584),
                    ("color-pick-1-4", 616)
                ]
            );
            p.event(ControlEvent::Activate("color-pick-1-4".into()), c);
            assert_eq!(p.state.shirt_color, 4);
        });
    }
}
