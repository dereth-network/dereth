//! What a press of the pointer finds by a body's box.

use auberean_primitives::Vec3;
use auberean_scene::seen::{check_object_selection, selection_bounds, SeenBody, SeenCamera};

fn camera() -> SeenCamera {
    SeenCamera {
        position: Vec3::new(0.0, 0.0, 0.0),
        heading: 0.0,
        pitch: 0.0,
        fov_y: 90.0,
        near: 0.1,
    }
}

fn body_at(y: f32) -> SeenBody {
    SeenBody {
        id: 1,
        origin: Vec3::new(0.0, y, 0.0),
        heading: 0.0,
        bounds: None,
        faded: false,
    }
}

fn cube(half: f32) -> (Vec3, Vec3) {
    (Vec3::new(-half, -half, -half), Vec3::new(half, half, half))
}

/// A body's box lies under the pointer when one of its corners stands on the screen and its square
/// of clip space meets the pointer's; a box whose every corner lies off the screen is not found,
/// however much of the screen it fills. Its depth is that of its deepest corner.
///
/// Behaviour: desktop.world.a-press-finds-a-box-with-a-corner-on-the-screen
#[test]
fn a_press_finds_a_box_only_with_a_corner_on_the_screen() {
    let c = camera();
    let screen = (800.0, 800.0);
    let middle = selection_bounds(screen, (400.0, 400.0));
    let near = check_object_selection(&c, &body_at(10.0), cube(1.0), 1.0, middle);
    let far = check_object_selection(&c, &body_at(20.0), cube(1.0), 1.0, middle);
    assert!(near.is_some() && far.is_some(), "under the middle");
    assert!(near < far, "the nearer box reaches less deep");
    assert_eq!(
        check_object_selection(
            &c,
            &body_at(10.0),
            cube(1.0),
            1.0,
            selection_bounds(screen, (40.0, 40.0))
        ),
        None,
        "not under a corner of the screen"
    );
    assert_eq!(
        check_object_selection(
            &c,
            &body_at(10.0),
            (
                Vec3::new(-100.0, -1.0, -100.0),
                Vec3::new(100.0, 1.0, 100.0)
            ),
            1.0,
            middle
        ),
        None,
        "a wall filling the screen, every corner off it"
    );
}

/// The pointer selects from the pixel up and to the left of it to that one, each by its corner,
/// half a pixel in: the clip square a press in the middle of a 100 by 100 screen selects.
///
/// Behaviour: desktop.world.a-press-finds-a-box-with-a-corner-on-the-screen
#[test]
fn the_pointer_selects_the_pixel_up_and_to_the_left_of_it() {
    let (min_x, min_y, max_x, max_y) = selection_bounds((100.0, 100.0), (50.0, 50.0));
    let near = |a: f32, b: f32| (a - b).abs() < 1e-5;
    assert!(near(min_x, -0.03) && near(max_x, -0.01), "{min_x} {max_x}");
    assert!(near(min_y, -0.01) && near(max_y, 0.01), "{min_y} {max_y}");
}
