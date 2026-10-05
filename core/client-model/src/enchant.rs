//! The enchantment registry — duelling, culling, the `f32` application pipeline and the
//! **receipt-time rebasing**.
//!
//! Three things here are load-bearing and easy to "improve" by accident:
//!
//! * **Additive is tested before multiplicative** in the value-application step. An
//!   enchantment with both bits set is treated as additive.
//! * Every value goes through the pipeline as a **32-bit `float`**, even for `double` float
//!   properties and `long` int properties. That is where "my buffed skill is one off" comes from.
//!   `f64` here changes displayed numbers.
//! * The start time and last-degraded time arrive **relative to receipt**, and the client stores
//!   `current_time + value`. The countdown is anchored to the moment the packet was *processed*,
//!   so one-way latency is silently subtracted from every enchantment for its whole life. Do not
//!   "fix" it with a server timestamp.
//!
//! Shared arithmetic and inquiries live in `dereth-rules`; this module keeps the
//! client model's state adapters and value tests.

use dereth_primitives::{LocalTime, ObjectId};

use dereth_rules::enchant::*;

// ---------------------------------------------------------------------------------------------
// The player's registry, reached the way the client reaches it.
// ---------------------------------------------------------------------------------------------

impl crate::world::World {
    /// The `beneficial` closure every counted operation needs, with the map lifted out of
    /// `self.magic` so the registry (which lives under `self.tables`) can be borrowed mutably at
    /// the same time. The map is always put back.
    fn with_player_registry<T>(
        &mut self,
        f: impl FnOnce(&mut EnchantmentRegistry, &dyn Fn(u16) -> Option<bool>) -> T,
        empty: T,
    ) -> T {
        let table = std::mem::take(&mut self.magic.spell_beneficial);
        let beneficial = |id: u16| table.get(&u32::from(id)).copied();
        let out = match self.player_qualities_mut() {
            Some(q) => f(&mut q.enchantments, &beneficial),
            None => empty,
        };
        self.magic.spell_beneficial = table;
        out
    }

    /// Handle the purge-enchantments message,
    /// **`0x02C6 Magic_PurgeEnchantments`**, which is a body-less game event.
    ///
    /// It fetches the player description; if there is one it purges that description's
    /// enchantments, then sends the enchantments-changed notice and the vitae-changed notice.
    ///
    /// So it is always the **local player's** registry, never another object's, and it raises two
    /// notices. The handler is a null guard followed by a forward.
    ///
    /// **This is where the client's "purge on death" comes from, and it is not a client hook**: the
    /// client has no death path into the registry at all. `0x02C6` and `0x0312` arriving is the
    /// entire mechanism, and the server chooses when: death and dispel are the same message arm.
    pub fn purge_enchantments(&mut self, out: &mut dyn crate::NoticeSink) -> bool {
        let changed = self.with_player_registry(|r, b| r.purge_enchantments(b), false);
        self.notify_enchantments_changed(out);
        changed
    }

    /// Handle purge-bad-enchantments message **`0x0312`**, the same three lines using the bad-
    /// enchantment purge.
    pub fn purge_bad_enchantments(&mut self, out: &mut dyn crate::NoticeSink) -> bool {
        let changed = self.with_player_registry(|r, b| r.purge_bad_enchantments(b), false);
        self.notify_enchantments_changed(out);
        changed
    }

    /// Both purge handlers raise the enchantments-changed and vitae-changed notices
    /// unconditionally. The vitae one is raised even though neither purge can
    /// change vitae — that is the client's, not an oversight here.
    fn notify_enchantments_changed(&mut self, out: &mut dyn crate::NoticeSink) {
        if let Some(p) = self.player {
            out.emit(crate::Notice::ItemAttributesChanged { object: p, kind: 0 });
        }
    }

    /// Update one enchantment on the player, **with** the spell totals — the `0x02C2
    /// Magic_UpdateEnchantment` path.
    pub fn update_player_enchantment(
        &mut self,
        e: &dereth_protocol::types::qualities::Enchantment,
        now: LocalTime,
    ) -> bool {
        let en = Enchantment::from_wire(e, now);
        self.with_player_registry(|r, b| r.update_enchantment_counted(en, b), false)
    }

    /// Remove one player enchantment and update the totals — `0x02C3` and `0x02C7`.
    pub fn remove_player_enchantment(&mut self, layered_spell_id: u32) -> bool {
        self.with_player_registry(
            |r, b| r.remove_enchantment_counted(layered_spell_id, b),
            false,
        )
    }

    /// The client's tail — zero the counters and re-count both spell
    /// lists. Run after a `0x0013` has replaced the whole registry, and after the spell table
    /// arrives (whichever is later), because the count needs both.
    ///
    /// Returns `(helpful, harmful)` so a caller can assert a denominator rather than a bare zero.
    pub fn recount_spell_totals(&mut self) -> (u32, u32) {
        self.with_player_registry(
            |r, b| {
                r.count_spells_in_lists(b);
                (r.helpful_count, r.harmful_count)
            },
            (0, 0),
        )
    }

    // -----------------------------------------------------------------------------------------
    // The **count-prefixed** trio, `0x02C4`, `0x02C5`, `0x02C8`.
    // -----------------------------------------------------------------------------------------

    /// Handle the update-multiple-enchantments message,
    /// **`0x02C4 Magic_UpdateMultipleEnchantments`**.
    ///
    /// ```text
    ///   obtain the current player's description
    ///   query its character-data interface
    ///   missing player description -> do nothing at all
    ///           update the qualities' enchantments
    ///           send enchantments-changed notice
    /// ```
    ///
    /// The update path creates the registry if the character has none and forwards to the list
    /// updater, which is a
    /// **plain loop over the list** updating each entry and OR-ing the answers:
    ///
    /// ```text
    ///   for each enchantment in the list:
    ///       any |= update one registry enchantment
    /// ```
    ///
    /// So `0x02C4` is exactly *n* × `0x02C2` and nothing else. The **count prefix** is
    /// the packed list's, which [`dereth_protocol::qualities::MagicUpdateMultipleEnchantments`] already
    /// reads — the message carries no per-entry header of its own.
    ///
    /// Two differences from the single arm, both deliberate:
    ///
    /// * the enchantments-changed notice is raised **once, after the whole list**, and
    ///   unconditionally — not per entry and not gated on the OR;
    /// * it is **always** `EnchantmentsChanged`, never `VitaeChanged`, even when a vitae entry is
    ///   in the list. The single-entry handler picks between the
    ///   two on `stat_mod.type & 0x800000`; this one has no such test.
    ///
    /// Returns how many entries the registry accepted — each update's leading parity
    /// check rejects a malformed `stat_mod.type`, so this is *not* `list.len()`.
    pub fn update_player_enchantments(
        &mut self,
        list: &[dereth_protocol::types::qualities::Enchantment],
        now: LocalTime,
        out: &mut dyn crate::NoticeSink,
    ) -> usize {
        let mut applied = 0;
        for e in list {
            if self.update_player_enchantment(e, now) {
                applied += 1;
            }
        }
        self.notify_enchantments_changed(out);
        applied
    }

    /// Handle the remove-multiple-enchantments message,
    /// behind **both** `0x02C5 Magic_RemoveMultipleEnchantments` and
    /// `0x02C8 Magic_DispelMultipleEnchantments`.
    ///
    /// The two opcodes are the same body on the wire and the same handler in the client; the only
    /// difference is the `bool` the dispatcher passes:
    ///
    /// ```text
    /// removal  -> the remove-multiple handler with notify = TRUE
    /// dispel   -> the dispel-multiple handler, which calls the remove-multiple handler
    ///             with notify = FALSE
    /// ```
    ///
    /// This is the same pairing used by the single-entry `0x02C3`/`0x02C7` handlers:
    /// **a dispel is silent; a removal announces.**
    ///
    /// The body:
    ///
    /// ```text
    ///   missing player description -> skip the registry work
    ///       otherwise: if there is a vitae enchantment, save its id;
    ///                  remove the enchantments through the qualities registry;
    ///                  send enchantments-changed notice
    ///   for each id in the list:
    ///       if notify: notify listeners of enchantment removal for `id`
    ///       if id == the saved vitae id: send vitae-changed notice
    /// ```
    ///
    /// Three readings that a plainer transcription loses:
    ///
    /// 1. **The vitae id is latched *before* the removal**, so the vitae-changed notice fires for
    ///    the id that *was* vitae — asking the registry afterwards would always answer "no vitae".
    /// 2. **The announcement loop runs even with no player description.** The null branch skips only the
    ///    registry block; the walk is outside it, so the chat lines still print.
    /// 3. The removal notification runs for **every id on the wire**, present in the
    ///    registry or not — it is a spell-table lookup, not a registry lookup.
    ///
    /// Returns `(removed, announced)`.
    pub fn remove_player_enchantments(
        &mut self,
        ids: &[u32],
        notify: bool,
        out: &mut dyn crate::NoticeSink,
    ) -> (usize, usize) {
        // The saved vitae id starts at zero and is set only if there is a vitae enchantment; a
        // zero id therefore matches nothing, which is the client's own guard against a vitae-less
        // character taking the vitae-changed-notice arm.
        let vitae = self
            .player_qualities()
            .and_then(|q| q.enchantments.vitae)
            .map(|v| v.id);
        let mut removed = 0;
        if self.player_qualities().is_some() {
            for id in ids {
                if self.remove_player_enchantment(*id) {
                    removed += 1;
                }
            }
            self.notify_enchantments_changed(out);
        }
        let mut announced = 0;
        for id in ids {
            if notify && self.notify_of_enchantment_removal(*id) {
                announced += 1;
            }
            if vitae == Some(*id) {
                // The vitae-changed notice — the vitae panel's redraw. The registry's own vitae
                // entry was cleared by the removal above.
                self.notify_enchantments_changed(out);
            }
        }
        (removed, announced)
    }

    /// Behavior: the *"&lt;spell&gt; has
    /// expired."* line, on chat type **7** (`MAGIC`).
    ///
    /// ```text
    ///   channel 7 squelched         -> return true, no line
    ///   spell_id = layered_spell_id & 0xFFFF
    ///   spell_id >= 0x8000           -> a cooldown, no line
    ///   look up the spell's base record
    ///   unknown spell               -> return FALSE, no line
    ///   text = the spell name
    ///   if spell_id == 0x29A: text += " penalty"      ; 0x29A is vitae
    ///   text += " has expired.\n"
    ///   add the text to scroll channel 7
    /// ```
    ///
    /// **The `0x8000` gate is the cooldown gate.** The client lays the cooldown id plus `0x8000` in
    /// the spell id, so an expiring cooldown never announces itself. Spell totals use the same
    /// `0x7FFF` boundary.
    ///
    /// **The squelch-check gate.** It is the
    /// function's **first** act, above the `0x8000` cooldown test and above the spell-table
    /// lookup, and it passes id `0` with the empty account name and the **constant** type `7`, so only
    /// the *global* `Magic` entry can refuse an expiry line. Neither the spell's school nor any
    /// per-character or account entry reaches it.
    ///
    /// Returns whether a line was written, which is `false` for every one of the four refusals
    /// — including the two branches where the native routine returns `true`.
    #[must_use]
    pub fn notify_of_enchantment_removal(&mut self, layered_spell_id: u32) -> bool {
        if self
            .chat
            .is_squelched(ObjectId(0), "", crate::chat::text_type::MAGIC)
        {
            return false;
        }
        let spell_id = layered_spell_id & 0xFFFF;
        if spell_id >= 0x8000 {
            return false;
        }
        let Some(table) = self.magic.spell_table.clone() else {
            return false;
        };
        let Some(base) = table.spells.get(&spell_id) else {
            return false;
        };
        let mut text = base.name.clone();
        // `0x29A` — the vitae spell. The client uses the same
        // literal to route a query at the vitae slot rather than at the two lists.
        if spell_id == 0x29A {
            text.push_str(" penalty");
        }
        text.push_str(" has expired.\n");
        self.scroll
            .add_text_to_scroll(&text, crate::chat::text_type::MAGIC, true, 0);
        true
    }
}
