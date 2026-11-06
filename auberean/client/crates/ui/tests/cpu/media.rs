//! What a state plays as the clock runs, the fades of top-level elements, attributes of each type,
//! kept places and tree nodes.

use auberean_assets::ui::{AttributeValue as V, Media};
use auberean_primitives::DataId;
use auberean_ui::draw::{self, Surface};
use auberean_ui::location::Location;
use auberean_ui::node::Nodes;
use auberean_ui::{Cursor, Token, Value};

use crate::fixture::{
    animation, element, fade, image, solid, state, state_with, ui, ui_on, LAYOUT,
};

fn picture(u: &auberean_ui::Ui, h: auberean_ui::Handle) -> Option<u32> {
    u.node(h).and_then(|n| n.picture).map(|p| p.id)
}

/// An animation shows the frame its time has reached (the time since it started over one frame's
/// share of its length), and the list waits on it until its last frame shows; then the next item
/// plays.
///
/// Behaviour: ui.media.an-animation-shows-the-frame-its-time-reached-then-the-list-moves-on
#[test]
fn an_animation_shows_the_frame_its_time_reached_then_the_list_moves_on() {
    let e = element(
        "Anim",
        "field",
        [0, 0, 9, 9],
        0,
        vec![state(
            "stateless",
            0,
            vec![],
            vec![animation(vec![1, 2, 3, 4], 1.0), image(9, 0)],
            vec![],
        )],
    );
    let mut u = ui(vec![e], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let h = u.create_root(l, Token::of("Anim")).unwrap();
    assert_eq!(picture(&u, h), Some(1), "the first frame at once");
    u.use_time(0.3);
    assert_eq!(picture(&u, h), Some(2), "0.3 s of four frames in a second");
    u.use_time(0.6);
    assert_eq!(picture(&u, h), Some(3));
    u.use_time(0.76);
    assert_eq!(
        picture(&u, h),
        Some(9),
        "the last frame reached, the list moved on to the picture after it"
    );
}

/// A pause holds the list until its time is up; a jump plays the list again from the item it
/// names, so an animation followed by a jump to it plays over and over.
///
/// Behaviour: ui.media.a-pause-holds-the-list-and-a-jump-plays-it-again
#[test]
fn a_pause_holds_the_list_and_a_jump_plays_it_again() {
    let e = element(
        "Loop",
        "field",
        [0, 0, 9, 9],
        0,
        vec![state(
            "stateless",
            0,
            vec![],
            vec![
                image(1, 0),
                Media::Pause { min: 0.5, max: 0.5 },
                image(2, 0),
                Media::Pause { min: 0.5, max: 0.5 },
                Media::Jump {
                    index: 0,
                    probability: 1.0,
                },
            ],
            vec![],
        )],
    );
    let mut u = ui(vec![e], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let h = u.create_root(l, Token::of("Loop")).unwrap();
    assert_eq!(picture(&u, h), Some(1));
    u.use_time(0.4);
    assert_eq!(picture(&u, h), Some(1), "the pause is not over");
    u.use_time(0.5);
    assert_eq!(picture(&u, h), Some(2), "the pause over");
    u.use_time(1.0);
    assert_eq!(picture(&u, h), Some(1), "the jump played the list again");
    u.use_time(1.5);
    assert_eq!(picture(&u, h), Some(2), "and the pause started over");
}

/// A sound item asks the host for its sound when its state plays (a button's pressed look), and a
/// cursor item gives the element its cursor, shown while the element is topmost under the
/// pointer.
///
/// Behaviour: ui.media.sounds-go-to-the-host-and-cursors-to-their-element
#[test]
fn sounds_go_to_the_host_and_cursors_to_their_element() {
    let mut b = element(
        "Button",
        "button",
        [10, 10, 59, 29],
        0,
        vec![
            state("normal", 1, vec![], vec![image(1, 0)], vec![]),
            state(
                "normal_pressed",
                1,
                vec![],
                vec![image(3, 0), Media::Sound { sound: DataId(5) }],
                vec![],
            ),
        ],
    );
    b.default_state = Token::of("normal").0;
    let grip = element(
        "Grip",
        "field",
        [100, 100, 109, 109],
        0,
        vec![state(
            "stateless",
            0,
            vec![],
            vec![Media::Cursor {
                picture: DataId(77),
                x: 11,
                y: 12,
            }],
            vec![],
        )],
    );
    let mut u = ui(vec![b, grip], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    u.create_root(l, Token::of("Button")).unwrap();
    let g = u.create_root(l, Token::of("Grip")).unwrap();
    assert!(u.take_sounds().is_empty());
    u.pointer_move(20, 20);
    u.pointer_press();
    let sounds: Vec<u32> = u.take_sounds().iter().map(|s| s.sound).collect();
    assert_eq!(sounds, vec![5], "the pressed look's sound");
    u.pointer_release();
    let c = Cursor {
        picture: 77,
        hot_x: 11,
        hot_y: 12,
    };
    assert_eq!(u.node(g).unwrap().cursor, Some(c));
    assert_ne!(u.cursor(), Some(c));
    u.pointer_move(105, 105);
    assert_eq!(
        u.cursor(),
        Some(c),
        "the element's cursor while it is topmost"
    );
}

/// A message item says its message, and a state item puts the element in another state.
///
/// Behaviour: ui.media.messages-are-said-and-states-change
#[test]
fn messages_are_said_and_states_change() {
    let e = element(
        "Talker",
        "field",
        [0, 0, 9, 9],
        0,
        vec![
            state(
                "stateless",
                0,
                vec![],
                vec![
                    Media::Message {
                        message: Token::of("hello").0,
                        probability: 1.0,
                    },
                    Media::State {
                        state: Token::of("later").0,
                    },
                ],
                vec![],
            ),
            state("later", 0, vec![], vec![image(4, 0)], vec![]),
        ],
    );
    let mut u = ui(vec![e], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let h = u.create_root(l, Token::of("Talker")).unwrap();
    assert!(u
        .take_messages()
        .iter()
        .any(|m| m.from == h && m.what == Token::of("hello")));
    assert_eq!(u.node(h).unwrap().state, Token::of("later"));
    assert_eq!(picture(&u, h), Some(4));
}

/// A top-level element fades in as it is made (its fade from no state), and when it leaves a state
/// whose fade runs on leaving, it stays in that state, fading, until the fade is done; then it is
/// in the new state. The compositor draws it at the alpha the fade reached.
///
/// Behaviour: ui.media.a-top-level-element-fades-as-it-enters-and-leaves-its-states
#[test]
fn a_top_level_element_fades_as_it_enters_and_leaves_its_states() {
    let e = element(
        "Picture",
        "field",
        [0, 0, 9, 9],
        0,
        vec![
            state_with(
                "stateless",
                vec![],
                vec![image(1, 0)],
                vec![fade(1, "no_state", 0.0, 1.0, 2.0)],
            ),
            state_with(
                "fading",
                vec![],
                vec![],
                vec![fade(2, "stateless", 1.0, 0.0, 2.0)],
            ),
        ],
    );
    let mut u = ui(vec![e], vec![(1, solid(10, 10, [255, 255, 255, 255]))]);
    let l = u.load_layout(LAYOUT).unwrap();
    let h = u.create_root(l, Token::of("Picture")).unwrap();
    let alpha = |u: &auberean_ui::Ui| u.node(h).unwrap().object_alpha;
    assert_eq!(alpha(&u), 0.0, "made, it starts unseen");
    u.use_time(1.0);
    assert_eq!(alpha(&u), 0.5, "half way in");
    let mut s = Surface::new(1024, 768);
    draw::draw_layers_where(&u, &mut s, |_| true);
    assert_eq!(s.get(5, 5)[3], 127, "drawn at the alpha reached");
    u.use_time(2.0);
    assert_eq!(alpha(&u), 1.0);
    u.set_state(h, Token::of("fading"));
    u.set_state(h, Token::of("stateless"));
    assert_eq!(
        u.node(h).unwrap().state,
        Token::of("fading"),
        "leaving waits on the fade"
    );
    u.use_time(3.0);
    assert_eq!(alpha(&u), 0.5);
    u.use_time(4.0);
    assert_eq!(
        u.node(h).unwrap().state,
        Token::of("stateless"),
        "the fade done, the element went on"
    );
}

/// An element keeps an attribute of each type under one name: a fraction set beside a whole number
/// of the same name leaves the number there. (Within one change of state the first value given
/// for a name wins, whatever its type.)
///
/// Behaviour: ui.element.one-name-holds-a-value-of-each-type
#[test]
fn one_name_holds_a_value_of_each_type() {
    let e = element(
        "Holder",
        "field",
        [0, 0, 9, 9],
        0,
        vec![state(
            "stateless",
            0,
            vec![("size", V::Int(3)), ("size", V::Float(0.5))],
            vec![],
            vec![],
        )],
    );
    let mut u = ui(vec![e], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let h = u.create_root(l, Token::of("Holder")).unwrap();
    let n = u.node(h).unwrap();
    assert_eq!(n.int_attr(Token::of("size")), Some(3));
    assert_eq!(
        n.float_attr(Token::of("size")),
        None,
        "the first value given wins"
    );
    u.set_attr(h, Token::of("size"), Value::Float(0.5));
    assert_eq!(u.node(h).unwrap().float_attr(Token::of("size")), Some(0.5));
    u.set_attr(h, Token::of("size"), Value::Bool(true));
    let n = u.node(h).unwrap();
    assert_eq!(
        n.int_attr(Token::of("size")),
        Some(3),
        "untouched by a value of another type"
    );
    assert!(n.bool_attr(Token::of("size")));
}

/// A kept place is the element's corners in the screen's clip space (less a pixel's share): put
/// back on a larger screen, the element stands at the same share of it, to the pixel the
/// conversion's rounding gives (half a pixel of the new screen short across).
///
/// Behaviour: ui.element.a-kept-place-comes-back-at-the-same-share-of-the-screen
#[test]
fn a_kept_place_comes_back_at_the_same_share_of_the_screen() {
    let make = || {
        element(
            "Window",
            "field",
            [256, 192, 355, 291],
            0,
            vec![state(
                "stateless",
                0,
                vec![("save_loc", V::Bool(true))],
                vec![],
                vec![],
            )],
        )
    };
    let mut small = ui(vec![make()], vec![]);
    let l = small.load_layout(LAYOUT).unwrap();
    let h = small.create_root(l, Token::of("Window")).unwrap();
    let kept: Location = small.location(h).expect("a kept place");
    assert_eq!(
        kept.x0, -0.500_976_56,
        "a quarter across, less a pixel's share"
    );
    assert!(
        (kept.y0 - 0.498_698).abs() < 1e-6,
        "a quarter down: {}",
        kept.y0
    );
    let mut large = ui_on(vec![make()], vec![], (2048, 1536));
    let l = large.load_layout(LAYOUT).unwrap();
    let g = large.create_root(l, Token::of("Window")).unwrap();
    assert!(large.apply_location(g, &kept));
    let r = large.screen_rect(g).unwrap();
    assert_eq!(
        (r.x0, r.y0),
        (511, 384),
        "a quarter across and down, as rounded"
    );
}

/// A tree node's lines run from the middle of its side to each child's: laid out down, from the
/// top's middle to each child's bottom middle.
///
/// Behaviour: ui.tree.a-nodes-lines-run-from-its-side-to-each-childs
#[test]
fn a_nodes_lines_run_from_its_side_to_each_childs() {
    let parent = element("Parent", "field", [10, 100, 29, 119], 0, vec![]);
    let child = element("Child", "field", [50, 40, 69, 59], 0, vec![]);
    let mut u = ui(vec![parent, child], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let p = u.create_root(l, Token::of("Parent")).unwrap();
    let c = u.create_root(l, Token::of("Child")).unwrap();
    let mut nodes = Nodes::default();
    let (pn, cn) = (nodes.add(Some(p)), nodes.add(Some(c)));
    nodes.add_child(pn, cn);
    nodes.add_parent(cn, pn);
    assert_eq!(nodes.token(&u, cn), Token::of("Child"));
    let lines = nodes.child_lines(&u, pn, false, false);
    assert_eq!(lines.len(), 1);
    assert_eq!(
        (lines[0].x0, lines[0].y0),
        (19, 100),
        "the parent's top middle"
    );
    assert_eq!(
        (lines[0].x1, lines[0].y1),
        (59, 59),
        "the child's bottom middle"
    );
    nodes.remove(cn);
    assert!(
        nodes.child_lines(&u, pn, false, false).is_empty(),
        "a removed child goes"
    );
}

fn layer(
    width: u32,
    height: u32,
    texels: &[[u8; 4]],
    alpha_only: bool,
) -> auberean_ui::source::Image {
    auberean_ui::source::Image {
        width,
        height,
        bgra: texels.iter().flatten().copied().collect(),
        alpha_only,
    }
}

/// An icon's layer goes over those below by its own alpha: none leaves the texel, whole replaces it
/// and its alpha, between mixes the colors and keeps the alpha below, truncating as the client
/// does; a layer of another size is stretched to fit; one of alpha alone adds nothing.
///
/// Behaviour: ui.media.an-icons-layers-mix-by-their-own-alpha
#[test]
fn an_icons_layers_mix_by_their_own_alpha() {
    use auberean_ui::media::lay_over;
    let under = [100, 100, 100, 40];
    let mut base = layer(4, 1, &[under; 4], false);
    let top = layer(
        4,
        1,
        &[
            [200, 200, 200, 0],
            [10, 20, 30, 255],
            [200, 0, 100, 128],
            [100, 100, 100, 1],
        ],
        false,
    );
    lay_over(&mut base, &top);
    assert_eq!(base.texel(0, 0), under, "no alpha: left");
    assert_eq!(base.texel(1, 0), [10, 20, 30, 255], "whole: replaced");
    // d - (d - s) * 128 / 256, toward zero, the alpha below kept.
    assert_eq!(base.texel(2, 0), [150, 50, 100, 40], "mixed");
    assert_eq!(base.texel(3, 0), under, "the same color stays");

    let mut wide = layer(4, 2, &[under; 8], false);
    let half = layer(2, 1, &[[1, 1, 1, 255], [2, 2, 2, 255]], false);
    lay_over(&mut wide, &half);
    assert_eq!(wide.texel(1, 1), [1, 1, 1, 255], "stretched");
    assert_eq!(wide.texel(2, 0), [2, 2, 2, 255], "stretched");

    let mut kept = layer(1, 1, &[under], false);
    lay_over(&mut kept, &layer(1, 1, &[[255, 255, 255, 200]], true));
    assert_eq!(kept.texel(0, 0), under, "alpha alone adds nothing");
}
