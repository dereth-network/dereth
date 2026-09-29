// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionBuyItems.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionBuyItems.cs`.

use empyrean_net::SessionId;

use crate::entity::item_profile::ItemProfile;
use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_commerce;
use crate::World;

// ACE: GameActionBuyItems.Handle
/// Not ACE's (retail, V351): the message is read as the client writes it —
/// each item's first dword is a 24-bit signed amount whose top byte flags an attached public
/// description, and the list is followed by the alternate-currency dword, which is read and not
/// used (as in ACE, the vendor's own currency applies). ACE read the amount as a plain 32-bit
/// value and stopped before the currency; for what a real client sends (a positive amount, no
/// description) the items are the same.
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<dereth_protocol::trade::VendorBuy>()?;
    let vendor_guid = m.vendor_id.0;
    let items: Vec<ItemProfile> = m
        .items
        .iter()
        .map(|i| ItemProfile::new(i.amount, i.iid.0))
        .collect();
    // currency id is set to 0 by default
    // if non-zero, use as alternate currency wcid
    let _alt_currency_wcid = m.alternate_currency_id;

    let player = session_player(w, session);
    player_commerce::handle_action_buy_item(w, player, vendor_guid, items);
    Ok(())
}
