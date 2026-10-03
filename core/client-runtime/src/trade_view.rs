//! The secure-trade snapshot shared by the interfaces.
//!
//! Names and icons are joined to the trade mirror here; the contract resolves the agreement
//! controls using the rows each interface displays.
//!
//! A row's name and icon come from the object's own `PublicWeenieDesc`, which is why an item the
//! tables do not hold yet draws as an empty name rather than as nothing: the partner's items
//! arrive as ordinary `0xF745` creates and the `0x0200` that names them can beat the create.

use dereth_client_contract::view::{TradeRow, TradeView};

/// `TradePanel`'s two lists and its two acceptance lights, for one world.
///
/// A world with no negotiation open produces [`TradeView::default`] — `open: false` — which the
/// panel draws as a hidden window rather than as an empty one.
#[must_use]
pub fn trade(w: &dereth_client_model::World) -> TradeView {
    let t = &w.trade;
    if !t.open {
        return TradeView::default();
    }
    let row = |id: dereth_primitives::ObjectId| TradeRow {
        item: id,
        name: w.weenie(id).map(|o| o.pwd.name.clone()).unwrap_or_default(),
        icon: w
            .weenie(id)
            .and_then(|o| (o.pwd.icon_id != 0).then_some(dereth_primitives::DataId(o.pwd.icon_id))),
    };
    TradeView {
        open: true,
        partner: (t.partner.0 != 0).then_some(t.partner),
        // Resolve the partner's object name, or use an empty string when the id is 0. Assigning
        // the text in both cases is what clears the field when a negotiation closes.
        partner_name: w
            .weenie(t.partner)
            .map(|o| o.pwd.name.clone())
            .unwrap_or_default(),
        // A 0x0208 resets only the window. Keep the stale mirror for the next acceptance's
        // out-of-sync comparison; later row notices rebuild each displayed side independently.
        self_rows: t.display_lists.as_ref().map_or_else(
            || t.trade.self_list.iter().map(|c| row(c.iid)).collect(),
            |lists| lists[0].iter().copied().map(row).collect(),
        ),
        partner_rows: t.display_lists.as_ref().map_or_else(
            || t.trade.partner_list.iter().map(|c| row(c.iid)).collect(),
            |lists| lists[1].iter().copied().map(row).collect(),
        ),
        accepted: t.trade.accepted,
        partner_accepted: t.trade.partner_accepted,
        // **Carry removal failures to the secure-trade panel.**
        //
        // Straight through, because the whole point of the field is that the panel cannot see it
        // any other way: a refused add was never in the self list, so no join over the
        // object table can reconstruct it. The two writers are
        // the remove-item-from-trade handler's side-1 arm and
        // the trade-failure handler; both preserve the rejected item here for display.
        self_removed: t.self_removed.clone(),
        // Straight through, for the same reason `self_removed` is: the window's
        // two lights are not a function of the mirror's two flags, and nothing on this side of the
        // seam can reconstruct the difference.
        acceptance_darkened: t.acceptance_darkened,
    }
}
