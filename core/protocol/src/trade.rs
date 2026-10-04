//! Family: trade, housing, vendor and the rest — `docs/networking/messages/10-trade-housing-vendor.md`.
//!
//! **Corrections this family carries:**
//!
//! * `0x0200 Trade_AddToTrade` (S2C) has a **third dword** the catalogue omits, and `0x01FD
//!   Trade_RegisterTrade`'s stamp is an IEEE **double**, not a `long`.
//! * `0x0248 House_UpdateRestrictions` is **byte-packed** from offset 4 with an unaligned object id
//!   at offset 5, exactly like [`crate::items::ItemUpdateStackSize`].
//! * `ItemProfile`'s first dword packs a **24-bit signed** amount with a `0xFF` flag byte, and the
//!   `PublicWeenieDesc` follows only when that byte is `0xFF`.
//! * `PageData` is **versioned**: the dword after the author account is either `textIncluded`
//!   directly or a `0xFFFF00nn` marker, and only the marker `0xFFFF0002` is followed by the two flag
//!   dwords. The generated catalogue has this right.

use crate::archive::{PackedHash, Reader, Writer};
use crate::error::MessageError;
use crate::opcodes::Opcode;
use crate::types::{ContentProfile, PositionWire, PublicWeenieDesc, RestrictionDb};
use crate::Message;
use dereth_primitives::ObjectId;

macro_rules! empty_message {
    ($(#[$m:meta])* $name:ident, $op:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub struct $name;
        impl Message for $name {
            const OPCODE: Opcode = Opcode::$op;
            fn read(_: &mut Reader<'_>) -> Result<Self, MessageError> { Ok(Self) }
            fn write(&self, _: &mut Writer) -> Result<(), MessageError> { Ok(()) }
        }
    };
}

macro_rules! id_message {
    ($(#[$m:meta])* $name:ident, $op:ident, $field:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub struct $name { pub $field: ObjectId }
        impl Message for $name {
            const OPCODE: Opcode = Opcode::$op;
            fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
                Ok(Self { $field: ObjectId(r.u32()?) })
            }
            fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
                w.u32(self.$field.0);
                Ok(())
            }
        }
    };
}

macro_rules! scalar_message {
    ($(#[$m:meta])* $name:ident, $op:ident, $field:ident, $ty:ty, $rd:ident, $wr:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub struct $name { pub $field: $ty }
        impl Message for $name {
            const OPCODE: Opcode = Opcode::$op;
            fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
                Ok(Self { $field: r.$rd()? })
            }
            fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
                w.$wr(self.$field);
                Ok(())
            }
        }
    };
}

macro_rules! string_message {
    ($(#[$m:meta])* $name:ident, $op:ident, $field:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, PartialEq, Eq, Default)]
        pub struct $name { pub $field: String }
        impl Message for $name {
            const OPCODE: Opcode = Opcode::$op;
            fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
                Ok(Self { $field: r.pstring()? })
            }
            fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
                w.pstring(&self.$field)
            }
        }
    };
}

// ---------------------------------------------------------------------------------------------
// 1. Trade
// ---------------------------------------------------------------------------------------------

/// Which side of the trade window. **1 = self, 2 = partner**; anything else is ignored by
/// The remove-from-trade handler.
pub mod trade_side {
    pub const SELF: u32 = 1;
    pub const PARTNER: u32 = 2;
}

/// The whole `Trade` object, which `0x01FA Trade_AcceptTrade` sends packed.
///
/// It is the only client-to-server message in the game that sends a whole packed object rather than
/// scalars; the server uses it to detect desynchronisation.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Trade {
    pub self_list: Vec<ContentProfile>,
    pub partner_list: Vec<ContentProfile>,
    pub partner: ObjectId,
    /// The server's trade stamp — a **double**.
    pub stamp: f64,
    pub status: u32,
    /// 1 if *you* opened the window.
    pub initiator: i32,
    pub accepted: i32,
    pub partner_accepted: i32,
}

impl Trade {
    /// **The scalars come first and the two lists last.** The trade
    /// pack stores `partner`, then `stamp`, then `status`, `initiator`, `accepted` and
    /// `partner_accepted`, and only then the two lists — `self_list` first, then `partner_list`.
    ///
    /// Reading the two lists **first**, in the struct's field order, fails on real data: the two
    /// accepts in `house-purchase-and-trade.jsonl` (blobs 2540 and 2586) then read `1342177310`
    /// -- that is `0x5000001E`, the partner's `ObjectId` -- as a list length. The same defect
    /// shape as `Fellow`: a struct's offsets are not its wire order.
    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let partner = ObjectId(r.u32()?);
        let stamp = r.f64()?;
        let status = r.u32()?;
        let initiator = r.i32()?;
        let accepted = r.i32()?;
        let partner_accepted = r.i32()?;
        Ok(Self {
            self_list: r.packed_list(ContentProfile::read)?,
            partner_list: r.packed_list(ContentProfile::read)?,
            partner,
            stamp,
            status,
            initiator,
            accepted,
            partner_accepted,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.partner.0);
        w.f64(self.stamp);
        w.u32(self.status);
        w.i32(self.initiator);
        w.i32(self.accepted);
        w.i32(self.partner_accepted);
        w.packed_list(&self.self_list, |w, c| {
            c.write(w);
            Ok(())
        })?;
        w.packed_list(&self.partner_list, |w, c| {
            c.write(w);
            Ok(())
        })?;
        Ok(())
    }
}

id_message!(
    /// `0x01F6 Trade_OpenTradeNegotiations` (C2S).
    TradeOpenTradeNegotiations,
    TRADE_OPEN_TRADE_NEGOTIATIONS,
    partner
);
empty_message!(
    /// `0x01F7 Trade_CloseTradeNegotiations` (C2S).
    TradeCloseTradeNegotiations,
    TRADE_CLOSE_TRADE_NEGOTIATIONS
);
empty_message!(
    /// `0x01FB Trade_DeclineTrade` (C2S).
    TradeDeclineTradeRequest,
    TRADE_DECLINE_TRADE
);
empty_message!(
    /// `0x0204 Trade_ResetTrade` (C2S).
    TradeResetTradeRequest,
    TRADE_RESET_TRADE
);
empty_message!(
    /// `0x0208 Trade_ClearTradeAcceptance` (S2C) — "someone changed the contents, both accepts are
    /// void".
    TradeClearTradeAcceptance,
    TRADE_CLEAR_TRADE_ACCEPTANCE
);

/// `0x01F8 Trade_AddToTrade`, **client to server**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TradeAddToTrade {
    pub item: ObjectId,
    pub slot: u32,
}

impl Message for TradeAddToTrade {
    const OPCODE: Opcode = Opcode::TRADE_ADD_TO_TRADE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            item: ObjectId(r.u32()?),
            slot: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.item.0);
        w.u32(self.slot);
        Ok(())
    }
}

/// `0x01FA Trade_AcceptTrade` (C2S) — the whole packed [`Trade`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TradeAcceptTradeRequest(pub Trade);

impl Message for TradeAcceptTradeRequest {
    const OPCODE: Opcode = Opcode::TRADE_ACCEPT_TRADE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self(Trade::read(r)?))
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        self.0.write(w)
    }
}

/// `0x01FD Trade_RegisterTrade` (S2C).
///
/// **The stamp is an IEEE `f64`** at offset 0x0C; the catalogue says `long`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TradeRegisterTrade {
    pub initiator: ObjectId,
    pub partner: ObjectId,
    pub stamp: f64,
}

impl Message for TradeRegisterTrade {
    const OPCODE: Opcode = Opcode::TRADE_REGISTER_TRADE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            initiator: ObjectId(r.u32()?),
            partner: ObjectId(r.u32()?),
            stamp: r.f64()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.initiator.0);
        w.u32(self.partner.0);
        w.f64(self.stamp);
        Ok(())
    }
}

/// `0x0200 Trade_AddToTrade`, **server to client**.
///
/// Carries item, side, and a third dword that is decoded and passed through unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TradeAddToTradeRecv {
    pub item: ObjectId,
    /// See [`trade_side`].
    pub side: u32,
    // The third dword's meaning is unspecified here.
    pub container_properties: u32,
}

impl Message for TradeAddToTradeRecv {
    const OPCODE: Opcode = Opcode::TRADE_ADD_TO_TRADE_RECV;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            item: ObjectId(r.u32()?),
            side: r.u32()?,
            container_properties: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.item.0);
        w.u32(self.side);
        w.u32(self.container_properties);
        Ok(())
    }
}

/// `0x0201 Trade_RemoveFromTrade` (S2C).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TradeRemoveFromTrade {
    pub item: ObjectId,
    pub side: u32,
}

impl Message for TradeRemoveFromTrade {
    const OPCODE: Opcode = Opcode::TRADE_REMOVE_FROM_TRADE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            item: ObjectId(r.u32()?),
            side: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.item.0);
        w.u32(self.side);
        Ok(())
    }
}

/// `0x0207 Trade_TradeFailure` (S2C). The handler also removes the item from side 1 first: a failed
/// add is rolled back locally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TradeTradeFailure {
    pub item: ObjectId,
    pub reason: u32,
}

impl Message for TradeTradeFailure {
    const OPCODE: Opcode = Opcode::TRADE_TRADE_FAILURE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            item: ObjectId(r.u32()?),
            reason: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.item.0);
        w.u32(self.reason);
        Ok(())
    }
}

id_message!(
    /// `0x01FE Trade_OpenTrade` (S2C).
    TradeOpenTrade,
    TRADE_OPEN_TRADE,
    source
);
id_message!(
    /// `0x0202 Trade_AcceptTrade` (S2C). `source == 0` clears *your* accept flag; `source ==
    /// player_id` sets it; anything else sets the partner's.
    TradeAcceptTradeRecv,
    TRADE_ACCEPT_TRADE_RECV,
    source
);
id_message!(
    /// `0x0203 Trade_DeclineTrade` (S2C) — the mirror image of `0x0202`.
    TradeDeclineTradeRecv,
    TRADE_DECLINE_TRADE_RECV,
    source
);
id_message!(
    /// `0x0205 Trade_ResetTrade` (S2C).
    TradeResetTradeRecv,
    TRADE_RESET_TRADE_RECV,
    source
);
scalar_message!(
    /// `0x01FF Trade_CloseTrade` (S2C).
    TradeCloseTrade,
    TRADE_CLOSE_TRADE,
    reason,
    u32,
    u32,
    u32
);

// ---------------------------------------------------------------------------------------------
// 2. Vendors
// ---------------------------------------------------------------------------------------------

/// One vendor profile.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VendorProfile {
    /// `ITEM_TYPE` mask of what this vendor buys.
    pub item_types: u32,
    pub min_value: i32,
    pub max_value: i32,
    pub magic: i32,
    pub buy_price: f32,
    pub sell_price: f32,
    /// Alternate-currency wcid.
    pub trade_id: u32,
    pub trade_num: i32,
    pub trade_name: String,
}

impl VendorProfile {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            item_types: r.u32()?,
            min_value: r.i32()?,
            max_value: r.i32()?,
            magic: r.i32()?,
            buy_price: r.f32()?,
            sell_price: r.f32()?,
            trade_id: r.u32()?,
            trade_num: r.i32()?,
            trade_name: r.pstring()?,
        })
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.item_types);
        w.i32(self.min_value);
        w.i32(self.max_value);
        w.i32(self.magic);
        w.f32(self.buy_price);
        w.f32(self.sell_price);
        w.u32(self.trade_id);
        w.i32(self.trade_num);
        w.pstring(&self.trade_name)
    }
}

/// One item profile.
///
/// A genuinely unusual encoding: the first dword is `amount | (has_pwd ? 0xFF000000 : 0)`, the
/// amount is a **24-bit signed** value (`if (v & 0x800000) amount = v | 0xFF000000`), and the
/// `PublicWeenieDesc` is present only when `(int32)v >> 24 == -1`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ItemProfile {
    /// The sign-extended 24-bit amount.
    pub amount: i32,
    pub iid: ObjectId,
    pub pwd: Option<PublicWeenieDesc>,
}

impl ItemProfile {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let v = r.u32()?;
        // Sign-extend the low 24 bits.
        let amount = if v & 0x0080_0000 != 0 {
            (v | 0xFF00_0000) as i32
        } else {
            (v & 0x00FF_FFFF) as i32
        };
        let iid = ObjectId(r.u32()?);
        let pwd = if (v as i32) >> 24 == -1 {
            Some(PublicWeenieDesc::read(r)?)
        } else {
            None
        };
        Ok(Self { amount, iid, pwd })
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        if !(-0x0080_0000..0x0080_0000).contains(&self.amount) {
            return Err(MessageError::Unencodable {
                field: "item-profile amount",
                reason: "the amount is a 24-bit signed value",
            });
        }
        let mut v = (self.amount as u32) & 0x00FF_FFFF;
        if self.pwd.is_some() {
            v |= 0xFF00_0000;
        }
        w.u32(v);
        w.u32(self.iid.0);
        if let Some(p) = &self.pwd {
            p.write(w)?;
        }
        Ok(())
    }
}

/// `0x0062 Vendor_VendorInfo` (S2C).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VendorInfo {
    pub merchant_id: ObjectId,
    pub profile: VendorProfile,
    pub items: Vec<ItemProfile>,
}

impl Message for VendorInfo {
    const OPCODE: Opcode = Opcode::VENDOR_VENDOR_INFO;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            merchant_id: ObjectId(r.u32()?),
            profile: VendorProfile::read(r)?,
            items: r.packed_list(ItemProfile::read)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.merchant_id.0);
        self.profile.write(w)?;
        w.packed_list(&self.items, |w, i| i.write(w))
    }
}

/// `0x005F Vendor_Buy` (C2S) — note the trailing alternate-currency dword, which `Vendor_Sell` does
/// not have.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VendorBuy {
    pub vendor_id: ObjectId,
    pub items: Vec<ItemProfile>,
    pub alternate_currency_id: u32,
}

impl Message for VendorBuy {
    const OPCODE: Opcode = Opcode::VENDOR_BUY;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            vendor_id: ObjectId(r.u32()?),
            items: r.packed_list(ItemProfile::read)?,
            alternate_currency_id: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.vendor_id.0);
        w.packed_list(&self.items, |w, i| i.write(w))?;
        w.u32(self.alternate_currency_id);
        Ok(())
    }
}

/// `0x0060 Vendor_Sell` (C2S) — **no** currency field.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VendorSell {
    pub vendor_id: ObjectId,
    pub items: Vec<ItemProfile>,
}

impl Message for VendorSell {
    const OPCODE: Opcode = Opcode::VENDOR_SELL;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            vendor_id: ObjectId(r.u32()?),
            items: r.packed_list(ItemProfile::read)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.vendor_id.0);
        w.packed_list(&self.items, |w, i| i.write(w))
    }
}

// ---------------------------------------------------------------------------------------------
// 3. Housing
// ---------------------------------------------------------------------------------------------

/// `HousePayment`.
///
/// **The field order is not the layout order, and a round trip cannot see it.** The client's
/// struct keeps the weenie id first, but the house-payment unpack reads `num`, `paid`, `wcid`,
/// then `name` and `pname` as narrow strings. `HousePaymentExtensions.Write` in
/// `ACE.Server/Network/Structure/HousePayment.cs` writes exactly that — `Num`, `Paid`, `WeenieID`,
/// `Name`, `PluralName` — so both oracles agree; reading `wcid, num, paid` decodes a live
/// shard's price list with the quantity in the weenie field. `0x0225` has never been recorded
/// (zero in all three opcode spaces over the 13,535 recorded blobs), so only a live shard
/// exercises this order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HousePayment {
    /// Required.
    pub num: i32,
    pub paid: i32,
    pub wcid: u32,
    pub name: String,
    pub plural_name: String,
}

impl HousePayment {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            num: r.i32()?,
            paid: r.i32()?,
            wcid: r.u32()?,
            name: r.pstring()?,
            plural_name: r.pstring()?,
        })
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.i32(self.num);
        w.i32(self.paid);
        w.u32(self.wcid);
        w.pstring(&self.name)?;
        w.pstring(&self.plural_name)
    }
}

/// `HouseProfile`.
///
/// **The wire order differs from the struct order in five places.** `0x021D` is a corpus zero
/// (zero in all three opcode spaces over the 13,535 recorded blobs), so a self-consistent round
/// trip cannot catch a wrong order.
///
/// The wire is read by the **virtual** entry, which does nothing but call the unpack with a
/// hard-coded `version = 3`,
///
/// so **there is no version dword on the wire** — the client always parses the version-3 shape,
/// and the unpack's own size floor agrees (`0x14` at version 0, `0x1C` at 1, `+4` at 2, `+4` at 3
/// = `0x24`, i.e. nine dwords). The nine dwords, in store order:
/// `id`, `owner`, `bitmask`, `min_level`, `max_level`, `min_alleg_rank` and `max_alleg_rank`
/// (version >= 1), `maintenance_free` (version >= 2) and `type` (version >= 3),
///
/// then the three packed sub-records, in this order: `name`, then `buy`, then `rent`.
/// `HouseProfileExtensions.Write` in
/// `ACE.Server/Network/Structure/HouseProfile.cs` writes the same twelve in the same order, so
/// both oracles agree.
///
/// Reading `id, owner, name, bitmask, buy, rent, minLevel, …` — the name and the two price lists
/// hoisted four and six positions early — misaligns **everything after the second dword**.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HouseProfile {
    pub id: ObjectId,
    pub owner: ObjectId,
    /// The house bitmask.
    pub bitmask: u32,
    pub min_level: i32,
    pub max_level: i32,
    pub min_alleg_rank: i32,
    pub max_alleg_rank: i32,
    pub maintenance_free: i32,
    /// 1 cottage, 2 villa, 3 mansion, 4 apartment.
    pub house_type: u32,
    /// ACE calls it `OwnerName`; the client's member is `_name` at `+0Ch`.
    pub name: String,
    pub buy: Vec<HousePayment>,
    pub rent: Vec<HousePayment>,
}

impl HouseProfile {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            id: ObjectId(r.u32()?),
            owner: ObjectId(r.u32()?),
            bitmask: r.u32()?,
            min_level: r.i32()?,
            max_level: r.i32()?,
            min_alleg_rank: r.i32()?,
            max_alleg_rank: r.i32()?,
            maintenance_free: r.i32()?,
            house_type: r.u32()?,
            name: r.pstring()?,
            buy: r.packed_list(HousePayment::read)?,
            rent: r.packed_list(HousePayment::read)?,
        })
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.id.0);
        w.u32(self.owner.0);
        w.u32(self.bitmask);
        w.i32(self.min_level);
        w.i32(self.max_level);
        w.i32(self.min_alleg_rank);
        w.i32(self.max_alleg_rank);
        w.i32(self.maintenance_free);
        w.u32(self.house_type);
        w.pstring(&self.name)?;
        w.packed_list(&self.buy, |w, p| p.write(w))?;
        w.packed_list(&self.rent, |w, p| p.write(w))?;
        Ok(())
    }
}

/// `GuestInfo`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GuestInfo {
    pub item_storage_permission: i32,
    pub char_name: String,
}

impl GuestInfo {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            item_storage_permission: r.i32()?,
            char_name: r.pstring()?,
        })
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.i32(self.item_storage_permission);
        w.pstring(&self.char_name)
    }
}

/// The house access record.
///
/// Three shapes keyed on the first dword, exactly like [`RestrictionDb`], plus a trailing align to
/// 4 that the version-0 shape does **not** have. Read directly from the client's own house-access
/// restrictions.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Har {
    /// 0 for the oldest form, where the first dword was the open-house flag itself.
    pub version: u32,
    /// The house-access-record bitmask.
    pub bitmask: u32,
    pub monarch_iid: ObjectId,
    pub guest_table: PackedHash<u32, GuestInfo>,
    /// Only present when `version >= 0x10000002`.
    pub roommate_list: Option<Vec<u32>>,
}

impl Har {
    /// The version every shipped server sends.
    pub const CURRENT_VERSION: u32 = 0x1000_0002;

    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let first = r.u32()?;
        let version = if first & 0xFFFF_0000 == 0 { 0 } else { first };
        let mut h = Self {
            version,
            ..Self::default()
        };
        if version == 0 {
            // The dword just read was the open-house flag itself.
            h.bitmask = u32::from(first != 0);
            h.guest_table = r.packed_hash(|r| Ok((r.u32()?, GuestInfo::read(r)?)))?;
            return Ok(h);
        }
        h.bitmask = r.u32()?;
        h.monarch_iid = ObjectId(r.u32()?);
        h.guest_table = r.packed_hash(|r| Ok((r.u32()?, GuestInfo::read(r)?)))?;
        if version >= Self::CURRENT_VERSION {
            h.roommate_list = Some(r.packed_list(Reader::u32)?);
        }
        r.align4()?;
        Ok(h)
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        let pair = |w: &mut Writer, k: &u32, v: &GuestInfo| -> Result<(), MessageError> {
            w.u32(*k);
            v.write(w)
        };
        if self.version == 0 {
            w.u32(self.bitmask & 1);
            return w.packed_hash(&self.guest_table, pair);
        }
        w.u32(self.version);
        w.u32(self.bitmask);
        w.u32(self.monarch_iid.0);
        w.packed_hash(&self.guest_table, pair)?;
        if let Some(l) = &self.roommate_list {
            w.packed_list(l, |w, v| {
                w.u32(*v);
                Ok(())
            })?;
        }
        w.align4();
        Ok(())
    }
}

/// `0x021D House_HouseProfile` (S2C).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HouseProfileMessage {
    pub covenant_crystal: ObjectId,
    pub profile: HouseProfile,
}

impl Message for HouseProfileMessage {
    const OPCODE: Opcode = Opcode::HOUSE_HOUSE_PROFILE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            covenant_crystal: ObjectId(r.u32()?),
            profile: HouseProfile::read(r)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.covenant_crystal.0);
        self.profile.write(w)
    }
}

/// `0x0225 House_HouseData` (S2C).
///
/// **Wire order, not struct order, here too.** The house-data unpack reads four scalars and
/// three nested structures, in this order: `buy_time`, `rent_time`, `type`, `maintenance_free`,
/// then the buy profile, the rent profile and the position.
///
/// `HouseDataExtensions.Write` in `ACE.Server/Network/Structure/HouseData.cs` writes
/// `BuyTime, RentTime, Type, MaintenanceFree, Buy, Rent, Position` — the same seven in the same
/// order. The previous reader put `buy`/`rent` before `type`/`maintenance_free` and `position`
/// last, so every field after the second dword was misaligned; the `0x10` minimum-size check at
/// The unpack's size floor is the only thing that would have rejected it, and it passes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HouseDataMessage {
    pub buy_time: i32,
    pub rent_time: i32,
    pub house_type: u32,
    pub maintenance_free: i32,
    pub buy: Vec<HousePayment>,
    pub rent: Vec<HousePayment>,
    /// 32 bytes on the wire; the 72 in the class layout is the in-memory size.
    pub position: PositionWire,
}

impl Message for HouseDataMessage {
    const OPCODE: Opcode = Opcode::HOUSE_HOUSE_DATA;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            buy_time: r.i32()?,
            rent_time: r.i32()?,
            house_type: r.u32()?,
            maintenance_free: r.i32()?,
            buy: r.packed_list(HousePayment::read)?,
            rent: r.packed_list(HousePayment::read)?,
            position: PositionWire::read(r)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.i32(self.buy_time);
        w.i32(self.rent_time);
        w.u32(self.house_type);
        w.i32(self.maintenance_free);
        w.packed_list(&self.buy, |w, p| p.write(w))?;
        w.packed_list(&self.rent, |w, p| p.write(w))?;
        self.position.write(w);
        Ok(())
    }
}

scalar_message!(
    /// `0x0226 House_HouseStatus` (S2C). **The client handles this and `0x0259` with the same
    /// function**; there is no behavioural difference.
    HouseHouseStatus,
    HOUSE_HOUSE_STATUS,
    notice_type,
    u32,
    u32,
    u32
);
scalar_message!(
    /// `0x0259 House_HouseTransaction` (S2C) — the same handler as `0x0226`.
    HouseHouseTransaction,
    HOUSE_HOUSE_TRANSACTION,
    notice_type,
    u32,
    u32,
    u32
);
scalar_message!(
    /// `0x0227 House_UpdateRentTime` (S2C).
    HouseUpdateRentTime,
    HOUSE_UPDATE_RENT_TIME,
    rent_time,
    i32,
    i32,
    i32
);

/// `0x0228 House_UpdateRentPayment` (S2C).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HouseUpdateRentPayment {
    pub payments: Vec<HousePayment>,
}

impl Message for HouseUpdateRentPayment {
    const OPCODE: Opcode = Opcode::HOUSE_UPDATE_RENT_PAYMENT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            payments: r.packed_list(HousePayment::read)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.packed_list(&self.payments, |w, p| p.write(w))
    }
}

/// `0x0248 House_UpdateRestrictions` (S2C) — **byte-packed, unaligned from offset 5**.
///
/// The community catalogue documents it as dword-aligned
/// like the rest of the protocol; reading it that way corrupts every restriction table and
/// everything after it in the blob.
///
/// `0x0248` does not reach both the update-restrictions and the update-HAR handlers: each
/// handler compares against one opcode only. Each refuses everything but its own opcode —
/// `0x0257` and `0x0248` respectively — and returns 0, so whatever the switch does upstream,
/// only one of the two can ever act. (Decimal `599` is `0x257`, not a second name for `0x0248`.)
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HouseUpdateRestrictions {
    pub sequence: u8,
    /// **Unaligned**: it starts at offset 5 of the blob.
    pub sender: ObjectId,
    pub restrictions: RestrictionDb,
}

impl Message for HouseUpdateRestrictions {
    const OPCODE: Opcode = Opcode::HOUSE_UPDATE_RESTRICTIONS;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        // No align after the sequence byte.
        Ok(Self {
            sequence: r.u8()?,
            sender: ObjectId(r.u32()?),
            restrictions: RestrictionDb::read(r)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u8(self.sequence);
        w.u32(self.sender.0);
        self.restrictions.write(w)
    }
}

/// `0x0257 House_UpdateHAR` (S2C).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HouseUpdateHar(pub Har);

impl Message for HouseUpdateHar {
    const OPCODE: Opcode = Opcode::HOUSE_UPDATE_HAR;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self(Har::read(r)?))
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        self.0.write(w)
    }
}

/// `0x0271 House_AvailableHouses` (S2C).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HouseAvailableHouses {
    pub house_type: u32,
    pub landcells: Vec<u32>,
    pub num_houses: i32,
}

impl Message for HouseAvailableHouses {
    const OPCODE: Opcode = Opcode::HOUSE_AVAILABLE_HOUSES;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            house_type: r.u32()?,
            landcells: r.packed_list(Reader::u32)?,
            num_houses: r.i32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.house_type);
        w.packed_list(&self.landcells, |w, v| {
            w.u32(*v);
            Ok(())
        })?;
        w.i32(self.num_houses);
        Ok(())
    }
}

/// `0x021C House_BuyHouse` and `0x0221 House_RentHouse` (C2S) share a layout.
macro_rules! house_payment_action {
    ($(#[$m:meta])* $name:ident, $op:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, PartialEq, Eq, Default)]
        pub struct $name {
            pub slumlord: ObjectId,
            pub items: Vec<ObjectId>,
        }

        impl Message for $name {
            const OPCODE: Opcode = Opcode::$op;

            fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
                Ok(Self {
                    slumlord: ObjectId(r.u32()?),
                    items: r.packed_list(|r| Ok(ObjectId(r.u32()?)))?,
                })
            }

            fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
                w.u32(self.slumlord.0);
                w.packed_list(&self.items, |w, i| {
                    w.u32(i.0);
                    Ok(())
                })
            }
        }
    };
}

house_payment_action!(
    /// `0x021C House_BuyHouse`.
    HouseBuyHouse,
    HOUSE_BUY_HOUSE
);
house_payment_action!(
    /// `0x0221 House_RentHouse`.
    HouseRentHouse,
    HOUSE_RENT_HOUSE
);

empty_message!(
    /// `0x021E House_QueryHouse`.
    HouseQueryHouse,
    HOUSE_QUERY_HOUSE
);
empty_message!(
    /// `0x021F House_AbandonHouse`.
    HouseAbandonHouse,
    HOUSE_ABANDON_HOUSE
);
empty_message!(
    /// `0x024C House_RemoveAllStoragePermission`.
    HouseRemoveAllStoragePermission,
    HOUSE_REMOVE_ALL_STORAGE_PERMISSION
);
empty_message!(
    /// `0x024D House_RequestFullGuestList`.
    HouseRequestFullGuestList,
    HOUSE_REQUEST_FULL_GUEST_LIST
);
empty_message!(
    /// `0x025C House_AddAllStoragePermission`.
    HouseAddAllStoragePermission,
    HOUSE_ADD_ALL_STORAGE_PERMISSION
);
empty_message!(
    /// `0x025E House_RemoveAllPermanentGuests`.
    HouseRemoveAllPermanentGuests,
    HOUSE_REMOVE_ALL_PERMANENT_GUESTS
);
empty_message!(
    /// `0x025F House_BootEveryone`.
    HouseBootEveryone,
    HOUSE_BOOT_EVERYONE
);
empty_message!(
    /// `0x0262 House_TeleToHouse`.
    HouseTeleToHouse,
    HOUSE_TELE_TO_HOUSE
);
empty_message!(
    /// `0x0278 House_TeleToMansion`.
    HouseTeleToMansion,
    HOUSE_TELE_TO_MANSION
);

string_message!(
    /// `0x0245 House_AddPermanentGuest`.
    HouseAddPermanentGuest,
    HOUSE_ADD_PERMANENT_GUEST,
    name
);
string_message!(
    /// `0x0246 House_RemovePermanentGuest`.
    HouseRemovePermanentGuest,
    HOUSE_REMOVE_PERMANENT_GUEST,
    name
);
string_message!(
    /// `0x024A House_BootSpecificHouseGuest`.
    HouseBootSpecificHouseGuest,
    HOUSE_BOOT_SPECIFIC_HOUSE_GUEST,
    name
);
scalar_message!(
    /// `0x0247 House_SetOpenHouseStatus`.
    HouseSetOpenHouseStatus,
    HOUSE_SET_OPEN_HOUSE_STATUS,
    open,
    i32,
    i32,
    i32
);
scalar_message!(
    /// `0x0266 House_SetHooksVisibility`.
    HouseSetHooksVisibility,
    HOUSE_SET_HOOKS_VISIBILITY,
    visible,
    i32,
    i32,
    i32
);
scalar_message!(
    /// `0x0267 House_ModifyAllegianceGuestPermission`.
    HouseModifyAllegianceGuestPermission,
    HOUSE_MODIFY_ALLEGIANCE_GUEST_PERMISSION,
    allow,
    i32,
    i32,
    i32
);
scalar_message!(
    /// `0x0268 House_ModifyAllegianceStoragePermission`.
    HouseModifyAllegianceStoragePermission,
    HOUSE_MODIFY_ALLEGIANCE_STORAGE_PERMISSION,
    allow,
    i32,
    i32,
    i32
);
scalar_message!(
    /// `0x0270 House_ListAvailableHouses`.
    HouseListAvailableHouses,
    HOUSE_LIST_AVAILABLE_HOUSES,
    house_type,
    u32,
    u32,
    u32
);
id_message!(
    /// `0x0258 House_QueryLord`.
    HouseQueryLord,
    HOUSE_QUERY_LORD,
    target
);

/// `0x0249 House_ChangeStoragePermission`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HouseChangeStoragePermission {
    pub name: String,
    pub has_permission: i32,
}

impl Message for HouseChangeStoragePermission {
    const OPCODE: Opcode = Opcode::HOUSE_CHANGE_STORAGE_PERMISSION;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            name: r.pstring()?,
            has_permission: r.i32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.pstring(&self.name)?;
        w.i32(self.has_permission);
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// 4. Chess
// ---------------------------------------------------------------------------------------------

/// The game-move data — variable-length, keyed on its move type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GameMoveData {
    /// `MoveType`: 0 Invalid, 1 Pass, 2 Resign, 3 Stalemate, 4 Grid, 5 FromTo, 6 SelectedPiece.
    pub move_type: u32,
    pub player: ObjectId,
    /// `Grid` (4) and `FromTo` (5).
    pub from: Option<(u32, u32)>,
    /// `FromTo` (5) only.
    pub to: Option<(u32, u32)>,
    /// `SelectedPiece` (6) only.
    pub piece_index: Option<u32>,
}

impl GameMoveData {
    pub const GRID: u32 = 4;
    pub const FROM_TO: u32 = 5;
    pub const SELECTED_PIECE: u32 = 6;

    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let move_type = r.u32()?;
        let player = ObjectId(r.u32()?);
        let mut d = Self {
            move_type,
            player,
            ..Self::default()
        };
        match move_type {
            Self::GRID => d.from = Some((r.u32()?, r.u32()?)),
            Self::FROM_TO => {
                d.from = Some((r.u32()?, r.u32()?));
                d.to = Some((r.u32()?, r.u32()?));
            }
            Self::SELECTED_PIECE => d.piece_index = Some(r.u32()?),
            _ => {}
        }
        Ok(d)
    }

    pub fn write(&self, w: &mut Writer) {
        w.u32(self.move_type);
        w.u32(self.player.0);
        if let Some((x, y)) = self.from {
            w.u32(x);
            w.u32(y);
        }
        if let Some((x, y)) = self.to {
            w.u32(x);
            w.u32(y);
        }
        if let Some(p) = self.piece_index {
            w.u32(p);
        }
    }
}

/// `0x0269 Game_Join` (C2S).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GameJoin {
    pub game_id: u32,
    pub which_team: u32,
}

impl Message for GameJoin {
    const OPCODE: Opcode = Opcode::GAME_JOIN;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            game_id: r.u32()?,
            which_team: r.u32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.game_id);
        w.u32(self.which_team);
        Ok(())
    }
}

empty_message!(
    /// `0x026A Game_Quit` (C2S).
    GameQuit,
    GAME_QUIT
);
empty_message!(
    /// `0x026D Game_MovePass` (C2S).
    GameMovePass,
    GAME_MOVE_PASS
);
scalar_message!(
    /// `0x026E Game_Stalemate` (C2S).
    GameStalemate,
    GAME_STALEMATE,
    on,
    i32,
    i32,
    i32
);

/// `0x026B Game_Move` (C2S) — **four raw ints**, not a [`GameMoveData`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GameMove {
    pub x_from: i32,
    pub y_from: i32,
    pub x_to: i32,
    pub y_to: i32,
}

impl Message for GameMove {
    const OPCODE: Opcode = Opcode::GAME_MOVE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            x_from: r.i32()?,
            y_from: r.i32()?,
            x_to: r.i32()?,
            y_to: r.i32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.i32(self.x_from);
        w.i32(self.y_from);
        w.i32(self.x_to);
        w.i32(self.y_to);
        Ok(())
    }
}

/// The `[game id][int]` shape shared by four of the chess responses.
macro_rules! game_id_and_int {
    ($(#[$m:meta])* $name:ident, $op:ident, $field:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub struct $name {
            pub game_id: u32,
            pub $field: i32,
        }

        impl Message for $name {
            const OPCODE: Opcode = Opcode::$op;

            fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
                Ok(Self { game_id: r.u32()?, $field: r.i32()? })
            }

            fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
                w.u32(self.game_id);
                w.i32(self.$field);
                Ok(())
            }
        }
    };
}

game_id_and_int!(
    /// `0x0281 Game_JoinGameResponse` — team `-1` means refused.
    GameJoinGameResponse,
    GAME_JOIN_GAME_RESPONSE,
    team
);
game_id_and_int!(
    /// `0x0282 Game_StartGame` — the team that moves first.
    GameStartGame,
    GAME_START_GAME,
    team
);
game_id_and_int!(
    /// `0x0283 Game_MoveResponse` — a `ChessMoveResult`; the server's codes only ever reject.
    GameMoveResponse,
    GAME_MOVE_RESPONSE,
    result
);
game_id_and_int!(
    /// `0x028C Game_GameOver`.
    GameGameOver,
    GAME_GAME_OVER,
    team_winner
);

/// `0x0284 Game_OpponentTurn` (S2C) — the variable-length [`GameMoveData`] form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GameOpponentTurn {
    pub game_id: u32,
    pub team: i32,
    pub move_data: GameMoveData,
}

impl Message for GameOpponentTurn {
    const OPCODE: Opcode = Opcode::GAME_OPPONENT_TURN;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            game_id: r.u32()?,
            team: r.i32()?,
            move_data: GameMoveData::read(r)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.game_id);
        w.i32(self.team);
        self.move_data.write(w);
        Ok(())
    }
}

/// `0x0285 Game_OpponentStalemateState` (S2C).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GameOpponentStalemateState {
    pub game_id: u32,
    pub team: i32,
    pub on: i32,
}

impl Message for GameOpponentStalemateState {
    const OPCODE: Opcode = Opcode::GAME_OPPONENT_STALEMATE_STATE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            game_id: r.u32()?,
            team: r.i32()?,
            on: r.i32()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.game_id);
        w.i32(self.team);
        w.i32(self.on);
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// 5. Books
// ---------------------------------------------------------------------------------------------

/// One book page.
///
/// After the author account comes one dword `v`:
///
/// * `v >> 16 == 0xFFFF` — a version marker. When the low half is 2, `textIncluded` and
///   `ignoreAuthor` follow as two dwords; otherwise neither is read and the object's constructor
///   defaults (`textIncluded = 1`, `ignoreAuthor = 0`) stand.
/// * otherwise — `v` **is** `textIncluded` and `ignoreAuthor` is 0.
///
/// Then `pageText` follows when `textIncluded != 0`. Read directly from the client's own page
/// data; the generated page-data catalogue documents the same versioned shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageData {
    pub author_id: ObjectId,
    pub author_name: String,
    pub author_account: String,
    /// `Some(n)` when the dword was the `0xFFFF00nn` marker.
    pub version: Option<u16>,
    pub text_included: i32,
    pub ignore_author: i32,
    pub page_text: Option<String>,
}

impl Default for PageData {
    fn default() -> Self {
        // The client's constructor defaults, which matter when a version marker other than 2
        // arrives and neither field is read.
        Self {
            author_id: ObjectId(0),
            author_name: String::new(),
            author_account: String::new(),
            version: None,
            text_included: 1,
            ignore_author: 0,
            // The client's constructor leaves `pageText` as the shared null string, and
            // `textIncluded = 1` means the field is on the wire, so the default must be `Some`.
            page_text: Some(String::new()),
        }
    }
}

impl PageData {
    /// The only version marker whose low half the client acts on.
    pub const VERSION_WITH_FLAGS: u16 = 2;

    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        let author_id = ObjectId(r.u32()?);
        let author_name = r.pstring()?;
        let author_account = r.pstring()?;
        let v = r.u32()?;
        let mut p = Self {
            author_id,
            author_name,
            author_account,
            ..Self::default()
        };
        if v >> 16 == 0xFFFF {
            // Guarded by the branch: the low half fits in 16 bits.
            #[allow(clippy::cast_possible_truncation)]
            let low = (v & 0xFFFF) as u16;
            p.version = Some(low);
            if low == Self::VERSION_WITH_FLAGS {
                p.text_included = r.i32()?;
                p.ignore_author = r.i32()?;
            }
        } else {
            p.text_included = v as i32;
            p.ignore_author = 0;
        }
        if p.text_included != 0 {
            p.page_text = Some(r.pstring()?);
        }
        Ok(p)
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.author_id.0);
        w.pstring(&self.author_name)?;
        w.pstring(&self.author_account)?;
        match self.version {
            Some(v) => {
                w.u32(0xFFFF_0000 | u32::from(v));
                if v == Self::VERSION_WITH_FLAGS {
                    w.i32(self.text_included);
                    w.i32(self.ignore_author);
                }
            }
            None => w.i32(self.text_included),
        }
        if self.text_included != 0 {
            let text = self.page_text.as_deref().ok_or(MessageError::Unencodable {
                field: "page text",
                reason: "textIncluded is set but no text is present",
            })?;
            w.pstring(text)?;
        }
        Ok(())
    }
}

/// The page list — two dwords then a plain `int32` count and that many pages.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PageDataList {
    pub max_num_pages: i32,
    pub max_num_chars_per_page: i32,
    pub pages: Vec<PageData>,
}

impl PageDataList {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            max_num_pages: r.i32()?,
            max_num_chars_per_page: r.i32()?,
            pages: r.packed_list(PageData::read)?,
        })
    }

    pub fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.i32(self.max_num_pages);
        w.i32(self.max_num_chars_per_page);
        w.packed_list(&self.pages, |w, p| p.write(w))
    }
}

/// `0x00B4 Writing_BookOpen` (S2C).
///
/// Confirmed field by field against the UI-queue dispatch arm for `0xB4`: book id,
/// a second dword, the `PageDataList`, the inscription, the scribe id, the scribe name.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WritingBookOpen {
    pub book_id: ObjectId,
    pub max_num_pages: u32,
    pub pages: PageDataList,
    pub inscription: String,
    pub scribe_id: ObjectId,
    pub scribe_name: String,
}

impl Message for WritingBookOpen {
    const OPCODE: Opcode = Opcode::WRITING_BOOK_OPEN;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            book_id: ObjectId(r.u32()?),
            max_num_pages: r.u32()?,
            pages: PageDataList::read(r)?,
            inscription: r.pstring()?,
            scribe_id: ObjectId(r.u32()?),
            scribe_name: r.pstring()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.book_id.0);
        w.u32(self.max_num_pages);
        self.pages.write(w)?;
        w.pstring(&self.inscription)?;
        w.u32(self.scribe_id.0);
        w.pstring(&self.scribe_name)
    }
}

/// `0x00B6` / `0x00B7` — the add- and delete-page responses, 16 bytes each.
macro_rules! book_page_response {
    ($(#[$m:meta])* $name:ident, $op:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub struct $name {
            pub book_id: ObjectId,
            pub page_number: u32,
            pub success: i32,
        }

        impl Message for $name {
            const OPCODE: Opcode = Opcode::$op;

            fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
                Ok(Self {
                    book_id: ObjectId(r.u32()?),
                    page_number: r.u32()?,
                    success: r.i32()?,
                })
            }

            fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
                w.u32(self.book_id.0);
                w.u32(self.page_number);
                w.i32(self.success);
                Ok(())
            }
        }
    };
}

book_page_response!(
    /// `0x00B5 Writing_BookModifyPageResponse`. The retail client drops it; the retail server
    /// answered each page edit with one (the retail captures).
    WritingBookModifyPageResponse,
    WRITING_BOOK_MODIFY_PAGE_RESPONSE
);
book_page_response!(
    /// `0x00B6 Writing_BookAddPageResponse`.
    WritingBookAddPageResponse,
    WRITING_BOOK_ADD_PAGE_RESPONSE
);
book_page_response!(
    /// `0x00B7 Writing_BookDeletePageResponse`.
    WritingBookDeletePageResponse,
    WRITING_BOOK_DELETE_PAGE_RESPONSE
);

/// `0x00B8 Writing_BookPageDataResponse` (S2C).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BookPageDataResponse {
    pub object_id: ObjectId,
    pub page: u32,
    pub data: PageData,
}

impl Message for BookPageDataResponse {
    const OPCODE: Opcode = Opcode::WRITING_BOOK_PAGE_DATA_RESPONSE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            object_id: ObjectId(r.u32()?),
            page: r.u32()?,
            data: PageData::read(r)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.object_id.0);
        w.u32(self.page);
        self.data.write(w)
    }
}

id_message!(
    /// `0x00AA Writing_BookData` (C2S) — request the book's data. The *client's* wrapper for it
    /// is named after the book-data request, which is the catalogue's name for this opcode; the
    /// confusion is in the other direction from what an earlier reading suggested.
    WritingBookData,
    WRITING_BOOK_DATA,
    book_id
);
id_message!(
    /// `0x00AC Writing_BookAddPage` (C2S).
    WritingBookAddPage,
    WRITING_BOOK_ADD_PAGE,
    book_id
);

/// `0x00AB Writing_BookModifyPage` (C2S).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WritingBookModifyPage {
    pub book_id: ObjectId,
    pub page: u32,
    pub text: String,
}

impl Message for WritingBookModifyPage {
    const OPCODE: Opcode = Opcode::WRITING_BOOK_MODIFY_PAGE;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            book_id: ObjectId(r.u32()?),
            page: r.u32()?,
            text: r.pstring()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.book_id.0);
        w.u32(self.page);
        w.pstring(&self.text)
    }
}

/// `0x00AD Writing_BookDeletePage` and `0x00AE Writing_BookPageData` (C2S) share a layout.
macro_rules! book_page_request {
    ($(#[$m:meta])* $name:ident, $op:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
        pub struct $name {
            pub book_id: ObjectId,
            pub page: u32,
        }

        impl Message for $name {
            const OPCODE: Opcode = Opcode::$op;

            fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
                Ok(Self { book_id: ObjectId(r.u32()?), page: r.u32()? })
            }

            fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
                w.u32(self.book_id.0);
                w.u32(self.page);
                Ok(())
            }
        }
    };
}

book_page_request!(
    /// `0x00AD Writing_BookDeletePage`.
    WritingBookDeletePage,
    WRITING_BOOK_DELETE_PAGE
);
book_page_request!(
    /// `0x00AE Writing_BookPageData`.
    WritingBookPageData,
    WRITING_BOOK_PAGE_DATA
);

/// `0x00BF Writing_SetInscription` (C2S).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WritingSetInscription {
    pub object_id: ObjectId,
    pub text: String,
}

impl Message for WritingSetInscription {
    const OPCODE: Opcode = Opcode::WRITING_SET_INSCRIPTION;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            object_id: ObjectId(r.u32()?),
            text: r.pstring()?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.u32(self.object_id.0);
        w.pstring(&self.text)
    }
}

// ---------------------------------------------------------------------------------------------
// 6-8. Barber, portal storms, advocate
// ---------------------------------------------------------------------------------------------

/// The sixteen values the barber screen exchanges, in the order `0x0075` sends them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BarberSettings {
    pub base_palette: u32,
    pub head_object: u32,
    pub head_texture: u32,
    pub default_head_texture: u32,
    pub eyes_texture: u32,
    pub default_eyes_texture: u32,
    pub nose_texture: u32,
    pub default_nose_texture: u32,
    pub mouth_texture: u32,
    pub default_mouth_texture: u32,
    pub skin_palette: u32,
    pub hair_palette: u32,
    pub eyes_palette: u32,
    pub setup_id: u32,
    pub option1: i32,
    pub option2: i32,
}

impl BarberSettings {
    pub fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            base_palette: r.u32()?,
            head_object: r.u32()?,
            head_texture: r.u32()?,
            default_head_texture: r.u32()?,
            eyes_texture: r.u32()?,
            default_eyes_texture: r.u32()?,
            nose_texture: r.u32()?,
            default_nose_texture: r.u32()?,
            mouth_texture: r.u32()?,
            default_mouth_texture: r.u32()?,
            skin_palette: r.u32()?,
            hair_palette: r.u32()?,
            eyes_palette: r.u32()?,
            setup_id: r.u32()?,
            option1: r.i32()?,
            option2: r.i32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        for v in [
            self.base_palette,
            self.head_object,
            self.head_texture,
            self.default_head_texture,
            self.eyes_texture,
            self.default_eyes_texture,
            self.nose_texture,
            self.default_nose_texture,
            self.mouth_texture,
            self.default_mouth_texture,
            self.skin_palette,
            self.hair_palette,
            self.eyes_palette,
            self.setup_id,
        ] {
            w.u32(v);
        }
        w.i32(self.option1);
        w.i32(self.option2);
    }
}

/// `0x0075 Character_StartBarber` (S2C).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CharacterStartBarber(pub BarberSettings);

impl Message for CharacterStartBarber {
    const OPCODE: Opcode = Opcode::CHARACTER_START_BARBER;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self(BarberSettings::read(r)?))
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        self.0.write(w);
        Ok(())
    }
}

/// `0x0311 Character_FinishBarber` (C2S) — 0x44 bytes of payload after the `OrderedActionHeader`, i.e. the
/// sub-type dword plus the same sixteen values.
///
/// The finish-barber event allocates `OrderedActionHeader + 0x44`, writes subtype
/// `0x0311`, then copies these sixteen arguments in exactly this order. This is independent of
/// the server-to-client `0x0075` reader and fixes the former inferred-order status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CharacterFinishBarber(pub BarberSettings);

impl Message for CharacterFinishBarber {
    const OPCODE: Opcode = Opcode::CHARACTER_FINISH_BARBER;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self(BarberSettings::read(r)?))
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        self.0.write(w);
        Ok(())
    }
}

/// `0x02C9` / `0x02CA` — the two portal-storm warnings. `extent <= 0.0` resets the warning timer.
macro_rules! portal_storm_warning {
    ($(#[$m:meta])* $name:ident, $op:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Default)]
        pub struct $name {
            pub extent: f32,
        }

        impl Message for $name {
            const OPCODE: Opcode = Opcode::$op;

            fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
                Ok(Self { extent: r.f32()? })
            }

            fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
                w.f32(self.extent);
                Ok(())
            }
        }
    };
}

portal_storm_warning!(
    /// `0x02C9 Misc_PortalStormBrewing`.
    MiscPortalStormBrewing,
    MISC_PORTAL_STORM_BREWING
);
portal_storm_warning!(
    /// `0x02CA Misc_PortalStormImminent`.
    MiscPortalStormImminent,
    MISC_PORTAL_STORM_IMMINENT
);

empty_message!(
    /// `0x02CB Misc_PortalStorm`.
    MiscPortalStorm,
    MISC_PORTAL_STORM
);
empty_message!(
    /// `0x02CC Misc_PortalStormSubsided`.
    MiscPortalStormSubsided,
    MISC_PORTAL_STORM_SUBSIDED
);

/// `0x00D6 Advocate_Teleport` (C2S).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AdvocateTeleport {
    pub target_name: String,
    /// 32 bytes on the wire.
    pub destination: PositionWire,
}

impl Message for AdvocateTeleport {
    const OPCODE: Opcode = Opcode::ADVOCATE_TELEPORT;

    fn read(r: &mut Reader<'_>) -> Result<Self, MessageError> {
        Ok(Self {
            target_name: r.pstring()?,
            destination: PositionWire::read(r)?,
        })
    }

    fn write(&self, w: &mut Writer) -> Result<(), MessageError> {
        w.pstring(&self.target_name)?;
        self.destination.write(w);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{round_trip, write_blob, write_body};

    /// Oracle: `docs/networking/messages/10-trade-housing-vendor.md` §2 and `docs/CORRECTIONS.md` —
    /// `0x0200` has a third dword and `0x01FD`'s stamp is a `double`.
    #[test]
    fn trade_add_to_trade_has_a_third_dword_and_register_has_a_double_stamp() {
        let add = TradeAddToTradeRecv {
            item: ObjectId(0x5000_0001),
            side: trade_side::PARTNER,
            container_properties: 1,
        };
        let bytes = write_body(&add).unwrap();
        assert_eq!(bytes.len(), 12, "three dwords, not two");
        let _: TradeAddToTradeRecv = round_trip(&bytes);

        let reg = TradeRegisterTrade {
            initiator: ObjectId(1),
            partner: ObjectId(2),
            stamp: 1234.5,
        };
        let bytes = write_body(&reg).unwrap();
        assert_eq!(bytes.len(), 16, "two ids then an eight-byte stamp");
        assert_eq!(f64::from_le_bytes(bytes[8..16].try_into().unwrap()), 1234.5);
        let _: TradeRegisterTrade = round_trip(&bytes);
    }

    /// The C2S and S2C halves of "add to trade" are different opcodes with different bodies.
    #[test]
    fn the_two_add_to_trade_messages_are_distinct() {
        assert_eq!(TradeAddToTrade::OPCODE.0, 0x01F8);
        assert_eq!(TradeAddToTradeRecv::OPCODE.0, 0x0200);
        assert_eq!(write_body(&TradeAddToTrade::default()).unwrap().len(), 8);
        assert_eq!(
            write_body(&TradeAddToTradeRecv::default()).unwrap().len(),
            12
        );
    }

    /// The second byte-packed message in the protocol. Oracle:
    /// `docs/networking/messages/10-trade-housing-vendor.md` §4.1 and `docs/CORRECTIONS.md`.
    #[test]
    fn update_restrictions_is_byte_packed_like_the_stack_size_message() {
        let m = HouseUpdateRestrictions {
            sequence: 0x2A,
            sender: ObjectId(0x5000_1234),
            restrictions: RestrictionDb {
                version: RestrictionDb::CURRENT_VERSION,
                bitmask: 1,
                monarch_iid: ObjectId(0x5000_00AA),
                table: crate::archive::PHash::new(vec![]),
            },
        };
        let blob = write_blob(&m).unwrap();
        assert_eq!(blob[4], 0x2A, "the sequence byte is at offset 4");
        assert_eq!(
            &blob[5..9],
            &0x5000_1234u32.to_le_bytes(),
            "the object id starts at offset 5, unaligned"
        );
        let _: HouseUpdateRestrictions = round_trip(&blob[4..]);
    }

    /// Oracle: the item profile carries a 24-bit signed amount, and the weenie
    /// description present only when the flag byte is `0xFF`.
    #[test]
    fn item_profile_packs_a_signed_24_bit_amount_and_an_optional_weenie_desc() {
        for amount in [0i32, 1, -1, 0x7F_FFFF, -0x80_0000] {
            let p = ItemProfile {
                amount,
                iid: ObjectId(1),
                pwd: None,
            };
            let mut w = Writer::body();
            p.write(&mut w).unwrap();
            assert_eq!(w.len(), 8, "no weenie description");
            let bytes = w.into_inner();
            let mut r = Reader::body(&bytes);
            assert_eq!(ItemProfile::read(&mut r).unwrap(), p, "amount {amount}");
        }

        let with_pwd = ItemProfile {
            amount: 5,
            iid: ObjectId(1),
            pwd: Some(PublicWeenieDesc {
                name: "Pyreal".into(),
                // A zero DataID packs as delta 0 and reads back as the base type, which is the
                // client's own rule (its packed-data-id writer: "a null id packs as delta 0").
                // Use a real id so the round trip is exact.
                icon_id: crate::types::weeniedesc::ICON_BASE,
                ..PublicWeenieDesc::default()
            }),
        };
        let mut w = Writer::body();
        with_pwd.write(&mut w).unwrap();
        let bytes = w.into_inner();
        assert_eq!(
            bytes[3], 0xFF,
            "the flag byte says a weenie description follows"
        );
        let mut r = Reader::body(&bytes);
        assert_eq!(ItemProfile::read(&mut r).unwrap(), with_pwd);
        r.expect_exhausted().unwrap();

        // The amount must fit in 24 signed bits.
        let too_big = ItemProfile {
            amount: 0x80_0000,
            ..ItemProfile::default()
        };
        let mut w = Writer::body();
        assert!(too_big.write(&mut w).is_err());
    }

    /// Oracle: the retail page-data reader. The community catalogue has this right; see
    /// `docs/networking/messages/10-trade-housing-vendor.md` §6.
    #[test]
    fn page_data_has_a_versioned_and_an_unversioned_form() {
        // Un-versioned: the dword is textIncluded itself.
        let plain = PageData {
            author_id: ObjectId(1),
            author_name: "Bob".into(),
            author_account: "bob".into(),
            version: None,
            text_included: 1,
            ignore_author: 0,
            page_text: Some("Once upon a time".into()),
        };
        let mut w = Writer::body();
        plain.write(&mut w).unwrap();
        let bytes = w.into_inner();
        let mut r = Reader::body(&bytes);
        assert_eq!(PageData::read(&mut r).unwrap(), plain);
        r.expect_exhausted().unwrap();

        // Versioned form 2: the two flag dwords follow the marker.
        let versioned = PageData {
            version: Some(2),
            ignore_author: 1,
            ..plain.clone()
        };
        let mut w = Writer::body();
        versioned.write(&mut w).unwrap();
        let bytes = w.into_inner();
        let mut r = Reader::body(&bytes);
        assert_eq!(PageData::read(&mut r).unwrap(), versioned);
        r.expect_exhausted().unwrap();

        // A version marker other than 2 reads neither flag, so the constructor defaults stand and
        // the page text is still read.
        let other = PageData {
            version: Some(1),
            text_included: 1,
            ignore_author: 0,
            ..plain
        };
        let mut w = Writer::body();
        other.write(&mut w).unwrap();
        let bytes = w.into_inner();
        let mut r = Reader::body(&bytes);
        assert_eq!(PageData::read(&mut r).unwrap(), other);
        r.expect_exhausted().unwrap();
    }

    /// Oracle: the housing-access reader has three shapes and a trailing
    /// align that the version-0 shape does not have.
    #[test]
    fn har_has_three_shapes() {
        let current = Har {
            version: Har::CURRENT_VERSION,
            bitmask: 1,
            monarch_iid: ObjectId(0x5000_00AA),
            guest_table: PackedHash {
                table_size: 8,
                entries: vec![(
                    1u32,
                    GuestInfo {
                        item_storage_permission: 1,
                        char_name: "Bob".into(),
                    },
                )],
            },
            roommate_list: Some(vec![2, 3]),
        };
        let mut w = Writer::body();
        current.write(&mut w).unwrap();
        let bytes = w.into_inner();
        let mut r = Reader::body(&bytes);
        assert_eq!(Har::read(&mut r).unwrap(), current);
        r.expect_exhausted().unwrap();

        let old = Har {
            version: 0,
            bitmask: 1,
            monarch_iid: ObjectId(0),
            guest_table: PackedHash {
                table_size: 0,
                entries: vec![],
            },
            roommate_list: None,
        };
        let mut w = Writer::body();
        old.write(&mut w).unwrap();
        assert_eq!(
            w.len(),
            8,
            "the flag dword plus an empty table header, and no align"
        );
        let bytes = w.into_inner();
        let mut r = Reader::body(&bytes);
        assert_eq!(Har::read(&mut r).unwrap(), old);
    }

    /// Oracle: game-move data is variable-length and keyed on its move type.
    #[test]
    fn game_move_data_is_keyed_on_its_type() {
        for (d, size) in [
            (
                GameMoveData {
                    move_type: 1,
                    player: ObjectId(1),
                    ..GameMoveData::default()
                },
                8,
            ),
            (
                GameMoveData {
                    move_type: GameMoveData::GRID,
                    player: ObjectId(1),
                    from: Some((1, 2)),
                    ..GameMoveData::default()
                },
                16,
            ),
            (
                GameMoveData {
                    move_type: GameMoveData::FROM_TO,
                    player: ObjectId(1),
                    from: Some((1, 2)),
                    to: Some((3, 4)),
                    ..GameMoveData::default()
                },
                24,
            ),
            (
                GameMoveData {
                    move_type: GameMoveData::SELECTED_PIECE,
                    player: ObjectId(1),
                    piece_index: Some(5),
                    ..GameMoveData::default()
                },
                12,
            ),
        ] {
            let mut w = Writer::body();
            d.write(&mut w);
            assert_eq!(w.len(), size, "type {}", d.move_type);
            let bytes = w.into_inner();
            let mut r = Reader::body(&bytes);
            assert_eq!(GameMoveData::read(&mut r).unwrap(), d);
            r.expect_exhausted().unwrap();
        }
    }

    #[test]
    fn the_rest_of_the_family_round_trips() {
        let _: TradeAcceptTradeRequest = round_trip(
            &write_body(&TradeAcceptTradeRequest(Trade {
                self_list: vec![ContentProfile {
                    iid: ObjectId(1),
                    container_properties: 0,
                }],
                partner_list: vec![],
                partner: ObjectId(2),
                stamp: 5.0,
                status: 1,
                initiator: 1,
                accepted: 0,
                partner_accepted: 0,
            }))
            .unwrap(),
        );
        let _: TradeOpenTradeNegotiations = round_trip(
            &write_body(&TradeOpenTradeNegotiations {
                partner: ObjectId(1),
            })
            .unwrap(),
        );
        let _: TradeRemoveFromTrade = round_trip(
            &write_body(&TradeRemoveFromTrade {
                item: ObjectId(1),
                side: trade_side::SELF,
            })
            .unwrap(),
        );
        let _: TradeTradeFailure = round_trip(
            &write_body(&TradeTradeFailure {
                item: ObjectId(1),
                reason: 2,
            })
            .unwrap(),
        );
        let _: TradeCloseTrade = round_trip(&write_body(&TradeCloseTrade { reason: 1 }).unwrap());
        assert_eq!(write_body(&TradeClearTradeAcceptance).unwrap().len(), 0);

        let profile = VendorProfile {
            item_types: 0x101,
            min_value: 0,
            max_value: 10_000,
            magic: 1,
            buy_price: 1.2,
            sell_price: 0.8,
            trade_id: 0,
            trade_num: 0,
            trade_name: String::new(),
        };
        let _: VendorInfo = round_trip(
            &write_body(&VendorInfo {
                merchant_id: ObjectId(1),
                profile: profile.clone(),
                items: vec![ItemProfile {
                    amount: 1,
                    iid: ObjectId(2),
                    pwd: None,
                }],
            })
            .unwrap(),
        );
        let _: VendorBuy = round_trip(
            &write_body(&VendorBuy {
                vendor_id: ObjectId(1),
                items: vec![],
                alternate_currency_id: 0,
            })
            .unwrap(),
        );
        let _: VendorSell = round_trip(
            &write_body(&VendorSell {
                vendor_id: ObjectId(1),
                items: vec![],
            })
            .unwrap(),
        );
        drop(profile);

        let payment = HousePayment {
            wcid: 273,
            num: 1,
            paid: 0,
            name: "Pyreal".into(),
            plural_name: "Pyreals".into(),
        };
        let _: HouseProfileMessage = round_trip(
            &write_body(&HouseProfileMessage {
                covenant_crystal: ObjectId(1),
                profile: HouseProfile {
                    id: ObjectId(2),
                    owner: ObjectId(3),
                    name: "Cottage".into(),
                    bitmask: 1,
                    buy: vec![payment.clone()],
                    rent: vec![payment.clone()],
                    min_level: 0,
                    max_level: 0,
                    min_alleg_rank: 0,
                    max_alleg_rank: 0,
                    maintenance_free: 0,
                    house_type: 1,
                },
            })
            .unwrap(),
        );
        let _: HouseDataMessage = round_trip(
            &write_body(&HouseDataMessage {
                buy_time: 0,
                rent_time: 0,
                buy: vec![payment.clone()],
                rent: vec![payment.clone()],
                position: PositionWire::default(),
                house_type: 1,
                maintenance_free: 0,
            })
            .unwrap(),
        );
        let _: HouseUpdateRentPayment = round_trip(
            &write_body(&HouseUpdateRentPayment {
                payments: vec![payment],
            })
            .unwrap(),
        );
        let _: HouseHouseStatus =
            round_trip(&write_body(&HouseHouseStatus { notice_type: 1 }).unwrap());
        let _: HouseHouseTransaction =
            round_trip(&write_body(&HouseHouseTransaction { notice_type: 1 }).unwrap());
        let _: HouseUpdateRentTime =
            round_trip(&write_body(&HouseUpdateRentTime { rent_time: 100 }).unwrap());
        let _: HouseAvailableHouses = round_trip(
            &write_body(&HouseAvailableHouses {
                house_type: 1,
                landcells: vec![0x00A9_0000],
                num_houses: 1,
            })
            .unwrap(),
        );
        let _: HouseBuyHouse = round_trip(
            &write_body(&HouseBuyHouse {
                slumlord: ObjectId(1),
                items: vec![ObjectId(2)],
            })
            .unwrap(),
        );
        let _: HouseChangeStoragePermission = round_trip(
            &write_body(&HouseChangeStoragePermission {
                name: "Bob".into(),
                has_permission: 1,
            })
            .unwrap(),
        );
        let _: HouseQueryLord = round_trip(
            &write_body(&HouseQueryLord {
                target: ObjectId(1),
            })
            .unwrap(),
        );
        assert_eq!(write_body(&HouseQueryHouse).unwrap().len(), 0);

        let _: GameJoin = round_trip(
            &write_body(&GameJoin {
                game_id: 1,
                which_team: 0,
            })
            .unwrap(),
        );
        let _: GameMove = round_trip(
            &write_body(&GameMove {
                x_from: 1,
                y_from: 2,
                x_to: 3,
                y_to: 4,
            })
            .unwrap(),
        );
        let _: GameJoinGameResponse = round_trip(
            &write_body(&GameJoinGameResponse {
                game_id: 1,
                team: -1,
            })
            .unwrap(),
        );
        let _: GameOpponentTurn = round_trip(
            &write_body(&GameOpponentTurn {
                game_id: 1,
                team: 0,
                move_data: GameMoveData {
                    move_type: GameMoveData::FROM_TO,
                    player: ObjectId(1),
                    from: Some((1, 2)),
                    to: Some((3, 4)),
                    piece_index: None,
                },
            })
            .unwrap(),
        );
        let _: GameOpponentStalemateState = round_trip(
            &write_body(&GameOpponentStalemateState {
                game_id: 1,
                team: 0,
                on: 1,
            })
            .unwrap(),
        );
        let _: GameGameOver = round_trip(
            &write_body(&GameGameOver {
                game_id: 1,
                team_winner: 0,
            })
            .unwrap(),
        );

        let _: WritingBookOpen = round_trip(
            &write_body(&WritingBookOpen {
                book_id: ObjectId(1),
                max_num_pages: 10,
                pages: PageDataList {
                    max_num_pages: 10,
                    max_num_chars_per_page: 1000,
                    pages: vec![PageData::default()],
                },
                inscription: "For Bob".into(),
                scribe_id: ObjectId(2),
                scribe_name: "Alice".into(),
            })
            .unwrap(),
        );
        let _: WritingBookAddPageResponse = round_trip(
            &write_body(&WritingBookAddPageResponse {
                book_id: ObjectId(1),
                page_number: 2,
                success: 1,
            })
            .unwrap(),
        );
        let _: BookPageDataResponse = round_trip(
            &write_body(&BookPageDataResponse {
                object_id: ObjectId(1),
                page: 0,
                data: PageData::default(),
            })
            .unwrap(),
        );
        let _: WritingBookModifyPage = round_trip(
            &write_body(&WritingBookModifyPage {
                book_id: ObjectId(1),
                page: 0,
                text: "text".into(),
            })
            .unwrap(),
        );
        let _: WritingSetInscription = round_trip(
            &write_body(&WritingSetInscription {
                object_id: ObjectId(1),
                text: "mine".into(),
            })
            .unwrap(),
        );

        let barber = BarberSettings {
            base_palette: 1,
            head_object: 2,
            head_texture: 3,
            default_head_texture: 4,
            eyes_texture: 5,
            default_eyes_texture: 6,
            nose_texture: 7,
            default_nose_texture: 8,
            mouth_texture: 9,
            default_mouth_texture: 10,
            skin_palette: 11,
            hair_palette: 12,
            eyes_palette: 13,
            setup_id: 14,
            option1: -15,
            option2: -16,
        };
        let start = write_body(&CharacterStartBarber(barber)).unwrap();
        let expected: Vec<u8> = (1_u32..=14)
            .chain([(-15_i32) as u32, (-16_i32) as u32])
            .flat_map(u32::to_le_bytes)
            .collect();
        assert_eq!(
            start, expected,
            "sixteen literal dwords in native argument order"
        );
        let _: CharacterStartBarber = round_trip(&start);
        let finish = write_body(&CharacterFinishBarber(barber)).unwrap();
        assert_eq!(
            finish, expected,
            "FinishBarber uses the independently verified sender order"
        );
        assert_eq!(finish.len() + 4, 0x44, "0x44 including the sub-type dword");
        let _: CharacterFinishBarber = round_trip(&finish);

        let _: MiscPortalStormBrewing =
            round_trip(&write_body(&MiscPortalStormBrewing { extent: 0.5 }).unwrap());
        assert_eq!(write_body(&MiscPortalStorm).unwrap().len(), 0);
        let _: AdvocateTeleport = round_trip(
            &write_body(&AdvocateTeleport {
                target_name: "Bob".into(),
                destination: PositionWire::default(),
            })
            .unwrap(),
        );
    }

    /// House data reads type and maintenance before the two price lists.
    #[test]
    fn house_data_reads_type_and_maintenance_before_the_two_price_lists() {
        let mut b: Vec<u8> = Vec::new();
        b.extend_from_slice(&1_600_000_000_i32.to_le_bytes()); // BuyTime
        b.extend_from_slice(&1_600_100_000_i32.to_le_bytes()); // RentTime
        b.extend_from_slice(&2_u32.to_le_bytes()); // Type — villa
        b.extend_from_slice(&0_u32.to_le_bytes()); // MaintenanceFree
                                                   // Buy: one payment. Num, Paid, WeenieID, Name, PluralName.
        b.extend_from_slice(&1_u32.to_le_bytes());
        b.extend_from_slice(&30_000_i32.to_le_bytes());
        b.extend_from_slice(&30_000_i32.to_le_bytes());
        b.extend_from_slice(&273_u32.to_le_bytes());
        for s in [&b"Pyreal"[..], &b"Pyreals"[..]] {
            b.extend_from_slice(&u16::try_from(s.len()).unwrap().to_le_bytes());
            b.extend_from_slice(s);
            while !b.len().is_multiple_of(4) {
                b.push(0);
            }
        }
        // Rent: empty list.
        b.extend_from_slice(&0_u32.to_le_bytes());
        // Position: cell then origin then quaternion.
        b.extend_from_slice(&0x7203_0110_u32.to_le_bytes());
        for f in [12.0_f32, 34.0, 0.0, 1.0, 0.0, 0.0, 0.0] {
            b.extend_from_slice(&f.to_le_bytes());
        }

        let m: HouseDataMessage = crate::read_body(&b).expect("the shard's layout decodes");
        assert_eq!(m.buy_time, 1_600_000_000);
        assert_eq!(m.rent_time, 1_600_100_000);
        assert_eq!(m.house_type, 2, "`+74h` is the third dword, not the last");
        assert_eq!(m.maintenance_free, 0);
        assert_eq!(m.buy.len(), 1);
        assert_eq!(m.buy[0].num, 30_000, "num is first, wcid third");
        assert_eq!(m.buy[0].paid, 30_000);
        assert_eq!(m.buy[0].wcid, 273);
        assert_eq!(m.buy[0].name, "Pyreal");
        assert_eq!(m.buy[0].plural_name, "Pyreals");
        assert!(m.rent.is_empty());
        assert_eq!(m.position.objcell_id, 0x7203_0110);
        assert_eq!(m.position.frame.origin.x, 12.0);
        let _: HouseDataMessage = round_trip(&write_body(&m).unwrap());
    }

    /// House profile reads nine dwords before the name and the two lists.
    #[test]
    fn house_profile_reads_nine_dwords_before_the_name_and_the_two_lists() {
        let mut b: Vec<u8> = Vec::new();
        b.extend_from_slice(&0x0000_0001_u32.to_le_bytes()); // covenant crystal (0x021D's own dword)
        b.extend_from_slice(&0x7000_0123_u32.to_le_bytes()); // DwellingID
        b.extend_from_slice(&0x5000_0456_u32.to_le_bytes()); // OwnerID
        b.extend_from_slice(&1_u32.to_le_bytes()); // Bitmask — Active
        b.extend_from_slice(&(-1_i32).to_le_bytes()); // MinLevel
        b.extend_from_slice(&50_i32.to_le_bytes()); // MaxLevel
        b.extend_from_slice(&(-1_i32).to_le_bytes()); // MinAllegRank
        b.extend_from_slice(&(-1_i32).to_le_bytes()); // MaxAllegRank
        b.extend_from_slice(&0_u32.to_le_bytes()); // MaintenanceFree
        b.extend_from_slice(&1_u32.to_le_bytes()); // Type — cottage
        for s in [&b"Bob"[..]] {
            b.extend_from_slice(&u16::try_from(s.len()).unwrap().to_le_bytes());
            b.extend_from_slice(s);
            while !b.len().is_multiple_of(4) {
                b.push(0);
            }
        }
        // Buy: one payment, in HousePayment's own order — Num, Paid, WeenieID, Name, PluralName.
        b.extend_from_slice(&1_u32.to_le_bytes());
        b.extend_from_slice(&20_000_i32.to_le_bytes());
        b.extend_from_slice(&0_i32.to_le_bytes());
        b.extend_from_slice(&273_u32.to_le_bytes());
        for s in [&b"Pyreal"[..], &b"Pyreals"[..]] {
            b.extend_from_slice(&u16::try_from(s.len()).unwrap().to_le_bytes());
            b.extend_from_slice(s);
            while !b.len().is_multiple_of(4) {
                b.push(0);
            }
        }
        // Rent: empty list.
        b.extend_from_slice(&0_u32.to_le_bytes());

        let m: HouseProfileMessage = crate::read_body(&b).expect("the shard's layout decodes");
        assert_eq!(m.covenant_crystal.0, 1);
        let p = &m.profile;
        assert_eq!(p.id.0, 0x7000_0123);
        assert_eq!(p.owner.0, 0x5000_0456);
        assert_eq!(p.bitmask, 1, "`+10h` is the third dword, not the fourth");
        assert_eq!(p.min_level, -1);
        assert_eq!(p.max_level, 50);
        assert_eq!(p.min_alleg_rank, -1);
        assert_eq!(p.max_alleg_rank, -1);
        assert_eq!(p.maintenance_free, 0);
        assert_eq!(
            p.house_type, 1,
            "`+48h` is the ninth dword, and the last one"
        );
        assert_eq!(
            p.name, "Bob",
            "the name comes after all nine dwords, not third"
        );
        assert_eq!(p.buy.len(), 1);
        assert_eq!(p.buy[0].num, 20_000);
        assert_eq!(p.buy[0].paid, 0);
        assert_eq!(p.buy[0].wcid, 273);
        assert_eq!(p.buy[0].plural_name, "Pyreals");
        assert!(p.rent.is_empty());
        let _: HouseProfileMessage = round_trip(&write_body(&m).unwrap());
    }
}
