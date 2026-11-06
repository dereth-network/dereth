//! The light inside the human training hall (hall copy 0x0298 of the September 2013 cell file):
//! its interior ambient and fog, its lights, how a draw keeps them, and a cell's vertices lit by
//! them and by the sun.

use auberean_primitives::Vec3;
use auberean_world::color::Color;
use auberean_world::inside::{BlockLights, CellVertex, LightKind, Outside};
use auberean_world::math::Mat4;

use crate::common::need_world;

/// A gray day: the sun half white at full brightness, travelling down and east, no ambient light.
fn gray_day() -> Outside {
    Outside {
        sun: Color::new(0.5, 0.5, 0.5, 1.0),
        brightness: 1.0,
        direction: Vec3::new(0.6, 0.0, -0.8),
        ambient: Color::new(0.0, 0.0, 0.0, 1.0),
    }
}

fn floor(p: Vec3) -> CellVertex {
    CellVertex {
        position: p,
        normal: Vec3::new(0.0, 0.0, 1.0),
        tangent_s: Vec3::new(1.0, 0.0, 0.0),
        tangent_t: Vec3::new(0.0, 1.0, 0.0),
    }
}

/// The training hall's information gives its interior ambient light (14, 13, 10) and a linear
/// interior fog the color (131, 116, 91) from 0 to 200 m; its light record 123 lights, 10 of them sky
/// lights; a light reaches the cells its record lists. A cell's vertices start black and take each
/// light listing them, never past white; a listed light leans the direction a vertex's normal map is
/// lit from toward itself as the cell's own frame has it; the lights kept about the starting cell
/// are the sky lights above it, and light every vertex.
///
/// Behaviour: world.inside.the-halls-light-and-a-cells-lit-vertices
#[test]
fn the_halls_light_and_a_cells_lit_vertices() {
    let w = need_world!();
    let l = BlockLights::of(&w.records, 0x02, 0x98).expect("the hall's light");
    let close = |a: f32, b: f32| (a - b).abs() < 1e-4;
    assert!(close(l.ambient.r, 14.0 / 255.0) && close(l.ambient.g, 13.0 / 255.0));
    assert!(close(l.ambient.b, 10.0 / 255.0), "{:?}", l.ambient);
    let (fog, near, far) = l.fog.expect("a linear fog");
    assert!(close(fog.r, 131.0 / 255.0) && close(near, 0.0) && close(far, 200.0));
    assert_eq!(l.lights.len(), 123);
    assert_eq!(l.lights.iter().filter(|x| x.sky()).count(), 10);
    let start = l.lookup.get(&0x0109).expect("the starting cell's lights");
    assert!(start.len() > 4, "{start:?}");
    // The starting cell's middle, 40 m across: its four lights, the strongest first.
    let day = gray_day();
    let chosen = l.chosen(Vec3::new(40.0, 260.0, -45.0), 30.0, &day);
    assert_eq!(chosen.len(), 4);
    assert!(
        (chosen[0].1.r - 0.5).abs() < 1e-4,
        "a sky light first: {chosen:?}"
    );
    // Seen from far above the hall, no light lights the cell as a whole: a vertex no light lists
    // stays black and its normal map is lit straight on; a vertex beside and 4 m beneath the one
    // light that lists it takes its light, and the light leans the vertex's direction toward itself:
    // 3 m along u, 4 m along the normal.
    let cell = 0x0298_0100;
    let listing = |i: u32| {
        l.lookup[&0x0100]
            .iter()
            .filter(|&&k| l.lights[k].lit.get(&cell).is_some_and(|v| v.contains(&i)))
            .count()
    };
    let listed = |i: u32| listing(i) > 0;
    let (light, first) = l.lookup[&0x0100]
        .iter()
        .map(|&k| &l.lights[k])
        .filter(|x| x.kind == LightKind::Point && x.most_range > 5.0)
        .find_map(|x| {
            let only = x.lit.get(&cell)?.iter().find(|&&i| listing(i) == 1)?;
            Some((x, *only))
        })
        .expect("a point light alone listing one of the cell's vertices");
    let unlisted = (0..).find(|&i| !listed(i)).expect("an unlisted vertex");
    let lift = Vec3::new(0.0, 0.0, 1000.0);
    let frame = Mat4::translation(lift);
    let away = floor(Vec3::new(-500.0, -500.0, 0.0));
    let mut vertices = vec![away; first.max(unlisted) as usize + 1];
    let o = light.origin;
    vertices[first as usize] = floor(Vec3::new(o.x - 3.0, o.y, o.z - 1000.0 - 4.0));
    let lit = l.cell_vertices(cell, &frame, &vertices, &[], &day, 4);
    let dark = lit[unlisted as usize];
    assert_eq!(dark.color, [0.0; 3], "{dark:?}");
    assert!((dark.toward.z - 1.0).abs() < 1e-4, "{dark:?}");
    let under = lit[first as usize];
    assert!(
        under.color.iter().all(|c| (0.0..=1.0).contains(c)),
        "{under:?}"
    );
    assert!(under.color[0] > 0.0, "{under:?}");
    assert!(
        under.toward.y.abs() < 1e-3 && (under.toward.x - 0.6).abs() < 0.02 && under.toward.z > 0.79,
        "the light leans the direction toward itself: {under:?}"
    );
    // Lit as a whole about the starting cell, every vertex takes the kept lights, and a vertex
    // beneath one sees it from above.
    let middle = Vec3::new(40.0, 260.0, -45.0);
    let kept = l.scene().keep(&l, middle, 30.0, &day);
    let at = l.lights[kept[0]].origin;
    let whole = l.cell_vertices(
        0x0298_0109,
        &Mat4::translation(middle),
        &[floor(Vec3::new(
            at.x - middle.x,
            at.y - middle.y,
            at.z - middle.z - 2.0,
        ))],
        &kept,
        &day,
        4,
    );
    assert!(whole[0].color[0] > 0.0, "{whole:?}");
    assert!(whole[0].toward.z > 0.0, "{whole:?}");
}

/// A light weighs how far within its range a draw stands, times the most its intensity reaches,
/// times its color's mean, held to 0..1; a sky light's color is the sun's alone (the ambient light
/// outside does not weigh), times the sun's brightness. A draw keeps at most six lights whose most
/// range reaches it, the heaviest first, and the cell south of the hall's start keeps the sky light
/// above the hall first.
///
/// Behaviour: world.inside.a-draw-keeps-the-lights-that-weigh-most-there
#[test]
fn a_draw_keeps_the_lights_that_weigh_most_there() {
    let w = need_world!();
    let l = BlockLights::of(&w.records, 0x02, 0x98).expect("the hall's light");
    let day = gray_day();
    let none = l.scene().keep(&l, Vec3::new(0.0, 0.0, 10_000.0), 0.0, &day);
    assert!(none.is_empty(), "nothing reaches far above: {none:?}");
    let all = l
        .scene()
        .keep(&l, Vec3::new(60.0, 260.0, -45.0), 1_000.0, &day);
    assert_eq!(all.len(), 6);
    // A sky light weighs by the sun alone: the ambient light outside changes nothing.
    let sky = l.lights.iter().find(|x| x.sky()).expect("a sky light");
    let bright = Outside {
        ambient: Color::new(0.9, 0.9, 0.9, 1.0),
        ..day
    };
    let half = sky.range * 0.5;
    let weight = l.weight(sky, half, &day);
    assert!((weight - 0.25).abs() < 1e-4, "{weight}");
    assert!((l.weight(sky, half, &bright) - 0.25).abs() < 1e-4);
    // A flickering light weighs by the most its intensity reaches.
    let flicker = l
        .lights
        .iter()
        .find(|x| {
            x.kind == LightKind::Point && x.most_intensity.is_some_and(|m| m > x.intensity + 0.1)
        })
        .expect("a flickering light");
    let mean = (flicker.color.r + flicker.color.g + flicker.color.b) / 3.0;
    let most = flicker.most_intensity.unwrap_or(1.0);
    assert!((l.weight(flicker, 0.0, &day) - (most * mean).min(1.0)).abs() < 1e-3);
    assert_eq!(l.weight(flicker, flicker.range * 2.0, &day), 0.0);
    // The cell south of the start keeps the sky light above the hall first.
    let kept = l
        .scene()
        .keep(&l, Vec3::new(120.0, 220.0, -45.0), 48.75, &day);
    assert_eq!(kept.len(), 6, "{kept:?}");
    assert!(l.lights[kept[0]].sky(), "{kept:?}");
}

/// When the light record carries sunlight, the sun lights the vertices its nearest stored
/// direction lists with its color, by how squarely it meets them, times its brightness times the
/// cosine between its way and that direction; a vertex no direction lists takes none of it, and a
/// sun without brightness lights nothing.
///
/// Behaviour: world.inside.the-sun-lights-the-vertices-its-directions-list
#[test]
fn the_sun_lights_the_vertices_its_directions_list() {
    let w = need_world!();
    let l = BlockLights::of(&w.records, 0x02, 0x98).expect("the hall's light");
    let sun = l.sunlight.as_ref().expect("the hall's sunlight");
    assert_eq!(sun.len(), 9);
    // A sun straight down lights the floor of the cell south of the start from its nearest
    // stored direction.
    let down = Outside {
        direction: Vec3::new(0.0, 0.0, -1.0),
        ..gray_day()
    };
    let (best, _, cos) = auberean_world::sky::sun_directions(down.direction);
    let best = best.expect("a nearest direction");
    let low = 0x011F;
    let listed = sun[best].get(&low).expect("the cell's sunlit vertices");
    let first = listed[0];
    let others: Vec<u32> = sun
        .iter()
        .flat_map(|d| d.get(&low).into_iter().flatten().copied())
        .collect();
    let unlisted = (0..)
        .find(|i| !others.contains(i))
        .expect("a vertex no direction lists");
    let vertices = vec![floor(Vec3::ZERO); first.max(unlisted) as usize + 1];
    let cell = 0x0298_0000 | low;
    let lit = l.cell_vertices(cell, &Mat4::IDENTITY, &vertices, &[], &down, 4);
    let k = 0.5 * cos;
    assert!(
        lit[first as usize].color.iter().all(|&c| c >= k - 2e-3),
        "{:?} against {k}",
        lit[first as usize]
    );
    assert_eq!(lit[unlisted as usize].color, [0.0; 3]);
    let night = Outside {
        brightness: 0.0,
        ..down
    };
    let dark = l.cell_vertices(cell, &Mat4::IDENTITY, &vertices, &[], &night, 4);
    assert_eq!(dark[first as usize].color, [0.0; 3]);
}

/// The share of the sun the nearest stored direction takes is its product with the light, whose
/// length is the sun's brightness: a dim sun leaves that direction less, and the next the rest.
///
/// Behaviour: world.inside.a-dim-suns-share-falls-with-its-brightness
#[test]
fn a_dim_suns_nearest_direction_takes_a_share_scaled_by_its_brightness() {
    let w = need_world!();
    let l = BlockLights::of(&w.records, 0x02, 0x98).expect("the hall's light");
    let sun = l.sunlight.as_ref().expect("the hall's sunlight");
    let travel = Vec3::new(0.0, 0.0, -1.0);
    let brightness = 0.65;
    let dim = Outside {
        brightness,
        direction: travel,
        sun: Color::new(1.0, 1.0, 1.0, 1.0),
        ..gray_day()
    };
    let (best, second, cos) = auberean_world::sky::sun_directions(travel);
    let (best, second) = (best.expect("a nearest"), second.expect("a next"));
    let low = 0x011F;
    let in_list = |d: usize, i: u32| sun[d].get(&low).is_some_and(|l| l.contains(&i));
    let only_first = sun[best]
        .get(&low)
        .expect("the cell's sunlit vertices")
        .iter()
        .copied()
        .find(|i| !in_list(second, *i))
        .expect("a vertex only the nearest direction lists");
    let vertices = vec![floor(Vec3::ZERO); only_first as usize + 1];
    let lit = l.cell_vertices(0x0298_0000 | low, &Mat4::IDENTITY, &vertices, &[], &dim, 4);
    // Straight down onto a floor, so the vertex takes the whole of its direction's share.
    let want = brightness * (brightness * cos);
    for c in lit[only_first as usize].color {
        assert!((c - want).abs() < 2e-3, "{c} against {want}");
    }
}

/// No interior cell of the data holds a light of its own as an entity: every entity placed in a
/// cell is an object, so a cell's lights are all its block's light record's, and no cell has a
/// corona to draw.
///
/// Behaviour: world.inside.no-cell-holds-a-light-of-its-own
#[test]
fn no_cell_holds_a_light_entity() {
    let w = need_world!();
    let r = &w.records;
    let mut entities = 0usize;
    for (x, y) in r.landblock_coordinates() {
        let Some(info) = r.landblock_info(x, y) else {
            continue;
        };
        for k in 0..info.cell_count {
            let Some(c) = u16::try_from(k).ok().and_then(|k| r.cell(x, y, 0x100 + k)) else {
                continue;
            };
            for e in c.entities.iter().flat_map(|g| g.entities.iter()) {
                entities += 1;
                assert_eq!(e.kind, Some(0x4000_0001), "block {x:x}{y:x} cell {k:#x}");
            }
        }
    }
    assert!(entities > 100_000, "{entities} entities");
}
