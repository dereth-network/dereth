//! Lists, scroll bars, meters, menus and trees on synthetic layouts: scrolling by arrows, the
//! wheel, the scroll bar's widget and smoothly over the clock; meters reaching their goals; a
//! menu's popup; a tree's lines.

use auberean_assets::ui::{AttributeValue as V, Element};
use auberean_ui::draw::{self, Surface};
use auberean_ui::token::{attr, msg};
use auberean_ui::types::button_id;
use auberean_ui::{Handle, Token, Ui, Value};

use crate::fixture::{element, state, ui, LAYOUT};

fn id(name: &str) -> V {
    V::Id(Token::of(name).0)
}

/// A row 100 by 20.
fn row() -> Element {
    element(
        "Row",
        "text",
        [0, 0, 99, 19],
        0,
        vec![state("stateless", 0, vec![], vec![], vec![])],
    )
}

/// A window holding a list two rows tall at (10, 10), with `extra` attributes and siblings.
fn window(list_attrs: Vec<(&str, V)>, siblings: Vec<Element>) -> Element {
    let list = element(
        "List",
        "listbox",
        [10, 10, 109, 49],
        0,
        vec![state("stateless", 0, list_attrs, vec![], vec![])],
    );
    let mut kids = vec![list];
    kids.extend(siblings);
    element(
        "Window",
        "field",
        [0, 0, 299, 299],
        0,
        vec![state("stateless", 0, vec![], vec![], kids)],
    )
}

/// The interface with the window up and five rows in its list.
fn five_rows(elements: Vec<Element>) -> (Ui, Handle, Vec<Handle>) {
    let mut u = ui(elements, vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let w = u.create_root(l, Token::of("Window")).unwrap();
    let list = u.child(w, Token::of("List")).unwrap();
    let rows = (0..5)
        .map(|_| {
            let r = u.create(l, Token::of("Row")).unwrap();
            u.list_add(list, r);
            r
        })
        .collect();
    (u, list, rows)
}

fn hot_button(name: &str, rect: [i16; 4]) -> Element {
    element(
        name,
        "button",
        rect,
        0,
        vec![state(
            "stateless",
            0,
            vec![("hot_button", V::Bool(true))],
            vec![],
            vec![],
        )],
    )
}

fn ghosted(u: &Ui, w: Handle, name: &str) -> bool {
    let b = u.child(w, Token::of(name)).unwrap();
    u.node(b).unwrap().bool_attr(attr::GHOSTED)
}

fn y_view(u: &Ui, list: Handle) -> i32 {
    u.node(list).unwrap().int_attr(Token::of("y_view")).unwrap()
}

fn click(u: &mut Ui, x: i32, y: i32) {
    u.pointer_move(x, y);
    u.pointer_press();
    u.pointer_release();
}

/// A row taken out of a list is no longer drawn or pressed where it stood, and the rows after it
/// move up; put back, it is in the list again.
///
/// Behaviour: ui.list.a-row-taken-out-stands-nowhere
#[test]
fn a_row_taken_out_of_a_list_stands_nowhere() {
    let (mut u, list, rows) = five_rows(vec![row(), window(vec![], vec![])]);
    let on_top = u.screen_rect(rows[0]).unwrap();
    let taken = u.list_remove(list, 0).expect("a row");
    assert_eq!(taken, rows[0]);
    assert_eq!(u.node(taken).and_then(|n| n.parent), None);
    assert!(!u.node(list).unwrap().children.contains(&taken));
    assert_eq!(
        u.screen_rect(rows[1]).unwrap().y0,
        on_top.y0,
        "the next row in its place"
    );
    assert_eq!(u.hit(on_top.x0 + 5, on_top.y0 + 5), Some(rows[1]));
    u.list_insert(list, taken, 0);
    assert_eq!(u.node(taken).and_then(|n| n.parent), Some(list));
    assert_eq!(u.list_index_of(list, taken), Some(0));
    assert_eq!(u.screen_rect(taken).unwrap().y0, on_top.y0);
}

/// A list two rows tall over five rows: its down arrow scrolls it a row at a time (the rows move
/// up by a row's height) until the last full view; the up arrow is ghosted at the top and the down
/// arrow at the bottom.
///
/// Behaviour: ui.list.arrow-buttons-scroll-a-row-and-ghost-at-the-ends
#[test]
fn arrow_buttons_scroll_a_row_and_ghost_at_the_ends() {
    let (mut u, list, rows) = five_rows(vec![
        row(),
        window(
            vec![("up_button", id("Up")), ("down_button", id("Down"))],
            vec![
                hot_button("Up", [120, 10, 139, 29]),
                hot_button("Down", [120, 30, 139, 49]),
            ],
        ),
    ]);
    let w = u.roots()[0];
    assert!(ghosted(&u, w, "Up"), "the top: up is ghosted");
    assert!(!ghosted(&u, w, "Down"));
    click(&mut u, 130, 40);
    assert_eq!(y_view(&u, list), 20, "a row on");
    assert_eq!(
        u.screen_rect(rows[1]).unwrap().y0,
        10,
        "the second row at the top"
    );
    assert!(!ghosted(&u, w, "Up"));
    click(&mut u, 130, 40);
    click(&mut u, 130, 40);
    assert_eq!(y_view(&u, list), 60, "the last full view");
    assert!(ghosted(&u, w, "Down"), "the bottom: down is ghosted");
    click(&mut u, 130, 40);
    assert_eq!(y_view(&u, list), 60, "a ghosted arrow does nothing");
}

/// A list's vertical scroll bar, its widget placed by the bar alone (as the layouts' widgets are).
fn bar() -> Element {
    let mut widget = element("widget", "field", [0, 0, 14, 9], 0, vec![]);
    widget.edges = [0, 0, 0, 0];
    element(
        "Bar",
        "scrollbar",
        [110, 10, 124, 49],
        0,
        vec![state(
            "stateless",
            0,
            vec![
                ("vertical", V::Bool(true)),
                ("proportional", V::Bool(true)),
                ("move_to_touched_position", V::Bool(true)),
            ],
            vec![],
            vec![widget],
        )],
    )
}

/// The wheel turned down over a list goes to its scroll bar, which tells the list to scroll a
/// line: the list moves a row on, and the bar's position follows the view.
///
/// Behaviour: ui.list.the-wheel-scrolls-a-list-through-its-scroll-bar
#[test]
fn the_wheel_scrolls_a_list_through_its_scroll_bar() {
    let (mut u, list, _) = five_rows(vec![
        row(),
        window(vec![("vertical_scrollbar", id("Bar"))], vec![bar()]),
    ]);
    let w = u.roots()[0];
    let b = u.child(w, Token::of("Bar")).unwrap();
    let size = u.node(b).unwrap().float_attr(attr::Y_WIDGET_SIZE).unwrap();
    assert!(
        (size - 0.4).abs() < 1e-6,
        "two rows of five in view: {size}"
    );
    u.pointer_move(50, 20);
    u.pointer_button(button_id::WHEEL_DOWN, true);
    assert_eq!(y_view(&u, list), 20);
    let p = u.node(b).unwrap().float_attr(attr::Y_POSITION).unwrap();
    assert!((p - 1.0 / 3.0).abs() < 1e-6, "a third of the way: {p}");
    u.pointer_button(button_id::WHEEL_UP, true);
    assert_eq!(y_view(&u, list), 0);
}

/// The wheel over a window's bare ground, or over something in it that is not a list, scrolls the
/// window's list by its bar.
///
/// Behaviour: ui.scrollbar.the-wheel-scrolls-the-window-under-the-pointer
#[test]
fn the_wheel_scrolls_the_window_under_the_pointer() {
    let label = element("Label", "field", [200, 200, 280, 280], 0, vec![]);
    let (mut u, list, _) = five_rows(vec![
        row(),
        window(vec![("vertical_scrollbar", id("Bar"))], vec![bar(), label]),
    ]);
    u.pointer_move(150, 150);
    u.pointer_button(button_id::WHEEL_DOWN, true);
    assert_eq!(y_view(&u, list), 20, "over the window's ground");
    u.pointer_move(250, 250);
    u.pointer_button(button_id::WHEEL_DOWN, true);
    assert_eq!(y_view(&u, list), 40, "over a label in it");
    u.pointer_button(button_id::WHEEL_UP, true);
    assert_eq!(y_view(&u, list), 20, "back up");
}

/// The wheel over another window, or nothing, leaves a window's list alone, and so does a wheel
/// over a window whose bar is hidden.
///
/// Behaviour: ui.scrollbar.the-wheel-leaves-other-windows-alone
#[test]
fn the_wheel_leaves_other_windows_alone() {
    let other = element("Other", "field", [400, 0, 499, 99], 0, vec![]);
    let mut u = ui(
        vec![
            row(),
            window(vec![("vertical_scrollbar", id("Bar"))], vec![bar()]),
            other,
        ],
        vec![],
    );
    let l = u.load_layout(LAYOUT).unwrap();
    let w = u.create_root(l, Token::of("Window")).unwrap();
    let list = u.child(w, Token::of("List")).unwrap();
    for _ in 0..5 {
        let r = u.create(l, Token::of("Row")).unwrap();
        u.list_add(list, r);
    }
    u.create_root(l, Token::of("Other")).unwrap();
    u.pointer_move(450, 50);
    u.pointer_button(button_id::WHEEL_DOWN, true);
    assert_eq!(y_view(&u, list), 0, "over another window");
    u.pointer_move(600, 600);
    u.pointer_button(button_id::WHEEL_DOWN, true);
    assert_eq!(y_view(&u, list), 0, "over nothing");
    let b = u.child(w, Token::of("Bar")).unwrap();
    u.set_visible(b, false);
    u.pointer_move(150, 150);
    u.pointer_button(button_id::WHEEL_DOWN, true);
    assert_eq!(y_view(&u, list), 0, "its bar hidden");
}

/// Dragging a scroll bar's widget moves the bar to the widget's share of the way, and the list
/// takes that share of its hidden rows (kept on a row's edge).
///
/// Behaviour: ui.scrollbar.dragging-the-widget-scrolls-its-list
#[test]
fn dragging_the_widget_scrolls_its_list() {
    let (mut u, list, _) = five_rows(vec![
        row(),
        window(vec![("vertical_scrollbar", id("Bar"))], vec![bar()]),
    ]);
    let w = u.roots()[0];
    let b = u.child(w, Token::of("Bar")).unwrap();
    let widget = u.child(b, Token::of("widget")).unwrap();
    let wr = u.screen_rect(widget).unwrap();
    assert_eq!(wr.height(), 16, "the widget is the share in view");
    u.pointer_move(115, wr.y0 + 5);
    u.pointer_press();
    u.pointer_move(115, wr.y0 + 5 + 12);
    let p = u.node(b).unwrap().float_attr(attr::Y_POSITION).unwrap();
    assert!((p - 0.5).abs() < 1e-6, "half way: {p}");
    assert_eq!(y_view(&u, list), 40, "half of 60, on a row's edge");
    u.pointer_release();
    assert!(u
        .take_messages()
        .iter()
        .any(|m| m.what == Token::of("widget_release")));
}

/// A list that comes to show all its rows disables its bar and leaves the widget as it was laid out
/// just before: long, the bar's whole length, and showing. Its rows laid out again, it stays. The
/// window updated, which lays the disabled bar out afresh, takes the widget away. More rows than
/// show, and the widget is back, the share in view.
///
/// Behaviour: ui.scrollbar.a-list-showing-all-its-rows-leaves-the-widget-long
#[test]
fn a_list_showing_all_its_rows_leaves_the_widget_long() {
    let (mut u, list, rows) = five_rows(vec![
        row(),
        window(vec![("vertical_scrollbar", id("Bar"))], vec![bar()]),
    ]);
    let w = u.roots()[0];
    let b = u.child(w, Token::of("Bar")).unwrap();
    let widget = u.child(b, Token::of("widget")).unwrap();
    assert_eq!(u.screen_rect(widget).unwrap().height(), 16);
    for r in &rows[1..] {
        u.list_delete(list, *r);
    }
    let whole = u.screen_rect(b).unwrap().height();
    let long = |u: &Ui| {
        assert!(u.is_visible(widget), "showing");
        assert_eq!(u.screen_rect(widget).unwrap().height(), whole);
    };
    long(&u);
    assert!(u.node(b).unwrap().bool_attr(Token::of("disabled")));
    u.list_update_and_draw(list);
    long(&u);
    u.update(w);
    assert!(!u.is_visible(widget), "laid out while disabled");
    let l = u.load_layout(LAYOUT).unwrap();
    for _ in 0..4 {
        let r = u.create(l, Token::of("Row")).unwrap();
        u.list_add(list, r);
    }
    assert!(u.is_visible(widget));
    assert!(!u.node(b).unwrap().bool_attr(Token::of("disabled")));
    assert_eq!(u.screen_rect(widget).unwrap().height(), 16);
}

/// A smooth list glides to its goal: half way after half its movement time, there at the end,
/// and the goal is then forgotten.
///
/// Behaviour: ui.list.a-smooth-list-glides-to-its-goal-over-its-movement-time
#[test]
fn a_smooth_list_glides_to_its_goal_over_its_movement_time() {
    let (mut u, list, _) = five_rows(vec![
        row(),
        window(vec![("smooth_movement", V::Bool(true))], vec![]),
    ]);
    u.use_time(10.0);
    u.set_attr(list, Token::of("goal_y_view"), Value::Int(40));
    assert_eq!(y_view(&u, list), 0, "not yet");
    u.use_time(10.125);
    assert_eq!(y_view(&u, list), 20, "half way after half the time");
    u.use_time(10.5);
    assert_eq!(y_view(&u, list), 40, "there");
    assert_eq!(
        u.node(list).unwrap().int_attr(Token::of("goal_y_view")),
        Some(-1_000_000),
        "no goal"
    );
}

/// A meter that moves its fills slides each whole: it keeps the size the layout gave it, a left
/// fill stands the part of the meter's width past the position before the meter, cut to a whole
/// pixel toward the meter, and a right fill at the position.
///
/// Behaviour: ui.meter.a-moving-fill-slides-keeping-its-own-size
#[test]
fn a_moving_fill_slides_keeping_its_own_size() {
    let fill = |name: &str, r: [i16; 4]| element(name, "field", r, 0, vec![]);
    let meter = element(
        "Meter",
        "meter",
        [10, 10, 110, 39],
        0,
        vec![state(
            "stateless",
            0,
            vec![("move_fill", V::Bool(true))],
            vec![],
            vec![
                fill("left_fill", [0, 0, 100, 22]),
                fill("right_fill", [0, 0, 100, 22]),
            ],
        )],
    );
    let mut u = ui(vec![meter], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let m = u.create_root(l, Token::of("Meter")).unwrap();
    u.set_meter(m, 0.5);
    let rect =
        |u: &auberean_ui::Ui, n: &str| u.node(u.child(m, Token::of(n)).unwrap()).unwrap().rect;
    assert_eq!(
        rect(&u, "left_fill"),
        auberean_ui::Rect::sized(-50, 0, 101, 23)
    );
    assert_eq!(
        rect(&u, "right_fill"),
        auberean_ui::Rect::sized(50, 0, 101, 23)
    );
}

/// A meter with a left fill: a goal is taken at once (the fill half the meter at 0.5); a smooth
/// meter moves toward it over its movement time.
///
/// Behaviour: ui.meter.a-meter-takes-its-goal-at-once-or-smoothly
#[test]
fn a_meter_takes_its_goal_at_once_or_smoothly() {
    let meter = |name: &str, smooth: bool| {
        element(
            name,
            "meter",
            [0, 0, 99, 9],
            0,
            vec![state(
                "stateless",
                0,
                vec![("smooth_movement", V::Bool(smooth))],
                vec![],
                vec![element("left_fill", "field", [0, 0, 99, 9], 0, vec![])],
            )],
        )
    };
    let mut u = ui(vec![meter("Plain", false), meter("Smooth", true)], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let plain = u.create_root(l, Token::of("Plain")).unwrap();
    let smooth = u.create_root(l, Token::of("Smooth")).unwrap();
    let fill = |u: &Ui, m| {
        u.node(u.child(m, attr::LEFT_FILL).unwrap())
            .unwrap()
            .rect
            .width()
    };
    u.use_time(1.0);
    u.set_attr(plain, attr::GOAL_POSITION, Value::Float(0.5));
    assert_eq!(fill(&u, plain), 50);
    u.set_attr(smooth, attr::GOAL_POSITION, Value::Float(1.0));
    assert_eq!(fill(&u, smooth), 0, "not yet");
    u.use_time(1.125);
    assert_eq!(fill(&u, smooth), 50, "half way after half the time");
    u.use_time(2.0);
    assert_eq!(fill(&u, smooth), 100);
}

/// A list emptied row by row keeps its cells' size and one cell of sheet, so the view it stood at
/// stays; inserting past its end, or before a row not its own, adds nothing.
///
/// Behaviour: ui.list.an-emptied-list-keeps-its-cell-and-a-row-past-the-end-is-refused
#[test]
fn an_emptied_list_keeps_its_cell_and_a_row_past_the_end_is_refused() {
    let (mut u, list, rows) = five_rows(vec![window(vec![], vec![]), row()]);
    let cell = u.node(rows[0]).unwrap().rect.height();
    let l = u.load_layout(LAYOUT).unwrap();
    let extra = u.create(l, Token::of("Row")).unwrap();
    u.list_insert(list, extra, 6);
    assert_eq!(u.list_items(list).len(), 5, "past the end: none");
    u.list_insert_before(list, extra, Some(list));
    assert_eq!(
        u.list_items(list).len(),
        5,
        "before no row of its own: none"
    );
    u.list_insert_before(list, extra, None);
    assert_eq!(u.list_items(list).len(), 6, "before nothing: at the end");
    u.list_remove(list, 5);
    u.list_scroll_to_row(list, 1, true);
    assert_eq!(y_view(&u, list), cell, "a row down");
    for _ in 0..5 {
        u.list_remove(list, 0);
    }
    assert_eq!(
        y_view(&u, list),
        cell,
        "the view stays, on one cell of sheet"
    );
}

/// An open menu closes as the window it stands in hides: its popup, its own top-level element,
/// does not stay on the screen.
///
/// Behaviour: ui.menu.an-open-menu-closes-as-its-window-hides
#[test]
fn an_open_menu_closes_as_its_window_hides() {
    let popup = element(
        "Popup",
        "field",
        [0, 0, 79, 59],
        0,
        vec![state(
            "stateless",
            0,
            vec![],
            vec![],
            vec![element(
                "MenuList",
                "listbox",
                [0, 0, 79, 59],
                0,
                vec![state(
                    "stateless",
                    0,
                    vec![("click_select", V::Bool(true))],
                    vec![],
                    vec![],
                )],
            )],
        )],
    );
    let item = element(
        "Item",
        "text",
        [0, 0, 79, 14],
        0,
        vec![state("stateless", 0, vec![], vec![], vec![])],
    );
    let menu = element(
        "Menu",
        "menu",
        [10, 10, 89, 29],
        0,
        vec![state(
            "stateless",
            0,
            vec![
                ("popup", id("Popup")),
                ("listbox", id("MenuList")),
                ("text_item", id("Item")),
            ],
            vec![],
            vec![],
        )],
    );
    let win = element(
        "Window",
        "field",
        [0, 0, 299, 299],
        0,
        vec![state("stateless", 0, vec![], vec![], vec![menu])],
    );
    let mut u = ui(vec![popup, item, win], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let w = u.create_root(l, Token::of("Window")).unwrap();
    let m = u.child(w, Token::of("Menu")).unwrap();
    u.menu_insert_text(m, "One", 0).unwrap();
    click(&mut u, 50, 20);
    let popup = u
        .roots()
        .iter()
        .copied()
        .find(|r| u.node(*r).unwrap().id == Token::of("Popup"))
        .expect("the popup is up");
    assert!(u.shown(popup));
    u.set_visible(w, false);
    assert!(!u.shown(popup), "gone with its window");
}

/// A menu opens its popup under itself, sized to its rows; choosing a row selects it, the menu
/// says what was chosen, and letting go on the list closes the popup.
///
/// Behaviour: ui.menu.a-menu-opens-its-list-and-closes-on-a-choice
#[test]
fn a_menu_opens_its_list_and_closes_on_a_choice() {
    let popup = element(
        "Popup",
        "field",
        [0, 0, 79, 59],
        0,
        vec![state(
            "stateless",
            0,
            vec![],
            vec![],
            vec![element(
                "MenuList",
                "listbox",
                [0, 0, 79, 59],
                0,
                vec![state(
                    "stateless",
                    0,
                    vec![("click_select", V::Bool(true))],
                    vec![],
                    vec![],
                )],
            )],
        )],
    );
    let item = element(
        "Item",
        "text",
        [0, 0, 79, 14],
        0,
        vec![state("stateless", 0, vec![], vec![], vec![])],
    );
    let menu = element(
        "Menu",
        "menu",
        [10, 10, 89, 29],
        0,
        vec![state(
            "stateless",
            0,
            vec![
                ("popup", id("Popup")),
                ("listbox", id("MenuList")),
                ("text_item", id("Item")),
            ],
            vec![],
            vec![],
        )],
    );
    let win = element(
        "Window",
        "field",
        [0, 0, 299, 299],
        0,
        vec![state("stateless", 0, vec![], vec![], vec![menu])],
    );
    let mut u = ui(vec![popup, item, win], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let w = u.create_root(l, Token::of("Window")).unwrap();
    let m = u.child(w, Token::of("Menu")).unwrap();
    u.menu_insert_text(m, "One", 0).unwrap();
    u.menu_insert_text(m, "Two", 1).unwrap();
    assert_eq!(u.menu_len(m), 2);
    click(&mut u, 50, 20);
    let popup = u
        .roots()
        .iter()
        .copied()
        .find(|r| u.node(*r).unwrap().id == Token::of("Popup"))
        .expect("the popup is up");
    assert!(u.shown(popup));
    let pr = u.screen_rect(popup).unwrap();
    assert_eq!((pr.x0, pr.y0, pr.width(), pr.height()), (10, 29, 80, 30));
    u.take_messages();
    click(&mut u, 30, pr.y0 + 20);
    assert_eq!(u.menu_selected_index(m), Some(1));
    let said: Vec<Token> = u
        .take_messages()
        .iter()
        .filter(|x| x.from == m)
        .map(|x| x.what)
        .collect();
    assert!(said.contains(&msg::NEW_SELECTION), "{said:?}");
    assert!(said.contains(&Token::of("closed")), "{said:?}");
    assert!(!u.shown(popup), "closed");
}

/// A menu shows its chosen row's text in its selection display, chosen by the player or by the
/// program, and nothing once nothing is chosen.
///
/// Behaviour: ui.menu.a-menu-shows-its-choice-in-its-display
#[test]
fn a_menu_shows_its_choice_in_its_display() {
    let popup = element(
        "Popup",
        "field",
        [0, 0, 79, 59],
        0,
        vec![state(
            "stateless",
            0,
            vec![],
            vec![],
            vec![element(
                "MenuList",
                "listbox",
                [0, 0, 79, 59],
                0,
                vec![state(
                    "stateless",
                    0,
                    vec![("click_select", V::Bool(true))],
                    vec![],
                    vec![],
                )],
            )],
        )],
    );
    let item = element(
        "Item",
        "text",
        [0, 0, 79, 14],
        0,
        vec![state("stateless", 0, vec![], vec![], vec![])],
    );
    let display = element(
        "Display",
        "text",
        [0, 0, 59, 19],
        0,
        vec![state("stateless", 0, vec![], vec![], vec![])],
    );
    let menu = element(
        "Menu",
        "menu",
        [10, 10, 89, 29],
        0,
        vec![state(
            "stateless",
            0,
            vec![
                ("popup", id("Popup")),
                ("listbox", id("MenuList")),
                ("text_item", id("Item")),
                ("selection_display", id("Display")),
            ],
            vec![],
            vec![display],
        )],
    );
    let win = element(
        "Window",
        "field",
        [0, 0, 299, 299],
        0,
        vec![state("stateless", 0, vec![], vec![], vec![menu])],
    );
    let mut u = ui(vec![popup, item, win], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let w = u.create_root(l, Token::of("Window")).unwrap();
    let m = u.child(w, Token::of("Menu")).unwrap();
    let d = u.child(m, Token::of("Display")).unwrap();
    let one = u.menu_insert_text(m, "One", 0).unwrap();
    u.menu_insert_text(m, "Two", 1).unwrap();
    u.menu_select_item(m, Some(one));
    assert_eq!(u.text(d).as_deref(), Some("One"), "chosen by the program");
    click(&mut u, 50, 20);
    let pr = u
        .roots()
        .iter()
        .copied()
        .find(|r| u.node(*r).unwrap().id == Token::of("Popup"))
        .and_then(|p| u.screen_rect(p))
        .expect("the popup is up");
    click(&mut u, 30, pr.y0 + 20);
    assert_eq!(u.text(d).as_deref(), Some("Two"), "chosen by the player");
    u.menu_select_item(m, None);
    assert_eq!(u.text(d).as_deref(), Some(""), "nothing chosen");
}

/// A tree whose top node names the other as its child: the child hangs from it, and the tree
/// draws a line from the parent's top middle to the child's bottom middle in its line color.
///
/// Behaviour: ui.tree.children-hang-from-their-parents-by-lines
#[test]
fn children_hang_from_their_parents_by_lines() {
    let a = element(
        "A",
        "field",
        [80, 10, 119, 29],
        0,
        vec![state(
            "stateless",
            0,
            vec![("child1", id("B"))],
            vec![],
            vec![],
        )],
    );
    let b = element("B", "field", [80, 100, 119, 119], 0, vec![]);
    let tree = element(
        "Tree",
        "tree",
        [0, 0, 199, 199],
        0,
        vec![state(
            "stateless",
            0,
            vec![("line_color_red", V::Float(1.0))],
            vec![],
            vec![a, b],
        )],
    );
    let mut u = ui(vec![tree], vec![]);
    let l = u.load_layout(LAYOUT).unwrap();
    let t = u.create_root(l, Token::of("Tree")).unwrap();
    let (segments, color) = u.tree_segments(t);
    assert_eq!(segments, vec![[99, 10, 99, 119]]);
    assert_eq!(color, [1.0, 0.0, 0.0]);
    let mut frame = Surface::new(200, 200);
    draw::draw(&u, &mut frame);
    assert_eq!(frame.get(99, 50), [0, 0, 255, 255], "a red line");
    assert_eq!(frame.get(98, 50), [0, 0, 0, 0]);
}

/// A list that selects by dragging, pressed on a row, selects it and listens for its window's own
/// pointer moves, which the manager never sends: moving onto another row with the button held
/// leaves the pressed row selected. The release undoes the listening.
///
/// Behaviour: ui.list.selecting-by-drag-stays-on-the-row-pressed
#[test]
fn selecting_by_drag_stays_on_the_row_pressed() {
    let (mut u, list, rows) = five_rows(vec![
        window(
            vec![
                ("click_select", V::Bool(true)),
                ("drag_select", V::Bool(true)),
            ],
            vec![],
        ),
        row(),
    ]);
    let w = u.root_of(list);
    u.pointer_move(20, 15);
    u.pointer_press();
    assert_eq!(u.list_selected_item(list), Some(rows[0]));
    assert!(u.is_registered_for_object_message(list, w, auberean_ui::manager::global::MOUSE_MOVE));
    u.pointer_move(20, 35);
    assert_eq!(
        u.list_selected_item(list),
        Some(rows[0]),
        "the moves never come"
    );
    u.pointer_release();
    u.pointer_move(21, 35);
    assert!(!u.is_registered_for_object_message(list, w, auberean_ui::manager::global::MOUSE_MOVE));
}
