//! The secure-trade state machine — `Trade` and its client-side controller.
//!
//! `0x0208 Trade_ClearTradeAcceptance` is the anti-scam mechanism and it lives on the
//! **server**. The client refuses to accept when the two lists the *window* is showing disagree
//! with the mirrored `Trade`. In that case it sends the accept-trade request carrying a **fresh, empty** `Trade`. The
//! different payload tells the server the two views are out of sync and prevents a stale display
//! from authorizing a trade.
//!
//! # Six details that are easy to get wrong, all checked against retail
//!
//! 1. **Registration sets `status = 2` (`Open`)**, not `Pending`.
//! 2. It **does not clear the lists or the acceptances**. Clearing all four would silently
//!    discard a partner's items on a re-register.
//! 3. It **does not touch `initiator`**. The receive handler sets that from the message's ids.
//! 4. **The default value initialises `stamp` to `-1.0`** (`0xBFF00000_00000000`), not
//!    to zero. Zero is a stamp the server can legitimately send.
//! 5. **The inner add refuses a duplicate, caps the list at `0x1A0A` and inserts at
//!    a caller-given position.** Not a bare `push`.
//! 6. **Removal takes a side and searches that one list.** Searching both would let a `0x0201`
//!    naming the partner's side delete an item out of *your* offer.
//!
//! And one about a name rather than a behaviour: **`0x0200 Trade_AddToTrade`'s third dword is an
//! insert position, not a container-properties flag.** The receive handler passes `(iid, side,
//! pos)` through the two add-item layers to the list insertion routine as the index to insert
//! before. Constructing `ContentProfile(iid)` writes `container_properties = 0`
//! and nothing else ever writes it, so **every profile in a client-side `Trade` carries zero** and
//! [`Trade::num_containers`] / [`Trade::num_partner_containers`] are structurally always 0. The protocol field
//! name `container_properties` is therefore misleading for this message.

use dereth_primitives::ObjectId;
use dereth_protocol::types::ContentProfile;

/// The packed content-list count must be below `0x1A0A` — the client's cap. **6666** items per
/// side.
pub const MAX_TRADE_LIST: usize = 0x1A0A;

/// The default constructor writes `0xBFF00000` into the high dword of `stamp` and `0` into the
/// low one, which is IEEE **-1.0**.
pub const INITIAL_STAMP: f64 = -1.0;

/// Display-string channel `0x1A`, used by all three trade refusals.
pub const TRADE_MESSAGE_CHANNEL: u32 = 0x1A;

/// The trade status — note that `WaitingToClose` is **4**, not 3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum TradeStatus {
    #[default]
    Undef = 0,
    Pending = 1,
    /// What registration writes: `status = 2`.
    Open = 2,
    WaitingToClose = 4,
}

/// Which of the two trade lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum TradeListId {
    #[default]
    Undef = 0,
    SelfList = 1,
    Partner = 2,
}

impl TradeListId {
    /// The wire value carried by `0x0200`/`0x0201` and tested by the add/remove handlers.
    #[must_use]
    pub fn from_wire(v: u32) -> Self {
        match v {
            1 => Self::SelfList,
            2 => Self::Partner,
            _ => Self::Undef,
        }
    }
}

/// The client's mirrored trade state, which is also the sync token sent back inside `0x01FA`.
/// Both original constructors allocate a `0x40`-byte layout; this Rust type models the state and
/// does not claim that physical size.
#[derive(Debug, Clone, PartialEq)]
pub struct Trade {
    pub self_list: Vec<ContentProfile>,
    pub partner_list: Vec<ContentProfile>,
    pub partner: ObjectId,
    /// The server's trade stamp. `-1.0` until a `0x01FD` registers one — see [`INITIAL_STAMP`].
    pub stamp: f64,
    pub status: TradeStatus,
    /// 1 if **I** opened the negotiation. Written by the *system*, not by registration.
    pub initiator: bool,
    pub accepted: bool,
    pub partner_accepted: bool,
}

impl Default for Trade {
    /// Construct an empty trade with the sentinel stamp.
    fn default() -> Self {
        Self {
            self_list: Vec::new(),
            partner_list: Vec::new(),
            partner: ObjectId(0),
            stamp: INITIAL_STAMP,
            status: TradeStatus::Undef,
            initiator: false,
            accepted: false,
            partner_accepted: false,
        }
    }
}

impl Trade {
    /// Register a nonzero partner and the server's stamp without disturbing either item list.
    ///
    /// ```text
    /// if (partner == 0) return 0
    /// partner = partner; stamp = stamp; status = 2; return 1
    /// ```
    ///
    /// **Three things it does not do**: clear the lists, clear the acceptances, and set
    /// `initiator`. See this module's header.
    pub fn register(&mut self, partner: ObjectId, stamp: f64) -> bool {
        if partner.0 == 0 {
            return false;
        }
        self.partner = partner;
        self.stamp = stamp;
        self.status = TradeStatus::Open;
        true
    }

    /// Add an item to the side named by the message; this is the entry point both the network handler and
    /// partner-item helper use.
    ///
    /// ```text
    /// if side == 1 && !contains(partner_list, iid) -> add_item_to(iid, self_list,    pos)
    /// if side == 2 && !contains(self_list,    iid) -> add_item_to(iid, partner_list, pos)
    /// ```
    ///
    /// The cross-list search is the client's: the same object may not appear on both sides.
    /// [`Self::add_item_to`] performs the per-list checks and insertion.
    pub fn add_item(&mut self, side: TradeListId, item: ObjectId, pos: usize) -> bool {
        match side {
            TradeListId::SelfList => {
                if Self::contains(&self.partner_list, item) {
                    return false;
                }
                Self::add_item_to(&mut self.self_list, item, pos)
            }
            TradeListId::Partner => {
                if Self::contains(&self.self_list, item) {
                    return false;
                }
                Self::add_item_to(&mut self.partner_list, item, pos)
            }
            TradeListId::Undef => false,
        }
    }

    /// Apply the inner add operation to one list.
    ///
    /// ```text
    /// iid == 0                                   -> 0
    /// the list already holds ContentProfile(iid) -> 0
    /// the list holds 0x1A0A or more              -> 0
    /// insert ContentProfile(iid) at pos          -> 1
    /// ```
    ///
    /// Constructing `ContentProfile(iid)` sets `container_properties = 0`, so the
    /// profile this stores always carries zero — see the module header.
    /// The retail list routine walks `pos` links and inserts *before* what it lands
    /// on, appending when it runs off the end, which is a `Vec::insert` clamped to the length.
    fn add_item_to(list: &mut Vec<ContentProfile>, item: ObjectId, pos: usize) -> bool {
        if item.0 == 0 || Self::contains(list, item) || list.len() >= MAX_TRADE_LIST {
            return false;
        }
        let at = pos.min(list.len());
        list.insert(
            at,
            ContentProfile {
                iid: item,
                container_properties: 0,
            },
        );
        true
    }

    /// Search by the id in a temporary `ContentProfile(iid)`.
    fn contains(list: &[ContentProfile], item: ObjectId) -> bool {
        list.iter().any(|c| c.iid == item)
    }

    /// Remove `(iid, side)` from **one** list, chosen by the side.
    ///
    /// The remove notice passes its own side through unchanged and does nothing for any other
    /// value. The trade-failure handler always passes **1**, because a failed add is
    /// rolled back out of *your* offer.
    pub fn remove_item(&mut self, item: ObjectId, side: TradeListId) -> bool {
        if item.0 == 0 {
            return false;
        }
        let list = match side {
            TradeListId::SelfList => &mut self.self_list,
            TradeListId::Partner => &mut self.partner_list,
            TradeListId::Undef => return false,
        };
        match list.iter().position(|c| c.iid == item) {
            Some(i) => {
                list.remove(i);
                true
            }
            None => false,
        }
    }

    /// Flush both lists and clear both acceptances. The registration,
    /// the stamp and the status survive.
    pub fn reset(&mut self) {
        self.self_list.clear();
        self.partner_list.clear();
        self.accepted = false;
        self.partner_accepted = false;
    }

    /// Test whether the partner-side list contains `id`.
    #[must_use]
    pub fn is_partner_trading_item(&self, id: ObjectId) -> bool {
        Self::contains(&self.partner_list, id)
    }

    /// Count entries whose `container_properties` is 0.
    ///
    /// **Structurally the whole list**, because nothing on the client ever writes that field; kept
    /// as the filter the client writes rather than as `len()` so that a `Trade` unpacked off the
    /// wire would count the same way the client's does.
    #[must_use]
    pub fn num_items(&self) -> usize {
        self.self_list
            .iter()
            .filter(|c| c.container_properties == 0)
            .count()
    }

    /// Count self-side container profiles.
    #[must_use]
    pub fn num_containers(&self) -> usize {
        self.self_list
            .iter()
            .filter(|c| c.container_properties != 0)
            .count()
    }

    /// Count partner-side item profiles.
    #[must_use]
    pub fn num_partner_items(&self) -> usize {
        self.partner_list
            .iter()
            .filter(|c| c.container_properties == 0)
            .count()
    }

    /// Count partner-side container profiles.
    #[must_use]
    pub fn num_partner_containers(&self) -> usize {
        self.partner_list
            .iter()
            .filter(|c| c.container_properties != 0)
            .count()
    }

    /// The self-side object total — [`Self::num_containers`] `+` [`Self::num_items`], in that
    /// order, which is the number the accept path compares against the window's self list.
    #[must_use]
    pub fn num_self_objects(&self) -> usize {
        self.num_containers() + self.num_items()
    }

    /// The partner-side object total.
    #[must_use]
    pub fn num_partner_objects(&self) -> usize {
        self.num_partner_containers() + self.num_partner_items()
    }

    /// Both sides have accepted; the server may now complete the trade.
    #[must_use]
    pub fn both_accepted(&self) -> bool {
        self.accepted && self.partner_accepted
    }

    /// Build the packed `Trade` carried by `0x01FA Trade_AcceptTrade`.
    ///
    /// The only client-to-server message in the game that sends a whole packed object rather than
    /// scalars. The body is `self_list + 0x1C + partner_list`, and `0x1C` is the
    /// six scalars below.
    #[must_use]
    pub fn to_wire(&self) -> dereth_protocol::trade::Trade {
        dereth_protocol::trade::Trade {
            self_list: self.self_list.clone(),
            partner_list: self.partner_list.clone(),
            partner: self.partner,
            stamp: self.stamp,
            status: self.status as u32,
            initiator: i32::from(self.initiator),
            accepted: i32::from(self.accepted),
            partner_accepted: i32::from(self.partner_accepted),
        }
    }
}

/// The window's accept-button decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceptDecision {
    /// Set `accepted = 1`, then send the accept-trade request
    /// (`0x01FA`) carrying the whole mirrored `Trade`.
    Accept,
    /// The out-of-sync branch sends a **freshly constructed** empty `Trade` through the same
    /// accept-trade request, so the server sees an empty offer and
    /// knows the two ends disagree. `accepted` is **not** set.
    OutOfSync,
}

/// Accept only if the *displayed* lists agree with the mirror.
///
/// ```text
/// if (self objects in trade    == items in the window's self list &&
///     partner objects in trade == items in the window's partner list) accept the trade
/// else tell the server the trade is out of sync
/// set the trade button to state 6; update the trade button state
/// ```
///
/// The two counts come from the **window**, which is why this takes them as arguments rather than
/// reading the same `Trade` twice: a comparison of a value with itself is not the anti-scam check.
#[must_use]
pub fn accept_decision(
    t: &Trade,
    displayed_self: usize,
    displayed_partner: usize,
) -> AcceptDecision {
    if t.num_self_objects() == displayed_self && t.num_partner_objects() == displayed_partner {
        AcceptDecision::Accept
    } else {
        AcceptDecision::OutOfSync
    }
}

/// Button-state values for the trade button `0x10000086` — the three values
/// the update, accept, and decline paths
/// write, as the literals they write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum TradeButtonState {
    /// `0x0D` — the value [`update_trade_button_state`] writes when both lists are empty.
    Disabled = 0x0D,
    /// `1` — adding a self item, adding a partner item and declining all write this.
    Enabled = 1,
    /// `6` — the accept path writes this, whichever branch it took.
    Accepted = 6,
}

/// Apply the trade button's **transition**, which is not a function of the model alone.
///
/// ```text
/// n = rows in the window's partner list + rows in its self list
/// state 6    and n == 0 -> 0x0D
/// state 1    and n == 0 -> 0x0D
/// state 0x0D and n > 0  -> 1
/// ```
///
/// Note what it cannot do: it never writes **6**, and from `Disabled` it only ever goes to
/// `Enabled`. `Accepted` is reachable only through the accept path.
#[must_use]
pub fn update_trade_button_state(current: TradeButtonState, displayed: usize) -> TradeButtonState {
    match (current, displayed) {
        (TradeButtonState::Accepted | TradeButtonState::Enabled, 0) => TradeButtonState::Disabled,
        (TradeButtonState::Disabled, n) if n > 0 => TradeButtonState::Enabled,
        (s, _) => s,
    }
}

/// Every string the secure-trade code produces itself as a wide literal. All of them
/// go to [`TRADE_MESSAGE_CHANNEL`].
///
/// A byte-level scan of both secure-trade routines found **nine literal pushes carrying eight
/// distinct strings**. `CANCELLED` appears twice because the client first measures and then copies
/// the same text. The window has no other trade-message strings besides its total-items label.
pub mod messages {
    /// Shown when the object is not owned by the player and the call was not the silent one.
    pub const ONLY_CARRIED: &str = "You can only trade items you are carrying";
    /// Shown when the item is selected and a partial split is pending.
    pub const MUST_SPLIT: &str = "You must split the stack before trading it.";
    /// Shown when splitting the selected stack fails before the tradeable sub-stack is created.
    pub const CANNOT_SPLIT: &str = "Cannot split the stack to trade it";

    // Five further messages.

    /// Refusal shown when the player is not in `CombatMode::NonCombat`.
    pub const PEACE_MODE: &str = "You need to be in peace mode to trade.";

    /// Refusal for dragging an item onto a second player while another negotiation is registered.
    pub const ALREADY_TRADING: &str = "You are already trading with someone else.";

    /// **Unconditional** close text, whatever the reason. It goes directly to the scroll with
    /// `(text, 0x1A, 1, 0)` rather than through the display-string notice.
    ///
    /// The two pushes are `wcslen` and `wcscpy` of the one literal, not two lines.
    pub const CANCELLED: &str = "The trade has been cancelled.";

    /// Container-arm text formatted with the object's display name. The literal is the one
    /// retail formats at the call site.
    #[must_use]
    pub fn trading_contents(name: &str) -> String {
        format!("Trading contents of {name}")
    }

    /// Split-arm text formatted with the object's display name.
    #[must_use]
    pub fn splitting_before_trading(name: &str) -> String {
        format!("Splitting the {name} before trading them")
    }
}

/// What the trade window decided after an item was dragged onto another player.
///
/// Three outcomes and not a `bool`, because two of the three arms return `false` and only one of
/// them says anything: retail's already-offered arm is silent and its split arm is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForDummies {
    /// The window's add-item `(id, 0, true, true, false)` — it inserts the row and `0x01F8` goes
    /// out.
    Offer,
    /// The window's self list already holds it: `return false` with no message.
    AlreadyOffered,
    /// [`messages::MUST_SPLIT`] on [`TRADE_MESSAGE_CHANNEL`], then `return false`.
    MustSplit,
}

/// The trade window's pending source-item id, class id, and split stack size.
///
/// A partial stack is not put on the table optimistically.  The window remembers these three
/// values until an attribute-change notice identifies the authoritative object the server
/// created, or an attempt-failed notice clears the attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingTradeSplit {
    pub source: ObjectId,
    pub wcid: u32,
    pub stack_size: u32,
}

/// The trade mirror plus the four ids initialized to zero. Ending a character session rebuilds a
/// fresh value, which is [`Self::default`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TradeSystem {
    /// The trade mirror.
    pub trade: Trade,
    /// Who started the trade.
    pub initiator: ObjectId,
    /// The trade partner — the id used for range checks and rejecting a second partner.
    pub partner: ObjectId,
    /// The player a pending item-on-player gesture is aimed at.
    pub attempt_to_player: ObjectId,
    /// The item of that pending gesture.
    pub attempt_object: ObjectId,
    /// **Whether `TradePanel` is up — the window's visibility and nothing else.**
    ///
    /// Registration is the only protocol event that makes the window visible. The close button is
    /// the only path that hides it. `0x01FE Trade_OpenTrade` reaches an inherited no-op, while a
    /// `0x01FF Trade_CloseTrade` resets the lists and partner label but leaves an empty window for
    /// the player to dismiss. [`Self::partner`] becoming zero marks the end of the negotiation.
    pub open: bool,

    // ---- server-removed ids ---------------------------------------------------------------------
    /// Ids the server has removed from the player's displayed offer during this negotiation. This
    /// is rebuild-only state with no counterpart in the original trade-system object.
    ///
    /// In the client the self-side removal is a *notice handler*: it edits the window as the datagram
    /// lands and needs no memory. This build's `TradePanel` is rebuilt from a snapshot once a
    /// frame, and the row it has to take away is the **optimistic** one the window inserted before
    /// the wire — an id that is not in `self_list` and, after a
    /// refusal, never will be. Without this the panel cannot tell "the server has not answered
    /// yet" from "the server refused", and the row stays on the table for ever.
    ///
    /// Written by the two retail producers: the `side == 1` remove arm and the trade-failure arm.
    /// Cleared per id by a
    /// `0x0200 Trade_AddToTrade` on side 1, and wholesale by
    /// the reset handler, close handler, and [`Self::end_character_session`].
    ///
    /// The server-attempt-failed notice is not a producer; it only clears the pending split id.
    pub self_removed: Vec<ObjectId>,

    // ---- acceptance lights --------------------------------------------------------------------
    /// **The window's two acceptance lights, which are not the mirror's two flags** — the
    /// *"any change resets both acceptances"* rule, and a second rebuild-only field declared here
    /// for the same reason [`Self::self_removed`] is.
    ///
    /// The add-self, add-partner, remove-self, and remove-partner UI paths all enable the trade
    /// button and disable the partner indicator **before** looking up the row. The update is
    /// therefore unconditional on whether a row actually moved.
    ///
    /// The server clears both protocol acceptances when an item changes but does not send an
    /// acceptance notice for that local UI update. The mirror flags can therefore remain set while
    /// the window has already darkened. A client
    /// that draws the lights straight off the mirror leaves a lit "partner accepted" indicator
    /// after the partner has added another item — which is the exact deception a *secure* trade
    /// window exists to prevent.
    ///
    /// Set by the `0x0200`/`0x0201`/`0x0207` handlers; cleared by `0x0202`, `0x0203` and by the
    /// local accept and decline presses, which are the four places retail writes the button with a
    /// state of its own; cleared with everything else on reset and close.
    pub acceptance_darkened: bool,

    /// The window's confirmed lists after the clear-acceptance handler resets
    /// them without resetting the protocol mirror. This is rebuild-only state, like
    /// `self_removed`: subsequent add/remove notices edit these lists even when the mirror
    /// rejects a duplicate. `None` uses the ordinary mirror-backed projection.
    pub display_lists: Option<[Vec<ObjectId>; 2]>,

    /// The trade window's three pending-split values.
    pub pending_split: Option<PendingTradeSplit>,
}

impl TradeSystem {
    /// Discard all trade state when the character session ends.
    pub fn end_character_session(&mut self) {
        *self = Self::default();
    }
}

// ==============================================================================================
// Trade-system integration on the world: ten inbound handlers and five senders.
// These are the production paths that construct trade requests and update `Weenie::trade_state`.
// ==============================================================================================

impl crate::World {
    /// Handle inbound `0x01FD Trade_RegisterTrade`.
    ///
    /// ```text
    /// self.initiator = initiator; self.partner = partner
    /// register(trade, partner, stamp)
    /// trade.initiator = (initiator == player_id)
    /// raise the register-trade notice
    /// if (partner == attempt_to_player) attempt_to_trade_item(...)
    /// attempt_object = 0; attempt_to_player = 0
    /// ```
    ///
    /// The `initiator` comparison belongs to this system handler, not the mirror registration, which
    /// never touches the field. It compares the message's initiator with the local player id.
    pub fn handle_register_trade(
        &mut self,
        m: &dereth_protocol::trade::TradeRegisterTrade,
        out: &mut dyn crate::NoticeSink,
        now: dereth_primitives::ServerTime,
    ) -> bool {
        self.trade.initiator = m.initiator;
        self.trade.partner = m.partner;
        if !self.trade.trade.register(m.partner, m.stamp) {
            return false;
        }
        self.trade.trade.initiator = self.player.is_some_and(|p| p == m.initiator);
        // The register notice makes the trade window visible. `0x01FE OpenTrade` reaches an
        // inherited no-op and is not required for a live negotiation.
        self.trade.open = true;
        out.emit(crate::Notice::TradeRegistered {
            initiator: m.initiator,
            partner: m.partner,
        });
        // Register a **5.0 m** range check on the partner. Crossing that range closes the
        // negotiation.
        self.register_trade_range_check(m.partner, now);
        // `if (partner == attempt_to_player) attempt_to_trade_item(to, object)` -- the
        // deferred half of an item-on-player gesture. The initial request parks the item until the
        // negotiation is registered, then this notice offers it.
        let pending = (self.trade.attempt_to_player == m.partner
            && self.trade.attempt_object.0 != 0)
            .then_some(self.trade.attempt_object);
        self.trade.attempt_object = dereth_primitives::ObjectId(0);
        self.trade.attempt_to_player = dereth_primitives::ObjectId(0);
        if let Some(item) = pending {
            out.emit(crate::Notice::TradeAnItemForDummies(item));
        }
        true
    }

    /// Handle `0x01FE Trade_OpenTrade` by raising the open notice. The retail trade window
    /// registers for this notice but inherits a no-op handler, so it does not change visibility.
    pub fn handle_open_trade(
        &mut self,
        source: dereth_primitives::ObjectId,
        out: &mut dyn crate::NoticeSink,
    ) {
        out.emit(crate::Notice::OpenSecureTrade { source });
    }

    /// Handle inbound `0x01FF Trade_CloseTrade`.
    ///
    /// Print the cancellation line, raise the close notice, reset both lists, and zero both ids.
    /// The window handler also clears the partner name.
    pub fn handle_close_trade(
        &mut self,
        reason: u32,
        now: dereth_primitives::ServerTime,
        out: &mut dyn crate::NoticeSink,
    ) {
        // This is the **first** statement of the handler and is not guarded on `reason`, so every
        // close says it: the
        // partner declining, the partner walking out of range, the window being closed, the
        // partner logging off. Without it a trade that ends says nothing whatsoever.
        out.emit(crate::Notice::DisplayString {
            channel: TRADE_MESSAGE_CHANNEL,
            text: messages::CANCELLED.to_string(),
        });
        self.queue_displayed_partner_contents(now);
        self.trade.trade.reset();
        // Flushes both lists, so nothing the window is
        // showing survives and there is nothing left for a removal to be about.
        self.trade.self_removed.clear();
        self.trade.acceptance_darkened = false;
        self.trade.display_lists = None;
        self.clear_all_trade_states();
        self.trade.initiator = dereth_primitives::ObjectId(0);
        self.trade.partner = dereth_primitives::ObjectId(0);
        // The receive-close path resets the window lists and clears the partner name but makes no
        // visibility change. The close button is the only retail path that takes this window down,
        // so a
        // `0x01FF` off the wire empties the table, clears the partner's name and leaves the window
        // on screen for the player to dismiss.
        //
        // [`TradeSystem::open`] is `TradePanel`'s **visibility** and nothing else; the fact
        // that the negotiation is over is [`TradeSystem::partner`] becoming zero above.
        out.emit(crate::Notice::CloseSecureTrade { reason });
    }

    /// Handle inbound `0x0200 Trade_AddToTrade`.
    ///
    /// ```text
    /// side 1: add to the self list      (trade, iid, 1, pos)
    /// side 2: add to the partner list   (trade, iid, 2, pos)
    /// raise the add-item-to-trade notice (iid, side, pos)
    /// ```
    ///
    /// The partner-side arm adds to the partner list and then runs
    /// `remove_contents_from_destruction_queue` **and** `remove_object_to_be_destroyed` -- the
    /// partner's item is streamed to you while it is on the table and would otherwise be reaped.
    ///
    /// The third dword is the **insert position**, not container properties; see this module's
    /// header.
    pub fn handle_add_to_trade(
        &mut self,
        m: &dereth_protocol::trade::TradeAddToTradeRecv,
        out: &mut dyn crate::NoticeSink,
    ) -> bool {
        let side = TradeListId::from_wire(m.side);
        let pos = m.container_properties as usize;
        let added = self.trade.trade.add_item(side, m.item, pos);
        if let Some(lists) = &mut self.trade.display_lists {
            let list = match side {
                TradeListId::SelfList => Some(&mut lists[0]),
                TradeListId::Partner => Some(&mut lists[1]),
                TradeListId::Undef => None,
            };
            if let Some(list) = list {
                if !list.contains(&m.item) {
                    list.insert(pos.min(list.len()), m.item);
                }
            }
        }
        // A `0x0200` on side 1 is the server putting the id back on your side, so
        // it leaves [`TradeSystem::self_removed`] — that keeps the set and the projected self
        // rows disjoint, including the separate post-0x0208 displayed list.
        // Unconditional on `added`, because refusing a duplicate means
        // the id is *already* on the list, which is the same answer.
        if side == TradeListId::SelfList {
            self.trade.self_removed.retain(|id| *id != m.item);
        }
        if side == TradeListId::Partner {
            self.remove_contents_from_destruction_queue(m.item);
            self.remove_object_to_be_destroyed(m.item);
        }
        // The add-my-item / add-partner-item button states 1 and
        // `0xD`, which are set before every lookup in both functions — see
        // [`TradeSystem::acceptance_darkened`]. Unconditional on `added` for the same reason the
        // `self_removed` retain above it is: the notice is what runs them.
        self.trade.acceptance_darkened = true;
        out.emit(crate::Notice::TradeItemAdded {
            item: m.item,
            side: m.side,
            position: pos,
        });
        added
    }

    /// Handle inbound `0x0201 Trade_RemoveFromTrade`.
    ///
    /// The side is passed through unchanged and **any other value removes nothing**, while the
    /// notice is raised either way because both branches join at the same tail.
    pub fn handle_remove_from_trade(
        &mut self,
        m: &dereth_protocol::trade::TradeRemoveFromTrade,
        now: dereth_primitives::ServerTime,
        out: &mut dyn crate::NoticeSink,
    ) -> bool {
        let side = TradeListId::from_wire(m.side);
        let partner_was_displayed =
            side == TradeListId::Partner && self.displayed_partner_items().contains(&m.item);
        let removed = self.trade.trade.remove_item(m.item, side);
        if let Some(lists) = &mut self.trade.display_lists {
            match side {
                TradeListId::SelfList => lists[0].retain(|id| *id != m.item),
                TradeListId::Partner => lists[1].retain(|id| *id != m.item),
                TradeListId::Undef => {}
            }
        }
        if side == TradeListId::SelfList {
            self.set_trade_state(m.item, 0);
            // **The client's `side == 1` arm**, the
            // first of the client's two callers. It runs
            // on the *notice*, so it is recorded here whether or not the mirror held the row:
            // The client's self-side removal writes the button and the light before it has even looked the row
            // up, and the row it looks up is the **window's**, which may be an optimistic one the
            // mirror never had.
            self.note_removed_from_self_trade(m.item);
        }
        if partner_was_displayed {
            self.queue_partner_contents(m.item, now);
        }
        // Self-side and partner-side removal, same two writes.
        self.trade.acceptance_darkened = true;
        out.emit(crate::Notice::TradeItemRemoved {
            item: m.item,
            side: m.side,
        });
        removed
    }

    /// One entry of [`TradeSystem::self_removed`].
    ///
    /// Kept as one function with two callers rather than two `push`es, so the set's membership
    /// rule lives in one place: an id the server has taken off your side, unless and until a
    /// `0x0200 Trade_AddToTrade` on side 1 puts it back.
    fn note_removed_from_self_trade(&mut self, item: dereth_primitives::ObjectId) {
        if item.0 != 0 && !self.trade.self_removed.contains(&item) {
            self.trade.self_removed.push(item);
        }
    }

    /// Handle inbound `0x0202 Trade_AcceptTrade`.
    ///
    /// ```text
    /// if (source == 0)              trade.accepted = 0
    /// else if (source == player_id) trade.accepted = 1
    /// else                          trade.partner_accepted = 1
    /// raise the accept-trade notice (source)
    /// ```
    ///
    /// The zero arm is the one that is easy to lose: **`source == 0` clears *your* accept**, and
    /// it is not the same thing as a decline.
    pub fn handle_accept_trade(
        &mut self,
        source: dereth_primitives::ObjectId,
        out: &mut dyn crate::NoticeSink,
    ) {
        if source.0 == 0 {
            self.trade.trade.accepted = false;
        } else if self.player == Some(source) {
            self.trade.trade.accepted = true;
        } else {
            self.trade.trade.partner_accepted = true;
        }
        // The receive notice writes the button in every
        // arm (accepted for your own accept, enabled for the server's
        // clear, and the partner arm writes the indicator), so the light stops being darkened.
        self.trade.acceptance_darkened = false;
        out.emit(crate::Notice::TradeAccepted { source });
    }

    /// Handle inbound `0x0203 Trade_DeclineTrade`. The mirror
    /// image, with **no** zero arm: `source == player_id` clears yours, anything else clears the
    /// partner's.
    pub fn handle_decline_trade(
        &mut self,
        source: dereth_primitives::ObjectId,
        out: &mut dyn crate::NoticeSink,
    ) {
        if self.player == Some(source) {
            self.trade.trade.accepted = false;
        } else {
            self.trade.trade.partner_accepted = false;
        }
        // The decline handler has the same shape.
        self.trade.acceptance_darkened = false;
        out.emit(crate::Notice::TradeDeclined { source });
    }

    /// Handle inbound `0x0205 Trade_ResetTrade`: raise the notice, then reset the mirror.
    pub fn handle_reset_trade(
        &mut self,
        source: dereth_primitives::ObjectId,
        now: dereth_primitives::ServerTime,
        out: &mut dyn crate::NoticeSink,
    ) {
        self.queue_displayed_partner_contents(now);
        self.trade.trade.reset();
        self.trade.display_lists = None;
        // See [`Self::handle_close_trade`]. The close path flushes the window's trade lists,
        // which is the window's copy of the same statement.
        self.trade.self_removed.clear();
        self.trade.acceptance_darkened = false;
        self.clear_all_trade_states();
        out.emit(crate::Notice::TradeReset { source });
    }

    /// Handle inbound `0x0207 Trade_TradeFailure`.
    ///
    /// `(iid, **1**)` first -- a failed add is rolled back out of *your* offer,
    /// always side 1 whatever the failure was -- then the trade-failure notice `(iid, reason)`.
    pub fn handle_trade_failure(
        &mut self,
        m: &dereth_protocol::trade::TradeTradeFailure,
        out: &mut dyn crate::NoticeSink,
    ) -> bool {
        let removed = self.trade.trade.remove_item(m.item, TradeListId::SelfList);
        if let Some(lists) = &mut self.trade.display_lists {
            lists[0].retain(|id| *id != m.item);
        }
        self.set_trade_state(m.item, 0);
        // The trade-failure path is the second and last caller of the removal
        // helper. This is the arm that matters most:
        // the refused id was never in `self_list` to begin with, so `remove_item` above answers
        // `false` and nothing else in this build would ever mention it again.
        self.note_removed_from_self_trade(m.item);
        // The same failure path reaches the removal helper and therefore its two writes as well.
        self.trade.acceptance_darkened = true;
        out.emit(crate::Notice::TradeFailure {
            item: m.item,
            reason: m.reason,
        });
        removed
    }

    /// Handle inbound `0x0208 Trade_ClearTradeAcceptance`.
    ///
    /// **The handler itself only raises the notice**; the two flags are cleared by the `0x0202`s
    /// the server sends alongside it. Reproduced exactly, because a client that helpfully cleared
    /// them here would disagree with the server about who has accepted.
    pub fn handle_clear_trade_acceptance(
        &mut self,
        now: dereth_primitives::ServerTime,
        out: &mut dyn crate::NoticeSink,
    ) {
        // The clear-acceptance notice flushes both displayed lists and the optimistic trade markers,
        // darken the lights, preserve the partner and visibility. The mirror above is not
        // touched. Reuse the existing removal carrier to flush pending UI-only rows as well.
        let pending: Vec<ObjectId> = self
            .tables
            .weenies
            .iter()
            .filter(|(_, w)| w.trade_state != 0)
            .map(|(id, _)| id)
            .collect();
        for id in pending {
            self.note_removed_from_self_trade(id);
        }
        self.queue_displayed_partner_contents(now);
        self.clear_all_trade_states();
        self.trade.display_lists = Some([Vec::new(), Vec::new()]);
        self.trade.acceptance_darkened = true;
        out.emit(crate::Notice::TradeAcceptanceCleared);
    }

    // ------------------------------------------------------------------------------------
    // the five senders
    // ------------------------------------------------------------------------------------

    /// Check the ownership half of the window's item-acceptable test.
    ///
    /// ```text
    /// w = the weenie for id; if (!w) return false
    /// if (w is not owned by the player) { if (!silent) display "You can only trade items you are carrying"; return false }
    /// return !(id is in the window's self list)
    /// ```
    ///
    /// `None` means the item may be dropped on the table. The already-in-list half is the window's and
    /// is the panel's; this is the ownership half.
    #[must_use]
    pub fn trade_item_acceptable(&self, item: dereth_primitives::ObjectId) -> Option<&'static str> {
        if self.weenie(item).is_none() {
            return Some(messages::ONLY_CARRIED);
        }
        if self.is_owned_by_player(item) {
            None
        } else {
            Some(messages::ONLY_CARRIED)
        }
    }

    /// Handle the window's partial-stack drop arm.
    ///
    /// Retail asks `(item, player, the item's container id, 0, 0)`
    /// to split the selected quantity beside its source.  Nothing is inserted in the trade list
    /// until the resulting object's attributes identify the expected WCID and stack size.
    pub fn split_item_for_trade(
        &mut self,
        item: ObjectId,
        split: crate::inventory::SplitState,
        now: dereth_primitives::ServerTime,
        out: &mut dyn crate::NoticeSink,
        req: &mut dyn crate::RequestSink,
    ) -> bool {
        if let Some(text) = self.trade_item_acceptable(item) {
            out.emit(crate::Notice::DisplayString {
                channel: TRADE_MESSAGE_CHANNEL,
                text: text.to_owned(),
            });
            return false;
        }
        let Some((container, wcid, name)) = self.weenie(item).map(|w| {
            (
                w.pwd.container_id.unwrap_or_default(),
                w.pwd.wcid,
                w.object_name(crate::weenie::NameType::Appropriate),
            )
        }) else {
            return false;
        };
        let stack_size = self.object_split_size(item, split);
        let Some(player) = self.player else {
            return false;
        };
        if !self
            .attempt_to_place_in_container(req, out, item, player, container, false, 0, split, now)
        {
            out.emit(crate::Notice::DisplayString {
                channel: TRADE_MESSAGE_CHANNEL,
                text: messages::CANNOT_SPLIT.to_owned(),
            });
            return false;
        }
        self.trade.pending_split = Some(PendingTradeSplit {
            source: item,
            wcid,
            stack_size,
        });
        out.emit(crate::Notice::DisplayString {
            channel: TRADE_MESSAGE_CHANNEL,
            text: messages::splitting_before_trading(&name),
        });
        true
    }

    /// Match the attribute-change notice against a pending trade split.
    ///
    /// The notice's bit 0, the saved WCID and `max(stack size, 1)` must all agree.  A match
    /// consumes the pending triple and tells the UI to run its add-item
    /// `(id, 0, false, false, true)`.
    #[must_use]
    pub fn trade_split_item_attributes_changed(&mut self, item: ObjectId, kind: u32) -> bool {
        let Some(pending) = self.trade.pending_split else {
            return false;
        };
        if kind & 1 == 0 {
            return false;
        }
        let matches = self.weenie(item).is_some_and(|w| {
            w.pwd.wcid == pending.wcid
                && u32::from(w.pwd.stack_size.unwrap_or(0).max(1)) == pending.stack_size
        });
        if matches {
            self.trade.pending_split = None;
        }
        matches
    }

    /// Clear the pending split when the server reports that the attempt failed.
    pub fn clear_pending_trade_split(&mut self) {
        self.trade.pending_split = None;
    }

    /// Handle the server's item-moved notice for a partner-side offer.
    ///
    /// A `0x0208` can flush the window while leaving the protocol mirror intact. If the ordinary
    /// item-move reply then says a mirror-known offer entered the trade partner, retail cancels
    /// destruction for the object and its contents and puts the missing row back at the mirror's
    /// position.
    pub fn trade_item_moved_to_partner(
        &mut self,
        item: dereth_primitives::ObjectId,
        container: dereth_primitives::ObjectId,
    ) -> bool {
        if self.trade.partner.0 == 0
            || container != self.trade.partner
            || !self.trade.trade.is_partner_trading_item(item)
            || self.displayed_partner_items().contains(&item)
        {
            return false;
        }
        let position = self
            .trade
            .trade
            .partner_list
            .iter()
            .position(|profile| profile.iid == item)
            .unwrap_or(0);
        let inserted = self.trade.display_lists.as_mut().is_some_and(|lists| {
            if lists[1].contains(&item) {
                return false;
            }
            lists[1].insert(position.min(lists[1].len()), item);
            true
        });
        self.remove_contents_from_destruction_queue(item);
        self.remove_object_to_be_destroyed(item);
        inserted
    }

    /// Add an item or its immediate contents to the player's displayed offer and send each row.
    ///
    /// ```text
    /// the object holds no items and no containers:
    ///     id already in the window's self list -> -1
    ///     set_trade_state(w, 1)
    ///     insert a row for id into the self list at slot
    ///     refresh the item count; update_trade_button_state
    ///     send 0x01F8 add-to-trade (id, the row's position in the list)
    /// ```
    ///
    /// **The second dword of `0x01F8` is the row's position in the window's own list**, which is
    /// why `position` is an argument rather than something this crate can compute: the list is the
    /// panel's.
    ///
    /// The second arm handles a container by walking its item list once:
    ///
    /// ```text
    /// the object holds no items and no containers:
    ///     id already in the window's self list -> -1
    ///     set_trade_state(w, 1)
    ///     insert a row for id into the self list at position
    ///     refresh the item count; update_trade_button_state
    ///     send 0x01F8 add-to-trade (id, the row's position in the list)
    /// with_contents and the object holds items:
    ///     name = the object's name (form 2)
    ///     display "Trading contents of %s" on 0x1A
    ///     for each child in its items list:
    ///         add child (position, arg3, with_contents false, true); if not -1, position = 0
    /// ```
    ///
    /// Three things follow, and each is user-visible:
    ///
    /// * **A container is never itself a row.** The insert is gated on the object holding
    ///   *nothing*, so dropping a full pack on the table trades its contents and not the pack.
    ///   Returning `false` here would make a full pack a dead drop with no message.
    /// * **`"Trading contents of %s"`** is emitted before the contained items are offered. See
    ///   [`messages::trading_contents`].
    /// * **Only the contained-items list is walked**, which is the `items` vector and *not*
    ///   `containers`. A pack whose only contents are side packs has `items == 0` and
    ///   `containers > 0`, so it is neither inserted nor descended into and the drop does nothing.
    ///   That residual behavior is reproduced rather than tidied.
    ///
    /// `position` is passed down and then forced to 0 after the first child that lands.
    pub fn trade_add_item(
        &mut self,
        item: dereth_primitives::ObjectId,
        position: u32,
        out: &mut dyn crate::NoticeSink,
        req: &mut dyn crate::RequestSink,
    ) -> bool {
        // The client's drag-acceptable check, which is above the add and
        // not inside it — so the recursion below does not repeat it, exactly as retail does not.
        if let Some(text) = self.trade_item_acceptable(item) {
            out.emit(crate::Notice::DisplayString {
                channel: TRADE_MESSAGE_CHANNEL,
                text: text.to_string(),
            });
            return false;
        }
        let mut sent = 0u32;
        self.trade_add_item_inner(item, position, true, out, req, &mut sent);
        // Retail's own `return` is the outer row's position, which is `-1` for a full pack whose
        // *contents* all went out — and the drop handler throws it away. The caller here counts
        // requests, so this answers the question the caller is actually asking: did anything reach
        // the wire.
        sent != 0
    }

    /// Apply the single-object arm. Return the position assigned to the row, or `None` for the
    /// client's `-1` result.
    fn trade_add_item_inner(
        &mut self,
        item: dereth_primitives::ObjectId,
        position: u32,
        with_contents: bool,
        out: &mut dyn crate::NoticeSink,
        req: &mut dyn crate::RequestSink,
        sent: &mut u32,
    ) -> Option<u32> {
        // The contained-item and contained-container counts, read once each because
        // the second arm reads the first of them again.
        let (items, containers) = self.inventory(item).map_or((Vec::new(), 0), |inv| {
            (inv.items.clone(), inv.containers.len())
        });
        let mut placed = None;
        if items.is_empty() && containers == 0 {
            // id already in the window's self list -> `return -1`. The window's list is the
            // mirror's self side plus whatever the panel inserted optimistically, and
            // the trade-state marker is what retail's own add-item writes for exactly that row —
            // see [`Self::trade_offered`].
            if self.trade_offered(item) {
                return None;
            }
            // A fresh local offer after a reset is no longer a refused/cleared pending row.
            self.trade.self_removed.retain(|id| *id != item);
            self.set_trade_state(item, 1);
            req.send(crate::Request::TradeAddToTrade(
                dereth_protocol::trade::TradeAddToTrade {
                    item,
                    slot: position,
                },
            ));
            *sent += 1;
            placed = Some(position);
        }
        if with_contents && !items.is_empty() {
            let name = self.weenie(item).map_or_else(String::new, |w| {
                w.object_name(crate::weenie::NameType::Appropriate)
            });
            out.emit(crate::Notice::DisplayString {
                channel: TRADE_MESSAGE_CHANNEL,
                text: messages::trading_contents(&name),
            });
            let mut pos = position;
            for child in items {
                // The add for each child passes the with-contents flag **false** one level
                // down, so the recursion is exactly one deep whatever the
                // nesting is.
                if self
                    .trade_add_item_inner(child, pos, false, out, req, sent)
                    .is_some()
                {
                    pos = 0;
                }
            }
        }
        placed
    }

    /// Answer the window's "is `id` in the self list" query from world state.
    ///
    /// The window's self list is not a thing this crate holds, but every id that
    /// ever entered it did so through the add-item path, whose second statement marks the object
    /// with trade state 1. Every path that takes a row back
    /// out clears it: [`Self::handle_remove_from_trade`]'s side-1 arm,
    /// [`Self::handle_trade_failure`], and [`Self::clear_all_trade_states`] from the mirror
    /// reset and the close-trade and reset-trade receivers. So `trade_state != 0` **is** the
    /// window's list on
    /// your side, including the optimistic rows the `0x0200` has not confirmed, and it is the
    /// marker retail keeps for precisely this purpose rather than a proxy invented here.
    ///
    /// The confirmed displayed list is `or`-ed in so that a row the server put on your side
    /// without a local add still counts. Normally this is the mirror's `self_list`; after
    /// 0x0208 it is the window-only list, because the stale mirror must not refuse a new offer.
    #[must_use]
    pub fn trade_offered(&self, item: dereth_primitives::ObjectId) -> bool {
        self.weenie(item).is_some_and(|w| w.trade_state != 0)
            || self.trade.display_lists.as_ref().map_or_else(
                || self.trade.trade.self_list.iter().any(|c| c.iid == item),
                |lists| lists[0].contains(&item),
            )
    }

    /// Handle the world half of the trade-an-item notice delivered by the window.
    ///
    /// **`Notice::TradeAnItemForDummies` has two producers** —
    /// [`Self::handle_register_trade`]'s deferred arm and
    /// [`Self::attempt_to_trade_item`] — and this is their receiver; without it *drag an item onto
    /// another player* would open the window and then drop the item. That gesture is the player's
    /// `DragItemOnPlayerOpensSecureTrade` character option (index 23 in `hud.rs`).
    ///
    /// ```text
    ///   if (no self list) return false
    ///   if (id is in the self list) return false        // silent
    ///   if (id != selected id
    ///        || split size == max split size) {
    ///       add item (id, 0, true, true, false); return true
    ///           }
    ///   display chat text (0x1A, L"You must split the stack before trading it.")
    ///           return false
    /// ```
    ///
    /// The already-offered test is **above** the split test, so an item already on the table is refused
    /// without a word even when a partial split is in hand; that ordering is kept here.
    #[must_use]
    pub fn trade_an_item_for_dummies(
        &mut self,
        item: dereth_primitives::ObjectId,
        split: crate::inventory::SplitState,
    ) -> ForDummies {
        if self.trade_offered(item) {
            return ForDummies::AlreadyOffered;
        }
        // The selected id with a split size below the maximum — the stack whose selected
        // quantity is smaller than the whole stack. The selected id and `SplitState` carry those
        // values.
        if self.selected == Some(item) && !split.is_whole_stack() {
            return ForDummies::MustSplit;
        }
        ForDummies::Offer
    }

    /// Run the anti-scam comparison and choose the payload it sends.
    ///
    /// The two counts are the **window's**, which is the whole point; see [`accept_decision`].
    /// Both branches send `0x01FA`, and they differ only in the `Trade` they carry: the mirror, or
    /// a default-constructed one. Only the accepting branch sets `accepted`
    /// in the mirror.
    pub fn accept_trade(
        &mut self,
        displayed_self: usize,
        displayed_partner: usize,
        req: &mut dyn crate::RequestSink,
    ) -> AcceptDecision {
        let d = accept_decision(&self.trade.trade, displayed_self, displayed_partner);
        let body = match d {
            AcceptDecision::Accept => {
                self.trade.trade.accepted = true;
                // The accept path writes button state 6.
                self.trade.acceptance_darkened = false;
                self.trade.trade.to_wire()
            }
            // The out-of-sync arm sends a fresh, empty trade.
            AcceptDecision::OutOfSync => Trade::default().to_wire(),
        };
        req.send(crate::Request::TradeAcceptTrade(
            dereth_protocol::trade::TradeAcceptTradeRequest(body),
        ));
        d
    }

    /// Declining writes `accepted = 0`, then sends the decline-trade request (`0x01FB`, empty body).
    pub fn decline_trade(&mut self, req: &mut dyn crate::RequestSink) {
        self.trade.trade.accepted = false;
        // The decline path writes button state 1.
        self.trade.acceptance_darkened = false;
        req.send(crate::Request::TradeDeclineTrade(
            dereth_protocol::trade::TradeDeclineTradeRequest,
        ));
    }

    /// Send the "Clear All Items" request. The body is the
    /// opcode **`0x0204`** alone, which is outside the `0x01F6..0x01FE`
    /// block the rest of the family lives in.
    pub fn reset_trade_request(&mut self, req: &mut dyn crate::RequestSink) {
        req.send(crate::Request::TradeResetTrade(
            dereth_protocol::trade::TradeResetTradeRequest,
        ));
    }

    /// Send `0x01F7 CloseTradeNegotiations` and reset the window's displayed lists. The two retail
    /// producers are a window hide and a partner crossing the 5 m range. This function itself does
    /// not hide the window: the first producer is reached *after* the close button hides it, and
    /// the range-exit producer resets the lists while leaving the empty window visible.
    ///
    /// The reset empties only the **window's** two lists and darkens its acceptance state. Retail
    /// leaves the partner id and authoritative mirror intact until `0x01FF` arrives.
    /// [`TradeSystem::display_lists`] keeps that UI reset separate from the authoritative mirror.
    pub fn close_trade_negotiations(
        &mut self,
        now: dereth_primitives::ServerTime,
        out: &mut dyn crate::NoticeSink,
        req: &mut dyn crate::RequestSink,
    ) {
        req.send(crate::Request::TradeCloseTradeNegotiations(
            dereth_protocol::trade::TradeCloseTradeNegotiations,
        ));
        self.queue_displayed_partner_contents(now);
        self.trade.self_removed.clear();
        self.trade.acceptance_darkened = true;
        self.trade.display_lists = Some([Vec::new(), Vec::new()]);
        self.clear_all_trade_states();
        out.emit(crate::Notice::CloseSecureTrade { reason: 0 });
    }

    /// Handle the trade window's **close button**, `0x1000008B`, the only path that hides it.
    ///
    /// The close-button case first hides the window. Its visibility callback then unregisters the
    /// range handler, calls [`Self::close_trade_negotiations`], and resets the lists. The datagram
    /// is therefore a consequence of the hide. One request either way, so the two are one call
    /// here, in retail order, because the visibility callback cannot run before the flag is written.
    pub fn close_trade_window(
        &mut self,
        now: dereth_primitives::ServerTime,
        out: &mut dyn crate::NoticeSink,
        req: &mut dyn crate::RequestSink,
    ) {
        // The close button writes visibility false before this callback runs.
        self.trade.open = false;
        // Unregister the window's own range handler, which is why a window closed by
        // hand does not then close a second time when the partner walks away.
        self.object_range_checks
            .unregister_all(crate::range::RangeHandler::SecureTrade);
        self.close_trade_negotiations(now, out, req);
    }

    // ------------------------------------------------------------------------------------
    // Trade-state writes shared with inventory and UI projections
    // ------------------------------------------------------------------------------------

    /// Set the object's trade marker to `v`.
    ///
    /// **This is [`crate::Weenie::trade_state`]'s first production writer.** It is read by
    /// the inventory drop and merge checks, by
    /// `use_object`'s give arm, by `dereth_client::cursor`'s target-compatibility test and by the
    /// item widget's overlay.
    pub fn set_trade_state(&mut self, item: dereth_primitives::ObjectId, v: u32) -> bool {
        match self.weenie_mut(item) {
            Some(w) => {
                let changed = w.trade_state != v;
                w.trade_state = v;
                changed
            }
            None => false,
        }
    }

    /// The client's first loop: set the trade state to 0 for every
    /// row of the **self** list. Returns how many objects were cleared.
    ///
    /// Driven from the mirror rather than from the window, because by the time this runs the two
    /// agree -- every path that reaches it (the mirror reset and the close-trade and reset-trade
    /// receivers) has
    /// already flushed or is about to flush both.
    pub fn clear_all_trade_states(&mut self) -> usize {
        let ids: Vec<dereth_primitives::ObjectId> = self
            .tables
            .weenies
            .iter()
            .filter(|(_, w)| w.trade_state != 0)
            .map(|(id, _)| id)
            .collect();
        let n = ids.len();
        for id in ids {
            self.set_trade_state(id, 0);
        }
        n
    }

    /// Return the partner-side rows the window currently holds. A post-`0x0208`
    /// display override is the window; otherwise the protocol mirror is its backing list.
    fn displayed_partner_items(&self) -> Vec<dereth_primitives::ObjectId> {
        self.trade.display_lists.as_ref().map_or_else(
            || {
                self.trade
                    .trade
                    .partner_list
                    .iter()
                    .map(|p| p.iid)
                    .collect()
            },
            |lists| lists[1].clone(),
        )
    }

    /// Shared destruction-queue handling for partner rows flushed or removed from the window.
    fn queue_partner_contents(
        &mut self,
        item: dereth_primitives::ObjectId,
        now: dereth_primitives::ServerTime,
    ) {
        if self.trade.partner.0 == 0 {
            return;
        }
        let eligible = self
            .weenie(item)
            .is_some_and(|w| w.pwd.location.unwrap_or(0) & 0x03F0_0000 == 0)
            && !self.is_owned_by_player(item);
        if eligible {
            self.add_contents_to_destruction_queue(item, now);
        }
    }

    fn queue_displayed_partner_contents(&mut self, now: dereth_primitives::ServerTime) {
        for item in self.displayed_partner_items() {
            self.queue_partner_contents(item, now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RecordingRequests, RecordingSink};

    fn profile(id: u32) -> ContentProfile {
        ContentProfile {
            iid: ObjectId(id),
            container_properties: 0,
        }
    }

    // ------------------------------------------------------------------------------------
    // the six corrections, each pinned against retail
    // ------------------------------------------------------------------------------------

    /// **Corrections 1, 2 and 3.** Registration writes exactly three fields:
    /// `_partner`, `_stamp` and `_status = 2`. It does not flush the lists, does not clear the
    /// acceptances, and does not touch `_initiator`.
    ///
    /// The previous transcription did all four of those, and its own test asserted the wrong
    /// status by reading it back through the same symbol. The literal **2** is stated here.
    #[test]
    fn register_sets_status_open_and_leaves_everything_else_alone() {
        let mut t = Trade::default();
        t.add_item(TradeListId::SelfList, ObjectId(10), 0);
        t.add_item(TradeListId::Partner, ObjectId(20), 0);
        t.accepted = true;
        t.partner_accepted = true;
        t.initiator = true;

        assert!(t.register(ObjectId(0x5000_0002), 12.5));

        assert_eq!(t.status as u32, 2, "registration writes the literal 2");
        assert_eq!(t.status, TradeStatus::Open);
        assert_eq!(t.partner, ObjectId(0x5000_0002));
        assert!((t.stamp - 12.5).abs() < f64::EPSILON);
        assert_eq!(t.self_list.len(), 1, "Register does not flush the lists");
        assert_eq!(t.partner_list.len(), 1);
        assert!(t.accepted && t.partner_accepted, "nor the acceptances");
        assert!(t.initiator, "nor _initiator, which the system writes");

        // A zero partner returns 0 — it registers nothing at all.
        let mut z = Trade::default();
        assert!(!z.register(ObjectId(0), 3.0));
        assert_eq!(z.status, TradeStatus::Undef);
    }

    /// **Correction 4.** The default stamp stores `0xBFF00000` in its high dword and
    /// `_stamp` and `0` in the low one. As a literal: the bit pattern and the value.
    #[test]
    fn a_fresh_trade_starts_with_a_stamp_of_minus_one() {
        assert_eq!(Trade::default().stamp.to_bits(), 0xBFF0_0000_0000_0000);
        assert!((Trade::default().stamp - (-1.0)).abs() < f64::EPSILON);
        assert!((INITIAL_STAMP - (-1.0)).abs() < f64::EPSILON);
        // Zero is a stamp the server may legitimately send, which is why the two must differ.
        assert_ne!(Trade::default().stamp, 0.0);
    }

    /// **Correction 5.** Adding refuses a duplicate, honours the insert position and
    /// caps the list at the literal `0x1A0A`.
    #[test]
    fn add_item_refuses_duplicates_inserts_at_a_position_and_caps_the_list() {
        assert_eq!(MAX_TRADE_LIST, 0x1A0A, "the cmp operand in ");
        assert_eq!(MAX_TRADE_LIST, 6666);

        let mut t = Trade::default();
        assert!(t.add_item(TradeListId::SelfList, ObjectId(10), 0));
        assert!(
            t.add_item(TradeListId::SelfList, ObjectId(11), 0),
            "position 0 = insert first"
        );
        assert_eq!(t.self_list, vec![profile(11), profile(10)]);
        assert!(t.add_item(TradeListId::SelfList, ObjectId(12), 1));
        assert_eq!(t.self_list, vec![profile(11), profile(12), profile(10)]);
        // A position past the end appends, which is the list insert's final branch.
        assert!(t.add_item(TradeListId::SelfList, ObjectId(13), 99));
        assert_eq!(t.self_list.last(), Some(&profile(13)));

        assert!(
            !t.add_item(TradeListId::SelfList, ObjectId(10), 0),
            "Search rejects a duplicate"
        );
        assert_eq!(t.self_list.len(), 4);
        assert!(
            !t.add_item(TradeListId::SelfList, ObjectId(0), 0),
            "iid == 0 adds nothing"
        );
        assert!(
            !t.add_item(TradeListId::Undef, ObjectId(14), 0),
            "no side, no list"
        );

        // The cross-list search prevents the same object from appearing on both sides.
        assert!(!t.add_item(TradeListId::Partner, ObjectId(10), 0));
        assert!(t.partner_list.is_empty());

        // The cap.
        let mut full = Trade::default();
        for i in 0..MAX_TRADE_LIST {
            assert!(full.add_item(
                TradeListId::SelfList,
                ObjectId(u32::try_from(i).expect("a small index") + 1),
                i
            ));
        }
        assert_eq!(full.self_list.len(), MAX_TRADE_LIST);
        assert!(
            !full.add_item(TradeListId::SelfList, ObjectId(0x7FFF_FFFF), 0),
            "the list count < 0x1A0A is the whole guard"
        );
    }

    /// **Correction 6.** Removal searches **one** list, chosen by the side.
    ///
    /// The previous transcription searched both, so a `0x0201` naming the partner's side would
    /// have deleted a same-id entry out of the player's own offer.
    #[test]
    fn remove_item_only_touches_the_side_it_is_given() {
        let mut t = Trade::default();
        t.add_item(TradeListId::SelfList, ObjectId(10), 0);
        t.add_item(TradeListId::Partner, ObjectId(20), 0);

        assert!(
            !t.remove_item(ObjectId(10), TradeListId::Partner),
            "wrong side, no removal"
        );
        assert_eq!(t.self_list.len(), 1);
        assert!(!t.remove_item(ObjectId(20), TradeListId::SelfList));
        assert_eq!(t.partner_list.len(), 1);
        assert!(
            !t.remove_item(ObjectId(10), TradeListId::Undef),
            "side 0 does nothing"
        );
        assert!(
            !t.remove_item(ObjectId(0), TradeListId::SelfList),
            "iid 0 does nothing"
        );

        assert!(t.remove_item(ObjectId(10), TradeListId::SelfList));
        assert!(t.self_list.is_empty());
        assert!(t.remove_item(ObjectId(20), TradeListId::Partner));
        assert!(t.partner_list.is_empty());
    }

    /// The wire values of both enums, as literals — `TradeStatus` skips 3, and `side` is 1/2.
    #[test]
    fn the_status_enum_skips_three_and_the_sides_are_one_and_two() {
        assert_eq!(TradeStatus::Undef as u32, 0);
        assert_eq!(TradeStatus::Pending as u32, 1);
        assert_eq!(TradeStatus::Open as u32, 2);
        assert_eq!(TradeStatus::WaitingToClose as u32, 4);
        assert_eq!(TradeListId::SelfList as u32, 1);
        assert_eq!(TradeListId::Partner as u32, 2);
        assert_eq!(TradeListId::from_wire(1), TradeListId::SelfList);
        assert_eq!(TradeListId::from_wire(2), TradeListId::Partner);
        assert_eq!(TradeListId::from_wire(0), TradeListId::Undef);
        assert_eq!(
            TradeListId::from_wire(3),
            TradeListId::Undef,
            "any other value is no list"
        );
    }

    /// Reset both lists and both acceptances while preserving the registration.
    #[test]
    fn reset_keeps_the_registration() {
        let mut t = Trade::default();
        t.register(ObjectId(0x5000_0002), 7.0);
        t.add_item(TradeListId::SelfList, ObjectId(10), 0);
        t.accepted = true;
        t.partner_accepted = true;
        t.reset();
        assert!(t.self_list.is_empty() && t.partner_list.is_empty());
        assert!(!t.accepted && !t.partner_accepted);
        assert_eq!(t.partner, ObjectId(0x5000_0002));
        assert_eq!(t.status, TradeStatus::Open);
        assert!((t.stamp - 7.0).abs() < f64::EPSILON);
    }

    /// The four counters and the two sums the accept path actually compares.
    ///
    /// **Every profile the client stores carries `container_properties == 0`**, because
    /// constructing `ContentProfile(iid)` writes zero and nothing else writes it —
    /// so `num_containers` is structurally 0 and `num_self_objects` is the list length.
    /// The filters are kept as the client writes them, and this asserts both facts.
    #[test]
    fn the_counters_split_on_container_properties_which_is_always_zero() {
        let mut t = Trade::default();
        t.add_item(TradeListId::SelfList, ObjectId(10), 0);
        t.add_item(TradeListId::SelfList, ObjectId(11), 1);
        t.add_item(TradeListId::Partner, ObjectId(20), 0);
        assert!(t.self_list.iter().all(|c| c.container_properties == 0));
        assert_eq!(t.num_items(), 2);
        assert_eq!(t.num_containers(), 0);
        assert_eq!(t.num_partner_items(), 1);
        assert_eq!(t.num_partner_containers(), 0);
        assert_eq!(t.num_self_objects(), 2);
        assert_eq!(t.num_partner_objects(), 1);
        assert!(t.is_partner_trading_item(ObjectId(20)));
        assert!(!t.is_partner_trading_item(ObjectId(10)));
        assert!(!t.both_accepted());
        t.accepted = true;
        t.partner_accepted = true;
        assert!(t.both_accepted());

        // A `Trade` unpacked off the wire *can* carry a non-zero one, and then the split matters.
        let mut w = Trade::default();
        w.self_list.push(ContentProfile {
            iid: ObjectId(30),
            container_properties: 1,
        });
        assert_eq!(w.num_items(), 0);
        assert_eq!(w.num_containers(), 1);
        assert_eq!(w.num_self_objects(), 1);
    }

    /// The button update is a **transition**, not a function of the model: it
    /// never writes 6, and from `Disabled` it can only reach `Enabled`.
    #[test]
    fn the_button_transition_never_reaches_accepted_on_its_own() {
        assert_eq!(TradeButtonState::Disabled as u32, 0x0D);
        assert_eq!(TradeButtonState::Enabled as u32, 1);
        assert_eq!(TradeButtonState::Accepted as u32, 6);

        use TradeButtonState::{Accepted, Disabled, Enabled};
        assert_eq!(update_trade_button_state(Accepted, 0), Disabled);
        assert_eq!(update_trade_button_state(Enabled, 0), Disabled);
        assert_eq!(update_trade_button_state(Disabled, 3), Enabled);
        assert_eq!(update_trade_button_state(Disabled, 0), Disabled);
        assert_eq!(
            update_trade_button_state(Enabled, 3),
            Enabled,
            "no arm fires"
        );
        assert_eq!(
            update_trade_button_state(Accepted, 3),
            Accepted,
            "an accepted button with items on the table stays accepted"
        );
    }

    // ------------------------------------------------------------------------------------
    // the anti-scam comparison
    // ------------------------------------------------------------------------------------

    /// Exercise the client's complete anti-scam comparison.
    #[test]
    fn accepting_requires_the_displayed_lists_to_agree_with_the_mirror() {
        let mut t = Trade::default();
        t.add_item(TradeListId::SelfList, ObjectId(10), 0);
        t.add_item(TradeListId::Partner, ObjectId(20), 0);
        assert_eq!(accept_decision(&t, 1, 1), AcceptDecision::Accept);
        assert_eq!(accept_decision(&t, 2, 1), AcceptDecision::OutOfSync);
        assert_eq!(accept_decision(&t, 1, 0), AcceptDecision::OutOfSync);
        assert_eq!(accept_decision(&t, 0, 0), AcceptDecision::OutOfSync);
    }

    // ------------------------------------------------------------------------------------
    // the system on the world
    // ------------------------------------------------------------------------------------

    const ME: ObjectId = ObjectId(0x5000_0001);
    const PARTNER: ObjectId = ObjectId(0x5000_0002);
    const ITEM: ObjectId = ObjectId(0x8000_0010);

    fn world_with_player() -> crate::World {
        let mut w = crate::World::new();
        w.player = Some(ME);
        w
    }

    fn register(w: &mut crate::World) {
        let mut out = RecordingSink::default();
        w.handle_register_trade(
            &dereth_protocol::trade::TradeRegisterTrade {
                initiator: ME,
                partner: PARTNER,
                stamp: 4.25,
            },
            &mut out,
            dereth_primitives::ServerTime(0.0),
        );
    }

    /// `0x01FD` — the ids, the mirror, and the `_initiator` flag the *system* writes.
    #[test]
    fn a_register_trade_message_opens_the_mirror() {
        let mut w = world_with_player();
        let mut out = RecordingSink::default();
        assert!(w.handle_register_trade(
            &dereth_protocol::trade::TradeRegisterTrade {
                initiator: ME,
                partner: PARTNER,
                stamp: 4.25
            },
            &mut out,
            dereth_primitives::ServerTime(0.0),
        ));
        assert_eq!(w.trade.initiator, ME);
        assert_eq!(w.trade.partner, PARTNER);
        assert_eq!(w.trade.trade.status, TradeStatus::Open);
        assert!((w.trade.trade.stamp - 4.25).abs() < f64::EPSILON);
        assert!(w.trade.trade.initiator, "the initiator is me");
        assert!(matches!(out.0[0], crate::Notice::TradeRegistered { .. }));

        // …and when it is not.
        let mut w = world_with_player();
        let mut out = RecordingSink::default();
        w.handle_register_trade(
            &dereth_protocol::trade::TradeRegisterTrade {
                initiator: PARTNER,
                partner: PARTNER,
                stamp: 1.0,
            },
            &mut out,
            dereth_primitives::ServerTime(0.0),
        );
        assert!(!w.trade.trade.initiator);
    }

    /// `0x0202`'s three arms, including the one that is easy to lose: **`source == 0` clears your
    /// own acceptance**, and is not a decline.
    #[test]
    fn accept_trade_from_the_server_has_three_arms() {
        let mut w = world_with_player();
        let mut out = RecordingSink::default();
        register(&mut w);

        w.handle_accept_trade(ME, &mut out);
        assert!(w.trade.trade.accepted && !w.trade.trade.partner_accepted);
        w.handle_accept_trade(PARTNER, &mut out);
        assert!(w.trade.trade.both_accepted());
        w.handle_accept_trade(ObjectId(0), &mut out);
        assert!(!w.trade.trade.accepted, "source 0 clears mine");
        assert!(w.trade.trade.partner_accepted, "and only mine");

        // `0x0203` has no zero arm at all: anything that is not me is the partner.
        w.handle_decline_trade(PARTNER, &mut out);
        assert!(!w.trade.trade.partner_accepted);
        w.handle_accept_trade(ME, &mut out);
        w.handle_decline_trade(ME, &mut out);
        assert!(!w.trade.trade.accepted);
        w.handle_accept_trade(ME, &mut out);
        w.handle_decline_trade(ObjectId(0), &mut out);
        assert!(
            w.trade.trade.accepted,
            "0x0203 with source 0 is NOT me, so it clears the partner's flag, not mine"
        );
    }

    /// `0x0208 Trade_ClearTradeAcceptance` raises its notice and **changes no mirror flag**. A client that helpfully
    /// cleared them here would disagree with the server about who has accepted.
    #[test]
    fn clear_trade_acceptance_preserves_the_mirror_and_resets_displayed_lists() {
        let mut w = world_with_player();
        let mut out = RecordingSink::default();
        register(&mut w);
        w.handle_accept_trade(ME, &mut out);
        w.handle_accept_trade(PARTNER, &mut out);
        out.0.clear();
        w.handle_clear_trade_acceptance(dereth_primitives::ServerTime(0.0), &mut out);
        assert!(
            w.trade.trade.both_accepted(),
            "the protocol handler clears no mirror flag"
        );
        assert_eq!(w.trade.display_lists, Some([Vec::new(), Vec::new()]));
        assert!(
            w.trade.acceptance_darkened,
            "the notice receiver darkens the window"
        );
        assert_eq!(out.0.len(), 1);
        assert!(matches!(out.0[0], crate::Notice::TradeAcceptanceCleared));
    }

    #[test]
    fn post_clear_trade_rows_follow_removal_and_failure_not_the_stale_mirror() {
        let mut w = world_with_player();
        let mut out = RecordingSink::default();
        register(&mut w);
        w.handle_clear_trade_acceptance(dereth_primitives::ServerTime(0.0), &mut out);
        for side in [1, 2] {
            w.handle_add_to_trade(
                &dereth_protocol::trade::TradeAddToTradeRecv {
                    item: ObjectId(side),
                    side,
                    container_properties: 0,
                },
                &mut out,
            );
        }
        assert_eq!(
            w.trade.display_lists,
            Some([vec![ObjectId(1)], vec![ObjectId(2)]])
        );
        w.handle_remove_from_trade(
            &dereth_protocol::trade::TradeRemoveFromTrade {
                item: ObjectId(2),
                side: 2,
            },
            dereth_primitives::ServerTime(0.0),
            &mut out,
        );
        w.handle_trade_failure(
            &dereth_protocol::trade::TradeTradeFailure {
                item: ObjectId(1),
                reason: 0,
            },
            &mut out,
        );
        assert_eq!(w.trade.display_lists, Some([Vec::new(), Vec::new()]));
        w.handle_reset_trade(ME, dereth_primitives::ServerTime(0.0), &mut out);
        assert_eq!(
            w.trade.display_lists, None,
            "a real mirror reset retires the display override"
        );
    }

    /// The item-move handler exits before either destruction-queue call when the
    /// partner row already exists. Once the window is flushed, the same mirror-known move takes
    /// the missing-row arm and unreaps the container and its contents.
    #[test]
    fn a_partner_move_unreaps_only_a_missing_display_row() {
        let mut w = world_with_player();
        let mut out = RecordingSink::default();
        register(&mut w);
        let child = ObjectId(0x8000_0011);
        w.tables.weenies.insert(ITEM, crate::Weenie::new(ITEM));
        let mut child_weenie = crate::Weenie::new(child);
        child_weenie.pwd.container_id = Some(ITEM);
        w.tables.weenies.insert(child, child_weenie);
        w.tables
            .inventories
            .insert(ITEM, crate::objects::ObjectInventory::new(ITEM));
        w.tables
            .inventories
            .get_mut(ITEM)
            .expect("container")
            .add_content(child, false, 0);
        w.handle_add_to_trade(
            &dereth_protocol::trade::TradeAddToTradeRecv {
                item: ITEM,
                side: 2,
                container_properties: 0,
            },
            &mut out,
        );

        w.schedule_destroy(child, dereth_primitives::ServerTime(1.0));
        assert!(
            !w.trade_item_moved_to_partner(ITEM, PARTNER),
            "the row already exists"
        );
        assert!(
            w.tables.doomed.contains_key(child),
            "the early return preserves its deadline"
        );

        w.handle_clear_trade_acceptance(dereth_primitives::ServerTime(2.0), &mut out);
        assert!(
            w.trade_item_moved_to_partner(ITEM, PARTNER),
            "the flushed row is missing"
        );
        assert!(
            !w.tables.doomed.contains_key(child),
            "the missing-row arm unreaps contents"
        );
        assert_eq!(w.trade.display_lists, Some([Vec::new(), vec![ITEM]]));
    }

    /// `0x0200`'s third dword is an insert position: two items added at position 0 come back in
    /// reverse order, which a build that treated it as container properties could not produce.
    #[test]
    fn the_third_dword_of_add_to_trade_is_an_insert_position() {
        let mut w = world_with_player();
        let mut out = RecordingSink::default();
        register(&mut w);
        let add = |w: &mut crate::World, out: &mut RecordingSink, id: u32, side: u32, pos: u32| {
            w.handle_add_to_trade(
                &dereth_protocol::trade::TradeAddToTradeRecv {
                    item: ObjectId(id),
                    side,
                    container_properties: pos,
                },
                out,
            )
        };
        assert!(add(&mut w, &mut out, 0x10, 1, 0));
        assert!(add(&mut w, &mut out, 0x11, 1, 0));
        // A **non-zero** position, without which a build that ignored the dword entirely would
        // produce the same list and this test could not tell the two apart.
        assert!(add(&mut w, &mut out, 0x12, 1, 1));
        assert_eq!(
            w.trade
                .trade
                .self_list
                .iter()
                .map(|c| c.iid.0)
                .collect::<Vec<_>>(),
            vec![0x11, 0x12, 0x10]
        );
        assert!(w
            .trade
            .trade
            .self_list
            .iter()
            .all(|c| c.container_properties == 0));
        // Side 2 is the partner's.
        assert!(add(&mut w, &mut out, 0x20, 2, 0));
        assert_eq!(w.trade.trade.num_partner_objects(), 1);
        // Any other side adds nothing but still raises the notice.
        out.0.clear();
        assert!(!add(&mut w, &mut out, 0x30, 7, 0));
        assert_eq!(
            out.0.len(),
            1,
            "the add-item-to-trade notice is unconditional"
        );
    }

    /// `0x0207 Trade_TradeFailure` rolls the item back out of **side 1**, whatever it was, and
    /// clears its trade state.
    #[test]
    fn a_trade_failure_rolls_the_item_out_of_your_own_offer() {
        let mut w = world_with_player();
        let mut out = RecordingSink::default();
        register(&mut w);
        w.handle_add_to_trade(
            &dereth_protocol::trade::TradeAddToTradeRecv {
                item: ITEM,
                side: 1,
                container_properties: 0,
            },
            &mut out,
        );
        assert!(w.handle_trade_failure(
            &dereth_protocol::trade::TradeTradeFailure {
                item: ITEM,
                reason: 9
            },
            &mut out,
        ));
        assert!(w.trade.trade.self_list.is_empty());
    }

    /// Adding an item marks it and removing it unmarks it.
    #[test]
    fn adding_an_item_marks_it_and_removing_it_unmarks_it() {
        let mut w = world_with_player();
        let mut out = RecordingSink::default();
        let mut req = RecordingRequests::default();
        register(&mut w);
        // A weenie the player owns.
        w.tables.weenies.insert(
            ITEM,
            crate::weenie::Weenie {
                id: ITEM,
                ..Default::default()
            },
        );
        if let Some(x) = w.weenie_mut(ITEM) {
            x.pwd.container_id = Some(ME);
        }

        assert!(
            w.trade_add_item(ITEM, 0, &mut out, &mut req),
            "the item is carried"
        );
        assert_eq!(
            w.weenie(ITEM).expect("exists").trade_state,
            1,
            "the trade state is set to 1"
        );
        assert_eq!(req.0.len(), 1);
        assert!(matches!(req.0[0], crate::Request::TradeAddToTrade(_)));

        w.handle_remove_from_trade(
            &dereth_protocol::trade::TradeRemoveFromTrade {
                item: ITEM,
                side: 1,
            },
            dereth_primitives::ServerTime(0.0),
            &mut out,
        );
        assert_eq!(w.weenie(ITEM).expect("exists").trade_state, 0);

        // …and the whole-list clear, which is the client's first loop.
        w.set_trade_state(ITEM, 1);
        assert_eq!(w.clear_all_trade_states(), 1);
        assert_eq!(w.weenie(ITEM).expect("exists").trade_state, 0);
        assert_eq!(w.clear_all_trade_states(), 0, "nothing left to clear");
    }

    /// The ownership check refuses an item the player is not carrying, with the
    /// client's own literal string, and sends nothing.
    #[test]
    fn an_item_the_player_is_not_carrying_is_refused_with_the_clients_own_words() {
        let mut w = world_with_player();
        let mut out = RecordingSink::default();
        let mut req = RecordingRequests::default();
        w.tables.weenies.insert(
            ITEM,
            crate::weenie::Weenie {
                id: ITEM,
                ..Default::default()
            },
        );

        assert_eq!(w.trade_item_acceptable(ITEM), Some(messages::ONLY_CARRIED));
        assert!(!w.trade_add_item(ITEM, 0, &mut out, &mut req));
        assert_eq!(req.0.len(), 0, "nothing leaves the machine");
        assert_eq!(
            out.0,
            vec![crate::Notice::DisplayString {
                channel: 0x1A,
                text: "You can only trade items you are carrying".to_string(),
            }]
        );
        assert_eq!(w.weenie(ITEM).expect("exists").trade_state, 0);
        // An object the tables do not hold at all is refused the same way.
        assert_eq!(
            w.trade_item_acceptable(ObjectId(0x8000_9999)),
            Some(messages::ONLY_CARRIED)
        );
    }

    /// The three refusal strings, as retail's own literals. Nothing else states them.
    #[test]
    fn the_three_refusal_strings_are_the_clients_own() {
        assert_eq!(
            messages::ONLY_CARRIED,
            "You can only trade items you are carrying"
        );
        assert_eq!(
            messages::MUST_SPLIT,
            "You must split the stack before trading it."
        );
        assert_eq!(messages::CANNOT_SPLIT, "Cannot split the stack to trade it");
        assert_eq!(TRADE_MESSAGE_CHANNEL, 0x1A);
    }

    /// **The out-of-sync fallback, which is the part a rebuild that drops it can be robbed over.**
    ///
    /// Both branches send `0x01FA`. The accepting one carries the mirror and sets `_accepted`; the
    /// refusing one carries a **default-constructed** `Trade` — empty lists, no partner, stamp
    /// `-1.0` — and sets nothing.
    #[test]
    fn accepting_out_of_sync_sends_an_empty_trade_and_does_not_set_accepted() {
        let mut w = world_with_player();
        let mut out = RecordingSink::default();
        register(&mut w);
        w.handle_add_to_trade(
            &dereth_protocol::trade::TradeAddToTradeRecv {
                item: ITEM,
                side: 1,
                container_properties: 0,
            },
            &mut out,
        );

        // The window shows two rows where the mirror holds one.
        let mut req = RecordingRequests::default();
        assert_eq!(w.accept_trade(2, 0, &mut req), AcceptDecision::OutOfSync);
        assert!(
            !w.trade.trade.accepted,
            "the out-of-sync notice does not accept"
        );
        let crate::Request::TradeAcceptTrade(sent) = &req.0[0] else {
            panic!("0x01FA")
        };
        assert!(sent.0.self_list.is_empty() && sent.0.partner_list.is_empty());
        assert_eq!(sent.0.partner, ObjectId(0));
        assert_eq!(sent.0.stamp.to_bits(), 0xBFF0_0000_0000_0000);

        // …and the agreeing case carries the mirror.
        let mut req = RecordingRequests::default();
        assert_eq!(w.accept_trade(1, 0, &mut req), AcceptDecision::Accept);
        assert!(w.trade.trade.accepted);
        let crate::Request::TradeAcceptTrade(sent) = &req.0[0] else {
            panic!("0x01FA")
        };
        assert_eq!(sent.0.self_list.len(), 1);
        assert_eq!(sent.0.partner, PARTNER);
        assert_eq!(sent.0.accepted, 1);
        assert_eq!(sent.0.status, 2);
    }

    /// The three empty-bodied senders, and the one thing about them worth stating: **reset is
    /// `0x0204`**, not a member of the `0x01F6..0x01FE` block.
    #[test]
    fn the_empty_senders_carry_the_opcodes_their_bytes_spell() {
        use dereth_protocol::Message;
        let mut w = world_with_player();
        let mut out = RecordingSink::default();
        let mut req = RecordingRequests::default();
        register(&mut w);
        w.handle_add_to_trade(
            &dereth_protocol::trade::TradeAddToTradeRecv {
                item: ITEM,
                side: 1,
                container_properties: 0,
            },
            &mut out,
        );
        w.handle_accept_trade(ME, &mut out);
        w.handle_accept_trade(PARTNER, &mut out);

        w.decline_trade(&mut req);
        w.handle_accept_trade(ME, &mut out);
        w.reset_trade_request(&mut req);
        w.close_trade_negotiations(dereth_primitives::ServerTime(0.0), &mut out, &mut req);
        assert_eq!(req.0.len(), 3);
        assert!(matches!(req.0[0], crate::Request::TradeDeclineTrade(_)));
        assert!(matches!(req.0[1], crate::Request::TradeResetTrade(_)));
        assert!(matches!(
            req.0[2],
            crate::Request::TradeCloseTradeNegotiations(_)
        ));
        assert_eq!(
            dereth_protocol::trade::TradeDeclineTradeRequest::OPCODE.0,
            0x01FB
        );
        assert_eq!(
            dereth_protocol::trade::TradeResetTradeRequest::OPCODE.0,
            0x0204
        );
        assert_eq!(
            dereth_protocol::trade::TradeCloseTradeNegotiations::OPCODE.0,
            0x01F7
        );
        assert_eq!(dereth_protocol::trade::TradeAddToTrade::OPCODE.0, 0x01F8);
        assert_eq!(
            dereth_protocol::trade::TradeAcceptTradeRequest::OPCODE.0,
            0x01FA
        );

        // The close sender has no side effects beyond sending the request. Both callers pair it
        // with a displayed-list reset; neither operation is a
        // `SetVisible`**, so the displayed lists are flushed, the mirror remains live until
        // `0x01FF`, and the window stays up.
        assert!(
            w.trade.open,
            "both close-sender callers send and flush; they do not hide"
        );
        assert_eq!(w.trade.display_lists, Some([Vec::new(), Vec::new()]));
        assert_eq!(
            w.trade
                .trade
                .self_list
                .iter()
                .map(|p| p.iid)
                .collect::<Vec<_>>(),
            vec![ITEM]
        );
        assert!(w.trade.trade.both_accepted());
        assert!(w.trade.acceptance_darkened);

        // The close button is the one path that hides it; that visibility change raises the send.
        w.close_trade_window(dereth_primitives::ServerTime(0.0), &mut out, &mut req);
        assert!(!w.trade.open);
        assert!(matches!(
            req.0[3],
            crate::Request::TradeCloseTradeNegotiations(_)
        ));
        assert_eq!(req.0.len(), 4, "one request either way");
    }

    /// A close notice zeroes both ids and resets the mirror; ending the character session discards
    /// the whole system.
    #[test]
    fn closing_and_ending_the_session_both_clear_the_system() {
        let mut w = world_with_player();
        let mut out = RecordingSink::default();
        register(&mut w);
        w.handle_open_trade(PARTNER, &mut out);
        assert!(w.trade.open);
        w.handle_close_trade(3, dereth_primitives::ServerTime(0.0), &mut out);
        assert_eq!(w.trade.partner, ObjectId(0));
        assert_eq!(w.trade.initiator, ObjectId(0));
        assert!(w.trade.open, "a 0x01FF leaves an empty window on screen");

        register(&mut w);
        w.trade.end_character_session();
        assert_eq!(w.trade, TradeSystem::default());
        assert_eq!(w.trade.trade.stamp.to_bits(), 0xBFF0_0000_0000_0000);
    }
}
