//! What the trade, shop and pet windows and the story ask of the player's scripts, and what the
//! shop windows read back.
//!
//! Each request is one of the interface's functions, called on the player's instance with its
//! arguments pushed last first, as the client's interface glue calls it: the trade's open, close,
//! accept, decline, offer and take-back; a shop's purchase and its confirmation, leaving and
//! entering, its pages; a consignment's sale, restock and collection; the pets' commands, modes
//! and names; the story's scenes done; the sale reminders shown.
//!
//! **The shop windows' answers.** When the scripts show a shop or change its sales, the client
//! asks again, for each sale, how many of its item the player may buy (an out word), and once what
//! share of a consignment's price putting it up costs (an out float); and again after the
//! player's money or pack changes while a shop shows.

use std::collections::BTreeMap;

use auberean_client_contract::{PetAction, StoreAction, TradeAction};
use auberean_script::native_object::{Field, NativeObject};

use crate::host::{InterfaceArg, PanelNotice, Scripts};
use crate::skills::Asker;
use crate::CallResult;

/// The interface's functions these windows call, by id.
pub mod function {
    pub const OPEN_TRADE: u32 = 0x00B4_00B8;
    pub const CLOSE_TRADE: u32 = 0x00B4_00B9;
    pub const ACCEPT_TRADE: u32 = 0x00B4_00BA;
    pub const DECLINE_TRADE: u32 = 0x00B4_00BB;
    pub const OFFER_ITEM_FOR_TRADE: u32 = 0x00B4_00BC;
    pub const REVOKE_ITEM_FROM_TRADE: u32 = 0x00B4_00BD;
    pub const PURCHASE_ITEM: u32 = 0x00B4_00D0;
    pub const PURCHASE_CONFIRMED: u32 = 0x00B4_00D1;
    pub const GET_MAX_PURCHASE_QUANTITY: u32 = 0x00B4_00D3;
    pub const LEAVE_STORE: u32 = 0x00B4_00D4;
    pub const LEAVE_CATALOG: u32 = 0x00B4_00D5;
    pub const LEAVE_CONSIGNMENT: u32 = 0x00B4_00D6;
    pub const ENTER_CONSIGNMENT: u32 = 0x00B4_00D7;
    pub const ENTER_STORE: u32 = 0x00B4_00D8;
    pub const NEXT_SALES: u32 = 0x00B4_00D9;
    pub const PREV_SALES: u32 = 0x00B4_00DA;
    pub const SALES: u32 = 0x00B4_00DB;
    pub const RESTOCK: u32 = 0x00B4_00DC;
    pub const SELL: u32 = 0x00B4_00DD;
    pub const GET_CONSIGNMENT_CHARGE: u32 = 0x00B4_00DE;
    pub const DISPLAY_SALE_REMINDERS: u32 = 0x00B4_00DF;
    pub const COLLECT: u32 = 0x00B4_00E0;
    pub const PET_ATTACK: u32 = 0x00B4_00E1;
    pub const PET_BUFF: u32 = 0x00B4_00E2;
    pub const PET_HEAL: u32 = 0x00B4_00E3;
    pub const PET_RETURN: u32 = 0x00B4_00E4;
    pub const PET_MODE: u32 = 0x00B4_00E5;
    pub const SET_PET_NAME: u32 = 0x00B4_00E6;
    pub const PLAY_SCENES_DONE: u32 = 0x00B4_0090;
}

/// The scripts' notices to the trade window that change what is offered: an item offered (who,
/// high and low; the item, high and low; how many), taken back (who; the item), every offer
/// emptied, the trade closed.
pub mod trade {
    pub const CLOSED: u32 = 0x0090 << 16 | 0x0003;
    pub const RESET: u32 = 0x0090 << 16 | 0x0006;
    pub const OFFERED: u32 = 0x0090 << 16 | 0x0008;
    pub const REVOKED: u32 = 0x0090 << 16 | 0x0009;
}

/// The scripts' notices that show a shop or the player's consignments, or change their sales;
/// those after which how many the player may buy changes.
pub mod notice {
    pub const VIEW_STORE: u32 = 0x1AD3 << 16 | 0x0003;
    pub const UPDATE_STORE: u32 = 0x1AD3 << 16 | 0x0004;
    pub const UPDATE_SALE: u32 = 0x1AD3 << 16 | 0x0005;
    pub const LEAVE_STORE: u32 = 0x1AD3 << 16 | 0x0006;
    pub const VIEW_CONSIGNMENT: u32 = 0x1AD3 << 16 | 0x0008;
    pub const UPDATE_MONEY: u32 = 0x008C << 16 | 0x0002;
    pub const UPDATE_CONTAINER: u32 = 0x008B << 16 | 0x0002;
}

/// The native type of a shop's adaptor and of a list of scenes.
pub const SHOP: u32 = 0x1AD6;
pub const SCENE_LIST: u32 = auberean_script::native_object::native_type::SCENE_INFO_LIST;

/// What the shop windows read: how many of each sale's item the player may buy, and the share of
/// a consignment's price putting it up costs; [`Store::version`] counts the askings.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Store {
    pub version: u64,
    /// The sales' items of the shop shown.
    pub items: Vec<u64>,
    pub max_purchase: BTreeMap<u64, u32>,
    pub consignment_charge: f32,
    /// Calls that did not run to their end.
    pub incomplete: Vec<(u32, CallResult)>,
}

/// The items a shop's adaptor sells, in order.
#[must_use]
pub fn sale_items(shop: &NativeObject) -> Vec<u64> {
    let NativeObject::Adaptor { fields, .. } = shop else {
        return Vec::new();
    };
    let Some(Field::Many(sales)) = fields.get("Sale") else {
        return Vec::new();
    };
    sales
        .iter()
        .filter_map(|s| match s {
            Field::Value(v) => match v.as_ref() {
                NativeObject::Adaptor { fields, .. } => match fields.get("ItemIID") {
                    Some(Field::Long(l)) => Some(*l),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        })
        .collect()
}

impl Store {
    /// What the notices of a frame say: a shop shown or changed gives the items to ask about; a
    /// shop left forgets them. Whether to ask again.
    #[must_use]
    pub fn noticed(&mut self, notices: &[PanelNotice]) -> bool {
        let mut ask = false;
        for n in notices {
            match n.native {
                notice::VIEW_STORE | notice::UPDATE_STORE => {
                    if let Some(shop) = n
                        .objects
                        .iter()
                        .flatten()
                        .find(|o| matches!(o, NativeObject::Adaptor { ty, .. } if *ty == SHOP))
                    {
                        self.items = sale_items(shop);
                    }
                    ask = true;
                }
                notice::LEAVE_STORE => self.items.clear(),
                notice::VIEW_CONSIGNMENT | notice::UPDATE_SALE => ask = true,
                notice::UPDATE_MONEY | notice::UPDATE_CONTAINER => ask |= !self.items.is_empty(),
                _ => {}
            }
        }
        ask
    }

    /// Asked again: how many of each item the player may buy, the consignment charge.
    // D2-00453, D2-01127, D2-00462, D2-01113
    pub fn ask(&mut self, scripts: &mut Scripts) {
        self.version += 1;
        let mut a = Asker {
            s: scripts,
            incomplete: Vec::new(),
        };
        self.max_purchase.clear();
        for item in self.items.clone() {
            use auberean_script::value::tag;
            #[allow(clippy::cast_possible_truncation)]
            let (lo, hi) = (item as u32, (item >> 32) as u32);
            let args = [(lo, tag::LONG | tag::MIXED), (hi, tag::LONG | tag::MIXED)];
            #[allow(clippy::cast_possible_truncation)]
            let n = a
                .ask_tagged(None, function::GET_MAX_PURCHASE_QUANTITY, &[], &args, false)
                .map_or(0, |r| r.value as u32);
            self.max_purchase.insert(item, n);
        }
        #[allow(clippy::cast_possible_truncation)]
        let charge = a
            .ask_with(None, function::GET_CONSIGNMENT_CHARGE, &[], &[], false)
            .map_or(0.0, |r| f32::from_bits(r.value as u32));
        self.consignment_charge = charge;
        self.incomplete = a.incomplete;
    }
}

/// A trade request as the interface calls it: the function and its arguments, last first.
// D2-00613, D2-01196, D2-00614, D2-01081, D2-00615, D2-01069, D2-00616, D2-01082, D2-00617
// D2-01194, D2-00618, D2-01239
#[must_use]
pub fn trade_call(a: TradeAction) -> (u32, Vec<InterfaceArg>) {
    use function as f;
    use InterfaceArg::{Long, Word};
    match a {
        TradeAction::Open(partner) => (f::OPEN_TRADE, vec![Long(partner)]),
        TradeAction::Close => (f::CLOSE_TRADE, Vec::new()),
        TradeAction::Accept => (f::ACCEPT_TRADE, Vec::new()),
        TradeAction::Decline => (f::DECLINE_TRADE, Vec::new()),
        TradeAction::Offer { item, quantity } => {
            (f::OFFER_ITEM_FOR_TRADE, vec![Word(quantity), Long(item)])
        }
        TradeAction::Revoke(item) => (f::REVOKE_ITEM_FROM_TRADE, vec![Long(item)]),
    }
}

/// A shop request as the interface calls it.
// D2-00447, D2-01199, D2-00451, D2-01198, D2-00454, D2-01210, D2-00455, D2-01208, D2-00456
// D2-01209, D2-00457, D2-01205, D2-00458, D2-01206, D2-00448, D2-01211, D2-00449, D2-01217
// D2-00450, D2-01222, D2-00460, D2-01220, D2-00459, D2-01223, D2-00461, D2-01204
#[must_use]
pub fn store_call(a: StoreAction) -> (u32, Vec<InterfaceArg>) {
    use function as f;
    use InterfaceArg::{Long, Word};
    match a {
        StoreAction::Purchase {
            item,
            quantity,
            slot,
        } => (
            f::PURCHASE_ITEM,
            vec![Word(slot), Word(quantity), Long(item)],
        ),
        StoreAction::Confirm(yes) => (f::PURCHASE_CONFIRMED, vec![Word(u32::from(yes))]),
        StoreAction::LeaveStore => (f::LEAVE_STORE, Vec::new()),
        StoreAction::LeaveCatalog => (f::LEAVE_CATALOG, Vec::new()),
        StoreAction::LeaveConsignment => (f::LEAVE_CONSIGNMENT, Vec::new()),
        StoreAction::EnterConsignment => (f::ENTER_CONSIGNMENT, Vec::new()),
        StoreAction::EnterShop(id) => (f::ENTER_STORE, vec![Word(id)]),
        StoreAction::NextSales => (f::NEXT_SALES, Vec::new()),
        StoreAction::PrevSales => (f::PREV_SALES, Vec::new()),
        StoreAction::Sales(n) => (f::SALES, vec![Word(n)]),
        StoreAction::Restock {
            item,
            quantity,
            sale,
        } => (f::RESTOCK, vec![Word(sale), Word(quantity), Long(item)]),
        StoreAction::Sell {
            item,
            quantity,
            price,
        } => (f::SELL, vec![Word(price), Word(quantity), Long(item)]),
        StoreAction::Collect { sale, sold } => {
            (f::COLLECT, vec![Word(u32::from(sold)), Word(sale)])
        }
    }
}

/// A pet request as the interface calls it.
// D2-00464, D2-01212, D2-00465, D2-01213, D2-00466, D2-01214, D2-00467, D2-01216, D2-00468
// D2-01215, D2-00469, D2-01224
#[must_use]
pub fn pet_call(a: PetAction) -> (u32, Vec<InterfaceArg>) {
    use function as f;
    use InterfaceArg::{Long, Text, Word};
    match a {
        PetAction::Attack(target) => (f::PET_ATTACK, vec![Long(target)]),
        PetAction::Buff => (f::PET_BUFF, Vec::new()),
        PetAction::Heal => (f::PET_HEAL, Vec::new()),
        PetAction::Return => (f::PET_RETURN, Vec::new()),
        PetAction::Mode { pet, mode } => (f::PET_MODE, vec![Word(mode), Long(pet)]),
        PetAction::Name { pet, name } => (
            f::SET_PET_NAME,
            vec![Text(name.encode_utf16().collect()), Long(pet)],
        ),
    }
}
