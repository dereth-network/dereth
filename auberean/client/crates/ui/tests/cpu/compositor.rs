//! The compositor: how a picture goes onto its element's layer, and the layers onto the screen.

use auberean_assets::ui::AttributeValue as V;
use auberean_ui::draw::{self, Surface};
use auberean_ui::{Rect, Token};

use crate::fixture::{element, image, solid, state, ui, LAYOUT};

fn screen_over(ui: &auberean_ui::Ui, under: [u8; 4]) -> Surface {
    let mut s = Surface::new(1024, 768);
    for px in s.bgra.chunks_mut(4) {
        px.copy_from_slice(&under);
    }
    draw::draw(ui, &mut s);
    s
}

/// Each top-level element starts from a clear layer: a nearer one that draws nothing over a deeper
/// one's place leaves the deeper one's texels as they were, not laid over themselves again.
///
/// Behaviour: ui.draw.each-layer-starts-clear-of-the-one-before
#[test]
fn each_layer_starts_clear_of_the_one_before() {
    let deep = element(
        "Deep",
        "field",
        [10, 10, 29, 29],
        5,
        vec![state("stateless", 0, vec![], vec![image(1, 0)], vec![])],
    );
    let near = element(
        "Near",
        "field",
        [0, 0, 49, 49],
        1,
        vec![state("stateless", 0, vec![], vec![image(2, 1)], vec![])],
    );
    let mut u = ui(
        vec![deep, near],
        vec![
            (1, solid(20, 20, [10, 90, 240, 128])),
            (2, solid(50, 50, [0, 0, 0, 0])),
        ],
    );
    let l = u.load_layout(LAYOUT).unwrap();
    u.create_root(l, Token::of("Deep")).unwrap();
    u.create_root(l, Token::of("Near")).unwrap();
    let mut own = Surface::new(1024, 768);
    draw::draw_layers_where(&u, &mut own, |_| true);
    assert_eq!(own.get(15, 15), [10, 90, 240, 128]);
    assert_eq!(own.get(5, 5), [0; 4]);
}

/// A picture smaller than its element repeats across it from the element's top left; one the
/// element asks to fit is stretched over it instead.
///
/// Behaviour: ui.draw.a-small-picture-repeats-unless-sized-to-its-element
#[test]
fn a_small_picture_repeats_unless_sized_to_its_element() {
    let mut checker = vec![0u8; 2 * 2 * 4];
    checker[0..4].copy_from_slice(&[255, 255, 255, 255]);
    checker[12..16].copy_from_slice(&[255, 255, 255, 255]);
    let pic = std::sync::Arc::new(auberean_ui::Image {
        width: 2,
        height: 2,
        bgra: checker,
        alpha_only: false,
    });
    let tiled = element(
        "Tiled",
        "field",
        [0, 0, 5, 5],
        0,
        vec![state("stateless", 0, vec![], vec![image(9, 0)], vec![])],
    );
    let fitted = element(
        "Fitted",
        "field",
        [10, 0, 13, 3],
        0,
        vec![state(
            "stateless",
            0,
            vec![("size_image_to_owner", V::Bool(true))],
            vec![image(9, 0)],
            vec![],
        )],
    );
    let mut u = ui(vec![tiled, fitted], vec![(9, pic)]);
    let l = u.load_layout(LAYOUT).unwrap();
    u.create_root(l, Token::of("Tiled")).unwrap();
    u.create_root(l, Token::of("Fitted")).unwrap();
    let s = screen_over(&u, [0, 0, 0, 255]);
    assert_eq!(s.get(4, 4), [255, 255, 255, 255], "repeated");
    assert_eq!(s.get(5, 4)[0], 0);
    assert_eq!(
        s.get(11, 1),
        [255, 255, 255, 255],
        "stretched: each texel twice"
    );
    assert_eq!(s.get(12, 1)[0], 0);
}

/// A picture its element asks to center is drawn once in the element's middle (half the room left
/// over before it), neither repeated nor stretched, even when the element asks to fit it.
///
/// Behaviour: ui.draw.a-centered-picture-is-drawn-once-in-the-middle
#[test]
fn a_centered_picture_is_drawn_once_in_the_middle() {
    let white = std::sync::Arc::new(auberean_ui::Image {
        width: 2,
        height: 2,
        bgra: vec![255; 2 * 2 * 4],
        alpha_only: false,
    });
    let field = element(
        "Centered",
        "field",
        [0, 0, 5, 5],
        0,
        vec![state(
            "stateless",
            0,
            vec![("size_image_to_owner", V::Bool(true))],
            vec![image(9, 0)],
            vec![],
        )],
    );
    let mut u = ui(vec![field], vec![(9, white)]);
    let l = u.load_layout(LAYOUT).unwrap();
    let h = u.create_root(l, Token::of("Centered")).unwrap();
    u.set_center_image(h, true);
    let s = screen_over(&u, [0, 0, 0, 255]);
    // Six wide, two drawn: two columns before it.
    for (x, y) in [(2, 2), (3, 3)] {
        assert_eq!(s.get(x, y), [255, 255, 255, 255], "({x}, {y}) drawn");
    }
    for (x, y) in [(0, 0), (1, 2), (4, 2), (2, 4), (5, 5)] {
        assert_eq!(s.get(x, y)[0], 0, "({x}, {y}) not drawn");
    }
}

/// Draw modes 1 and 2 blend a picture over what its layer already holds, keeping the layer's
/// alpha; mode 0 copies its texels, alpha included, so a transparent texel cuts through the
/// element beneath it in the same layer.
///
/// Behaviour: ui.draw.blended-pictures-keep-the-layers-alpha-copied-ones-replace-it
#[test]
fn blended_pictures_keep_the_layers_alpha_copied_ones_replace_it() {
    let base = |child_mode: u32| {
        element(
            "Base",
            "field",
            [0, 0, 9, 9],
            0,
            vec![state(
                "stateless",
                0,
                vec![],
                vec![image(1, 0)],
                vec![element(
                    "Top",
                    "field",
                    [0, 0, 9, 9],
                    0,
                    vec![state(
                        "stateless",
                        0,
                        vec![],
                        vec![image(2, child_mode)],
                        vec![],
                    )],
                )],
            )],
        )
    };
    let red = solid(10, 10, [0, 0, 255, 255]);
    let half_green = solid(10, 10, [0, 255, 0, 128]);
    // Blended: half green over red, the layer still opaque.
    let mut u = ui(
        vec![base(2)],
        vec![(1, red.clone()), (2, half_green.clone())],
    );
    let l = u.load_layout(LAYOUT).unwrap();
    u.create_root(l, Token::of("Base")).unwrap();
    let s = screen_over(&u, [255, 0, 0, 255]);
    let p = s.get(5, 5);
    assert_eq!(p[0], 0, "no blue from beneath: the layer stayed opaque");
    assert!(p[1] > 100 && p[2] > 100, "green over red: {p:?}");
    // Copied: the half-transparent green replaces the red, so the blue beneath shows through.
    let mut u = ui(vec![base(0)], vec![(1, red), (2, half_green)]);
    let l = u.load_layout(LAYOUT).unwrap();
    u.create_root(l, Token::of("Base")).unwrap();
    let s = screen_over(&u, [255, 0, 0, 255]);
    let p = s.get(5, 5);
    assert!(
        p[0] > 100 && p[1] > 100 && p[2] == 0,
        "green over blue: {p:?}"
    );
}

/// Draw mode 3 copies a picture and fades it by its element's transparency.
///
/// Behaviour: ui.draw.mode-three-fades-by-the-elements-alpha
#[test]
fn mode_three_fades_by_the_elements_alpha() {
    let e = element(
        "Faded",
        "field",
        [0, 0, 9, 9],
        0,
        vec![state(
            "stateless",
            0,
            vec![("alpha_value", V::Float(0.5))],
            vec![image(1, 3)],
            vec![],
        )],
    );
    let mut u = ui(vec![e], vec![(1, solid(10, 10, [0, 0, 255, 255]))]);
    let l = u.load_layout(LAYOUT).unwrap();
    u.create_root(l, Token::of("Faded")).unwrap();
    let s = screen_over(&u, [0, 0, 0, 255]);
    let r = s.get(5, 5)[2];
    assert!((126..=129).contains(&r), "half red over black: {r}");
}

/// Top-level elements are laid over each other from the deepest: the nearest is drawn last.
///
/// Behaviour: ui.draw.the-nearest-top-level-element-is-drawn-last
#[test]
fn the_nearest_top_level_element_is_drawn_last() {
    let near = element(
        "Near",
        "field",
        [0, 0, 9, 9],
        10,
        vec![state("stateless", 0, vec![], vec![image(1, 0)], vec![])],
    );
    let far = element(
        "Far",
        "field",
        [0, 0, 9, 9],
        20,
        vec![state("stateless", 0, vec![], vec![image(2, 0)], vec![])],
    );
    let mut u = ui(
        vec![near, far],
        vec![
            (1, solid(10, 10, [0, 0, 255, 255])),
            (2, solid(10, 10, [0, 255, 0, 255])),
        ],
    );
    let l = u.load_layout(LAYOUT).unwrap();
    u.create_root(l, Token::of("Near")).unwrap();
    u.create_root(l, Token::of("Far")).unwrap();
    assert_eq!(screen_over(&u, [0, 0, 0, 255]).get(5, 5), [0, 0, 255, 255]);
}

/// Of two top-level elements at one depth the first put on the screen is drawn in front, and the
/// pointer meets it first.
///
/// Behaviour: ui.draw.of-equal-depths-the-first-on-the-screen-is-drawn-in-front
#[test]
fn of_equal_depths_the_first_on_the_screen_is_drawn_in_front() {
    let piece = |name, picture| {
        element(
            name,
            "field",
            [0, 0, 9, 9],
            10,
            vec![state(
                "stateless",
                0,
                vec![],
                vec![image(picture, 0)],
                vec![],
            )],
        )
    };
    let mut u = ui(
        vec![piece("First", 1), piece("Second", 2)],
        vec![
            (1, solid(10, 10, [0, 0, 255, 255])),
            (2, solid(10, 10, [0, 255, 0, 255])),
        ],
    );
    let l = u.load_layout(LAYOUT).unwrap();
    let first = u.create_root(l, Token::of("First")).unwrap();
    let second = u.create_root(l, Token::of("Second")).unwrap();
    assert_eq!(screen_over(&u, [0, 0, 0, 255]).get(5, 5), [0, 0, 255, 255]);
    assert_eq!(u.hit(5, 5), Some(first), "the pointer meets what it sees");
    assert_ne!(first, second);
}

/// A rectangle painted over an element (the radar's blips) is a window onto the element's own
/// picture, pressed onto it: one reaching past an edge paints the texels along that edge, one
/// inside paints itself, and nothing lands outside the element.
///
/// Behaviour: ui.draw.a-fill-reaching-off-its-element-paints-the-edge-it-passes
#[test]
fn a_fill_reaching_off_its_element_paints_the_edge_it_passes() {
    let field = element(
        "Field",
        "field",
        [100, 100, 109, 109],
        0,
        vec![state("stateless", 0, vec![], vec![image(1, 0)], vec![])],
    );
    let mut u = ui(vec![field], vec![(1, solid(10, 10, [0, 0, 0, 255]))]);
    let l = u.load_layout(LAYOUT).unwrap();
    let h = u.create_root(l, Token::of("Field")).unwrap();
    let green = [0, 255, 0, 255];
    u.set_fills(
        h,
        vec![
            (auberean_ui::Rect::new(-3, 4, -3, 4), green),
            (auberean_ui::Rect::new(12, 2, 14, 2), green),
            (auberean_ui::Rect::new(5, 13, 5, 13), green),
            (auberean_ui::Rect::new(5, 5, 5, 5), green),
        ],
    );
    let s = screen_over(&u, [0, 0, 0, 255]);
    assert_eq!(
        s.get(100, 104),
        green,
        "left of the element: its first column"
    );
    assert_eq!(s.get(109, 102), green, "right of it: its last column");
    assert_eq!(s.get(105, 109), green, "below it: its last row");
    assert_eq!(s.get(105, 105), green, "inside: where it is");
    for (x, y) in [(97, 104), (112, 102), (105, 113), (101, 104), (108, 102)] {
        assert_eq!(s.get(x, y), [0, 0, 0, 255], "nothing at ({x}, {y})");
    }
}

/// The interface drawn into a picture of its own that starts transparent, then laid over a scene
/// by that picture's alpha, comes out as the interface drawn straight over the scene: layers that
/// cover each other partly, at partial alpha, included (to one step of rounding).
///
/// Behaviour: ui.draw.a-picture-of-its-own-laid-over-the-scene-matches-drawing-over-it
#[test]
fn a_picture_of_its_own_laid_over_the_scene_matches_drawing_over_it() {
    let faded = element(
        "Faded",
        "field",
        [0, 0, 19, 19],
        5,
        vec![state(
            "stateless",
            0,
            vec![("alpha_value", V::Float(0.7))],
            vec![image(1, 3)],
            vec![],
        )],
    );
    let half = element(
        "Half",
        "field",
        [10, 10, 29, 29],
        3,
        vec![state("stateless", 0, vec![], vec![image(2, 0)], vec![])],
    );
    let mut u = ui(
        vec![faded, half],
        vec![
            (1, solid(20, 20, [200, 40, 10, 255])),
            (2, solid(20, 20, [10, 90, 240, 128])),
        ],
    );
    let l = u.load_layout(LAYOUT).unwrap();
    u.create_root(l, Token::of("Faded")).unwrap();
    u.create_root(l, Token::of("Half")).unwrap();
    let scene = [30, 160, 90, 255];
    let straight = screen_over(&u, scene);
    let mut own = Surface::new(1024, 768);
    draw::draw_layers_where(&u, &mut own, |_| true);
    for (x, y) in [(5, 5), (15, 15), (25, 25), (40, 40)] {
        let o = own.get(x, y);
        let a = f32::from(o[3]) / 255.0;
        let laid: Vec<f32> = (0..3)
            .map(|c| f32::from(o[c]) * a + f32::from(scene[c]) * (1.0 - a))
            .collect();
        let want = straight.get(x, y);
        for c in 0..3 {
            assert!(
                (laid[c] - f32::from(want[c])).abs() <= 1.5,
                "at ({x}, {y}) channel {c}: {laid:?} against {want:?}"
            );
        }
    }
}

/// The tree's revision moves whenever something drawn may have changed, and stays put while
/// nothing does, so a host redraws the interface only when it changed.
///
/// Behaviour: ui.draw.the-revision-moves-only-with-a-change
#[test]
fn the_revision_moves_only_with_a_change() {
    let mut u = ui(
        vec![element(
            "Panel",
            "field",
            [0, 0, 9, 9],
            0,
            vec![state("stateless", 0, vec![], vec![], vec![])],
        )],
        vec![],
    );
    let l = u.load_layout(LAYOUT).unwrap();
    let h = u.create_root(l, Token::of("Panel")).unwrap();
    let r0 = u.revision();
    let _ = u.hit(5, 5);
    let mut s = Surface::new(16, 16);
    draw::draw(&u, &mut s);
    assert_eq!(u.revision(), r0, "looking changes nothing");
    u.set_visible(h, false);
    assert!(u.revision() > r0, "hiding is a change");
}

/// A window with a resize line keeps its picture while it is resized: a white outline one texel
/// wide follows the pointer, and on release the window takes the outline's place and the outline
/// goes. Laid over the scene as a picture of its own, the outline shows the same.
///
/// Behaviour: ui.draw.a-resize-line-outlines-the-new-size-until-release
#[test]
fn a_resize_line_outlines_the_new_size_until_release() {
    const WHITE: [u8; 4] = [255, 255, 255, 255];
    const UNDER: [u8; 4] = [10, 20, 30, 255];
    let mut u = ui(
        vec![element(
            "Window",
            "field",
            [100, 100, 199, 199],
            0,
            vec![state(
                "stateless",
                0,
                vec![("resize_line", V::Bool(true))],
                vec![],
                vec![],
            )],
        )],
        vec![],
    );
    let l = u.load_layout(LAYOUT).unwrap();
    let w = u.create_root(l, Token::of("Window")).unwrap();
    assert!(screen_over(&u, UNDER).get(100, 100) == UNDER, "no line yet");

    u.pointer_move(199, 199);
    u.start_resizing(w, auberean_ui::types::resizebar::Border::LowerRight);
    let before = u.revision();
    u.pointer_move(239, 219);
    assert!(u.revision() > before, "the line moving is a change");
    assert_eq!(u.node(w).unwrap().rect, Rect::new(100, 100, 199, 199));

    let s = screen_over(&u, UNDER);
    for (x, y) in [
        (100, 100),
        (239, 100),
        (100, 219),
        (239, 219),
        (170, 100),
        (239, 160),
        (170, 219),
        (100, 160),
    ] {
        assert_eq!(s.get(x, y), WHITE, "the outline at {x},{y}");
    }
    for (x, y) in [(99, 100), (240, 219), (170, 170), (199, 199)] {
        assert_eq!(s.get(x, y), UNDER, "not the outline at {x},{y}");
    }
    let mut over = Surface::new(1024, 768);
    draw::draw_layers_where(&u, &mut over, |_| true);
    assert_eq!(over.get(239, 219), WHITE, "laid over the scene too");
    assert_eq!(over.get(170, 170), [0; 4]);

    u.stop_resizing(w);
    assert_eq!(u.node(w).unwrap().rect, Rect::new(100, 100, 239, 219));
    assert_eq!(
        screen_over(&u, UNDER).get(239, 219),
        UNDER,
        "the line is gone"
    );
}

/// A top-level element marked `no_surface_object` is drawn into no picture of its own: its own
/// picture and its children's never reach the screen, though the pointer still finds it.
///
/// Behaviour: ui.draw.a-top-level-element-without-a-surface-shows-nothing
#[test]
fn a_top_level_element_without_a_surface_shows_nothing() {
    let child = element(
        "Child",
        "field",
        [0, 0, 9, 9],
        0,
        vec![state("stateless", 0, vec![], vec![image(7, 0)], vec![])],
    );
    let root = |name: &str, x: i16, bare: bool| {
        let attributes = if bare {
            vec![("no_surface_object", V::Bool(true))]
        } else {
            vec![]
        };
        element(
            name,
            "field",
            [x, 0, x + 19, 19],
            0,
            vec![state(
                "stateless",
                0,
                attributes,
                vec![image(7, 0)],
                vec![child.clone()],
            )],
        )
    };
    let white = solid(4, 4, [255, 255, 255, 255]);
    let mut u = ui(
        vec![root("Shown", 0, false), root("Bare", 100, true)],
        vec![(7, white)],
    );
    let l = u.load_layout(LAYOUT).unwrap();
    u.create_root(l, Token::of("Shown")).unwrap();
    let bare = u.create_root(l, Token::of("Bare")).unwrap();
    let s = screen_over(&u, [0, 0, 0, 255]);
    assert_eq!(
        s.get(15, 15),
        [255, 255, 255, 255],
        "an ordinary one is drawn"
    );
    assert_eq!(s.get(115, 15), [0, 0, 0, 255], "the bare one's picture");
    assert_eq!(s.get(102, 2), [0, 0, 0, 255], "the bare one's child");
    assert_eq!(u.hit(115, 15), Some(bare), "the pointer still finds it");
}
