//! `LinkStatusPanel` — the panel the connection lamp opens, and **both halves of the ping**.
//!
//! Element type `0x1000001D`,
//! instance `0x10000187`, one child: the main text `0x10000169`, a `TextElement`
//! [measured on the shipped layout].
//!
//! # The pair, and why neither half worked alone
//!
//! `0x01EA Character_ReturnPing` is an answer to a request the retail client makes only while
//! this panel is open, which is why recorded sessions rarely carry it. Both halves live in this one
//! panel:
//!
//! * **The update's tail** — unless a ping has been asked for, or 120 seconds have passed since
//!   the last request, it stops; otherwise it records the request time, sends event `0x01E9` and
//!   clears the asked-for flag.
//! * **The ping notice** — the round trip is the current time minus the last request time; if
//!   the panel is visible and its next-update time has come, it pushes that five seconds on and
//!   updates.
//! * **The element-message handler** — `0x18` on the panel with `param1 != 0` registers for
//!   global message 3, asks for a ping and updates; with `param1 == 0` it unregisters and clears
//!   the ask.
//! * **The global-message handler** — on message 3, if visible and the next-update time has
//!   come, it pushes that five seconds on and updates.
//!
//! So **opening the panel is what sends the ping**, and nothing else in the client ever does:
//! this panel is the only producer of `dereth_protocol::admin::CharacterRequestPing` and the
//! only receiver of `0x01EA`.
//!
//! # The five lines, and which of them are live
//!
//! The update composes one `StringInfo` per line into the main text, in this order — the tokens and
//! their literal pieces read out of the shipped dats:
//!
//! | line | token | source | live? |
//! |---|---|---|---|
//! | what the lamp means | `ID_LinkStatus_Info` | constant | yes |
//! | what the colours mean | `ID_LinkStatus_Colors` | constant | yes |
//! | the forty-second drop | `ID_LinkStatus_Disconnect` | constant | yes |
//! | packet loss, last 10 s | `ID_LinkStatus_PacketLoss` | the packet-loss query | yes |
//! | round-trip ping | `ID_LinkStatus_Ping` | the `0x01E9` / `0x01EA` pair | yes |
//!
//! The packet-loss query reads a cached value. The heartbeat handler refreshes it
//! from the average-loss accumulator on each two-second snapshot. See
//! `dereth_client_net::linkstatus` for the arithmetic, which is
//! `2 * (NAKed + retransmitted) / (received + sent)` over a **forty-heartbeat** ring — the
//! string's *"last 10 sec"* is not the window.
//!
//! Two things that are easy to get wrong here:
//!
//! * **There is no `????` on this line, ever.** The update unconditionally formats
//!   packet loss as a float with two decimal places. The four-question-mark literal
//!   belongs only to the **ping** line's round-trip `<= 0.0` arm; retail always shows a number
//!   on the loss line.
//! * **The value before the first heartbeat is `1.00`, not zero.**
//!   Initialization writes `1.0f` to the cached packet-loss field. On
//!   the ratio reading that is 100 % loss, which is the honest state of a link nothing has been
//!   heard on, and it is what retail prints for the first two seconds of every session.
//!
//! # `????`
//!
//! The update branches on the round trip being `<= 0.0`. A positive value is multiplied
//! by 1000 and formatted as milliseconds with zero decimal places. Otherwise it uses
//! **`"????"`**, four question marks rather than three. The request gate's interval is
//! 120 seconds.

use dereth_ui::{ElemHandle, ElementId, UiSystem};

use crate::view::{GameView, UiRequest};

/// The `LinkStatusPanel` element — the `0x57` listener for input action `0x10000009`
/// *"Show/Hide Link Status Panel"*, which the lamp at `0x100000F8` fires.
pub const PANEL: ElementId = ElementId(0x1000_0187);

/// The main text — the one child the post-init binds.
pub const MAIN_TEXT: ElementId = ElementId(0x1000_0169);

/// What the ping line shows before the first `0x01EA` comes back.
///
/// **Four** question marks.
pub const UNKNOWN: &str = "????";

/// The client's request gate is `120.0` seconds. A panel that stays
/// open re-pings every two minutes.
pub const PING_INTERVAL: f64 = 120.0;

/// The cached packet loss's initial value — `1.0f`.
///
/// Defined in [`dereth_client_contract::linkstatus`], because
/// `GameView::packet_loss_percent`'s default body is this number and the contract crate may not
/// depend on this one.
pub use dereth_client_contract::linkstatus::INITIAL_PACKET_LOSS;

/// The packet-loss float variable's precision — two decimal places.
pub const PACKET_LOSS_PRECISION: usize = 2;

/// The client's throttle — the panel redraws at most every five seconds
/// off the frame tick.
pub const REDRAW_INTERVAL: f64 = 5.0;

/// The five `StringInfo` tokens the update composes, in order. Table enum `0x10000001`.
pub mod string {
    pub const INFO: &str = "ID_LinkStatus_Info";
    pub const COLORS: &str = "ID_LinkStatus_Colors";
    pub const DISCONNECT: &str = "ID_LinkStatus_Disconnect";
    /// `%PacketLoss`, a float.
    pub const PACKET_LOSS: &str = "ID_LinkStatus_PacketLoss";
    /// `%Ping` — a float in milliseconds, or `UNKNOWN` as a string.
    pub const PING: &str = "ID_LinkStatus_Ping";
}

/// The two **variable names** the update files its values under.
///
/// Neither is hashed in this panel's own code: both ids are computed once at startup, by
/// hashing "PACKET_LOSS" and "PING".
///
/// The panel's update adds the packet-loss id as a float variable, and the ping id as a
/// string variable on the unknown arm and a float variable on the other.
/// The string hash of `"PING"` is `0x00054E27` and that of `"PACKET_LOSS"` is
/// `0x0AD86373` -- exactly the ids the two shipped rows carry in their variable lists
/// [measured against the shipped string table].
pub mod var {
    /// The ping variable, the hash of "PING": `0x00054E27`.
    pub const PING: &str = "PING";
    /// The packet-loss variable, the hash of "PACKET_LOSS": `0x0AD86373`.
    pub const PACKET_LOSS: &str = "PACKET_LOSS";
}

/// `LinkStatusPanel`.
#[derive(Debug, Default)]
pub struct LinkStatusPanel {
    /// The panel element. `None` when the layout has no such sub-panel.
    pub panel: Option<ElemHandle>,
    main_text: Option<ElemHandle>,
    /// The ping round trip, **seconds**. `<= 0.0` means no answer yet.
    pub ping_round_trip: f64,
    /// When the last ping was requested.
    pub last_ping_request: f64,
    /// Whether a ping is asked for — set by the `0x18` show arm, cleared by the send.
    pub please_request_ping: bool,
    /// When the panel next redraws.
    next_update: f64,
    /// How many `0x01EA` arrivals this panel has already accounted for.
    seen_returns: u64,
    /// How many `0x01E9` requests this panel has emitted — the request half, countable without
    /// sending anything.
    pub pings_requested: u32,
    /// What the main text last read.
    pub text: String,
    /// How many times [`Self::write`] has written.
    pub updates: u32,
    was_visible: bool,
}

impl LinkStatusPanel {
    /// The link-status panel's post-init, minus its one notice-handler registration
    /// (the ping notice) — this build has no notice bus, and that handler's body is the
    /// `seen_returns` edge in [`Self::update`].
    pub fn post_init(&mut self, ui: &mut UiSystem, root: ElemHandle) {
        self.panel = ui.get_child_recursive(root, PANEL);
        let Some(p) = self.panel else { return };
        self.main_text = ui.get_child_recursive(p, MAIN_TEXT);
        self.was_visible = self.visible(ui);
    }

    #[must_use]
    pub fn bound(&self) -> bool {
        self.panel.is_some()
    }

    #[must_use]
    pub fn fully_bound(&self) -> bool {
        self.panel.is_some() && self.main_text.is_some()
    }

    #[must_use]
    pub fn visible(&self, ui: &UiSystem) -> bool {
        self.panel
            .and_then(|h| ui.node(h))
            .is_some_and(|n| n.region.flags.visible)
    }

    /// One frame's drive — the client's `0x18` arms and
    /// its five-second tick, folded into the poll this crate's
    /// panels use (see [`super::effects::EffectsPanel::update`] for why).
    ///
    /// Returns true on a frame that rewrote the text.
    pub fn update(&mut self, ui: &mut UiSystem, view: &dyn GameView) -> bool {
        if !self.fully_bound() {
            return false;
        }
        let visible = self.visible(ui);
        let shown_now = visible && !self.was_visible;
        let hidden_now = !visible && self.was_visible;
        self.was_visible = visible;
        if hidden_now {
            // Unregister from the tick and clear the ask.
            self.please_request_ping = false;
        }
        if !visible {
            return false;
        }
        let now = view.now();
        // The answer. Accounted before the redraw so that the
        // frame a `0x01EA` lands on is the frame the number appears.
        let returns = view.ping_returns();
        let answered = returns != self.seen_returns;
        if answered {
            self.seen_returns = returns;
            self.ping_round_trip = now - self.last_ping_request;
            // The notice's own redraw is unconditional on the five-second gate in the sense that
            // it sets it forward; see the client's tail.
            self.next_update = now + REDRAW_INTERVAL;
        }
        if shown_now {
            // Ask for a ping, then update.
            self.please_request_ping = true;
        } else if !answered && now < self.next_update {
            // The five-second throttle. The panel is up and nothing has changed.
            return false;
        }
        if !answered {
            self.next_update = now + REDRAW_INTERVAL;
        }
        self.write(ui, view);
        // The update's tail: the request gate.
        if self.please_request_ping || now - self.last_ping_request >= PING_INTERVAL {
            self.last_ping_request = now;
            self.please_request_ping = false;
            // -> `0x01E9`. **Nothing is sent here**:
            // the request is emitted onto the same seam every other panel's actions use, and the
            // host is what puts it on the wire.
            ui.requests.emit(UiRequest::RequestPing);
            self.pings_requested += 1;
        }
        true
    }

    /// The update's five `StringInfo`s.
    pub fn write(&mut self, ui: &mut UiSystem, view: &dyn GameView) {
        let mut s = super::statmgmt::label(ui, string::INFO);
        s.push_str(&super::statmgmt::label(ui, string::COLORS));
        s.push_str(&super::statmgmt::label(ui, string::DISCONNECT));
        // The packet loss as a two-decimal float variable,
        // **unconditional** — there is no unknown arm on this line.
        let loss = format!("{:.*}", PACKET_LOSS_PRECISION, view.packet_loss_percent());
        s.push_str(&super::characterinfo::compose(
            ui,
            string::PACKET_LOSS,
            &[(var::PACKET_LOSS, loss.as_str())],
        ));
        // Round trip `<= 0.0` -> the string arm; otherwise `* 1000.0` milliseconds.
        let ping = if self.ping_round_trip <= 0.0 {
            UNKNOWN.to_owned()
        } else {
            format!("{:.0}", self.ping_round_trip * 1000.0)
        };
        s.push_str(&super::characterinfo::compose(
            ui,
            string::PING,
            &[(var::PING, ping.as_str())],
        ));
        self.text.clone_from(&s);
        if let Some(t) = self.main_text.and_then(|h| ui.text_element_mut(h)) {
            t.set_text(&s);
        }
        self.updates += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the stored placeholder string and timeout constant described in the module header.
    ///
    /// The marker is **four** question marks. Three would be `super::inforegion::UNKNOWN`, which
    /// is a different string in a different function, and mixing them up is the kind of thing a
    /// paraphrase does.
    #[test]
    fn the_unknown_ping_marker_is_the_four_byte_literal_in_the_image() {
        assert_eq!(UNKNOWN, "????");
        assert_eq!(UNKNOWN.len(), 4);
        assert_ne!(
            UNKNOWN,
            super::super::inforegion::UNKNOWN,
            "not the InfoRegion's three"
        );
        assert!((PING_INTERVAL - 120.0).abs() < f64::EPSILON);
    }

    /// Oracle: `crate::panels::catalogue`'s `LinkStatusPanel` row.
    #[test]
    fn the_one_bound_child_is_the_catalogued_one() {
        let spec = crate::panels::catalogue::spec("LinkStatusPanel").expect("catalogued");
        assert_eq!(spec.children.len(), 1);
        assert_eq!(spec.children[0].id, MAIN_TEXT);
    }
}
