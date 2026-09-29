//! The vendor seam: `dereth_client_model`'s open shop priced into `dereth_ui_screens`' [`ShopView`].
//!
//! The same shape as [`crate::allegiance_view`], and for the same reason:
//! `dereth-ui-screens` may not depend on `dereth-client-model`, so the pricing has to happen on
//! this side of the seam, in the crate that can see both.
//!
//! **This file gives four vendor operations their production callers:** sell pricing, buy pricing,
//! acceptability, and the refusal-message table.
//!
//! Which price a row gets is the client's own split and is not cosmetic:
//!
//! * a **stock** row is what the vendor will sell you, so it is `vendor_sell_price(pwd, count)`;
//! * a **sell-basket** row is what the vendor will pay you, so it is `vendor_buy_price(pwd)` — and
//!   that function returns **0** for an item the shop refuses, because its first act is to
//!   return 0 whenever `inq_acceptability(pwd)` is non-zero.

use dereth_client_contract::view::{ShopRow, ShopView};

/// The vendor panel's three lists and two totals, for one world.
///
/// A world with no shop open produces [`ShopView::default`] — `open: false` — which the panel
/// draws as a hidden window rather than as an empty one.
#[must_use]
pub fn shop(w: &dereth_client_model::World) -> ShopView {
    let s = &w.shop;
    if !s.is_open() {
        return ShopView::default();
    }
    let stock = s
        .stock
        .iter()
        .map(|p| ShopRow {
            item: p.iid,
            name: p.pwd.name.clone(),
            icon: (p.pwd.icon_id != 0).then_some(dereth_primitives::DataId(p.pwd.icon_id)),
            amount: p.amount,
            obj_type: inq_type(w, p.iid, p.pwd.obj_type),
            max_stack_size: max_stack_size(w, p.iid, &p.pwd),
            contained_items: num_contained_items(w, p.iid),
            contained_containers: num_contained_containers(w, p.iid),
            // The vendor item list prices a stock row at one unit; the count changes only when the
            // player moves the stack slider, and that is the single-item buy's reading, not the
            // list's. The client divides the stack value out before pricing the displayed row.
            price: s.profile.vendor_sell_price(&p.pwd, 1),
            refusal: None,
        })
        .collect();
    let buy_list = s
        .buy_list
        .iter()
        .filter_map(|(id, n)| {
            let p = s.stock_item(*id)?;
            Some(ShopRow {
                item: *id,
                name: p.pwd.name.clone(),
                icon: (p.pwd.icon_id != 0).then_some(dereth_primitives::DataId(p.pwd.icon_id)),
                amount: *n,
                obj_type: inq_type(w, *id, p.pwd.obj_type),
                max_stack_size: max_stack_size(w, *id, &p.pwd),
                contained_items: num_contained_items(w, *id),
                contained_containers: num_contained_containers(w, *id),
                price: s.profile.vendor_sell_price(&p.pwd, *n),
                refusal: None,
            })
        })
        .collect();
    let sell_list = s
        .sell_list
        .iter()
        .filter_map(|(id, _)| {
            let o = w.weenie(*id)?;
            Some(ShopRow {
                item: *id,
                name: o.pwd.name.clone(),
                icon: (o.pwd.icon_id != 0).then_some(dereth_primitives::DataId(o.pwd.icon_id)),
                // The single-item sell writes the literal 1 into every sell row.
                amount: 1,
                // A sell-basket row **is** an object the client holds — the `filter_map` above is
                // the client's object lookup by `iid` — so there is no profile to fall back to and this is
                // the object's own type query, unconditionally.
                obj_type: o.pwd.obj_type,
                // Same reason: the object is in hand, so these are its own fields and its own
                // inventory state. Only item-list updating reads the last two; they are filled
                // on all three lists so that a row is a row.
                max_stack_size: u32::from(o.pwd.max_stack_size.unwrap_or(0)),
                contained_items: num_contained_items(w, *id),
                contained_containers: num_contained_containers(w, *id),
                price: s.profile.vendor_buy_price(&o.pwd),
                // The row a player can see a refusal on. `refusal_message` is the four-string
                // table; the containment test runs first and may supply its own refusal.
                refusal: w.drag_item_acceptable(*id),
            })
        })
        .collect();
    ShopView {
        open: true,
        vendor: s.vendor_id,
        sell_mode: s.mode == dereth_client_model::vendor::ShopMode::Sell,
        stock,
        buy_list,
        sell_list,
        // **Three numbers, four writers.** `buy_transaction` and `sell_transaction`
        // are the two sub-UIs' transaction totals and are different numbers computed by
        // different functions from different lists; `total_value` is the *parent's* purse, which
        // both total-value displays render and neither computes. See [`ShopView`]'s own table.
        buy_transaction: w.transaction_value(),
        sell_transaction: w.sell_value(),
        // Both `filter_map`s require a successful object lookup and put the accumulator
        // inside: a basket row naming an object the client does not hold contributes nothing at
        // all. Same predicate, same order, as the two `ShopRow` lists above.
        buy_items: basket_items(
            s.buy_list
                .iter()
                .filter_map(|(id, n)| Some((s.stock_item(*id)?.pwd.stack_size, *n))),
        ),
        sell_items: basket_items(
            s.sell_list
                .iter()
                .filter_map(|(id, _)| Some((w.weenie(*id)?.pwd.stack_size, 1))),
        ),
        total_value: s.total_value,
        type_filters: type_filters(w),
    }
}

/// The `%d` of `L"Buying %d %s worth %hsp"` / `L"Selling %d %s worth %hsp"`.
///
/// Retail's two transaction loops carry a second accumulator beside the money:
/// each row adds its object's stack size, with a stack size of 0 counted as 1. It is a count of
/// **things**, not rows or money, and it is what picks `L"item"` versus `L"items"`.
///
/// `rows` yields `(pwd.stack_size, how many basket entries this row stands for)`, already filtered
/// by the caller to rows whose object the client actually holds — the live-object lookup
/// encloses the accumulator as well as the price.
/// A stack size of 0 becomes 1, and an
/// **absent** stack size (the wire's optional field) is the same 1.
///
/// The second term is the one deviation and it is bounded: retail's buy basket is a
/// item-list widget with one element per "Add to List" press, while
/// [`dereth_client_model::vendor::Shop::buy_list`] merges them into one row carrying the count — so `n`
/// presses of an object with stack size `s` are `n` rows of `s` there and one row of `n·s` here,
/// and the sum is the same. The sell basket's `n` is the literal 1 the single-item sell writes.
fn basket_items(rows: impl Iterator<Item = (Option<u16>, i32)>) -> i32 {
    rows.map(|(stack, n)| i32::from(stack.unwrap_or(1).max(1)) * n.max(1))
        .sum()
}

/// Construct the vendor-item tabs: walk the eighteen
/// [`TYPE_FILTERS`](dereth_client_model::vendor::TYPE_FILTERS) in order and keep the ones
/// stock-type predicate answers true for.
///
/// **It is built from the stock, and the count is not fixed.** Exactly eighteen type
/// tests are made, each immediately gating its matching filter row, so a shop that sells only food
/// gets one row and a shop with no stock gets none.
///
/// **Retail *manufactures* the object it then asks.**
///
/// The type-filter pass asks the live object for its type and skips a stock row whose object the
/// client has not created. Item-list updating uses the same lookup and skip. The apparent difference
/// from reading the `PublicWeenieDesc` carried by `0x0062` is resolved by [`inq_type`] below.
///
/// **This is why the walk lives here rather than on `dereth_client_model::vendor::Shop`.**
/// `Shop::list_contains_type` asks the same question of a `Shop` alone, but a `Shop` cannot see the
/// live object or player ownership — the two things the fork needs. It therefore
/// has **no production caller**; it is kept as the `Shop`-local form the vendor
/// tests use to select the recorded grocer by its contents, and its doc says so.
fn type_filters(w: &dereth_client_model::World) -> Vec<(&'static str, u32)> {
    // Run the stock-type predicate once per mask, with the type resolved the way
    // retail resolves it. The `any` is the client's own early `return true`.
    let types: Vec<u32> = w
        .shop
        .stock
        .iter()
        .map(|p| inq_type(w, p.iid, p.pwd.obj_type))
        .collect();
    dereth_client_model::vendor::TYPE_FILTERS
        .iter()
        .filter(|(_, m)| types.iter().any(|t| t & *m != 0))
        .copied()
        .collect()
}

/// Resolve the type on the live object named by a stock row.
///
/// The live-object type comes from its public description. That description normally starts as a
/// copy of the vendor profile: a missing object is created from the profile, and an existing
/// unowned object is overwritten from it. Only an existing object owned by the player keeps its
/// own description.
///
/// So on every row but one, retail's "live weenie" carries the **same bytes** this build reads
/// straight off the profile — the two readings are equal by construction, not by coincidence. The
/// exception is an object the client already held **and the player owns**, which keeps
/// its own `_type`. That is the one row on which a live type query and the profile field can
/// disagree. This function is that fork, transcribed, so this build has no deviation left to
/// declare — it reads the object when the client would use its untouched public description,
/// and the profile whenever retail would have overwritten it with exactly that profile.
///
/// **The corpus reaches the existing-object arm and stops short of the owned half**, measured
/// rather than assumed. Across every recorded `0x0062`, **one** stock row names an object the
/// corpus also creates — and it is an object the player **sold to that vendor**, which is how a
/// shop comes to advertise something the client already holds. The existing-object and profile-copy
/// path is therefore live; whether `is_owned_by_player` was true for it
/// at that instant needs container state a whole-session replay would carry, so the owned arm is a
/// **constructed** test. The vendor filter tests hold both, with the count derived from the
/// corpus rather than pinned, and the create scan calibrated on the create set itself.
fn inq_type(
    w: &dereth_client_model::World,
    iid: dereth_primitives::ObjectId,
    profile_type: u32,
) -> u32 {
    match w.weenie(iid) {
        Some(o) if w.is_owned_by_player(iid) => o.pwd.obj_type,
        _ => profile_type,
    }
}

/// Read maximum stack size from the object a stock row names, using
/// [`inq_type`]'s fork applied to the neighbouring field.
///
/// Item-list updating reads the type and the maximum stack size out of the **same** live object.
/// Two fields of one description cannot be resolved by different rules, so
/// this is deliberately the same three-arm fork and not a plain profile read: an object the client
/// holds and the player owns keeps its own `pwd`, and that is as true of its maximum stack size as
/// it is of its type.
fn max_stack_size(
    w: &dereth_client_model::World,
    iid: dereth_primitives::ObjectId,
    profile: &dereth_protocol::types::PublicWeenieDesc,
) -> u32 {
    let pwd = match w.weenie(iid) {
        Some(o) if w.is_owned_by_player(iid) => &o.pwd,
        _ => profile,
    };
    u32::from(pwd.max_stack_size.unwrap_or(0))
}

/// Count the live object's contained items, or return zero when it has no inventory.
///
/// **There is no profile arm and there cannot be one.** Inventory state is not part of a
/// `PublicWeenieDesc`; it is built from the inventory stream, so a stock row's containment is
/// knowable only from the world. An object the client does not hold answers 0 here for the same
/// reason a null object inventory does -- which is why the gate cannot fire for a stock row
/// the create stream never sent, and why no recorded vendor can witness it.
fn num_contained_items(w: &dereth_client_model::World, iid: dereth_primitives::ObjectId) -> u32 {
    w.inventory(iid)
        .map_or(0, |i| u32::try_from(i.items.len()).unwrap_or(u32::MAX))
}

/// Count contained containers with the same null-inventory rule.
fn num_contained_containers(
    w: &dereth_client_model::World,
    iid: dereth_primitives::ObjectId,
) -> u32 {
    w.inventory(iid)
        .map_or(0, |i| u32::try_from(i.containers.len()).unwrap_or(u32::MAX))
}
