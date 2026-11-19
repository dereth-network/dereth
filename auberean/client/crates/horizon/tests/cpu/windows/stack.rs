//! The windows' stacking: a window opened comes to the front over the one it was opened from (the
//! windows of things among themselves too), a press brings a window to the front, a window opens where it keeps clear of the HUD, and where
//! a window lies over another it is backed enough to read.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_client_sdk::contract::StoreAction;
use auberean_client_sdk::Action;
use auberean_horizon::draw::Rect;
use auberean_horizon::keys::Keyed;
use auberean_horizon::state::GameState;
use auberean_horizon::ui::goods::Window as Things;
use auberean_horizon::ui::stack::{Window, OVER_WINDOW};

use crate::play::fit::{full_hud, meet, on};
use crate::things::{holding, item, mid, pack};
use crate::Harness;

const SWORD: u64 = 0x7000_0000_0000_0001;

/// Where in this frame's drawing window `w` was drawn.
fn drawn(h: &Harness, w: Window) -> std::ops::Range<usize> {
    h.ui.stack
        .drawn
        .iter()
        .find(|(o, _)| *o == w)
        .map(|(_, r)| r.clone())
        .unwrap_or_else(|| panic!("{w:?} drawn"))
}

/// Whether window `a` was drawn wholly after window `b` this frame.
fn after(h: &Harness, a: Window, b: Window) -> bool {
    let (a, b) = (drawn(h, a), drawn(h, b));
    !a.is_empty() && !b.is_empty() && a.start >= b.end
}

/// How opaque the flat colors drawn among `quads` make the point `(x, y)`, one over another.
fn opacity(h: &Harness, quads: std::ops::Range<usize>, (x, y): (f32, f32)) -> f32 {
    let clear: f32 = h.list.quads[quads]
        .iter()
        .filter(|q| q.tex.is_none() && q.dst.contains(x, y))
        .map(|q| 1.0 - f32::from(u8::try_from(q.colour >> 24).unwrap_or(0)) / 255.0)
        .product();
    1.0 - clear
}

/// A player holding a sword in the pack.
fn with_sword() -> GameState {
    holding(pack(
        [&[SWORD], &[], &[], &[]],
        &[(SWORD, item("Sword"))],
        &[],
        0,
    ))
}

#[test]
fn a_window_opened_from_another_comes_to_the_front_and_a_press_brings_one_back() {
    let mut h = Harness::new();
    let s = with_sword();
    h.ui.keyed(Keyed::Examine, &s);
    h.frame(&s);
    h.ui.keyed(Keyed::Inventory, &s);
    h.frame(&s);
    assert!(
        after(&h, Window::Goods, Window::Examine),
        "the inventory opened last, in front"
    );
    // A right press on the sword examines it: the examine window comes to the front.
    let cell =
        h.ui.goods
            .inventory
            .layout(h.k())
            .expect("open")
            .cell(0, h.k());
    h.input.mouse = mid(cell);
    h.input.pressed[1] = true;
    h.frame(&s);
    h.frame(&s);
    assert!(
        after(&h, Window::Examine, Window::Goods),
        "{:?}",
        h.ui.stack.order()
    );
    // A press on the inventory brings it back to the front.
    let inventory = h.ui.goods.rects()[0];
    let examine = h.ui.examine.rect().expect("open");
    let spot = [
        (inventory.x + 4.0, inventory.y + 4.0),
        (inventory.right() - 4.0, inventory.bottom() - 4.0),
    ]
    .into_iter()
    .find(|(x, y)| !examine.contains(*x, *y))
    .expect("a corner of the inventory the examine window leaves");
    h.click(&s, spot);
    assert!(after(&h, Window::Goods, Window::Examine));
}

/// A shop the player stands in.
fn in_a_shop(mut s: GameState) -> GameState {
    use auberean_client_sdk::contract::notice::Store;
    use auberean_horizon::state::goods::ShopShown;
    s.goods.shop = Some(ShopShown {
        store: Store {
            id: 4,
            position: 0,
            size: 12,
            ..Store::default()
        },
        name: "Smithy".to_owned(),
        description: String::new(),
        sales: Vec::new(),
    });
    s
}

/// The windows of things drawn this frame, the front one last.
fn things(h: &Harness) -> Vec<Things> {
    h.ui.goods.drawn().iter().map(|(w, _)| *w).collect()
}

/// A window of things opened while another is open (the inventory with the shop up) comes in
/// front of it; a press on the one behind brings that back; Escape closes the one in front.
#[test]
fn a_window_of_things_opened_after_another_comes_in_front_of_it() {
    let mut h = Harness::new();
    let s = in_a_shop(with_sword());
    h.frame(&s);
    h.frame(&s);
    assert_eq!(things(&h), vec![Things::Shop]);
    h.ui.keyed(Keyed::Inventory, &s);
    h.frame(&s);
    h.frame(&s);
    assert_eq!(
        things(&h),
        vec![Things::Shop, Things::Inventory],
        "the inventory opened last, in front"
    );
    let inventory = h.ui.goods.drawn()[1].1;
    let shop = h.ui.goods.drawn()[0].1;
    let spot = [
        (shop.x + 4.0, shop.y + 4.0),
        (shop.right() - 4.0, shop.y + 4.0),
        (shop.x + 4.0, shop.bottom() - 4.0),
        (shop.right() - 4.0, shop.bottom() - 4.0),
    ]
    .into_iter()
    .find(|(x, y)| !inventory.contains(*x, *y))
    .expect("a corner of the shop the inventory leaves");
    h.click(&s, spot);
    h.frame(&s);
    assert_eq!(things(&h), vec![Things::Inventory, Things::Shop], "pressed");
    let mut acts = Vec::new();
    assert!(h.ui.goods.close_top(&mut acts));
    assert_eq!(
        acts,
        vec![Action::Store(StoreAction::LeaveStore)],
        "the shop in front, left"
    );
    assert!(h.ui.goods.inventory.is_open());
}

#[test]
fn escape_closes_the_window_in_front_whichever_was_opened_first() {
    let mut h = Harness::new();
    let s = with_sword();
    h.ui.keyed(Keyed::Inventory, &s);
    h.frame(&s);
    h.ui.keyed(Keyed::Skills, &s);
    h.frame(&s);
    assert!(
        after(&h, Window::Skills, Window::Goods),
        "the skill window opened last, in front"
    );
    let mut acts = Vec::new();
    assert!(h.ui.close_top(&mut acts));
    assert!(!h.ui.skills.is_open() && h.ui.goods.inventory.is_open());
}

#[test]
fn each_window_opens_clear_of_the_huds_elements() {
    let keys = [
        Keyed::Skills,
        Keyed::Inventory,
        Keyed::Examine,
        Keyed::Map,
        Keyed::Journal,
        Keyed::Social,
        Keyed::Crafting,
    ];
    for screen in [(1920.0, 1080.0), (1280.0, 960.0)] {
        for key in keys {
            let mut h = on(screen, None);
            let s = full_hud();
            h.frame(&s);
            h.ui.keyed(key, &s);
            h.frame(&s);
            h.frame(&s);
            let rects: Vec<Rect> =
                h.ui.stack
                    .drawn
                    .iter()
                    .filter(|(_, r)| !r.is_empty())
                    .filter_map(|(w, _)| match w {
                        Window::Skills => h.ui.skills.rect(),
                        Window::Examine => h.ui.examine.rect(),
                        Window::Social => h.ui.social.rect(),
                        Window::Map => h.ui.map.rect(),
                        Window::Journal => h.ui.journal.rect(),
                        Window::Book => h.ui.book.rect(),
                        Window::Goods => h.ui.goods.rects().first().copied(),
                        Window::Crafting => h.ui.crafting.rect(),
                        Window::Making => h.ui.making.rect(),
                    })
                    .collect();
            assert_eq!(rects.len(), 1, "{key:?} opened on {screen:?}");
            for (name, r) in &h.ui.layout.outlines {
                assert!(
                    !meet(&rects[0], r),
                    "{key:?}'s window {:?} over {name} {r:?} on {screen:?}",
                    rects[0]
                );
            }
        }
    }
}

#[test]
fn a_window_over_another_is_backed_to_be_read_and_over_the_world_it_is_not() {
    let mut h = Harness::with_built_in_art();
    let s = with_sword();
    h.ui.keyed(Keyed::Skills, &s);
    h.frame(&s);
    h.ui.keyed(Keyed::Examine, &s);
    h.frame(&s);
    let skills = h.ui.skills.rect().expect("open");
    let examine = h.ui.examine.rect().expect("open");
    // The examine window dragged by its title over the skill window's right side.
    let grip = (examine.x + examine.w / 2.0, examine.y + 10.0);
    let dx = skills.right() - 120.0 - examine.x;
    let dy = skills.y + 40.0 - examine.y;
    h.drag(&s, grip, (grip.0 + dx, grip.1 + dy));
    h.frame(&s);
    let examine = h.ui.examine.rect().expect("open");
    let over = examine.intersect(&skills).expect("over the skill window");
    assert!(after(&h, Window::Examine, Window::Skills));
    let quads = drawn(&h, Window::Examine);
    let a = opacity(&h, quads.clone(), mid(over));
    assert!(a >= OVER_WINDOW, "{a} over the skill window");
    let world = (examine.right() - 20.0, examine.bottom() - 20.0);
    assert!(!skills.contains(world.0, world.1));
    let a = opacity(&h, quads, world);
    assert!(a < OVER_WINDOW, "{a} over the world");
}
