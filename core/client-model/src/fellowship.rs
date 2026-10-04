//! Fellowships: the member table, the XP split and the even-split recomputation.
//!
//! The client recomputes the even-split flag so the panel can grey the state; **the server does the
//! real division**.
//!
//! The pure rules of this module live in [`dereth_rules::fellowship`]; they are re-exported
//! here, so every `dereth_client_model::fellowship::*` path resolves.

use dereth_primitives::ObjectId;
use std::collections::BTreeMap;

pub use dereth_rules::fellowship::*;

/// How a `0x02C0 Fellowship_UpdateFellow` describes what changed — `FellowUpdateType`, passed
/// through the fellow-updated notice to the fellowship panel untouched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FellowUpdate {
    /// 1 — the whole `Fellow`.
    Full,
    /// 2 — level and the caches.
    Stats,
    /// 3 — the six health/stamina/mana words.
    Vitals,
    /// Anything else the shard sends. Retail passes the raw dword on, so this does too.
    Other(u32),
}

impl FellowUpdate {
    #[must_use]
    pub fn from_raw(v: u32) -> Self {
        match v {
            1 => Self::Full,
            2 => Self::Stats,
            3 => Self::Vitals,
            other => Self::Other(other),
        }
    }
}

/// **The client fellowship system's five inbound handlers.**
///
/// # Why these five, and why together
///
/// The fellowship family is **77 of 94** messages in three recorded sessions that need a
/// receiver. [`Fellowship`] models the wire object (`0x44`) with the remove-fellow
/// locked-departure rule and the even-split recomputation, `dereth_protocol::social`
/// decodes all five messages, and the selection logic reads `self.fellowship`. These handlers
/// are that field's **only writers**: a message with no receiver and a field with no writer
/// are the same defect seen from opposite sides.
///
/// # What moves when these run
///
/// Two things, both wired to something a player can see:
///
/// * **Tab-targeting stops picking your fellows.** The `Monster` selection arm calls `is_fellow`
///   and skips a member of your own fellowship. With an empty table every fellow would be a
///   valid monster target.
/// * **The Fellowship chat tab enables and disables.** Every one of these handlers enables or
///   disables fellowship talk focus for channel 3, and `ChatState`'s notice queue is drained into
///   the live drop-down by `App`.
///
/// # The one retail step that has no equivalent here, deliberately
///
/// All five ask the radar to refresh each affected member's blip. In this build the radar reads
/// the fellowship out of `World` on the frame it draws rather than being pushed at; there is no
/// `Notice` variant for it,
/// and adding one would give the colour two owners. The *state* the notice announces is written
/// here; only the push is absent.
impl crate::World {
    /// The panel's guarded recruit action. Availability (fullness and who may invite) belongs
    /// to its controls; this sender independently validates the selected player and membership.
    pub fn recruit_fellow(
        &mut self,
        req: &mut dyn crate::RequestSink,
        out: &mut dyn crate::NoticeSink,
        target: ObjectId,
    ) -> bool {
        let Some(fellowship) = &self.fellowship else {
            return false;
        };
        if !self.weenie(target).is_some_and(|w| w.is_player()) {
            return false;
        }
        if fellowship.members.contains_key(&target) {
            let text = if self.player == Some(target) {
                "You can't recruit yourself"
            } else {
                "That person is already in your fellowship"
            };
            out.emit(crate::Notice::DisplayString {
                feedback: dereth_client_contract::feedback::Feedback::WARNING,
                channel: 0x1a,
                text: text.into(),
            });
            return false;
        }
        self.fellowship_recruit(req, target);
        true
    }

    pub fn dismiss_fellow(
        &mut self,
        req: &mut dyn crate::RequestSink,
        out: &mut dyn crate::NoticeSink,
        target: ObjectId,
    ) -> bool {
        let Some(fellowship) = &self.fellowship else {
            return false;
        };
        let refusal = if !fellowship.members.contains_key(&target) {
            Some("That person is not in your fellowship")
        } else if self.player == Some(target) {
            Some("You can't dismiss yourself")
        } else {
            None
        };
        if let Some(text) = refusal {
            out.emit(crate::Notice::DisplayString {
                feedback: dereth_client_contract::feedback::Feedback::WARNING,
                channel: 0x1a,
                text: text.into(),
            });
            return false;
        }
        self.fellowship_dismiss(req, target);
        true
    }

    pub fn assign_fellow_leader(
        &mut self,
        req: &mut dyn crate::RequestSink,
        out: &mut dyn crate::NoticeSink,
        target: ObjectId,
    ) -> bool {
        let Some(fellowship) = &self.fellowship else {
            return false;
        };
        let refusal = if !fellowship.members.contains_key(&target) {
            Some("That person is not in the fellowship.")
        } else if self.player == Some(target) {
            Some("You are already the leader")
        } else {
            None
        };
        if let Some(text) = refusal {
            out.emit(crate::Notice::DisplayString {
                feedback: dereth_client_contract::feedback::Feedback::WARNING,
                channel: 0x1a,
                text: text.into(),
            });
            return false;
        }
        self.fellowship_assign_new_leader(req, target);
        true
    }

    /// Transfer to the first nonleader before leaving; do not wait for acknowledgement.
    pub fn leave_fellowship(
        &mut self,
        req: &mut dyn crate::RequestSink,
        out: &mut dyn crate::NoticeSink,
        disband: bool,
    ) {
        if !disband {
            let next = self
                .fellowship
                .as_ref()
                .filter(|f| self.player == Some(f.leader))
                .and_then(|f| f.members.keys().copied().find(|id| *id != f.leader));
            if let Some(next) = next {
                self.assign_fellow_leader(req, out, next);
            }
        }
        self.fellowship_quit(req, disband);
    }

    /// The fellowship full-update handler — `0x02BE`.
    ///
    /// A **replace**, not a merge: the client overwrites the whole fellowship object,
    /// members, departures and all. Returns `(created, leader_changed)` — the two branches retail
    /// takes after the assignment, which decide whose blip is repainted. They are returned rather
    /// than acted on because this crate cannot reach the radar.
    pub fn recv_fellowship_full_update(
        &mut self,
        f: &dereth_protocol::social::Fellowship,
    ) -> (bool, bool) {
        let created = self.fellowship.is_none();
        let old_leader = self.fellowship.as_ref().map(|f| f.leader);
        let new_leader = f.leader;
        let mut next = Fellowship {
            members: BTreeMap::new(),
            name: f.name.clone(),
            leader: f.leader,
            share_xp: f.share_xp != 0,
            even_xp_split: f.even_xp_split != 0,
            open_fellow: f.open_fellow != 0,
            locked: f.locked != 0,
            fellows_departed: BTreeMap::new(),
        };
        // `or_insert`, not `insert`. **Measured, and easy to get backwards:**
        // The packed hash-table insertion returns 0 *without overwriting* on a duplicate key,
        // and the table's throw-away-duplicate-keys flag only stops that zero failing the unpack. A
        // duplicate key therefore keeps the **first** value, and
        // `entries.into_iter().collect::<BTreeMap<_, _>>()` -- the obvious spelling -- silently
        // does the opposite.
        for (id, fellow) in &f.members.entries {
            next.members
                .entry(ObjectId(*id))
                .or_insert_with(|| from_wire(fellow));
        }
        for (id, at) in &f.fellows_departed.entries {
            next.fellows_departed
                .entry(ObjectId(*id))
                .or_insert(i64::from(*at));
        }
        self.fellowship = Some(next);
        // Enabling talk focus 3 is the last line of the handler and is unconditional.
        self.chat
            .set_talk_focus_enabled(crate::chat::TalkFocus::Fellowship, true);
        (created, !created && old_leader != Some(new_leader))
    }

    /// The fellowship update-fellow handler — `0x02C0`.
    ///
    /// Returns `true` when the member was **not** already in the table, which is retail's
    /// membership test taken *before* the update and the condition on the fellow-added notice.
    ///
    /// Retail dereferences the fellowship without a null check here, on the assumption that a
    /// `0x02C0` only ever follows a `0x02BE`. This build cannot turn that assumption into a crash,
    /// so a `0x02C0` arriving with no fellowship is refused and returns `false`; the caller counts
    /// it, because a non-zero count there means the two are arriving out of order.
    pub fn recv_fellowship_update_fellow(
        &mut self,
        id: ObjectId,
        fellow: &dereth_protocol::social::Fellow,
        _update: FellowUpdate,
    ) -> bool {
        let Some(f) = self.fellowship.as_mut() else {
            return false;
        };
        let added = !f.members.contains_key(&id);
        f.members.insert(id, from_wire(fellow));
        self.chat
            .set_talk_focus_enabled(crate::chat::TalkFocus::Fellowship, true);
        added
    }

    /// The fellowship disband handler — `0x02BF`.
    ///
    /// The fellowship is deleted, then the focus goes off. The whole object goes, so the departure table goes
    /// with it: there is no fellowship left for a locked re-admission to consult.
    pub fn recv_fellowship_disband(&mut self) {
        self.fellowship = None;
        self.chat
            .set_talk_focus_enabled(crate::chat::TalkFocus::Fellowship, false);
    }

    /// The fellowship quit handler (`0x00A3`) and
    /// the dismiss handler (`0x00A4`).
    ///
    /// The two handlers are the **same body** but for which notice they raise at the end —
    /// the fellowship-quit notice against the fellowship-dismissed notice — so they share one here and
    /// the caller raises the difference.
    ///
    /// The branch is on identity, and it is the whole handler: *the player* leaving deletes the
    /// fellowship, and *anyone else* leaving only removes a row. The talk focus follows the same
    /// test, which is why it is not simply `false` — somebody else being dismissed from a
    /// fellowship you are still in must leave your Fellowship tab enabled.
    ///
    /// `real_time` is the current time at the instant of the removal: the handler
    /// records it in the departed-fellows table when the fellowship is **locked**, so a locked
    /// fellowship can re-admit exactly the people who were in it. Returns whether the departure
    /// was the player's own, which is the branch retail takes and the one the caller needs.
    pub fn recv_fellowship_member_left(&mut self, member: ObjectId, real_time: i64) -> bool {
        let is_the_player = self.is_the_player(member);
        if is_the_player {
            self.fellowship = None;
            self.chat
                .set_talk_focus_enabled(crate::chat::TalkFocus::Fellowship, false);
        } else if let Some(f) = self.fellowship.as_mut() {
            f.remove_fellow(member, real_time);
        }
        is_the_player
    }

    /// Behavior: false when there is no fellowship at all.
    ///
    /// This is the radar's membership question, called once per blip; it is what colours a
    /// fellow in the radar's fellowship colour.
    #[must_use]
    pub fn is_fellow(&self, id: ObjectId) -> bool {
        self.fellowship.as_ref().is_some_and(|f| f.is_fellow(id))
    }

    /// Behavior: `leader == id`, and **not**
    /// gated on membership: the comparison is the whole function.
    #[must_use]
    pub fn is_fellowship_leader(&self, id: ObjectId) -> bool {
        self.fellowship.as_ref().is_some_and(|f| f.leader == id)
    }

    /// The fellowship update-request toggle — `0x00A6`, the live-vitals subscribe the panel
    /// sends, and its **only** statement.
    ///
    /// The shard sends `0x02C0 Fellowship_UpdateFellow` only while a client has asked for it.
    /// Runtime traffic contains an enable request, then sixteen updates, then a disable request.
    pub fn fellowship_update_request(&mut self, req: &mut dyn crate::RequestSink, on: bool) {
        req.send(crate::Request::FellowshipUpdateRequest(
            dereth_protocol::social::FellowshipUpdateRequest { on: i32::from(on) },
        ));
    }

    /// The create-fellowship send — `0x00A2`, from the fellowship panel's
    /// Create button.
    ///
    /// The panel formats the entry box's text before sending it, then writes the formatted name
    /// back into the box. `share_xp` is the check box on this same tab, not a field of the
    /// fellowship being made.
    ///
    /// Runtime traffic confirms the wire order: a padded `PString` name followed by `share_xp`.
    pub fn create_fellowship(
        &mut self,
        req: &mut dyn crate::RequestSink,
        name: &str,
        share_xp: bool,
    ) {
        req.send(crate::Request::FellowshipCreate(
            dereth_protocol::social::FellowshipCreate {
                name: name.to_owned(),
                share_xp: i32::from(share_xp),
            },
        ));
    }

    /// The quit send `(disband)` — `0x00A3`, and **both** of the panel's leave
    /// buttons: `0x1000027C` sends `0`, `0x10000280` (Disband) sends `1`.
    ///
    /// Capture oracle: `a3000000 00000000` three times and `a3000000 01000000` once across the
    /// recorded fellowship sessions.
    pub fn fellowship_quit(&mut self, req: &mut dyn crate::RequestSink, disband: bool) {
        req.send(crate::Request::FellowshipQuit(
            dereth_protocol::social::FellowshipQuitRequest {
                disband: i32::from(disband),
            },
        ));
    }

    /// The dismiss send `(id)` — `0x00A4`, from the panel's Dismiss button,
    /// which refuses a non-member and refuses the
    /// player themselves before it sends.
    ///
    /// Capture oracle: a recorded fellowship session carries `a4000000 1f000050`.
    pub fn fellowship_dismiss(&mut self, req: &mut dyn crate::RequestSink, target: ObjectId) {
        req.send(crate::Request::FellowshipDismiss(
            dereth_protocol::social::FellowshipDismiss { target },
        ));
    }

    /// The recruit send `(id)` — `0x00A5`, from the panel's Recruit button
    /// with the currently selected object.
    ///
    /// **It carries a real object id.** Nine `0x00A5` across the three captures and every one is
    /// a `0x5000001E`/`0x5000001F`/`0x50000020` — the *selected* player's instance id. Nothing
    /// on this path identifies anybody by name.
    pub fn fellowship_recruit(&mut self, req: &mut dyn crate::RequestSink, target: ObjectId) {
        req.send(crate::Request::FellowshipRecruit(
            dereth_protocol::social::FellowshipRecruit { target },
        ));
    }

    /// The assign-new-leader send `(id)` — `0x0290`, from the panel's
    /// Leadership button, which refuses a non-member and
    /// refuses the player themselves.
    ///
    /// Capture oracle: a recorded fellowship session carries `90020000 1e000050` and
    /// `90020000 20000050`.
    pub fn fellowship_assign_new_leader(
        &mut self,
        req: &mut dyn crate::RequestSink,
        target: ObjectId,
    ) {
        req.send(crate::Request::FellowshipAssignNewLeader(
            dereth_protocol::social::FellowshipAssignNewLeader { target },
        ));
    }

    /// The change-fellow-openness send — `0x0291`.
    ///
    /// **The client flips its own copy first and sends the result**, which is the one place in
    /// this family where the panel does not wait for the shard:
    /// the panel's element-message listener toggles the open flag, sends its new value in the
    /// fellowship-open-state event, and then updates the buttons. So the button's caption changes
    /// on the click and a refusal is undone by
    /// the next `0x02BE`. Returns the value sent, or `None` when there is no fellowship — the
    /// fellowship check the arm opens with.
    ///
    /// Runtime traffic confirms that the request carries the toggled open-state value.
    pub fn fellowship_toggle_openness(&mut self, req: &mut dyn crate::RequestSink) -> Option<bool> {
        let f = self.fellowship.as_mut()?;
        f.open_fellow = !f.open_fellow;
        let open = f.open_fellow;
        req.send(crate::Request::FellowshipChangeFellowOpenness(
            dereth_protocol::social::FellowshipChangeFellowOpenness {
                open: i32::from(open),
            },
        ));
        Some(open)
    }
}

/// The wire `Fellow` as the client's `Fellow` — field for field, no conversion.
fn from_wire(f: &dereth_protocol::social::Fellow) -> Fellow {
    Fellow {
        name: f.name.clone(),
        level: f.level,
        cp_cache: f.cp_cache,
        lum_cache: f.lum_cache,
        share_loot: f.share_loot,
        max_health: f.max_health,
        max_stamina: f.max_stamina,
        max_mana: f.max_mana,
        current_health: f.current_health,
        current_stamina: f.current_stamina,
        current_mana: f.current_mana,
    }
}
