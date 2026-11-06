//! How pictures reach the layers, against a plain working out of each texel: a picture put down in
//! each of its ways, and a top-level element turned.
//!
//! Behaviour: none (the same texels by a quicker way, which no behavior of the game names)

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::sync::Arc;

use auberean_ui::draw::{self, Blit, Surface};
use auberean_ui::{Image, Rect, Token};

use crate::fixture::{element, image, state, ui, LAYOUT};

/// A picture whose texels run through every alpha.
fn picture(width: u32, height: u32) -> Image {
    let mut bgra = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let k = x * 7 + y * 13;
            bgra.extend([
                (k * 3) as u8,
                (k * 5 + 40) as u8,
                (k * 11 + 90) as u8,
                match k % 5 {
                    0 => 0,
                    1 => 255,
                    _ => (k * 17) as u8,
                },
            ]);
        }
    }
    Image {
        width,
        height,
        bgra,
        alpha_only: false,
    }
}

/// A layer already holding something, some of it clear.
fn background(width: i32, height: i32) -> Surface {
    let mut s = Surface::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let k = (x * 3 + y * 5) as u8;
            s.put(x, y, [k, k.wrapping_mul(3), 200 - k / 2, k.wrapping_mul(7)]);
        }
    }
    s
}

fn blit_plain(dst: &mut Surface, x: i32, y: i32, s: [u8; 4], how: Blit) {
    match how {
        Blit::Copy => dst.put(x, y, s),
        Blit::CopyFaded(a) => {
            let faded = (f32::from(s[3]) * a.clamp(0.0, 1.0)) as u8;
            dst.put(x, y, [s[0], s[1], s[2], faded]);
        }
        Blit::Blend => {
            if s[3] == 0xFF {
                dst.put(x, y, s);
            } else if s[3] != 0 {
                let d = dst.get(x, y);
                let a = i32::from(s[3]);
                let mix = |dc: u8, sc: u8| {
                    let dc = i32::from(dc);
                    u8::try_from(dc - (dc - i32::from(sc)) * a / 256).unwrap_or(0)
                };
                let t = [mix(d[0], s[0]), mix(d[1], s[1]), mix(d[2], s[2]), d[3]];
                dst.put(x, y, t);
            }
        }
    }
}

/// The picture's texels put down one by one.
fn draw_plain(
    dst: &mut Surface,
    img: &Image,
    area: Rect,
    clip: Rect,
    how: Blit,
    stretch: bool,
    center: bool,
) {
    let (iw, ih) = (img.width as i32, img.height as i32);
    let Some(c) = area.intersect(clip) else {
        return;
    };
    let (bw, bh) = (area.width(), area.height());
    if center && !(iw == bw && ih == bh) {
        let x0 = area.x0 + ((bw - iw) / 2).max(0);
        let y0 = area.y0 + ((bh - ih) / 2).max(0);
        let Some(c) = Rect::new(x0, y0, x0 + iw - 1, y0 + ih - 1).intersect(c) else {
            return;
        };
        for y in c.y0..=c.y1 {
            for x in c.x0..=c.x1 {
                let t = img.texel((x - x0) as u32, (y - y0) as u32);
                blit_plain(dst, x, y, t, how);
            }
        }
        return;
    }
    if (iw == bw && ih == bh) || !stretch {
        for y in c.y0..=c.y1 {
            let sy = (y - area.y0).rem_euclid(ih) as u32;
            for x in c.x0..=c.x1 {
                let sx = (x - area.x0).rem_euclid(iw) as u32;
                blit_plain(dst, x, y, img.texel(sx, sy), how);
            }
        }
        return;
    }
    for y in c.y0..=c.y1 {
        let sy = i64::from(y - area.y0) * i64::from(ih) / i64::from(bh);
        for x in c.x0..=c.x1 {
            let sx = i64::from(x - area.x0) * i64::from(iw) / i64::from(bw);
            let t = img.texel(
                (sx as u32).min(img.width - 1),
                (sy as u32).min(img.height - 1),
            );
            blit_plain(dst, x, y, t, how);
        }
    }
}

/// A picture put down in each way, over areas that fit it, are larger and smaller, reach off the
/// layer and are clipped, gives the texels each worked out alone gives.
#[test]
fn a_picture_put_down_is_the_texels_each_worked_out_alone() {
    let img = picture(16, 11);
    let hows = [
        Blit::Copy,
        Blit::Blend,
        Blit::CopyFaded(0.4),
        Blit::CopyFaded(2.0),
    ];
    let areas = [
        Rect::sized(10, 12, 16, 11),
        Rect::sized(4, 5, 50, 40),
        Rect::sized(8, 9, 7, 5),
        Rect::sized(-6, -3, 30, 20),
        Rect::sized(50, 40, 30, 30),
        Rect::sized(5, 5, 1, 1),
    ];
    let clips = [
        Rect::sized(0, 0, 64, 48),
        Rect::sized(12, 14, 9, 6),
        Rect::sized(-10, -10, 200, 200),
        Rect::sized(60, 44, 3, 3),
    ];
    let mut compared = 0;
    for how in hows {
        for area in areas {
            for clip in clips {
                for (stretch, center) in [(false, false), (true, false), (false, true)] {
                    let mut quick = background(64, 48);
                    let mut plain = background(64, 48);
                    draw::draw_image(&mut quick, &img, area, clip, how, stretch, center);
                    draw_plain(&mut plain, &img, area, clip, how, stretch, center);
                    assert_eq!(
                        quick.bgra, plain.bgra,
                        "{how:?} over {area:?} in {clip:?}, stretch {stretch}, center {center}"
                    );
                    compared += 1;
                }
            }
        }
    }
    assert_eq!(compared, 4 * 6 * 4 * 3);
}

/// A layer turned, texel by texel, as the plain working out does it.
fn turned_plain(layer: &Surface, out: &mut Surface, area: Rect, degrees: i32) {
    let a = (degrees as f32).to_radians();
    let (sin, cos) = (
        dereth_primitives::num::math::sinf(a),
        dereth_primitives::num::math::cosf(a),
    );
    let cx = (area.x0 + area.x1) as f32 / 2.0 + 0.5;
    let cy = (area.y0 + area.y1) as f32 / 2.0 + 0.5;
    let (hw, hh) = (area.width() as f32 / 2.0, area.height() as f32 / 2.0);
    let rw = hw * cos.abs() + hh * sin.abs();
    let rh = hw * sin.abs() + hh * cos.abs();
    let covers = Rect::new(
        (cx - rw).floor() as i32,
        (cy - rh).floor() as i32,
        (cx + rw).ceil() as i32,
        (cy + rh).ceil() as i32,
    );
    let Some(c) = covers.intersect(layer.bounds()) else {
        return;
    };
    for y in c.y0..=c.y1 {
        for x in c.x0..=c.x1 {
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            let sx = dx * cos - dy * sin + cx - 0.5;
            let sy = dx * sin + dy * cos + cy - 0.5;
            let (x0, y0) = (sx.floor(), sy.floor());
            let (fx, fy) = (sx - x0, sy - y0);
            let (ix, iy) = (x0 as i32, y0 as i32);
            let mut acc = [0.0f32; 4];
            for (ox, oy, w) in [
                (0, 0, (1.0 - fx) * (1.0 - fy)),
                (1, 0, fx * (1.0 - fy)),
                (0, 1, (1.0 - fx) * fy),
                (1, 1, fx * fy),
            ] {
                let (nx, ny) = (ix + ox, iy + oy);
                let t = if area.contains(nx, ny) {
                    layer.get(nx, ny)
                } else {
                    [0; 4]
                };
                let alpha = f32::from(t[3]) * w;
                acc[0] += f32::from(t[0]) * alpha;
                acc[1] += f32::from(t[1]) * alpha;
                acc[2] += f32::from(t[2]) * alpha;
                acc[3] += alpha;
            }
            if acc[3] <= 0.0 {
                continue;
            }
            let px = |v: f32| (v / acc[3]).round().clamp(0.0, 255.0) as u8;
            let alpha = acc[3].round().clamp(0.0, 255.0) as u8;
            // Laid over the clear picture, a texel with no alpha leaves it clear.
            if alpha != 0 {
                out.put(x, y, [px(acc[0]), px(acc[1]), px(acc[2]), alpha]);
            }
        }
    }
}

/// A top-level element turned by any angle is the texels each worked out alone gives, and a kept
/// set of layers leaves the next picture as a new set would.
#[test]
fn a_turned_element_is_the_texels_each_worked_out_alone() {
    let e = element(
        "Turned",
        "field",
        [20, 15, 79, 58],
        1,
        vec![state("stateless", 0, vec![], vec![image(1, 0)], vec![])],
    );
    let mut u = ui(vec![e], vec![(1, Arc::new(picture(60, 44)))]);
    let l = u.load_layout(LAYOUT).unwrap();
    let h = u.create_root(l, Token::of("Turned")).unwrap();
    let mut unturned = Surface::new(1024, 768);
    draw::draw_layers_where(&u, &mut unturned, |_| true);
    assert!(unturned.bgra.iter().any(|b| *b != 0), "it is drawn");
    let mut kept = draw::Layers::default();
    for degrees in [1, 30, 45, 90, 137, 180, 271, 359] {
        u.set_rotation(h, degrees);
        let mut plain = Surface::new(1024, 768);
        turned_plain(&unturned, &mut plain, Rect::new(20, 15, 79, 58), degrees);
        let mut quick = Surface::new(1024, 768);
        draw::draw_layers_with(&u, &mut quick, |_| true, &mut kept);
        assert_eq!(quick.bgra, plain.bgra, "turned {degrees}");
    }
    u.set_rotation(h, 0);
    let mut again = Surface::new(1024, 768);
    draw::draw_layers_with(&u, &mut again, |_| true, &mut kept);
    assert_eq!(again.bgra, unturned.bgra, "the kept layers were left clear");
}

/// A top-level element drawn at a scale covers that share of its size from its top-left corner,
/// its middle keeping its colors, and scaling back leaves the next picture as it was.
#[test]
fn a_scaled_element_covers_its_share_from_its_corner() {
    let e = element(
        "Scaled",
        "field",
        [20, 15, 79, 58],
        1,
        vec![state("stateless", 0, vec![], vec![image(1, 0)], vec![])],
    );
    let mut u = ui(vec![e], vec![(1, Arc::new(picture(60, 44)))]);
    let l = u.load_layout(LAYOUT).unwrap();
    let h = u.create_root(l, Token::of("Scaled")).unwrap();
    let bounds = |s: &Surface| {
        let (mut lo, mut hi) = ((i32::MAX, i32::MAX), (-1, -1));
        for y in 0..s.height {
            for x in 0..s.width {
                if s.get(x, y)[3] != 0 {
                    lo = (lo.0.min(x), lo.1.min(y));
                    hi = (hi.0.max(x), hi.1.max(y));
                }
            }
        }
        (lo, hi)
    };
    let mut plain = Surface::new(1024, 768);
    draw::draw_layers_where(&u, &mut plain, |_| true);
    assert_eq!(bounds(&plain), ((20, 15), (79, 58)));
    let mut kept = draw::Layers::default();
    for (scale, want) in [(2.0, (139, 102)), (0.5, (49, 36))] {
        u.set_object_scale(h, scale);
        let mut drawn = Surface::new(1024, 768);
        draw::draw_layers_with(&u, &mut drawn, |_| true, &mut kept);
        assert_eq!(bounds(&drawn), ((20, 15), want), "at {scale}");
    }
    u.set_object_scale(h, 1.0);
    let mut again = Surface::new(1024, 768);
    draw::draw_layers_with(&u, &mut again, |_| true, &mut kept);
    assert_eq!(again.bgra, plain.bgra, "the kept layers were left clear");
}

/// Saying what is already so draws nothing new: the same attribute, visibility, place or picture,
/// and the clock passing over text no one can edit, leave the picture as it was. A change does
/// not.
#[test]
fn saying_what_is_already_so_draws_nothing_new() {
    use auberean_assets::ui::AttributeValue as V;
    use auberean_ui::token::attr;
    use auberean_ui::Value;
    let e = element(
        "Plain",
        "field",
        [20, 15, 79, 58],
        1,
        vec![state("stateless", 0, vec![], vec![image(1, 0)], vec![])],
    );
    let words = element(
        "Words",
        "text",
        [0, 0, 99, 19],
        1,
        vec![state(
            "stateless",
            0,
            vec![("editable", V::Bool(false))],
            vec![],
            vec![],
        )],
    );
    let mut u = ui(vec![e, words], vec![(1, Arc::new(picture(60, 44)))]);
    let l = u.load_layout(LAYOUT).unwrap();
    let h = u.create_root(l, Token::of("Plain")).unwrap();
    let t = u.create_root(l, Token::of("Words")).unwrap();
    u.set_text(t, "Holtburg");
    u.lay_out_text_shown();
    u.use_time(1.0);
    u.set_attr(h, attr::POSITION, Value::Float(0.5));
    u.set_visible(h, true);
    u.move_picture(h, 30, 30);
    u.set_image(h, 1, 0);
    let still = u.revision();
    u.use_time(2.0);
    u.use_time(3.0);
    assert_eq!(u.revision(), still, "the clock passes over the words");
    u.set_attr(h, attr::POSITION, Value::Float(0.5));
    assert_eq!(u.revision(), still, "the same attribute");
    u.set_visible(h, true);
    assert_eq!(u.revision(), still, "the same visibility");
    u.move_picture(h, 30, 30);
    assert_eq!(u.revision(), still, "the same place");
    u.set_image(h, 1, 0);
    assert_eq!(u.revision(), still, "the same picture");
    u.set_attr(h, attr::POSITION, Value::Float(0.75));
    assert_ne!(u.revision(), still, "another attribute");
    let still = u.revision();
    u.set_visible(h, false);
    assert_ne!(u.revision(), still, "another visibility");
    let still = u.revision();
    u.move_picture(h, 31, 30);
    assert_ne!(u.revision(), still, "another place");
}

/// A meter's position draws nothing new until a fill moves by a texel.
#[test]
fn a_meter_draws_nothing_new_until_a_fill_moves() {
    use auberean_ui::token::attr;
    use auberean_ui::Value;
    let m = element(
        "Bar",
        "meter",
        [0, 0, 99, 9],
        0,
        vec![state(
            "stateless",
            0,
            vec![],
            vec![],
            vec![element("left_fill", "field", [0, 0, 99, 9], 0, vec![])],
        )],
    );
    let mut u = ui(vec![m], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let h = u.create_root(l, Token::of("Bar")).unwrap();
    u.set_attr(h, attr::POSITION, Value::Float(0.500));
    let still = u.revision();
    u.set_attr(h, attr::POSITION, Value::Float(0.503));
    assert_eq!(u.revision(), still, "the fill stays within its texel");
    u.set_attr(h, attr::POSITION, Value::Float(0.6));
    assert_ne!(u.revision(), still, "the fill moves");
    let fill = u.child(h, attr::LEFT_FILL).unwrap();
    assert_eq!(u.node(fill).unwrap().rect.width(), 60);
}
