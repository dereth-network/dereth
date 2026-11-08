//! The panels' framework on synthetic layouts: listeners outside the tree, page tabs, frameworks
//! shown through their parents, the screen flow, dragging items, elements by name, removal at the
//! next tick, kept places, and the display set up again.

use auberean_assets::ui::AttributeValue as V;
use auberean_ui::framework::flow::{Factory, Screen};
use auberean_ui::framework::Flow;
use auberean_ui::listener::Mail;
use auberean_ui::manager::global;
use auberean_ui::token::msg;
use auberean_ui::{Token, Ui};

use auberean_ui::draw::{self, Surface};
use auberean_ui::Rect;

use crate::fixture::{element, image, solid, state, ui, ui_on, LAYOUT};

fn field(name: &str, rect: [i16; 4], attrs: Vec<(&str, V)>) -> auberean_assets::ui::Element {
    element(
        name,
        "field",
        rect,
        0,
        vec![state("stateless", 0, attrs, vec![], vec![])],
    )
}

/// An element named as never shown stays hidden when asked to show, and so does one made later
/// with that name.
///
/// Behaviour: none (a choice of this client: which elements are hidden is tested where it is made).
#[test]
fn an_element_named_as_never_shown_stays_hidden() {
    let mut u = ui(
        vec![
            field("Made", [0, 0, 9, 9], vec![]),
            field("Later", [0, 0, 9, 9], vec![]),
            field("Other", [0, 0, 9, 9], vec![]),
        ],
        vec![],
    );
    let l = u.load_layout(LAYOUT).unwrap();
    let made = u.create_root(l, Token::of("Made")).unwrap();
    let other = u.create_root(l, Token::of("Other")).unwrap();
    assert!(u.shown(made));

    u.never_show(&[Token::of("Made"), Token::of("Later")]);
    assert!(!u.shown(made));
    u.set_visible(made, true);
    assert!(!u.shown(made));
    let later = u.create_root(l, Token::of("Later")).unwrap();
    assert!(!u.shown(later));
    assert!(u.shown(other));
}

/// A listener outside the element tree hears an element's message only once registered for it,
/// as mail; an undoing waits for the next delivery; a global message registered twice needs two
/// undoings.
///
/// Behaviour: ui.listener.registrations-take-effect-in-order
#[test]
fn registrations_take_effect_in_order() {
    let mut u = ui(vec![field("Window", [0, 0, 99, 99], vec![])], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let w = u.create_root(l, Token::of("Window")).unwrap();
    let me = u.new_listener();
    u.pointer_move(10, 10);
    assert!(
        u.take_mail(me).is_empty(),
        "nothing heard before registering"
    );
    u.listen_for_message(me, Token::of("Window"), msg::MOUSE_PRESS);
    u.pointer_press();
    u.pointer_release();
    let mail = u.take_mail(me);
    assert_eq!(mail.len(), 1, "{mail:?}");
    assert!(
        matches!(mail[0], Mail::Element { from, what, .. } if from == w && what == msg::MOUSE_PRESS)
    );

    u.listen_for_global_message(me, global::TIME);
    u.listen_for_global_message(me, global::TIME);
    u.unregister_for_global_message(me, global::TIME);
    u.use_time(1.0);
    assert!(
        u.take_mail(me).iter().any(|m| matches!(
            m,
            Mail::Global {
                what: global::TIME,
                ..
            }
        )),
        "registered twice, undone once: still heard"
    );
    u.unregister_for_global_message(me, global::TIME);
    u.use_time(2.0);
    assert!(u.take_mail(me).is_empty(), "undone twice: no longer heard");
    u.drop_listener(me);
}

fn tabs() -> Vec<auberean_assets::ui::Element> {
    let tab = |name: &str, page: &str, x: i16, open: bool| {
        let mut t = element(
            name,
            "panel_page",
            [x, 0, x + 49, 19],
            0,
            vec![
                state("open", 0, vec![], vec![], vec![]),
                state("closed", 0, vec![], vec![], vec![]),
                state("rollover", 0, vec![], vec![], vec![]),
                state(
                    "stateless",
                    0,
                    vec![("page", V::Id(Token::of(page).0))],
                    vec![],
                    vec![],
                ),
            ],
        );
        t.default_state = Token::of(if open { "open" } else { "closed" }).0;
        t
    };
    let panel = element(
        "Tabs",
        "panel",
        [0, 0, 199, 199],
        0,
        vec![state(
            "stateless",
            0,
            vec![],
            vec![],
            vec![
                tab("TabA", "PageA", 0, true),
                tab("TabB", "PageB", 60, false),
            ],
        )],
    );
    vec![
        panel,
        field("PageA", [0, 30, 199, 199], vec![]),
        field("PageB", [0, 30, 199, 199], vec![]),
    ]
}

/// A panel's tabs each make their page beside them; only an open tab's page shows; pressing a
/// closed tab opens it and closes the one that was open; a closed tab rolls over under the
/// pointer.
///
/// Behaviour: ui.panel.opening-a-tab-closes-the-others
#[test]
fn opening_a_tab_closes_the_others() {
    let mut u = ui(tabs(), vec![]);
    let panel = u
        .create_root_element_by_data_id(LAYOUT, Token::of("Tabs"))
        .unwrap();
    let a = u.child(panel, Token::of("TabA")).unwrap();
    let b = u.child(panel, Token::of("TabB")).unwrap();
    let page_a = u.node(a).unwrap().page.expect("tab A made its page");
    let page_b = u.node(b).unwrap().page.expect("tab B made its page");
    assert_eq!(u.node(page_a).unwrap().parent, Some(panel));
    assert!(u.node(page_a).unwrap().visible && !u.node(page_b).unwrap().visible);

    u.pointer_move(70, 10);
    assert_eq!(u.node(b).unwrap().state, Token::of("rollover"));
    u.pointer_press();
    u.pointer_release();
    assert_eq!(u.node(b).unwrap().state, Token::of("open"));
    assert_eq!(u.node(a).unwrap().state, Token::of("closed"));
    assert!(!u.node(page_a).unwrap().visible && u.node(page_b).unwrap().visible);
}

/// A panel's framework shows only while it and every framework above it show; showing a
/// framework tells each child to show again by its own flag.
///
/// Behaviour: ui.frame.a-panel-shows-only-while-its-parents-do
#[test]
fn a_panel_shows_only_while_its_parents_do() {
    let mut u = ui(vec![field("Window", [0, 0, 99, 99], vec![])], vec![]);
    let screen = u.new_main_frame();
    let panel = u.new_child_frame(screen);
    let inner = u.new_child_frame(panel);
    assert!(u.frame_shown(inner));
    u.frame_show(panel, false);
    assert!(!u.frame_shown(inner) && !u.frame_shown(panel) && u.frame_shown(screen));
    let mail = u.take_mail(u.frame_listener(inner).unwrap());
    assert_eq!(
        mail,
        vec![Mail::Show { shown: true }],
        "told to show by its own flag"
    );
    u.frame_show(panel, true);
    assert!(u.frame_shown(inner));

    let root = u
        .frame_create_root_by_data_id(inner, LAYOUT, Token::of("Window"))
        .unwrap();
    assert_eq!(
        u.frame_roots(screen),
        vec![root],
        "kept by the screen's top"
    );
    u.drop_frame(inner);
    assert_eq!(u.frame_find_child(panel, inner), None);
    u.drop_frame(screen);
    assert!(
        u.node(root).is_none(),
        "the screen's top removes what it made"
    );
}

struct Named(&'static str);

impl Screen<Vec<&'static str>> for Named {
    fn show(&mut self, _ui: &mut Ui, data: &mut Vec<&'static str>, on: bool) {
        data.push(if on { "show" } else { "hide" });
    }
    fn update(&mut self, _ui: &mut Ui, _data: &mut Vec<&'static str>) {}
    fn argument(&mut self, _ui: &mut Ui, data: &mut Vec<&'static str>, argument: &str) {
        if argument == "why" {
            data.push("argument");
        }
    }
    fn close(self: Box<Self>, _ui: &mut Ui, data: &mut Vec<&'static str>) {
        data.push(self.0);
    }
}

fn first(_ui: &mut Ui, data: &mut Vec<&'static str>) -> Option<Box<dyn Screen<Vec<&'static str>>>> {
    data.push("make first");
    Some(Box::new(Named("close first")))
}

fn second(
    _ui: &mut Ui,
    data: &mut Vec<&'static str>,
) -> Option<Box<dyn Screen<Vec<&'static str>>>> {
    data.push("make second");
    Some(Box::new(Named("close second")))
}

/// A screen asked for comes up at the next tick of the clock: the one up hidden and closed, the
/// new one made, given its text and shown.
///
/// Behaviour: ui.flow.a-screen-asked-for-comes-up-at-the-next-tick
#[test]
fn a_screen_asked_for_comes_up_at_the_next_tick() {
    let mut u = ui(vec![], vec![]);
    let mut flow = Flow::new(&mut u, Vec::new());
    flow.register(Token::of("First"), first as Factory<_>);
    flow.register(Token::of("Second"), second as Factory<_>);
    flow.queue_mode(Token::of("First"), "");
    flow.tick(&mut u);
    assert!(flow.data.is_empty(), "nothing before the clock ticks");
    u.use_time(0.1);
    flow.tick(&mut u);
    assert_eq!(flow.data, vec!["make first", "show"]);
    flow.data.clear();
    flow.queue_mode(Token::of("Second"), "why");
    u.use_time(0.2);
    flow.tick(&mut u);
    assert_eq!(
        flow.data,
        vec!["hide", "close first", "make second", "argument", "show"]
    );
    assert_eq!(flow.mode(), Token::of("Second"));
    flow.close(&mut u);
}

fn drag_layout() -> Vec<auberean_assets::ui::Element> {
    let item = element(
        "Item",
        "field",
        [0, 0, 19, 19],
        0,
        vec![state(
            "stateless",
            0,
            vec![("dragable", V::Bool(true))],
            vec![],
            vec![],
        )],
    );
    let bag = element(
        "Bag",
        "field",
        [10, 10, 59, 59],
        0,
        vec![state(
            "stateless",
            0,
            vec![("container", V::Bool(true))],
            vec![],
            vec![item],
        )],
    );
    let chest = field(
        "Chest",
        [100, 10, 149, 59],
        vec![("container", V::Bool(true))],
    );
    let wall = field("Wall", [200, 10, 249, 59], vec![]);
    let window = element(
        "Window",
        "field",
        [0, 0, 299, 99],
        0,
        vec![state(
            "stateless",
            0,
            vec![],
            vec![],
            vec![bag, chest, wall],
        )],
    );
    vec![window]
}

/// A press on an item followed by one long move, the pointer leaving the item in a single step,
/// drags that item: what the press was on, not what the pointer has come to.
///
/// Behaviour: ui.drag.one-long-move-drags-what-the-press-was-on
#[test]
fn one_long_move_drags_what_the_press_was_on() {
    let mut u = ui(drag_layout(), vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let w = u.create_root(l, Token::of("Window")).unwrap();
    let bag = u.child(w, Token::of("Bag")).unwrap();
    u.pointer_move(15, 15);
    u.pointer_press();
    u.pointer_move(120, 30);
    assert!(u.dragged().is_some(), "dragging");
    assert_eq!(u.drag_owner(), Some(bag));
}

/// An item held and moved far enough is dragged out of its container as a copy that follows the
/// pointer, carrying what the item was given; dropped on a container it is taken in, dropped on
/// anything else it comes back. The copy dropped is kept, hidden, for the listeners that read
/// their mail after the drop, until the next drag starts.
///
/// Behaviour: ui.drag.an-item-dragged-is-taken-by-a-container-or-comes-back
#[test]
fn an_item_dragged_is_taken_by_a_container_or_comes_back() {
    let mut u = ui(drag_layout(), vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let w = u.create_root(l, Token::of("Window")).unwrap();
    let bag = u.child(w, Token::of("Bag")).unwrap();
    let item = u.child(bag, Token::of("Item")).unwrap();

    u.set_attr(
        item,
        Token::of("ShortcutData"),
        auberean_ui::Value::InstanceId(7),
    );
    u.pointer_move(15, 15);
    u.pointer_press();
    u.pointer_move(16, 16);
    assert!(u.dragged().is_none(), "a short move is still a press");
    u.pointer_move(20, 20);
    u.pointer_move(30, 30);
    let copy = u.dragged().expect("dragging");
    assert_ne!(copy, item, "the copy goes, the item stays");
    assert_eq!(u.drag_owner(), Some(bag));
    assert_eq!(
        u.screen_rect(copy).unwrap().x0,
        30,
        "its corner at the pointer: pressed inside the item"
    );
    u.pointer_move(120, 30);
    u.take_messages();
    u.pointer_release();
    let said: Vec<(Token, u32)> = u
        .take_messages()
        .iter()
        .filter(|m| m.what == Token::of("stop_drag_item"))
        .map(|m| (m.element, m.param))
        .collect();
    assert_eq!(said, vec![(Token::of("Chest"), 1)], "the chest took it");
    assert!(u.dragged().is_none());
    assert_eq!(u.dropped(), Some(copy), "the copy kept for the listeners");
    assert!(!u.shown(copy), "hidden");
    assert_eq!(
        u.node(copy)
            .and_then(|n| n.instance_id_attr(Token::of("ShortcutData"))),
        Some(7),
        "it carries what the item was given"
    );

    u.pointer_move(15, 15);
    u.pointer_press();
    u.use_time(1.0);
    u.pointer_move(220, 30);
    assert!(u.dragged().is_some(), "held long enough");
    assert!(
        u.node(copy).is_none(),
        "the last drop's copy gone with the next drag"
    );
    u.pointer_release();
    let said: Vec<(Token, u32)> = u
        .take_messages()
        .iter()
        .filter(|m| m.what == Token::of("stop_drag_item"))
        .map(|m| (m.element, m.param))
        .collect();
    assert_eq!(
        said,
        vec![(Token::of("Bag"), 0)],
        "nothing took it: it came back"
    );
}

/// On a screen larger than its layout's, the copy of an item whose edges follow its parent's
/// grows with the screen (it has no parent), and it is shown only in the item's own size, its
/// corner at the pointer: what it draws past that is not shown.
///
/// Behaviour: ui.drag.the-dragged-copy-shows-in-its-items-size
#[test]
fn the_dragged_copy_shows_in_its_items_size() {
    let mut layout = drag_layout();
    // The window and the bag keep their size; the item follows the bag's edges.
    layout[0].edges = [0; 4];
    let bag = &mut layout[0].states[0].children[0];
    bag.edges = [0; 4];
    bag.states[0].children[0].states[0].media.push(image(9, 0));
    let mut u = ui_on(
        layout,
        vec![(9, solid(20, 20, [255, 0, 0, 255]))],
        (1600, 1200),
    );
    let l = u.load_layout(LAYOUT).unwrap();
    let w = u.create_root(l, Token::of("Window")).unwrap();
    let bag = u.child(w, Token::of("Bag")).unwrap();
    let item = u.child(bag, Token::of("Item")).unwrap();
    assert_eq!(u.screen_rect(item).unwrap(), Rect::sized(10, 10, 20, 20));

    u.pointer_move(15, 15);
    u.pointer_press();
    u.pointer_move(20, 20);
    u.pointer_move(300, 300);
    let copy = u.dragged().expect("dragging");
    assert!(
        u.screen_rect(copy).unwrap().width() > 20,
        "the copy grows with the screen"
    );
    assert_eq!(u.drag_area(), Some(Rect::sized(300, 300, 20, 20)));
    u.pointer_move(400, 350);
    assert_eq!(
        u.drag_area(),
        Some(Rect::sized(400, 350, 20, 20)),
        "it follows"
    );
    let mut s = Surface::new(1600, 1200);
    draw::draw(&u, &mut s);
    assert_eq!(s.get(410, 360), [255, 0, 0, 255], "the item's picture");
    assert_eq!(s.get(425, 360), [0, 0, 0, 0], "nothing right of its size");
    assert_eq!(s.get(410, 375), [0, 0, 0, 0], "nothing under it");
    u.pointer_release();
}

/// An element is found by name, the oldest first, or within one root; an element removed is found
/// no more.
///
/// Behaviour: ui.manager.elements-by-name-and-removal
#[test]
fn elements_by_name_and_removal() {
    let mut u = ui(vec![field("Window", [0, 0, 99, 99], vec![])], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let a = u.create_root(l, Token::of("Window")).unwrap();
    let b = u.create_root(l, Token::of("Window")).unwrap();
    assert_eq!(u.element(Token::of("Window")), Some(a));
    u.remove(a);
    assert!(u.node(a).is_none());
    assert_eq!(u.element(Token::of("Window")), Some(b));
}

/// A window whose place is kept is put back where it was, after the player moved it; one whose
/// edges stay where they are placed keeps its place when the display is set up again at another
/// size.
///
/// Behaviour: ui.frame.kept-places-come-back
#[test]
fn kept_places_come_back() {
    let mut window = field("Window", [0, 0, 99, 99], vec![("save_loc", V::Bool(true))]);
    window.edges = [0, 0, 0, 0];
    let mut u = ui(vec![window], vec![]);
    let screen = u.new_main_frame();
    let w = u
        .frame_create_root_by_data_id(screen, LAYOUT, Token::of("Window"))
        .unwrap();
    u.move_to(w, 300, 200);
    let kept = u.frame_inq_locations(screen);
    u.move_to(w, 0, 0);
    assert!(u.frame_apply_locations(screen, &kept));
    let r = u.screen_rect(w).unwrap();
    assert_eq!((r.x0, r.y0), (300, 200));
    u.refresh(1280, 960);
    assert_eq!(u.display(), (1280, 960));
    assert_eq!(
        u.screen_rect(w).unwrap().x0,
        300,
        "a moved window keeps its place"
    );
}

/// A new kept place, before any element's place is read into it, is hidden with every corner at
/// -3: off the screen, whose clip space runs from -1 to 1.
///
/// Behaviour: ui.frame.a-new-kept-place-is-hidden-off-the-screen
#[test]
fn a_new_kept_place_is_hidden_off_the_screen() {
    let p = auberean_ui::location::Location::default();
    assert!(!p.visible);
    assert_eq!((p.x0, p.y0, p.x1, p.y1), (-3.0, -3.0, -3.0, -3.0));
}
