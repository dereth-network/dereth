//! Behaviour: none (tooling: the world editor's marks on the world over the retail dats)
//! The marks on the world reach the world view's right and bottom edges at a UI scale over 100%,
//! however the panes beside and under it are dragged.
//! Fixture: the sample world over Holtburg, seen from straight above with its land cells' grid
//! shown, in a 1600 by 1000 window at 150%.

use std::path::Path;

use dereth_input::host::HostEvent;
use dereth_input::keys::MouseButton;
use dereth_primitives::{Vec3, Viewport};
use lyceum_editor::app::{EditorApp, Tab};

use crate::{capture, editor_with, gpu_lock, sample, shots, HOLTBURG};

/// A drag of the pointer from `from` to `to` (window pixels), over a few frames.
fn drag(ed: &mut EditorApp, from: (f32, f32), to: (f32, f32)) {
    let at = |ed: &mut EditorApp, p: (f32, f32)| {
        ed.queue(HostEvent::CursorMoved {
            x: f64::from(p.0),
            y: f64::from(p.1),
        });
    };
    at(ed, from);
    ed.frame();
    ed.queue(HostEvent::MouseInput {
        button: MouseButton::Left,
        pressed: true,
    });
    ed.frame();
    for i in 1..=4_u8 {
        let t = f32::from(i) / 4.0;
        at(
            ed,
            (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t),
        );
        ed.frame();
    }
    ed.queue(HostEvent::MouseInput {
        button: MouseButton::Left,
        pressed: false,
    });
    ed.frame();
    ed.frame();
}

/// The pixels the grid lights: drawn with it shown, not without, and the same without it before
/// and after. The frame with it shown is kept as `keep`.
fn grid_pixels(ed: &mut EditorApp, dir: &Path, keep: &Path) -> (usize, Vec<bool>) {
    let shot = |ed: &mut EditorApp, grid: bool| {
        ed.panels.show_grid = grid;
        ed.frame();
        ed.frame();
        capture(ed, dir)
    };
    let before = shot(ed, false);
    let with = shot(ed, true);
    std::fs::copy(dir.join("frame.png"), keep).unwrap();
    let after = shot(ed, false);
    let differ = |a: &[u8], b: &[u8]| {
        a.iter()
            .zip(b)
            .take(3)
            .map(|(x, y)| (i32::from(*x) - i32::from(*y)).abs())
            .sum::<i32>()
    };
    let px = |img: &(usize, usize, Vec<u8>), i: usize| -> [u8; 3] {
        [img.2[i * 4], img.2[i * 4 + 1], img.2[i * 4 + 2]]
    };
    let lit = (0..before.2.len() / 4)
        .map(|i| {
            differ(&px(&before, i), &px(&after, i)) < 12
                && differ(&px(&before, i), &px(&with, i)) > 48
        })
        .collect();
    (before.0, lit)
}

/// How many of the pixels the grid lights stand in the strip `depth` pixels deep inside the
/// view's right edge, and in the one inside its bottom edge.
fn at_the_edges(width: usize, lit: &[bool], v: Viewport, depth: u32) -> (usize, usize) {
    let (x0, y0) = (v.x, v.y);
    let (x1, y1) = (x0 + v.width, y0 + v.height);
    let on = |x: u32, y: u32| lit[y as usize * width + x as usize];
    let right = (y0 + 2..y1 - 2)
        .flat_map(|y| (x1 - depth..x1 - 2).map(move |x| (x, y)))
        .filter(|(x, y)| on(*x, *y))
        .count();
    let bottom = (y1 - depth..y1 - 2)
        .flat_map(|y| (x0 + 2..x1 - 2).map(move |x| (x, y)))
        .filter(|(x, y)| on(*x, *y))
        .count();
    (right, bottom)
}

/// The middle of the edge between the world and the pane beside it (`under` false) or under it,
/// in points, where the layout puts it.
fn separator(ed: &mut EditorApp, under: bool) -> (f32, f32) {
    use egui_dock::{Node, NodeIndex};
    let tree = ed.layout_mut().expect("the layout").main_surface_mut();
    let (world, _) = tree.find_tab(&Tab::World).expect("the world");
    if under {
        let parent = world.parent().expect("a pane under the world");
        let Node::Vertical(split) = &tree[parent] else {
            panic!("the world's pane is not split over another");
        };
        let r = split.rect();
        (r.center().x, r.min.y + r.height() * split.fraction)
    } else {
        let Node::Horizontal(split) = &tree[NodeIndex::root()] else {
            panic!("the window is not split beside the world");
        };
        let r = split.rect();
        (r.min.x + r.width() * split.fraction, r.center().y)
    }
}

#[test]
fn the_marks_on_the_world_reach_the_world_views_right_and_bottom_edges_however_its_panes_are_dragged(
) {
    let _gpu = gpu_lock();
    let (dir, world) = sample("editor-marks-edges");
    let mut ed = editor_with(&world, &["--size", "1600x1000"]);
    ed.set_ui_scale(1.5);
    ed.panels.show_hints = false;
    // The Log in a pane of its own under the world.
    {
        let tree = ed.layout_mut().expect("the layout").main_surface_mut();
        let log = tree.find_tab(&Tab::Log).expect("the Log");
        tree.remove_tab(log);
        let (world_node, _) = tree.find_tab(&Tab::World).expect("the world");
        tree.split_below(world_node, 0.6, vec![Tab::Log]);
    }
    // Straight down over Holtburg's middle from 30 meters, north up: the grid of its land cells
    // runs on past every edge of the view.
    let z = ed.session.ground(HOLTBURG, 96.0, 96.0).unwrap();
    ed.look_at(HOLTBURG, Vec3::new(96.0, 96.0, z), 30.0, 0.0, -89.0);
    for _ in 0..3 {
        ed.frame();
    }
    let s = ed.ui_scale();
    // Each pane dragged both ways: the right one by its edge beside the world, the bottom one by
    // its edge under it.
    let mut views = Vec::new();
    for (name, right, down) in [
        ("right-pane-narrower", 100.0, 0.0),
        ("right-pane-wider", -200.0, 0.0),
        ("bottom-pane-shorter", 0.0, 80.0),
        ("bottom-pane-taller", 0.0, -160.0),
    ] {
        let v = ed.world_view();
        let from = separator(&mut ed, down != 0.0);
        let from = (from.0 * s, from.1 * s);
        drag(&mut ed, from, (from.0 + right * s, from.1 + down * s));
        let after = ed.world_view();
        // The drag moved the pane's edge, as far as the panes' sizes let it.
        #[allow(clippy::cast_possible_wrap)] // a window's pixels
        let (grew_x, grew_y) = (
            after.width as i32 - v.width as i32,
            after.height as i32 - v.height as i32,
        );
        let moved = |grew: i32, by: f32| by == 0.0 || (grew > 0) == (by > 0.0) && grew.abs() >= 30;
        assert!(
            moved(grew_x, right) && moved(grew_y, down),
            "{name}: the world view {v:?} became {after:?}"
        );
        let keep = shots("marks-edges").join(format!("{name}.png"));
        let (width, lit) = grid_pixels(&mut ed, dir.path(), &keep);
        let (at_right, at_bottom) = at_the_edges(width, &lit, after, 24);
        assert!(
            at_right >= 10 && at_bottom >= 10,
            "{name}: the grid lights {at_right} pixels by the right edge and {at_bottom} by the \
             bottom of the world view {after:?}"
        );
        views.push(after);
    }
    assert!(views[0].width > views[1].width && views[2].height > views[3].height);
}
