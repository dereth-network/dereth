//! Elements from descriptions: references, attributes, state children, places and depths.

use auberean_assets::ui::AttributeValue as V;
use auberean_ui::token::{attr, state as st};
use auberean_ui::{Rect, Token};

use crate::fixture::{element, image, state, ui, ui_on, LAYOUT};

/// A reference takes its kind from the template it names, its place from where the layout puts
/// it, and children from both: the template's and its own.
///
/// Behaviour: ui.element.a-reference-is-its-template-placed-where-the-reference-stands
#[test]
fn a_reference_is_its_template_placed_where_the_reference_stands() {
    let template = element(
        "Template",
        "button",
        [0, 0, 9, 9],
        0,
        vec![state(
            "stateless",
            0,
            vec![],
            vec![],
            vec![element("Inner", "field", [1, 1, 3, 3], 0, vec![])],
        )],
    );
    let host = element(
        "Host",
        "field",
        [100, 50, 199, 89],
        0,
        vec![state(
            "stateless",
            0,
            vec![],
            vec![],
            vec![element(
                "Placed",
                "reference",
                [10, 20, 49, 39],
                0,
                vec![state(
                    "stateless",
                    0,
                    vec![("referent", V::Id(Token::of("Template").0))],
                    vec![],
                    vec![element("Own", "field", [0, 0, 4, 4], 0, vec![])],
                )],
            )],
        )],
    );
    let mut u = ui(vec![template, host], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let h = u.create_root(l, Token::of("Host")).unwrap();
    let placed = u.child(h, Token::of("Placed")).unwrap();
    let n = u.node(placed).unwrap();
    assert_eq!(n.kind, Token::of("button"));
    assert_eq!(n.rect, Rect::new(10, 20, 49, 39));
    assert_eq!(u.screen_rect(placed), Some(Rect::new(110, 70, 149, 89)));
    assert!(
        u.child(placed, Token::of("Inner")).is_some(),
        "the template's child"
    );
    assert!(
        u.child(placed, Token::of("Own")).is_some(),
        "the reference's own child"
    );
}

/// An attribute takes the first value given: the reference's before the template's, the current
/// state's before the base state's.
///
/// Behaviour: ui.element.the-first-value-given-wins
#[test]
fn the_first_value_given_wins() {
    let template = element(
        "Template",
        "field",
        [0, 0, 9, 9],
        0,
        vec![
            state(
                "stateless",
                0,
                vec![("left_margin", V::Int(1)), ("top_margin", V::Int(1))],
                vec![],
                vec![],
            ),
            state(
                "lit",
                0,
                vec![("left_margin", V::Int(2)), ("right_margin", V::Int(2))],
                vec![],
                vec![],
            ),
        ],
    );
    let r = element(
        "Ref",
        "reference",
        [0, 0, 9, 9],
        0,
        vec![state(
            "stateless",
            0,
            vec![
                ("referent", V::Id(Token::of("Template").0)),
                ("right_margin", V::Int(3)),
            ],
            vec![],
            vec![],
        )],
    );
    let mut u = ui(vec![template, r], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let h = u.create_root(l, Token::of("Ref")).unwrap();
    u.set_state(h, Token::of("lit"));
    let n = u.node(h).unwrap();
    assert_eq!(
        n.int_attr(attr::LEFT_MARGIN),
        Some(2),
        "the state's value before the base's"
    );
    assert_eq!(
        n.int_attr(attr::RIGHT_MARGIN),
        Some(3),
        "the reference's value before the template's"
    );
    assert_eq!(
        n.int_attr(attr::TOP_MARGIN),
        Some(1),
        "the base state's when the state gives none"
    );
}

/// Entering a state makes the children it describes and drops those of the state left; a state
/// marked to pass itself on puts every child into the same state.
///
/// Behaviour: ui.element.a-state-brings-its-children-and-passes-itself-on
#[test]
fn a_state_brings_its_children_and_passes_itself_on() {
    let child = element(
        "Child",
        "field",
        [0, 0, 4, 4],
        0,
        vec![
            state("stateless", 0, vec![], vec![], vec![]),
            state("on", 0, vec![], vec![image(7, 0)], vec![]),
        ],
    );
    let e = element(
        "E",
        "field",
        [0, 0, 49, 49],
        0,
        vec![
            state("stateless", 0, vec![], vec![], vec![child]),
            state(
                "off",
                0,
                vec![],
                vec![],
                vec![element("OffOnly", "field", [0, 0, 1, 1], 0, vec![])],
            ),
            state(
                "on",
                1,
                vec![],
                vec![],
                vec![element("OnOnly", "field", [0, 0, 1, 1], 0, vec![])],
            ),
        ],
    );
    let mut u = ui(vec![e], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let h = u.create_root(l, Token::of("E")).unwrap();
    u.set_state(h, Token::of("off"));
    assert!(u.child(h, Token::of("OffOnly")).is_some());
    u.set_state(h, Token::of("on"));
    assert!(
        u.child(h, Token::of("OffOnly")).is_none(),
        "left with its state"
    );
    assert!(u.child(h, Token::of("OnOnly")).is_some());
    let c = u.child(h, Token::of("Child")).unwrap();
    assert_eq!(u.node(c).unwrap().state, Token::of("on"), "passed on");
    assert_eq!(u.node(c).unwrap().picture.map(|p| p.id), Some(7));
    assert_ne!(u.node(h).unwrap().state, st::STATELESS);
}

/// A child given a picture of its own (added to its state's media, which gives it a
/// description of its own) stays itself when its parent changes state: it keeps its picture,
/// and goes back to the layout's description.
///
/// Behaviour: ui.element.a-child-keeps-its-own-picture-through-its-parents-states
#[test]
fn a_child_keeps_its_own_picture_through_its_parents_states() {
    let icon = element(
        "Icon",
        "field",
        [0, 0, 9, 9],
        0,
        vec![state("stateless", 0, vec![], vec![], vec![])],
    );
    let button = element(
        "Button",
        "field",
        [0, 0, 9, 9],
        0,
        vec![
            state("stateless", 0, vec![], vec![], vec![icon]),
            state("pressed", 0, vec![], vec![], vec![]),
        ],
    );
    let mut u = ui(vec![button], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let h = u.create_root(l, Token::of("Button")).unwrap();
    let c = u.child(h, Token::of("Icon")).unwrap();
    let layout_desc = u.node(c).unwrap().desc;
    assert!(u.add_image_to_state(c, 9, 2, st::STATELESS));
    assert_ne!(
        u.node(c).unwrap().desc,
        layout_desc,
        "a description of its own"
    );
    for s in ["pressed", "stateless"] {
        u.set_state(h, Token::of(s));
        assert_eq!(u.child(h, Token::of("Icon")), Some(c), "the same child");
        assert_eq!(
            u.node(c).unwrap().picture.map(|p| p.id),
            Some(9),
            "its picture kept in {s}"
        );
    }
    assert_eq!(u.node(c).unwrap().desc, layout_desc, "the layout's again");
}

/// Children are kept nearest first: the lower depth in front, and of equal depths the one made
/// later in front.
///
/// Behaviour: ui.element.lower-depth-is-nearer-and-the-newest-of-equals
#[test]
fn lower_depth_is_nearer_and_the_newest_of_equals() {
    let e = element(
        "E",
        "field",
        [0, 0, 99, 99],
        0,
        vec![state(
            "stateless",
            0,
            vec![],
            vec![],
            vec![
                element("Deep", "field", [0, 0, 9, 9], 5, vec![]),
                element("Near", "field", [0, 0, 9, 9], 1, vec![]),
                element("Level", "field", [0, 0, 9, 9], 5, vec![]),
            ],
        )],
    );
    let mut u = ui(vec![e], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let h = u.create_root(l, Token::of("E")).unwrap();
    let names: Vec<Token> = u
        .node(h)
        .unwrap()
        .children
        .iter()
        .map(|c| u.node(*c).unwrap().id)
        .collect();
    assert_eq!(names[0], Token::of("Near"));
    assert_eq!(names.len(), 3);
    // Of the two at depth 5, whichever the client made later stands in front.
    let deep = names.iter().position(|n| *n == Token::of("Deep")).unwrap();
    let level = names.iter().position(|n| *n == Token::of("Level")).unwrap();
    assert_eq!(u.hit(5, 5), Some(u.child(h, Token::of("Near")).unwrap()));
    assert!(deep.min(level) == 1 && deep.max(level) == 2);
}

/// On a screen larger than the layout's, edges keep their distance from the side they follow:
/// 1 from the near side, 2 from the far one, 3 centers the element.
///
/// Behaviour: ui.element.edges-follow-the-screen-as-it-grows
#[test]
fn edges_follow_the_screen_as_it_grows() {
    let mut a = element("Stretch", "field", [10, 10, 1013, 757], 0, vec![]);
    a.edges = [1, 1, 1, 1];
    let mut b = element("Foot", "field", [100, 700, 299, 767], 0, vec![]);
    b.edges = [3, 2, 3, 1];
    let mut u = ui_on(vec![a, b], vec![], (1280, 1024));
    let l = u.load_layout(LAYOUT).unwrap();
    let s = u.create_root(l, Token::of("Stretch")).unwrap();
    let f = u.create_root(l, Token::of("Foot")).unwrap();
    assert_eq!(u.node(s).unwrap().rect, Rect::new(10, 10, 1269, 1013));
    assert_eq!(u.node(f).unwrap().rect, Rect::new(540, 956, 739, 1023));
}

/// An element whose edges follow a larger screen grows only as far as its greatest width and
/// height, and a child of it no less than its least: the size its edges give it is held as a
/// resize holds it.
///
/// Behaviour: ui.element.edges-grow-it-no-larger-than-its-greatest-size
#[test]
fn edges_grow_it_no_larger_than_its_greatest_size() {
    let held = |w: i32, h: i32| {
        vec![
            ("min_width", V::Int(w)),
            ("max_width", V::Int(w)),
            ("min_height", V::Int(h)),
            ("max_height", V::Int(1000)),
        ]
    };
    let mut inner = element("Inner", "field", [0, 0, 99, 99], 0, vec![]);
    inner.edges = [1, 1, 1, 1];
    inner.states.push(state(
        "stateless",
        0,
        vec![("min_width", V::Int(300))],
        vec![],
        vec![],
    ));
    let mut window = element(
        "Window",
        "field",
        [0, 354, 255, 767],
        0,
        vec![state("stateless", 0, held(256, 400), vec![], vec![inner])],
    );
    window.edges = [1, 1, 1, 1];
    let mut u = ui_on(vec![window], vec![], (3840, 2160));
    let l = u.load_layout(LAYOUT).unwrap();
    let w = u.create_root(l, Token::of("Window")).unwrap();
    assert_eq!(u.node(w).unwrap().rect, Rect::new(0, 354, 255, 1353));
    let i = u.child(w, Token::of("Inner")).unwrap();
    assert_eq!(u.node(i).unwrap().rect.width(), 300);
}

/// A top-level element reaching past the screen's bottom is moved up onto it.
///
/// Behaviour: ui.element.a-top-level-element-is-kept-on-the-screen
#[test]
fn a_top_level_element_is_kept_on_the_screen() {
    let mut e = element("Bar", "field", [113, 723, 910, 768], 0, vec![]);
    e.edges = [3, 2, 3, 1];
    let mut u = ui(vec![e], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let h = u.create_root(l, Token::of("Bar")).unwrap();
    assert_eq!(u.node(h).unwrap().rect, Rect::new(113, 722, 910, 767));
}
