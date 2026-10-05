//! Housing: `HouseProfile`/`HouseData`, the rent and purchase periods, hooks and `RestrictionDB`.
//!
//! Housing messages normally forward data to the panels. Restriction updates additionally
//! reject invalid targets and stale timestamps before storing the new permissions.

use dereth_primitives::ObjectId;
use std::collections::BTreeMap;

mod payments;
pub use payments::{PaymentEffect, PaymentLists};

/// `HouseType`: the client only ever tests `type == 4`.
pub mod house_type {
    pub const UNDEF: u32 = 0;
    pub const COTTAGE: u32 = 1;
    pub const VILLA: u32 = 2;
    pub const MANSION: u32 = 3;
    /// The only value the client branches on.
    pub const APARTMENT: u32 = 4;
}

/// Flags attached to a dwelling's terms.
pub mod house_bitmask {
    pub const UNDEF: u32 = 0;
    pub const ACTIVE: u32 = 1;
    /// Only a monarch may buy it â€” allegiance housing.
    pub const REQUIRES_MONARCH: u32 = 2;
}

/// `HouseOp`.
pub const BUY_HOUSE: u32 = 1;
pub const RENT_HOUSE: u32 = 2;

/// The Pyreal coin class used by housing payment lists.
///
/// It is **Pyreal**, weenie class **273**: ACE's `HouseProfile.SetPaidItems` maps every trade
/// note to `wcid = 273`, and the captured housing fixture carries `11010000` (273) on
/// the Pyreal line of both the buy and the rent list of a real villa. The client resolves it from
/// the dat rather than hard-coding it; this build has no reader for that enum group, so the
/// number is taken from the two oracles that agree and the lookup is named rather than faked.
pub const COINSTACK_WCID: u32 = 273;

/// The UI notice channel used by housing payments and their partial-stack split arm.
pub const SLUMLORD_NOTICE_CHANNEL: u32 = 0x1A;
/// The drop handler's refusal, after placing the stack in a container refuses.
pub const CANNOT_SPLIT_FOR_DWELLING: &str = "Cannot split the stack for dwelling costs";

/// The client's formatted string, with the name of type 2 substituted.
#[must_use]
pub fn splitting_before_housing(name: &str) -> String {
    format!("Splitting the {name} before adding to housing panel")
}

/// Valid hook locations, represented as a mask.
pub mod hook_type_enum {
    pub const UNDEF: u32 = 0;
    pub const FLOOR: u32 = 1;
    pub const WALL: u32 = 2;
    pub const CEILING: u32 = 4;
    pub const YARD: u32 = 8;
    pub const ROOF: u32 = 16;
}

/// `HookType` â€” the *visual* effect a hook applies to what is placed on it.
pub mod hook_type {
    pub const SCALING: u32 = 0;
    pub const TRANSLUCENCY: u32 = 1;
    pub const PART_TRANSLUCENCY: u32 = 2;
    pub const LUMINOSITY: u32 = 3;
    pub const DIFFUSION: u32 = 4;
    pub const PART_LUMINOSITY: u32 = 5;
    pub const PART_DIFFUSION: u32 = 6;
    pub const CALL_PES: u32 = 7;
}

/// `HousePanelTextColor`.
pub mod panel_color {
    pub const NORMAL: u32 = 0;
    pub const RENT_PAID: u32 = 1;
    pub const RENT_NOT_PAID: u32 = 2;
}

/// Flags in the house access-restriction record.
pub mod rdb_bitmask {
    pub const UNDEF: u32 = 0;
    pub const OPEN_HOUSE: u32 = 1;
}

/// Behavior: `0x278D00`, thirty days in seconds.
pub const PURCHASE_WAIT_SECONDS: i64 = 2_592_000;

/// Player integer quality `0xC7`: the house-purchase timestamp used by the waiting-period check.
pub const HOUSE_PURCHASE_TIMESTAMP: u32 = 0xC7;
/// Behavior: apartments rent for **90** days, everything else 30.
pub const RENT_PERIOD_APARTMENT: f64 = 7_776_000.0;
pub const RENT_PERIOD_DEFAULT: f64 = 2_592_000.0;

/// Whether the purchase waiting period has expired. The comparison is strictly greater.
#[must_use]
pub fn has_purchase_wait_period_expired(now: i64, buy_time: i64) -> bool {
    now - buy_time > PURCHASE_WAIT_SECONDS
}

/// Maintenance period in seconds for the given dwelling type.
#[must_use]
pub fn rent_period(t: u32) -> f64 {
    if t == house_type::APARTMENT {
        RENT_PERIOD_APARTMENT
    } else {
        RENT_PERIOD_DEFAULT
    }
}

/// The rent period in whole days, which is what the warning message prints.
#[must_use]
pub fn rent_period_days(t: u32) -> i64 {
    #[allow(clippy::cast_possible_truncation)] // both constants are exact whole days
    {
        dereth_primitives::num::to_i32_f64(rent_period(t) / 86_400.0) as i64
    }
}

/// `HousePayment` (0x18) â€” one line of a price.
///
/// `num` and `paid` are **signed** in retail (compared as signed), and ACE writes them as
/// `int`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HousePayment {
    /// The weenie class of the required item.
    pub wcid: u32,
    pub num: i32,
    pub paid: i32,
    pub name: String,
    pub pname: String,
}

impl HousePayment {
    /// Behavior: the name for `count` items.
    ///
    /// The function is three arms, not two:
    ///
    /// * `num == 1`: use the singular name.
    /// * Otherwise, use the server's plural name if it is nonempty.
    /// * Otherwise, derive a plural from the singular name, with the terminator behavior below.
    ///
    /// Returning `pname + "s"` for every `num != 1` would mangle the plural a shard *did* send
    /// (ACE's `wo.GetPluralName()` is never empty, so the second arm is the one that runs
    /// against an ACE server: "Pyreals" would become "Pyrealss").
    ///
    /// **The `"es"` arm is dead in retail and is reproduced as dead.**
    /// The stored string length counts the terminator: callers append `len - 1` characters and test
    /// `len != 1` for "non-empty" â€” and the character retail tests as the last letter is the one
    /// at index `len - 1`, which is that terminator, on both arms. So the
    /// comparison is always against `'\0'` and the suffix is always `"s"`. Writing the `'s'`/`'x'`
    /// test here would be *more* than retail does, and retail is the specification.
    #[must_use]
    pub fn get_name(&self, count: i32) -> String {
        if count == 1 {
            self.name.clone()
        } else if !self.pname.is_empty() {
            self.pname.clone()
        } else {
            format!("{}s", self.name)
        }
    }

    /// The row's text â€” `"<num> <name>"`.
    #[must_use]
    pub fn compose_text(&self) -> String {
        format!("{} {}", self.num, self.get_name(self.num))
    }

    /// The row's text with the paid count â€” `"<paid>/<num> <name>"`.
    #[must_use]
    pub fn compose_text2(&self) -> String {
        format!("{}/{} {}", self.paid, self.num, self.get_name(self.num))
    }
}

/// `HousePaymentList` â€” a `PackableList<HousePayment>`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HousePaymentList(pub Vec<HousePayment>);

impl HousePaymentList {
    /// Whether every row is paid in full.
    #[must_use]
    pub fn is_paid_in_full(&self) -> bool {
        self.0.iter().all(|p| p.paid >= p.num)
    }

    /// Whether the list still needs `wcid`.
    ///
    /// Retail does not sum `num - paid` over every matching row. It walks and
    /// `break`s at the **first** row whose wcid matches, and then answers a comparison:
    ///
    /// It returns `paid < num`, a **predicate represented as an integer**. Its consumer tests
    /// `result != 0`. For ACE's lists, which
    /// carry one row per weenie class, the two readings agree on `!= 0` and disagree on the
    /// value.
    ///
    /// **The trade-note arm.** When the row is the coin row
    /// (Pyreal, class **273**) and the dropped item is a
    /// trade note, the answer is `value <= num - paid` where `value` is the note's face value out
    /// of a **reverse** class-id lookup in a `0x27xxxxxx` mapper
    /// this crate cannot reach. `trade_note_value` is that answer, passed in; `None` means "not a
    /// trade note"; the host supplies the mapper.
    #[must_use]
    pub fn needs_more(&self, wcid: u32, trade_note_value: Option<i32>) -> i32 {
        for p in &self.0 {
            if p.wcid == COINSTACK_WCID {
                if let Some(v) = trade_note_value {
                    if p.paid >= p.num {
                        return 0;
                    }
                    return i32::from(v <= p.num - p.paid);
                }
            }
            if p.wcid == wcid {
                return i32::from(p.paid < p.num);
            }
        }
        0
    }

    /// Pay `amount` of `wcid` into the list.
    ///
    /// Retail does not spread `amount` across every matching row: it takes the **first**
    /// matching row and that row only.
    ///
    /// An amount below 1 pays nothing and answers 0, as does a row already paid in full; an amount
    /// that fits (`paid + amount <= num`) is added and answered whole; otherwise the row is filled
    /// to `num` and the answer is the `num - paid` that filled it.
    ///
    /// The payment call is a thin wrapper over it â€”
    /// `attempt_to_pay(payment.wcid, payment.num) != 0` â€” and that boolean is what
    /// the payment panel tests before it puts the row in the window, so an
    /// item the list does not want is **not added at all**.
    ///
    /// The trade-note arm uses the dropped stack count and mapped face value. It credits only
    /// whole notes which fit in the remaining Pyreal requirement and returns the number of notes
    /// consumed (not the number of Pyreals credited).
    pub fn attempt_to_pay(&mut self, wcid: u32, amount: i32, trade_note_value: Option<i32>) -> i32 {
        for p in &mut self.0 {
            if p.wcid == COINSTACK_WCID {
                if let Some(value) = trade_note_value {
                    // Native takes min(stack count, floor(remainder / face value)) whole notes.
                    if amount < 1 || value < 1 || p.paid >= p.num {
                        return 0;
                    }
                    let notes = amount.min((p.num - p.paid) / value);
                    if notes < 1 {
                        return 0;
                    }
                    p.paid += notes * value;
                    return notes;
                }
            }
            if p.wcid == wcid {
                if amount < 1 || p.paid >= p.num {
                    return 0;
                }
                if p.paid + amount <= p.num {
                    p.paid += amount;
                    return amount;
                }
                let d = p.num - p.paid;
                p.paid = p.num;
                return d;
            }
        }
        0
    }

    /// Pay one payment â€” `attempt_to_pay(payment.wcid, payment.num) != 0`.
    pub fn pay(&mut self, wcid: u32, amount: i32, trade_note_value: Option<i32>) -> bool {
        self.attempt_to_pay(wcid, amount, trade_note_value) != 0
    }

    /// Clear every row's paid count.
    pub fn clear_payment(&mut self) {
        for p in &mut self.0 {
            p.paid = 0;
        }
    }

    /// The list's text â€” the rows' texts, `", "`-joined.
    #[must_use]
    pub fn compose_text(&self) -> String {
        self.0
            .iter()
            .map(HousePayment::compose_text)
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The list's text with paid counts, `", "`-joined.
    #[must_use]
    pub fn compose_text2(&self) -> String {
        self.0
            .iter()
            .map(HousePayment::compose_text2)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// `HouseProfile` (0x4C) â€” the dwelling's terms.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HouseProfile {
    pub id: ObjectId,
    /// 0 when unowned.
    pub owner: ObjectId,
    pub name: String,
    pub bitmask: u32,
    pub buy: HousePaymentList,
    pub rent: HousePaymentList,
    /// **`i32`, and `-1` means "no requirement"** â€” retail's are signed 32-bit, ACE's are
    /// `int`, and its `HouseProfile()` constructor sets all four of these to `-1`. That is a
    /// distinction with a difference: a
    /// recorded villa carries `min_level 35` and
    /// `ffffffff ffffffff ffffffff` for the other three, and a `u32` reading of those is
    /// 4,294,967,295 rather than *"none"*.
    pub min_level: i32,
    pub max_level: i32,
    pub min_alleg_rank: i32,
    pub max_alleg_rank: i32,
    pub maintenance_free: i32,
    pub house_type: u32,
}

/// Which of `HouseProfile`'s two payment lists an operation is about; the six list operations
/// answer **0 / false / empty** for the undefined one.
pub use dereth_client_contract::panels::slumlord::HouseOp;

impl HouseProfile {
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.bitmask & house_bitmask::ACTIVE != 0
    }

    /// Whether the *buy* list is paid in full.
    #[must_use]
    pub fn is_paid_in_full(&self) -> bool {
        self.buy.is_paid_in_full()
    }

    /// The buy/rent list selection the six op-dispatchers share.
    #[must_use]
    pub fn list(&self, op: HouseOp) -> Option<&HousePaymentList> {
        match op {
            HouseOp::Undef => None,
            HouseOp::Buy => Some(&self.buy),
            HouseOp::Rent => Some(&self.rent),
        }
    }

    /// The same, for paying and removing a payment.
    pub fn list_mut(&mut self, op: HouseOp) -> Option<&mut HousePaymentList> {
        match op {
            HouseOp::Undef => None,
            HouseOp::Buy => Some(&mut self.buy),
            HouseOp::Rent => Some(&mut self.rent),
        }
    }

    /// Whether the selected payment list is paid in full.
    #[must_use]
    pub fn op_is_paid_in_full(&self, op: HouseOp) -> bool {
        self.list(op).is_some_and(HousePaymentList::is_paid_in_full)
    }

    /// Whether the selected list can accept this item or trade note.
    #[must_use]
    pub fn needs_more(&self, op: HouseOp, wcid: u32, trade_note_value: Option<i32>) -> i32 {
        self.list(op)
            .map_or(0, |l| l.needs_more(wcid, trade_note_value))
    }

    /// Apply a payment to the selected list.
    pub fn pay(&mut self, op: HouseOp, wcid: u32, amount: i32, note: Option<i32>) -> bool {
        self.list_mut(op).is_some_and(|l| l.pay(wcid, amount, note))
    }

    /// Format the **buy** requirements so the purchase panel shows `"<num>
    /// <name>"` with no paid count.
    #[must_use]
    pub fn compose_text(&self, op: HouseOp) -> String {
        self.list(op)
            .map(HousePaymentList::compose_text)
            .unwrap_or_default()
    }

    /// Format the **rent**
    /// requirements with *this* one, so the maintenance panel shows `"<paid>/<num> <name>"`.
    /// The asymmetry is retail's and is the whole visible difference between the two tabs' top
    /// lines.
    #[must_use]
    pub fn compose_text2(&self, op: HouseOp) -> String {
        self.list(op)
            .map(HousePaymentList::compose_text2)
            .unwrap_or_default()
    }

    /// Built from the decoded `0x021D` message.
    ///
    /// Validated byte for byte against a recorded `0x021D`: 260 bytes, dwelling
    /// `0x0592`, owner 0, bitmask 1, min level 35, the three `-1`s, type **2 (villa)**, an empty
    /// owner name, three buy lines and two rent lines.
    #[must_use]
    pub fn from_wire(m: &dereth_protocol::trade::HouseProfile) -> Self {
        Self {
            id: m.id,
            owner: m.owner,
            name: m.name.clone(),
            bitmask: m.bitmask,
            buy: HousePaymentList::from_wire(&m.buy),
            rent: HousePaymentList::from_wire(&m.rent),
            min_level: m.min_level,
            max_level: m.max_level,
            min_alleg_rank: m.min_alleg_rank,
            max_alleg_rank: m.max_alleg_rank,
            maintenance_free: m.maintenance_free,
            house_type: m.house_type,
        }
    }
}

/// The local player's own house.
///
/// `position` reads through the stored house location. The two timestamps are `i32`: they are
/// 32-bit `long`s in retail and ACE writes them as `uint`, and the one arithmetic the client does
/// on either (`buy_time + 0x278D00`) is a 32-bit `add`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HouseData {
    /// Unix time.
    pub buy_time: i32,
    /// Unix time of the current period's start.
    pub rent_time: i32,
    pub buy: HousePaymentList,
    pub rent: HousePaymentList,
    pub house_type: u32,
    pub maintenance_free: i32,
    /// Stored house position. Location display asks the host to validate it, obtain its outside
    /// cell and convert that cell to landscape coordinates.
    pub position: dereth_protocol::types::PositionWire,
}

impl HousePaymentList {
    /// Convert the wire payment list shared by `0x0225` and `0x0228`.
    /// Both initial house data and subsequent rent-payment updates use the same conversion.
    #[must_use]
    pub fn from_wire(v: &[dereth_protocol::trade::HousePayment]) -> Self {
        Self(
            v.iter()
                .map(|p| HousePayment {
                    wcid: p.wcid,
                    num: p.num,
                    paid: p.paid,
                    name: p.name.clone(),
                    pname: p.plural_name.clone(),
                })
                .collect(),
        )
    }
}

impl HouseData {
    /// Build house data from the decoded `0x0225` message.
    #[must_use]
    pub fn from_wire(m: &dereth_protocol::trade::HouseDataMessage) -> Self {
        Self {
            buy_time: m.buy_time,
            rent_time: m.rent_time,
            buy: HousePaymentList::from_wire(&m.buy),
            rent: HousePaymentList::from_wire(&m.rent),
            house_type: m.house_type,
            maintenance_free: m.maintenance_free,
            position: m.position,
        }
    }

    /// The client's first line â€” *"This maintenance period ends: "*.
    ///
    /// Add the rent timestamp to the `double`
    /// the period function returned, then convert to an integer.
    #[must_use]
    pub fn maintenance_period_end(&self) -> i64 {
        dereth_primitives::num::to_i32_f64(rent_period(self.house_type) + f64::from(self.rent_time))
            .into()
    }

    /// The client's second line â€” *"Maintenance is next due: "*.
    ///
    /// The **period is doubled** when nothing more is owed. This is the
    /// arm taken when `maintenance_free != 0` **or** the rent is paid in full.
    #[must_use]
    pub fn maintenance_next_due(&self) -> i64 {
        let mut period = rent_period(self.house_type);
        if self.maintenance_free != 0 || self.rent.is_paid_in_full() {
            period += period;
        }
        dereth_primitives::num::to_i32_f64(period + f64::from(self.rent_time)).into()
    }

    /// The warning and rent-time display share this test: `maintenance_free`
    /// is zero **and** the rent list is not yet paid in full.
    #[must_use]
    pub fn rent_is_owed(&self) -> bool {
        self.maintenance_free == 0 && !self.rent.is_paid_in_full()
    }
}

/// `GuestInfo` (12 bytes).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GuestInfo {
    pub item_storage_permission: i32,
    pub char_name: String,
}

/// House Access Restrictions (0x34).
#[derive(Debug, Clone, Default)]
pub struct Har {
    pub bitmask: u32,
    /// The allegiance whose members are guests; 0 if none.
    pub monarch_iid: ObjectId,
    pub guests: BTreeMap<ObjectId, GuestInfo>,
    /// No known client consumer; stored opaquely.
    pub roommate_list: Vec<ObjectId>,
}

/// `RestrictionDB` â€” the compact form attached to **every** object in the house.
///
/// It is what makes a door refuse to open for a non-guest. The protocol crate owns the wire form
/// ([`dereth_protocol::types::RestrictionDb`]); this is the evaluation.
#[derive(Debug, Clone, Default)]
pub struct Restrictions {
    pub bitmask: u32,
    pub monarch_iid: ObjectId,
    /// `characterId â†’ permission bits`.
    pub table: BTreeMap<ObjectId, u32>,
}

impl Restrictions {
    /// Set whether the house is open to everyone.
    pub fn set_open_house(&mut self, on: bool) {
        if on {
            self.bitmask |= rdb_bitmask::OPEN_HOUSE;
        } else {
            self.bitmask &= !rdb_bitmask::OPEN_HOUSE;
        }
    }

    /// Test the mover's individual and allegiance permissions before entering a dwelling.
    #[must_use]
    pub fn is_allowed_in(&self, mover: ObjectId, mover_monarch: ObjectId) -> bool {
        if self.bitmask & rdb_bitmask::OPEN_HOUSE != 0 {
            return true;
        }
        if self.table.contains_key(&mover) {
            return true;
        }
        self.monarch_iid.0 != 0 && self.monarch_iid == mover_monarch
    }
}

/// Behavior: `"N/A"` for zero, otherwise the locale short date/time.
///
/// The `strftime("%c")` half is `dereth_ui_screens::ctime::strftime_c`'s (the
/// locale is fixed to English at initialization);
/// this returns the sentinel and leaves formatting to the caller.
#[must_use]
pub fn convert_time(unix_time: i64) -> Option<i64> {
    if unix_time == 0 {
        None
    } else {
        Some(unix_time)
    }
}

/// The display sentinel for a zero timestamp.
pub const TIME_NA: &str = "N/A";

/// Compose the unpaid-maintenance warning.
///
/// **There is no `deadline` parameter; retail does not have one.** The function is
/// one `set` and three `append`s and there is no fourth string:
///
/// The three literals surround the whole-day count; no deadline value is interpolated.
/// Note the **two spaces** after `"Warning!"` and after `"period."`.
#[must_use]
pub fn construct_rent_warning_message(period_days: i64) -> String {
    format!(
        "Warning!  You have not paid your maintenance costs for the last {period_days} day \
         maintenance period.  Please pay these costs by this deadline or you will lose your \
         house, and all your items within it."
    )
}

/// The paid-maintenance message, displayed in colour
/// [`panel_color::RENT_PAID`].
pub const MAINTENANCE_ALREADY_PAID: &str = "The maintenance has already been paid for this \
                                            period. You may not prepay next period's maintenance.";

/// The apartment exemption line the rent-period text's caller appends.
pub const APARTMENT_EXEMPTION: &str = ". This restriction does not apply to apartments.";

impl crate::world::World {
    /// The update-restrictions handler, `(ts, objId, rdb)` â€” the only housing
    /// handler with logic.
    ///
    /// Four gates in order: a zero id is ignored, the **local player is never given restrictions**,
    /// an unknown object is ignored, and a stale timestamp is rejected by the per-object
    /// house-restriction timestamp byte.
    pub fn recv_update_restrictions(
        &mut self,
        timestamp: u8,
        object: ObjectId,
        rdb: dereth_protocol::types::RestrictionDb,
    ) -> bool {
        if object.0 == 0 || self.player == Some(object) {
            return false;
        }
        let Some(w) = self.weenie_mut(object) else {
            return false;
        };
        let accept = match w.house_restriction_ts {
            None => true,
            Some(old) => dereth_protocol::wrap::not_older_u8(timestamp, old),
        };
        if !accept {
            return false;
        }
        w.house_restriction_ts = Some(timestamp);
        w.pwd.restrictions = Some(rdb);
        true
    }

    /// Store the house data received in `0x0225` and schedule a panel redraw.
    ///
    /// The original message handler forwards to the panel, which keeps its own copy. Here the panel
    /// pulls from the model, so
    /// the copy lives here and [`Self::house_data_notices`] is the redraw edge â€” the same shape
    /// `allegiance_aborts` and `house_status_notices` already use, and a **count** rather than a
    /// flag because two messages in one frame are two redraws in retail.
    pub fn recv_house_data(&mut self, m: &dereth_protocol::trade::HouseDataMessage) {
        self.house = Some(HouseData::from_wire(m));
        self.house_data_notices = self.house_data_notices.wrapping_add(1);
    }

    /// Receive `0x021D`, the reply to using a slumlord statue with `0x0036`.
    ///
    /// Arrival of this profile opens the purchase/maintenance window. The panel refreshes its
    /// data and registers a 9 m range watch with flags `(true, false)` and timing `(1.0, 0.0)`.
    /// It has no separate open-panel notice; the profile is the opening event.
    ///
    /// Receiving a profile means *keep a copy, back it up, refresh*:
    /// store the statue id and profile, preserve a backup, then refresh the display. A
    /// dropped item can be "paid" into the live copy and the untouched original is still there;
    /// both copies live in the shared payment session.
    pub fn recv_house_profile(&mut self, m: &dereth_protocol::trade::HouseProfileMessage) {
        self.slumlord = Some((m.covenant_crystal, HouseProfile::from_wire(&m.profile)));
        self.payments
            .receive(m.covenant_crystal, HouseProfile::from_wire(&m.profile));
        self.house_profile_notices = self.house_profile_notices.wrapping_add(1);
    }

    /// Send the house query once during player initialization.
    ///
    /// Without this request the House tab stays in its houseless state, because the server is
    /// never asked for the data.
    /// The query is unconditional on the once-only path, so `0x021E` goes out on the **first**
    /// `0x0013` of every session and on no other event. The body is empty
    /// (`empty_message!(HouseQueryHouse)`), the queue is `Weenie`, and the answer is `0x0225` when
    /// the account owns a house and `0x0226` when it does not.
    ///
    /// Returns whether it sent â€” `false` on every `0x0013` after the first, which is the whole of
    /// the initialization guard. The two operations not performed here are the
    /// spell-component drain (`dereth_client_model::magic` already owns that queue) and
    /// the login-complete notification attempt; both are named so that "not done" and "not known
    /// about" cannot look alike.
    pub fn initialize_player(&mut self, req: &mut dyn crate::RequestSink) -> bool {
        if self.player_initialized {
            return false;
        }
        self.player_initialized = true;
        req.send(crate::Request::QueryHouse(
            dereth_protocol::trade::HouseQueryHouse,
        ));
        true
    }

    /// Send a house purchase (`0x021C`) or rent payment (`0x0221`).
    ///
    /// The panel's three guards are repeated here, as `create_tinkering_tool`'s are, because a
    /// chat command could reach this without passing through the window:
    ///
    /// ```text
    ///   op must be buy-house or rent-house      ; `rent` is that choice, so it is implied
    ///   the house has an owner (owner id != 0)
    ///   the collected list is non-empty
    /// ```
    ///
    /// The order of `items` is the window's own, top to bottom, and reaches the wire unchanged.
    pub fn house_payment(
        &mut self,
        req: &mut dyn crate::RequestSink,
        slumlord: ObjectId,
        rent: bool,
        items: &[ObjectId],
    ) -> bool {
        if slumlord.0 == 0 || items.is_empty() {
            return false;
        }
        let items = items.to_vec();
        req.send(if rent {
            crate::Request::RentHouse(dereth_protocol::trade::HouseRentHouse { slumlord, items })
        } else {
            crate::Request::BuyHouse(dereth_protocol::trade::HouseBuyHouse { slumlord, items })
        });
        true
    }

    /// Handle a partial stack dropped onto the housing-payment panel.
    ///
    /// Split the selected quantity beside the source in its existing container. This does
    /// not park a slumlord-specific pending id: the generic authoritative object-create path
    /// selects the result, and the player drops that now-whole stack into the payment list in a
    /// second gesture.
    pub fn split_item_for_house(
        &mut self,
        item: ObjectId,
        split: crate::inventory::SplitState,
        now: dereth_primitives::ServerTime,
        out: &mut dyn crate::NoticeSink,
        req: &mut dyn crate::RequestSink,
    ) -> bool {
        if !self.is_owned_by_player(item) {
            return false;
        }
        let Some((container, name)) = self.weenie(item).map(|w| {
            (
                w.pwd.container_id.unwrap_or_default(),
                w.object_name(crate::weenie::NameType::Appropriate),
            )
        }) else {
            return false;
        };
        let Some(player) = self.player else {
            return false;
        };
        if !self
            .attempt_to_place_in_container(req, out, item, player, container, false, 0, split, now)
        {
            out.emit(crate::Notice::DisplayString {
                feedback: dereth_client_contract::feedback::Feedback::WARNING,
                channel: SLUMLORD_NOTICE_CHANNEL,
                text: CANNOT_SPLIT_FOR_DWELLING.to_owned(),
            });
            return false;
        }
        out.emit(crate::Notice::DisplayString {
            feedback: dereth_client_contract::feedback::Feedback::LOCAL,
            channel: SLUMLORD_NOTICE_CHANNEL,
            text: splitting_before_housing(&name),
        });
        true
    }

    /// Query the slumlord named by the profile with `0x0258`.
    ///
    /// The panel guards this action on having a profile; the equivalent here is that it has a
    /// profile to name a slumlord from, so this repeats only the non-zero target.
    pub fn query_lord(&mut self, req: &mut dyn crate::RequestSink, target: ObjectId) -> bool {
        if target.0 == 0 {
            return false;
        }
        req.send(crate::Request::QueryLord(
            dereth_protocol::trade::HouseQueryLord { target },
        ));
        true
    }

    /// Receive `0x0227 House_UpdateRentTime`.
    ///
    /// When house data exists, the new rent time replaces the old one, every rent-payment
    /// row is cleared, and the panel redraws. Without house data the message does nothing.
    ///
    /// **The null guard is the whole message.** A shard that sends `0x0227` to a houseless
    /// character changes nothing and draws nothing â€” there is no "create the house data" arm â€” so
    /// this returns `false` and the panel is not disturbed.
    ///
    /// A new maintenance period starting is exactly this: the period's start moves forward and
    /// every `paid` goes back to zero. Both displayed rent-time rows move, the paid-in-full predicate
    /// flips, and the warning line changes colour and sentence.
    ///
    /// **This raises no notice**: the original path redraws directly. Here `HousePanel::update`'s
    /// third arm â€” the comparison
    /// against what was last drawn â€” is the edge for this message.
    pub fn recv_update_rent_time(&mut self, rent_time: i32) -> bool {
        let Some(h) = self.house.as_mut() else {
            return false;
        };
        h.rent_time = rent_time;
        h.rent.clear_payment();
        true
    }

    /// Receive `0x0228 House_UpdateRentPayment`.
    ///
    /// When house data exists, the incoming rent-payment list replaces the old list and the
    /// panel redraws. Without house data the message does nothing.
    ///
    /// The assignment **replaces** the list; it does not merge into it. That matters because ACE
    /// re-derives the whole `Rent` list with `HouseProfile.SetPaidItems` on every payment, so a
    /// part payment arrives as a complete list with the new `Paid` values, and anything the client
    /// held that the shard did not re-send is gone. The `buy` list is untouched: only the rent
    /// list is written.
    pub fn recv_update_rent_payment(
        &mut self,
        payments: &[dereth_protocol::trade::HousePayment],
    ) -> bool {
        let Some(h) = self.house.as_mut() else {
            return false;
        };
        h.rent = HousePaymentList::from_wire(payments);
        true
    }

    /// Behavior: the `0x0226`/`0x0259` arm **deletes** the house data.
    ///
    /// The panel's `0x0226` arm must not reconstruct "no house" from a count alone: that is right
    /// only when the count can never follow a `0x0225`.
    pub fn clear_house_data(&mut self) {
        self.house = None;
    }

    /// Resolve a non-apartment house's landscape location through host-supplied geometry.
    ///
    /// ```text
    ///           *lx = -1; *ly = -1
    ///           if (no house data) return false;
    ///           if (house type != 4) {                      ; an apartment has no landscape cell
    ///               if (!position_is_valid) return false;
    ///               gid_to_lcoord(get_outside_cell_id(&position), lx, ly);
    ///           }
    ///           return true;
    /// ```
    ///
    /// **The original apartment arm returns `true` with both coordinates still `-1`**. Display
    /// then tests `lx != -1 && ly != -1`, so an apartment draws no location line at
    /// all. That is the one place the two functions' return values disagree, and it is why this
    /// one answers `Option` rather than `bool`.
    ///
    /// Position validation, outside-cell lookup and landscape conversion live in `dereth-physics`, which this crate does not
    /// depend on; the caller supplies them. `dereth_client_shell::hud` is that caller.
    #[must_use]
    pub fn house_location(
        &self,
        lcoord: impl Fn(dereth_protocol::types::PositionWire) -> Option<(i32, i32)>,
    ) -> Option<(i32, i32)> {
        let h = self.house.as_ref()?;
        if h.house_type == house_type::APARTMENT {
            return None;
        }
        lcoord(h.position).filter(|&(x, y)| x != -1 && y != -1)
    }
}

// ---------------------------------------------------------------------------------------------
// 9. The two scroll-printing receivers â€” `0x0257` and `0x0271`.
// ---------------------------------------------------------------------------------------------
//
// Both handlers format a string for chat type 0, window 0, rather than the command's window.
// These answer two of the `@house` requests sent by `dereth_client_model::chat_cmd`. Captured traffic
// verifies the full-guest-list request `0x024D` and its `0x0257` reply.

/// Behavior: the `0x0257 House_UpdateHAR` scroll text.
///
/// ```text
///           "Guests:\n"                       ; always
///           if (the guest table is empty)     "  None\n"
///           else                              one formatted line per guest
///           if (roommates)                    "Roommates:\n" then the list or "  None\n"
/// ```
///
/// `roommates` is `false` from the production housing-message receiver, so the
/// roommate half never reaches the scroll in retail either. It is transcribed because the flag is
/// a parameter and the branch is real; a caller that passes `true` gets retail's other output.
///
/// Guest and roommate sections print `"  None"` when empty. A roommate row uses
/// `"  0x%08X\n"`: a **hex id**, because the roommate list carries ids rather than guest names.
#[must_use]
pub fn har_dump(har: &dereth_protocol::trade::Har, roommates: bool) -> String {
    let mut out = String::from("Guests:\n");
    if har.guest_table.entries.is_empty() {
        out.push_str("  None\n");
    } else {
        for (_, guest) in guest_dump_order(&har.guest_table) {
            guest_info_dump(guest, &mut out);
        }
    }
    if roommates {
        out.push_str("Roommates:\n");
        match har.roommate_list.as_deref() {
            None | Some([]) => out.push_str("  None\n"),
            Some(list) => {
                for id in list {
                    out.push_str(&format!("  {id:#010X}\n"));
                }
            }
        }
    }
    out
}

/// One guest's line: two leading spaces, the name, `" *"` if storage permission is nonzero,
/// then a newline. The indentation precedes the name.
fn guest_info_dump(guest: &dereth_protocol::trade::GuestInfo, out: &mut String) {
    out.push_str("  ");
    out.push_str(&guest.char_name);
    if guest.item_storage_permission != 0 {
        out.push_str(" *");
    }
    out.push('\n');
}

/// The source table walks buckets `0..table_size` ascending,
/// which is **not** the order the entries arrived on the wire.
///
/// `dereth_protocol::archive::PackedHash` keeps the wire order and the bucket count, so the bucket walk
/// is reconstructed here: a stable sort on `key % table_size`. **What is still open** is the order
/// *within* one bucket â€” whether insertion prepends or appends to the chain â€” and
/// there is no oracle for it: the captured `0x0257` fixture carries an empty guest table.
/// Two guests that collide in one
/// bucket may therefore print in the wrong order; one that does not, cannot.
fn guest_dump_order(
    table: &dereth_protocol::archive::PackedHash<u32, dereth_protocol::trade::GuestInfo>,
) -> Vec<&(u32, dereth_protocol::trade::GuestInfo)> {
    let mut rows: Vec<&(u32, dereth_protocol::trade::GuestInfo)> = table.entries.iter().collect();
    if table.table_size != 0 {
        rows.sort_by_key(|(k, _)| k % table.table_size);
    }
    rows
}

/// The available-houses handler's header line â€” `0x0271`'s first
/// scroll print.
///
/// Types 1 through 4 print `cottages`, `villas`, `mansions` and `apartments`, respectively,
/// in `"There are %d %s available.\n"`.
///
/// An unrecognised house type is not refused: it prints the sentence with an empty `%s`, two
/// spaces and all. That is retail's, and it is transcribed rather than tidied.
#[must_use]
pub fn available_houses_header(house_type: u32, num_houses: i32) -> String {
    let word = match house_type {
        house_type::COTTAGE => "cottages",
        house_type::VILLA => "villas",
        house_type::MANSION => "mansions",
        house_type::APARTMENT => "apartments",
        _ => "",
    };
    format!("There are {num_houses} {word} available.\n")
}

/// Format one landcell's line.
///
/// ```text
///   gid_to_lcoord(gid) -> (ew, ns)
///   ew >= 0x400 -> "E", else "W"
///   ns >= 0x400 -> "N", else "S"
///   value = |(c - 0x400) * 0.1 + 0.5|     ; on BOTH axes
///   _snprintf(buf, 99, "     %.1f%s, %.1f%s\n", ns_value, ns_letter, ew_value, ew_letter)
/// ```
///
/// Three things that are easy to miss: the **five leading spaces**, the
/// **`fabs`** â€” so a western or southern cell's magnitude is printed and the letter carries the
/// sign â€” and that **north/south is printed first** even though `gid_to_lcoord`'s first
/// out-parameter is east/west. The `+ 0.5` before the `fabs` is retail's own rounding and is
/// deliberately not a `round()`: at `lcoord == 1024` exactly it prints `0.5`, not `0.0`.
///
/// Takes the two landscape coordinates rather than the cell id because `gid_to_lcoord` lives in
/// `dereth-physics`, which this crate does not depend on â€” the same split
/// `dereth_client_model::quests::contract_location_text` has.
#[must_use]
pub fn coord_line(ew: i32, ns: i32) -> String {
    let ns_letter = if ns >= 0x400 { "N" } else { "S" };
    let ew_letter = if ew >= 0x400 { "E" } else { "W" };
    let ns_value = (f64::from(ns - 0x400) * 0.1 + 0.5).abs();
    let ew_value = (f64::from(ew - 0x400) * 0.1 + 0.5).abs();
    format!("     {ns_value:.1}{ns_letter}, {ew_value:.1}{ew_letter}\n")
}

/// Printed after the coordinate list when `num_houses > 400`, **with no trailing newline**.
pub const TOO_MANY_HOUSES: &str = "There were too many houses to display all the locations. Only the first 400 locations are displayed here.";

/// An **apartment** listing prints its header
/// and nothing else: no coordinates, and therefore not the over-400 line either, however many
/// there are. Apartments are stacked in fixed complexes and have no landscape position to give.
#[must_use]
pub fn available_houses_lists_coords(house_type: u32) -> bool {
    house_type != house_type::APARTMENT
}

/// Whether the result count is strictly greater than **400**.
#[must_use]
pub fn available_houses_truncated(num_houses: i32) -> bool {
    num_houses > 400
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Housing time helpers with their literal constants.
    #[test]
    fn the_rent_and_purchase_periods_are_exact() {
        assert_eq!(PURCHASE_WAIT_SECONDS, 0x0027_8D00);
        assert_eq!(rent_period(house_type::APARTMENT), 7_776_000.0);
        for t in [
            house_type::UNDEF,
            house_type::COTTAGE,
            house_type::VILLA,
            house_type::MANSION,
        ] {
            assert_eq!(rent_period(t), 2_592_000.0, "type {t}");
        }
        assert_eq!(rent_period_days(house_type::APARTMENT), 90);
        assert_eq!(rent_period_days(house_type::COTTAGE), 30);

        // The comparison is strictly greater.
        assert!(!has_purchase_wait_period_expired(2_592_000, 0));
        assert!(has_purchase_wait_period_expired(2_592_001, 0));
    }

    /// Payment lines render the documented way.
    #[test]
    fn payment_lines_render_the_documented_way() {
        let p = HousePayment {
            wcid: 273,
            num: 1,
            paid: 0,
            name: "Pyreal".into(),
            pname: "Pyreals".into(),
        };
        assert_eq!(
            p.compose_text(),
            "1 Pyreal",
            "num == 1 takes `name`, never `pname`"
        );
        assert_eq!(p.compose_text2(), "0/1 Pyreal");
        let p2 = HousePayment {
            num: 30_000,
            paid: 100,
            ..p.clone()
        };
        assert_eq!(
            p2.compose_text(),
            "30000 Pyreals",
            "the shard's plural, verbatim"
        );
        assert_eq!(p2.compose_text2(), "100/30000 Pyreals");

        // The third arm: no plural supplied, so the name grows an `"s"`. The `'s'`/`'x'` -> `"es"`
        // test reads the NUL terminator and can never fire; a name ending in `s`
        // therefore gets a plain `"s"` here too, exactly as retail does.
        let bare = HousePayment {
            num: 3,
            name: "Olthoi Mask".into(),
            pname: String::new(),
            ..HousePayment::default()
        };
        assert_eq!(bare.compose_text(), "3 Olthoi Masks");
        let ess = HousePayment {
            name: "Brass".into(),
            ..bare.clone()
        };
        assert_eq!(
            ess.compose_text(),
            "3 Brasss",
            "the `es` arm is dead in retail"
        );

        let list = HousePaymentList(vec![p, p2]);
        assert_eq!(list.compose_text(), "1 Pyreal, 30000 Pyreals");
        assert_eq!(list.compose_text2(), "0/1 Pyreal, 100/30000 Pyreals");
    }

    /// Payment-list predicates and accounting.
    #[test]
    fn payment_lists_track_what_is_still_wanted() {
        let mut l = HousePaymentList(vec![
            HousePayment {
                wcid: 273,
                num: 100,
                paid: 0,
                ..HousePayment::default()
            },
            HousePayment {
                wcid: 20630,
                num: 5,
                paid: 5,
                ..HousePayment::default()
            },
        ]);
        assert!(!l.is_paid_in_full());
        assert_eq!(l.needs_more(273, None), 1);
        assert_eq!(
            l.needs_more(20630, None),
            0,
            "that row is already paid in full"
        );
        assert_eq!(l.needs_more(999, None), 0, "no row wants it");

        assert_eq!(l.attempt_to_pay(273, 40, None), 40);
        assert_eq!(l.needs_more(273, None), 1);
        assert_eq!(
            l.attempt_to_pay(273, 999, None),
            60,
            "paying more than is wanted credits only what is wanted"
        );
        assert!(l.is_paid_in_full());
        assert_eq!(
            l.attempt_to_pay(273, 1, None),
            0,
            "`row.paid >= row.num` -> 0"
        );

        l.clear_payment();
        assert!(!l.is_paid_in_full());
    }

    /// Trade notes are indivisible: the return value is the number of notes
    /// consumed, while `paid` advances by their face value.
    #[test]
    fn trade_notes_credit_the_pyreal_row_in_whole_notes() {
        let mut l = HousePaymentList(vec![HousePayment {
            wcid: COINSTACK_WCID,
            num: 250_000,
            ..HousePayment::default()
        }]);

        assert_eq!(l.needs_more(0xDEAD, Some(100_000)), 1);
        assert_eq!(l.attempt_to_pay(0xDEAD, 3, Some(100_000)), 2);
        assert_eq!(l.0[0].paid, 200_000);
        assert_eq!(
            l.needs_more(0xDEAD, Some(100_000)),
            0,
            "one more note would overpay"
        );
        assert_eq!(l.attempt_to_pay(0xDEAD, 1, Some(100_000)), 0);
        assert_eq!(l.attempt_to_pay(0xDEAD, 1, Some(50_000)), 1);
        assert_eq!(l.0[0].paid, 250_000);
    }

    /// The housing payment panel adds its own sentence after the generic inventory splitter
    /// refuses. The global request lock is the smallest real refusal condition.
    #[test]
    fn a_refused_house_split_uses_the_dwelling_costs_sentence() {
        const PLAYER: ObjectId = ObjectId(1);
        const COINS: ObjectId = ObjectId(2);
        let mut world = crate::World::new();
        world.player = Some(PLAYER);
        let mut player = crate::Weenie::new(PLAYER);
        player.valid = true;
        player.pwd.bitfield |= crate::weenie::bitfield::OPENABLE;
        player.pwd.items_capacity = Some(10);
        world.tables.weenies.insert(PLAYER, player);
        let mut coins = crate::Weenie::new(COINS);
        coins.valid = true;
        coins.pwd.container_id = Some(PLAYER);
        coins.pwd.stack_size = Some(10);
        coins.pwd.max_stack_size = Some(10);
        coins.pwd.name = "Pyreal".to_owned();
        coins.pwd.plural_name = Some("Pyreals".to_owned());
        world.tables.weenies.insert(COINS, coins);
        world.selected = Some(COINS);
        world.attack_in_progress = true;
        let mut notices = crate::RecordingSink::default();
        let mut requests = crate::RecordingRequests::default();

        assert!(!world.split_item_for_house(
            COINS,
            crate::inventory::SplitState {
                split_size: 3,
                max_split_size: 10
            },
            dereth_primitives::ServerTime(2.0),
            &mut notices,
            &mut requests,
        ));
        assert!(requests.0.is_empty());
        assert_eq!(
            notices.0.last(),
            Some(&crate::Notice::DisplayString {
                feedback: dereth_client_contract::feedback::Feedback::WARNING,
                channel: SLUMLORD_NOTICE_CHANNEL,
                text: CANNOT_SPLIT_FOR_DWELLING.to_owned(),
            })
        );
    }

    /// Individual, allegiance and open-house access controls.
    #[test]
    fn restrictions_admit_guests_the_allegiance_and_everyone_on_open_house() {
        let mut r = Restrictions::default();
        assert!(!r.is_allowed_in(ObjectId(5), ObjectId(0)));

        r.table.insert(ObjectId(5), 1);
        assert!(r.is_allowed_in(ObjectId(5), ObjectId(0)));
        assert!(!r.is_allowed_in(ObjectId(6), ObjectId(0)));

        r.monarch_iid = ObjectId(0x77);
        assert!(
            r.is_allowed_in(ObjectId(6), ObjectId(0x77)),
            "an allegiance member is a guest"
        );
        assert!(!r.is_allowed_in(ObjectId(6), ObjectId(0x78)));

        r.set_open_house(true);
        assert!(r.is_allowed_in(ObjectId(999), ObjectId(0)));
        r.set_open_house(false);
        assert!(!r.is_allowed_in(ObjectId(999), ObjectId(0)));
    }

    /// Restriction-update acceptance, covering all four gates.
    #[test]
    fn restriction_updates_skip_the_player_and_reject_a_stale_timestamp() {
        use crate::weenie::Weenie;
        let mut w = crate::world::World::new();
        w.set_player(ObjectId(1));
        w.tables
            .weenies
            .insert(ObjectId(1), Weenie::new(ObjectId(1)));
        w.tables
            .weenies
            .insert(ObjectId(2), Weenie::new(ObjectId(2)));
        let rdb = dereth_protocol::types::RestrictionDb::default;

        assert!(
            !w.recv_update_restrictions(1, ObjectId(0), rdb()),
            "a zero id is ignored"
        );
        assert!(
            !w.recv_update_restrictions(1, ObjectId(1), rdb()),
            "restrictions are never applied to yourself"
        );
        assert!(
            !w.recv_update_restrictions(1, ObjectId(99), rdb()),
            "an unknown object"
        );

        assert!(w.recv_update_restrictions(5, ObjectId(2), rdb()));
        assert!(w.weenie(ObjectId(2)).unwrap().pwd.restrictions.is_some());
        assert!(
            !w.recv_update_restrictions(4, ObjectId(2), rdb()),
            "a stale timestamp"
        );
        assert!(w.recv_update_restrictions(6, ObjectId(2), rdb()));
    }

    /// The rent warning and the time sentinel match.
    #[test]
    fn the_rent_warning_and_the_time_sentinel_match() {
        assert_eq!(convert_time(0), None);
        assert_eq!(convert_time(1_600_000_000), Some(1_600_000_000));
        let m = construct_rent_warning_message(30);
        assert_eq!(
            m,
            "Warning!  You have not paid your maintenance costs for the last 30 day maintenance \
             period.  Please pay these costs by this deadline or you will lose your house, and \
             all your items within it."
        );
        assert!(
            construct_rent_warning_message(rent_period_days(house_type::APARTMENT))
                .contains("the last 90 day")
        );
        assert_eq!(
            MAINTENANCE_ALREADY_PAID.len(),
            100,
            "the retail string is 100 bytes before the NUL"
        );
    }

    /// House data carries the two maintenance instants and the apartment exit.
    #[test]
    fn house_data_carries_the_two_maintenance_instants_and_the_apartment_exit() {
        let owed = HousePayment {
            wcid: 273,
            num: 10,
            paid: 0,
            ..HousePayment::default()
        };
        let mut h = HouseData {
            rent_time: 1_000_000,
            house_type: house_type::COTTAGE,
            rent: HousePaymentList(vec![owed.clone()]),
            ..HouseData::default()
        };
        assert!(h.rent_is_owed());
        assert_eq!(h.maintenance_period_end(), 1_000_000 + 2_592_000);
        assert_eq!(
            h.maintenance_next_due(),
            1_000_000 + 2_592_000,
            "rent still owed: one period, not two"
        );
        h.rent = HousePaymentList(vec![HousePayment { paid: 10, ..owed }]);
        assert!(!h.rent_is_owed());
        assert_eq!(
            h.maintenance_next_due(),
            1_000_000 + 2 * 2_592_000,
            "the period is doubled once nothing is owed"
        );

        let mut w = crate::world::World::new();
        assert_eq!(
            w.house_location(|_| Some((99, 98))),
            None,
            "no house, no line"
        );
        w.house = Some(h.clone());
        assert_eq!(w.house_location(|_| Some((99, 98))), Some((99, 98)));
        assert_eq!(
            w.house_location(|_| None),
            None,
            "an invalid Position draws nothing"
        );
        assert_eq!(w.house_location(|_| Some((-1, -1))), None);
        w.house = Some(HouseData {
            house_type: house_type::APARTMENT,
            ..h
        });
        assert_eq!(
            w.house_location(|_| Some((99, 98))),
            None,
            "an apartment returns true with (-1,-1) and DisplayLocation drops the line"
        );
    }

    /// The house data receiver keeps the copy and moves the redraw edge.
    #[test]
    fn the_house_data_receiver_keeps_the_copy_and_moves_the_redraw_edge() {
        let mut w = crate::world::World::new();
        assert!(w.house.is_none());
        assert_eq!(w.house_data_notices, 0);
        let m = dereth_protocol::trade::HouseDataMessage {
            buy_time: 1_600_000_000,
            rent_time: 1_600_100_000,
            house_type: house_type::VILLA,
            maintenance_free: 0,
            buy: vec![dereth_protocol::trade::HousePayment {
                num: 30_000,
                paid: 30_000,
                wcid: 273,
                name: "Pyreal".into(),
                plural_name: "Pyreals".into(),
            }],
            rent: vec![],
            position: dereth_protocol::types::PositionWire::default(),
        };
        w.recv_house_data(&m);
        let h = w
            .house
            .as_ref()
            .expect("the full house update keeps its copied profile");
        assert_eq!(h.buy_time, 1_600_000_000);
        assert_eq!(h.house_type, house_type::VILLA);
        assert_eq!(h.buy.compose_text(), "30000 Pyreals");
        assert_eq!(w.house_data_notices, 1);
        w.recv_house_data(&m);
        assert_eq!(w.house_data_notices, 2, "two notices are two redraws");
        w.clear_house_data();
        assert!(w.house.is_none());
    }

    /// Housing enum values.
    #[test]
    fn the_housing_enums_match() {
        assert_eq!(house_type::APARTMENT, 4);
        assert_eq!(house_bitmask::REQUIRES_MONARCH, 2);
        assert_eq!(hook_type_enum::ROOF, 16);
        assert_eq!(hook_type::CALL_PES, 7);
        assert_eq!(panel_color::RENT_NOT_PAID, 2);
        assert_eq!(rdb_bitmask::OPEN_HOUSE, 1);
        assert_eq!(BUY_HOUSE, 1);
        assert_eq!(RENT_HOUSE, 2);
    }
}
