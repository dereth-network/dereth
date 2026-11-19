//! The windows of the player's things: the inventory, the loot window, the trade, the shops and
//! the market, and the player's things as Horizon follows them; and what their tests share, a pack
//! as the view gives it and a thing pressed, carried and put down over frames.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

mod goods;
mod inventory;
mod loot;
mod market;
mod shop;
mod trade;

use std::collections::BTreeMap;

use auberean_client_sdk::contract::view::{Container, Contents, Inventory, Item};
use auberean_client_sdk::Action;
use auberean_horizon::draw::Rect;
use auberean_horizon::state::GameState;

use crate::{in_world, Harness, PLAYER};

/// An item: one of it, not a stack, the player's, with a name and an icon.
pub fn item(name: &str) -> Item {
    Item {
        quantity: 1,
        icon: 0x0600_1234,
        stack_size: 1,
        name: name.to_owned(),
        had: true,
        ..Item::default()
    }
}

/// A stack of `n` that holds at most `most`.
pub fn stack(name: &str, n: u32, most: u32) -> Item {
    Item {
        quantity: n,
        stackable: true,
        stack_size: most,
        ..item(name)
    }
}

/// The player's pack as the view gives it: the items of each segment in order (the pack itself,
/// then the three bags), the items' records, what is worn, and the money.
pub fn pack(
    segments: [&[u64]; 4],
    items: &[(u64, Item)],
    worn: &[(u32, u64)],
    money: i32,
) -> Inventory {
    let listed: Vec<u64> = segments.iter().flat_map(|s| s.iter().copied()).collect();
    let mut first = 0;
    let mut windows = Vec::new();
    for s in segments {
        let n = u32::try_from(s.len()).expect("few");
        windows.push((first, n));
        first += n;
    }
    let mut containers = BTreeMap::new();
    containers.insert(
        PLAYER,
        Container {
            size: 78,
            segments: windows
                .iter()
                .map(|(first, count)| Contents {
                    items: listed.clone(),
                    first: *first,
                    count: *count,
                })
                .collect(),
            slots: windows.clone(),
        },
    );
    Inventory {
        version: 1,
        player: PLAYER,
        containers,
        segments: windows,
        worn: worn.to_vec(),
        items: items.iter().cloned().collect(),
        money,
        coin_stack: 500,
        ..Inventory::default()
    }
}

/// A player in the world holding `inventory`.
pub fn holding(inventory: Inventory) -> GameState {
    let mut s = in_world();
    s.goods.inventory = Some(inventory);
    s
}

/// The middle of `r`.
pub fn mid(r: Rect) -> (f32, f32) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

/// Press at `from`, pull to `to`, let go there, a frame each; the actions every frame asked.
pub fn carry(h: &mut Harness, s: &GameState, from: (f32, f32), to: (f32, f32)) -> Vec<Action> {
    let mut acts = Vec::new();
    h.input.mouse = from;
    h.input.down[0] = true;
    h.input.pressed[0] = true;
    acts.extend(h.frame(s).acts);
    h.input.mouse = to;
    acts.extend(h.frame(s).acts);
    h.input.down[0] = false;
    h.input.released[0] = true;
    acts.extend(h.frame(s).acts);
    acts
}

/// The actions that are not the selection's (a press selects what it presses).
pub fn without_selecting(acts: Vec<Action>) -> Vec<Action> {
    acts.into_iter()
        .filter(|a| !matches!(a, Action::SetSelection { .. }))
        .collect()
}

/// Frames until the amount box is up, then Enter in it; the actions asked.
pub fn enter_amount(h: &mut Harness, s: &GameState) -> Vec<Action> {
    assert!(h.ui.goods.amount.is_some(), "the box asks how many");
    h.key(s, auberean_horizon::ui::vk::ENTER).acts
}
